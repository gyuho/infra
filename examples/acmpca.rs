use std::{
    collections::HashMap,
    env::args,
    fs::{self, File},
    io::Write,
    process::{Command, Stdio},
};

use aws_manager::{self, acmpca};
use tokio::time::{sleep, Duration};

/// cargo run --example acmpca --features="acmpca random-manager" -- [ORG] [COMMON NAME]
/// cargo run --example acmpca --features="acmpca random-manager" -- "myorg" "hello.com"
///
/// 1. create ROOT CA
/// 2. get ROOT CA CSR
/// 3. issue new cert from the ROOT CA, with the CSR
/// 4. import the newly issued cert to the ROOT CA
/// 5. ROOT CA is now "ACTIVE"
///
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(Some(String::from("us-west-2")), None).await;
    log::info!("region {:?}", shared_config.region().unwrap());
    let acmpca_manager = acmpca::Manager::new(&shared_config);

    let name = random_manager::secure_string(10);
    let mut tags = HashMap::new();
    tags.insert("Name".to_string(), name);
    tags.insert("Kind".to_string(), "aws-manager".to_string());

    let org = args().nth(1).expect("no org given");
    let common_name = args().nth(2).expect("no common name given");
    log::info!("org {org}, common name {common_name}");

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    let root_ca_arn = acmpca_manager
        .create_root_ca(&org, &common_name, Some(tags))
        .await
        .unwrap();

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    sleep(Duration::from_secs(3)).await;
    let ca_csr = acmpca_manager.get_ca_csr(&root_ca_arn).await.unwrap();
    log::info!("ca_csr:\n\n{ca_csr}\n");

    let root_ca_csr_path = random_manager::tmp_path(10, Some(".csr")).unwrap();
    let mut root_ca_csr_file = File::create(&root_ca_csr_path).unwrap();
    root_ca_csr_file.write_all(ca_csr.as_bytes()).unwrap();
    log::info!("wrote CSR to {root_ca_csr_path}");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let openssl_args = vec![
        "req".to_string(),
        "-text".to_string(),
        "-noout".to_string(),
        "-verify".to_string(),
        "-in".to_string(),
        root_ca_csr_path.to_string(),
    ];
    let openssl_cmd = Command::new("openssl")
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .args(openssl_args)
        .spawn()
        .unwrap();
    log::info!("ran openssl req with PID {}", openssl_cmd.id());
    let res = openssl_cmd.wait_with_output();
    match res {
        Ok(output) => {
            println!(
                "openssl output {} bytes:\n{}\n",
                output.stdout.len(),
                String::from_utf8(output.stdout).unwrap()
            )
        }
        Err(e) => {
            log::warn!("failed to run openssl {}", e)
        }
    }

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    sleep(Duration::from_secs(3)).await;
    let issued_cert_arn = acmpca_manager
        .issue_cert_from_root_ca(&root_ca_arn, 365, aws_smithy_types::Blob::new(ca_csr))
        .await
        .unwrap();

    sleep(Duration::from_secs(3)).await;
    let issued_cert_pem = acmpca_manager
        .get_cert_pem(&root_ca_arn, &issued_cert_arn)
        .await
        .unwrap();
    log::info!("issued_cert_pem:\n\n{issued_cert_pem}\n");

    sleep(Duration::from_secs(3)).await;
    match acmpca_manager.get_ca_cert_pem(&root_ca_arn).await {
        Ok(_) => {}
        Err(e) => {
            log::warn!("CA cert not imported yet: {:?}", e);
        }
    }

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let issued_cert_pem_path = random_manager::tmp_path(10, Some(".csr")).unwrap();
    let mut issued_cert_pem_file = File::create(&issued_cert_pem_path).unwrap();
    issued_cert_pem_file
        .write_all(issued_cert_pem.as_bytes())
        .unwrap();
    log::info!("wrote issued cert PEM to {issued_cert_pem_path}");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let openssl_args = vec![
        "x509".to_string(),
        "-text".to_string(),
        "-noout".to_string(),
        "-in".to_string(),
        issued_cert_pem_path.to_string(),
    ];
    let openssl_cmd = Command::new("openssl")
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .args(openssl_args)
        .spawn()
        .unwrap();
    log::info!("ran openssl x509 with PID {}", openssl_cmd.id());
    let res = openssl_cmd.wait_with_output();
    match res {
        Ok(output) => {
            println!(
                "openssl output {} bytes:\n{}\n",
                output.stdout.len(),
                String::from_utf8(output.stdout).unwrap()
            )
        }
        Err(e) => {
            log::warn!("failed to run openssl {}", e)
        }
    }

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    sleep(Duration::from_secs(3)).await;
    acmpca_manager
        .import_cert(&issued_cert_pem_path, &root_ca_arn)
        .await
        .unwrap();

    sleep(Duration::from_secs(3)).await;
    let described = acmpca_manager.describe_ca(&root_ca_arn).await.unwrap();
    log::info!("described CA: {:?}", described);
    assert_eq!(described.status().unwrap().as_str(), "ACTIVE");

    sleep(Duration::from_secs(3)).await;
    let ca_cert = acmpca_manager.get_ca_cert_pem(&root_ca_arn).await.unwrap();
    log::info!("CA cert:\n\n{ca_cert}\n");

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    // The following will fail since the self-signed cert cannot be revoked...
    sleep(Duration::from_secs(3)).await;
    match acmpca_manager.disable_ca(&root_ca_arn).await {
        Ok(_) => {}
        Err(e) => log::warn!("failed disable_ca {:?}", e),
    };

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    sleep(Duration::from_secs(3)).await;
    acmpca_manager.delete_ca(&root_ca_arn).await.unwrap();
    sleep(Duration::from_secs(3)).await;
    acmpca_manager.delete_ca(&root_ca_arn).await.unwrap();

    //
    //
    //
    //
    //
    //
    //
    //
    //
    println!();
    println!();
    println!();
    log::info!("removing test files");
    fs::remove_file(root_ca_csr_path).unwrap();
    fs::remove_file(issued_cert_pem_path).unwrap();
}
