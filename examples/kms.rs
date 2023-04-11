use std::{
    fs::File,
    io::{Read, Write},
};

use aws_manager::{
    self,
    kms::{self, envelope::Manager},
    sts,
};
use tokio::time::{sleep, Duration};

/// cargo run --example kms --features="kms,sts"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let shared_config = aws_manager::load_config(Some(String::from("us-east-1")), None)
        .await
        .unwrap();
    log::info!("region {:?}", shared_config.region().unwrap());
    let sts_manager = sts::Manager::new(&shared_config);
    let identity = sts_manager.get_identity().await.unwrap();
    log::info!("STS identity: {:?}", identity);

    let kms_manager = kms::Manager::new(&shared_config);

    let mut key_desc = id_manager::time::with_prefix("test");
    key_desc.push_str("-cmk");

    // error should be ignored if it does not exist
    kms_manager
        .schedule_to_delete("invalid_id", 7)
        .await
        .unwrap();

    let encrypt_key = kms_manager
        .create_symmetric_default_key(&key_desc)
        .await
        .unwrap();

    let (grant_id, _grant_token) = kms_manager
        .create_grant_for_encrypt_decrypt(&encrypt_key.id, &identity.role_arn)
        .await
        .unwrap();

    let dek = kms_manager
        .generate_data_key(&encrypt_key.id, None)
        .await
        .unwrap();

    let dek_ciphertext_decrypted = kms_manager
        .decrypt(&encrypt_key.id, None, dek.ciphertext)
        .await
        .unwrap();
    assert_eq!(dek.plaintext, dek_ciphertext_decrypted);

    let dek_plaintext_encrypted = kms_manager
        .encrypt(&encrypt_key.id, None, dek.plaintext.clone())
        .await
        .unwrap();
    let dek_plaintext_encrypted_decrypted = kms_manager
        .decrypt(&encrypt_key.id, None, dek_plaintext_encrypted)
        .await
        .unwrap();
    assert_eq!(dek.plaintext, dek_plaintext_encrypted_decrypted);
    assert_eq!(dek_ciphertext_decrypted, dek_plaintext_encrypted_decrypted);

    let plaintext = "Hello World!";
    let mut plaintext_file = tempfile::NamedTempFile::new().unwrap();
    plaintext_file.write_all(plaintext.as_bytes()).unwrap();
    let plaintext_file_path = plaintext_file.path().to_str().unwrap();

    let encrypted_file_path = random_manager::tmp_path(10, Some(".encrypted")).unwrap();
    let decrypted_file_path = random_manager::tmp_path(10, Some(".encrypted")).unwrap();

    kms_manager
        .encrypt_file(
            &encrypt_key.id,
            None,
            plaintext_file_path,
            &encrypted_file_path,
        )
        .await
        .unwrap();
    kms_manager
        .decrypt_file(
            &encrypt_key.id,
            None,
            &encrypted_file_path,
            &decrypted_file_path,
        )
        .await
        .unwrap();

    let mut encrypted_file = File::open(encrypted_file_path).unwrap();
    let mut encrypted_file_contents = Vec::new();
    encrypted_file
        .read_to_end(&mut encrypted_file_contents)
        .unwrap();
    let mut decrypted_file = File::open(decrypted_file_path).unwrap();
    let mut decrypted_file_contents = Vec::new();
    decrypted_file
        .read_to_end(&mut decrypted_file_contents)
        .unwrap();
    log::info!("encrypted_file_contents: {:?}", encrypted_file_contents);
    log::info!("decrypted_file_contents: {:?}", decrypted_file_contents);
    assert_eq!(&decrypted_file_contents, plaintext.as_bytes());
    assert!(cmp_manager::eq_vectors(
        &decrypted_file_contents,
        plaintext.as_bytes()
    ));

    let envelope_manager = Manager::new(
        &kms_manager,
        encrypt_key.id.clone(),
        "test-aad-tag".to_string(), // AAD tag
    );
    let sealed_aes_256_file_path = random_manager::tmp_path(10, Some(".encrypted")).unwrap();
    let unsealed_aes_256_file_path = random_manager::tmp_path(10, None).unwrap();
    envelope_manager
        .seal_aes_256_file(plaintext_file_path, &sealed_aes_256_file_path)
        .await
        .unwrap();
    envelope_manager
        .unseal_aes_256_file(&sealed_aes_256_file_path, &unsealed_aes_256_file_path)
        .await
        .unwrap();
    let mut sealed_aes_256_file = File::open(sealed_aes_256_file_path).unwrap();
    let mut sealed_aes_256_file_contents = Vec::new();
    sealed_aes_256_file
        .read_to_end(&mut sealed_aes_256_file_contents)
        .unwrap();
    let mut unsealed_aes_256_file = File::open(unsealed_aes_256_file_path).unwrap();
    let mut unsealed_aes_256_file_contents = Vec::new();
    unsealed_aes_256_file
        .read_to_end(&mut unsealed_aes_256_file_contents)
        .unwrap();
    log::info!(
        "sealed_aes_256_file_contents: {:?}",
        sealed_aes_256_file_contents
    );
    log::info!(
        "unsealed_aes_256_file_contents: {:?}",
        unsealed_aes_256_file_contents
    );
    assert_eq!(&unsealed_aes_256_file_contents, plaintext.as_bytes());
    assert!(cmp_manager::eq_vectors(
        &unsealed_aes_256_file_contents,
        plaintext.as_bytes()
    ));

    sleep(Duration::from_secs(2)).await;

    // envelope encryption with "AES_256" (32-byte)
    let plaintext_sealed = envelope_manager
        .seal_aes_256(plaintext.as_bytes())
        .await
        .unwrap();

    sleep(Duration::from_secs(2)).await;

    let plaintext_sealed_unsealed = envelope_manager
        .unseal_aes_256(&plaintext_sealed)
        .await
        .unwrap();

    log::info!("plaintext_sealed: {:?}", plaintext_sealed);
    log::info!("plaintext_sealed_unsealed: {:?}", plaintext_sealed_unsealed);
    assert_eq!(&plaintext_sealed_unsealed, plaintext.as_bytes());
    assert!(cmp_manager::eq_vectors(
        &plaintext_sealed_unsealed,
        plaintext.as_bytes()
    ));

    kms_manager
        .revoke_grant(&encrypt_key.id, &grant_id)
        .await
        .unwrap();

    kms_manager
        .schedule_to_delete(&encrypt_key.id, 7)
        .await
        .unwrap();

    sleep(Duration::from_secs(2)).await;

    // error should be ignored if it's already scheduled for delete
    kms_manager
        .schedule_to_delete(&encrypt_key.id, 7)
        .await
        .unwrap();
}
