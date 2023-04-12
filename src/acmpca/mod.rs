use std::collections::HashMap;

use crate::errors::{Error, Result};
use aws_sdk_acmpca::{
    operation::delete_certificate_authority::DeleteCertificateAuthorityError,
    types::{
        Asn1Subject, CertificateAuthority, CertificateAuthorityConfiguration,
        CertificateAuthorityType, CertificateAuthorityUsageMode, KeyAlgorithm, SigningAlgorithm,
        Tag, Validity, ValidityPeriodType,
    },
    Client,
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;

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
                            .organization(org)
                            .country("US")
                            .common_name(common_name)
                            .build(),
                    )
                    .build(),
            );
        if let Some(tags) = &tags {
            for (k, v) in tags.iter() {
                req = req.tags(Tag::builder().key(k).value(v).build());
            }
        }

        let ret = req.send().await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed create_certificate_authority {:?}", e),
                    retryable: is_err_retryable(&e),
                });
            }
        };

        let ca_arn = resp.certificate_authority_arn().unwrap().to_owned();
        log::info!("successfully created a private CA '{ca_arn}'");

        Ok(ca_arn)
    }

    /// Deletes a private CA.
    pub async fn delete_ca(&self, arn: &str) -> Result<()> {
        log::info!("deleting a private CA '{arn}'");
        let ret = self
            .cli
            .delete_certificate_authority()
            .certificate_authority_arn(arn)
            .send()
            .await;
        match ret {
            Ok(_) => log::info!("successfully deleted the private CA"),
            Err(e) => {
                if is_err_not_found_delete_certificate_authority(&e) {
                    log::warn!(
                        "private CA '{arn}' not found thus no need to delete ({})",
                        e
                    );
                    return Ok(());
                }
                return Err(Error::API {
                    message: format!("failed delete_certificate_authority {:?}", e),
                    retryable: is_err_retryable(&e)
                        || is_err_retryable_delete_certificate_authority(&e),
                });
            }
        };

        Ok(())
    }

    /// Describes a private root CA.
    pub async fn describe_ca(&self, arn: &str) -> Result<CertificateAuthority> {
        log::info!("describing a new private CA '{arn}'");
        let ret = self
            .cli
            .describe_certificate_authority()
            .certificate_authority_arn(arn)
            .send()
            .await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_certificate_authority {:?}", e),
                    retryable: is_err_retryable(&e),
                });
            }
        };

        let ca = resp.certificate_authority().unwrap();
        Ok(ca.clone())
    }

    /// Issues a new certificate and returns the Arn with SHA256 RSA signing algorithm.
    /// ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html>
    pub async fn issue_cert(
        &self,
        ca: &str,
        valid_days: i64,
        csr_blob: aws_smithy_types::Blob,
    ) -> Result<String> {
        log::info!(
            "issuing a new cert for the certificate authority '{ca}' with valid days {valid_days}"
        );

        // ref. <https://docs.aws.amazon.com/privateca/latest/APIReference/API_IssueCertificate.html#API_IssueCertificate_RequestSyntax>
        let ret = self
            .cli
            .issue_certificate()
            .certificate_authority_arn(ca)
            .signing_algorithm(SigningAlgorithm::Sha256Withrsa)
            .csr(csr_blob)
            .validity(
                Validity::builder()
                    .r#type(ValidityPeriodType::Days)
                    .value(valid_days)
                    .build(),
            )
            .send()
            .await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed issue_certificate {:?}", e),
                    retryable: is_err_retryable(&e),
                });
            }
        };

        let cert_arn = resp.certificate_arn().unwrap();
        Ok(cert_arn.to_string())
    }
}

#[inline]
pub fn is_err_retryable<E>(e: &SdkError<E>) -> bool {
    match e {
        SdkError::TimeoutError(_) | SdkError::ResponseError { .. } => true,
        SdkError::DispatchFailure(e) => e.is_timeout() || e.is_io(),
        _ => false,
    }
}

#[inline]
pub fn is_err_retryable_delete_certificate_authority(
    e: &SdkError<DeleteCertificateAuthorityError>,
) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_concurrent_modification_exception(),
        _ => false,
    }
}

#[inline]
pub fn is_err_not_found_delete_certificate_authority(
    e: &SdkError<DeleteCertificateAuthorityError>,
) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_resource_not_found_exception(),
        _ => false,
    }
}
