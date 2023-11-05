package ec2

import (
	"context"

	"github.com/gyuho/infra/aws/go/pkg/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

// Lists the EIPs by filter.
// e.g., "tag:Kind" and "tag:Id".
func ListAddresses(ctx context.Context, cfg aws.Config, filters map[string]string) ([]aws_ec2_v2_types.Address, error) {
	logutil.S().Infow("listing eips", "filter", filters)

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
	out, err := cli.DescribeAddresses(ctx, &aws_ec2_v2.DescribeAddressesInput{
		Filters: fts,
	})
	if err != nil {
		return nil, err
	}
	if len(out.Addresses) == 0 {
		logutil.S().Warnw("no address found")
		return nil, nil
	}

	logutil.S().Infow("listed eips", "eips", len(out.Addresses))
	return out.Addresses, nil
}

func ReleaseAddress(ctx context.Context, cfg aws.Config, allocationID string) error {
	logutil.S().Infow("releasing an EIP", "allocationID", allocationID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.ReleaseAddress(ctx, &aws_ec2_v2.ReleaseAddressInput{
		AllocationId: &allocationID,
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully released an EIP", "allocationID", allocationID)
	return nil
}
