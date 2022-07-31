use std::{
    fs::{self, File},
    io::{self, Write},
    thread, time,
};

/// Makes a new file system on a specified device.
///
/// e.g.,
/// sudo mkfs -t ext4 /dev/nvme1n1
///
/// ref. See https://github.com/cholcombe973/block-utils/blob/master/src/lib.rs for other commands.
/// ref. https://stackoverflow.com/questions/45167717/mounting-a-nvme-disk-on-aws-ec2
pub fn make_filesystem(filesystem_name: &str, device_name: &str) -> io::Result<(String, String)> {
    match run_make_filesystem(filesystem_name, device_name) {
        Ok(v) => {
            log::debug!("first time success 'make_filesystem'");
            return Ok(v);
        }
        Err(e) => {
            log::warn!(
                "first time failure 'make_filesystem' {}; retrying after 5-second...",
                e
            );
            thread::sleep(time::Duration::from_secs(5));
            return run_make_filesystem(filesystem_name, device_name);
        }
    }
}

pub fn run_make_filesystem(
    filesystem_name: &str,
    device_name: &str,
) -> io::Result<(String, String)> {
    let device_path = if device_name.starts_with("/dev/") {
        device_name.to_string()
    } else {
        format!("/dev/{}", device_name)
    };

    log::info!(
        "making file system with 'mkfs' command on the device path '{}'",
        device_path
    );

    let cmd = format!("sudo mkfs -t {} {}", filesystem_name, device_path);
    let res = command_manager::run(&cmd);
    if res.is_err() {
        // e.g., mke2fs 1.45.5 (07-Jan-2020) /dev/nvme1n1 is mounted; will not make a filesystem here!
        // TODO: handle "mount: /data: wrong fs type, bad option, bad superblock on /dev/nvme1n1, missing codepage or helper program, or other error."
        let e = res.err().unwrap();
        if !e
            .to_string()
            .contains(format!("{} is mounted", device_path).as_str())
        {
            return Err(e);
        }

        log::warn!("ignoring the 'is mounted' error '{}'", e.to_string());
        Ok((String::new(), e.to_string()))
    } else {
        res
    }
}

/// Mounts the file system to the specified directory.
/// And updates "/etc/fstab" to auto remount in case of instance reboot.
///
/// e.g.,
/// sudo mount /dev/nvme1n1 /data -t ext4
///
/// ref. See https://github.com/cholcombe973/block-utils/blob/master/src/lib.rs for other commands.
/// ref. https://stackoverflow.com/questions/45167717/mounting-a-nvme-disk-on-aws-ec2
pub fn mount_filesystem(
    filesystem_name: &str,
    device_name: &str,
    dir_name: &str,
) -> io::Result<(String, String)> {
    let device_path = if device_name.starts_with("/dev/") {
        device_name.to_string()
    } else {
        format!("/dev/{}", device_name)
    };

    log::info!(
        "mounting the file system with 'mount' command on the device path '{}'",
        device_path
    );

    let cmd = format!(
        "sudo mount {} {} -t {}",
        device_path, dir_name, filesystem_name
    );
    let res = command_manager::run(&cmd);
    if res.is_err() {
        // e.g., mount: /data: /dev/nvme1n1 already mounted on /data
        let e = res.err().unwrap();
        if !e
            .to_string()
            .contains(format!("{} already mounted", device_path).as_str())
        {
            return Err(e);
        }

        log::warn!("ignoring the 'already mounted' error '{}'", e.to_string());
        Ok((String::new(), e.to_string()))
    } else {
        res
    }
}

const FSTAB_PATH: &str = "/etc/fstab";

/// Updates "/etc/fstab" to auto remount in case of instance reboot.
/// The output is the contents of the "fstab" file.
///
/// e.g.,
/// sudo echo '/dev/nvme1n1       /data   ext4    defaults,nofail 0       2' >> /etc/fstab
/// sudo mount --all
///
/// ref. See https://github.com/cholcombe973/block-utils/blob/master/src/lib.rs for other commands.
/// ref. https://stackoverflow.com/questions/45167717/mounting-a-nvme-disk-on-aws-ec2
pub fn update_fstab(
    filesystem_name: &str,
    device_name: &str,
    dir_name: &str,
) -> io::Result<(String, String)> {
    let device_path = if device_name.starts_with("/dev/") {
        device_name.to_string()
    } else {
        format!("/dev/{}", device_name)
    };

    log::info!(
        "updating the fstab file on the device path '{}'",
        device_path
    );

    let line = format!(
        "{}       {}   {}    defaults,nofail 0       2",
        device_path, dir_name, filesystem_name
    );
    let mut contents = fs::read_to_string(FSTAB_PATH)?;
    if contents.contains(&line) {
        log::warn!("fstab already contains '{}', skipping updating fstab", line);
        return Ok((contents, String::new()));
    }
    contents.push('\n');
    contents.push_str(&line);

    let tmp_path = random_manager::tmp_path(10, None)?;
    let mut f = File::create(&tmp_path)?;
    f.write_all(contents.as_bytes())?;

    let cmd = format!("sudo cp {} {}", tmp_path, FSTAB_PATH);
    command_manager::run(&cmd)?;
    command_manager::run("sudo mount --all")?;

    Ok((contents, String::new()))
}
