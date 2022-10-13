pub mod envelope;

use std::{
    fs::{self, File},
    io::Write,
};

use crate::errors::{
    Error::{Other, API},
    Result,
};
use aws_sdk_kms::{
    error::{
        CreateKeyError, CreateKeyErrorKind, DecryptError, DecryptErrorKind, EncryptError,
        EncryptErrorKind, GenerateDataKeyError, GenerateDataKeyErrorKind, GetPublicKeyError,
        GetPublicKeyErrorKind, ScheduleKeyDeletionError, ScheduleKeyDeletionErrorKind, SignError,
        SignErrorKind,
    },
    model::{
        DataKeySpec, EncryptionAlgorithmSpec, KeySpec, KeyUsageType, MessageType,
        SigningAlgorithmSpec, Tag,
    },
    types::{Blob, SdkError},
    Client,
};
use aws_types::SdkConfig as AwsSdkConfig;

/// Represents the data encryption key.
#[derive(Debug)]
pub struct DEK {
    pub ciphertext: Vec<u8>,
    pub plaintext: Vec<u8>,
}

impl DEK {
    pub fn new(cipher: Vec<u8>, plain: Vec<u8>) -> Self {
        // ref. https://doc.rust-lang.org/1.0.0/style/ownership/constructors.html
        Self {
            ciphertext: cipher,
            plaintext: plain,
        }
    }
}

/// Implements AWS KMS manager.
#[derive(Debug, Clone)]
pub struct Manager {
    #[allow(dead_code)]
    shared_config: AwsSdkConfig,
    cli: Client,
}

impl Manager {
    pub fn new(shared_config: &AwsSdkConfig) -> Self {
        let cloned = shared_config.clone();
        let cli = Client::new(shared_config);
        Self {
            shared_config: cloned,
            cli,
        }
    }

    pub fn client(&self) -> Client {
        self.cli.clone()
    }

    /// Creates an AWS KMS CMK.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html
    pub async fn create_key(
        &self,
        name: &str,
        key_spec: KeySpec,
        key_usage: KeyUsageType,
    ) -> Result<Key> {
        log::info!(
            "creating KMS CMK {}, key spec {:?}, key usage {:?}",
            name,
            key_spec,
            key_usage
        );
        let resp = self
            .cli
            .create_key()
            .description(name)
            // ref. https://docs.aws.amazon.com/kms/latest/developerguide/asymmetric-key-specs.html#key-spec-ecc
            // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html#API_CreateKey_RequestSyntax
            .key_spec(key_spec)
            // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html#KMS-CreateKey-request-KeyUsage
            .key_usage(key_usage)
            .tags(Tag::builder().tag_key("Name").tag_value(name).build())
            .tags(
                Tag::builder()
                    .tag_key("KIND")
                    .tag_value("aws-manager")
                    .build(),
            )
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed create_key {:?}", e),
                is_retryable: is_error_retryable(&e) || is_error_retryable_create_key(&e),
            })?;

        let meta = match resp.key_metadata() {
            Some(v) => v,
            None => {
                return Err(Other {
                    message: String::from("unexpected empty key metadata"),
                    is_retryable: false,
                });
            }
        };

        let key_id = meta.key_id().unwrap_or("");
        let key_arn = meta.arn().unwrap_or("");
        log::info!(
            "successfully KMS CMK -- key Id '{}' and Arn '{}'",
            key_id,
            key_arn
        );

        Ok(Key::new(key_id, key_arn))
    }

    /// Creates a default symmetric AWS KMS CMK.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html
    pub async fn create_symmetric_default_key(&self, name: &str) -> Result<Key> {
        self.create_key(
            name,
            KeySpec::SymmetricDefault,
            KeyUsageType::EncryptDecrypt,
        )
        .await
    }

    /// Signs the 32-byte SHA256 output message with the ECDSA private key and the recoverable code
    /// using AWS KMS CMK.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html
    pub async fn secp256k1_sign_digest(&self, key_id: &str, digest: &[u8]) -> Result<Vec<u8>> {
        log::info!(
            "secp256k1 signing {}-byte digest message with key Id {}",
            digest.len(),
            key_id
        );

        // DO NOT DO THIS -- fails with "Digest is invalid length for algorithm ECDSA_SHA_256"
        // let msg = aws_smithy_types::base64::encode(digest);

        // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html
        let sign_output = self
            .cli
            .sign()
            .key_id(key_id)
            .message(aws_smithy_types::Blob::new(digest)) // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-Message
            .message_type(MessageType::Digest) // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-MessageType
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed sign {:?}", e),
                is_retryable: is_error_retryable(&e) || is_error_retryable_sign(&e),
            })?;

        if let Some(blob) = sign_output.signature() {
            let sig = blob.as_ref();
            log::info!("signature out {}-byte", sig.len());
            return Ok(Vec::from(sig));
        }

        return Err(API {
            message: String::from("signature blob not found"),
            is_retryable: false,
        });
    }

    /// Schedules to delete a KMS CMK.
    pub async fn schedule_to_delete(&self, key_id: &str) -> Result<()> {
        log::info!("deleting KMS CMK '{}'", key_id);
        let ret = self
            .cli
            .schedule_key_deletion()
            .key_id(key_id)
            .pending_window_in_days(7)
            .send()
            .await;

        let deleted = match ret {
            Ok(_) => true,
            Err(e) => {
                let mut ignore_err: bool = false;
                if is_error_schedule_key_deletion_does_not_exist(&e) {
                    log::warn!("KMS CMK '{}' does not exist", key_id);
                    ignore_err = true
                }
                if is_error_schedule_key_deletion_already_scheduled(&e) {
                    log::warn!("KMS CMK '{}' already scheduled for deletion", key_id);
                    ignore_err = true
                }
                if !ignore_err {
                    return Err(API {
                        message: format!("failed schedule_key_deletion {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
                false
            }
        };
        if deleted {
            log::info!("successfully scheduled to delete KMS CMK '{}'", key_id);
        };

        Ok(())
    }

    /// Encrypts data. The maximum size of the data KMS can encrypt is 4096 bytes for
    /// "SYMMETRIC_DEFAULT" encryption algorithm. To specify a KMS key, use its key ID,
    /// key ARN, alias name, or alias ARN.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Encrypt.html
    pub async fn encrypt(
        &self,
        key_id: &str,
        spec: Option<EncryptionAlgorithmSpec>,
        plaintext: Vec<u8>,
    ) -> Result<Vec<u8>> {
        // default to "SYMMETRIC_DEFAULT"
        let key_spec = spec.unwrap_or(EncryptionAlgorithmSpec::SymmetricDefault);
        log::info!(
            "encrypting data (plaintext size {})",
            human_readable::bytes(plaintext.len() as f64),
        );

        let resp = self
            .cli
            .encrypt()
            .key_id(key_id)
            .plaintext(Blob::new(plaintext))
            .encryption_algorithm(key_spec)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed encrypt {:?}", e),
                is_retryable: is_error_retryable(&e) || is_error_retryable_encrypt(&e),
            })?;

        let ciphertext = match resp.ciphertext_blob() {
            Some(v) => v,
            None => {
                return Err(API {
                    message: String::from("EncryptOutput.ciphertext_blob not found"),
                    is_retryable: false,
                });
            }
        };
        let ciphertext = ciphertext.clone().into_inner();

        log::info!(
            "successfully encrypted data (ciphertext size {})",
            human_readable::bytes(ciphertext.len() as f64),
        );
        Ok(ciphertext)
    }

    /// Decrypts data.
    /// The maximum length of "ciphertext" is 6144 bytes.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Decrypt.html
    pub async fn decrypt(
        &self,
        key_id: &str,
        spec: Option<EncryptionAlgorithmSpec>,
        ciphertext: Vec<u8>,
    ) -> Result<Vec<u8>> {
        // default to "SYMMETRIC_DEFAULT"
        let key_spec = spec.unwrap_or(EncryptionAlgorithmSpec::SymmetricDefault);
        log::info!(
            "decrypting data (ciphertext size {})",
            human_readable::bytes(ciphertext.len() as f64),
        );

        let resp = self
            .cli
            .decrypt()
            .key_id(key_id)
            .ciphertext_blob(Blob::new(ciphertext))
            .encryption_algorithm(key_spec)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed decrypt {:?}", e),
                is_retryable: is_error_retryable(&e) || is_error_retryable_decrypt(&e),
            })?;

        let plaintext = match resp.plaintext() {
            Some(v) => v,
            None => {
                return Err(API {
                    message: String::from("DecryptOutput.plaintext not found"),
                    is_retryable: false,
                });
            }
        };
        let plaintext = plaintext.clone().into_inner();

        log::info!(
            "successfully decrypted data (plaintext size {})",
            human_readable::bytes(plaintext.len() as f64),
        );
        Ok(plaintext)
    }

    /// Encrypts data from a file and save the ciphertext to the other file.
    pub async fn encrypt_file(
        &self,
        key_id: &str,
        spec: Option<EncryptionAlgorithmSpec>,
        src_file: &str,
        dst_file: &str,
    ) -> Result<()> {
        log::info!("encrypting file {} to {}", src_file, dst_file);
        let d = fs::read(src_file).map_err(|e| Other {
            message: format!("failed read {:?}", e),
            is_retryable: false,
        })?;
        let ciphertext = self.encrypt(key_id, spec, d).await?;
        let mut f = File::create(dst_file).map_err(|e| Other {
            message: format!("failed File::create {:?}", e),
            is_retryable: false,
        })?;
        f.write_all(&ciphertext).map_err(|e| Other {
            message: format!("failed File::write_all {:?}", e),
            is_retryable: false,
        })
    }

    /// Decrypts data from a file and save the plaintext to the other file.
    pub async fn decrypt_file(
        &self,
        key_id: &str,
        spec: Option<EncryptionAlgorithmSpec>,
        src_file: &str,
        dst_file: &str,
    ) -> Result<()> {
        log::info!("decrypting file {} to {}", src_file, dst_file);
        let d = fs::read(src_file).map_err(|e| Other {
            message: format!("failed read {:?}", e),
            is_retryable: false,
        })?;
        let plaintext = self.decrypt(key_id, spec, d).await?;
        let mut f = File::create(dst_file).map_err(|e| Other {
            message: format!("failed File::create {:?}", e),
            is_retryable: false,
        })?;
        f.write_all(&plaintext).map_err(|e| Other {
            message: format!("failed File::write_all {:?}", e),
            is_retryable: false,
        })
    }

    /// Generates a data-encryption key.
    /// The default key spec is AES_256 generate a 256-bit symmetric key.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_GenerateDataKey.html
    pub async fn generate_data_key(&self, key_id: &str, spec: Option<DataKeySpec>) -> Result<DEK> {
        // default to "AES_256" for generate 256-bit symmetric key (32-byte)
        let dek_spec = spec.unwrap_or(DataKeySpec::Aes256);
        log::info!(
            "generating KMS data key for '{}' with key spec {:?}",
            key_id,
            dek_spec
        );
        let resp = self
            .cli
            .generate_data_key()
            .key_id(key_id)
            .key_spec(dek_spec)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed generate_data_key {:?}", e),
                is_retryable: is_error_retryable(&e) || is_error_retryable_generate_data_key(&e),
            })?;

        let cipher = resp.ciphertext_blob().unwrap();
        let plain = resp.plaintext().unwrap();
        Ok(DEK::new(
            cipher.clone().into_inner(),
            plain.clone().into_inner(),
        ))
    }
}

/// Represents the KMS CMK.
#[derive(Debug)]
pub struct Key {
    pub id: String,
    pub arn: String,
}

impl Key {
    pub fn new(id: &str, arn: &str) -> Self {
        // ref. https://doc.rust-lang.org/1.0.0/style/ownership/constructors.html
        Self {
            id: String::from(id),
            arn: String::from(arn),
        }
    }
}

#[inline]
pub fn is_error_retryable<E>(e: &SdkError<E>) -> bool {
    match e {
        SdkError::TimeoutError(_) | SdkError::ResponseError { .. } => true,
        SdkError::DispatchFailure(e) => e.is_timeout() || e.is_io(),
        _ => false,
    }
}

#[inline]
pub fn is_error_retryable_create_key(e: &SdkError<CreateKeyError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                CreateKeyErrorKind::DependencyTimeoutException(_)
                    | CreateKeyErrorKind::KmsInternalException(_)
            )
        }
        _ => false,
    }
}

#[inline]
pub fn is_error_retryable_generate_data_key(e: &SdkError<GenerateDataKeyError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                GenerateDataKeyErrorKind::DependencyTimeoutException(_)
                    | GenerateDataKeyErrorKind::KeyUnavailableException(_)
                    | GenerateDataKeyErrorKind::KmsInternalException(_)
            )
        }
        _ => false,
    }
}

#[inline]
pub fn is_error_retryable_encrypt(e: &SdkError<EncryptError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                EncryptErrorKind::DependencyTimeoutException(_)
                    | EncryptErrorKind::KeyUnavailableException(_)
                    | EncryptErrorKind::KmsInternalException(_)
            )
        }
        _ => false,
    }
}

#[inline]
pub fn is_error_retryable_decrypt(e: &SdkError<DecryptError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                DecryptErrorKind::DependencyTimeoutException(_)
                    | DecryptErrorKind::KeyUnavailableException(_)
                    | DecryptErrorKind::KmsInternalException(_)
            )
        }
        _ => false,
    }
}

#[inline]
fn is_error_schedule_key_deletion_does_not_exist(e: &SdkError<ScheduleKeyDeletionError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(err.kind, ScheduleKeyDeletionErrorKind::NotFoundException(_))
        }
        _ => false,
    }
}

#[inline]
fn is_error_schedule_key_deletion_already_scheduled(
    e: &SdkError<ScheduleKeyDeletionError>,
) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            let msg = format!("{:?}", err);
            msg.contains("pending deletion")
        }
        _ => false,
    }
}

/// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_GetPublicKey.html
#[inline]
pub fn is_error_retryable_get_public_key(e: &SdkError<GetPublicKeyError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                GetPublicKeyErrorKind::DependencyTimeoutException(_)
                    | GetPublicKeyErrorKind::KeyUnavailableException(_)
                    | GetPublicKeyErrorKind::KmsInternalException(_)
            )
        }
        _ => false,
    }
}

/// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-SigningAlgorithm
#[inline]
pub fn is_error_retryable_sign(e: &SdkError<SignError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                SignErrorKind::DependencyTimeoutException(_)
                    | SignErrorKind::KeyUnavailableException(_)
                    | SignErrorKind::KmsInternalException(_)
            )
        }
        _ => false,
    }
}
