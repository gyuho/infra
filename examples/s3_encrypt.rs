use std::{
    fs::File,
    io::{Read, Write},
};

use aws_manager::{
    self,
    kms::{self, envelope::Manager},
    s3,
};
use tokio::time::{sleep, Duration};

/// cargo run --example s3_encrypt --features="s3,kms"
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
    let s3_manager = s3::Manager::new(&shared_config);
    let kms_manager = kms::Manager::new(&shared_config);

    println!();
    println!();
    println!();
    let cmk = kms_manager
        .create_symmetric_default_key("test key description")
        .await
        .unwrap();
    let envelope_manager = Manager::new(
        &kms_manager,
        cmk.id.clone(),
        "test-aad-tag".to_string(), // AAD tag
    );

    println!();
    println!();
    println!();
    let s3_bucket = format!(
        "aws-manager-examples-tests-s3-encrypt-{}",
        random_manager::secure_string(10).to_lowercase()
    );
    let s3_key = "sub-dir/aaa.zstd.encrypted".to_string();
    s3_manager.create_bucket(&s3_bucket).await.unwrap();

    println!();
    println!();
    println!();
    let contents = vec![7; 50 * 1024 * 1024];
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&contents.to_vec()).unwrap();
    let src_file_path = file.path().to_str().unwrap().to_string();
    let dst_file_path = random_manager::tmp_path(10, None).unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    envelope_manager
        .compress_seal_put_object(&s3_manager, &src_file_path, &s3_bucket, &s3_key)
        .await
        .unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    envelope_manager
        .get_object_unseal_decompress(&s3_manager, &s3_bucket, &s3_key, &dst_file_path)
        .await
        .unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_objects(&s3_bucket, None).await.unwrap();
    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_bucket(&s3_bucket).await.unwrap();
    kms_manager.schedule_to_delete(&cmk.id, 7).await.unwrap();

    println!();
    println!();
    println!();
    let mut src_file = File::open(src_file_path).unwrap();
    let mut src_file_contents = Vec::new();
    src_file.read_to_end(&mut src_file_contents).unwrap();
    let mut dst_file = File::open(dst_file_path).unwrap();
    let mut dst_file_contents = Vec::new();
    dst_file.read_to_end(&mut dst_file_contents).unwrap();
    assert!(cmp_manager::eq_vectors(
        &src_file_contents,
        &dst_file_contents
    ));
}
