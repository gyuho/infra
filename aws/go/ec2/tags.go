package ec2

import (
	"context"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

// Creates tags to the resource.
func CreateTags(ctx context.Context, cfg aws.Config, resource string, tags map[string]string) error {
	logutil.S().Infow("creating tags", "resource", resource, "tags", len(tags))

	ec2Tags := make([]aws_ec2_v2_types.Tag, 0, len(tags))
	for k, v := range tags {
		k := k
		v := v
		ec2Tags = append(ec2Tags, aws_ec2_v2_types.Tag{
			Key:   &k,
			Value: &v,
		})
	}

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.CreateTags(ctx, &aws_ec2_v2.CreateTagsInput{
		Resources: []string{resource},
		Tags:      ec2Tags,
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully created tags", "resource", resource)
	return nil
}
