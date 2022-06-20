pub mod cloudformation;
pub mod cloudwatch;
pub mod ec2;
pub mod envelope;
pub mod errors;
pub mod kms;
pub mod s3;
pub mod sts;
pub mod utils;

use std::io;

use aws_config::{self, meta::region::RegionProviderChain};
use aws_sdk_ec2::Region;
use aws_types::SdkConfig as AwsSdkConfig;
use log::info;

/// Loads an AWS config from default environments.
pub async fn load_config(reg: Option<String>) -> io::Result<AwsSdkConfig> {
    info!("loading AWS configuration for region {:?}", reg);
    let regp = RegionProviderChain::first_try(reg.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    let shared_config = aws_config::from_env().region(regp).load().await;
    Ok(shared_config)
}
