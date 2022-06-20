use ripemd::{Digest, Ripemd160};

/// Creates an ID based on host information.
pub fn string(n: usize) -> String {
    let id = format!(
        "{}-{}-{}-{}-{}",
        whoami::username(),
        whoami::realname(),
        whoami::hostname(),
        whoami::platform(),
        whoami::devicename(),
    );

    let mut hasher = Ripemd160::new();
    hasher.update(id.as_bytes());
    let result = hasher.finalize();

    let mut id = bs58::encode(&result[..]).into_string();
    if n > 0 && id.len() > n {
        id.truncate(n);
    }
    id.to_lowercase()
}

/// RUST_LOG=debug cargo test --all-features --package avalanche-utils --lib -- system_id::test_string --exact --show-output
#[test]
fn test_string() {
    let _ = env_logger::builder().is_test(true).try_init();
    use log::info;

    let system1 = string(10);
    let system2 = string(10);
    assert_eq!(system1, system2);

    info!("system1: {:?}", system1);
    info!("system2: {:?}", system2);
}
