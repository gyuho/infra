pub mod disk;
pub mod metadata;
pub mod plugins;

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Write},
    path::Path,
    str::FromStr,
};

use crate::errors::{self, Error, Result};
use aws_sdk_ec2::{
    operation::delete_key_pair::DeleteKeyPairError,
    types::{
        Address, AttachmentStatus, Filter, Image, ImageState, Instance, InstanceState,
        InstanceStateName, KeyFormat, KeyType, ResourceType, SecurityGroup, Subnet, Tag,
        TagSpecification, Volume, VolumeAttachmentState, VolumeState, Vpc,
    },
    Client,
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration, Instant};

/// Defines the Arch type.
#[derive(
    Deserialize,
    Serialize,
    std::clone::Clone,
    std::cmp::Eq,
    std::cmp::Ord,
    std::cmp::PartialEq,
    std::cmp::PartialOrd,
    std::fmt::Debug,
    std::hash::Hash,
)]
pub enum ArchType {
    #[serde(rename = "amd64")]
    Amd64,
    #[serde(rename = "arm64")]
    Arm64,

    /// For p4 instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/p4/>
    #[serde(rename = "amd64-gpu-p4-nvidia-tesla-a100")]
    Amd64GpuP4NvidiaTeslaA100,
    /// For g3 instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/g3/>
    #[serde(rename = "amd64-gpu-g3-nvidia-tesla-m60")]
    Amd64GpuG3NvidiaTeslaM60,
    /// For g4dn instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/g4/>
    #[serde(rename = "amd64-gpu-g4dn-nvidia-t4")]
    Amd64GpuG4dnNvidiaT4,
    /// For g4 instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/g4/>
    #[serde(rename = "amd64-gpu-g4ad-radeon")]
    Amd64GpuG4adRadeon,
    /// For g5 instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/g5/>
    #[serde(rename = "amd64-gpu-g5-nvidia-a10g")]
    Amd64GpuG5NvidiaA10G,

    /// For inf1 instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/inf1/>
    #[serde(rename = "amd64-gpu-inf1")]
    Amd64GpuInf1,

    /// For trn1 instances.
    /// ref. <https://aws.amazon.com/ec2/instance-types/trn1/>
    #[serde(rename = "amd64-gpu-trn1")]
    Amd64GpuTrn1,

    Unknown(String),
}

impl std::convert::From<&str> for ArchType {
    fn from(s: &str) -> Self {
        match s {
            "amd64" => ArchType::Amd64,
            "arm64" => ArchType::Arm64,
            "amd64-gpu-p4-nvidia-tesla-a100" => ArchType::Amd64GpuP4NvidiaTeslaA100,
            "amd64-gpu-g3-nvidia-tesla-m60" => ArchType::Amd64GpuG3NvidiaTeslaM60,
            "amd64-gpu-g4dn-nvidia-t4" => ArchType::Amd64GpuG4dnNvidiaT4,
            "amd64-gpu-g4ad-radeon" => ArchType::Amd64GpuG4adRadeon,
            "amd64-gpu-g5-nvidia-a10g" => ArchType::Amd64GpuG5NvidiaA10G,
            "amd64-gpu-inf1" => ArchType::Amd64GpuInf1,
            "amd64-gpu-trn1" => ArchType::Amd64GpuTrn1,
            other => ArchType::Unknown(other.to_owned()),
        }
    }
}

impl std::str::FromStr for ArchType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(ArchType::from(s))
    }
}

impl ArchType {
    /// Returns the `&str` value of the enum member.
    pub fn as_str(&self) -> &str {
        match self {
            ArchType::Amd64 => "amd64",
            ArchType::Arm64 => "arm64",
            ArchType::Amd64GpuP4NvidiaTeslaA100 => "amd64-gpu-p4-nvidia-tesla-a100",
            ArchType::Amd64GpuG3NvidiaTeslaM60 => "amd64-gpu-g3-nvidia-tesla-m60",
            ArchType::Amd64GpuG4dnNvidiaT4 => "amd64-gpu-g4dn-nvidia-t4",
            ArchType::Amd64GpuG4adRadeon => "amd64-gpu-g4ad-radeon",
            ArchType::Amd64GpuG5NvidiaA10G => "amd64-gpu-g5-nvidia-a10g",
            ArchType::Amd64GpuInf1 => "amd64-gpu-inf1",
            ArchType::Amd64GpuTrn1 => "amd64-gpu-trn1",
            ArchType::Unknown(s) => s.as_ref(),
        }
    }

    /// Returns all the `&str` values of the enum members.
    pub fn values() -> &'static [&'static str] {
        &[
            "amd64",                          //
            "arm64",                          //
            "amd64-gpu-p4-nvidia-tesla-a100", //
            "amd64-gpu-g3-nvidia-tesla-m60",  //
            "amd64-gpu-g4dn-nvidia-t4",       //
            "amd64-gpu-g4ad-radeon",          //
            "amd64-gpu-g5-nvidia-a10g",       //
            "amd64-gpu-inf1",                 //
            "amd64-gpu-trn1",                 //
        ]
    }

    pub fn is_nvidia(&self) -> bool {
        matches!(
            self,
            ArchType::Amd64GpuP4NvidiaTeslaA100
                | ArchType::Amd64GpuG3NvidiaTeslaM60
                | ArchType::Amd64GpuG4dnNvidiaT4
                | ArchType::Amd64GpuG5NvidiaA10G
        )
    }
}

impl AsRef<str> for ArchType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// ref. <https://docs.aws.amazon.com/linux/al2023/ug/compare-with-al2.html>
/// ref. <https://us-west-2.console.aws.amazon.com/systems-manager/parameters/?region=us-west-2&tab=PublicTable#public_parameter_service=canonical>
pub fn default_image_id_ssm_parameter(arch: &str, os: &str) -> io::Result<String> {
    let arch_type = ArchType::from_str(arch).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("failed ArchType::from_str '{arch}' with {}", e),
        )
    })?;
    let os_type = OsType::from_str(os).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed OsType::from_str '{os}' with {}", e),
        )
    })?;

    match (arch_type, os_type) {
        (
            ArchType::Amd64
            | ArchType::Amd64GpuP4NvidiaTeslaA100
            | ArchType::Amd64GpuG3NvidiaTeslaM60
            | ArchType::Amd64GpuG4dnNvidiaT4
            | ArchType::Amd64GpuG4adRadeon
            | ArchType::Amd64GpuG5NvidiaA10G
            | ArchType::Amd64GpuInf1
            | ArchType::Amd64GpuTrn1,
            OsType::Al2023,
        ) => {
            Ok("/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64".to_string())
        }
        (ArchType::Arm64, OsType::Al2023) => {
            Ok("/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64".to_string())
        }

        (
            ArchType::Amd64
            | ArchType::Amd64GpuP4NvidiaTeslaA100
            | ArchType::Amd64GpuG3NvidiaTeslaM60
            | ArchType::Amd64GpuG4dnNvidiaT4
            | ArchType::Amd64GpuG4adRadeon
            | ArchType::Amd64GpuG5NvidiaA10G
            | ArchType::Amd64GpuInf1
            | ArchType::Amd64GpuTrn1,
            OsType::Ubuntu2004,
        ) => Ok(
            "/aws/service/canonical/ubuntu/server/20.04/stable/current/amd64/hvm/ebs-gp2/ami-id"
                .to_string(),
        ),
        (ArchType::Arm64, OsType::Ubuntu2004) => Ok(
            "/aws/service/canonical/ubuntu/server/20.04/stable/current/arm64/hvm/ebs-gp2/ami-id"
                .to_string(),
        ),

        (
            ArchType::Amd64
            | ArchType::Amd64GpuP4NvidiaTeslaA100
            | ArchType::Amd64GpuG3NvidiaTeslaM60
            | ArchType::Amd64GpuG4dnNvidiaT4
            | ArchType::Amd64GpuG4adRadeon
            | ArchType::Amd64GpuG5NvidiaA10G
            | ArchType::Amd64GpuInf1
            | ArchType::Amd64GpuTrn1,
            OsType::Ubuntu2204,
        ) => Ok(
            "/aws/service/canonical/ubuntu/server/22.04/stable/current/amd64/hvm/ebs-gp2/ami-id"
                .to_string(),
        ),
        (ArchType::Arm64, OsType::Ubuntu2204) => Ok(
            "/aws/service/canonical/ubuntu/server/22.04/stable/current/arm64/hvm/ebs-gp2/ami-id"
                .to_string(),
        ),

        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown arch '{arch}' or os_type '{os}'"),
        )),
    }
}

/// Returns default instance types.
/// ref. <https://instances.vantage.sh/?min_memory=8&min_vcpus=4&region=us-west-2&cost_duration=monthly&selected=t4g.xlarge,c5.xlarge>
/// ref. <https://docs.aws.amazon.com/dlami/latest/devguide/gpu.html>
///
/// TODO: add Graviton 3 (in preview)
/// ref. <https://aws.amazon.com/ec2/instance-types/c7g/>
pub fn default_instance_types(
    region: &str,
    arch: &str,
    instance_size: &str,
) -> io::Result<Vec<String>> {
    let arch_type = ArchType::from_str(arch).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("failed ArchType::from_str '{arch}' with {}", e),
        )
    })?;

    // NOTE
    // ICN/* regions do not support c6a/m6a yet
    // ICN/* regions do not support a1 yet
    // r6g more expensive than c5...
    // ref. <https://aws.amazon.com/ec2/instance-types/r6g>
    match (region, arch_type, instance_size) {
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Amd64, "4xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Amd64, "8xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Amd64, "12xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Amd64, "16xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Amd64, "24xlarge") => Ok(vec![
            format!("m5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Amd64, _) => Ok(vec![
            format!("t3a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t3.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t2.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t2>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),

        ("ap-northeast-2" | "ap-northeast-3", ArchType::Arm64, "4xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Arm64, "8xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Arm64, "12xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Arm64, "16xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        ("ap-northeast-2" | "ap-northeast-3", ArchType::Arm64, _) => Ok(vec![
            format!("t4g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t4g>
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),

        (_, ArchType::Amd64, "4xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, ArchType::Amd64, "8xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, ArchType::Amd64, "12xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, ArchType::Amd64, "16xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, ArchType::Amd64, "24xlarge") => Ok(vec![
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),
        (_, ArchType::Amd64, _) => Ok(vec![
            format!("t3a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t3.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t3>
            format!("t2.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/t2>
            format!("c6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6a>
            format!("m6a.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6a>
            format!("m5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/m5>
            format!("c5.{instance_size}"),  // ref. <https://aws.amazon.com/ec2/instance-types/c5>
        ]),

        (_, ArchType::Arm64, "4xlarge") => Ok(vec![
            format!("a1.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/a1>
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        (_, ArchType::Arm64, "8xlarge") => Ok(vec![
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),
        (_, ArchType::Arm64, _) => Ok(vec![
            format!("a1.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/a1>
            format!("t4g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/t4g>
            format!("c6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/c6g>
            format!("m6g.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/m6g>
        ]),

        (_, ArchType::Amd64GpuP4NvidiaTeslaA100, "24xlarge") => Ok(vec![
            format!("p4d.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/p4>
        ]),

        (_, ArchType::Amd64GpuG3NvidiaTeslaM60, "xlarge") => {
            Ok(vec![
                format!("g3s.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/g3>
            ])
        }
        (_, ArchType::Amd64GpuG3NvidiaTeslaM60, "4xlarge" | "8xlarge" | "16xlarge") => {
            Ok(vec![
                format!("g3.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/g3>
            ])
        }

        (
            _,
            ArchType::Amd64GpuG4dnNvidiaT4,
            "xlarge" | "2xlarge" | "4xlarge" | "8xlarge" | "12xlarge" | "16xlarge",
        ) => Ok(vec![
            format!("g4dn.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/g4>
        ]),
        (
            _,
            ArchType::Amd64GpuG4adRadeon,
            "xlarge" | "2xlarge" | "4xlarge" | "8xlarge" | "16xlarge",
        ) => {
            Ok(vec![
                format!("g4ad.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/g4>
            ])
        }

        (
            _,
            ArchType::Amd64GpuG5NvidiaA10G,
            "xlarge" | "2xlarge" | "4xlarge" | "8xlarge" | "12xlarge" | "16xlarge" | "24xlarge"
            | "48xlarge",
        ) => Ok(vec![
            format!("g5.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/g5>
        ]),

        (_, ArchType::Amd64GpuInf1, "xlarge" | "2xlarge" | "6xlarge" | "24xlarge") => Ok(vec![
            format!("inf1.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/inf1>
        ]),
        (_, ArchType::Amd64GpuTrn1, "2xlarge" | "32xlarge") => Ok(vec![
            format!("trn1.{instance_size}"), // ref. <https://aws.amazon.com/ec2/instance-types/trn1>
        ]),

        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown region '{region}' and arch '{arch}'"),
        )),
    }
}

/// Returns a set of valid instance types.
/// Empty if not known.
pub fn valid_instance_types(arch_type: ArchType) -> HashSet<String> {
    match arch_type {
        ArchType::Amd64GpuP4NvidiaTeslaA100 => {
            // ref. <https://aws.amazon.com/ec2/instance-types/p4>
            let mut s = HashSet::new();
            s.insert("p4d.24xlarge".to_string());
            s
        }
        ArchType::Amd64GpuG3NvidiaTeslaM60 => {
            // ref. <https://aws.amazon.com/ec2/instance-types/g3>
            let mut s = HashSet::new();
            s.insert("g3s.xlarge".to_string());
            s.insert("g3.4xlarge".to_string());
            s.insert("g3.8xlarge".to_string());
            s.insert("g3.16xlarge".to_string());
            s
        }
        ArchType::Amd64GpuG4dnNvidiaT4 => {
            // ref. <https://aws.amazon.com/ec2/instance-types/g4>
            let mut s = HashSet::new();
            s.insert("g4dn.xlarge".to_string());
            s.insert("g4dn.2xlarge".to_string());
            s.insert("g4dn.4xlarge".to_string());
            s.insert("g4dn.8xlarge".to_string());
            s.insert("g4dn.16xlarge".to_string());
            s.insert("g4dn.12xlarge".to_string());
            s.insert("g4dn.metal".to_string());
            s
        }
        ArchType::Amd64GpuG4adRadeon => {
            // ref. <https://aws.amazon.com/ec2/instance-types/g4>
            let mut s = HashSet::new();
            s.insert("g4ad.xlarge".to_string());
            s.insert("g4ad.2xlarge".to_string());
            s.insert("g4ad.4xlarge".to_string());
            s.insert("g4ad.8xlarge".to_string());
            s.insert("g4ad.16xlarge".to_string());
            s
        }
        ArchType::Amd64GpuG5NvidiaA10G => {
            // ref. <https://aws.amazon.com/ec2/instance-types/g5>
            let mut s = HashSet::new();
            s.insert("g5.xlarge".to_string());
            s.insert("g5.2xlarge".to_string());
            s.insert("g5.4xlarge".to_string());
            s.insert("g5.8xlarge".to_string());
            s.insert("g5.16xlarge".to_string());
            s.insert("g5.12xlarge".to_string());
            s.insert("g5.24xlarge".to_string());
            s.insert("g5.48xlarge".to_string());
            s
        }
        ArchType::Amd64GpuInf1 => {
            // ref. <https://aws.amazon.com/ec2/instance-types/inf1>
            let mut s = HashSet::new();
            s.insert("inf1.xlarge".to_string());
            s.insert("inf1.2xlarge".to_string());
            s.insert("inf1.6xlarge".to_string());
            s.insert("inf1.24xlarge".to_string());
            s
        }
        ArchType::Amd64GpuTrn1 => {
            // ref. <https://aws.amazon.com/ec2/instance-types/trn1>
            let mut s = HashSet::new();
            s.insert("trn1.2xlarge".to_string());
            s.insert("trn1.32xlarge".to_string());
            s.insert("trn1n.32xlarge".to_string());
            s
        }
        _ => HashSet::new(),
    }
}

/// Defines the OS type.
#[derive(
    Deserialize,
    Serialize,
    std::clone::Clone,
    std::cmp::Eq,
    std::cmp::Ord,
    std::cmp::PartialEq,
    std::cmp::PartialOrd,
    std::fmt::Debug,
    std::hash::Hash,
)]
pub enum OsType {
    #[serde(rename = "al2023")]
    Al2023,
    #[serde(rename = "ubuntu20.04")]
    Ubuntu2004,
    #[serde(rename = "ubuntu22.04")]
    Ubuntu2204,

    Unknown(String),
}

impl std::convert::From<&str> for OsType {
    fn from(s: &str) -> Self {
        match s {
            "al2023" => OsType::Al2023,
            "ubuntu20.04" => OsType::Ubuntu2004,
            "ubuntu-20.04" => OsType::Ubuntu2004,
            "ubuntu22.04" => OsType::Ubuntu2204,
            "ubuntu-22.04" => OsType::Ubuntu2204,
            other => OsType::Unknown(other.to_owned()),
        }
    }
}

impl std::str::FromStr for OsType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(OsType::from(s))
    }
}

impl OsType {
    /// Returns the `&str` value of the enum member.
    pub fn as_str(&self) -> &str {
        match self {
            OsType::Al2023 => "al2023",
            OsType::Ubuntu2004 => "ubuntu20.04",
            OsType::Ubuntu2204 => "ubuntu22.04",
            OsType::Unknown(s) => s.as_ref(),
        }
    }

    /// Returns all the `&str` values of the enum members.
    pub fn values() -> &'static [&'static str] {
        &[
            "al2023",      //
            "ubuntu20.04", //
            "ubuntu22.04", //
        ]
    }
}

impl AsRef<str> for OsType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// ref. <https://docs.aws.amazon.com/linux/al2023/ug/compare-with-al2.html>
pub fn default_user_name(os_type: &str) -> io::Result<String> {
    match OsType::from_str(os_type).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("failed OsType::from_str '{os_type}' with {}", e),
        )
    })? {
        OsType::Al2023 => Ok("ec2-user".to_string()),
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("ubuntu".to_string()),
        OsType::Unknown(v) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown os_type '{v}'"),
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

    /// Imports a public key to EC2 key.
    pub async fn import_key(&self, key_name: &str, pubkey_path: &str) -> Result<String> {
        let path = Path::new(pubkey_path);
        if !path.exists() {
            return Err(Error::Other {
                message: format!("public key path {pubkey_path} does not exist"),
                retryable: false,
            });
        }
        let pubkey_raw = fs::read(pubkey_path).map_err(|e| Error::Other {
            message: format!("failed to read {} {:?}", pubkey_path, e),
            retryable: false,
        })?;
        let pubkey_material = aws_smithy_types::Blob::new(pubkey_raw);

        log::info!(
            "importing a public key '{pubkey_path}' with key name '{key_name}' in region '{}'",
            self.region
        );

        let out = self
            .cli
            .import_key_pair()
            .key_name(key_name)
            .public_key_material(pubkey_material)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed import_key_pair {} {:?}", pubkey_path, e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        let key_pair_id = out.key_pair_id().unwrap().clone();
        log::info!("imported key pair id '{key_pair_id}' -- describing");

        let out = self
            .cli
            .describe_key_pairs()
            .key_pair_ids(key_pair_id)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed describe_key_pairs {} {:?}", pubkey_path, e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;
        if let Some(kps) = out.key_pairs() {
            if kps.len() != 1 {
                return Err(Error::API {
                    message: format!("unexpected {} key pairs from describe_key_pairs", kps.len()),
                    retryable: false,
                });
            }

            let described_key_name = kps[0].key_name().clone().unwrap().to_string();
            let described_key_pair_id = kps[0].key_pair_id().clone().unwrap().to_string();
            log::info!("described imported key name {described_key_name} and key pair id {described_key_pair_id}");

            if described_key_name != key_name {
                return Err(Error::API {
                    message: format!(
                        "unexpected described key name {} != {}",
                        described_key_name, key_name
                    ),
                    retryable: false,
                });
            }
            if described_key_pair_id != key_pair_id {
                return Err(Error::API {
                    message: format!(
                        "unexpected described key pair id {} != {}",
                        described_key_pair_id, key_pair_id
                    ),
                    retryable: false,
                });
            }
        } else {
            return Err(Error::API {
                message: format!("unexpected empty key pair from describe_key_pairs"),
                retryable: false,
            });
        }

        log::info!(
            "successfully imported the key {key_name} with the public key file {pubkey_path}"
        );
        Ok(key_pair_id.to_string())
    }

    /// Creates an AWS EC2 key-pair and saves the private key to disk.
    /// It overwrites "key_path" file with the newly created key.
    pub async fn create_key_pair(&self, key_name: &str, key_path: &str) -> Result<()> {
        let path = Path::new(key_path);
        if path.exists() {
            return Err(Error::Other {
                message: format!(
                    "private key path {} already exists, can't overwrite with a new key",
                    key_path
                ),
                retryable: false,
            });
        }

        // "KeyType::Rsa" is the default
        // "KeyFormat::Pem" is the default
        log::info!(
            "creating EC2 key-pair '{}' '{key_name}' in region '{}'",
            KeyType::Rsa.as_str(),
            self.region
        );
        let ret = self
            .cli
            .create_key_pair()
            .key_name(key_name)
            .key_type(KeyType::Rsa)
            .key_format(KeyFormat::Pem)
            .send()
            .await;
        let resp = match ret {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed create_key_pair {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
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
                return Err(Error::Other {
                    message: format!("failed to create file {:?}", e),
                    retryable: false,
                });
            }
        };
        match f.write_all(key_material.as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                return Err(Error::Other {
                    message: format!("failed to write file {:?}", e),
                    retryable: false,
                });
            }
        }

        Ok(())
    }

    /// Deletes the AWS EC2 key-pair.
    pub async fn delete_key_pair(&self, key_name: &str) -> Result<()> {
        log::info!(
            "deleting EC2 key-pair '{key_name}' in region '{}'",
            self.region
        );
        let ret = self.cli.delete_key_pair().key_name(key_name).send().await;
        match ret {
            Ok(_) => {}
            Err(e) => {
                if !is_err_does_not_exist_delete_key_pair(&e) {
                    return Err(Error::API {
                        message: format!("failed delete_key_pair {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
                log::warn!("key already deleted ({})", e);
            }
        };

        Ok(())
    }

    /// Describes an AWS EC2 VPC.
    /// ref. <https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVpcs.html>
    pub async fn describe_vpc(&self, vpc_id: &str) -> Result<Vpc> {
        log::info!("describing VPC '{vpc_id}' in region '{}'", self.region);
        let ret = self.cli.describe_vpcs().vpc_ids(vpc_id).send().await;
        let vpcs = match ret {
            Ok(out) => {
                if let Some(vpcs) = out.vpcs() {
                    vpcs.to_vec()
                } else {
                    return Err(Error::API {
                        message: "no vpc found".to_string(),
                        retryable: false,
                    });
                }
            }
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_vpcs {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };
        if vpcs.len() != 1 {
            return Err(Error::API {
                message: format!("expected 1 VPC, got {} VPCs", vpcs.len()),
                retryable: false,
            });
        }

        Ok(vpcs[0].to_owned())
    }

    /// Describes security groups by VPC Id.
    /// ref. <https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeSecurityGroups.html>
    pub async fn describe_security_groups_by_vpc(
        &self,
        vpc_id: &str,
    ) -> Result<Vec<SecurityGroup>> {
        log::info!(
            "describing security groups for '{vpc_id}' in region '{}'",
            self.region
        );
        let ret = self
            .cli
            .describe_security_groups()
            .filters(
                Filter::builder()
                    .set_name(Some(String::from("vpc-id")))
                    .set_values(Some(vec![vpc_id.to_string()]))
                    .build(),
            )
            .send()
            .await;
        match ret {
            Ok(out) => {
                if let Some(sgs) = out.security_groups() {
                    Ok(sgs.to_vec())
                } else {
                    return Err(Error::API {
                        message: "no security group found".to_string(),
                        retryable: false,
                    });
                }
            }
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_security_groups {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        }
    }

    /// Describes subnets by VPC Id.
    /// ref. <https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeSubnets.html>
    pub async fn describe_subnets_by_vpc(&self, vpc_id: &str) -> Result<Vec<Subnet>> {
        log::info!(
            "describing subnets for '{vpc_id}' in region '{}'",
            self.region
        );
        let ret = self
            .cli
            .describe_subnets()
            .filters(
                Filter::builder()
                    .set_name(Some(String::from("vpc-id")))
                    .set_values(Some(vec![vpc_id.to_string()]))
                    .build(),
            )
            .send()
            .await;
        match ret {
            Ok(out) => {
                if let Some(ss) = out.subnets() {
                    Ok(ss.to_vec())
                } else {
                    return Err(Error::API {
                        message: "no subnet found".to_string(),
                        retryable: false,
                    });
                }
            }
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_subnets {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        }
    }

    /// Describes the EBS volumes by filters.
    /// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVolumes.html
    pub async fn describe_volumes(&self, filters: Option<Vec<Filter>>) -> Result<Vec<Volume>> {
        log::info!("describing volumes in region '{}'", self.region);
        let resp = match self
            .cli
            .describe_volumes()
            .set_filters(filters)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::API {
                    message: format!("failed describe_volumes {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        let volumes = if let Some(vols) = resp.volumes {
            vols
        } else {
            Vec::new()
        };

        log::info!(
            "described {} volumes in region '{}'",
            volumes.len(),
            self.region
        );
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

        Err(Error::Other {
            message: format!(
                "failed to poll volume state for '{}' in time",
                ebs_volume_id
            ),
            retryable: true,
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
    /// ref. <https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVolumes.html>
    /// ref. <https://github.com/ava-labs/avalanche-ops/blob/fcbac87a219a8d3d6d3c38a1663fe1dafe78e04e/bin/avalancheup-aws/cfn-templates/asg_amd64_ubuntu.yaml#L397-L409>
    pub async fn describe_local_volumes(
        &self,
        ebs_volume_id: Option<String>,
        ebs_device_name: String,
        local_ec2_instance_id: Option<String>,
    ) -> Result<Vec<Volume>> {
        let mut filters: Vec<Filter> = vec![];

        if let Some(v) = ebs_volume_id {
            log::info!(
                "filtering volumes via volume Id '{}' in region '{}'",
                v,
                self.region
            );
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
        log::info!(
            "filtering volumes via EBS device name '{}' in region '{}'",
            device,
            self.region
        );
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

        Err(Error::Other {
            message: format!(
                "failed to poll volume attachment state for '{}' in time",
                local_ec2_instance_id
            ),
            retryable: true,
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
                return Err(Error::API {
                    message: format!("failed describe_instances {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        let reservations = match resp.reservations {
            Some(rvs) => rvs,
            None => {
                return Err(Error::API {
                    message: String::from("empty reservation from describe_instances response"),
                    retryable: false,
                });
            }
        };
        if reservations.len() != 1 {
            return Err(Error::API {
                message: format!(
                    "expected only 1 reservation from describe_instances response but got {}",
                    reservations.len()
                ),
                retryable: false,
            });
        }

        let rvs = reservations.get(0).unwrap();
        let instances = rvs.instances.to_owned().unwrap();
        if instances.len() != 1 {
            return Err(Error::API {
                message: format!(
                    "expected only 1 instance from describe_instances response but got {}",
                    instances.len()
                ),
                retryable: false,
            });
        }

        let instance = instances.get(0).unwrap();
        let tags = match instance.tags.to_owned() {
            Some(ss) => ss,
            None => {
                return Err(Error::API {
                    message: String::from("empty tags from describe_instances response"),
                    retryable: false,
                });
            }
        };
        log::info!("fetched {} tags for '{}'", tags.len(), instance_id);

        Ok(tags)
    }

    /// Lists instances by the Auto Scaling Groups name.
    pub async fn list_asg(&self, asg_name: &str) -> Result<Vec<Droplet>> {
        log::info!("listing asg '{asg_name}' for the region '{}'", self.region);

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
                return Err(Error::API {
                    message: format!("failed describe_instances {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
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
                return Err(Error::API {
                    message: format!("failed allocate_address {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
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
                return Err(Error::API {
                    message: format!("failed associate_address {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
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
                return Err(Error::API {
                    message: format!("failed describe_addresses {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
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
                return Err(Error::API {
                    message: format!("failed describe_addresses {:?}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
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
                    return Err(Error::API {
                        message: format!("failed describe_addresses {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
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

        Err(Error::Other {
            message: format!(
                "failed to poll describe_address elastic IP association Id {association_id} for EC2 instance {instance_id} in time",
            ),
            retryable: true,
        })
    }

    /// Creates an image and returns the AMI ID.
    pub async fn create_image(&self, instance_id: &str, image_name: &str) -> Result<String> {
        log::info!("creating an image '{image_name}' in instance '{instance_id}'");
        let ami = self
            .cli
            .create_image()
            .instance_id(instance_id)
            .name(image_name)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed create_image {:?}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        let ami_id = ami.image_id().clone().unwrap().to_string();
        log::info!("created AMI '{ami_id}' from the instance '{instance_id}'");

        Ok(ami_id)
    }

    /// Polls the image until the state is "Available".
    pub async fn poll_image_until_available(
        &self,
        image_id: &str,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Image> {
        log::info!("describing AMI {image_id} until available");

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

            let resp = match self.cli.describe_images().image_ids(image_id).send().await {
                Ok(r) => r,
                Err(e) => {
                    return Err(Error::API {
                        message: format!("failed describe_images {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
            };
            let images = if let Some(images) = resp.images() {
                images.to_vec()
            } else {
                Vec::new()
            };
            if images.len() != 1 {
                return Err(Error::Other {
                    message: format!(
                        "unexpected output from describe_images, expected 1 image but got {}",
                        images.len()
                    ),
                    retryable: false,
                });
            }
            let state = images[0].state().clone().unwrap();
            if state.eq(&ImageState::Available) {
                return Ok(images[0].clone());
            }

            log::info!(
                "image {image_id} is still {} (elapsed {:?})",
                state.as_str(),
                elapsed
            );

            cnt += 1;
        }

        Err(Error::Other {
            message: format!("failed to poll image state {image_id} in time",),
            retryable: true,
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

/// EC2 does not return any error for non-existing key deletes, just in case...
#[inline]
fn is_err_does_not_exist_delete_key_pair(e: &SdkError<DeleteKeyPairError>) -> bool {
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
            io::Error::new(
                io::ErrorKind::Other,
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
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file {} does not exists", file_path),
            ));
        }

        let f = File::open(file_path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("failed to open {} ({})", file_path, e),
            )
        })?;

        serde_yaml::from_reader(f).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("invalid YAML: {}", e))
        })
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
