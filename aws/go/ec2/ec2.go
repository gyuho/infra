// Package ec2 contains ec2 related aws functions.
package ec2

import (
	"context"
	"errors"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

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
