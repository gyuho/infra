use std::{
    fs::{self, File},
    io::Write,
};

use aws_manager::{
    self,
    ec2::{self, ssl::ssh},
};
use tokio::time::{sleep, Duration};

/// cargo run --example ec2_key_pair_import_openssl --features="ec2 ec2-openssl"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let (encoded_pk, encoded_pubkey) = ssh::new_rsa_key().unwrap();
    log::info!("encoded private key: {encoded_pk}");
    log::info!("encoded public key: {encoded_pubkey}");

    let f = tempfile::NamedTempFile::new().unwrap();
    let encoded_pubkey_path = f.path().to_str().unwrap();
    fs::remove_file(encoded_pubkey_path).unwrap();
    log::info!("created public key path {}", encoded_pubkey_path);

    let mut f = File::create(&encoded_pubkey_path).unwrap();
    f.write_all(encoded_pubkey.as_bytes()).unwrap();

    let shared_config = aws_manager::load_config(None, None, None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let ec2_manager = ec2::Manager::new(&shared_config);

    let mut key_name = id_manager::time::with_prefix("test");
    key_name.push_str("-key");

    // error should be ignored if it does not exist
    ec2_manager.delete_key_pair(&key_name).await.unwrap();

    sleep(Duration::from_secs(2)).await;

    let _key_pair_id = ec2_manager
        .import_key(&key_name, &encoded_pubkey_path)
        .await
        .unwrap();

    sleep(Duration::from_secs(2)).await;

    ec2_manager.delete_key_pair(&key_name).await.unwrap();
}
