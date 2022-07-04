use sysinfo::{DiskExt, System, SystemExt};

/// cargo run --example ec2_disk
fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let s = System::new();
    for disk in s.disks() {
        println!(
            "{:?}: {:?} {:?}",
            disk.name(),
            disk.type_(),
            disk.mount_point()
        );
    }
}
