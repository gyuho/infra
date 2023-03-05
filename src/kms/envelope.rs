use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
};

use crate::{
    errors::{Error::Other, Result},
    kms,
};
use aws_sdk_kms::model::{DataKeySpec, EncryptionAlgorithmSpec};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use compress_manager::{self, Decoder, Encoder};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN},
    rand::{SecureRandom, SystemRandom},
};

const DEK_AES_256_LENGTH: usize = 32;

/// Implements envelope encryption manager.
#[derive(Clone)]
pub struct Manager<'k> {
    pub kms_manager: &'k kms::Manager,
    pub kms_key_id: String,

    /// Represents additional authenticated data (AAD) that attaches information
    /// to the ciphertext that is not encrypted.
    aad_tag: String,
}

impl<'k> Manager<'k> {
    /// Creates a new envelope encryption manager.
    pub fn new(kms_manager: &'k kms::Manager, kms_key_id: String, aad_tag: String) -> Self {
        Self {
            kms_manager,
            kms_key_id,
            aad_tag,
        }
    }

    /// Envelope-encrypts the data using AWS KMS data-encryption key (DEK) and "AES_256_GCM"
    /// because kms:Encrypt can only encrypt 4 KiB.
    ///
    /// The encrypted data are aligned as below:
    /// [ Nonce bytes "length" ][ DEK.ciphertext "length" ][ Nonce bytes ][ DEK.ciphertext ][ data ciphertext ]
    ///
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Decrypt.html
    pub async fn seal_aes_256(&self, d: &[u8]) -> Result<Vec<u8>> {
        log::info!(
            "AES_256 envelope-encrypting data (size before encryption {})",
            human_readable::bytes(d.len() as f64)
        );

        let dek = self
            .kms_manager
            .generate_data_key(&self.kms_key_id, Some(DataKeySpec::Aes256))
            .await?;
        if dek.plaintext.len() != DEK_AES_256_LENGTH {
            return Err(Other {
                message: format!(
                    "DEK.plaintext for AES_256 must be {}-byte, got {}-byte",
                    DEK_AES_256_LENGTH,
                    dek.plaintext.len()
                ),
                is_retryable: false,
            });
        }

        let random = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        match random.fill(&mut nonce_bytes) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to generate ring.random for nonce ({:?})", e),
                    is_retryable: false,
                });
            }
        }
        let unbound_key = match UnboundKey::new(&AES_256_GCM, &dek.plaintext) {
            Ok(v) => v,
            Err(e) => {
                return Err(Other {
                    message: format!("failed to create UnboundKey ({:?})", e),
                    is_retryable: false,
                });
            }
        };
        let safe_key = LessSafeKey::new(unbound_key);

        // overwrites the original array
        let mut cipher = d.to_vec();
        match safe_key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(self.aad_tag.clone()),
            &mut cipher,
        ) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to seal ({:?})", e),
                    is_retryable: false,
                });
            }
        }

        // align bytes in the order of
        // - Nonce bytes "length"
        // - DEK.ciphertext "length"
        // - Nonce bytes
        // - DEK.ciphertext
        // - data ciphertext
        let mut encrypted = Vec::new();

        // Nonce bytes "length"
        match encrypted.write_u16::<LittleEndian>(NONCE_LEN as u16) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to write ({:?})", e),
                    is_retryable: false,
                });
            }
        }

        // DEK.ciphertext "length"
        match encrypted.write_u16::<LittleEndian>(dek.ciphertext.len() as u16) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to write ({:?})", e),
                    is_retryable: false,
                });
            }
        }

        // Nonce bytes
        match encrypted.write_all(&nonce_bytes) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to write ({:?})", e),
                    is_retryable: false,
                });
            }
        }

        // DEK.ciphertext
        match encrypted.write_all(&dek.ciphertext) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to write ({:?})", e),
                    is_retryable: false,
                });
            }
        }

        // data ciphertext
        match encrypted.write_all(&cipher) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to write ({:?})", e),
                    is_retryable: false,
                });
            }
        }

        log::info!(
            "AES_256 envelope-encrypted data (encrypted size {})",
            human_readable::bytes(encrypted.len() as f64)
        );
        Ok(encrypted)
    }

    /// Envelope-decrypts using KMS DEK and "AES_256_GCM".
    ///
    /// Assume the input (ciphertext) data are packed in the order of:
    /// [ Nonce bytes "length" ][ DEK.ciphertext "length" ][ Nonce bytes ][ DEK.ciphertext ][ data ciphertext ]
    ///
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Encrypt.html
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_GenerateDataKey.html
    pub async fn unseal_aes_256(&self, d: &[u8]) -> Result<Vec<u8>> {
        log::info!(
            "AES_256 envelope-decrypting data (size before decryption {})",
            human_readable::bytes(d.len() as f64)
        );

        // bytes are packed in the order of
        // - Nonce bytes "length"
        // - DEK.ciphertext "length"
        // - Nonce bytes
        // - DEK.ciphertext
        // - data ciphertext
        let mut buf = Cursor::new(d);

        let nonce_len = match buf.read_u16::<LittleEndian>() {
            Ok(v) => v as usize,
            Err(e) => {
                return Err(Other {
                    message: format!("failed to read_u16 for nonce_len ({:?})", e),
                    is_retryable: false,
                });
            }
        };
        if nonce_len != NONCE_LEN {
            return Err(Other {
                message: format!("nonce_len {} != NONCE_LEN {}", nonce_len, NONCE_LEN),
                is_retryable: false,
            });
        }

        let dek_ciphertext_len = match buf.read_u16::<LittleEndian>() {
            Ok(v) => v as usize,
            Err(e) => {
                return Err(Other {
                    message: format!("failed to read_u16 for dek_ciphertext_len ({:?})", e),
                    is_retryable: false,
                });
            }
        };
        if dek_ciphertext_len > d.len() {
            return Err(Other {
                message: format!(
                    "invalid DEK ciphertext len {} > cipher.len {}",
                    dek_ciphertext_len,
                    d.len()
                ),
                is_retryable: false,
            });
        }

        // "NONCE_LEN" is the per-record nonce (iv_length), 12-byte
        // ref. <https://www.rfc-editor.org/rfc/rfc8446#appendix-E.2>
        let mut nonce_bytes = [0u8; NONCE_LEN];
        match buf.read_exact(&mut nonce_bytes) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to read_exact for nonce_bytes ({:?})", e),
                    is_retryable: false,
                });
            }
        };
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut dek_ciphertext = zero_vec(dek_ciphertext_len);
        match buf.read_exact(&mut dek_ciphertext) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to read_exact for DEK.ciphertext ({:?})", e),
                    is_retryable: false,
                });
            }
        };
        // use the default "SYMMETRIC_DEFAULT"
        let dek_plain = self
            .kms_manager
            .decrypt(
                &self.kms_key_id,
                Some(EncryptionAlgorithmSpec::SymmetricDefault),
                dek_ciphertext,
            )
            .await?;
        let unbound_key = match UnboundKey::new(&AES_256_GCM, &dek_plain) {
            Ok(v) => v,
            Err(e) => {
                return Err(Other {
                    message: format!("failed to create UnboundKey ({:?})", e),
                    is_retryable: false,
                });
            }
        };
        let safe_key = LessSafeKey::new(unbound_key);

        let mut cipher = Vec::new();
        match buf.read_to_end(&mut cipher) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to read_to_end for ciphertext ({:?})", e),
                    is_retryable: false,
                });
            }
        };

        let decrypted =
            match safe_key.open_in_place(nonce, Aad::from(self.aad_tag.clone()), &mut cipher) {
                Ok(plaintext) => plaintext.to_vec(),
                Err(e) => {
                    return Err(Other {
                        message: format!("failed to open_in_place ciphertext ({:?})", e),
                        is_retryable: false,
                    });
                }
            };

        log::info!(
            "AES_256 envelope-decrypted data (decrypted size {})",
            human_readable::bytes(decrypted.len() as f64)
        );
        Ok(decrypted)
    }

    /// Envelope-encrypts data from a file and save the ciphertext to the other file.
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn seal_aes_256_file(&self, src_file: &str, dst_file: &str) -> Result<()> {
        log::info!("envelope-encrypting file {} to {}", src_file, dst_file);
        let d = match fs::read(src_file.to_string()) {
            Ok(d) => d,
            Err(e) => {
                return Err(Other {
                    message: format!("failed read {:?}", e),
                    is_retryable: false,
                });
            }
        };

        let ciphertext = match self.seal_aes_256(&d).await {
            Ok(d) => d,
            Err(e) => {
                return Err(e);
            }
        };

        let mut f = match File::create(dst_file.to_string()) {
            Ok(f) => f,
            Err(e) => {
                return Err(Other {
                    message: format!("failed File::create {:?}", e),
                    is_retryable: false,
                });
            }
        };
        match f.write_all(&ciphertext) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed File::write_all {:?}", e),
                    is_retryable: false,
                });
            }
        };

        Ok(())
    }

    /// Envelope-decrypts data from a file and save the plaintext to the other file.
    pub async fn unseal_aes_256_file(&self, src_file: &str, dst_file: &str) -> Result<()> {
        log::info!("envelope-decrypting file {} to {}", src_file, dst_file);
        let d = match fs::read(src_file.to_string()) {
            Ok(d) => d,
            Err(e) => {
                return Err(Other {
                    message: format!("failed read {:?}", e),
                    is_retryable: false,
                });
            }
        };

        let plaintext = match self.unseal_aes_256(&d).await {
            Ok(d) => d,
            Err(e) => {
                return Err(e);
            }
        };

        let mut f = match File::create(dst_file.to_string()) {
            Ok(f) => f,
            Err(e) => {
                return Err(Other {
                    message: format!("failed File::create {:?}", e),
                    is_retryable: false,
                });
            }
        };
        match f.write_all(&plaintext) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed File::write_all {:?}", e),
                    is_retryable: false,
                });
            }
        };

        Ok(())
    }

    /// Compresses the source file ("src_file") and envelope-encrypts to "dst_file".
    /// The compression uses "zstd".
    /// The encryption uses AES 256.
    pub async fn compress_seal(&self, src_file: &str, dst_file: &str) -> Result<()> {
        log::info!("compress-seal: compressing the file '{}'", src_file);
        let compressed_path = random_manager::tmp_path(10, None).unwrap();
        compress_manager::pack_file(&src_file.to_string(), &compressed_path, Encoder::Zstd(3))
            .map_err(|e| Other {
                message: format!("failed compression {}", e),
                is_retryable: false,
            })?;

        log::info!(
            "compress-seal: sealing the compressed file '{}'",
            compressed_path
        );
        self.seal_aes_256_file(&compressed_path, dst_file.clone())
            .await
    }

    /// Reverse of "compress_seal".
    /// The decompression uses "zstd".
    /// The decryption uses AES 256.
    pub async fn unseal_decompress(&self, src_file: &str, dst_file: &str) -> Result<()> {
        log::info!(
            "unseal-decompress: unsealing the encrypted file '{}'",
            src_file
        );
        let unsealed_path = random_manager::tmp_path(10, None).unwrap();
        self.unseal_aes_256_file(src_file, &unsealed_path).await?;

        log::info!("unseal-decompress: decompressing the file '{}'", src_file);
        compress_manager::unpack_file(&unsealed_path, dst_file, Decoder::Zstd).map_err(|e| Other {
            message: format!("failed decompression {}", e),
            is_retryable: false,
        })
    }

    /// Compresses the file, encrypts, and uploads to S3.
    #[cfg(feature = "s3")]
    pub async fn compress_seal_put_object(
        &self,
        s3_manager: &crate::s3::Manager,
        source_file_path: &str,
        s3_bucket: &str,
        s3_key: &str,
    ) -> Result<()> {
        log::info!(
            "compress-seal-put-object: compress and seal '{}'",
            source_file_path
        );

        let tmp_compressed_sealed_path = random_manager::tmp_path(10, None).unwrap();
        self.compress_seal(source_file_path, &tmp_compressed_sealed_path)
            .await?;

        log::info!(
            "compress-seal-put-object: upload object '{}'",
            tmp_compressed_sealed_path
        );
        s3_manager
            .put_object(&tmp_compressed_sealed_path, s3_bucket, s3_key)
            .await?;

        fs::remove_file(tmp_compressed_sealed_path).map_err(|e| Other {
            message: format!("failed remove_file tmp_compressed_sealed_path: {}", e),
            is_retryable: false,
        })
    }

    /// Reverse of "compress_seal_put_object".
    #[cfg(feature = "s3")]
    pub async fn get_object_unseal_decompress(
        &self,
        s3_manager: &crate::s3::Manager,
        s3_bucket: &str,
        s3_key: &str,
        download_file_path: &str,
    ) -> Result<()> {
        log::info!(
            "get-object-unseal-decompress: downloading object {}/{}",
            s3_bucket,
            s3_key
        );

        let tmp_downloaded_path = random_manager::tmp_path(10, None).unwrap();
        s3_manager
            .get_object(s3_bucket, s3_key, &tmp_downloaded_path)
            .await?;

        log::info!(
            "get-object-unseal-decompress: unseal and decompress '{}'",
            tmp_downloaded_path
        );
        self.unseal_decompress(&tmp_downloaded_path, download_file_path)
            .await?;

        fs::remove_file(tmp_downloaded_path).map_err(|e| Other {
            message: format!("failed remove_file tmp_downloaded_path: {}", e),
            is_retryable: false,
        })
    }
}

fn zero_vec(n: usize) -> Vec<u8> {
    (0..n).map(|_| 0).collect()
}
