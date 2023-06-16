use std::io::{self, Error, ErrorKind};

use crate::ec2::{ArchType, OsType};

pub fn start(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("#!/usr/bin/env bash

# print all executed commands to terminal
set -x

# do not mask errors in a pipeline
set -o pipefail

# treat unset variables as an error
set -o nounset

# exit script whenever it errs
set -o errexit

# makes the  default answers be used for all questions
export DEBIAN_FRONTEND=noninteractive



############################################
### Machine Architecture ###################
############################################
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# 'dpkg --print-architecture' to decide amd64/arm64

MACHINE=$(uname -m)
if [ \"$MACHINE\" == \"x86_64\" ]; then
    ARCH=\"amd64\"
elif [ \"$MACHINE\" == \"aarch64\" ]; then
    ARCH=\"arm64\"
else
    echo \"Unknown machine architecture '$MACHINE'\" >&2
    exit 1
fi
echo MACHINE: $MACHINE
echo ARCH: $ARCH

# running as root, in /, check CPU/OS/host info
whoami
pwd
lscpu
cat /etc/os-release
hostnamectl



############################################
### Basic packages #########################
############################################

while [ 1 ]; do
    sudo apt-get update -yq
    sudo apt-get upgrade -yq
    sudo apt-get install -yq \\
    build-essential tmux zsh git \\
    jq curl wget \\
    unzip zip gzip tar \\
    libssl-dev \\
    pkg-config lsb-release \\
    linux-headers-$(uname -r)
    sudo apt-get clean
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
while [ 1 ]; do
    sudo apt update
    sudo apt clean all
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

# /usr/sbin/iptables
which iptables || true
iptables --version

# /usr/sbin/iptables-save
which iptables-save || true
iptables-save --version

# /usr/sbin/iptables-restore
which iptables-restore || true
iptables-restore --version

/usr/bin/gcc --version
/usr/bin/c++ -v
lsb_release --all
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn end(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("###########################






###########################
sudo apt clean
sudo apt-get clean
sudo find /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl list-units --type=service --no-pager
df -h
###########################







set +x
###########################
echo SUCCESS

BASE_AMI_ID=$(imds /latest/meta-data/ami-id)
cat << EOF > /tmp/release
BASE_AMI_ID=$BASE_AMI_ID
BUILD_TIME=$(date)
BUILD_KERNEL=$(uname -r)
ARCH=$(uname -m)
EOF
cat /tmp/release

sudo cp /tmp/release /etc/release || true
sudo chmod 0444 /etc/release
###########################




"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn update_bash_profile(
    os_type: OsType,
    anaconda_installed: bool,
    python_installed: bool,
    rust_installed: bool,
    go_installed: bool,
    kubectl_installed: bool,
    helm_installed: bool,
    data_directory_mounted: bool,
) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            let mut paths = Vec::new();
            if anaconda_installed {
                paths.push("/home/ubuntu/anaconda3/bin".to_string());
            }
            if python_installed {
                paths.push("/home/ubuntu/.local/bin".to_string());
            }
            if rust_installed {
                paths.push("/home/ubuntu/.cargo/bin".to_string());
            }

            let mut profile = String::from(
                "cat<<EOF >> /home/ubuntu/.profile

",
            );
            let mut bashrc = String::from(
                "cat<<EOF >> /home/ubuntu/.bashrc

",
            );
            if go_installed {
                paths.push("/usr/local/go/bin".to_string());
                paths.push("/home/ubuntu/go/bin".to_string());

                profile.push_str(
                    "export GOPATH=/home/ubuntu/go
",
                );
                bashrc.push_str(
                    "export GOPATH=/home/ubuntu/go
",
                );
            }

            if kubectl_installed {
                profile.push_str(
                    "alias k=kubectl
",
                );
                bashrc.push_str(
                    "alias k=kubectl
",
                );
            }
            if helm_installed {
                profile.push_str(
                    "alias h=helm
",
                );
                bashrc.push_str(
                    "alias h=helm
",
                );
            }

            let path_line = format!(
                "export PATH={}:$PATH
",
                paths.join(":")
            );

            profile.push_str(&path_line);
            if rust_installed {
                // only include in the profile
                profile.push_str(
                    ". /opt/rust/env || true
",
                )
            }
            bashrc.push_str(&path_line);
            if rust_installed {
                // only include in the bashrc
                bashrc.push_str(
                    ". /opt/rust/env || true
",
                )
            }

            // add "path_line" once more to use the PATH
            // during the following executions
            if data_directory_mounted {
                Ok(format!(
                    "
###########################
# setting up user bash profiles

{profile}
# set permissions
sudo chown -R $(whoami) /data || true
sudo chown -R ubuntu /data || true

EOF

{bashrc}
# set permissions
sudo chown -R $(whoami) /data || true
sudo chown -R ubuntu /data || true

EOF

{path_line}"
                ))
            } else {
                Ok(format!(
                    "
###########################
# setting up user bash profiles

{profile}
# set permissions

EOF

{bashrc}

EOF

{path_line}"
                ))
            }
        }
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn write_cluster_data(s3_bucket: &str, id: &str, data_directory_mounted: bool) -> String {
    if data_directory_mounted {
        format!(
            "
###########################
# write cluster information, assume /data is mounted

echo -n \"{s3_bucket}\" > /data/current_s3_bucket
echo -n \"{id}\" > /data/current_id
"
        )
    } else {
        format!(
            "
###########################
# write cluster information, assume /data is not mounted

echo -n \"{s3_bucket}\" > /tmp/current_s3_bucket
echo -n \"{id}\" > /tmp/current_id
"
        )
    }
}

pub fn imds(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install imds utils
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# https://github.com/awslabs/amazon-eks-ami/tree/master/files/bin

while [ 1 ]; do
    rm -f /tmp/imds || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/bin/imds\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/imds
sudo mv /tmp/imds /usr/bin/imds
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn provider_id(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install provider-id utils
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# https://github.com/awslabs/amazon-eks-ami/tree/master/files/bin

while [ 1 ]; do
    rm -f /tmp/provider-id || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/bin/provider-id\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/provider-id
sudo mv /tmp/provider-id /usr/bin/provider-id
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn vercmp(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install vercmp utils
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# https://github.com/awslabs/amazon-eks-ami/tree/master/files/bin

while [ 1 ]; do
    rm -f /tmp/vercmp || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/bin/vercmp\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/vercmp
sudo mv /tmp/vercmp /usr/bin/vercmp
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn setup_local_disks(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install setup-local-disks utils
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# https://github.com/awslabs/amazon-eks-ami/tree/master/files/bin

while [ 1 ]; do
    rm -f /tmp/setup-local-disks || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/bin/setup-local-disks\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/setup-local-disks
sudo mv /tmp/setup-local-disks /usr/bin/setup-local-disks
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn mount_bpf_fs(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install mount-bpf-fs utils
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# https://github.com/awslabs/amazon-eks-ami/tree/master/files/bin

while [ 1 ]; do
    rm -f /tmp/mount-bpf-fs || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/bin/mount-bpf-fs\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/mount-bpf-fs
sudo mv /tmp/mount-bpf-fs /usr/bin/mount-bpf-fs
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn time_sync(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install time sync utils

sudo timedatectl set-ntp on

# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
sudo apt-get install -yq chrony
# https://manpages.ubuntu.com/manpages/trusty/man8/update-rc.d.8.html
update-rc.d chrony defaults 80 20
find /etc/init.d/

cat /etc/chrony/chrony.conf

# already tsc
cat /sys/devices/system/clocksource/clocksource0/current_clocksource

# If current clocksource is xen, switch to tsc
if grep --quiet xen /sys/devices/system/clocksource/clocksource0/current_clocksource \\
    && grep --quiet tsc /sys/devices/system/clocksource/clocksource0/available_clocksource; then
    echo \"tsc\" | sudo tee /sys/devices/system/clocksource/clocksource0/current_clocksource
else
    echo \"tsc as a clock source is not applicable, skipping.\"
fi
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn system_limit_bump(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# bumping up system limits
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh

echo fs.inotify.max_user_watches=524288 | sudo tee -a /etc/sysctl.conf
echo fs.inotify.max_user_instances=8192 | sudo tee -a /etc/sysctl.conf
echo vm.max_map_count=524288 | sudo tee -a /etc/sysctl.conf

# e.g.,
# \"Accept error: accept tcp [::]:9650: accept4: too many open files; retrying in 1s\"
sudo echo \"* hard nofile 1000000\" >> /etc/security/limits.conf
sudo echo \"* soft nofile 1000000\" >> /etc/security/limits.conf
sudo sysctl -w fs.file-max=1000000
sudo sysctl -p
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn ssm_agent(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(
            "
###########################
# install ssm agent
# https://docs.aws.amazon.com/systems-manager/latest/userguide/agent-install-ubuntu.html

sudo snap install amazon-ssm-agent --classic
sudo systemctl enable snap.amazon-ssm-agent.amazon-ssm-agent.service
sudo systemctl restart snap.amazon-ssm-agent.amazon-ssm-agent.service
mkdir -p /etc/systemd/system/snap.amazon-ssm-agent.amazon-ssm-agent.service.d
cat > /tmp/amazon-ssm-agent-10-restart-always.conf <<EOF
[Service]
Restart=always
RestartSec=60s
EOF

sudo mv /tmp/amazon-ssm-agent-10-restart-always.conf /etc/systemd/system/snap.amazon-ssm-agent.amazon-ssm-agent.service.d/10-restart-always.conf
sudo systemctl start --no-block snap.amazon-ssm-agent.amazon-ssm-agent.service
".to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn cloudwatch_agent(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!(
            "
###########################
# install cloudwatch agent
# https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/QuickStartEC2Instance.html
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    rm -f /tmp/amazon-cloudwatch-agent.deb || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://s3.amazonaws.com/amazoncloudwatch-agent/ubuntu/$(dpkg --print-architecture)/latest/amazon-cloudwatch-agent.deb\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
while [ 1 ]; do
    sudo dpkg -i -E /tmp/amazon-cloudwatch-agent.deb
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
"
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn static_volume_provisioner(
    os_type: OsType,
    id: &str,
    region: &str,
    volume_type: &str,
    volume_size: u32,
    volume_iops: u32,
    volume_throughput: u32,
    ebs_device_name: &str,
    provisioner_initial_wait_random_seconds: usize,
) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!(
            "
###########################
# install aws-volume-manager for x86_64 (mac, linux x86), arm64 (M*), aarch64 (graviton)
# https://github.com/ava-labs/volume-manager/releases

while [ 1 ]; do
    rm -f /tmp/aws-volume-provisioner.$(uname -m)-{os_type}-linux-gnu || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/ava-labs/volume-manager/releases/download/latest/aws-volume-provisioner.$(uname -m)-{os_type}-linux-gnu\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/aws-volume-provisioner.$(uname -m)-{os_type}-linux-gnu
/tmp/aws-volume-provisioner.$(uname -m)-{os_type}-linux-gnu --version

/tmp/aws-volume-provisioner.$(uname -m)-{os_type}-linux-gnu \\
--log-level=info \\
--region {region} \\
--initial-wait-random-seconds={provisioner_initial_wait_random_seconds} \\
--id-tag-key=Id \\
--id-tag-value={id} \\
--kind-tag-key=Kind \\
--kind-tag-value=aws-volume-provisioner \\
--ec2-tag-asg-name-key=ASG_NAME \\
--asg-tag-key=autoscaling:groupName \\
--volume-type={volume_type} \\
--volume-size={volume_size} \\
--volume-iops={volume_iops} \\
--volume-throughput={volume_throughput} \\
--ebs-device-name={ebs_device_name} \\
--block-device-name=/dev/nvme1n1 \\
--filesystem-name=ext4 \\
--mount-directory-path=/data

# set permissions
sudo chown -R $(whoami) /data || true
sudo chown -R ubuntu /data || true
",
            os_type = os_type.as_str(),
            id=id,
            region=region,
            volume_type=volume_type,
            volume_size=volume_size,
            volume_iops=volume_iops,
            volume_throughput=volume_throughput,
            ebs_device_name=ebs_device_name,
            provisioner_initial_wait_random_seconds=provisioner_initial_wait_random_seconds,
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn static_ip_provisioner(
    os_type: OsType,
    id: &str,
    region: &str,
    provisioner_initial_wait_random_seconds: usize,
) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!(
            "
###########################
# install aws-ip-manager for x86_64 (mac, linux x86), arm64 (M*), aarch64 (graviton)
# https://github.com/ava-labs/ip-manager/releases

while [ 1 ]; do
    rm -f /tmp/aws-ip-provisioner.$(uname -m)-{os_type}-linux-gnu || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/ava-labs/ip-manager/releases/download/latest/aws-ip-provisioner.$(uname -m)-{os_type}-linux-gnu\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/aws-ip-provisioner.$(uname -m)-{os_type}-linux-gnu
/tmp/aws-ip-provisioner.$(uname -m)-{os_type}-linux-gnu --version

/tmp/aws-ip-provisioner.$(uname -m)-{os_type}-linux-gnu \\
--log-level=info \\
--region {region} \\
--initial-wait-random-seconds={provisioner_initial_wait_random_seconds} \\
--id-tag-key=Id \\
--id-tag-value={id} \\
--kind-tag-key=Kind \\
--kind-tag-value=aws-ip-provisioner \\
--ec2-tag-asg-name-key=ASG_NAME \\
--asg-tag-key=autoscaling:groupName \\
--mounted-eip-file-path=/data/eip.yaml
",
            os_type = os_type.as_str(),
            id=id,
            region=region,
            provisioner_initial_wait_random_seconds=provisioner_initial_wait_random_seconds,
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn anaconda(os_type: OsType) -> io::Result<String> {
    match os_type {
        // eval "$(/home/ubuntu/anaconda3/bin/conda shell.bash hook)"
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install anaconda
# https://docs.conda.io/projects/conda/en/latest/user-guide/install/linux.html
# https://www.anaconda.com/download#downloads

wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://repo.anaconda.com/archive/Anaconda3-2023.03-1-Linux-$(uname -m).sh\"

# batch mode to not interrup
export PREFIX=/home/ubuntu/anaconda3
PREFIX=/home/ubuntu/anaconda3 HOME=/home/ubuntu sh /tmp/Anaconda3-2023.03-1-Linux-$(uname -m).sh -b || true
rm -f /tmp/Anaconda3-2023.03-1-Linux-$(uname -m).sh

# conda update conda -y
# /home/ubuntu/anaconda3/bin/conda

sudo chown -R ubuntu /home/ubuntu/anaconda3/pkgs || true
sudo chown -R ubuntu /home/ubuntu/.conda/pkgs || true
sudo chown -R ubuntu /home/ubuntu/anaconda3/envs || true
sudo chown -R ubuntu /home/ubuntu/.conda/envs || true
sudo chown -R ubuntu /home/ubuntu/anaconda3/etc/conda || true
sudo chown -R ubuntu /home/ubuntu/anaconda3 || true

# check versions
which conda || true
/home/ubuntu/anaconda3/bin/conda --version || true

# check default system versions
which python3 || true
python3 --version || true
which python || true
python --version || true
which pip3 || true
pip3 --version || true
which pip || true
pip --version || true

# check versions from conda
/home/ubuntu/anaconda3/bin/python3 --version || true
/home/ubuntu/anaconda3/bin/python --version || true
/home/ubuntu/anaconda3/bin/pip3 --version || true
/home/ubuntu/anaconda3/bin/pip --version || true
".to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn python(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install python

sudo apt-get install -yq python3-pip
sudo apt install -yq python-is-python3

# /usr/bin/python3
which python3 || true
python3 --version || true

# /usr/bin/python
which python || true
python --version || true

pip3 install --upgrade pip

# /usr/local/bin/pip3
which pip3 || true
pip3 --version || true

# /usr/local/bin/pip
which pip || true
pip --version || true
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn rust(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install rust
# https://www.rust-lang.org/tools/install

export RUSTUP_HOME=/opt/rust
export CARGO_HOME=/opt/rust
curl --proto '=https' --tlsv1.2 -sSf --retry 70 --retry-delay 1 https://sh.rustup.rs | bash -s -- -y --no-modify-path --default-toolchain stable --profile default
sudo -H -u ubuntu bash -c 'source /opt/rust/env && rustup default stable'

. /opt/rust/env || true

# /opt/rust/bin/rustc
which rustc
rustc --version || true
".to_string())
        }

        OsType::Al2023 => {
            Ok("
###########################
# install rust
# https://www.rust-lang.org/tools/install

export RUSTUP_HOME=/opt/rust
export CARGO_HOME=/opt/rust
curl --proto '=https' --tlsv1.2 -sSf --retry 70 --retry-delay 1 https://sh.rustup.rs | bash -s -- -y --no-modify-path --default-toolchain stable --profile default
sudo -H -u ec2-user bash -c 'source /opt/rust/env && rustup default stable'

. /opt/rust/env || true

# /opt/rust/bin/rustc
which rustc
rustc --version || true
".to_string())
        }

        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn go(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install go for amd64 or arm64
# https://go.dev/dl
# 'dpkg --print-architecture' to decide amd64/arm64

sudo rm -rf /usr/local/go
GO_VERSION=1.20.5
sudo curl -s --retry 70 --retry-delay 1 https://storage.googleapis.com/golang/go$GO_VERSION.linux-$(dpkg --print-architecture).tar.gz | sudo tar -v -C /usr/local/ -xz

/usr/local/go/bin/go version
go version
".to_string())
        }
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn docker(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(
            "
###########################
# install docker
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    sudo apt-get install -yq ca-certificates gnupg
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
while [ 1 ]; do
    sudo rm -f /usr/share/keyrings/docker-archive-keyring.gpg && curl -fsSL --retry 70 --retry-delay 1 https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
echo \"deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/ubuntu \\
    $(lsb_release -cs) stable\" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
while [ 1 ]; do
    sudo apt-get update -y && sudo apt-get install -yq docker-ce docker-ce-cli containerd.io docker-compose-plugin
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

sudo systemctl enable docker

sudo usermod -aG docker ubuntu

sudo newgrp docker
sudo systemctl start docker.service
sudo systemctl enable --now docker
sudo docker ps
sudo docker version

# /usr/bin/containerd
which containerd || true
containerd --version || true

# /usr/bin/ctr
which ctr || true
ctr --version || true

# /usr/bin/docker
which docker || true
docker ps || true
docker version || true
".to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn containerd(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install containerd
# https://containerd.io/downloads/
# 'dpkg --print-architecture' to decide amd64/arm64

# /usr/bin/containerd
which containerd || true
containerd --version || true

while [ 1 ]; do
    export CONTAINERD_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/containerd/containerd/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/containerd-${CONTAINERD_CURRENT_VERSION}-linux-$(dpkg --print-architecture).tar.gz || true;
    rm -rf /tmp/containerd || true;
    mkdir -p /tmp/containerd
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/containerd/containerd/releases/download/v${CONTAINERD_CURRENT_VERSION}/containerd-${CONTAINERD_CURRENT_VERSION}-linux-$(dpkg --print-architecture).tar.gz\" -O - | tar -xzv -C /tmp/containerd
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/containerd/bin/*
sudo mv /tmp/containerd/bin/* /usr/bin/
rm -rf /tmp/containerd || true;

# /usr/bin/containerd
which containerd || true
containerd --version || true

# /usr/bin/ctr
which ctr || true
ctr --version || true
ctr version || true
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn runc(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install runc
# https://github.com/opencontainers/runc
# 'dpkg --print-architecture' to decide amd64/arm64

which runc || true
runc --version || true

# this removes \"containerd.io docker-ce\"
# sudo apt-get install -yq runc

sudo apt-get install -yq libseccomp-dev

# rm -rf /tmp/runc
# git clone https://github.com/opencontainers/runc /tmp/runc
# cd /tmp/runc
# make
# sudo make install

while [ 1 ]; do
    export RUNC_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/opencontainers/runc/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/runc.$(dpkg --print-architecture) || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/opencontainers/runc/releases/download/v${RUNC_CURRENT_VERSION}/runc.$(dpkg --print-architecture)\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/runc.$(dpkg --print-architecture)
sudo mv /tmp/runc.$(dpkg --print-architecture) /usr/bin/runc

# /usr/bin/runc
which runc || true
runc --version || true
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn cni_plugins(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install CNI plugins
# https://github.com/containernetworking/plugins
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    export CNI_PLUGIN_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/containernetworking/plugins/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/cni-plugins-linux-$(dpkg --print-architecture)-v${CNI_PLUGIN_CURRENT_VERSION}.tgz || true;
    rm -rf /tmp/cni-plugins-linux-$(dpkg --print-architecture)-v${CNI_PLUGIN_CURRENT_VERSION} || true;
    rm -rf /tmp/cni-plugins || true;
    mkdir -p /tmp/cni-plugins
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/containernetworking/plugins/releases/download/v${CNI_PLUGIN_CURRENT_VERSION}/cni-plugins-linux-$(dpkg --print-architecture)-v${CNI_PLUGIN_CURRENT_VERSION}.tgz\" -O - | tar -xzv -C /tmp/cni-plugins
    rm -f /tmp/cni-plugins-linux-$(dpkg --print-architecture)-v${CNI_PLUGIN_CURRENT_VERSION}.tgz || true;
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

ls -lah /tmp/cni-plugins
chmod +x /tmp/cni-plugins/*

sudo mkdir -p /opt/cni/bin
sudo mv /tmp/cni-plugins/* /opt/cni/bin/
rm -rf /tmp/cni-plugins || true;

sudo find /opt/cni/bin/
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn protobuf_compiler(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install protobuf-compiler

sudo apt-get install -yq protobuf-compiler
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn aws_cfn_helper(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install aws-cfn-bootstrap and other helpers
# https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/cfn-helper-scripts-reference.html
# https://repost.aws/knowledge-center/install-cloudformation-scripts

# pip3 install --user aws-cfn-bootstrap doesn't work
# pip install https://s3.amazonaws.com/cloudformation-examples/aws-cfn-bootstrap-latest.tar.gz
# install for user
while [ 1 ]; do
    sudo -H -u ubuntu bash -c 'pip3 install --user https://s3.amazonaws.com/cloudformation-examples/aws-cfn-bootstrap-py3-latest.tar.gz'
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

# install for root
while [ 1 ]; do
    pip3 install --user https://s3.amazonaws.com/cloudformation-examples/aws-cfn-bootstrap-py3-latest.tar.gz
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

# /home/ubuntu/.local/bin/cfn-hup
which cfn-hup || true
cfn-hup --help || true
ls /home/ubuntu/.local/bin/

# sudo /sbin/service cfn-hup restart
# sudo ln -s /home/ubuntu/.local/bin/cfn-hup /etc/init.d/cfn-hup
# update-rc.d cfn-hup defaults
#
# sudo systemctl daemon-reload
# sudo systemctl status cfn-hup || true
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn saml2aws(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install saml2aws
# https://api.github.com/repos/Versent/saml2aws/releases/latest
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    export SAML2AWS_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/Versent/saml2aws/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/saml2aws_${SAML2AWS_CURRENT_VERSION}_linux_$(dpkg --print-architecture).tar.gz || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/Versent/saml2aws/releases/download/v${SAML2AWS_CURRENT_VERSION}/saml2aws_${SAML2AWS_CURRENT_VERSION}_linux_$(dpkg --print-architecture).tar.gz\" -O - | tar -xzv -C /tmp
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/saml2aws
sudo mv /tmp/saml2aws /usr/bin/saml2aws

saml2aws --version
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn aws_iam_authenticator(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => {
            Ok("
###########################
# install aws-iam-authenticator
# https://docs.aws.amazon.com/eks/latest/userguide/install-aws-iam-authenticator.html
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    export AWS_IAM_AUTHENTICATOR_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/kubernetes-sigs/aws-iam-authenticator/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_CURRENT_VERSION}_linux_$(dpkg --print-architecture) || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://github.com/kubernetes-sigs/aws-iam-authenticator/releases/download/v${AWS_IAM_AUTHENTICATOR_CURRENT_VERSION}/aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_CURRENT_VERSION}_linux_$(dpkg --print-architecture)\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_CURRENT_VERSION}_linux_$(dpkg --print-architecture)
sudo mv /tmp/aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_CURRENT_VERSION}_linux_$(dpkg --print-architecture) /usr/bin/aws-iam-authenticator

# /usr/bin/aws-iam-authenticator
which aws-iam-authenticator || true
aws-iam-authenticator version || true
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn ecr_credential_helper(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => Ok("
###########################
# install ECR credential helper
# https://github.com/awslabs/amazon-ecr-credential-helper

which docker-credential-ecr-login || true
docker-credential-ecr-login -version || true

while [ 1 ]; do
    export ECR_CREDENTIAL_HELPER_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/awslabs/amazon-ecr-credential-helper/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/aws-iam-authenticator_${ECR_CREDENTIAL_HELPER_CURRENT_VERSION}_linux_$(dpkg --print-architecture) || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://amazon-ecr-credential-helper-releases.s3.us-east-2.amazonaws.com/${ECR_CREDENTIAL_HELPER_CURRENT_VERSION}/linux-$(dpkg --print-architecture)/docker-credential-ecr-login\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/docker-credential-ecr-login
sudo mv /tmp/docker-credential-ecr-login /usr/bin/docker-credential-ecr-login

# /usr/bin/docker-credential-ecr-login
which docker-credential-ecr-login || true
docker-credential-ecr-login -version || true
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn ecr_credential_provider(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install ecr-credential-provider
# https://github.com/kubernetes/cloud-provider-aws/tree/master/cmd/ecr-credential-provider
# https://github.com/kubernetes/cloud-provider-aws/releases
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh

if ! command -v go &> /dev/null
then
    echo \"go could not be found\"
    exit 1
fi

while [ 1 ]; do
    HOME=/home/ubuntu GOPATH=/home/ubuntu/go /usr/local/go/bin/go install -v k8s.io/cloud-provider-aws/cmd/ecr-credential-provider@v1.27.1
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

which ecr-credential-provider || true
chmod +x /home/ubuntu/go/bin/ecr-credential-provider
sudo cp /home/ubuntu/go/bin/ecr-credential-provider /usr/bin/ecr-credential-provider

# /usr/bin/ecr-credential-provider
which ecr-credential-provider || true

# TODO: this blocks
# ecr-credential-provider --help

sudo mkdir -p /etc/eks
sudo mkdir -p /etc/eks/image-credential-provider

sudo cp /home/ubuntu/go/bin/ecr-credential-provider /etc/eks/image-credential-provider/ecr-credential-provider

while [ 1 ]; do
    rm -f /tmp/ecr-credential-provider-config.json || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/ecr-credential-provider-config.json\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/ecr-credential-provider-config.json
sudo mv /tmp/ecr-credential-provider-config.json /etc/eks/image-credential-provider/config.json

sudo chown -R root:root /etc/eks
sudo chown -R ubuntu:ubuntu /etc/eks
find /etc/eks
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn kubelet(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 | OsType::Al2023 => {
            Ok("
###########################
# install kubelet
# https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    export KUBELET_CURRENT_VERSION=$(curl -L -s --retry 70 --retry-delay 1 https://dl.k8s.io/release/stable.txt)
    export KUBELET_CURRENT_VERSION=\"v1.26.6\"
    rm -f /tmp/kubelet || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://dl.k8s.io/release/${KUBELET_CURRENT_VERSION}/bin/linux/$(dpkg --print-architecture)/kubelet\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/kubelet
sudo mv /tmp/kubelet /usr/bin/kubelet
rm -f /tmp/kubelet || true;

# /usr/bin/kubelet
which kubelet || true
kubelet --version || true
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn kubectl(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install kubectl
# https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    export KUBECTL_CURRENT_VERSION=$(curl -L -s --retry 70 --retry-delay 1 https://dl.k8s.io/release/stable.txt)
    rm -f /tmp/kubectl || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://dl.k8s.io/release/${KUBECTL_CURRENT_VERSION}/bin/linux/$(dpkg --print-architecture)/kubectl\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/kubectl
sudo mv /tmp/kubectl /usr/bin/kubectl
rm -f /tmp/kubectl || true;

# /usr/bin/kubectl
which kubectl || true
kubectl version --client || true
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn helm(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install helm
# https://helm.sh/docs/intro/install/
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    export HELM_CURRENT_VERSION=$(curl -Ls --retry 70 --retry-delay 1 https://api.github.com/repos/helm/helm/releases/latest | grep 'tag_name' | cut -d'v' -f2 | cut -d'\"' -f1)
    rm -f /tmp/helm-${HELM_CURRENT_VERSION}-linux-$(dpkg --print-architecture).tar.gz || true;
    rm -rf /tmp/helm || true;
    mkdir -p /tmp/helm
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://get.helm.sh/helm-v${HELM_CURRENT_VERSION}-linux-$(dpkg --print-architecture).tar.gz\" -O - | tar -xzv -C /tmp/helm
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

chmod +x /tmp/helm/linux-$(dpkg --print-architecture)/helm
sudo mv /tmp/helm/linux-$(dpkg --print-architecture)/helm /usr/bin/helm
rm -rf /tmp/helm || true;

# /usr/bin/helm
which helm
helm version || true
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn terraform(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => {
            Ok("
###########################
# install terraform
# https://developer.hashicorp.com/terraform/tutorials/aws-get-started/install-cli
# 'dpkg --print-architecture' to decide amd64/arm64

while [ 1 ]; do
    sudo apt-get install -yq gnupg software-properties-common
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
while [ 1 ]; do
    sudo rm -f /usr/share/keyrings/hashicorp-archive-keyring.gpg && curl -fsSL --retry 70 --retry-delay 1 https://apt.releases.hashicorp.com/gpg | sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/hashicorp-archive-keyring.gpg --fingerprint

echo \"deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main\" | sudo tee /etc/apt/sources.list.d/hashicorp.list > /dev/null
while [ 1 ]; do
    sudo apt-get update -y && sudo apt-get install -yq terraform
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

# /usr/bin/terraform
which terraform || true
terraform --version || true
".to_string())
        }
        _  => {
            Err(Error::new(
                ErrorKind::InvalidInput,
                format!("os_type '{}' not supported", os_type.as_str()),
            ))
        }
    }
}

pub fn ssh_key_with_email(os_type: OsType, email: &str) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!(
            "
###########################
# create an SSH key

# NOTE/SECURITY: this must be deleted when building AMI
ssh-keygen -q -t rsa -b 4096 -C \"{email}\" -N '' -f /home/ubuntu/.ssh/id_rsa <<<y >/dev/null 2>&1
eval \"$(ssh-agent -s)\"
ssh-add /home/ubuntu/.ssh/id_rsa
cat /home/ubuntu/.ssh/id_rsa.pub

# set permissions
sudo chown -R $(whoami) /home/ubuntu/.ssh || true
sudo chown -R ubuntu /home/ubuntu/.ssh || true
",
            email = email,
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn nvidia_driver(arch_type: ArchType, os_type: OsType) -> io::Result<String> {
    match (&arch_type, &os_type) {
        (ArchType::Amd64GpuG3NvidiaTeslaM60 | ArchType::Amd64GpuG4dnNvidiaT4 | ArchType::Amd64GpuG5NvidiaA10G , OsType::Ubuntu2004) => Ok("
###########################
# install nvidia driver for ubuntu 20.04
# https://www.nvidia.com/Download/index.aspx?lang=en-us
# https://docs.nvidia.com/datacenter/tesla/tesla-installation-notes/index.html
# https://www.nvidia.com/en-us/drivers/unix/

DRIVER_VERSION=460.106.00
BASE_URL=https://us.download.nvidia.com/tesla

while [ 1 ]; do
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"${BASE_URL}/${DRIVER_VERSION}/NVIDIA-Linux-$(uname -m)-${DRIVER_VERSION}.run\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

sudo sh /tmp/NVIDIA-Linux-$(uname -m)-${DRIVER_VERSION}.run --silent --ui=none --no-questions
rm -f /tmp/NVIDIA-Linux-$(uname -m)-${DRIVER_VERSION}.run
sudo tail /var/log/nvidia-installer.log

# check the driver
find /usr/lib/modules -name nvidia.ko
find /usr/lib/modules -name nvidia.ko -exec modinfo {} \\;

which nvidia-smi
nvidia-smi
"
        .to_string()),

        (ArchType::Amd64GpuG3NvidiaTeslaM60 | ArchType::Amd64GpuG4dnNvidiaT4 | ArchType::Amd64GpuG5NvidiaA10G , OsType::Ubuntu2204) => Ok("
###########################
# install nvidia driver for ubuntu 22.04
# https://www.nvidia.com/Download/index.aspx?lang=en-us
# https://docs.nvidia.com/datacenter/tesla/tesla-installation-notes/index.html
# https://www.nvidia.com/en-us/drivers/unix/

DRIVER_VERSION=525.105.17
BASE_URL=https://us.download.nvidia.com/tesla

while [ 1 ]; do
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"${BASE_URL}/${DRIVER_VERSION}/NVIDIA-Linux-$(uname -m)-${DRIVER_VERSION}.run\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

sudo sh /tmp/NVIDIA-Linux-$(uname -m)-${DRIVER_VERSION}.run --silent --ui=none --no-questions
rm -f /tmp/NVIDIA-Linux-$(uname -m)-${DRIVER_VERSION}.run
sudo tail /var/log/nvidia-installer.log

# check the driver
find /usr/lib/modules -name nvidia.ko
find /usr/lib/modules -name nvidia.ko -exec modinfo {} \\;

which nvidia-smi
nvidia-smi
"
        .to_string()),

        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("arch_type '{}', os_type '{}' not supported", arch_type.as_str(), os_type.as_str()),
        )),
    }
}

pub fn nvidia_cuda_toolkit(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install nvidia cuda toolkit
# this installs cuda 11 by default on ubuntu 20.04
# sudo apt install -yq nvidia-cuda-toolkit
# https://developer.nvidia.com/cuda-downloads?target_os=Linux&target_arch=x86_64&Distribution=Ubuntu&target_version=22.04&target_type=runfile_local

# this upgrades to CUDA Version: 12.1
CUDA_VERSION=12.1.1
TOOL_KIT_VERSION=530.30.02
BASE_URL=https://developer.download.nvidia.com/compute/cuda

while [ 1 ]; do
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"${BASE_URL}/${CUDA_VERSION}/local_installers/cuda_${CUDA_VERSION}_${TOOL_KIT_VERSION}_linux.run\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

sudo sh /tmp/cuda_${CUDA_VERSION}_${TOOL_KIT_VERSION}_linux.run --silent
rm -f /tmp/cuda_${CUDA_VERSION}_${TOOL_KIT_VERSION}_linux.run

which nvcc || true
nvcc --version || true

which nvidia-smi || true
nvidia-smi || true
"
            .to_string()),

        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn nvidia_container_toolkit(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install nvidia container toolkit
# https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html

distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -fsSL --retry 70 --retry-delay 1 https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg

curl -s -L --retry 70 --retry-delay 1 https://nvidia.github.io/libnvidia-container/$distribution/libnvidia-container.list | \\
sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \\
sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list

sudo apt-get update
sudo apt-get install -yq nvidia-container-toolkit

# checking nvidia container toolkit
which nvidia-ctk
sudo nvidia-ctk runtime configure --runtime=docker

# restart docker
sudo systemctl restart docker

# test nvidia container toolkit
sudo docker run --rm --runtime=nvidia --gpus all nvidia/cuda:11.6.2-base-ubuntu20.04 nvidia-smi
"
            .to_string()),

        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn cmake(os_type: OsType, python_pip_bin_path: &str) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!("
###########################
# install cmake
# https://askubuntu.com/questions/355565/how-do-i-install-the-latest-version-of-cmake-from-the-command-line

sudo apt purge --auto-remove cmake

# wget -O - https://apt.kitware.com/keys/kitware-archive-latest.asc 2>/dev/null | gpg --dearmor - | sudo tee /etc/apt/trusted.gpg.d/kitware.gpg >/dev/null
# sudo apt-add-repository 'deb https://apt.kitware.com/ubuntu/ focal main'
# sudo apt-add-repository 'deb https://apt.kitware.com/ubuntu/ jammy main'
# sudo apt update -y
# sudo apt install -yq cmake

which pip || true
{python_pip_bin_path}/pip install --upgrade cmake

which cmake || true
cmake --version || true
")),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn dev_bark(os_type: OsType, python_pip_bin_path: &str) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!(
            "
###########################
# install bark
# https://github.com/suno-ai/bark

ls -lah /data/
git clone https://github.com/suno-ai/bark.git /data/bark
cd /data/bark

which python || true
{python_pip_bin_path}/python -m pip install .
which pip || true
{python_pip_bin_path}/pip install --verbose nltk
"
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn gcc7(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install gcc7
# gcc downgrade <8, otherwise faiss 'cmake -B build .' fails with
# 138 | #error -- unsupported GNU version! gcc versions later than 8 are not supported!
# https://stackoverflow.com/questions/65605972/cmake-unsupported-gnu-version-gcc-versions-later-than-8-are-not-supported

sudo apt remove -y gcc
sudo apt-get install gcc-7 g++-7 -y
sudo ln -s /usr/bin/gcc-7 /usr/bin/gcc
sudo ln -s /usr/bin/g++-7 /usr/bin/g++
sudo ln -s /usr/bin/gcc-7 /usr/bin/cc
sudo ln -s /usr/bin/g++-7 /usr/bin/c++
/usr/bin/gcc --version
/usr/bin/c++ -v
"
            .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn dev_faiss_gpu(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install faiss
# https://github.com/facebookresearch/faiss#installing
# https://github.com/facebookresearch/faiss/blob/main/INSTALL.md
# https://github.com/facebookresearch/faiss/wiki/Troubleshooting

# otherwise,
# Could NOT find BLAS (missing: BLAS_LIBRARIES)
sudo apt-get install -yq libopenblas-dev

# otherwise,
# Could NOT find SWIG (missing: SWIG_EXECUTABLE SWIG_DIR python)
sudo apt-get install -yq swig

/usr/bin/gcc --version
/usr/bin/c++ -v

which cmake || true
cmake --version || true

ls -lah /data/
git clone https://github.com/facebookresearch/faiss.git /data/faiss

# generates the system-dependent configuration/build files in the build/ subdirectory
# cd /data/faiss
# cmake -B build .

# builds the C++ library
# cd /data/faiss
# make -C build -j faiss

# builds the python bindings for Faiss
# cd /data/faiss
# make -C build -j swigfaiss

# generates and installs the python package
# cd /data/faiss/build/faiss/python
# python setup.py install

# make the compiled library (either libfaiss.a or libfaiss.so on Linux) available system-wide, as well as the C++ headers
# cd /data/faiss
# make -C build install
"
            .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn eks_worker_node_ami(os_type: OsType) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok("
###########################
# install EKS worker node AMI
# https://github.com/awslabs/amazon-eks-ami/blob/master/scripts/install-worker.sh

#######
# install packages
#######
while [ 1 ]; do
    sudo apt-get update -yq
    sudo apt-get upgrade -yq
    sudo apt-get install -yq conntrack socat nfs-kernel-server ipvsadm
    sudo apt-get clean
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;

#######
# set up files
#######
sudo mkdir -p /etc/eks

targets=(
    get-ecr-uri.sh
    eni-max-pods.txt
    bootstrap.sh
    max-pods-calculator.sh
)
for target in \"${targets[@]}\"
do
    while [ 1 ]; do
        rm -f /tmp/${target} || true;
        wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/${target}\"
        if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
        sleep 2s;
    done;
    chmod +x /tmp/${target}
    sudo mv /tmp/${target} /etc/eks/${target}
done

sudo chown -R root:root /etc/eks
sudo chown -R ubuntu:ubuntu /etc/eks
find /etc/eks

#######
# set up iptables
#######
sudo mkdir -p /etc/eks

while [ 1 ]; do
    rm -f /tmp/iptables-restore.service || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/iptables-restore.service\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
chmod +x /tmp/iptables-restore.service
sudo mv /tmp/iptables-restore.service /etc/eks/iptables-restore.service

sudo chown -R root:root /etc/eks
sudo chown -R ubuntu:ubuntu /etc/eks
find /etc/eks

#######
# set up containerd
#######
sudo mkdir -p /etc/eks
sudo mkdir -p /etc/eks/containerd

targets=(
    containerd-config.toml
    kubelet-containerd.service
    sandbox-image.service
    pull-sandbox-image.sh
    pull-image.sh
)
for target in \"${targets[@]}\"
do
    while [ 1 ]; do
        rm -f /tmp/${target} || true;
        wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/${target}\"
        if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
        sleep 2s;
    done;
    chmod +x /tmp/${target}
    sudo mv /tmp/${target} /etc/eks/containerd/${target}
done

sudo chown -R root:root /etc/eks
sudo chown -R ubuntu:ubuntu /etc/eks
find /etc/eks

sudo mkdir -p /etc/systemd/system/containerd.service.d
cat << EOF | sudo tee /etc/systemd/system/containerd.service.d/10-compat-symlink.conf
[Service]
ExecStartPre=/bin/ln -sf /run/containerd/containerd.sock /run/dockershim.sock
EOF

cat << EOF | sudo tee -a /etc/modules-load.d/containerd.conf
overlay
br_netfilter
EOF

cat << EOF | sudo tee -a /etc/sysctl.d/99-kubernetes-cri.conf
net.bridge.bridge-nf-call-ip6tables = 1
net.bridge.bridge-nf-call-iptables = 1
net.ipv4.ip_forward = 1
EOF

cat <<EOF | sudo tee /etc/systemd/system/containerd.service
[Unit]
Description=containerd
Documentation=https://containerd.io

[Service]
Type=notify
ExecStart=/usr/bin/containerd

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl enable containerd
sudo systemctl start containerd
sudo ctr version

#######
# set up log-collector
#######
sudo mkdir -p /etc/eks/log-collector-script/

while [ 1 ]; do
    rm -f /tmp/eks-log-collector.sh || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/log-collector-script/linux/eks-log-collector.sh\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
chmod +x /tmp/eks-log-collector.sh
sudo mv /tmp/eks-log-collector.sh /etc/eks/log-collector-script/eks-log-collector.sh

sudo chown -R root:root /etc/eks
sudo chown -R ubuntu:ubuntu /etc/eks
find /etc/eks

#######
# set up logrotate
#######
sudo mkdir -p /var/log/journal

while [ 1 ]; do
    rm -f /tmp/logrotate-kube-proxy || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/logrotate-kube-proxy\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
sudo mv /tmp/logrotate-kube-proxy /etc/logrotate.d/kube-proxy
sudo chown root:root /etc/logrotate.d/kube-proxy

while [ 1 ]; do
    rm -f /tmp/logrotate.conf || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/logrotate.conf\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
sudo mv /tmp/logrotate.conf /etc/logrotate.conf
sudo chown root:root /etc/logrotate.conf

#######
# set up kubernetes
#######
sudo mkdir -p /var/lib/kubernetes
sudo mkdir -p /var/lib/kubelet
sudo mkdir -p /etc/kubernetes
sudo mkdir -p /etc/kubernetes/manifests
sudo mkdir -p /etc/kubernetes/kubelet
sudo mkdir -p /etc/systemd/system/kubelet.service.d

while [ 1 ]; do
    rm -f /tmp/kubelet-kubeconfig || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/kubelet-kubeconfig\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
sudo mv /tmp/kubelet-kubeconfig /var/lib/kubelet/kubeconfig
sudo chown root:root /var/lib/kubelet/kubeconfig

while [ 1 ]; do
    rm -f /tmp/kubelet.service || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/kubelet.service\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
sudo mv /tmp/kubelet.service /etc/systemd/system/kubelet.service
sudo chown root:root /etc/systemd/system/kubelet.service

while [ 1 ]; do
    rm -f /tmp/kubelet-config.json || true;
    wget --quiet --retry-connrefused --waitretry=1 --read-timeout=20 --timeout=15 --tries=70 --directory-prefix=/tmp/ --continue \"https://raw.githubusercontent.com/awslabs/amazon-eks-ami/master/files/kubelet-config.json\"
    if [ $? = 0 ]; then break; fi; # check return value, break if successful (0)
    sleep 2s;
done;
sudo mv /tmp/kubelet-config.json /etc/kubernetes/kubelet/kubelet-config.json
sudo chown root:root /etc/kubernetes/kubelet/kubelet-config.json

sudo systemctl daemon-reload
sudo systemctl disable kubelet

#######
# clean up
#######
cat /etc/machine-id || true

CLEANUP_IMAGE=\"${CLEANUP_IMAGE:-false}\"
if [[ \"$CLEANUP_IMAGE\" == \"true\" ]]; then
    sudo apt-get clean

    sudo rm -rf \
    /etc/hostname \
    /etc/machine-id \
    /etc/resolv.conf \
    /etc/ssh/ssh_host* \
    /home/ubuntu/.ssh/authorized_keys \
    /root/.ssh/authorized_keys \
    /var/lib/cloud/data \
    /var/lib/cloud/instance \
    /var/lib/cloud/instances \
    /var/lib/cloud/sem \
    /var/lib/dhclient/* \
    /var/lib/dhcp/dhclient.* \
    /var/lib/apt/history \
    /var/log/cloud-init-output.log \
    /var/log/cloud-init.log \
    /var/log/auth.log \
    /var/log/wtmp || true

    sudo rm -rf /home/ubuntu/.aws
    sudo rm -f /tmp/*
    sudo touch /etc/machine-id
fi

#######
# write release file
#######
BASE_AMI_ID=$(imds /latest/meta-data/ami-id)
cat << EOF > /tmp/release-full
BASE_AMI_ID=$BASE_AMI_ID
BUILD_TIME=$(date)
BUILD_KERNEL=$(uname -r)
ARCH=$(uname -m)

OS_RELEASE_ID=$(. /etc/os-release;echo $ID)
OS_RELEASE_DISTRIBUTION=$(. /etc/os-release;echo $ID$VERSION_ID)

KUBELET_VERSION=\"$(kubelet --version)\"
EOF
cat /tmp/release-full

sudo cp /tmp/release-full /etc/release-full || true
sudo chmod 0444 /etc/release-full
"
        .to_string()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}

pub fn aws_key(
    os_type: OsType,
    region: &str,
    aws_secret_key_id: &str,
    aws_secret_access_key: &str,
) -> io::Result<String> {
    match os_type {
        OsType::Ubuntu2004 | OsType::Ubuntu2204 => Ok(format!(
            "
###########################
# writing AWS access key

# NOTE/SECURITY: this must be deleted when building AMI
mkdir -p /home/ubuntu/.aws || true
rm -f /home/ubuntu/.aws/config || true
cat<<EOF >> /home/ubuntu/.aws/config
[default]
region = {region}
EOF

rm -f /home/ubuntu/.aws/credentials || true
cat<<EOF >> /home/ubuntu/.aws/credentials
[default]
aws_access_key_id = {aws_secret_key_id}
aws_secret_access_key = {aws_secret_access_key}
EOF

aws sts get-caller-identity
",
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("os_type '{}' not supported", os_type.as_str()),
        )),
    }
}
