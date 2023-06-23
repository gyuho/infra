use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use aws_manager::ec2;

/// cargo run --example ec2_plugins --features="ec2"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let (plugins, contents) = ec2::plugins::create(
        ec2::ArchType::Amd64,
        ec2::OsType::Ubuntu2004,
        vec![
            "imds".to_string(),
            "provider-id".to_string(),
            "system-limit-bump".to_string(),
            "time-sync".to_string(),
            "aws-cli".to_string(),
            "ssm-agent".to_string(),
            "cloudwatch-agent".to_string(),
            "static-volume-provisioner".to_string(),
            "static-ip-provisioner".to_string(),
            "anaconda".to_string(),
            "python".to_string(),
            "rust".to_string(),
            "go".to_string(),
            "docker".to_string(),
            "post-init-script".to_string(),
        ],
        false,
        "s3_bucket",
        "id",
        "us-west-2",
        "gp3",
        120,
        3000,
        1000,
        None,
        None,
        None,
        Some(String::from(
            "

echo 123

",
        )),
    )
    .unwrap();

    for p in plugins {
        println!("plugin: {}", p.as_str());
    }

    println!("contents:\n{contents}");
    let fp = Path::new("/tmp/init.bash");
    let parent_dir = fp.parent().unwrap();
    fs::create_dir_all(parent_dir).unwrap();
    let mut f = File::create(fp).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    log::info!("wrote init bash script '{}'", fp.display());
}
