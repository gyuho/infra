pub mod envelope;

use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
};

use crate::errors::{self, Error, Result};
use aws_config::retry::ProvideErrorKind;
use aws_sdk_kms::{
    operation::{
        create_grant::CreateGrantError,
        create_key::CreateKeyError,
        decrypt::DecryptError,
        describe_key::{DescribeKeyError, DescribeKeyOutput},
        encrypt::EncryptError,
        generate_data_key::GenerateDataKeyError,
        get_public_key::{GetPublicKeyError, GetPublicKeyOutput},
        revoke_grant::RevokeGrantError,
        schedule_key_deletion::ScheduleKeyDeletionError,
        sign::SignError,
    },
    primitives::Blob,
    types::{
        DataKeySpec, EncryptionAlgorithmSpec, GrantOperation, KeySpec, KeyUsageType, MessageType,
        SigningAlgorithmSpec, Tag,
    },
    Client,
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;

/// Represents the data encryption key.
#[derive(Debug)]
pub struct DEK {
    pub ciphertext: Vec<u8>,
    pub plaintext: Vec<u8>,
}

impl DEK {
    pub fn new(cipher: Vec<u8>, plain: Vec<u8>) -> Self {
        // ref. <https://doc.rust-lang.org/1.0.0/style/ownership/constructors.html>
        Self {
            ciphertext: cipher,
            plaintext: plain,
        }
    }
}

/// Implements AWS KMS manager.
#[derive(Debug, Clone)]
pub struct Manager {
    pub region: String,
    pub cli: Client,
}

impl Manager {
    pub fn new(shared_config: &AwsSdkConfig) -> Self {
        Self {
            region: shared_config.region().unwrap().to_string(),
            cli: Client::new(shared_config),
        }
    }

    /// Creates an AWS KMS CMK.
    /// Set the tag "Name" with the name value for more descriptive key creation.
    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html>
    pub async fn create_key(
        &self,
        key_spec: KeySpec,
        key_usage: KeyUsageType,
        tags: Option<HashMap<String, String>>,
        multi_region: bool,
    ) -> Result<Key> {
        log::info!(
            "creating KMS CMK key spec {:?}, key usage {:?}, multi-region '{multi_region}', region '{}'",
            key_spec,
            key_usage,
            self.region
        );
        let mut req = self
            .cli
            .create_key()
            // ref. https://docs.aws.amazon.com/kms/latest/developerguide/asymmetric-key-specs.html#key-spec-ecc
            // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html#API_CreateKey_RequestSyntax
            .key_spec(key_spec)
            // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html#KMS-CreateKey-request-KeyUsage
            .key_usage(key_usage);
        if let Some(tags) = &tags {
            for (k, v) in tags.iter() {
                req = req.tags(Tag::builder().tag_key(k).tag_value(v).build());
            }
        }
        if multi_region {
            req = req.multi_region(true);
        }

        let resp = req.send().await.map_err(|e| Error::API {
            message: format!("failed create_key {:?}", e),
            retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_create_key(&e),
        })?;

        let meta = match resp.key_metadata() {
            Some(v) => v,
            None => {
                return Err(Error::Other {
                    message: String::from("unexpected empty key metadata"),
                    retryable: false,
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

    /// Creates a KMS grant for encrypt and decrypt.
    /// And returns the grant Id and token.
    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateGrant.html>
    pub async fn create_grant_for_encrypt_decrypt(
        &self,
        key_id: &str,
        grantee_principal: &str,
    ) -> Result<(String, String)> {
        log::info!("creating KMS grant for encrypt and decrypt for the key Id '{key_id}' on the grantee '{grantee_principal}' in region '{}'", self.region);

        let out = self
            .cli
            .create_grant()
            .key_id(key_id)
            .grantee_principal(grantee_principal)
            .operations(GrantOperation::Encrypt)
            .operations(GrantOperation::Decrypt)
            .operations(GrantOperation::DescribeKey)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed create_grant {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_create_grant(&e),
            })?;

        let grant_id = out.grant_id().unwrap().to_string();
        let grant_token = out.grant_token().unwrap().to_string();
        log::info!("created grant Id {grant_id} and token {grant_token}");

        Ok((grant_id, grant_token))
    }

    /// Creates a KMS grant for Sign, Verify, and other read operations.
    /// And returns the grant Id and token.
    /// Note that "GetPublicKey" are not supported when creating a grant for a symmetric KMS.
    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateGrant.html>
    pub async fn create_grant_for_sign_reads(
        &self,
        key_id: &str,
        grantee_principal: &str,
    ) -> Result<(String, String)> {
        log::info!("creating KMS grant for sign and reads for the key '{key_id}' on the grantee '{grantee_principal}' in region '{}'", self.region);

        let out = self
            .cli
            .create_grant()
            .key_id(key_id)
            .grantee_principal(grantee_principal)
            .operations(GrantOperation::Sign)
            .operations(GrantOperation::Verify)
            .operations(GrantOperation::DescribeKey)
            .operations(GrantOperation::GetPublicKey)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed create_grant {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_create_grant(&e),
            })?;

        let grant_id = out.grant_id().unwrap().to_string();
        let grant_token = out.grant_token().unwrap().to_string();
        log::info!(
            "created grant Id '{grant_id}' and token '{grant_token}' in region '{}'",
            self.region
        );

        Ok((grant_id, grant_token))
    }

    /// Revokes a KMS grant.
    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_RevokeGrant.html>
    pub async fn revoke_grant(&self, key_id: &str, grant_id: &str) -> Result<()> {
        log::info!(
            "revoking KMS grant '{grant_id}' for the key Id '{key_id}' in region '{}'",
            self.region
        );

        self.cli
            .revoke_grant()
            .key_id(key_id)
            .grant_id(grant_id)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed revoke_grant {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_revoke_grant(&e),
            })?;

        Ok(())
    }

    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_DescribeKey.html>
    pub async fn describe_key(&self, key_arn: &str) -> Result<(String, DescribeKeyOutput)> {
        log::info!("describing KMS ARN '{key_arn}' in region '{}'", self.region);
        let desc = self
            .cli
            .describe_key()
            .key_id(key_arn)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed describe_key {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_describe_key(&e),
            })?;

        let key_id = desc.key_metadata().unwrap().key_id().unwrap().to_string();
        log::info!(
            "successfully described KMS CMK -- key Id '{}' and Arn '{}'",
            key_id,
            key_arn
        );

        Ok((key_id, desc))
    }

    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_GetPublicKey.html>
    pub async fn get_public_key(&self, key_arn: &str) -> Result<GetPublicKeyOutput> {
        log::info!(
            "getting public key for KMS ARN '{key_arn}' in region '{}'",
            self.region
        );
        let out = self
            .cli
            .get_public_key()
            .key_id(key_arn)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed get_public_key {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_get_public_key(&e),
            })?;

        Ok(out)
    }

    /// Creates a default symmetric AWS KMS CMK.
    /// ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_CreateKey.html
    pub async fn create_symmetric_default_key(
        &self,
        name: &str,
        multi_region: bool,
    ) -> Result<Key> {
        let mut tags = HashMap::new();
        tags.insert(String::from("Name"), name.to_string());
        self.create_key(
            KeySpec::SymmetricDefault,
            KeyUsageType::EncryptDecrypt,
            Some(tags),
            multi_region,
        )
        .await
    }

    /// Signs the 32-byte SHA256 output message with the ECDSA private key and the recoverable code.
    /// Make sure to use key ARN in case sign happens cross-account with a grant token.
    /// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html>
    pub async fn sign_digest_secp256k1_ecdsa_sha256(
        &self,
        key_arn: &str,
        digest: &[u8],
        grant_token: Option<String>,
    ) -> Result<Vec<u8>> {
        log::info!(
            "secp256k1 signing {}-byte digest message with key Arn '{key_arn}' (grant token exists '{}', region '{}')",
            digest.len(),
            grant_token.is_some(),
            self.region
        );

        // DO NOT DO THIS -- fails with "Digest is invalid length for algorithm ECDSA_SHA_256"
        // ref. https://github.com/awslabs/aws-sdk-rust/discussions/571
        // let msg = aws_smithy_types::base64::encode(digest);

        let mut builder = self
            .cli
            .sign()
            .key_id(key_arn)
            .message(Blob::new(digest)) // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-Message
            .message_type(MessageType::Digest) // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-MessageType
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256);
        if let Some(grant_token) = grant_token {
            builder = builder.grant_tokens(grant_token);
        }

        // ref. https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html
        let sign_output = builder.send().await.map_err(|e| {
            let retryable = errors::is_sdk_err_retryable(&e) || is_err_retryable_sign(&e);
            if !retryable {
                log::warn!("non-retryable sign error {}", explain_err_sign(&e));
            } else {
                log::warn!("retryable sign error {}", explain_err_sign(&e));
            }
            Error::API {
                message: e.to_string(),
                retryable,
            }
        })?;

        if let Some(blob) = sign_output.signature() {
            let sig = blob.as_ref();
            log::debug!(
                "DER-encoded signature output from KMS is {}-byte",
                sig.len()
            );
            return Ok(Vec::from(sig));
        }

        return Err(Error::API {
            message: String::from("signature blob not found"),
            retryable: false,
        });
    }

    /// Schedules to delete a KMS CMK.
    /// Pass either CMK Id or Arn.
    /// The minimum pending window days are 7.
    pub async fn schedule_to_delete(
        &self,
        key_arn: &str,
        pending_window_in_days: i32,
    ) -> Result<()> {
        log::info!("scheduling to delete KMS key '{key_arn}' in {pending_window_in_days} days, in region '{}'", self.region);
        let ret = self
            .cli
            .schedule_key_deletion()
            .key_id(key_arn)
            .pending_window_in_days(pending_window_in_days)
            .send()
            .await;

        let deleted = match ret {
            Ok(_) => true,
            Err(e) => {
                let mut ignore_err: bool = false;
                if is_err_does_not_exist_schedule_key_deletion(&e) {
                    log::warn!("KMS key '{key_arn}' does not exist");
                    ignore_err = true
                }
                if is_err_schedule_key_deletion_already_scheduled(&e) {
                    log::warn!("KMS key '{key_arn}' already scheduled for deletion");
                    ignore_err = true
                }
                if !ignore_err {
                    return Err(Error::API {
                        message: format!("failed schedule_key_deletion {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
                false
            }
        };
        if deleted {
            log::info!("successfully scheduled to delete KMS CMK '{key_arn}'");
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
            "encrypting data (plaintext size {}) in region '{}'",
            human_readable::bytes(plaintext.len() as f64),
            self.region
        );

        let resp = self
            .cli
            .encrypt()
            .key_id(key_id)
            .plaintext(Blob::new(plaintext))
            .encryption_algorithm(key_spec)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed encrypt {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_encrypt(&e),
            })?;

        let ciphertext = match resp.ciphertext_blob() {
            Some(v) => v,
            None => {
                return Err(Error::API {
                    message: String::from("EncryptOutput.ciphertext_blob not found"),
                    retryable: false,
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
            "decrypting data (ciphertext size {}) in region '{}'",
            human_readable::bytes(ciphertext.len() as f64),
            self.region
        );

        let resp = self
            .cli
            .decrypt()
            .key_id(key_id)
            .ciphertext_blob(Blob::new(ciphertext))
            .encryption_algorithm(key_spec)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed decrypt {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e) || is_err_retryable_decrypt(&e),
            })?;

        let plaintext = match resp.plaintext() {
            Some(v) => v,
            None => {
                return Err(Error::API {
                    message: String::from("DecryptOutput.plaintext not found"),
                    retryable: false,
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
        let d = fs::read(src_file).map_err(|e| Error::Other {
            message: format!("failed read {:?}", e),
            retryable: false,
        })?;
        let ciphertext = self.encrypt(key_id, spec, d).await?;
        let mut f = File::create(dst_file).map_err(|e| Error::Other {
            message: format!("failed File::create {:?}", e),
            retryable: false,
        })?;
        f.write_all(&ciphertext).map_err(|e| Error::Other {
            message: format!("failed File::write_all {:?}", e),
            retryable: false,
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
        let d = fs::read(src_file).map_err(|e| Error::Other {
            message: format!("failed read {:?}", e),
            retryable: false,
        })?;
        let plaintext = self.decrypt(key_id, spec, d).await?;
        let mut f = File::create(dst_file).map_err(|e| Error::Other {
            message: format!("failed File::create {:?}", e),
            retryable: false,
        })?;
        f.write_all(&plaintext).map_err(|e| Error::Other {
            message: format!("failed File::write_all {:?}", e),
            retryable: false,
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
            .map_err(|e| Error::API {
                message: format!("failed generate_data_key {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e)
                    || is_err_retryable_generate_data_key(&e),
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
        // ref. <https://doc.rust-lang.org/1.0.0/style/ownership/constructors.html>
        Self {
            id: String::from(id),
            arn: String::from(arn),
        }
    }
}

#[inline]
pub fn is_err_retryable_create_key(e: &SdkError<CreateKeyError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception() || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
pub fn is_err_retryable_create_grant(e: &SdkError<CreateGrantError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception() || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
pub fn is_err_retryable_revoke_grant(e: &SdkError<RevokeGrantError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception() || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
pub fn is_err_retryable_describe_key(e: &SdkError<DescribeKeyError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception() || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

/// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_GetPublicKey.html>
#[inline]
pub fn is_err_retryable_get_public_key(e: &SdkError<GetPublicKeyError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception()
                || err.err().is_key_unavailable_exception()
                || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
pub fn is_err_retryable_generate_data_key(e: &SdkError<GenerateDataKeyError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception()
                || err.err().is_key_unavailable_exception()
                || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
fn is_err_retryable_encrypt(e: &SdkError<EncryptError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception()
                || err.err().is_key_unavailable_exception()
                || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
fn is_err_retryable_decrypt(e: &SdkError<DecryptError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception()
                || err.err().is_key_unavailable_exception()
                || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

#[inline]
fn is_err_does_not_exist_schedule_key_deletion(e: &SdkError<ScheduleKeyDeletionError>) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_not_found_exception(),
        _ => false,
    }
}

#[inline]
fn is_err_schedule_key_deletion_already_scheduled(e: &SdkError<ScheduleKeyDeletionError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let msg = format!("{:?}", err);
            msg.contains("pending deletion")
        }
        _ => false,
    }
}

/// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-SigningAlgorithm>
#[inline]
pub fn is_err_retryable_sign(e: &SdkError<SignError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_dependency_timeout_exception()
                || err.err().is_key_unavailable_exception()
                || err.err().is_kms_internal_exception()
        }
        _ => false,
    }
}

/// ref. <https://docs.aws.amazon.com/kms/latest/APIReference/API_Sign.html#KMS-Sign-request-SigningAlgorithm>
#[inline]
pub fn explain_err_sign(e: &SdkError<SignError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "sign service error [code '{:?}', kind '{:?}', meta '{:?}']",
            err.err().code(),
            err.err().retryable_error_kind(),
            err.err().meta(),
        ),
        _ => e.to_string(),
    }
}
