pub mod disk;
pub mod metadata;

use std::{fs::File, io::prelude::*, path::Path, sync::Arc};

use crate::errors::{
    Error::{Other, API},
    Result,
};
use aws_sdk_ec2::{
    error::DeleteKeyPairError,
    model::{
        AttachmentStatus, Filter, Instance, InstanceState, InstanceStateName, ResourceType, Tag,
        TagSpecification, Volume, VolumeAttachmentState, VolumeState,
    },
    types::SdkError,
    Client,
};
use aws_types::SdkConfig as AwsSdkConfig;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration, Instant};

/// Implements AWS EC2 manager.
#[derive(Debug, Clone)]
pub struct Manager {
    #[allow(dead_code)]
    shared_config: AwsSdkConfig,
    cli: Client,
}

impl Manager {
    pub fn new(shared_config: &AwsSdkConfig) -> Self {
        let cloned = shared_config.clone();
        let cli = Client::new(shared_config);
        Self {
            shared_config: cloned,
            cli,
        }
    }

    pub fn client(&self) -> Client {
        self.cli.clone()
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
                    VolumeState::Unknown(String::from("not found"))
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
    pub async fn fetch_tags(&self, instance_id: Arc<String>) -> Result<Vec<Tag>> {
        log::info!("fetching tags for '{}'", instance_id);
        let ret = self
            .cli
            .describe_instances()
            .instance_ids(instance_id.to_string())
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
    pub async fn allocate_eip(&self, id: &str) -> Result<(String, String)> {
        log::info!("allocating elastic IP with '{id}'");
        let resp = match self
            .cli
            .allocate_address()
            .tag_specifications(
                TagSpecification::builder()
                    .resource_type(ResourceType::ElasticIp)
                    .tags(Tag::builder().key(String::from("Name")).value(id).build())
                    .tags(Tag::builder().key(String::from("Id")).value(id).build())
                    .build(),
            )
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

        Ok((allocation_id, public_ip))
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

    /// Polls the elastic Ip for its describe add state.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeAddresses.html
    pub async fn poll_eip_by_describe_addresses(
        &self,
        association_id: &str,
        instance_id: &str,
        timeout: Duration,
        interval: Duration,
    ) -> Result<()> {
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
                        message: format!("failed associate_address {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
            };
            let addrs = if let Some(addrs) = resp.addresses() {
                addrs.to_vec()
            } else {
                Vec::new()
            };
            log::info!("described addresses: {:?}", addrs);
            if !addrs.is_empty() {
                break;
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
            .unwrap_or_else(|| InstanceStateName::Unknown(String::from("unknown")));
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
                        .unwrap_or_else(|| AttachmentStatus::Unknown(String::new()));
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
        SdkError::ServiceError { err, .. } => {
            let msg = format!("{:?}", err);
            msg.contains("does not exist")
        }
        _ => false,
    }
}
