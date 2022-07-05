use aws_manager::ec2;

/// cargo run --example ec2_disk
fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let filesystem_name = "ext4";
    let device_name = "/dev/nvme1n1";
    let dir_name = "/data";

    let (o1, o2) = ec2::disk::make_filesystem(filesystem_name, device_name).unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = ec2::disk::mount_filesystem(filesystem_name, device_name, dir_name).unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = ec2::disk::update_fstab(filesystem_name, device_name, dir_name).unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = command_manager::run("lsblk").unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = command_manager::run("df -h").unwrap();
    println!("out1 {}, out2 {}", o1, o2);
}
