package ec2

import (
	"context"
	"sort"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

func toTags(name string, m map[string]string) []aws_ec2_v2_types.Tag {
	nameKey := "Name"
	if name != "" {
		m[nameKey] = name
	}
	tags := make([]aws_ec2_v2_types.Tag, 0, len(m))
	for k, v := range m {
		// TODO: remove this in Go 1.22
		// ref. https://go.dev/blog/loopvar-preview
		k, v := k, v
		tags = append(tags, aws_ec2_v2_types.Tag{
			Key:   &k,
			Value: &v,
		})
	}
	sort.SliceStable(tags, func(i, j int) bool {
		return *tags[i].Key < *tags[j].Key
	})
	return tags
}

// Creates tags to the resource.
func CreateTags(ctx context.Context, cfg aws.Config, resource string, tags map[string]string) error {
	logutil.S().Infow("creating tags", "resource", resource, "tags", len(tags))

	ts := toTags("", tags)
	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.CreateTags(ctx, &aws_ec2_v2.CreateTagsInput{
		Resources: []string{resource},
		Tags:      ts,
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully created tags", "resource", resource)
	return nil
}
