package ec2

import (
	"context"

	"github.com/gyuho/infra/aws/go/pkg/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

// Lists the volumes by filter.
// e.g., "tag:Kind" and "tag:Id".
func ListVolumes(ctx context.Context, cfg aws.Config, filters map[string]string) ([]aws_ec2_v2_types.Volume, error) {
	logutil.S().Infow("listing volumes", "filter", filters)

	fts := make([]aws_ec2_v2_types.Filter, 0, len(filters))
	for k, v := range filters {
		k := k
		v := v
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

	logutil.S().Infow("listed volumes", "volumes", len(out.Volumes))
	return out.Volumes, nil
}

func DeleteVolume(ctx context.Context, cfg aws.Config, volumeID string) error {
	logutil.S().Infow("deleting a volume", "volumeID", volumeID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.DeleteVolume(ctx, &aws_ec2_v2.DeleteVolumeInput{
		VolumeId: &volumeID,
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully deleted a volume", "volumeID", volumeID)
	return nil
}
