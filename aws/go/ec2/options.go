package ec2

import (
	"time"

	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

type Op struct {
	availabilityZone string

	volumeType       string
	volumeEncrypted  bool
	volumeSizeInGB   int32
	volumeIOPS       int32
	volumeThroughput int32

	volumeState           aws_ec2_v2_types.VolumeState
	volumeAttachmentState aws_ec2_v2_types.VolumeAttachmentState

	interval time.Duration

	desc string
	tags map[string]string

	expectedInstanceStates map[aws_ec2_v2_types.InstanceStateName]struct{}
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithAvailabilityZone(az string) OpOption {
	return func(op *Op) {
		op.availabilityZone = az
	}
}

func WithVolumeType(v string) OpOption {
	return func(op *Op) {
		op.volumeType = v
	}
}

func WithVolumeEncrypted(e bool) OpOption {
	return func(op *Op) {
		op.volumeEncrypted = e
	}
}

func WithVolumeSizeInGB(v int32) OpOption {
	return func(op *Op) {
		op.volumeSizeInGB = v
	}
}

func WithVolumeIOPS(v int32) OpOption {
	return func(op *Op) {
		op.volumeIOPS = v
	}
}

func WithVolumeThroughput(v int32) OpOption {
	return func(op *Op) {
		op.volumeThroughput = v
	}
}

func WithVolumeState(v aws_ec2_v2_types.VolumeState) OpOption {
	return func(op *Op) {
		op.volumeState = v
	}
}

func WithVolumeAttachmentState(v aws_ec2_v2_types.VolumeAttachmentState) OpOption {
	return func(op *Op) {
		op.volumeAttachmentState = v
	}
}

func WithInterval(v time.Duration) OpOption {
	return func(op *Op) {
		op.interval = v
	}
}

func WithDescription(v string) OpOption {
	return func(op *Op) {
		op.desc = v
	}
}

func WithTags(m map[string]string) OpOption {
	return func(op *Op) {
		op.tags = m
	}
}

func WithInstanceState(s aws_ec2_v2_types.InstanceStateName) OpOption {
	return func(op *Op) {
		if op.expectedInstanceStates == nil {
			op.expectedInstanceStates = make(map[aws_ec2_v2_types.InstanceStateName]struct{})
		}
		op.expectedInstanceStates[s] = struct{}{}
	}
}
