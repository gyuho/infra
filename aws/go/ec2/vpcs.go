package ec2

import (
	"bytes"
	"context"
	"fmt"
	"sort"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	"github.com/olekukonko/tablewriter"
)

type Subnet struct {
	VPCID            string            `json:"vpc_id"`
	ID               string            `json:"id"`
	Name             string            `json:"name"`
	AvailabilityZone string            `json:"availability_zone"`
	State            string            `json:"state"`
	CIDRBlock        string            `json:"cidr_block"`
	Tags             map[string]string `json:"tags"`
}

type Subnets []Subnet

func GetSubnet(ctx context.Context, cfg aws.Config, subnetID string) (Subnet, error) {
	logutil.S().Infow("getting subnet", "subnetID", subnetID)

	cli := aws_ec2_v2.NewFromConfig(cfg)

	out, err := cli.DescribeSubnets(ctx,
		&aws_ec2_v2.DescribeSubnetsInput{
			SubnetIds: []string{subnetID},
		},
	)
	if err != nil {
		return Subnet{}, err
	}
	if len(out.Subnets) != 1 {
		return Subnet{}, fmt.Errorf("expected 1 Subnet, got %d", len(out.Subnets))
	}
	sb := out.Subnets[0]

	tags := make(map[string]string, len(sb.Tags))
	subnet := Subnet{
		VPCID:            *sb.VpcId,
		ID:               *sb.SubnetId,
		Name:             "",
		AvailabilityZone: *sb.AvailabilityZone,
		State:            string(sb.State),
		CIDRBlock:        *sb.CidrBlock,
		Tags:             tags,
	}
	for _, tg := range sb.Tags {
		subnet.Tags[*tg.Key] = *tg.Value
		if *tg.Key == "Name" {
			subnet.Name = *tg.Value
		}
	}
	return subnet, nil
}

type VPC struct {
	ID      string            `json:"id"`
	Name    string            `json:"name"`
	State   string            `json:"state"`
	Subnets Subnets           `json:"subnets"`
	Tags    map[string]string `json:"tags"`
}

type VPCs []VPC

func (vss VPCs) String() string {
	sort.SliceStable(vss, func(i, j int) bool {
		if vss[i].Name == vss[j].Name {
			return vss[i].ID < vss[j].ID
		}
		return vss[i].Name < vss[j].Name
	})

	rows := make([][]string, 0, len(vss))
	for _, v := range vss {
		sort.SliceStable(v.Subnets, func(i, j int) bool {
			if v.Subnets[i].Name == v.Subnets[j].Name {
				return v.Subnets[i].AvailabilityZone < v.Subnets[j].AvailabilityZone
			}
			return v.Subnets[i].Name < v.Subnets[j].Name
		})

		for _, s := range v.Subnets {
			row := []string{
				v.ID,
				v.Name,
				v.State,
				s.ID,
				s.Name,
				s.AvailabilityZone,
				s.State,
			}
			rows = append(rows, row)
		}
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"vpc id", "vpc name", "vpc state", "subnet id", "subnet name", "subnet az", "subnet state"})
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}

// List VPCs.
func ListVPCs(ctx context.Context, cfg aws.Config) (VPCs, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	raw := make([]aws_ec2_v2_types.Vpc, 0, 10)
	var nextToken *string = nil
	for i := 0; i < 20; i++ {
		out, err := cli.DescribeVpcs(ctx,
			&aws_ec2_v2.DescribeVpcsInput{
				NextToken: nextToken,
			},
		)
		if err != nil {
			return nil, err
		}

		raw = append(raw, out.Vpcs...)

		if nextToken == nil {
			// no more resources are available
			break
		}

		// TODO: add wait to prevent api throttle (rate limit)?
	}

	vpcs := make(VPCs, 0, len(raw))
	for _, v := range raw {
		vpc, err := GetVPC(ctx, cfg, *v.VpcId)
		if err != nil {
			return nil, err
		}
		vpcs = append(vpcs, vpc)
	}

	sort.SliceStable(vpcs, func(i, j int) bool {
		if vpcs[i].Name == vpcs[j].Name {
			return vpcs[i].ID < vpcs[j].ID
		}
		return vpcs[i].Name < vpcs[j].Name
	})
	return vpcs, nil
}

func GetVPC(ctx context.Context, cfg aws.Config, vpcID string) (VPC, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	out, err := cli.DescribeVpcs(ctx,
		&aws_ec2_v2.DescribeVpcsInput{
			Filters: []aws_ec2_v2_types.Filter{
				{
					Name:   aws.String("vpc-id"),
					Values: []string{vpcID},
				},
			},
		},
	)
	if err != nil {
		return VPC{}, err
	}
	if len(out.Vpcs) != 1 {
		return VPC{}, fmt.Errorf("expected 1 VPC, got %d", len(out.Vpcs))
	}
	vp := out.Vpcs[0]

	vpc := VPC{
		ID:      *vp.VpcId,
		Name:    "",
		State:   string(vp.State),
		Subnets: nil,
	}
	tags := make(map[string]string, len(vp.Tags))
	for _, tg := range vp.Tags {
		tags[*tg.Key] = *tg.Value
		if *tg.Key == "Name" {
			vpc.Name = *tg.Value
		}
	}
	vpc.Tags = tags

	out2, err := cli.DescribeSubnets(ctx,
		&aws_ec2_v2.DescribeSubnetsInput{
			Filters: []aws_ec2_v2_types.Filter{
				{
					Name:   aws.String("vpc-id"),
					Values: []string{vpc.ID},
				},
			},
		},
	)
	if err != nil {
		return VPC{}, err
	}

	for _, s := range out2.Subnets {
		subnetName := ""
		tags := make(map[string]string, len(vp.Tags))
		for _, tg := range s.Tags {
			tags[*tg.Key] = *tg.Value
			if *tg.Key == "Name" {
				subnetName = *tg.Value
			}
		}
		vpc.Subnets = append(vpc.Subnets, Subnet{
			VPCID:            *s.VpcId,
			ID:               *s.SubnetId,
			Name:             subnetName,
			AvailabilityZone: *s.AvailabilityZone,
			State:            string(s.State),
			CIDRBlock:        *s.CidrBlock,
			Tags:             tags,
		})
	}

	sort.SliceStable(vpc.Subnets, func(i, j int) bool {
		if vpc.Subnets[i].Name == vpc.Subnets[j].Name {
			return vpc.Subnets[i].AvailabilityZone < vpc.Subnets[j].AvailabilityZone
		}
		return vpc.Subnets[i].Name < vpc.Subnets[j].Name
	})
	return vpc, nil
}
