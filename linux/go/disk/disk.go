package disk

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/gyuho/infra/go/logutil"
	"github.com/gyuho/infra/go/randutil"

	"k8s.io/utils/exec"
)

// Makes a new file system on the specified device.
//
// e.g.,
// sudo mkfs -t ext4 /dev/nvme1n1
//
// Usually, "nvme0n1" is the boot volume.
// "nvme1n1" is the additional mounted volume.
//
// See https://github.com/cholcombe973/block-utils/blob/master/src/lib.rs for other commands.
// ref. https://stackoverflow.com/questions/45167717/mounting-a-nvme-disk-on-aws-ec2
func Mkfs(ctx context.Context, fsName string, deviceName string) ([]byte, error) {
	cmdPath, err := exec.New().LookPath("mkfs")
	if err != nil {
		return nil, fmt.Errorf("mkfs not found (%w)", err)
	}

	devicePath := deviceName
	if !strings.HasPrefix(deviceName, "/dev/") {
		devicePath = "/dev/" + deviceName
	}

	args := []string{cmdPath, "-t", fsName, devicePath}
	logutil.S().Infow("making a file system",
		"fsName", fsName,
		"devicePath", devicePath,
		"command", strings.Join(args, " "),
	)

	out, err := exec.New().CommandContext(ctx, args[0], args[1:]...).CombinedOutput()
	if err != nil {
		// e.g., mke2fs 1.45.5 (07-Jan-2020) /dev/nvme1n1 is mounted; will not make a filesystem here!
		// TODO: handle "mount: /data: wrong fs type, bad option, bad superblock on /dev/nvme1n1, missing codepage or helper program, or other error."
		if strings.Contains(string(out), devicePath+" is mounted") {
			logutil.S().Warnw("device is already mounted", "devicePath", devicePath, "error", err)
			return out, nil
		}
		return nil, err
	}
	return out, nil
}

// Mounts a file system to the specified directory.
//
// e.g.,
// sudo mount /dev/nvme1n1 /data -t ext4
//
// See https://github.com/cholcombe973/block-utils/blob/master/src/lib.rs for other commands.
// ref. https://stackoverflow.com/questions/45167717/mounting-a-nvme-disk-on-aws-ec2
func Mount(ctx context.Context, fsName string, deviceName string, dirPath string) ([]byte, error) {
	cmdPath, err := exec.New().LookPath("mount")
	if err != nil {
		return nil, fmt.Errorf("mount not found (%w)", err)
	}

	devicePath := deviceName
	if !strings.HasPrefix(deviceName, "/dev/") {
		devicePath = "/dev/" + deviceName
	}

	args := []string{cmdPath, devicePath, dirPath, fsName}
	logutil.S().Infow("mounting the file system",
		"fsName", fsName,
		"devicePath", devicePath,
		"dirPath", dirPath,
		"command", strings.Join(args, " "),
	)

	out, err := exec.New().CommandContext(ctx, args[0], args[1:]...).CombinedOutput()
	if err != nil {
		// e.g., mount: /data: /dev/nvme1n1 already mounted on /data
		if strings.Contains(string(out), devicePath+" already mounted") {
			logutil.S().Warnw("device is already mounted", "devicePath", devicePath, "error", err)
			return out, nil
		}
		return nil, err
	}
	return out, nil
}

const FSTAB_PATH = "/etc/fstab"

// Updates "/etc/fstab" to auto remount in case of instance reboot.
func UpdateFstab(ctx context.Context, fsName string, deviceName string, dirPath string) ([]byte, error) {
	devicePath := deviceName
	if !strings.HasPrefix(deviceName, "/dev/") {
		devicePath = "/dev/" + deviceName
	}

	line := fmt.Sprintf(`%s       %s   %s    defaults,nofail 0       2`, devicePath, dirPath, fsName)
	b, err := os.ReadFile(FSTAB_PATH)
	if err != nil {
		return nil, err
	}
	if strings.Contains(string(b), line) {
		logutil.S().Warnw("fstab already contains the line", "line", line)
		return b, nil
	}
	b = append(b, []byte("\n")...)
	b = append(b, []byte(line)...)

	tmpPath := filepath.Join(os.TempDir(), "fstab."+randutil.String(10)+".tmp")
	if err := os.WriteFile(tmpPath, b, 0644); err != nil {
		return nil, err
	}

	if _, err := exec.New().CommandContext(ctx, "cp", tmpPath, FSTAB_PATH).CombinedOutput(); err != nil {
		return nil, err
	}
	return b, nil
}
