use crate::errors::{self, Error, Result};
use aws_sdk_cloudformation::{
    operation::{delete_stack::DeleteStackError, describe_stacks::DescribeStacksError},
    types::{Capability, OnFailure, Output, Parameter, StackStatus, Tag},
    Client,
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;
use tokio::time::{sleep, Duration, Instant};

/// Implements AWS CloudFormation manager.
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

    /// Creates a CloudFormation stack.
    /// The separate caller is expected to poll the status asynchronously.
    pub async fn create_stack(
        &self,
        stack_name: &str,
        capabilities: Option<Vec<Capability>>,
        on_failure: OnFailure,
        template_body: &str,
        tags: Option<Vec<Tag>>,
        parameters: Option<Vec<Parameter>>,
    ) -> Result<Stack> {
        log::info!("creating stack '{}'", stack_name);
        let ret = self
            .cli
            .create_stack()
            .stack_name(stack_name)
            .set_capabilities(capabilities)
            .on_failure(on_failure)
            .template_body(template_body)
            .set_tags(tags)
            .set_parameters(parameters)
            .send()
            .await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed create_stack {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        let stack_id = resp.stack_id().unwrap();
        log::info!("created stack '{}' with '{}'", stack_name, stack_id);
        Ok(Stack::new(
            stack_name,
            stack_id,
            StackStatus::CreateInProgress,
            None,
        ))
    }

    /// Deletes a CloudFormation stack.
    /// The separate caller is expected to poll the status asynchronously.
    pub async fn delete_stack(&self, stack_name: &str) -> Result<Stack> {
        log::info!("deleting stack '{}'", stack_name);
        let ret = self.cli.delete_stack().stack_name(stack_name).send().await;
        match ret {
            Ok(_) => {}
            Err(e) => {
                if !is_err_does_not_exist_delete_stack(&e) {
                    return Err(Error::API {
                        message: format!("failed schedule_key_deletion {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
                log::warn!("stack already deleted so returning DeleteComplete status (original error '{}')", e);
                return Ok(Stack::new(
                    stack_name,
                    "",
                    StackStatus::DeleteComplete,
                    None,
                ));
            }
        };

        Ok(Stack::new(
            stack_name,
            "",
            StackStatus::DeleteInProgress,
            None,
        ))
    }

    /// Polls CloudFormation stack status.
    pub async fn poll_stack(
        &self,
        stack_name: &str,
        desired_status: StackStatus,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Stack> {
        log::info!(
            "polling stack '{}' with desired status {:?} for timeout {:?} and interval {:?}",
            stack_name,
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
                .describe_stacks()
                .stack_name(stack_name)
                .send()
                .await;
            let stacks = match ret {
                Ok(v) => v.stacks,
                Err(e) => {
                    // CFN should fail for non-existing stack, instead of returning 0 stack
                    if is_err_does_not_exist_describe_stacks(&e)
                        && desired_status.eq(&StackStatus::DeleteComplete)
                    {
                        log::info!("stack already deleted as desired");
                        return Ok(Stack::new(stack_name, "", desired_status, None));
                    }
                    return Err(Error::API {
                        message: format!("failed describe_stacks {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
            };
            let stacks = stacks.unwrap();
            if stacks.len() != 1 {
                // CFN should fail for non-existing stack, instead of returning 0 stack
                return Err(Error::Other {
                    message: String::from("failed to find stack"),
                    retryable: false,
                });
            }

            let stack = stacks.get(0).unwrap();
            let current_id = stack.stack_id().unwrap();
            let current_stack_status = stack.stack_status().unwrap();
            log::info!(
                "poll (current stack status {:?}, elapsed {:?})",
                current_stack_status,
                elapsed
            );

            if desired_status.eq(&StackStatus::CreateComplete)
                && current_stack_status.eq(&StackStatus::CreateFailed)
            {
                return Err(Error::Other {
                    message: String::from("stack create failed"),
                    retryable: false,
                });
            }
            if desired_status.eq(&StackStatus::CreateComplete)
                && current_stack_status.eq(&StackStatus::DeleteInProgress)
            {
                return Err(Error::Other {
                    message: String::from("stack create failed, being deleted"),
                    retryable: false,
                });
            }
            if desired_status.eq(&StackStatus::CreateComplete)
                && current_stack_status.eq(&StackStatus::DeleteComplete)
            {
                return Err(Error::Other {
                    message: String::from("stack create failed, already deleted"),
                    retryable: false,
                });
            }

            if desired_status.ne(&StackStatus::DeleteComplete) // create or update
                && current_stack_status.eq(&StackStatus::DeleteInProgress)
            {
                return Err(Error::Other {
                    message: String::from("stack create/update failed, being deleted"),
                    retryable: false,
                });
            }
            if desired_status.ne(&StackStatus::DeleteComplete) // create or update
                && current_stack_status.eq(&StackStatus::DeleteComplete)
            {
                return Err(Error::Other {
                    message: String::from("stack create/update failed, already deleted"),
                    retryable: false,
                });
            }

            if desired_status.eq(&StackStatus::DeleteComplete)
                && current_stack_status.eq(&StackStatus::DeleteFailed)
            {
                return Err(Error::Other {
                    message: String::from("stack delete failed"),
                    retryable: false,
                });
            }

            if current_stack_status.eq(&desired_status) {
                let outputs = if let Some(outputs) = stack.outputs() {
                    Some(Vec::from(outputs))
                } else {
                    None
                };
                return Ok(Stack::new(
                    stack_name,
                    current_id,
                    current_stack_status.clone(),
                    outputs,
                ));
            }

            cnt += 1;
        }

        Err(Error::Other {
            message: format!("failed to poll stack {} in time", stack_name),
            retryable: true,
        })
    }
}

/// Represents the CloudFormation stack.
#[derive(Debug)]
pub struct Stack {
    pub name: String,
    pub id: String,
    pub status: StackStatus,
    pub outputs: Option<Vec<Output>>,
}

impl Stack {
    pub fn new(name: &str, id: &str, status: StackStatus, outputs: Option<Vec<Output>>) -> Self {
        // ref. <https://doc.rust-lang.org/1.0.0/style/ownership/constructors.html>
        Self {
            name: String::from(name),
            id: String::from(id),
            status,
            outputs,
        }
    }
}

#[inline]
fn is_err_does_not_exist_delete_stack(e: &SdkError<DeleteStackError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let msg = format!("{:?}", err);
            msg.contains("does not exist")
        }
        _ => false,
    }
}

#[inline]
fn is_err_does_not_exist_describe_stacks(e: &SdkError<DescribeStacksError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let msg = format!("{:?}", err);
            msg.contains("does not exist")
        }
        _ => false,
    }
}
