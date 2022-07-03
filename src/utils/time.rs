use chrono::prelude::*;

/// Generates a random ID with the prefix followed by a
/// timestamp and random characters.
pub fn with_prefix(pfx: &str) -> String {
    format!("{}-{}-{}", pfx, timestamp(6), random_manager::string(6))
}

/// RUST_LOG=debug cargo test --all-features --package avalanche-utils --lib -- time::test_with_prefix --exact --show-output
#[test]
fn test_with_prefix() {
    use log::info;
    use std::{thread, time};
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init();

    let ts1 = with_prefix("hello");
    thread::sleep(time::Duration::from_millis(1001));
    let ts2 = with_prefix("hello");
    assert_ne!(ts1, ts2);

    info!("ts1: {:?}", ts1);
    info!("ts2: {:?}", ts2);
}

/// Gets the current timestamp in concatenated string format.
pub fn timestamp(n: usize) -> String {
    let local: DateTime<Local> = Local::now();
    let mut d = format!(
        "{}{:02}{:02}{:02}{:02}",
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.second(),
    );
    if d.len() > n {
        d.truncate(n);
    }
    d
}

/// RUST_LOG=debug cargo test --all-features --package avalanche-utils --lib -- time::test_timestamp --exact --show-output
#[test]
fn test_timestamp() {
    use log::info;
    use std::{thread, time};
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init();

    let ts1 = timestamp(12);
    thread::sleep(time::Duration::from_millis(1001));
    let ts2 = timestamp(12);
    assert_ne!(ts1, ts2);

    info!("ts1: {:?}", ts1);
    info!("ts2: {:?}", ts2);
}
