use std::{collections::HashMap, fs, io::Write};

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
    let s3_bucket = format!(
        "aws-manager-example-{}",
        random_manager::secure_string(10).to_lowercase()
    );
    s3_manager.delete_bucket(&s3_bucket).await.unwrap(); // error should be ignored if it does not exist

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.create_bucket(&s3_bucket).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.create_bucket(&s3_bucket).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    let mut days_to_pfxs = HashMap::new();
    days_to_pfxs.insert(3, vec!["sub-dir-3day/".to_string()]);
    days_to_pfxs.insert(10, vec!["sub-dir-10day/".to_string()]);
    s3_manager
        .put_bucket_object_expire_configuration(&s3_bucket, days_to_pfxs)
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
    let mut metadata = HashMap::new();
    let request_id = random_manager::secure_string(300);
    metadata.insert("x-amz-meta-request-id".to_string(), request_id.clone());
    s3_manager
        .put_object_with_metadata(&upload_path, &s3_bucket, &s3_key, Some(metadata))
        .await
        .unwrap();
    let (head_object, exists) = s3_manager.exists(&s3_bucket, &s3_key).await.unwrap();
    assert!(exists);
    assert!(head_object.is_some());
    println!("head object: {:?}", head_object.clone().unwrap());
    assert!(head_object
        .clone()
        .unwrap()
        .metadata()
        .unwrap()
        .contains_key("x-amz-meta-request-id"));
    assert_eq!(
        head_object
            .clone()
            .unwrap()
            .metadata()
            .unwrap()
            .get("x-amz-meta-request-id")
            .unwrap()
            .to_string(),
        request_id
    );
    let exists = s3_manager
        .get_object(
            &s3_bucket,
            &random_manager::secure_string(10),
            &random_manager::secure_string(10),
        )
        .await
        .unwrap();
    assert!(!exists);

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    let download_path = random_manager::tmp_path(10, None).unwrap();
    s3_manager
        .get_object(&s3_bucket, &s3_key, &download_path)
        .await
        .unwrap();
    let download_contents = fs::read(&download_path).unwrap();
    assert_eq!(contents.to_vec().len(), download_contents.len());
    assert_eq!(contents.to_vec(), download_contents);
    assert!(s3_manager
        .download_executable_with_retries(&s3_bucket, &s3_key, &download_path, false,)
        .await
        .unwrap());
    assert!(s3_manager
        .download_executable_with_retries(&s3_bucket, &s3_key, &download_path, true,)
        .await
        .unwrap());

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    let objects = s3_manager
        .list_objects(&s3_bucket, Some(String::from("sub-dir/").as_str()))
        .await
        .unwrap();
    for obj in objects.iter() {
        log::info!("object: {}", obj.key().unwrap());
    }

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_objects(&s3_bucket, None).await.unwrap();

    println!();
    println!();
    println!();
    sleep(Duration::from_secs(2)).await;
    s3_manager.delete_bucket(&s3_bucket).await.unwrap();
}
