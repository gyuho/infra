pub mod errors;

#[cfg(feature = "autoscaling")]
pub mod autoscaling;

#[cfg(feature = "cloudformation")]
pub mod cloudformation;

#[cfg(feature = "cloudwatch")]
pub mod cloudwatch;

#[cfg(feature = "ec2")]
pub mod ec2;

#[cfg(feature = "kms")]
pub mod kms;

#[cfg(feature = "s3")]
pub mod s3;

#[cfg(feature = "ssm")]
pub mod ssm;

#[cfg(feature = "sts")]
pub mod sts;

use std::io;

use aws_config::{self, meta::region::RegionProviderChain};
use aws_types::{region::Region, SdkConfig as AwsSdkConfig};

/// Loads an AWS config from default environments.
pub async fn load_config(reg: Option<String>) -> io::Result<AwsSdkConfig> {
    log::info!("loading AWS configuration for region {:?}", reg);
    let regp = RegionProviderChain::first_try(reg.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    let shared_config = aws_config::from_env().region(regp).load().await;
    Ok(shared_config)
}
