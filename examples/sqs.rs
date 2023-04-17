use aws_manager::{self, sqs};
use tokio::time::{sleep, Duration};

/// cargo run --example sqs --features="sqs random-manager"
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
    let queue_url = sqs_manager.create_fifo(&name, 30).await.unwrap();

    sleep(Duration::from_secs(10)).await;
    let msg_group_id = random_manager::secure_string(32);
    for _ in 0..3 {
        let msg_body = random_manager::secure_string(100);
        let msg_dedup_id = random_manager::secure_string(32);
        let _ = sqs_manager
            .send_msg_to_fifo(
                &queue_url,
                &msg_group_id,
                Some(msg_dedup_id),
                None,
                &msg_body,
            )
            .await
            .unwrap();
    }

    sleep(Duration::from_secs(5)).await;
    let msgs = sqs_manager.recv_msgs(&queue_url, 10, 3).await.unwrap();
    for msg in &msgs {
        log::info!("message {:?}", msg);
    }

    sleep(Duration::from_secs(5)).await;
    sqs_manager.delete(&queue_url).await.unwrap();

    sleep(Duration::from_secs(5)).await;
    sqs_manager.delete(&queue_url).await.unwrap();
}
