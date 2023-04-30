use crate::errors::{self, Error, Result};
use aws_sdk_acm::{types::CertificateDetail, Client};
use aws_types::SdkConfig as AwsSdkConfig;

/// Implements AWS ACM manager.
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

    /// Describes the cert.
    /// ref. <https://docs.aws.amazon.com/acm/latest/APIReference/API_DescribeCertificate.html>
    /// ref. <https://docs.aws.amazon.com/acm/latest/APIReference/API_CertificateDetail.html#ACM-Type-CertificateDetail-Serial>
    pub async fn describe_cert(&self, cert_arn: &str) -> Result<CertificateDetail> {
        log::info!("describing cert '{cert_arn}'");
        let resp = match self
            .cli
            .describe_certificate()
            .certificate_arn(cert_arn)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_certificate {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        let cert_details = resp.certificate().unwrap();
        Ok(cert_details.clone())
    }
}
