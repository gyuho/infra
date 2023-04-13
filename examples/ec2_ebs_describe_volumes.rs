use aws_manager::{self, ec2};
use aws_sdk_ec2::types::VolumeAttachmentState;
use tokio::time::Duration;

/// cargo run --example ec2_ebs_describe_volumes --features="ec2"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(None, None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let ec2_manager = ec2::Manager::new(&shared_config);

    let vol = ec2_manager
        .describe_local_volumes(None, String::from("/dev/xvdb"), None)
        .await
        .unwrap();
    log::info!("found volume {:?}", vol);

    let vol = ec2_manager
        .poll_local_volume_by_attachment_state(
            None,
            String::from("/dev/xvdb"),
            VolumeAttachmentState::Attached,
            Duration::from_secs(180),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    log::info!("attched volume {:?}", vol);
}
