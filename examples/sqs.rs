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

    let queue_name = format!("{}.fifo", random_manager::secure_string(10));
    let queue_url = sqs_manager.create_fifo(&queue_name, 30, 1).await.unwrap();

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

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    let attr = sqs_manager.get_attributes(&queue_url).await.unwrap();
    for (k, v) in attr.iter() {
        log::info!("attribute '{:?}' = '{}'", k, v);
    }

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    let mut receipt_handles = Vec::new();
    let msgs = sqs_manager.recv_msgs(&queue_url, 10, 3).await.unwrap();
    for msg in &msgs {
        log::info!("message {:?}", msg);
        receipt_handles.push(msg.receipt_handle().unwrap().to_string());
    }

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    let attr = sqs_manager.get_attributes(&queue_url).await.unwrap();
    for (k, v) in attr.iter() {
        log::info!("attribute '{:?}' = '{}'", k, v);
    }

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    for receipt_handle in &receipt_handles {
        sqs_manager
            .delete_msg(&queue_url, &receipt_handle)
            .await
            .unwrap();

        // second delete should succeed if not exists
        sqs_manager
            .delete_msg(&queue_url, &receipt_handle)
            .await
            .unwrap();
    }

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    let attr = sqs_manager.get_attributes(&queue_url).await.unwrap();
    for (k, v) in attr.iter() {
        log::info!("attribute '{:?}' = '{}'", k, v);
    }

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    sqs_manager.delete(&queue_url).await.unwrap();

    println!();
    println!();
    sleep(Duration::from_secs(5)).await;
    sqs_manager.delete(&queue_url).await.unwrap();
}
