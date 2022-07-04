use sysinfo::{DiskExt, System, SystemExt};

/// cargo run --example ec2_disk
fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    command_manager::run("sudo systemctl daemon-reload").expect("failed command");

    let res = command_manager::run("sudo mkfs -t ext4 /dev/nvme1n1");
    if res.is_err() {
        // e.g., mke2fs 1.45.5 (07-Jan-2020) /dev/nvme1n1 is mounted; will not make a filesystem here!
        let e = res.err().unwrap();
        if e.to_string()
            .contains(format!("{} is mounted", "/dev/nvme1n1").as_str())
        {
            log::warn!("ignoring {:?}", e);
        } else {
            panic!("unexpected err {:?}", e);
        }
    }

    command_manager::run("mkdir -p /data").expect("failed command");

    let res = command_manager::run("sudo mount /dev/nvme1n1 /data -t ext4");
    if res.is_err() {
        // e.g., mount: /data: /dev/nvme1n1 already mounted on /data
        let e = res.err().unwrap();
        if e.to_string()
            .contains(format!("{} already mounted", "/dev/nvme1n1").as_str())
        {
            log::warn!("ignoring {:?}", e);
        } else {
            panic!("unexpected err {:?}", e);
        }
    }

    command_manager::run("sudo cp /etc/fstab /tmp/fstab").expect("failed command");
    command_manager::run("sudo chmod 777 /tmp/fstab").expect("failed command");
    command_manager::run(
        "echo '/dev/nvme1n1       /data   ext4    defaults,nofail 0       2' >> /tmp/fstab",
    )
    .expect("failed command");
    command_manager::run("sudo cp /tmp/fstab /etc/fstab").expect("failed command");

    let (o1, o2) = command_manager::run("sudo cat /etc/fstab").expect("failed command");
    println!("out1 {}, out2 {}", o1, o2);

    command_manager::run("sudo mount --all").expect("failed command");

    let (o1, o2) = command_manager::run("lsblk").expect("failed command");
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = command_manager::run("df -h").expect("failed command");
    println!("out1 {}, out2 {}", o1, o2);

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
