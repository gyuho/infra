use crate::errors::{self, Error, Result};
use aws_sdk_acm::{
    operation::export_certificate::ExportCertificateOutput, types::CertificateDetail, Client,
};
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

    /// Requests cert from the certificate authority.
    /// ref. <https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html>
    pub async fn request_private_cert(&self, domain_name: &str, ca_arn: &str) -> Result<String> {
        log::info!("requesting private cert on domain '{domain_name}' using CA '{ca_arn}'");
        let resp = match self
            .cli
            .request_certificate()
            .domain_name(domain_name)
            .certificate_authority_arn(ca_arn)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed request_certificate {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        let cert_arn = resp.certificate_arn().unwrap();
        log::info!("successfully issued private cert '{cert_arn}'");

        Ok(cert_arn.to_owned())
    }

    /// Deletes a cert.
    /// ref. <https://docs.aws.amazon.com/acm/latest/APIReference/API_DeleteCertificate.html>
    pub async fn delete_cert(&self, cert_arn: &str) -> Result<()> {
        log::info!("deleting cert '{cert_arn}'");
        let _ = match self
            .cli
            .delete_certificate()
            .certificate_arn(cert_arn)
            .send()
            .await
        {
            Ok(_) => {
                log::info!("successfully deleted cert '{cert_arn}'");
            }
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed delete_certificate {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        Ok(())
    }

    /// Exports cert.
    /// ref. <https://docs.aws.amazon.com/acm/latest/APIReference/API_ExportCertificate.html>
    pub async fn export_private_cert(
        &self,
        cert_arn: &str,
        passphrase: aws_smithy_types::Blob,
    ) -> Result<ExportCertificateOutput> {
        log::info!("exporting private cert '{cert_arn}'");
        let resp = match self
            .cli
            .export_certificate()
            .certificate_arn(cert_arn)
            .passphrase(passphrase)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed export_certificate {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };
        Ok(resp)
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
        Ok(cert_details.to_owned())
    }
}
