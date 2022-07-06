use tokio::time::{sleep, Duration};

use aws_manager::{self, ec2};
use aws_sdk_ec2::model::{ResourceType, Tag, TagSpecification, VolumeState, VolumeType};

/// cargo run --example ec2_ebs_describe_volumes
#[tokio::main]
async fn main() {
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

    let cli = ec2_manager.client();

    let resp = cli
        .create_volume()
        .volume_type(VolumeType::Gp3)
        .size(400)
        .iops(3000)
        .throughput(500)
        .encrypted(true)
        .tag_specifications(
            TagSpecification::builder()
                .resource_type(ResourceType::Volume)
                .tags(
                    Tag::builder()
                        .key(random_manager::string(10))
                        .value(random_manager::string(10))
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .unwrap();
    let volume_id = resp.volume_id().unwrap();
    log::info!("created {}", volume_id);

    sleep(Duration::from_secs(60)).await;

    let volume = ec2_manager
        .poll_volume_state(
            volume_id.to_string(),
            VolumeState::Available,
            Duration::from_secs(120),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    log::info!("polled volume {:?}", volume);

    sleep(Duration::from_secs(60)).await;

    let resp = cli
        .delete_volume()
        .volume_id(volume_id.to_string())
        .send()
        .await
        .unwrap();
    log::info!("deleted {:?}", resp);

    sleep(Duration::from_secs(20)).await;

    let volume = ec2_manager
        .poll_volume_state(
            volume_id.to_string(),
            VolumeState::Deleted,
            Duration::from_secs(120),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    log::info!("polled volume {:?}", volume);
}

/*
https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-properties-ec2-launchtemplate-blockdevicemapping.html
https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-properties-ec2-launchtemplate-blockdevicemapping-ebs.html#cfn-ec2-launchtemplate-blockdevicemapping-ebs-volumesize
https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-properties-ec2-launchtemplate-blockdevicemapping-ebs.html

BlockDeviceMappings:
    - DeviceName: "/dev/sda1"
      Ebs:
        VolumeType: gp3
        VolumeSize: 200

    - DeviceName: "/dev/xvdb"
      Ebs:
        VolumeType: !Ref VolumeType
        VolumeSize: !Ref VolumeSize
        Iops: !Ref VolumeIops
        Throughput: !Ref VolumeThroughput
        DeleteOnTermination: true
        Encrypted: true
*/
