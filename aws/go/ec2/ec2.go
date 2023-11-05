// Package ec2 contains ec2 related aws functions.
package ec2

import (
	"context"
	"errors"

	"github.com/gyuho/infra/aws/go/pkg/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

type Op struct {
	expectedInstanceStates map[aws_ec2_v2_types.InstanceStateName]struct{}
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
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

// Fetches the instance by ID.
func GetInstance(ctx context.Context, cfg aws.Config, instanceID string) (aws_ec2_v2_types.Instance, error) {
	logutil.S().Infow("getting instance", "instanceID", instanceID)
	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeInstances(ctx, &aws_ec2_v2.DescribeInstancesInput{
		InstanceIds: []string{instanceID},
	})
	if err != nil {
		return aws_ec2_v2_types.Instance{}, err
	}
	if len(out.Reservations) != 1 {
		logutil.S().Warnw("no instance found", "instanceID", instanceID)
		return aws_ec2_v2_types.Instance{}, errors.New("not found")
	}
	if len(out.Reservations[0].Instances) != 1 {
		logutil.S().Warnw("no instance found", "instanceID", instanceID)
		return aws_ec2_v2_types.Instance{}, errors.New("not found")
	}
	return out.Reservations[0].Instances[0], nil
}
