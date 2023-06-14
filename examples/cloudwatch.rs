use aws_manager::cloudwatch;
use tokio::time::{sleep, Duration};

/// cargo run --example cloudwatch --features="cloudwatch"
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
    let shared_config = aws_manager::load_config(Some(String::from("us-east-1")), None, None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let cw_manager = cloudwatch::Manager::new(&shared_config);
    let log_group_name = random_manager::secure_string(15);

    // error should be ignored if it does not exist
    cw_manager.delete_log_group("invalid_id").await.unwrap();

    cw_manager.create_log_group(&log_group_name).await.unwrap();

    sleep(Duration::from_secs(5)).await;

    cw_manager.delete_log_group(&log_group_name).await.unwrap();

    sleep(Duration::from_secs(5)).await;

    // error should be ignored if it's already scheduled for delete
    cw_manager.delete_log_group(&log_group_name).await.unwrap();
}
