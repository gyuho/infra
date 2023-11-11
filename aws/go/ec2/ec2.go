// Package ec2 contains ec2 related aws functions.
package ec2

import (
	"context"
	"errors"
	"os"
	"time"

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

// Waits until the instance has the expected tag key, and returns the value
func WaitInstanceTagValue(ctx context.Context, cfg aws.Config, instanceID string, tagKey string) (aws_ec2_v2_types.Instance, string, error) {
	logutil.S().Infow("waiting for instance tag value", "instanceID", instanceID, "tagKey", tagKey)
	var instance aws_ec2_v2_types.Instance
	tagValue := ""
	for ctx.Err() == nil {
		select {
		case <-ctx.Done():
			return aws_ec2_v2_types.Instance{}, "", ctx.Err()
		case <-time.After(10 * time.Second):
		}

		var err error
		instance, err = GetInstance(ctx, cfg, instanceID)
		if err != nil {
			logutil.S().Warnw("failed to get instance", "error", err)
			os.Exit(1)
		}

		for _, tag := range instance.Tags {
			k, v := *tag.Key, *tag.Value
			logutil.S().Infow("found instance tag", "key", k, "value", v)
			if k == tagKey { // e.g., aws:autoscaling:groupName
				tagValue = v
				break
			}
		}

		if tagValue != "" {
			break
		}
	}
	if tagValue == "" {
		return instance, "", errors.New("failed to get tag value in time")
	}
	return instance, tagValue, nil
}
