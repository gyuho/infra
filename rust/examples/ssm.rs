use aws_manager::{self, ssm};

/// cargo run --example ssm --features="ssm"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(Some(String::from("us-west-2")), None, None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let ssm_manager = ssm::Manager::new(&shared_config);

    let ami = ssm_manager
        .fetch_ami(
            "/aws/service/canonical/ubuntu/eks/20.04/1.26/stable/current/amd64/hvm/ebs-gp2/ami-id",
        )
        .await
        .unwrap();
    println!("ami: {:?}", ami);
}
