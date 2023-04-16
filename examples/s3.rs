use std::{fs, io::Write};

use aws_manager::{self, s3};
use tokio::time::{sleep, Duration};

/// cargo run --example s3 --features="s3"
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

    println!();
    println!();
    println!();
    let bucket = format!(
        "aws-manager-examples-tests-{}",
        random_manager::secure_string(5).to_lowercase()
    );
    s3_manager.delete_bucket(&bucket).await.unwrap(); // error should be ignored if it does not exist

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.create_bucket(&bucket).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.create_bucket(&bucket).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager
        .put_bucket_object_expire_configuration(&bucket, 3, vec!["sub-dir/".to_string()])
        .await
        .unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    let contents = vec![7; 50 * 1024 * 1024];
    let mut upload_file = tempfile::NamedTempFile::new().unwrap();
    upload_file.write_all(&contents.to_vec()).unwrap();
    let upload_path = upload_file.path().to_str().unwrap().to_string();
    let s3_key = "sub-dir/aaa.txt".to_string();
    s3_manager
        .put_object(&upload_path, &bucket, &s3_key)
        .await
        .unwrap();
    s3_manager.exists(&bucket, &s3_key).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    let download_path = random_manager::tmp_path(10, None).unwrap();
    s3_manager
        .get_object(&bucket, &s3_key, &download_path)
        .await
        .unwrap();
    let download_contents = fs::read(download_path).unwrap();
    assert_eq!(contents.to_vec().len(), download_contents.len());
    assert_eq!(contents.to_vec(), download_contents);

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    let objects = s3_manager
        .list_objects(&bucket, Some(String::from("sub-dir/").as_str()))
        .await
        .unwrap();
    for obj in objects.iter() {
        log::info!("object: {}", obj.key().unwrap());
    }

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_objects(&bucket, None).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_bucket(&bucket).await.unwrap();
}
