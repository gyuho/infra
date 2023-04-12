use std::collections::HashMap;

use aws_manager::{self, acmpca};
use tokio::time::{sleep, Duration};

/// cargo run --example acmpca --features="acmpca"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(Some(String::from("us-west-2")), None)
        .await
        .unwrap();
    log::info!("region {:?}", shared_config.region().unwrap());
    let acmpca_manager = acmpca::Manager::new(&shared_config);

    sleep(Duration::from_secs(5)).await;

    let name = random_manager::secure_string(10);
    let mut tags = HashMap::new();
    tags.insert("Name".to_string(), name);
    tags.insert("Kind".to_string(), "aws-manager".to_string());

    let org = random_manager::secure_string(10);
    let common_name = random_manager::secure_string(10);
    let ca_arn = acmpca_manager
        .create_root_ca(&org, &common_name, Some(tags))
        .await
        .unwrap();

    sleep(Duration::from_secs(5)).await;

    let described = acmpca_manager.describe_ca(&ca_arn).await.unwrap();
    log::info!("described CA: {:?}", described);

    sleep(Duration::from_secs(5)).await;

    acmpca_manager.delete_ca(&ca_arn).await.unwrap();
}
