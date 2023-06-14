use aws_manager::{self, ec2};
use aws_sdk_ec2::types::{Filter, ResourceType, Tag, TagSpecification, VolumeState, VolumeType};
use tokio::time::{sleep, Duration};

/// cargo run --example ec2_ebs_create_volume --features="ec2"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(None, None, None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let ec2_manager = ec2::Manager::new(&shared_config);

    // ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeAvailabilityZones.html
    let resp = ec2_manager
        .cli
        .describe_availability_zones()
        .filters(
            Filter::builder()
                .set_name(Some(String::from("region-name")))
                .set_values(Some(vec![shared_config.region().unwrap().to_string()]))
                .build(),
        )
        .send()
        .await
        .unwrap();
    let az = {
        if let Some(v) = resp.availability_zones() {
            for z in v {
                log::info!("found AZ {:?}", z.zone_name());
            }
            v[0].zone_name().unwrap().to_string()
        } else {
            String::from("us-west-2a")
        }
    };
    log::info!("using az {}", az);

    // ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateVolume.html
    let resp = ec2_manager
        .cli
        .create_volume()
        .availability_zone(az)
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
                        .key(String::from("Name"))
                        .value(format!("test-{}", random_manager::secure_string(10)))
                        .build(),
                )
                .tags(
                    Tag::builder()
                        .key(String::from("ClusterId"))
                        .value(random_manager::secure_string(10))
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .unwrap();
    let volume_id = resp.volume_id().unwrap();
    log::info!("created {}", volume_id);

    sleep(Duration::from_secs(20)).await;

    let volume = ec2_manager
        .poll_volume_state(
            volume_id.to_string(),
            VolumeState::Available,
            Duration::from_secs(120),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    log::info!("polled volume after create: {:?}", volume.unwrap());

    sleep(Duration::from_secs(20)).await;

    let resp = ec2_manager
        .cli
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
    log::info!("polled volume after delete: {:?}", volume);
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
