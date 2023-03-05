use aws_manager::ec2;

/// cargo run --example ec2_disk --features="ec2"
#[tokio::main]
async fn main() {
    // ref. https://github.com/env-logger-rs/env_logger/issues/47
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let filesystem_name = "ext4";
    let device_name = "nvme1n1";
    let dir_name = "/data";

    let (o1, o2) = ec2::disk::make_filesystem(filesystem_name, device_name).unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = ec2::disk::mount_filesystem(filesystem_name, device_name, dir_name).unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, o2) = ec2::disk::update_fstab(filesystem_name, device_name, dir_name).unwrap();
    println!("out1 {}, out2 {}", o1, o2);

    let (o1, _) = command_manager::run("lsblk").unwrap();
    println!("out1 {}", o1);
    assert!(o1.contains(device_name));
    assert!(o1.contains(dir_name));
    /*
    NAME         MAJ:MIN RM   SIZE RO TYPE MOUNTPOINT
    loop0          7:0    0  25.1M  1 loop /snap/amazon-ssm-agent/5656
    loop1          7:1    0  55.5M  1 loop /snap/core18/2409
    loop2          7:2    0  61.9M  1 loop /snap/core20/1518
    loop3          7:3    0  67.8M  1 loop /snap/lxd/22753
    loop4          7:4    0    47M  1 loop /snap/snapd/16010
    nvme1n1      259:0    0   400G  0 disk /data
    nvme0n1      259:1    0   200G  0 disk
    ├─nvme0n1p1  259:2    0 199.9G  0 part /
    ├─nvme0n1p14 259:3    0     4M  0 part
    └─nvme0n1p15 259:4    0   106M  0 part /boot/efi
    */

    let (o1, _) = command_manager::run("df -h").unwrap();
    println!("out1 {}", o1);
    assert!(o1.contains(device_name));
    assert!(o1.contains(dir_name));
    /*
    Filesystem       Size  Used Avail Use% Mounted on
    /dev/root        194G  9.7G  184G   6% /
    devtmpfs          31G     0   31G   0% /dev
    tmpfs             31G     0   31G   0% /dev/shm
    tmpfs            6.2G  968K  6.2G   1% /run
    tmpfs            5.0M     0  5.0M   0% /run/lock
    tmpfs             31G     0   31G   0% /sys/fs/cgroup
    /dev/loop0        26M   26M     0 100% /snap/amazon-ssm-agent/5656
    /dev/nvme0n1p15  105M  5.2M  100M   5% /boot/efi
    /dev/loop1        56M   56M     0 100% /snap/core18/2409
    /dev/loop2        62M   62M     0 100% /snap/core20/1518
    /dev/loop3        68M   68M     0 100% /snap/lxd/22753
    /dev/loop4        47M   47M     0 100% /snap/snapd/16010
    /dev/nvme1n1     393G   73M  373G   1% /data
    tmpfs            6.2G     0  6.2G   0% /run/user/1000
    */
}
