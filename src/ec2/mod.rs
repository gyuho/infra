pub mod disk;

use std::{
    fs::File,
    io::prelude::*,
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use crate::{
    errors::{
        Error::{Other, API},
        Result,
    },
    utils::rfc3339,
};
use aws_sdk_ec2::{
    error::DeleteKeyPairError,
    model::{
        Filter, Instance, InstanceState, InstanceStateName, Tag, Volume, VolumeAttachmentState,
    },
    types::SdkError,
    Client,
};
use aws_types::SdkConfig as AwsSdkConfig;
use chrono::{DateTime, NaiveDateTime, Utc};
use hyper::{Body, Method, Request};
use log::{info, warn};
use serde::{Deserialize, Serialize};

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

        info!("creating EC2 key-pair '{}'", key_name);
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

        info!("saving EC2 key-pair '{}' to '{}'", key_name, key_path);
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
        info!("deleting EC2 key-pair '{}'", key_name);
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
                warn!("key already deleted ({})", e);
            }
        };

        Ok(())
    }

    /// Describes all attached volumes by instance Id and device.
    /// If the "volume_id" is none and "instance_id" is none, it fetches the "instance_id"
    /// from the local EC2 instance's metadata service. If the "volume_id" is not none,
    /// it ignores the "instance_id" and "device".
    ///
    /// The region used for API call is inherited from the EC2 client SDK.
    ///
    /// e.g.,
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
    pub async fn describe_volumes(
        &self,
        volume_id: Option<String>,
        instance_id: Option<String>,
        device: Option<String>,
    ) -> Result<Vec<Volume>> {
        let mut filters: Vec<Filter> = vec![];

        if let Some(vol_id) = volume_id {
            info!("filtering volumes via volume Id {}", vol_id);
            filters.push(
                Filter::builder()
                    .set_name(Some(String::from("volume-id")))
                    .set_values(Some(vec![vol_id]))
                    .build(),
            );
        } else {
            let inst_id = if let Some(inst_id) = instance_id {
                inst_id
            } else {
                fetch_instance_id().await?
            };

            info!("filtering volumes via instance Id {}", inst_id);
            filters.push(
                Filter::builder()
                    .set_name(Some(String::from("attachment.instance-id")))
                    .set_values(Some(vec![inst_id.clone()]))
                    .build(),
            );

            if let Some(device) = device {
                info!("filtering volumes via device {}", device);
                filters.push(
                    Filter::builder()
                        .set_name(Some(String::from("attachment.device")))
                        .set_values(Some(vec![device]))
                        .build(),
                );
            }
        }

        let resp = match self
            .cli
            .describe_volumes()
            .set_filters(Some(filters))
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

        info!("described {} volumes", volumes.len());
        Ok(volumes)
    }

    /// Fetches the EBS volume by its attachment state.
    /// If "instance_id" is empty, it fetches from the local EC2 instance's metadata service.
    pub async fn get_volume(
        &self,
        instance_id: Option<String>,
        device_name: &str,
    ) -> Result<Volume> {
        let inst_id = if let Some(inst_id) = instance_id {
            inst_id
        } else {
            fetch_instance_id().await?
        };

        let device_path = if device_name.starts_with("/dev/") {
            device_name.to_string()
        } else {
            format!("/dev/{}", device_name).to_string()
        };

        info!("fetching EBS volume for '{}' on '{}'", inst_id, device_path);

        let volumes = self
            .describe_volumes(None, Some(inst_id), Some(device_path.to_string()))
            .await?;
        if volumes.is_empty() {
            return Err(API {
                message: "no volume found".to_string(),
                is_retryable: false,
            });
        }
        if volumes.len() != 1 {
            return Err(API {
                message: format!("unexpected volume devices found {}", volumes.len()),
                is_retryable: false,
            });
        }
        let volume = volumes[0].clone();

        return Ok(volume);
    }

    /// Polls EBS volume attachment state.
    /// If "instance_id" is empty, it fetches from the local EC2 instance's metadata service.
    pub async fn poll_volume_attachment_state(
        &self,
        instance_id: Option<String>,
        device_name: &str,
        desired_attachment_state: VolumeAttachmentState,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Volume> {
        let inst_id = if let Some(inst_id) = instance_id {
            inst_id
        } else {
            fetch_instance_id().await?
        };

        let device_path = if device_name.starts_with("/dev/") {
            device_name.to_string()
        } else {
            format!("/dev/{}", device_name).to_string()
        };

        info!(
            "polling volume attachment state '{}' '{}' with desired state {:?} for timeout {:?} and interval {:?}",
            inst_id, device_path, desired_attachment_state, timeout, interval,
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
            thread::sleep(itv);

            let volume = self.get_volume(Some(inst_id.clone()), &device_path).await?;
            if volume.attachments().is_none() {
                warn!("no attachment found");
                continue;
            }
            let attachments = volume.attachments().unwrap();
            if attachments.is_empty() {
                warn!("no attachment found");
                continue;
            }
            if attachments.len() != 1 {
                warn!("unexpected attachment found {}", attachments.len());
                continue;
            }
            let current_attachment_state = attachments[0].state().unwrap();
            info!(
                "poll (current volume attachment state {:?}, elapsed {:?})",
                current_attachment_state, elapsed
            );

            if current_attachment_state.eq(&desired_attachment_state) {
                return Ok(volume);
            }

            cnt += 1;
        }

        return Err(Other {
            message: format!("failed to poll volume state for '{}' in time", inst_id),
            is_retryable: true,
        });
    }

    /// Fetches all tags for the specified instance.
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn fetch_tags(&self, instance_id: Arc<String>) -> Result<Vec<Tag>> {
        info!("fetching tags for '{}'", instance_id);
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
        info!("fetched {} tags for '{}'", tags.len(), instance_id);

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
                warn!("empty reservation from describe_instances response");
                return Ok(vec![]);
            }
        };

        let mut droplets: Vec<Droplet> = Vec::new();
        for rsv in reservations.iter() {
            let instances = rsv.instances().unwrap();
            for instance in instances {
                let instance_id = instance.instance_id().unwrap();
                info!("instance {}", instance_id);
                droplets.push(Droplet::new(instance));
            }
        }

        Ok(droplets)
    }
}

/// Represents the underlying EC2 instance.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Droplet {
    pub instance_id: String,
    /// Represents the data format in RFC3339.
    /// ref. https://serde.rs/custom-date-format.html
    #[serde(with = "rfc3339::serde_format")]
    pub launched_at_utc: DateTime<Utc>,
    pub instance_state_code: i32,
    pub instance_state_name: String,
    pub availability_zone: String,
    pub public_hostname: String,
    pub public_ipv4: String,
}

impl Droplet {
    pub fn new(inst: &Instance) -> Self {
        let instance_id = match inst.instance_id.to_owned() {
            Some(v) => v,
            None => String::new(),
        };
        let launch_time = inst.launch_time().unwrap();
        let native_dt = NaiveDateTime::from_timestamp(launch_time.secs(), 0);
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

        Self {
            instance_id,
            launched_at_utc,
            instance_state_code,
            instance_state_name,
            availability_zone,
            public_hostname,
            public_ipv4,
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

/// Fetches the instance ID on the host EC2 machine.
pub async fn fetch_instance_id() -> Result<String> {
    fetch_metadata("instance-id").await
}

/// Fetches the public hostname of the host EC2 machine.
pub async fn fetch_public_hostname() -> Result<String> {
    fetch_metadata("public-hostname").await
}

/// Fetches the public IPv4 address of the host EC2 machine.
pub async fn fetch_public_ipv4() -> Result<String> {
    fetch_metadata("public-ipv4").await
}

/// Fetches the availability of the host EC2 machine.
pub async fn fetch_availability_zone() -> Result<String> {
    fetch_metadata("placement/availability-zone").await
}

/// Fetches the region of the host EC2 machine.
/// TODO: fix this...
pub async fn fetch_region() -> Result<String> {
    let mut az = fetch_availability_zone().await?;
    az.truncate(az.len() - 1);
    Ok(az)
}

/// Fetches instance metadata service v2 with the "path".
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-retrieval.html
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
/// e.g., curl -H "X-aws-ec2-metadata-token: $TOKEN" -v http://169.254.169.254/latest/meta-data/public-ipv4
async fn fetch_metadata(path: &str) -> Result<String> {
    info!("fetching meta-data/{}", path);

    let uri = format!("http://169.254.169.254/latest/meta-data/{}", path);
    let token = fetch_token().await?;
    let req = match Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("X-aws-ec2-metadata-token", token)
        .body(Body::empty())
    {
        Ok(r) => r,
        Err(e) => {
            return Err(API {
                message: format!("failed to build GET meta-data/{} {:?}", path, e),
                is_retryable: false,
            });
        }
    };

    let ret = http_manager::read_bytes(req, Duration::from_secs(5), false, true).await;
    let rs = match ret {
        Ok(bytes) => {
            let s = match String::from_utf8(bytes.to_vec()) {
                Ok(text) => text,
                Err(e) => {
                    return Err(API {
                        message: format!(
                            "GET meta-data/{} returned unexpected bytes {:?} ({})",
                            path, bytes, e
                        ),
                        is_retryable: false,
                    });
                }
            };
            s
        }
        Err(e) => {
            return Err(API {
                message: format!("failed GET meta-data/{} {:?}", path, e),
                is_retryable: false,
            })
        }
    };
    Ok(rs)
}

/// Serves session token for instance metadata service v2.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
/// e.g., curl -X PUT "http://169.254.169.254/latest/api/token" -H "X-aws-ec2-metadata-token-ttl-seconds: 21600"
const IMDS_V2_SESSION_TOKEN_URI: &str = "http://169.254.169.254/latest/api/token";

/// Fetches the IMDS v2 token.
async fn fetch_token() -> Result<String> {
    info!("fetching IMDS v2 token");

    let req = match Request::builder()
        .method(Method::PUT)
        .uri(IMDS_V2_SESSION_TOKEN_URI)
        .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
        .body(Body::empty())
    {
        Ok(r) => r,
        Err(e) => {
            return Err(API {
                message: format!("failed to build PUT api/token {:?}", e),
                is_retryable: false,
            });
        }
    };

    let ret = http_manager::read_bytes(req, Duration::from_secs(5), false, true).await;
    let token = match ret {
        Ok(bytes) => {
            let s = match String::from_utf8(bytes.to_vec()) {
                Ok(text) => text,
                Err(e) => {
                    return Err(API {
                        message: format!(
                            "PUT api/token returned unexpected bytes {:?} ({})",
                            bytes, e
                        ),
                        is_retryable: false,
                    });
                }
            };
            s
        }
        Err(e) => {
            return Err(API {
                message: format!("failed PUT api/token {:?}", e),
                is_retryable: false,
            })
        }
    };
    Ok(token)
}
