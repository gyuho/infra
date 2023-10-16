use crate::errors::{self, Error, Result};
use aws_sdk_ssm::{types::CommandInvocationStatus, Client};
use aws_types::SdkConfig as AwsSdkConfig;
use tokio::time::{sleep, Duration, Instant};

#[derive(Debug, Clone)]
pub struct Ami {
    pub arn: String,
    pub name: String,
    pub version: i64,
    pub image_id: String,
    pub last_modified_date: aws_smithy_types::DateTime,
}

/// Implements AWS SSM manager.
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

    pub async fn fetch_ami(&self, key: &str) -> Result<Ami> {
        log::info!("polling ssm parameter for AMI {key}");
        let out = self
            .cli
            .get_parameters()
            .names(key)
            .send()
            .await
            .map_err(|e| Error::Other {
                message: format!("failed get_parameters {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        if let Some(ps) = out.parameters() {
            if ps.len() == 1 {
                Ok(Ami {
                    arn: ps[0].arn().clone().unwrap().to_string(),
                    name: ps[0].name().clone().unwrap().to_string(),
                    version: ps[0].version(),
                    image_id: ps[0].value().clone().unwrap().to_string(),
                    last_modified_date: *ps[0].last_modified_date().clone().unwrap(),
                })
            } else {
                Err(Error::Other {
                    message: "no parameter found".to_string(),
                    retryable: false,
                })
            }
        } else {
            Err(Error::Other {
                message: "no parameter found".to_string(),
                retryable: false,
            })
        }
    }

    /// Polls SSM command status.
    /// ref. <https://docs.aws.amazon.com/systems-manager/latest/APIReference/API_GetCommandInvocation.html>
    pub async fn poll_command(
        &self,
        command_id: &str,
        instance_id: &str,
        desired_status: CommandInvocationStatus,
        timeout: Duration,
        interval: Duration,
    ) -> Result<CommandInvocationStatus> {
        log::info!(
            "polling invocation status for command '{command_id}' and instance id '{instance_id}' in region '{}' with desired status {:?} for timeout {:?} and interval {:?}",
            self.region,
            desired_status,
            timeout,
            interval,
        );

        let start = Instant::now();
        let mut cnt: u128 = 0;
        loop {
            let elapsed = start.elapsed();
            if elapsed.gt(&timeout) {
                break;
            }

            let itv = {
                if cnt == 0 {
                    // first poll with no wait
                    Duration::from_secs(1)
                } else {
                    interval
                }
            };
            sleep(itv).await;

            let ret = self
                .cli
                .get_command_invocation()
                .command_id(command_id)
                .instance_id(instance_id)
                .send()
                .await;
            let out = match ret {
                Ok(v) => v,
                Err(e) => {
                    return Err(Error::API {
                        message: format!("failed get_command_invocation {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
            };

            let current_status = out.status().unwrap();
            log::info!(
                "poll (current command status {:?}, elapsed {:?})",
                current_status,
                elapsed
            );

            if desired_status.ne(&CommandInvocationStatus::Failed)
                && current_status.eq(&CommandInvocationStatus::Failed)
            {
                return Err(Error::Other {
                    message: String::from("command invocation failed"),
                    retryable: false,
                });
            }

            if current_status.eq(&desired_status) {
                return Ok(current_status.clone());
            }

            cnt += 1;
        }

        Err(Error::Other {
            message: format!("failed to get command invocation {} in time", command_id),
            retryable: true,
        })
    }
}
