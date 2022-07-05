use std::time::Duration;

use aws_manager::{self, ec2};
use aws_sdk_ec2::model::VolumeAttachmentState;

/// cargo run --example ec2_ebs_describe_volumes
fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    macro_rules! ab {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    let ret = ab!(aws_manager::load_config(None));
    let shared_config = ret.unwrap();
    let ec2_manager = ec2::Manager::new(&shared_config);

    let vol = ab!(ec2_manager.find_volume(None, "/dev/xvdb")).unwrap();
    log::info!("found volume {:?}", vol);

    let vol = ab!(ec2_manager.poll_volume_attachment_state(
        None,
        "/dev/xvdb",
        VolumeAttachmentState::Attached,
        Duration::from_secs(180),
        Duration::from_secs(10)
    ))
    .unwrap();
    log::info!("attched volume {:?}", vol);
}
