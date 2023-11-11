// Volume provisioner for AWS.
// See https://github.com/ava-labs/volume-manager/tree/main/aws-volume-provisioner/src for the original Rust code.
package main

import (
	"context"
	"fmt"
	"math/rand"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/aws/go/ec2"
	"github.com/gyuho/infra/aws/go/ec2/metadata"
	"github.com/gyuho/infra/go/logutil"
	"github.com/gyuho/infra/linux/go/disk"

	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	"github.com/spf13/cobra"
)

const appName = "aws-volume-provisioner"

var cmd = &cobra.Command{
	Use:        appName,
	Short:      appName,
	Aliases:    []string{"volume-provisioner"},
	SuggestFor: []string{"volume-provisioner"},
	Run:        cmdFunc,
}

var (
	region                   string
	initialWaitRandomSeconds int

	idTagKey   string
	idTagValue string

	kindTagKey   string
	kindTagValue string

	localInstancePublishTagKey string

	volLeaseHoldKey string

	volType       string
	volEncrypted  bool
	volSizeInGB   int32
	volIOPS       int32
	volThroughput int32

	ebsDevice       string
	blockDevice     string
	fsName          string
	mountDir        string
	curEBSVolIDFile string
)

// Do not use "aws:" for custom tag creation, as it's not allowed.
// e.g., aws:autoscaling:groupName
// Only use "aws:autoscaling:groupName" for querying.
const asgNameTagKey = "autoscaling:groupName"

func init() {
	cobra.EnablePrefixMatching = true

	cmd.PersistentFlags().StringVar(&region, "region", "us-east-1", "region to provision the volume in")
	cmd.PersistentFlags().IntVar(&initialWaitRandomSeconds, "initial-wait-random-seconds", 60, "maximum number of seconds to wait (value chosen at random with the range, highly recommend setting value >=60 because EC2 tags take awhile to pupulate)")

	cmd.PersistentFlags().StringVar(&idTagKey, "id-tag-key", "Id", "key for the EBS volume 'Id' tag")
	cmd.PersistentFlags().StringVar(&idTagValue, "id-tag-value", "", "value for the EBS volume 'Id' tag key")

	cmd.PersistentFlags().StringVar(&kindTagKey, "kind-tag-key", "Kind", "key for the EBS volume 'Kind' tag")
	cmd.PersistentFlags().StringVar(&kindTagValue, "kind-tag-value", "aws-volume-provisioner", "value for the EBS volume 'Kind' tag key")

	cmd.PersistentFlags().StringVar(&localInstancePublishTagKey, "local-instance-publish-tag-key", "AWS_VOLUME_PROVISIONER_ATTACHED_VOLUME_ID", "tag key to create with the resource value to the local EC2 instance")

	cmd.PersistentFlags().StringVar(&volLeaseHoldKey, "volume-lease-hold-key", "LeaseHold", "key for the EBS volume lease holder (e.g., i-12345678_1662596730 means i-12345678 acquired the lease for this volume at the unix timestamp 1662596730)")

	cmd.PersistentFlags().StringVar(&volType, "volume-type", "gp3", "EBS volume type")
	cmd.PersistentFlags().BoolVar(&volEncrypted, "volume-encrypted", true, "whether to encrypt volume or not")
	cmd.PersistentFlags().Int32Var(&volSizeInGB, "volume-size-in-gb", 300, "EBS volume size in GB")
	cmd.PersistentFlags().Int32Var(&volIOPS, "volume-iops", 3000, "EBS volume IOPS")
	cmd.PersistentFlags().Int32Var(&volThroughput, "volume-throughput", 500, "EBS volume throughput")

	cmd.PersistentFlags().StringVar(&ebsDevice, "ebs-device", "", "EBS device name (e.g., /dev/xvdb)")
	cmd.PersistentFlags().StringVar(&blockDevice, "block-device", "", "OS-level block device name (e.g., /dev/nvme1n1)")
	cmd.PersistentFlags().StringVar(&fsName, "filesystem", "", "filesystem name to create (e.g., ext4)")
	cmd.PersistentFlags().StringVar(&mountDir, "mount-directory", "", "directory path to mount onto the device (e.g., /data)")
	cmd.PersistentFlags().StringVar(&curEBSVolIDFile, "current-ebs-volume-id-file", "/data/current-ebs-volume-id", "file path to write the current EBS volume ID (useful for paused instances)")
}

func main() {
	if err := cmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "%q failed %v\n", appName, err)
		os.Exit(1)
	}
	os.Exit(0)
}

func cmdFunc(cmd *cobra.Command, args []string) {
	initialWait := time.Duration(rand.Intn(initialWaitRandomSeconds)) * time.Second
	logutil.S().Infow("starting 'aws-volume-provisioner'", "initialWait", initialWait)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	az, err := metadata.FetchAvailabilityZone(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to fetch availability zone", "error", err)
		os.Exit(1)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	localInstanceID, err := metadata.FetchInstanceID(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to fetch EC2 instance ID", "error", err)
		os.Exit(1)
	}

	cfg, err := aws.New(&aws.Config{
		Region: region,
	})
	if err != nil {
		logutil.S().Warnw("failed to create aws config", "error", err)
		os.Exit(1)
	}

	logutil.S().Infow("fetching instance tags to get the asg name",
		"region", region,
		"az", az,
		"instanceID", localInstanceID,
		"asgNameTagKey", asgNameTagKey,
	)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Minute)
	_, asgNameTagValue, err := ec2.WaitInstanceTagValue(ctx, cfg, localInstanceID, "aws:autoscaling:groupName")
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to get asg tag value in time", "error", err)
		os.Exit(1)
	}
	if asgNameTagValue == "" {
		logutil.S().Warnw("failed to get asg tag value in time")
		os.Exit(1)
	}
	logutil.S().Infow("found asg tag", "key", asgNameTagKey, "value", asgNameTagValue)

	// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVolumes.html
	describeVolTags := map[string]string{
		"attachment.device": ebsDevice,

		// ensures the call only returns the volume that is attached to this local instance
		"attachment.instance-id": localInstanceID,

		// ensures the call only returns the volume that is currently attached
		"attachment.status": "attached",

		// ensures the call only returns the volume that is currently in use
		"status": "in-use",

		"availability-zone": az,

		"tag:" + idTagKey:      idTagValue,
		"tag:" + kindTagKey:    kindTagValue,
		"tag:" + asgNameTagKey: asgNameTagValue,

		"volume-type": volType,
	}
	logutil.S().Infow(
		"checking if local instance already has an attached volume",
		"region", region,
		"describeVolumeTags", describeVolTags,
	)

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	localAttachedVols, err := ec2.DescribeVolumes(ctx, cfg, describeVolTags)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to describe volume", "error", err)
		os.Exit(1)
	}
	if len(localAttachedVols) > 0 {
		logutil.S().Infow("found locally attached volumes to this instance", "volumes", len(localAttachedVols))
	} else {
		logutil.S().Infow("no locally attached volume found")
	}

	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, syscall.SIGTERM, syscall.SIGINT)
	stopc := make(chan struct{})

	// only make filesystem (format) for initial creation
	// do not format volume for already attached EBS volumes
	// do not format volume for reused EBS volumes
	needMkfs := true
	attachVolumeID := ""
	if len(localAttachedVols) == 1 {
		logutil.S().Infow("no need mkfs because the local EC2 instance already has an volume attached")
		needMkfs = false
		attachVolumeID = *localAttachedVols[0].VolumeId
	} else {
		// ref. https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeVolumes.html
		describeVolTags := map[string]string{
			// ensures the call only returns the volume that is currently in use
			// ensures the call only returns the volume that is currently available
			"status": "available",

			"availability-zone": az,

			"tag:" + idTagKey:      idTagValue,
			"tag:" + kindTagKey:    kindTagValue,
			"tag:" + asgNameTagKey: asgNameTagValue,

			"volume-type": volType,
		}

		logutil.S().Infow("local EC2 instance has no attached volume, querying available volume by AZ",
			"instanceID", localInstanceID,
			"describeVolumeTags", describeVolTags,
		)

		ctx, cancel = context.WithTimeout(context.Background(), 40*time.Second)
		describedVols := make([]aws_ec2_v2_types.Volume, 0)
		for ctx.Err() == nil {
			select {
			case <-ctx.Done():
				logutil.S().Warnw("failed to get reusable volumes in time", "error", ctx.Err())
				os.Exit(1)
			case <-time.After(5 * time.Second):
			}

			describedVols, err = ec2.DescribeVolumes(ctx, cfg, describeVolTags)
			cancel()
			if err != nil {
				logutil.S().Warnw("failed to describe volume", "error", err)
				os.Exit(1)
			}

			logutil.S().Infow("described volumes", "volumes", len(describedVols))
			if len(describedVols) > 0 {
				break
			}

			logutil.S().Infow("no volume found... retrying in case of inconsistent/stale EBS describe_volumes API response")
		}

		reusableVolFoundInAZ := len(describedVols) > 0

		// if we don't check whether the other instance in the same AZ has "just" created
		// this EBS volume or not, this can be racey -- two instances may be trying to attach
		// the same EBS volume to two different instances at the same time
		if reusableVolFoundInAZ {
			logutil.S().Infow("checking volume lease holder", "key", volLeaseHoldKey)
			for _, tag := range describedVols[0].Tags {
				if *tag.Key != volLeaseHoldKey {
					continue
				}

				ss := strings.Split(*tag.Value, "_")
				if len(ss) != 2 {
					logutil.S().Warnw("unexpected lease hold key value", "value", *tag.Value)
					os.Exit(1)
				}

				leaseHolder := ss[0]
				lease := ss[1]
				leasedAt, err := strconv.ParseInt(lease, 10, 64)
				if err != nil {
					logutil.S().Warnw("failed to parse lease key value", "error", err)
					os.Exit(1)
				}

				// only reuse iff:
				// (1) leased by the same local EC2 instance (restarted volume provisioner)
				// (2) leased by the other EC2 instance but >10-minute ago

				// (1) leased by the same local EC2 instance (restarted volume provisioner)
				if leaseHolder == localInstanceID {
					logutil.S().Infow("lease holder same as local instance ID", "leaseHolder", leaseHolder)
					reusableVolFoundInAZ = true
					break
				}

				logutil.S().Warnw("was leased by some other instance", "leaseHolder", leaseHolder)
				leaseDelta := time.Now().UTC().Unix() - leasedAt
				if leaseDelta > 600 {
					logutil.S().Infow("lease expired >10 minutes ago, taking over")
					reusableVolFoundInAZ = true
				} else {
					logutil.S().Infow("lease not expired yet, do not take over", "leaseDelta", leaseDelta)
				}

				break
			}
		}

		unixTS := time.Now().UTC().Unix()
		volLeaseHoldValue := localInstanceID + "_" + fmt.Sprintf("%d", unixTS)

		if reusableVolFoundInAZ {
			reusedVolID := *describedVols[0].VolumeId

			logutil.S().Infow("found reusable volume -- renewing the lease", "volumeID", reusedVolID)
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			err = ec2.CreateTags(
				ctx,
				cfg,
				[]string{reusedVolID},
				map[string]string{
					volLeaseHoldKey: volLeaseHoldValue,
				})
			cancel()
			if err != nil {
				logutil.S().Warnw("failed to create tags", "error", err)
				os.Exit(1)
			}
			needMkfs = false
		} else {
			logutil.S().Infow("no reusable volume found in AZ, creating a new one")

			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			createdVolID, err := ec2.CreateVolume(
				ctx,
				cfg,
				asgNameTagValue,
				ec2.WithAvailabilityZone(az),
				ec2.WithVolumeType(volType),
				ec2.WithVolumeEncrypted(volEncrypted),
				ec2.WithVolumeSizeInGB(volSizeInGB),
				ec2.WithVolumeIOPS(volIOPS),
				ec2.WithVolumeThroughput(volThroughput),
				ec2.WithTags(map[string]string{
					idTagKey:        idTagValue,
					kindTagKey:      kindTagValue,
					asgNameTagKey:   asgNameTagValue,
					volLeaseHoldKey: volLeaseHoldValue,
				}),
			)
			cancel()
			if err != nil {
				logutil.S().Warnw("failed to create a volume", "error", err)
				os.Exit(1)
			}

			logutil.S().Infow("successfully created a volume", "volumeID", createdVolID)

			ctx, cancel = context.WithTimeout(context.Background(), 5*time.Minute)
			ch := ec2.PollVolume(
				ctx,
				stopc,
				cfg,
				createdVolID,
				ec2.WithInterval(10*time.Second),
				ec2.WithVolumeState(aws_ec2_v2_types.VolumeStateAvailable),
			)
			var volStatus ec2.VolumeStatus
			for volStatus = range ch {
				select {
				case <-ctx.Done():
					logutil.S().Warnw("failed to get volume in time", "error", ctx.Err())
					close(stopc)
					os.Exit(1)
				case sig := <-sigs:
					logutil.S().Warnw("received signal", "signal", sig)
					close(stopc)
					os.Exit(1)
				default:
				}
				logutil.S().Infow("current volume status",
					"volumeID", *volStatus.Volume.VolumeId,
					"state", volStatus.Volume.State,
					"error", volStatus.Error,
				)
			}
			cancel()
			if volStatus.Error != nil || volStatus.Volume.VolumeId == nil {
				logutil.S().Warnw("failed to poll volume", "error", volStatus.Error)
				os.Exit(1)
			}

			describedVols = []aws_ec2_v2_types.Volume{volStatus.Volume}
		}

		attachVolumeID = *describedVols[0].VolumeId
		logutil.S().Infow("attaching the volume", "volumeID", attachVolumeID)

		ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
		err = ec2.AttachVolume(ctx, cfg, attachVolumeID, localInstanceID, ebsDevice)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to attach volume", "error", err)
			os.Exit(1)
		}
	}

	time.Sleep(2 * time.Second)

	ctx, cancel = context.WithTimeout(context.Background(), 5*time.Minute)
	ch := ec2.PollVolume(
		ctx,
		stopc,
		cfg,
		attachVolumeID,
		ec2.WithInterval(10*time.Second),
		ec2.WithVolumeState(aws_ec2_v2_types.VolumeStateInUse),
		ec2.WithVolumeAttachmentState(aws_ec2_v2_types.VolumeAttachmentStateAttached),
	)
	var volStatus ec2.VolumeStatus
	for volStatus = range ch {
		select {
		case <-ctx.Done():
			logutil.S().Warnw("failed to get volume in time", "error", ctx.Err())
			close(stopc)
			os.Exit(1)
		case sig := <-sigs:
			logutil.S().Warnw("received signal", "signal", sig)
			close(stopc)
			os.Exit(1)
		default:
		}
		logutil.S().Infow("current volume status",
			"volumeID", *volStatus.Volume.VolumeId,
			"state", volStatus.Volume.State,
			"error", volStatus.Error,
		)
	}
	cancel()
	if volStatus.Error != nil || volStatus.Volume.VolumeId == nil {
		logutil.S().Warnw("failed to poll volume", "error", volStatus.Error)
		os.Exit(1)
	}

	attachedVolumeID := *volStatus.Volume.VolumeId
	logutil.S().Infow("successfully polled volume", "volumeID", attachedVolumeID)

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	err = ec2.CreateTags(
		ctx,
		cfg,
		[]string{localInstanceID},
		map[string]string{
			localInstancePublishTagKey: attachedVolumeID,
		})
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to create tags", "error", err)
		os.Exit(1)
	}

	if needMkfs {
		logutil.S().Infow("making filesystem", "filesystem", fsName, "blockDevice", blockDevice)
		ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
		b, err := disk.Mkfs(ctx, fsName, blockDevice)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to make filesystem", "error", err)
			os.Exit(1)
		}
		logutil.S().Infow("successfully made filesystem", "output", string(b))
	} else {
		logutil.S().Infow("no need to make filesystem")
	}

	logutil.S().Infow("mkdir", "mountDir", mountDir)
	if err := os.MkdirAll(mountDir, 0755); err != nil {
		logutil.S().Warnw("failed to mkdir", "error", err)
		os.Exit(1)
	}

	logutil.S().Infow("wait a bit before mounting the file system")
	time.Sleep(5 * time.Second)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	blkLs, err := disk.Lsblk(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to lsblk", "error", err)
		os.Exit(1)
	}
	fmt.Println("'lsblk' output:" + "\n\n" + string(blkLs))

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	dfOut, err := disk.Df(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to df", "error", err)
		os.Exit(1)
	}
	fmt.Println("'df' output:" + "\n\n" + string(dfOut))

	ctx, cancel = context.WithTimeout(context.Background(), 15*time.Second)
	b, err := disk.Mount(ctx, fsName, blockDevice, mountDir)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to mount", "error", err)
		os.Exit(1)
	}
	logutil.S().Infow("successfully mounted a filesystem", "output", string(b))

	ctx, cancel = context.WithTimeout(context.Background(), 15*time.Second)
	b, err = disk.UpdateFstab(ctx, fsName, blockDevice, mountDir)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to update fstab", "error", err)
		os.Exit(1)
	}
	logutil.S().Infow("successfully updated fstab", "output", string(b))

	ctx, cancel = context.WithTimeout(context.Background(), 15*time.Second)
	b, err = disk.MountAll(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to mount all filesystems", "error", err)
		os.Exit(1)
	}
	logutil.S().Infow("successfully mounted all", "output", string(b))

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	blkLs, err = disk.Lsblk(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to lsblk", "error", err)
		os.Exit(1)
	}
	fmt.Println("'lsblk' output:" + "\n\n" + string(blkLs))

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	dfOut, err = disk.Df(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to df", "error", err)
		os.Exit(1)
	}
	fmt.Println("'df' output:" + "\n\n" + string(dfOut))

	logutil.S().Infow("writing",
		"volumeID", attachVolumeID,
		"currentEBSVolumeIDFile", curEBSVolIDFile,
	)
	if err := os.WriteFile(curEBSVolIDFile, []byte(attachVolumeID), 0644); err != nil {
		logutil.S().Warnw("failed to write", "error", err)
		os.Exit(1)
	} else {
		logutil.S().Infow("successfully  mounted and provisioned the volume!")
	}
}
