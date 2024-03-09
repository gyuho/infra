package ec2

import (
	"time"

	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

type Op struct {
	availabilityZone      string
	desc                  string
	eniIDs                []string
	filters               map[string][]string
	instanceStates        map[aws_ec2_v2_types.InstanceStateName]struct{}
	interval              time.Duration
	overwrite             bool
	tags                  map[string]string
	volumeAttachmentState aws_ec2_v2_types.VolumeAttachmentState
	volumeEncrypted       bool
	volumeIOPS            int32
	volumeSizeInGB        int32
	volumeState           aws_ec2_v2_types.VolumeState
	volumeThroughput      int32
	volumeType            string
	retryErrFunc          func(error) bool
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

func WithDescription(v string) OpOption {
	return func(op *Op) {
		op.desc = v
	}
}

func WithENIIDs(ss []string) OpOption {
	return func(op *Op) {
		op.eniIDs = ss
	}
}

func WithFilters(filters map[string][]string) OpOption {
	return func(op *Op) {
		op.filters = filters
	}
}

func WithInstanceState(s aws_ec2_v2_types.InstanceStateName) OpOption {
	return func(op *Op) {
		if op.instanceStates == nil {
			op.instanceStates = make(map[aws_ec2_v2_types.InstanceStateName]struct{})
		}
		op.instanceStates[s] = struct{}{}
	}
}

func WithInterval(v time.Duration) OpOption {
	return func(op *Op) {
		op.interval = v
	}
}

func WithOverwrite(b bool) OpOption {
	return func(op *Op) {
		op.overwrite = b
	}
}

func WithTags(m map[string]string) OpOption {
	return func(op *Op) {
		op.tags = m
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

func WithRetryErrFunc(f func(error) bool) OpOption {
	return func(op *Op) {
		op.retryErrFunc = f
	}
}
