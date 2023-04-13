use aws_manager::{self, sts};

/// cargo run --example sts --features="sts"
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
    let shared_config = aws_manager::load_config(Some(String::from("us-east-1")), None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let sts_manager = sts::Manager::new(&shared_config);
    let identity1 = sts_manager.get_identity().await.unwrap();
    log::info!("STS identity1: {:?}", identity1);

    let shared_config = aws_manager::load_config(Some(String::from("us-east-1")), None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let sts_manager = sts::Manager::new(&shared_config);
    let identity2 = sts_manager.get_identity().await.unwrap();
    log::info!("STS identity2: {:?}", identity2);

    assert_eq!(identity1, identity2);
}
