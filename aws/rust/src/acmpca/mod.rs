use std::collections::HashMap;

use crate::errors::{self, Error, Result};
use aws_sdk_acmpca::{
    operation::delete_certificate_authority::DeleteCertificateAuthorityError,
    types::{
        Asn1Subject, CertificateAuthority, CertificateAuthorityConfiguration,
        CertificateAuthorityStatus, CertificateAuthorityType, CertificateAuthorityUsageMode,
        KeyAlgorithm, RevocationReason, SigningAlgorithm, Tag, Validity, ValidityPeriodType,
    },
    Client,
};
use aws_smithy_runtime_api::client::result::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;
use tokio::fs;

/// Implements AWS Private CA manager.
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

    /// Creates a new private root CA with RSA2048 key algorithm and SHA256 RSA signing algorithm.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_CreateCertificateAuthority.html>
    /// ref. <https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-acmpca-certificateauthority.html>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/Create-CA-CLI.html>
    pub async fn create_root_ca(
        &self,
        org: &str,
        common_name: &str,
        tags: Option<HashMap<String, String>>,
    ) -> Result<String> {
        log::info!("creating a new private CA with org '{org}' and common name '{common_name}'");

        let mut req = self
            .cli
            .create_certificate_authority()
            .certificate_authority_type(CertificateAuthorityType::Root)
            .usage_mode(CertificateAuthorityUsageMode::GeneralPurpose)
            .certificate_authority_configuration(
                CertificateAuthorityConfiguration::builder()
                    .key_algorithm(KeyAlgorithm::Rsa2048)
                    .signing_algorithm(SigningAlgorithm::Sha256Withrsa)
                    .subject(
                        Asn1Subject::builder()
                            .country("US")
                            .organization(org)
                            .common_name(common_name)
                            .build(),
                    )
                    .build()
                    .map_err(|e| Error::Other {
                        message: format!("failed to build CertificateAuthorityConfiguration {}", e),
                        retryable: false,
                    })?,
            );
        if let Some(tags) = &tags {
            for (k, v) in tags.iter() {
                req =
                    req.tags(
                        Tag::builder()
                            .key(k)
                            .value(v)
                            .build()
                            .map_err(|e| Error::Other {
                                message: format!("failed to build Tag {}", e),
                                retryable: false,
                            })?,
                    );
            }
        }

        let resp = match req.send().await {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed create_certificate_authority {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let ca_arn = resp.certificate_authority_arn().unwrap().to_owned();
        log::info!("successfully created a private CA '{ca_arn}'");

        Ok(ca_arn)
    }

    /// Disables the private CA.
    /// Note that self-signed cert cannot be revoked.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_UpdateCertificateAuthority.html>
    pub async fn disable_ca(&self, ca_arn: &str) -> Result<()> {
        log::info!("disabling private CA '{ca_arn}'");
        match self
            .cli
            .update_certificate_authority()
            .certificate_authority_arn(ca_arn)
            .status(CertificateAuthorityStatus::Disabled)
            .send()
            .await
        {
            Ok(_) => log::info!("successfully disabled the private CA"),
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed update_certificate_authority {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        Ok(())
    }

    /// Deletes a private CA.
    pub async fn delete_ca(&self, ca_arn: &str) -> Result<()> {
        log::info!("deleting private CA '{ca_arn}'");
        match self
            .cli
            .delete_certificate_authority()
            .certificate_authority_arn(ca_arn)
            .send()
            .await
        {
            Ok(_) => log::info!("successfully deleted the private CA"),
            Err(e) => {
                if is_err_not_found_delete_certificate_authority(&e) {
                    log::warn!(
                        "private CA '{ca_arn}' not found thus no need to delete ({})",
                        e
                    );
                    return Ok(());
                }
                return Err(Error::API {
                    message: format!("failed delete_certificate_authority {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e)
                        || is_err_retryable_delete_certificate_authority(&e),
                });
            }
        };

        Ok(())
    }

    /// Describes a private root CA.
    pub async fn describe_ca(&self, ca_arn: &str) -> Result<CertificateAuthority> {
        log::info!("describing private CA '{ca_arn}'");
        let resp = match self
            .cli
            .describe_certificate_authority()
            .certificate_authority_arn(ca_arn)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_certificate_authority {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let ca = resp.certificate_authority().unwrap();
        Ok(ca.clone())
    }

    /// Get the certificate, and returns the certificate in the base64-encoded PEM format.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_GetCertificateAuthorityCsr.html>
    pub async fn get_ca_csr(&self, ca_arn: &str) -> Result<String> {
        log::info!("getting CSR for CA '{ca_arn}'");

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_GetCertificateAuthorityCsr.html>
        let resp = match self
            .cli
            .get_certificate_authority_csr()
            .certificate_authority_arn(ca_arn)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed get_certificate_authority_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let csr_pem = resp.csr().unwrap();
        Ok(csr_pem.to_string())
    }

    /// Issues a new certificate with SHA256 RSA signing algorithm from the root CA.
    /// And returns the created certificate arn.
    /// CSR must be for the root. Otherwise fails with "Root CA CSR is not provided".
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/UsingTemplates.html>
    pub async fn issue_self_signed_cert_from_root_ca(
        &self,
        root_ca_arn: &str,
        valid_days: i64,
        csr_blob: aws_smithy_types::Blob,
    ) -> Result<String> {
        log::info!(
            "with CSR, issuing self-signed cert from root CA '{root_ca_arn}' with valid days {valid_days}"
        );

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html#API_IssueCertificate_RequestSyntax>
        let resp = match self
            .cli
            .issue_certificate()
            .certificate_authority_arn(root_ca_arn)
            .signing_algorithm(SigningAlgorithm::Sha256Withrsa)
            .validity(
                Validity::builder()
                    .r#type(ValidityPeriodType::Days)
                    .value(valid_days)
                    .build()
                    .map_err(|e| Error::Other {
                        message: format!("failed to build Validity {}", e),
                        retryable: false,
                    })?,
            )
            .csr(csr_blob)
            // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html#privateca-IssueCertificate-request-TemplateArn>
            // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
            // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/UsingTemplates.html>
            // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/UsingTemplates.html#RootCACertificate-V1>
            .template_arn("arn:aws:acm-pca:::template/RootCACertificate/V1")
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed issue_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let issued_cert_arn = resp.certificate_arn().unwrap();
        log::info!("successfully issued cert '{issued_cert_arn}' for root CA '{root_ca_arn}'");

        Ok(issued_cert_arn.to_string())
    }

    /// The root CA must be "ACTIVE" in order to issue this cert.
    /// Make sure the valid days is within the root CA validation period.
    /// Otherwise, "The certificate validity specified exceeds the certificate authority validity".
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/UsingTemplates.html>
    pub async fn issue_end_cert_from_root_ca(
        &self,
        root_ca_arn: &str,
        valid_days: i64,
        csr_blob: aws_smithy_types::Blob,
    ) -> Result<String> {
        log::info!(
            "with CSR, issuing end cert from root CA '{root_ca_arn}' with valid days {valid_days}"
        );

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html#API_IssueCertificate_RequestSyntax>
        let resp = match self
            .cli
            .issue_certificate()
            .certificate_authority_arn(root_ca_arn)
            .signing_algorithm(SigningAlgorithm::Sha256Withrsa)
            .validity(
                Validity::builder()
                    .r#type(ValidityPeriodType::Days)
                    .value(valid_days)
                    .build()
                    .map_err(|e| Error::Other {
                        message: format!("failed to build Validity {}", e),
                        retryable: false,
                    })?,
            )
            .csr(csr_blob)
            // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html#privateca-IssueCertificate-request-TemplateArn>
            // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
            // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/UsingTemplates.html>
            // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/UsingTemplates.html#EndEntityCertificate-V1>
            .template_arn("arn:aws:acm-pca:::template/EndEntityCertificate/V1")
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed issue_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let issued_cert_arn = resp.certificate_arn().unwrap();
        log::info!("successfully issued cert '{issued_cert_arn}' for root CA '{root_ca_arn}'");

        Ok(issued_cert_arn.to_string())
    }

    /// Get the certificate, and returns the certificate in the base64-encoded PEM format.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_GetCertificate.html>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    pub async fn get_cert_pem(&self, ca_arn: &str, cert_arn: &str) -> Result<String> {
        log::info!("getting cert PEM '{cert_arn}' for CA '{ca_arn}'");

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_GetCertificate.html>
        // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
        let resp = match self
            .cli
            .get_certificate()
            .certificate_authority_arn(ca_arn)
            .certificate_arn(cert_arn)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed get_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let cert_pem = resp.certificate().unwrap();
        Ok(cert_pem.to_string())
    }

    /// Imports the base64-encoded PEM formatted certificate into the CA.
    /// Can only import a self-signed cert if the CA is a type root.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_ImportCertificateAuthorityCertificate.html>
    /// ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    pub async fn import_cert(&self, cert_file_path: &str, ca_arn: &str) -> Result<()> {
        log::info!("importing cert PEM '{cert_file_path}' to CA '{ca_arn}'");
        let cert_raw = fs::read(cert_file_path).await.map_err(|e| Error::Other {
            message: format!("failed File::open {:?}", e),
            retryable: false,
        })?;

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_ImportCertificateAuthorityCertificate.html>
        // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
        let _ = match self
            .cli
            .import_certificate_authority_certificate()
            .certificate(aws_smithy_types::Blob::new(cert_raw))
            .certificate_authority_arn(ca_arn)
            .send()
            .await
        {
            Ok(_) => {
                log::info!("successfully imported cert PEM '{cert_file_path}' to CA '{ca_arn}'");
            }
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed import_certificate_authority_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        Ok(())
    }

    /// Get the imported CA certificate, and returns the certificate in the base64-encoded PEM format.
    /// Fails if the CA is still pending certificate.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_GetCertificateAuthorityCertificate.html>
    pub async fn get_ca_cert_pem(&self, ca_arn: &str) -> Result<String> {
        log::info!("getting cert PEM for CA '{ca_arn}'");

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_GetCertificateAuthorityCertificate.html>
        let resp = match self
            .cli
            .get_certificate_authority_certificate()
            .certificate_authority_arn(ca_arn)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed get_certificate_authority_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        let cert_pem = resp.certificate().unwrap();
        Ok(cert_pem.to_string())
    }

    /// Revokes the private CA.
    /// Note that self-signed cert cannot be revoked.
    pub async fn revoke_ca(&self, ca_arn: &str, cert_serial: &str) -> Result<()> {
        log::info!("revoking a private CA '{ca_arn}'");
        match self
            .cli
            .revoke_certificate()
            .certificate_authority_arn(ca_arn)
            .certificate_serial(cert_serial)
            .revocation_reason(RevocationReason::CessationOfOperation)
            .send()
            .await
        {
            Ok(_) => log::info!("successfully disabled the private CA"),
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed revoke_certificate {:?}", e),
                    retryable: match e.raw_response() {
                        Some(v) => v.status().is_server_error(),
                        None => false, // TODO: use "errors::is_sdk_err_retryable"
                    },
                });
            }
        };

        Ok(())
    }
}

#[inline]
pub fn is_err_retryable_delete_certificate_authority(
    e: &SdkError<
        DeleteCertificateAuthorityError,
        aws_smithy_runtime_api::client::orchestrator::HttpResponse,
    >,
) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_concurrent_modification_exception(),
        _ => false,
    }
}

#[inline]
pub fn is_err_not_found_delete_certificate_authority(
    e: &SdkError<
        DeleteCertificateAuthorityError,
        aws_smithy_runtime_api::client::orchestrator::HttpResponse,
    >,
) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_resource_not_found_exception(),
        _ => false,
    }
}
