use std::{
    fs::File,
    io::{Read, Write},
    sync::Arc,
    thread, time,
};

use aws_manager::{
    self,
    kms::{self, envelope::Manager},
    s3,
};
use log::info;

/// cargo run --example s3_encrypt
fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    macro_rules! ab {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    info!("creating AWS resources!");
    let shared_config = ab!(aws_manager::load_config(None)).unwrap();

    println!();
    println!();
    println!();
    let kms_manager = kms::Manager::new(&shared_config);
    let cmk = ab!(kms_manager.create_symmetric_default_key("test key description")).unwrap();
    let envelope_manager = Manager::new(
        kms_manager.clone(),
        cmk.id.clone(),
        "test-aad-tag".to_string(), // AAD tag
    );

    println!();
    println!();
    let s3_manager = s3::Manager::new(&shared_config);
    let s3_bucket = format!(
        "aws-manager-examples-tests-s3-encrypt-{}",
        random_manager::string(10).to_lowercase()
    );
    let s3_key = "sub-dir/aaa.zstd.encrypted".to_string();
    ab!(s3_manager.create_bucket(&s3_bucket)).unwrap();

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
    thread::sleep(time::Duration::from_secs(3));
    ab!(s3::spawn_compress_seal_put_object(
        s3_manager.clone(),
        envelope_manager.clone(),
        &src_file_path,
        &s3_bucket,
        &s3_key,
    ))
    .unwrap();

    println!();
    println!();
    println!();
    thread::sleep(time::Duration::from_secs(3));
    ab!(s3::spawn_get_object_unseal_decompress(
        s3_manager.clone(),
        envelope_manager.clone(),
        &s3_bucket,
        &s3_key,
        &dst_file_path,
    ))
    .unwrap();

    println!();
    println!();
    println!();
    thread::sleep(time::Duration::from_secs(3));
    ab!(s3_manager.delete_objects(Arc::new(s3_bucket.clone()), None)).unwrap();
    thread::sleep(time::Duration::from_secs(3));
    ab!(s3_manager.delete_bucket(&s3_bucket)).unwrap();
    ab!(kms_manager.schedule_to_delete(&cmk.id, 7)).unwrap();

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
