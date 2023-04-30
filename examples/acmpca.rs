use std::{
    collections::HashMap,
    env::args,
    fs::{self, File},
    io::Write,
    process::{Command, Stdio},
};

use aws_manager::{self, acm, acmpca};
use rcgen::{DistinguishedName, DnType};
use tokio::time::{sleep, Duration};

/// cargo run --example acmpca --features="acm acmpca random-manager" -- [ORG] [COMMON NAME]
/// cargo run --example acmpca --features="acm acmpca random-manager" -- "myorg" "hello.com"
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
    let acm_manager = acm::Manager::new(&shared_config);
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
    println!("STEP 1. creating a root CA for the following CSRs");
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
    println!("STEP 2. getting the root CSR to import");
    let root_ca_csr_to_import = acmpca_manager.get_ca_csr(&root_ca_arn).await.unwrap();
    log::info!("root_ca_csr_to_import:\n\n{root_ca_csr_to_import}\n");

    let root_ca_csr_to_import_path = random_manager::tmp_path(10, Some(".csr")).unwrap();
    {
        let mut file = File::create(&root_ca_csr_to_import_path).unwrap();
        file.write_all(root_ca_csr_to_import.as_bytes()).unwrap();
    }
    log::info!("wrote CSR to {root_ca_csr_to_import_path}");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let openssl_args = vec![
        "req".to_string(),
        "-text".to_string(),
        "-noout".to_string(),
        "-verify".to_string(),
        "-in".to_string(),
        root_ca_csr_to_import_path.to_string(),
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
    println!("STEP 3. issuing the self-signed cert from the root CA and CSR");
    let issued_self_signed_cert_arn = acmpca_manager
        .issue_self_signed_cert_from_root_ca(
            &root_ca_arn,
            365,
            aws_smithy_types::Blob::new(root_ca_csr_to_import),
        )
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
    println!("STEP 4. fetching the newly issued the self-signed cert PEM");
    let issued_self_signed_cert_pem = acmpca_manager
        .get_cert_pem(&root_ca_arn, &issued_self_signed_cert_arn)
        .await
        .unwrap();
    log::info!("issued_self_signed_cert_pem:\n\n{issued_self_signed_cert_pem}\n");

    sleep(Duration::from_secs(3)).await;
    match acmpca_manager.get_ca_cert_pem(&root_ca_arn).await {
        Ok(_) => {}
        Err(e) => {
            log::warn!("CA cert not imported yet: {:?}", e);
        }
    }

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let issued_self_signed_cert_pem_path = random_manager::tmp_path(10, Some(".csr")).unwrap();
    {
        let mut file = File::create(&issued_self_signed_cert_pem_path).unwrap();
        file.write_all(issued_self_signed_cert_pem.as_bytes())
            .unwrap();
    }
    log::info!("wrote issued cert PEM to {issued_self_signed_cert_pem_path}");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let openssl_args = vec![
        "x509".to_string(),
        "-text".to_string(),
        "-noout".to_string(),
        "-in".to_string(),
        issued_self_signed_cert_pem_path.to_string(),
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
    println!("STEP 5. importing the self-signed cert PEM to the root CA");
    acmpca_manager
        .import_cert(&issued_self_signed_cert_pem_path, &root_ca_arn)
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
    println!("STEP 6. making sure the root CA is now ACTIVE");
    sleep(Duration::from_secs(3)).await;
    let described = acmpca_manager.describe_ca(&root_ca_arn).await.unwrap();
    log::info!("described CA: {:?}", described);
    assert_eq!(described.status().unwrap().as_str(), "ACTIVE");

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
    println!("STEP 7. getting the root CA PEM");
    let root_ca_cert = acmpca_manager.get_ca_cert_pem(&root_ca_arn).await.unwrap();
    log::info!("Root CA cert:\n\n{root_ca_cert}\n");

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
    println!("STEP 8. requesting a private certificate");
    let requested_private_cert_arn = acm_manager
        .request_private_cert(&common_name, &root_ca_arn)
        .await
        .unwrap();

    // ONLY WORKS IF YOU CAN PASS THE VALIDATION (e.g., DNS, email)
    // ref. <https://docs.aws.amazon.com/acm/latest/APIReference/API_RequestCertificate.html#ACM-RequestCertificate-request-ValidationMethod>
    if 1 == 10 {
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
        println!("STEP 9. exporting the private certificate");
        let exported_private_cert = acm_manager
            .export_private_cert(
                &requested_private_cert_arn,
                aws_smithy_types::Blob::new("1234"),
            )
            .await
            .unwrap();
        let request_private_cert_key_path = random_manager::tmp_path(10, Some(".key")).unwrap();
        {
            let mut file = File::create(&request_private_cert_key_path).unwrap();
            file.write_all(
                exported_private_cert
                    .private_key()
                    .clone()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        }
        log::info!("wrote requested private cert key file {request_private_cert_key_path}");
        let request_private_cert_cert_path = random_manager::tmp_path(10, Some(".cert")).unwrap();
        {
            let mut file = File::create(&request_private_cert_cert_path).unwrap();
            file.write_all(
                exported_private_cert
                    .certificate()
                    .clone()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        }
        log::info!("wrote requested private cert cert file {request_private_cert_cert_path}");

        fs::remove_file(request_private_cert_key_path).unwrap();
        fs::remove_file(request_private_cert_cert_path).unwrap();
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
    println!("STEP 10. writing another CSR");
    let mut csr_params =
        cert_manager::x509::default_params(Some(common_name.clone()), false).unwrap();
    csr_params.distinguished_name = DistinguishedName::new();
    csr_params
        .distinguished_name
        .push(DnType::CountryName, "US");
    csr_params
        .distinguished_name
        .push(DnType::StateOrProvinceName, "NY");
    csr_params
        .distinguished_name
        .push(DnType::OrganizationName, org);
    csr_params
        .distinguished_name
        .push(DnType::CommonName, common_name.clone());

    let csr_entity_to_import =
        cert_manager::x509::CsrEntity::new_with_parameters(csr_params).unwrap();
    let csr_to_import = csr_entity_to_import.csr_pem.clone();

    log::info!(
        "csr_entity_to_import:\n\n{}\n",
        csr_entity_to_import.csr_pem
    );
    let (csr_key_path, csr_cert_path, csr_to_import_path) =
        csr_entity_to_import.save(true, None, None, None).unwrap();
    log::info!("csr_key_path: {csr_key_path}");
    log::info!("csr_cert_path: {csr_cert_path}");
    log::info!("csr_to_import_path: {csr_to_import_path}");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let openssl_args = vec![
        "req".to_string(),
        "-text".to_string(),
        "-noout".to_string(),
        "-verify".to_string(),
        "-in".to_string(),
        csr_to_import_path.to_string(),
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
    println!("STEP 11. issuing a new end cert from root CA and CSR");
    let issued_end_cert_arn = acmpca_manager
        .issue_end_cert_from_root_ca(
            &root_ca_arn,
            180,
            aws_smithy_types::Blob::new(csr_to_import),
        )
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
    sleep(Duration::from_secs(3)).await;
    println!("STEP 12. fetching the newly issued the end cert PEM");
    let issued_end_cert_pem = acmpca_manager
        .get_cert_pem(&root_ca_arn, &issued_end_cert_arn)
        .await
        .unwrap();
    log::info!("issued_end_cert_pem:\n\n{issued_end_cert_pem}\n");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let issued_end_cert_pem_path = random_manager::tmp_path(10, Some(".csr")).unwrap();
    {
        let mut file = File::create(&issued_end_cert_pem_path).unwrap();
        file.write_all(issued_end_cert_pem.as_bytes()).unwrap();
    }
    log::info!("wrote issued cert PEM to {issued_end_cert_pem_path}");

    // ref. <https://docs.aws.amazon.com/privateca/latest/userguide/PCACertInstall.html#InstallRoot>
    let openssl_args = vec![
        "x509".to_string(),
        "-text".to_string(),
        "-noout".to_string(),
        "-in".to_string(),
        issued_end_cert_pem_path.to_string(),
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
    println!("STEP 13. importing the end cert PEM to the root CA and expect failures");
    match acmpca_manager
        .import_cert(&issued_end_cert_pem_path, &root_ca_arn)
        .await
    {
        Ok(_) => {}
        Err(e) => {
            log::info!("as expected, it failed {:?}", e)
        }
    }

    sleep(Duration::from_secs(3)).await;
    let described = acmpca_manager.describe_ca(&root_ca_arn).await.unwrap();
    log::info!("described CA: {:?}", described);
    assert_eq!(described.status().unwrap().as_str(), "ACTIVE");

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
    println!("STEP 14. getting the root CA PEM");
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
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
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
    sleep(Duration::from_secs(250)).await;
    println!("STEP 15. deleting the private certificate");
    acm_manager
        .delete_cert(&requested_private_cert_arn)
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
    // The following will fail since the self-signed cert cannot be revoked...
    sleep(Duration::from_secs(3)).await;
    println!("STEP 16. disabling root CA");
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
    println!("STEP 16. deleting root CA");
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
    log::info!("STEP 17. removing test files");
    fs::remove_file(root_ca_csr_to_import_path).unwrap();
    fs::remove_file(issued_self_signed_cert_pem_path).unwrap();
    fs::remove_file(csr_key_path).unwrap();
    fs::remove_file(csr_cert_path).unwrap();
    fs::remove_file(csr_to_import_path).unwrap();
}
