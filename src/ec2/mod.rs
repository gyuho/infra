pub mod disk;
pub mod metadata;

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Error, ErrorKind, Write},
    path::Path,
};

use crate::errors::{
    Error::{Other, API},
    Result,
};
use aws_sdk_ec2::{
    operation::delete_key_pair::DeleteKeyPairError,
    types::{
        Address, AttachmentStatus, Filter, Instance, InstanceState, InstanceStateName,
        ResourceType, Tag, TagSpecification, Volume, VolumeAttachmentState, VolumeState,
    },
    Client,
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration, Instant};

/// Returns default instance types.
/// Avalanche consensus paper used "c5.large" for testing 125 ~ 2,000 nodes
/// Avalanche test net ("fuji") runs "c5.2xlarge"
///
/// https://aws.amazon.com/ec2/instance-types/c6a/
/// c6a.large:   2  vCPU + 4  GiB RAM
/// c6a.xlarge:  4  vCPU + 8  GiB RAM
/// c6a.2xlarge: 8  vCPU + 16 GiB RAM
/// c6a.4xlarge: 16 vCPU + 32 GiB RAM
/// c6a.8xlarge: 32 vCPU + 64 GiB RAM
///
/// https://aws.amazon.com/ec2/instance-types/m6a/
/// m6a.large:   2  vCPU + 8  GiB RAM
/// m6a.xlarge:  4  vCPU + 16 GiB RAM
/// m6a.2xlarge: 8  vCPU + 32 GiB RAM
/// m6a.4xlarge: 16 vCPU + 64 GiB RAM
/// m6a.8xlarge: 32 vCPU + 128 GiB RAM
///
/// https://aws.amazon.com/ec2/instance-types/m5/
/// m5.large:   2  vCPU + 8  GiB RAM
/// m5.xlarge:  4  vCPU + 16 GiB RAM
/// m5.2xlarge: 8  vCPU + 32 GiB RAM
/// m5.4xlarge: 16 vCPU + 64 GiB RAM
/// m5.8xlarge: 32 vCPU + 128 GiB RAM
///
/// https://aws.amazon.com/ec2/instance-types/c5/
/// c5.large:   2  vCPU + 4  GiB RAM
/// c5.xlarge:  4  vCPU + 8  GiB RAM
/// c5.2xlarge: 8  vCPU + 16 GiB RAM
/// c5.4xlarge: 16 vCPU + 32 GiB RAM
/// c5.9xlarge: 32 vCPU + 72 GiB RAM
///
/// https://aws.amazon.com/ec2/instance-types/r5/
/// r5.large:   2  vCPU + 16 GiB RAM
/// r5.xlarge:  4  vCPU + 32 GiB RAM
/// r5.2xlarge: 8  vCPU + 64 GiB RAM
/// r5.4xlarge: 16 vCPU + 128 GiB RAM
/// r5.8xlarge: 32 vCPU + 256 GiB RAM
///
/// https://aws.amazon.com/ec2/instance-types/t3/
/// t3.large:    2  vCPU + 8 GiB RAM
/// t3.xlarge:   4  vCPU + 16 GiB RAM
/// t3.2xlarge:  8  vCPU + 32 GiB RAM
///
///
/// Graviton
/// https://aws.amazon.com/ec2/instance-types/a1/
/// a1.large:   2 vCPU + 8  GiB RAM
/// a1.xlarge:  4 vCPU + 8 GiB RAM
/// a1.2xlarge: 8 vCPU + 16 GiB RAM
///
/// Graviton 3 (in preview)
/// https://aws.amazon.com/ec2/instance-types/c7g/
/// c7g.large:   2 vCPU + 8  GiB RAM
/// c7g.xlarge:  4 vCPU + 16 GiB RAM
/// c7g.2xlarge: 8 vCPU + 32 GiB RAM
///
/// Graviton 2
/// https://aws.amazon.com/ec2/instance-types/c6g/
/// c6g.large:   2 vCPU + 4  GiB RAM
/// c6g.xlarge:  4 vCPU + 8  GiB RAM
/// c6g.2xlarge: 8 vCPU + 16 GiB RAM
///
/// Graviton 2
/// https://aws.amazon.com/ec2/instance-types/m6g/
/// m6g.large:   2 vCPU + 8  GiB RAM
/// m6g.xlarge:  4 vCPU + 16 GiB RAM
/// m6g.2xlarge: 8 vCPU + 32 GiB RAM
///
/// Graviton 2
/// https://aws.amazon.com/ec2/instance-types/r6g/
/// r6g.large:   2 vCPU + 16 GiB RAM
/// r6g.xlarge:  4 vCPU + 32 GiB RAM
/// r6g.2xlarge: 8 vCPU + 64 GiB RAM
///
/// Graviton 2
/// https://aws.amazon.com/ec2/instance-types/t4/
/// t4g.large:   2 vCPU + 8 GiB RAM
/// t4g.xlarge:  4 vCPU + 16 GiB RAM
/// t4g.2xlarge: 8 vCPU + 32 GiB RAM
///
/// ref. <https://instances.vantage.sh/?min_memory=8&min_vcpus=4&region=us-west-2&cost_duration=monthly&selected=t4g.xlarge,c5.xlarge>
pub fn default_instance_types(
    region: &str,
    arch: &str,
    instance_size: &str,
) -> io::Result<Vec<String>> {
    // NOTE
    // incheon region doesn't support c6a/m6a yet
    // incheon region doesn't support a1 yet
    // r6g more expensive than c5...
    // ref. <https://aws.amazon.com/ec2/instance-types/r6g>
    match (region, arch, instance_size) {
        ("ap-northeast-2", "amd64", "4xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2", "amd64", "8xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2", "amd64", "12xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2", "amd64", "16xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2", "amd64", "24xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2", "amd64", _) => Ok(vec![
            format!("t3a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t3.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t2.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t2>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2", "arm64", "4xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2", "arm64", "8xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2", "arm64", "12xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2", "arm64", "16xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2", "arm64", _) => Ok(vec![
            format!("t4g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t4g>
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        (_, "amd64", "4xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, "amd64", "8xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, "amd64", "12xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, "amd64", "16xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, "amd64", "24xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, "amd64", _) => Ok(vec![
            format!("t3a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t3.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t2.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t2>
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, "arm64", "4xlarge") => Ok(vec![
            format!("a1.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/a1>
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        (_, "arm64", "8xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        (_, "arm64", _) => Ok(vec![
            format!("a1.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/a1>
            format!("t4g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t4g>
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unknown region '{region}' and arch '{arch}'"),
        )),
    }
}

/// Implements AWS EC2 manager.
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

    /// Creates an AWS EC2 key-pair and saves the private key to disk.
    /// It overwrites "key_path" file with the newly created key.
    pub async fn create_key_pair(&self, key_name: &str, key_path: &str) -> Result<()> {
        let path = Path::new(key_path);
        if path.exists() {
            return Err(Other {
                message: format!("key path {} already exists", key_path),
                is_retryable: false,
            });
        }

        log::info!("creating EC2 key-pair '{}'", key_name);
        let ret = self.cli.create_key_pair().key_name(key_name).send().await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(API {
                    message: format!("failed create_key_pair {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };

        log::info!(
            "persisting the created EC2 key-pair '{}' in '{}'",
            key_name,
            key_path
        );
        let key_material = resp.key_material().unwrap();

        let mut f = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                return Err(Other {
                    message: format!("failed to create file {:?}", e),
                    is_retryable: false,
                });
            }
        };
        match f.write_all(key_material.as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                return Err(Other {
                    message: format!("failed to write file {:?}", e),
                    is_retryable: false,
                });
            }
        }

        Ok(())
    }

    /// Deletes the AWS EC2 key-pair.
    pub async fn delete_key_pair(&self, key_name: &str) -> Result<()> {
        log::info!("deleting EC2 key-pair '{}'", key_name);
        let ret = self.cli.delete_key_pair().key_name(key_name).send().await;
        match ret {
            Ok(_) => {}
            Err(e) => {
                if !is_error_delete_key_pair_does_not_exist(&e) {
                    return Err(API {
                        message: format!("failed delete_key_pair {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
                log::warn!("key already deleted ({})", e);
            }
        };

        Ok(())
    }

    /// Describes the EBS volumes by filters.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVolumes.html
    pub async fn describe_volumes(&self, filters: Option<Vec<Filter>>) -> Result<Vec<Volume>> {
        log::info!("describing volumes...");
        let resp = match self
            .cli
            .describe_volumes()
            .set_filters(filters)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed describe_volumes {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };

        let volumes = if let Some(vols) = resp.volumes {
            vols
        } else {
            Vec::new()
        };

        log::info!("described {} volumes", volumes.len());
        Ok(volumes)
    }

    /// Polls the EBS volume by its state.
    pub async fn poll_volume_state(
        &self,
        ebs_volume_id: String,
        desired_state: VolumeState,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Option<Volume>> {
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

            let volumes = self
                .describe_volumes(Some(vec![Filter::builder()
                    .set_name(Some(String::from("volume-id")))
                    .set_values(Some(vec![ebs_volume_id.clone()]))
                    .build()]))
                .await?;
            if volumes.is_empty() {
                if desired_state.eq(&VolumeState::Deleted) {
                    log::info!("volume already deleted");
                    return Ok(None);
                }

                log::warn!("no volume found");
                continue;
            }
            if volumes.len() != 1 {
                log::warn!("unexpected {} volumes found", volumes.len());
                continue;
            }
            let volume = volumes[0].clone();

            let current_state = {
                if let Some(v) = volume.state() {
                    v.clone()
                } else {
                    VolumeState::from("not found")
                }
            };
            log::info!(
                "poll (current volume state {:?}, elapsed {:?})",
                current_state,
                elapsed
            );

            if current_state.eq(&desired_state) {
                return Ok(Some(volume));
            }

            cnt += 1;
        }

        Err(Other {
            message: format!(
                "failed to poll volume state for '{}' in time",
                ebs_volume_id
            ),
            is_retryable: true,
        })
    }

    /// Describes the attached volume by the volume Id and EBS device name.
    /// The "local_ec2_instance_id" is only set to bypass extra EC2 metadata
    /// service API calls.
    /// The region used for API call is inherited from the EC2 client SDK.
    ///
    /// e.g.,
    ///
    /// aws ec2 describe-volumes \
    /// --region ${AWS::Region} \
    /// --filters \
    ///   Name=attachment.instance-id,Values=$INSTANCE_ID \
    ///   Name=attachment.device,Values=/dev/xvdb \
    /// --query Volumes[].Attachments[].State \
    /// --output text
    ///
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVolumes.html
    /// ref. https://github.com/ava-labs/avalanche-ops/blob/fcbac87a219a8d3d6d3c38a1663fe1dafe78e04e/bin/avalancheup-aws/cfn-templates/asg_amd64_ubuntu.yaml#L397-L409
    ///
    pub async fn describe_local_volumes(
        &self,
        ebs_volume_id: Option<String>,
        ebs_device_name: String,
        local_ec2_instance_id: Option<String>,
    ) -> Result<Vec<Volume>> {
        let mut filters: Vec<Filter> = vec![];

        if let Some(v) = ebs_volume_id {
            log::info!("filtering volumes via volume Id {}", v);
            filters.push(
                Filter::builder()
                    .set_name(Some(String::from("volume-id")))
                    .set_values(Some(vec![v]))
                    .build(),
            );
        }

        let device = if ebs_device_name.starts_with("/dev/") {
            ebs_device_name
        } else {
            format!("/dev/{}", ebs_device_name.clone()).to_string()
        };
        log::info!("filtering volumes via EBS device name {}", device);
        filters.push(
            Filter::builder()
                .set_name(Some(String::from("attachment.device")))
                .set_values(Some(vec![device]))
                .build(),
        );

        let ec2_instance_id = if let Some(v) = local_ec2_instance_id {
            v
        } else {
            metadata::fetch_instance_id().await?
        };
        log::info!("filtering volumes via instance Id {}", ec2_instance_id);
        filters.push(
            Filter::builder()
                .set_name(Some(String::from("attachment.instance-id")))
                .set_values(Some(vec![ec2_instance_id]))
                .build(),
        );

        self.describe_volumes(Some(filters)).await
    }

    /// Polls the EBS volume attachment state.
    /// For instance, the "device_name" can be either "/dev/xvdb" or "xvdb" (for the secondary volume).
    pub async fn poll_local_volume_by_attachment_state(
        &self,
        ebs_volume_id: Option<String>,
        ebs_device_name: String,
        desired_attachment_state: VolumeAttachmentState,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Volume> {
        let local_ec2_instance_id = metadata::fetch_instance_id().await?;
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

            let volumes = self
                .describe_local_volumes(
                    ebs_volume_id.clone(),
                    ebs_device_name.clone(),
                    Some(local_ec2_instance_id.clone()),
                )
                .await?;
            if volumes.is_empty() {
                log::warn!("no volume found");
                continue;
            }
            if volumes.len() != 1 {
                log::warn!("unexpected {} volumes found", volumes.len());
                continue;
            }
            let volume = volumes[0].clone();
            if volume.attachments().is_none() {
                log::warn!("no attachment found");
                continue;
            }
            let attachments = volume.attachments().unwrap();
            if attachments.is_empty() {
                log::warn!("no attachment found");
                continue;
            }
            if attachments.len() != 1 {
                log::warn!("unexpected attachment found {}", attachments.len());
                continue;
            }
            let current_attachment_state = attachments[0].state().unwrap();
            log::info!(
                "poll (current volume attachment state {:?}, elapsed {:?})",
                current_attachment_state,
                elapsed
            );

            if current_attachment_state.eq(&desired_attachment_state) {
                return Ok(volume);
            }

            cnt += 1;
        }

        Err(Other {
            message: format!(
                "failed to poll volume attachment state for '{}' in time",
                local_ec2_instance_id
            ),
            is_retryable: true,
        })
    }

    /// Fetches all tags for the specified instance.
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn fetch_tags(&self, instance_id: &str) -> Result<Vec<Tag>> {
        log::info!("fetching tags for '{}'", instance_id);
        let ret = self
            .cli
            .describe_instances()
            .instance_ids(instance_id)
            .send()
            .await;
        let resp = match ret {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed describe_instances {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };

        let reservations = match resp.reservations {
            Some(rvs) => rvs,
            None => {
                return Err(API {
                    message: String::from("empty reservation from describe_instances response"),
                    is_retryable: false,
                });
            }
        };
        if reservations.len() != 1 {
            return Err(API {
                message: format!(
                    "expected only 1 reservation from describe_instances response but got {}",
                    reservations.len()
                ),
                is_retryable: false,
            });
        }

        let rvs = reservations.get(0).unwrap();
        let instances = rvs.instances.to_owned().unwrap();
        if instances.len() != 1 {
            return Err(API {
                message: format!(
                    "expected only 1 instance from describe_instances response but got {}",
                    instances.len()
                ),
                is_retryable: false,
            });
        }

        let instance = instances.get(0).unwrap();
        let tags = match instance.tags.to_owned() {
            Some(ss) => ss,
            None => {
                return Err(API {
                    message: String::from("empty tags from describe_instances response"),
                    is_retryable: false,
                });
            }
        };
        log::info!("fetched {} tags for '{}'", tags.len(), instance_id);

        Ok(tags)
    }

    /// Lists instances by the Auto Scaling Groups name.
    pub async fn list_asg(&self, asg_name: &str) -> Result<Vec<Droplet>> {
        let filter = Filter::builder()
            .set_name(Some(String::from("tag:aws:autoscaling:groupName")))
            .set_values(Some(vec![String::from(asg_name)]))
            .build();
        let resp = match self
            .cli
            .describe_instances()
            .set_filters(Some(vec![filter]))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed describe_instances {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };

        let reservations = match resp.reservations {
            Some(rvs) => rvs,
            None => {
                log::warn!("empty reservation from describe_instances response");
                return Ok(vec![]);
            }
        };

        let mut droplets: Vec<Droplet> = Vec::new();
        for rsv in reservations.iter() {
            let instances = rsv.instances().unwrap();
            for instance in instances {
                let instance_id = instance.instance_id().unwrap();
                log::info!("instance {}", instance_id);
                droplets.push(Droplet::new(instance));
            }
        }

        Ok(droplets)
    }

    /// Allocates an EIP and returns the allocation Id and the public Ip.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_AllocateAddress.html
    pub async fn allocate_eip(&self, tags: HashMap<String, String>) -> Result<Eip> {
        log::info!("allocating elastic IP with tags {:?}", tags);

        let mut eip_tags = TagSpecification::builder().resource_type(ResourceType::ElasticIp);
        for (k, v) in tags.iter() {
            eip_tags = eip_tags.tags(Tag::builder().key(k).value(v).build());
        }

        let resp = match self
            .cli
            .allocate_address()
            .tag_specifications(eip_tags.build())
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed allocate_address {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };

        let allocation_id = resp
            .allocation_id
            .to_owned()
            .unwrap_or_else(|| String::from(""));
        let public_ip = resp
            .public_ip
            .to_owned()
            .unwrap_or_else(|| String::from(""));
        log::info!("successfully allocated elastic IP {public_ip} with {allocation_id}");

        Ok(Eip {
            allocation_id,
            public_ip,
        })
    }

    /// Associates the elastic Ip with an EC2 instance.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_AssociateAddress.html
    pub async fn associate_eip(&self, allocation_id: &str, instance_id: &str) -> Result<String> {
        log::info!("associating elastic IP {allocation_id} with EC2 instance {instance_id}");
        let resp = match self
            .cli
            .associate_address()
            .allocation_id(allocation_id)
            .instance_id(instance_id)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed associate_address {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };

        let association_id = resp
            .association_id
            .to_owned()
            .unwrap_or_else(|| String::from(""));
        log::info!("successfully associated elastic IP {allocation_id} with association Id {association_id}");

        Ok(association_id)
    }

    /// Describes the elastic IP addresses with the instance Id.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeAddresses.html
    pub async fn describe_eips_by_instance_id(&self, instance_id: &str) -> Result<Vec<Address>> {
        log::info!("describing elastic IP addresses for EC2 instance {instance_id}");

        let resp = match self
            .cli
            .describe_addresses()
            .set_filters(Some(vec![Filter::builder()
                .set_name(Some(String::from("instance-id")))
                .set_values(Some(vec![instance_id.to_string()]))
                .build()]))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed describe_addresses {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };
        let addrs = if let Some(addrs) = resp.addresses() {
            addrs.to_vec()
        } else {
            Vec::new()
        };

        log::info!("successfully described addresses: {:?}", addrs);
        Ok(addrs)
    }

    /// Describes the elastic IP addresses with the tags.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeAddresses.html
    pub async fn describe_eips_by_tags(
        &self,
        tags: HashMap<String, String>,
    ) -> Result<Vec<Address>> {
        log::info!("describing elastic IP addresses with tags {:?}", tags);

        let mut filters = Vec::new();
        for (k, v) in tags.iter() {
            filters.push(
                Filter::builder()
                    .set_name(Some(format!("tag:{}", k)))
                    .set_values(Some(vec![v.clone()]))
                    .build(),
            );
        }
        let resp = match self
            .cli
            .describe_addresses()
            .set_filters(Some(filters))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(API {
                    message: format!("failed describe_addresses {:?}", e),
                    is_retryable: is_error_retryable(&e),
                });
            }
        };
        let addrs = if let Some(addrs) = resp.addresses() {
            addrs.to_vec()
        } else {
            Vec::new()
        };

        log::info!("successfully described addresses: {:?}", addrs);
        Ok(addrs)
    }

    /// Polls the elastic Ip for its describe address state,
    /// until the elastic Ip becomes attached to the instance.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeAddresses.html
    pub async fn poll_eip_by_describe_addresses(
        &self,
        association_id: &str,
        instance_id: &str,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Vec<Address>> {
        log::info!(
            "describing elastic IP association Id {association_id} for EC2 instance {instance_id}"
        );

        let filters = vec![
            Filter::builder()
                .set_name(Some(String::from("association-id")))
                .set_values(Some(vec![association_id.to_string()]))
                .build(),
            Filter::builder()
                .set_name(Some(String::from("instance-id")))
                .set_values(Some(vec![instance_id.to_string()]))
                .build(),
        ];

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

            let resp = match self
                .cli
                .describe_addresses()
                .set_filters(Some(filters.clone()))
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    return Err(API {
                        message: format!("failed describe_addresses {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
            };
            let addrs = if let Some(addrs) = resp.addresses() {
                addrs.to_vec()
            } else {
                Vec::new()
            };
            log::info!("successfully described addresses: {:?}", addrs);
            if !addrs.is_empty() {
                return Ok(addrs);
            }

            cnt += 1;
        }

        Err(Other {
            message: format!(
                "failed to poll describe_address elastic IP association Id {association_id} for EC2 instance {instance_id} in time",
            ),
            is_retryable: true,
        })
    }
}

/// Represents the underlying EC2 instance.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Droplet {
    pub instance_id: String,
    /// Represents the data format in RFC3339.
    /// ref. https://serde.rs/custom-date-format.html
    #[serde(with = "rfc_manager::serde_format::rfc_3339")]
    pub launched_at_utc: DateTime<Utc>,
    pub instance_state_code: i32,
    pub instance_state_name: String,
    pub availability_zone: String,
    pub public_hostname: String,
    pub public_ipv4: String,

    pub block_device_mappings: Vec<BlockDeviceMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BlockDeviceMapping {
    pub device_name: String,
    pub volume_id: String,
    pub attachment_status: String,
}

impl Droplet {
    pub fn new(inst: &Instance) -> Self {
        let instance_id = match inst.instance_id.to_owned() {
            Some(v) => v,
            None => String::new(),
        };
        let launch_time = inst.launch_time().unwrap();
        let native_dt = NaiveDateTime::from_timestamp_opt(launch_time.secs(), 0).unwrap();
        let launched_at_utc = DateTime::<Utc>::from_utc(native_dt, Utc);

        let instance_state = match inst.state.to_owned() {
            Some(v) => v,
            None => InstanceState::builder().build(),
        };
        let instance_state_code = instance_state.code.unwrap_or(0);
        let instance_state_name = instance_state
            .name
            .unwrap_or_else(|| InstanceStateName::from("unknown"));
        let instance_state_name = instance_state_name.as_str().to_string();

        let availability_zone = match inst.placement.to_owned() {
            Some(v) => match v.availability_zone {
                Some(v2) => v2,
                None => String::new(),
            },
            None => String::new(),
        };

        let public_hostname = inst
            .public_dns_name
            .to_owned()
            .unwrap_or_else(|| String::from(""));
        let public_ipv4 = inst
            .public_ip_address
            .to_owned()
            .unwrap_or_else(|| String::from(""));

        let mut block_device_mappings = Vec::new();
        if let Some(mappings) = inst.block_device_mappings() {
            for block_device_mapping in mappings.iter() {
                let device_name = block_device_mapping
                    .device_name
                    .to_owned()
                    .unwrap_or_else(|| String::from(""));

                let (volume_id, attachment_status) = if let Some(ebs) = block_device_mapping.ebs() {
                    let volume_id = ebs.volume_id.to_owned().unwrap_or_else(|| String::from(""));
                    let attachment_status = ebs
                        .status
                        .to_owned()
                        .unwrap_or_else(|| AttachmentStatus::from(""));
                    (volume_id, attachment_status.as_str().to_string())
                } else {
                    (String::new(), String::new())
                };

                block_device_mappings.push(BlockDeviceMapping {
                    device_name,
                    volume_id,
                    attachment_status,
                });
            }
        }

        Self {
            instance_id,
            launched_at_utc,
            instance_state_code,
            instance_state_name,
            availability_zone,
            public_hostname,
            public_ipv4,
            block_device_mappings,
        }
    }
}

#[inline]
pub fn is_error_retryable<E>(e: &SdkError<E>) -> bool {
    match e {
        SdkError::TimeoutError(_) | SdkError::ResponseError { .. } => true,
        SdkError::DispatchFailure(e) => e.is_timeout() || e.is_io(),
        _ => false,
    }
}

/// EC2 does not return any error for non-existing key deletes, just in case...
#[inline]
fn is_error_delete_key_pair_does_not_exist(e: &SdkError<DeleteKeyPairError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let msg = format!("{:?}", err);
            msg.contains("does not exist")
        }
        _ => false,
    }
}

/// Represents Elastic IP spec for management.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct Eip {
    pub allocation_id: String,
    pub public_ip: String,
}

impl Eip {
    /// Saves to disk overwriting the file, if any.
    pub fn sync(&self, file_path: &str) -> io::Result<()> {
        log::info!("syncing Eip spec to '{}'", file_path);
        let path = Path::new(file_path);
        let parent_dir = path.parent().expect("unexpected None parent");
        fs::create_dir_all(parent_dir)?;

        let d = serde_yaml::to_string(self).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to serialize Eip spec info to YAML {}", e),
            )
        })?;

        let mut f = File::create(file_path)?;
        f.write_all(d.as_bytes())?;

        Ok(())
    }

    /// Loads the Eip spec from disk.
    pub fn load(file_path: &str) -> io::Result<Self> {
        log::info!("loading Eip spec from {}", file_path);

        if !Path::new(file_path).exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("file {} does not exists", file_path),
            ));
        }

        let f = File::open(file_path).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to open {} ({})", file_path, e),
            )
        })?;

        serde_yaml::from_reader(f)
            .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("invalid YAML: {}", e)))
    }
}

/// RUST_LOG=debug cargo test --package aws-manager --lib -- ec2::test_eip --exact --show-output
#[test]
fn test_eip() {
    let d = r#"
allocation_id: test
public_ip: 1.2.3.4

"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let ret = f.write_all(d.as_bytes());
    assert!(ret.is_ok());
    let eip_path = f.path().to_str().unwrap();

    let ret = Eip::load(eip_path);
    assert!(ret.is_ok());
    let eip = ret.unwrap();

    let ret = eip.sync(eip_path);
    assert!(ret.is_ok());

    let orig = Eip {
        allocation_id: String::from("test"),
        public_ip: String::from("1.2.3.4"),
    };
    assert_eq!(eip, orig);
}
