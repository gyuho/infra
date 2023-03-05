use aws_manager::{self, cloudformation};
use aws_sdk_cloudformation::model::{OnFailure, Parameter, StackStatus, Tag};
use rust_embed::RustEmbed;
use tokio::time::{sleep, Duration};

/// cargo run --example cloudformation_vpc --features="cloudformation"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    println!();
    println!();
    println!();
    log::info!("creating AWS S3 resources!");
    let shared_config = aws_manager::load_config(Some(String::from("us-east-1")))
        .await
        .unwrap();
    log::info!("region {:?}", shared_config.region().unwrap());
    let cloudformation_manager = cloudformation::Manager::new(&shared_config);

    #[derive(RustEmbed)]
    #[folder = "examples/templates/"]
    #[prefix = "examples/templates/"]
    struct Asset;

    let vpc_yaml = Asset::get("examples/templates/vpc.yaml").unwrap();
    let template_body = std::str::from_utf8(vpc_yaml.data.as_ref()).unwrap();
    log::info!("{:?}", template_body);

    let stack_name = id_manager::time::with_prefix("test");

    // error should be ignored if it does not exist
    cloudformation_manager
        .delete_stack(&stack_name)
        .await
        .unwrap();

    let stack = cloudformation_manager
        .create_stack(
            &stack_name,
            None,
            OnFailure::Delete,
            template_body,
            Some(Vec::from([
                Tag::builder().key("KIND").value("avalanche-ops").build(),
                Tag::builder().key("a").value("b").build(),
            ])),
            Some(Vec::from([
                Parameter::builder()
                    .parameter_key("Id")
                    .parameter_value(id_manager::time::with_prefix("id"))
                    .build(),
                Parameter::builder()
                    .parameter_key("VpcCidr")
                    .parameter_value("10.0.0.0/16")
                    .build(),
                Parameter::builder()
                    .parameter_key("PublicSubnetCidr1")
                    .parameter_value("10.0.64.0/19")
                    .build(),
                Parameter::builder()
                    .parameter_key("PublicSubnetCidr2")
                    .parameter_value("10.0.128.0/19")
                    .build(),
                Parameter::builder()
                    .parameter_key("PublicSubnetCidr3")
                    .parameter_value("10.0.192.0/19")
                    .build(),
                Parameter::builder()
                    .parameter_key("IngressIpv4Range")
                    .parameter_value("0.0.0.0/0")
                    .build(),
                Parameter::builder()
                    .parameter_key("HttpPort")
                    .parameter_value("9650")
                    .build(),
                Parameter::builder()
                    .parameter_key("StakingPort")
                    .parameter_value("9651")
                    .build(),
            ])),
        )
        .await
        .unwrap();
    assert_eq!(stack.name, stack_name);
    assert_eq!(stack.status, StackStatus::CreateInProgress);
    let stack = cloudformation_manager
        .poll_stack(
            &stack_name,
            StackStatus::CreateComplete,
            Duration::from_secs(500),
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert_eq!(stack.name, stack_name);
    assert_eq!(stack.status, StackStatus::CreateComplete);
    let outputs = stack.outputs.unwrap();
    for o in outputs {
        log::info!("output {:?} {:?}", o.output_key, o.output_value)
    }

    sleep(Duration::from_secs(5)).await;

    cloudformation_manager
        .delete_stack(&stack_name)
        .await
        .unwrap();
    let stack = cloudformation_manager
        .poll_stack(
            &stack_name,
            StackStatus::DeleteComplete,
            Duration::from_secs(500),
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert_eq!(stack.name, stack_name);
    assert_eq!(stack.status, StackStatus::DeleteComplete);
}
