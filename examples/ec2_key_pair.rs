use std::fs;

use aws_manager::{self, ec2};
use tokio::time::{sleep, Duration};

/// cargo run --example ec2_key_pair --features="ec2"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(None, None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let ec2_manager = ec2::Manager::new(&shared_config);

    let mut key_name = id_manager::time::with_prefix("test");
    key_name.push_str("-key");

    // error should be ignored if it does not exist
    ec2_manager.delete_key_pair(&key_name).await.unwrap();

    let f = tempfile::NamedTempFile::new().unwrap();
    let key_path = f.path().to_str().unwrap();
    fs::remove_file(key_path).unwrap();
    log::info!("created file path {}", key_path);

    ec2_manager
        .create_key_pair(&key_name, key_path)
        .await
        .unwrap();

    sleep(Duration::from_secs(2)).await;

    ec2_manager.delete_key_pair(&key_name).await.unwrap();
}
