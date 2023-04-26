use std::collections::{BTreeSet, HashMap};

use crate::errors::{self, Error, Result};
use aws_sdk_sqs::{
    error::ProvideErrorMetadata,
    {
        operation::{
            create_queue::CreateQueueError, delete_message::DeleteMessageError,
            delete_queue::DeleteQueueError, get_queue_attributes::GetQueueAttributesError,
            receive_message::ReceiveMessageError, send_message::SendMessageError,
        },
        types::{Message, MessageAttributeValue, QueueAttributeName},
        Client,
    },
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;

/// Implements AWS SQS manager.
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

    /// Creates a FIFO SQS queue.
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html>
    pub async fn create_fifo(
        &self,
        queue_name: &str,
        msg_visibility_timeout_seconds: i32,
        msg_retention_period_days: i32,
    ) -> Result<String> {
        log::info!("creating a FIFO queue '{queue_name}' with visibility seconds '{msg_visibility_timeout_seconds}', retention period  days '{msg_retention_period_days}', region '{}'", self.region);

        if queue_name.len() > 80 {
            return Err(Error::Other {
                message: format!("queue name '{queue_name}' exceeds >80"),
                retryable: false,
            });
        }
        // FIFO queue name must end with the .fifo suffix.
        if !queue_name.ends_with(".fifo") {
            return Err(Error::Other {
                message: format!("queue name '{queue_name}' does not end with .fifo"),
                retryable: false,
            });
        }

        // The default visibility timeout for a message is 30 seconds. The minimum is 0 seconds. The maximum is 12 hours.
        // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-visibility-timeout.html>
        let vs = if msg_visibility_timeout_seconds <= 0 {
            log::warn!("visibility seconds default to 30");
            "30".to_string()
        } else if msg_visibility_timeout_seconds > 43200 {
            log::warn!(
                "visibility seconds '{msg_visibility_timeout_seconds}' enforced to 12-hour (max allowed)"
            );
            "43200".to_string()
        } else {
            format!("{msg_visibility_timeout_seconds}").to_string()
        };

        // default 4 days, max 14 days
        let rs = if msg_retention_period_days < 4 {
            log::warn!("retention period days default to 4");
            "345600".to_string()
        } else if msg_retention_period_days > 14 {
            log::warn!(
                "retention period days '{msg_retention_period_days}' enforced to 14-day (max allowed)"
            );
            "1209600".to_string()
        } else {
            let sec = msg_retention_period_days * 24 * 60 * 60;
            format!("{sec}").to_string()
        };

        let resp = self
            .cli
            .create_queue()
            .queue_name(queue_name)
            .attributes(QueueAttributeName::MaximumMessageSize, "262144") // 256-KiB
            //
            // The default retention period is 4 days. The retention period has a range of 60 seconds to 1,209,600 seconds (14 days).
            // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_SetQueueAttributes.html>
            .attributes(QueueAttributeName::MessageRetentionPeriod, rs)
            //
            // 30-second; prevent other consumers from processing the message again
            // When a consumer receives and processes a message from a queue, the message remains in the queue.
            // Amazon SQS doesn't automatically delete the message.
            // Because Amazon SQS is a distributed system, there's no guarantee that the consumer actually receives the message.
            // Thus, the consumer must delete the message from the queue after receiving and processing it.
            // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-visibility-timeout.html>
            .attributes(QueueAttributeName::VisibilityTimeout, vs)
            //
            // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/high-throughput-fifo.html>
            .attributes(QueueAttributeName::FifoQueue, "true")
            .attributes(QueueAttributeName::SqsManagedSseEnabled, "true")
            .attributes(QueueAttributeName::DeduplicationScope, "messageGroup")
            .attributes(QueueAttributeName::ContentBasedDeduplication, "true")
            .attributes(QueueAttributeName::FifoThroughputLimit, "perMessageGroupId")
            //
            // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-server-side-encryption.html>
            .attributes(QueueAttributeName::SqsManagedSseEnabled, "true")
            .tags("Name", queue_name)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed create_queue '{}'", explain_err_create_queue(&e)),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        if let Some(queue_url) = resp.queue_url() {
            log::info!("created a FIFO queue '{queue_url}");
            Ok(queue_url.to_string())
        } else {
            Err(Error::API {
                message: "no queue URL found".to_string(),
                retryable: false,
            })
        }
    }

    /// Delete a queue.
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_DeleteQueue.html>
    pub async fn delete(&self, queue_url: &str) -> Result<()> {
        log::info!("deleting a queue '{queue_url}' in region '{}'", self.region);

        match self.cli.delete_queue().queue_url(queue_url).send().await {
            Ok(_) => {
                log::info!("successfully deleted '{queue_url}'");
            }
            Err(e) => {
                if !is_err_does_not_exist_delete_queue(&e) {
                    return Err(Error::API {
                        message: format!("failed delete_queue '{}'", explain_err_delete_queue(&e)),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
                log::warn!(
                    "queue already deleted or does not exist '{}'",
                    explain_err_delete_queue(&e)
                );
            }
        };

        Ok(())
    }

    /// Gets the queue attributes.
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_GetQueueAttributes.html>
    pub async fn get_attributes(
        &self,
        queue_url: &str,
    ) -> Result<HashMap<QueueAttributeName, String>> {
        log::info!(
            "getting the queue attributes '{queue_url}' in region '{}'",
            self.region
        );

        let resp = self
            .cli
            .get_queue_attributes()
            .queue_url(queue_url)
            .attribute_names(QueueAttributeName::All)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!(
                    "failed get_queue_attributes '{}'",
                    explain_err_get_queue_attributes(&e)
                ),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        if let Some(attr) = resp.attributes() {
            if let Some(v) = attr.get(&QueueAttributeName::ApproximateNumberOfMessages) {
                log::info!("queue '{queue_url}' has approximate '{v}' messages remaining");
                Ok(attr.clone())
            } else {
                Err(Error::API {
                    message: "no QueueAttributeName::ApproximateNumberOfMessages found in get_queue_attributes".to_string(),
                    retryable: false,
                })
            }
        } else {
            Err(Error::API {
                message: "empty queue attribute".to_string(),
                retryable: false,
            })
        }
    }

    /// Sends a message to an FIFO queue.
    ///
    /// Every message must have a unique MessageDeduplicationId.
    /// If empty, the FIFO must set "QueueAttributeName::ContentBasedDeduplication" to "true".
    ///
    /// It returns a message Id.
    ///
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_SendMessage.html>
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/using-messagededuplicationid-property.html>
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-message-metadata.html#sqs-message-attributes>
    pub async fn send_msg_to_fifo(
        &self,
        queue_url: &str,
        msg_group_id: &str,
        msg_dedup_id: Option<String>,
        msg_attributes: Option<HashMap<String, MessageAttributeValue>>,
        msg_body: &str,
    ) -> Result<String> {
        log::info!("sending msg to FIFO '{queue_url}' with group id '{msg_group_id}'");

        if let Some(id) = &msg_dedup_id {
            if id.len() > 128 {
                return Err(Error::Other {
                    message: format!("message duduplication id exceeds '{id}' exceeds >128"),
                    retryable: false,
                });
            }
        }
        if msg_body.len() > 262144 {
            return Err(Error::Other {
                message: "message length exceeds exceeds >256 KiB".to_string(),
                retryable: false,
            });
        }

        let mut req = self
            .cli
            .send_message()
            .queue_url(queue_url)
            .message_group_id(msg_group_id)
            .message_body(msg_body);

        // Every message must have a unique MessageDeduplicationId.
        // If empty, the FIFO must set "QueueAttributeName::ContentBasedDeduplication" to "true".
        // If you aren't able to provide a MessageDeduplicationId
        // and you enable ContentBasedDeduplication for your queue,
        // Amazon SQS uses a SHA-256 hash to generate the MessageDeduplicationId
        // using the body of the message (but not the attributes of the message).
        // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_SendMessage.html>
        if let Some(id) = &msg_dedup_id {
            req = req.message_deduplication_id(id);
        }
        if let Some(attrs) = &msg_attributes {
            for (k, v) in attrs.iter() {
                req = req.message_attributes(k, v.clone());
            }
        }

        let resp = req.send().await.map_err(|e| Error::API {
            message: format!("failed send_message '{}'", explain_err_send_message(&e)),
            retryable: errors::is_sdk_err_retryable(&e),
        })?;

        if let Some(msg_id) = resp.message_id() {
            log::info!("successfully sent message with id '{msg_id}'");
            Ok(msg_id.to_string())
        } else {
            Err(Error::API {
                message: "empty message Id from send_message".to_string(),
                retryable: true,
            })
        }
    }

    /// Receives messages from the queue, and returns the list of messages.
    /// When you delete later, make sure to use the receipt handle.
    ///
    /// If `msg_visibility_timeout_seconds` is zero, the overall visibility timeout for the queue is used for the returned messages.
    /// To return all of the attributes, specify "All" or ".*" in your request.
    ///
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_Message.html>
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_ReceiveMessage.html>
    pub async fn recv_msgs(
        &self,
        queue_url: &str,
        msg_visibility_timeout_seconds: i32,
        max_msgs: i32,
        msg_attribute_names: Option<BTreeSet<String>>,
    ) -> Result<Vec<Message>> {
        log::info!(
            "receiving msg from '{queue_url}' with visibility seconds '{msg_visibility_timeout_seconds}'"
        );

        if max_msgs > 10 {
            return Err(Error::Other {
                message: format!("MaxNumberOfMessages '{max_msgs}' exceeds >10"),
                retryable: false,
            });
        }

        // The default visibility timeout for a message is 30 seconds. The minimum is 0 seconds. The maximum is 12 hours.
        // ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-visibility-timeout.html>
        if msg_visibility_timeout_seconds <= 0 {
            return Err(Error::Other {
                message: format!(
                    "visibility second minimum is 0 second, got '{msg_visibility_timeout_seconds}'"
                ),
                retryable: false,
            });
        }
        if msg_visibility_timeout_seconds > 43200 {
            return Err(Error::Other {
                message: format!(
                    "visibility second maximum is 12-hour (43200-sec), got '{msg_visibility_timeout_seconds}'"
                ),
                retryable: false,
            });
        }

        let mut req = self
            .cli
            .receive_message()
            .queue_url(queue_url)
            .visibility_timeout(msg_visibility_timeout_seconds)
            .max_number_of_messages(max_msgs);
        if let Some(attrs) = &msg_attribute_names {
            for attr in attrs {
                req = req.message_attribute_names(attr.to_owned());
            }
        };

        let resp = req.send().await.map_err(|e| Error::API {
            message: format!(
                "failed receive_message '{}'",
                explain_err_receive_message(&e)
            ),
            retryable: errors::is_sdk_err_retryable(&e),
        })?;

        if let Some(msgs) = resp.messages() {
            log::info!(
                "received {} messages (requested max messages {max_msgs})",
                msgs.len()
            );
            Ok(msgs.to_vec())
        } else {
            log::info!("received zero message");
            Ok(Vec::new())
        }
    }

    /// Deletes a message from the queue with the receipt Id.
    /// Use the receipt handle to delete message(s) from the queue, not the message Id.
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_Message.html>
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_DeleteMessage.html>
    pub async fn delete_msg(&self, queue_url: &str, msg_receipt_handle: &str) -> Result<()> {
        log::info!("deleting msg receipt '{msg_receipt_handle}' from '{queue_url}'");

        match self
            .cli
            .delete_message()
            .queue_url(queue_url)
            .receipt_handle(msg_receipt_handle)
            .send()
            .await
        {
            Ok(_) => {
                log::info!(
                    "successfully deleted msg receipt '{msg_receipt_handle}' from '{queue_url}'"
                );
            }
            Err(e) => {
                if !is_err_does_not_exist_delete_message(&e) {
                    return Err(Error::API {
                        message: format!(
                            "failed delete_message '{}'",
                            explain_err_delete_message(&e)
                        ),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
                log::warn!(
                    "message already deleted or does not exist '{}'",
                    explain_err_delete_message(&e)
                );
            }
        };

        Ok(())
    }
}

#[inline]
fn explain_err_create_queue(e: &SdkError<CreateQueueError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "create_queue [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
fn explain_err_delete_queue(e: &SdkError<DeleteQueueError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "delete_queue [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

/// Handle:
/// ErrorMetadata { code: Some(\"AWS.SimpleQueueService.NonExistentQueue\"), message: Some(\"The specified queue does not exist for this wsdl version.\")
#[inline]
fn is_err_does_not_exist_delete_queue(e: &SdkError<DeleteQueueError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let code_match = if let Some(code) = err.err().code() {
                code.contains("NonExistentQueue")
            } else {
                false
            };
            let msg_match = if let Some(msg) = err.err().message() {
                msg.contains("does not exist")
            } else {
                false
            };
            code_match && msg_match
        }
        _ => false,
    }
}

#[inline]
fn explain_err_get_queue_attributes(e: &SdkError<GetQueueAttributesError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "get_queue_attributes [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
fn explain_err_send_message(e: &SdkError<SendMessageError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "send_message [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
fn explain_err_receive_message(e: &SdkError<ReceiveMessageError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "receive_message [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
pub fn explain_err_delete_message(e: &SdkError<DeleteMessageError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "delete_message [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

/// Handle:
/// delete_message [code 'Some(\"InvalidParameterValue\")', message 'Some(\"Value ... for parameter ReceiptHandle is invalid. Reason: The receipt handle has expired.\")
#[inline]
fn is_err_does_not_exist_delete_message(e: &SdkError<DeleteMessageError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let code_match = if let Some(code) = err.err().code() {
                code.contains("InvalidParameterValue")
            } else {
                false
            };
            let msg_match = if let Some(msg) = err.err().message() {
                msg.contains("ReceiptHandle is invalid")
            } else {
                false
            };
            code_match && msg_match
        }
        _ => false,
    }
}
