use crate::errors::{self, Error, Result};
use aws_sdk_sts::Client;
use aws_types::SdkConfig as AwsSdkConfig;
use serde::{Deserialize, Serialize};

/// Implements AWS STS manager.
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

    /// Queries the AWS caller identity from the default AWS configuration.
    pub async fn get_identity(&self) -> Result<Identity> {
        log::info!("fetching STS caller identity");
        let resp = match self.cli.get_caller_identity().send().await {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed get_caller_identity {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        Ok(Identity::new(
            resp.account().unwrap_or(""),
            resp.arn().unwrap_or(""),
            resp.user_id().unwrap_or(""),
        ))
    }
}

/// Represents the caller identity.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct Identity {
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub role_arn: String,
    #[serde(default)]
    pub user_id: String,
}

impl Identity {
    pub fn new(account_id: &str, role_arn: &str, user_id: &str) -> Self {
        // ref. <https://doc.rust-lang.org/1.0.0/style/ownership/constructors.html>
        Self {
            account_id: String::from(account_id),
            role_arn: String::from(role_arn),
            user_id: String::from(user_id),
        }
    }
}
