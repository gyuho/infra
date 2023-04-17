use std::collections::HashMap;

use crate::errors::{self, Error, Result};
use aws_sdk_sqs::{
    error::ProvideErrorMetadata,
    {
        operation::{
            create_queue::CreateQueueError, delete_message::DeleteMessageError,
            delete_queue::DeleteQueueError,
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
        name: &str,
        msg_visibility_timeout_seconds: i32,
        msg_retention_period_days: i32,
    ) -> Result<String> {
        log::info!("creating a FIFO queue '{name}' with visibility seconds '{msg_visibility_timeout_seconds}', retention period  days '{msg_retention_period_days}'");

        if name.len() > 80 {
            return Err(Error::Other {
                message: format!("queue name '{name}' exceeds >80"),
                retryable: false,
            });
        }
        // FIFO queue name must end with the .fifo suffix.
        if !name.ends_with(".fifo") {
            return Err(Error::Other {
                message: format!("queue name '{name}' does not end with .fifo"),
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
            .queue_name(name)
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
            .tags("Name", name)
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
        log::info!("deleting a queue '{queue_url}'");

        self.cli
            .delete_queue()
            .queue_url(queue_url)
            .send()
            .await
            .map_err(|e| {
                log::warn!("failed to delete queue {:?}", e);
                Error::API {
                    message: format!("failed delete_queue {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e)
                        || is_err_retryable_delete_queue(&e),
                }
            })?;

        log::info!("successfully deleted '{queue_url}'");
        Ok(())
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

        let resp = req.send().await.map_err(|e| {
            log::warn!("failed to send msg {:?}", e);
            Error::API {
                message: format!("failed send_message {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            }
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
    ///
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_Message.html>
    /// ref. <https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_ReceiveMessage.html>
    pub async fn recv_msgs(
        &self,
        queue_url: &str,
        msg_visibility_timeout_seconds: i32,
        max_msgs: i32,
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

        let resp = self
            .cli
            .receive_message()
            .queue_url(queue_url)
            .attribute_names(QueueAttributeName::FifoQueue) // make it configurable
            .visibility_timeout(msg_visibility_timeout_seconds)
            .max_number_of_messages(max_msgs)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed receive_message {:?}", e),
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
        log::info!("deleting msg from '{queue_url}' with receipt id '{msg_receipt_handle}'");

        let _ = self
            .cli
            .delete_message()
            .queue_url(queue_url)
            .receipt_handle(msg_receipt_handle)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed delete_message '{}'", explain_err_delete_message(&e)),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        Ok(())
    }
}

#[inline]
pub fn is_err_retryable_delete_queue(e: &SdkError<DeleteQueueError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            // TODO: handle this...
            log::info!("message {}", err.err().message().unwrap());
            false
        }
        _ => false,
    }
}

#[inline]
pub fn explain_err_create_queue(e: &SdkError<CreateQueueError>) -> String {
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
