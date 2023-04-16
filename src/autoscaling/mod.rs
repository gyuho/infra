use crate::errors::{self, Error, Result};
use aws_sdk_autoscaling::{operation::set_instance_health::SetInstanceHealthError, Client};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;

/// Implements AWS EC2 autoscaling manager.
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

    /// Sets the instance health: "Healthy" or "Unhealthy".
    pub async fn set_instance_health(&self, instance_id: &str, status: &str) -> Result<()> {
        log::info!(
            "setting instance health for '{}' with {}",
            instance_id,
            status
        );
        let ret = self
            .cli
            .set_instance_health()
            .instance_id(instance_id)
            .health_status(status)
            .send()
            .await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed set_instance_health {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e)
                        || is_err_retryable_set_instance_health(&e),
                });
            }
        };

        log::info!(
            "successfully set instance health for '{}' with {} (output: {:?})",
            instance_id,
            status,
            resp
        );
        Ok(())
    }
}

#[inline]
pub fn is_err_retryable_set_instance_health(e: &SdkError<SetInstanceHealthError>) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_resource_contention_fault(),
        _ => false,
    }
}
