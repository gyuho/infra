pub mod errors;

#[cfg(feature = "acm")]
pub mod acm;

#[cfg(feature = "acmpca")]
pub mod acmpca;

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

#[cfg(feature = "sqs")]
pub mod sqs;

#[cfg(feature = "ssm")]
pub mod ssm;

#[cfg(feature = "sts")]
pub mod sts;

use aws_config::{
    self, meta::region::RegionProviderChain, timeout::TimeoutConfig, BehaviorVersion,
};
use aws_types::{region::Region, SdkConfig as AwsSdkConfig};
use tokio::time::Duration;

/// Loads an AWS config from default environments.
pub async fn load_config(
    region: Option<String>,
    profile_name: Option<String>,
    operation_timeout: Option<Duration>,
) -> AwsSdkConfig {
    log::info!("loading config for the region {:?}", region);

    // if region is None, it automatically detects iff it's running inside the EC2 instance
    let reg_provider = RegionProviderChain::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    let mut builder = TimeoutConfig::builder().connect_timeout(Duration::from_secs(5));
    if let Some(to) = &operation_timeout {
        if !to.is_zero() {
            builder = builder.operation_timeout(to.clone());
        }
    }
    let timeout_cfg = builder.build();

    let mut cfg = aws_config::defaults(BehaviorVersion::v2023_11_09())
        .region(reg_provider)
        .timeout_config(timeout_cfg);
    if let Some(p) = profile_name {
        log::info!("loading the aws profile '{p}'");
        cfg = cfg.profile_name(p);
    }

    cfg.load().await
}
