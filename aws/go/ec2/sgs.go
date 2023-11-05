package ec2

import (
	"bytes"
	"context"
	"fmt"
	"sort"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	"github.com/olekukonko/tablewriter"
)

type SG struct {
	VPCID       string            `json:"vpc_id"`
	ID          string            `json:"id"`
	Name        string            `json:"name"`
	Description string            `json:"description"`
	Tags        map[string]string `json:"tags"`
}

type SGs []SG

var SGsCols = []string{"vpc id", "sg id", "sg name", "sg description"}

func (sgss SGs) String() string {
	sort.SliceStable(sgss, func(i, j int) bool {
		if sgss[i].VPCID == sgss[j].VPCID {
			if sgss[i].Name == sgss[j].Name {
				return sgss[i].Description < sgss[j].Description
			}
			return sgss[i].Name < sgss[j].Name
		}
		return sgss[i].VPCID < sgss[j].VPCID
	})

	rows := make([][]string, 0, len(sgss))
	for _, v := range sgss {
		rows = append(rows, []string{
			v.VPCID,
			v.ID,
			v.Name,
			v.Description,
		})
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader(SGsCols)
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}

func ListSGsByVPC(ctx context.Context, cfg aws.Config, vpcID string) (SGs, error) {
	return ListSGs(ctx, cfg, aws_ec2_v2_types.Filter{
		Name:   aws.String("vpc-id"),
		Values: []string{vpcID},
	})
}

// List security groups.
func ListSGs(ctx context.Context, cfg aws.Config, filters ...aws_ec2_v2_types.Filter) (SGs, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	ss := make([]aws_ec2_v2_types.SecurityGroup, 0, 10)
	var nextToken *string = nil
	for i := 0; i < 20; i++ {
		out, err := cli.DescribeSecurityGroups(ctx,
			&aws_ec2_v2.DescribeSecurityGroupsInput{
				NextToken: nextToken,
				Filters:   filters,
			},
		)
		if err != nil {
			return nil, err
		}

		ss = append(ss, out.SecurityGroups...)

		if nextToken == nil {
			// no more resources are available
			break
		}

		// TODO: add wait to prevent api throttle (rate limit)?
	}

	sgs := make(SGs, 0, len(ss))
	for _, sg := range ss {
		tags := make(map[string]string, len(sg.Tags))
		for _, tg := range sg.Tags {
			tags[*tg.Key] = *tg.Value
		}
		sgs = append(sgs, SG{
			VPCID:       *sg.VpcId,
			ID:          *sg.GroupId,
			Name:        *sg.GroupName,
			Description: *sg.Description,
			Tags:        tags,
		})
	}
	return sgs, nil
}

func GetSG(ctx context.Context, cfg aws.Config, sgID string) (SG, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	out, err := cli.DescribeSecurityGroups(ctx,
		&aws_ec2_v2.DescribeSecurityGroupsInput{
			Filters: []aws_ec2_v2_types.Filter{
				{
					Name:   aws.String("group-id"),
					Values: []string{sgID},
				},
			},
		},
	)
	if err != nil {
		return SG{}, err
	}

	if len(out.SecurityGroups) != 1 {
		return SG{}, fmt.Errorf("expected 1 security group, got %d", len(out.SecurityGroups))
	}
	sg := out.SecurityGroups[0]

	tags := make(map[string]string, len(sg.Tags))
	for _, tg := range sg.Tags {
		tags[*tg.Key] = *tg.Value
	}
	return SG{
		VPCID:       *sg.VpcId,
		ID:          *sg.GroupId,
		Name:        *sg.GroupName,
		Description: *sg.Description,
		Tags:        tags,
	}, nil
}
