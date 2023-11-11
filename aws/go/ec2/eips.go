package ec2

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

// Lists the EIPs by filter.
// e.g., "tag:Kind" and "tag:Id".
func ListEIPs(ctx context.Context, cfg aws.Config, opts ...OpOption) ([]aws_ec2_v2_types.Address, error) {
	ret := &Op{}
	ret.applyOpts(opts)
	logutil.S().Infow("listing eips", "filter", ret.filters)

	input := aws_ec2_v2.DescribeAddressesInput{}
	if len(ret.filters) > 0 {
		input.Filters = make([]aws_ec2_v2_types.Filter, 0, len(ret.filters))
		for k, vs := range ret.filters {
			// TODO: remove this in Go 1.22
			// ref. https://go.dev/blog/loopvar-preview
			k, vs := k, vs
			input.Filters = append(input.Filters, aws_ec2_v2_types.Filter{
				Name:   &k,
				Values: vs,
			})
		}
	}

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeAddresses(ctx, &input)
	if err != nil {
		return nil, err
	}
	if len(out.Addresses) == 0 {
		logutil.S().Warnw("no eip address found")
		return nil, nil
	}

	logutil.S().Infow("listed eips", "eips", len(out.Addresses))
	return out.Addresses, nil
}

func ReleaseEIP(ctx context.Context, cfg aws.Config, allocationID string) error {
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

func AllocateEIP(ctx context.Context, cfg aws.Config, name string, opts ...OpOption) (EIP, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("allocating an EIP", "name", name)

	tags := toTags(name, ret.tags)
	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.AllocateAddress(ctx, &aws_ec2_v2.AllocateAddressInput{
		TagSpecifications: []aws_ec2_v2_types.TagSpecification{
			{
				ResourceType: aws_ec2_v2_types.ResourceTypeElasticIp,
				Tags:         tags,
			},
		},
	})
	if err != nil {
		return EIP{}, err
	}

	eip := EIP{
		AllocationID: *out.AllocationId,
		PublicIP:     *out.PublicIp,
	}
	logutil.S().Infow("successfully allocated an EIP", "eip", eip)
	return eip, nil
}

type EIP struct {
	AllocationID string `json:"allocation_id"`
	PublicIP     string `json:"public_ip"`
}

func (e EIP) Sync(p string) error {
	parentDir := filepath.Dir(p)
	if parentDir != "" && parentDir != "/" {
		if err := os.MkdirAll(filepath.Dir(p), 0755); err != nil {
			return err
		}
	}
	b, err := json.Marshal(e)
	if err != nil {
		return err
	}
	if err := os.WriteFile(p, b, 0644); err != nil {
		return err
	}
	return nil
}

func (e EIP) String() string {
	b, err := json.Marshal(e)
	if err != nil {
		return err.Error()
	}
	return string(b)
}

func LoadEIP(p string) (EIP, error) {
	b, err := os.ReadFile(p)
	if err != nil {
		return EIP{}, err
	}
	var e EIP
	if err := json.Unmarshal(b, &e); err != nil {
		return EIP{}, err
	}
	return e, nil
}

type EIPs []EIP

func (e EIPs) Sync(p string) error {
	parentDir := filepath.Dir(p)
	if parentDir != "" && parentDir != "/" {
		if err := os.MkdirAll(filepath.Dir(p), 0755); err != nil {
			return err
		}
	}
	b, err := json.Marshal(e)
	if err != nil {
		return err
	}
	if err := os.WriteFile(p, b, 0644); err != nil {
		return err
	}
	return nil
}

func (e EIPs) String() string {
	b, err := json.Marshal(e)
	if err != nil {
		return err.Error()
	}
	return string(b)
}

func LoadEIPs(p string) (EIPs, error) {
	b, err := os.ReadFile(p)
	if err != nil {
		return nil, err
	}
	var e EIPs
	if err := json.Unmarshal(b, &e); err != nil {
		return nil, err
	}
	return e, nil
}

// Associates the EIP to the instance.
// It will fail if the EC2 instance has multiple ENIs.
// e.g.,
// "operation error EC2: AssociateAddress, https response error StatusCode: 400, api error InvalidInstanceID:
// There are multiple interfaces attached to instance 'i-...'. Please specify an interface ID for the operation instead."
func AssociateEIPByInstanceID(ctx context.Context, cfg aws.Config, allocationID string, instanceID string) error {
	logutil.S().Infow("associating EIP", "allocationID", allocationID, "instanceID", instanceID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.AssociateAddress(ctx, &aws_ec2_v2.AssociateAddressInput{
		AllocationId:       &allocationID,
		AllowReassociation: aws.Bool(true),
		InstanceId:         &instanceID,
	})
	if err != nil {
		return err
	}
	logutil.S().Infow("successfully associated EIP")
	return nil
}
