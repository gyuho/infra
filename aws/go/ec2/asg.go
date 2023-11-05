package ec2

import (
	"context"
	"sort"
	"time"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

var asgGroupNameTagKey = "tag:aws:autoscaling:groupName"

// Lists all instances in the asg.
func ListInstancesByASG(ctx context.Context, cfg aws.Config, asg string, opts ...OpOption) ([]aws_ec2_v2_types.Instance, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("listing instances", "asg", asg)

	filters := []aws_ec2_v2_types.Filter{
		{
			Name:   &asgGroupNameTagKey,
			Values: []string{asg},
		},
	}

	cli := aws_ec2_v2.NewFromConfig(cfg)
	instances := make([]aws_ec2_v2_types.Instance, 0)
	token := ""
	for {
		input := &aws_ec2_v2.DescribeInstancesInput{
			Filters: filters,
		}
		if token != "" {
			input.NextToken = &token
		}
		out, err := cli.DescribeInstances(ctx, input)
		if err != nil {
			return nil, err
		}
		if len(out.Reservations) == 0 {
			logutil.S().Warnw("no instance found", "asg", asg)
			break
		}
		for _, r := range out.Reservations {
			for _, inst := range r.Instances {
				if ret.expectedInstanceStates == nil {
					instances = append(instances, inst)
					continue
				}
				if _, ok := ret.expectedInstanceStates[inst.State.Name]; ok {
					instances = append(instances, inst)
				}
			}
		}

		if out.NextToken == nil {
			break
		}
		token = *out.NextToken

		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(time.Second):
		}
	}

	sort.SliceStable(instances, func(i, j int) bool {
		return *instances[i].InstanceId < *instances[j].InstanceId
	})
	logutil.S().Infow("listed instances", "asg", asg, "instances", len(instances))

	return instances, nil
}
