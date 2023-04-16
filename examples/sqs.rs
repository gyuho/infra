use aws_manager::{self, sqs};
use tokio::time::{sleep, Duration};

/// cargo run --example sqs --features="sqs"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(Some(String::from("us-west-2")), None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let sqs_manager = sqs::Manager::new(&shared_config);

    let name = format!("{}.fifo", random_manager::secure_string(10));
    let queue_arn = sqs_manager.create_fifo(&name).await.unwrap();

    sleep(Duration::from_secs(60)).await;

    sqs_manager.delete(&queue_arn).await.unwrap();
    sqs_manager.delete(&queue_arn).await.unwrap();
}
