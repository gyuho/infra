package ec2

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/dustin/go-humanize"
	"github.com/gyuho/infra/go/ctxutil"
	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

// Describes the volumes by filter.
// e.g., "tag:Kind" and "tag:Id".
func DescribeVolumes(ctx context.Context, cfg aws.Config, filters map[string]string) ([]aws_ec2_v2_types.Volume, error) {
	logutil.S().Infow("listing volumes", "filter", filters)

	fts := make([]aws_ec2_v2_types.Filter, 0, len(filters))
	for k, v := range filters {
		// TODO: remove this in Go 1.22
		// ref. https://go.dev/blog/loopvar-preview
		k, v := k, v
		fts = append(fts, aws_ec2_v2_types.Filter{
			Name:   &k,
			Values: []string{v},
		})
	}

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeVolumes(ctx, &aws_ec2_v2.DescribeVolumesInput{
		Filters: fts,
	})
	if err != nil {
		return nil, err
	}
	if len(out.Volumes) == 0 {
		logutil.S().Warnw("no volume found")
		return nil, nil
	}

	logutil.S().Infow("described volumes", "volumes", len(out.Volumes))
	return out.Volumes, nil
}

// Creates the volume.
func CreateVolume(ctx context.Context, cfg aws.Config, name string, opts ...OpOption) (string, error) {
	ret := &Op{
		availabilityZone: cfg.Region + "a",
		volumeType:       "gp3",
		volumeEncrypted:  true,
		volumeSizeInGB:   300,
		volumeIOPS:       3000,
		volumeThroughput: 500,
	}
	ret.applyOpts(opts)

	volType := aws_ec2_v2_types.VolumeType(ret.volumeType)
	switch volType {
	case aws_ec2_v2_types.VolumeTypeIo1,
		aws_ec2_v2_types.VolumeTypeIo2,
		aws_ec2_v2_types.VolumeTypeGp2,
		aws_ec2_v2_types.VolumeTypeGp3:
	default:
		return "", fmt.Errorf("invalid ec2 volume type %q", ret.volumeType)
	}

	if name == "" {
		return "", errors.New("volume name must be set")
	}
	if ret.volumeSizeInGB == 0 {
		return "", errors.New("volumeSizeInGB must be set")
	}
	if ret.volumeIOPS == 0 {
		return "", errors.New("volumeIOPS must be set")
	}
	if ret.volumeThroughput == 0 {
		return "", errors.New("volumeThroughput must be set")
	}

	logutil.S().Infow("creating a volume",
		"name", name,
		"type", ret.volumeType,
		"encrypted", ret.volumeEncrypted,
		"sizeInGB", ret.volumeSizeInGB,
		"iops", ret.volumeIOPS,
		"throughput", ret.volumeThroughput,
	)

	input := aws_ec2_v2.CreateVolumeInput{
		VolumeType:       volType,
		AvailabilityZone: &ret.availabilityZone,
		Encrypted:        &ret.volumeEncrypted,
		Size:             &ret.volumeSizeInGB,
		Iops:             &ret.volumeIOPS,
		Throughput:       &ret.volumeThroughput,
	}

	tags := make(map[string]string, len(ret.tags))
	tags["Name"] = name
	for k, v := range ret.tags {
		tags[k] = v
	}
	volTags := make([]aws_ec2_v2_types.Tag, 0, len(tags))
	for k, v := range tags {
		// TODO: remove this in Go 1.22
		// ref. https://go.dev/blog/loopvar-preview
		k, v := k, v
		logutil.S().Infow("adding a tag to the volume", "key", k, "value", v)
		volTags = append(volTags, aws_ec2_v2_types.Tag{
			Key:   &k,
			Value: &v,
		})
	}
	input.TagSpecifications = []aws_ec2_v2_types.TagSpecification{
		{
			ResourceType: aws_ec2_v2_types.ResourceTypeVolume,
			Tags:         volTags,
		},
	}

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.CreateVolume(ctx, &input)
	if err != nil {
		return "", err
	}
	if out.VolumeId == nil {
		return "", errors.New("volumeID is nil")
	}
	volID := *out.VolumeId

	logutil.S().Infow("successfully created a volume", "volumeID", volID)
	return volID, nil
}

// Deletes the volume.
func DeleteVolume(ctx context.Context, cfg aws.Config, volumeID string) error {
	logutil.S().Infow("deleting volume", "volumeID", volumeID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.DeleteVolume(ctx, &aws_ec2_v2.DeleteVolumeInput{
		VolumeId: &volumeID,
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully deleted volume", "volumeID", volumeID)
	return nil
}

type VolumeStatus struct {
	Volume aws_ec2_v2_types.Volume
	Error  error
}

// Polls the volume by its state.
func PollVolume(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	volumeID string,
	opts ...OpOption,
) <-chan VolumeStatus {
	ret := &Op{}
	ret.applyOpts(opts)

	ch := make(chan VolumeStatus, 10)

	if ret.volumeState == "" {
		ch <- VolumeStatus{Error: errors.New("empty volume state")}
		close(ch)
		return ch
	}

	now := time.Now()

	logutil.S().Infow("polling volume",
		"volumeID", volumeID,
		"volumeState", string(ret.volumeState),
		"volumeAttachmentState", string(ret.volumeAttachmentState),
		"pollInterval", ret.interval.String(),
		"ctxTimeLeft", ctxutil.TimeLeftTillDeadline(ctx),
	)

	go func() {
		// very first poll should be no-wait
		// in case stack has already reached desired status
		// wait from second interation
		interval := time.Duration(0)

		for ctx.Err() == nil {
			select {
			case <-ctx.Done():
				logutil.S().Warnw("wait aborted, ctx done", "err", ctx.Err())
				ch <- VolumeStatus{Error: ctx.Err()}
				close(ch)
				return

			case <-stopc:
				logutil.S().Warnw("wait stopped, stopc closed", "err", ctx.Err())
				ch <- VolumeStatus{Error: errors.New("wait stopped")}
				close(ch)
				return

			case <-time.After(interval):
				// very first poll should be no-wait
				// in case stack has already reached desired status
				// wait from second interation
				if interval == time.Duration(0) {
					interval = ret.interval
				}
			}

			vols, err := DescribeVolumes(ctx, cfg, map[string]string{"volume-id": volumeID})
			if err != nil {
				// TODO: handle error when volume does not exist
				logutil.S().Warnw("describe volume failed; retrying", "err", err)
				ch <- VolumeStatus{Error: err}
				continue
			}

			if len(vols) == 0 {
				if ret.volumeState == aws_ec2_v2_types.VolumeStateDeleted {
					logutil.S().Infow("volume is already deleted as desired; exiting", "err", err)
					ch <- VolumeStatus{Error: nil}
					close(ch)
					return
				}
			}

			if len(vols) != 1 {
				logutil.S().Warnw("expected only 1 volume; retrying", "volumes", fmt.Sprintf("%v", vols))
				ch <- VolumeStatus{Error: fmt.Errorf("unexpected volume response %+v", vols)}
				continue
			}

			vol := vols[0]
			curState := vol.State

			logutil.S().Infow("poll",
				"volume", volumeID,
				"desiredState", string(ret.volumeState),
				"currentState", string(curState),
				"started", humanize.RelTime(now, time.Now(), "ago", "from now"),
				"ctxTimeLeft", ctxutil.TimeLeftTillDeadline(ctx),
			)

			if curState != ret.volumeState {
				ch <- VolumeStatus{Volume: vol, Error: nil}
				continue
			}

			if ret.volumeAttachmentState == "" {
				logutil.S().Infow("desired volume state; done", "state", string(curState))
				ch <- VolumeStatus{Volume: vol, Error: nil}
				close(ch)
				return
			}

			if len(vol.Attachments) != 1 {
				logutil.S().Warnw("expected 1 attachment; retrying", "attachments", len(vol.Attachments))
				ch <- VolumeStatus{Volume: vol, Error: fmt.Errorf("unexpected attachment response %+v", vol.Attachments)}
				continue
			}

			attachment := vol.Attachments[0]
			curAttach := attachment.State

			if curAttach == ret.volumeAttachmentState {
				logutil.S().Infow(
					"desired volume and attachment state; done",
					"state", string(curState),
					"attachmentState", string(curAttach),
				)
				ch <- VolumeStatus{Volume: vol, Error: nil}
				close(ch)
				return
			}

			ch <- VolumeStatus{Volume: vol, Error: nil}
		}

		logutil.S().Warnw("wait aborted, ctx done", "err", ctx.Err())
		ch <- VolumeStatus{Error: ctx.Err()}
		close(ch)
	}()
	return ch
}
