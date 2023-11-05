package ec2

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"log"
	"sort"
	"strings"
	"time"

	"github.com/gyuho/infra/aws/go/pkg/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	"github.com/olekukonko/tablewriter"
)

type ENI struct {
	ID               string            `json:"id"`
	Name             string            `json:"name"`
	Description      string            `json:"description"`
	Status           string            `json:"status"`
	AttachmentStatus string            `json:"attachment_status"`
	PrivateIP        string            `json:"private_ip"`
	PrivateDNS       string            `json:"private_dns"`
	VPCID            string            `json:"vpc_id"`
	SubnetID         string            `json:"subnet_id"`
	AvailabilityZone string            `json:"availability_zone"`
	SecurityGroupIDs []string          `json:"security_group_ids"`
	Tags             map[string]string `json:"tags"`
}

func convertENI(raw aws_ec2_v2_types.NetworkInterface) ENI {
	desc := ""
	if raw.Description != nil {
		desc = *raw.Description
	}
	attachmentStatus := ""
	if raw.Attachment != nil {
		attachmentStatus = string(raw.Attachment.Status)
	}
	privateIP := ""
	if raw.PrivateIpAddress != nil {
		privateIP = *raw.PrivateIpAddress
	}
	privateDNS := ""
	if raw.PrivateDnsName != nil {
		privateDNS = *raw.PrivateDnsName
	}

	eni := ENI{
		ID:               *raw.NetworkInterfaceId,
		Description:      desc,
		Status:           string(raw.Status),
		AttachmentStatus: attachmentStatus,
		PrivateIP:        privateIP,
		PrivateDNS:       privateDNS,
		VPCID:            *raw.VpcId,
		SubnetID:         *raw.SubnetId,
		AvailabilityZone: *raw.AvailabilityZone,
	}

	sgs := make([]string, 0, len(raw.Groups))
	for _, sg := range raw.Groups {
		sgs = append(sgs, *sg.GroupId)
	}
	eni.SecurityGroupIDs = sgs

	tags := make(map[string]string, len(raw.TagSet))
	for _, tg := range raw.TagSet {
		if *tg.Key == "Name" {
			eni.Name = *tg.Value
		}
		tags[*tg.Key] = *tg.Value
	}
	eni.Tags = tags

	return eni
}

type ENIs []ENI

func (vss ENIs) String() string {
	sort.SliceStable(vss, func(i, j int) bool {
		if vss[i].VPCID == vss[j].VPCID {
			if vss[i].SubnetID == vss[j].SubnetID {
				if vss[i].Status == vss[j].Status {
					return vss[i].Description < vss[j].Description
				}
				return vss[i].Status < vss[j].Status
			}
			return vss[i].SubnetID < vss[j].SubnetID
		}
		return vss[i].VPCID < vss[j].VPCID
	})

	rows := make([][]string, 0, len(vss))
	for _, v := range vss {
		row := []string{
			v.Name,
			v.ID,
			v.Description,
			v.Status,
			v.PrivateIP,
			v.PrivateDNS,
			v.VPCID,
			v.SubnetID,
			v.AvailabilityZone,
			strings.Join(v.SecurityGroupIDs, ", "),
		}
		rows = append(rows, row)
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"name", "eni id", "eni description", "eni status", "private ip", "private dns", "vpc id", "subnet id", "az", "sgs"})
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}

// Fetches the primary network interface of the EC2 instance.
// Useful for --query "Reservations[0].Instances[0].PrivateIpAddress".
func GetPrimaryENIByInstanceID(ctx context.Context, cfg aws.Config, instanceID string) (eni aws_ec2_v2_types.NetworkInterface, err error) {
	logutil.S().Infow("getting primary ENI", "instanceID", instanceID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeInstances(
		ctx,
		&aws_ec2_v2.DescribeInstancesInput{
			InstanceIds: []string{instanceID},
		},
	)
	if err != nil {
		return aws_ec2_v2_types.NetworkInterface{}, err
	}
	if len(out.Reservations) != 1 {
		return aws_ec2_v2_types.NetworkInterface{}, fmt.Errorf("expected 1 reservation, got %d", len(out.Reservations))
	}

	inst := out.Reservations[0].Instances[0]
	privateIP := *inst.PrivateIpAddress

	var instanceENI aws_ec2_v2_types.InstanceNetworkInterface
	for _, v := range inst.NetworkInterfaces {
		if *v.PrivateIpAddress == privateIP {
			instanceENI = v
		}
	}
	eniID := *instanceENI.NetworkInterfaceId
	if eniID == "" {
		return aws_ec2_v2_types.NetworkInterface{}, fmt.Errorf("could not find ENI for private IP %q and instance ID %q", privateIP, instanceID)
	}

	eniOut, err := cli.DescribeNetworkInterfaces(
		ctx,
		&aws_ec2_v2.DescribeNetworkInterfacesInput{
			NetworkInterfaceIds: []string{eniID},
		},
	)
	if err != nil {
		return aws_ec2_v2_types.NetworkInterface{}, err
	}
	if len(eniOut.NetworkInterfaces) != 1 {
		return aws_ec2_v2_types.NetworkInterface{}, fmt.Errorf("expected 1 ENI, got %d", len(eniOut.NetworkInterfaces))
	}

	eni = eniOut.NetworkInterfaces[0]
	if eni.PrivateIpAddress == nil {
		return aws_ec2_v2_types.NetworkInterface{}, fmt.Errorf("ENI %q has no private IP", eniID)
	}
	if *eni.PrivateIpAddress != privateIP {
		return aws_ec2_v2_types.NetworkInterface{}, fmt.Errorf("ENI %q has private IP %q, expected %q", eniID, *eni.PrivateIpAddress, privateIP)
	}
	return eni, nil
}

// Returns false if the ENI does not exist.
func GetENI(ctx context.Context, cfg aws.Config, eniID string) (ENI, bool, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	out, err := cli.DescribeNetworkInterfaces(ctx,
		&aws_ec2_v2.DescribeNetworkInterfacesInput{
			Filters: []aws_ec2_v2_types.Filter{
				{
					Name:   aws.String("network-interface-id"),
					Values: []string{eniID},
				},
			},
		},
	)
	if err != nil {
		return ENI{}, false, err
	}

	if len(out.NetworkInterfaces) != 1 {
		return ENI{}, false, nil
	}
	return convertENI(out.NetworkInterfaces[0]), true, nil
}

// Returns false if the ENI does not exist.
func GetENIByName(ctx context.Context, cfg aws.Config, name string) (ENI, bool, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	out, err := cli.DescribeNetworkInterfaces(ctx,
		&aws_ec2_v2.DescribeNetworkInterfacesInput{
			Filters: []aws_ec2_v2_types.Filter{
				{
					Name:   aws.String("tag:Name"),
					Values: []string{name},
				},
			},
		},
	)
	if err != nil {
		return ENI{}, false, err
	}

	if len(out.NetworkInterfaces) != 1 {
		return ENI{}, false, nil
	}
	return convertENI(out.NetworkInterfaces[0]), true, nil
}

// List ENIs.
func ListENIs(ctx context.Context, cfg aws.Config) (ENIs, error) {
	logutil.S().Infow("listing ENIs")

	cli := aws_ec2_v2.NewFromConfig(cfg)

	raw := make([]aws_ec2_v2_types.NetworkInterface, 0, 10)
	var nextToken *string = nil
	for i := 0; i < 20; i++ {
		out, err := cli.DescribeNetworkInterfaces(
			ctx,
			&aws_ec2_v2.DescribeNetworkInterfacesInput{
				NextToken: nextToken,
			},
		)
		if err != nil {
			return nil, err
		}

		raw = append(raw, out.NetworkInterfaces...)

		if nextToken == nil {
			// no more resources are available
			break
		}

		// TODO: add wait to prevent api throttle (rate limit)?
	}

	enis := make(ENIs, 0, len(raw))
	for _, v := range raw {
		enis = append(enis, convertENI(v))
	}
	return enis, nil
}

// Creates an ENI for a given subnet and security groups.
func CreateENI(ctx context.Context, cfg aws.Config, name string, desc string, subnetID string, sgIDs ...string) (ENI, error) {
	logutil.S().Infow("creating an ENI", "name", name, "subnetID", subnetID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.CreateNetworkInterface(ctx, &aws_ec2_v2.CreateNetworkInterfaceInput{
		SubnetId:    aws.String(subnetID),
		Groups:      sgIDs,
		Description: aws.String(desc),
		TagSpecifications: []aws_ec2_v2_types.TagSpecification{
			{
				ResourceType: aws_ec2_v2_types.ResourceTypeNetworkInterface,
				Tags: []aws_ec2_v2_types.Tag{
					{
						Key:   aws.String("Name"),
						Value: aws.String(name),
					},
				},
			},
		},
	})
	if err != nil {
		return ENI{}, err
	}

	return convertENI(*out.NetworkInterface), nil
}

func DeleteENI(ctx context.Context, cfg aws.Config, eniID string) error {
	logutil.S().Infow("deleting an ENI", "eniID", eniID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.DeleteNetworkInterface(ctx,
		&aws_ec2_v2.DeleteNetworkInterfaceInput{
			NetworkInterfaceId: aws.String(eniID),
		},
	)
	if eniNotExist(err) {
		err = nil
		logutil.S().Infow("ENI does not exist", "eniID", eniID)
	}
	if err == nil {
		logutil.S().Infow("successfully deleted ENI", "eniID", eniID)
	}
	return err
}

type ENIStatus struct {
	ENI   aws_ec2_v2_types.NetworkInterface
	Error error
}

// Poll periodically fetches the stack status
// until the stack becomes the desired state.
func PollENI(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	eniID string,
	desired aws_ec2_v2_types.NetworkInterfaceStatus,
	desiredAttach aws_ec2_v2_types.AttachmentStatus,
	initialWait time.Duration,
	pollInterval time.Duration,
) <-chan ENIStatus {
	return pollENI(ctx, stopc, cfg, eniID, false, desired, desiredAttach, initialWait, pollInterval)
}

func PollENIDelete(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	eniID string,
	initialWait time.Duration,
	pollInterval time.Duration,
) <-chan ENIStatus {
	return pollENI(ctx, stopc, cfg, eniID, true, aws_ec2_v2_types.NetworkInterfaceStatus(""), aws_ec2_v2_types.AttachmentStatus(""), initialWait, pollInterval)
}

func pollENI(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	eniID string,
	waitForDelete bool,
	desired aws_ec2_v2_types.NetworkInterfaceStatus,
	desiredAttach aws_ec2_v2_types.AttachmentStatus,
	initialWait time.Duration,
	pollInterval time.Duration,
) <-chan ENIStatus {
	now := time.Now()
	cli := aws_ec2_v2.NewFromConfig(cfg)

	ch := make(chan ENIStatus, 10)
	go func() {
		// very first poll should be no-wait
		// in case stack has already reached desired status
		// wait from second interation
		interval := time.Duration(0)

		first := true
		for ctx.Err() == nil {
			select {
			case <-ctx.Done():
				ch <- ENIStatus{Error: ctx.Err()}
				close(ch)
				return

			case <-stopc:
				ch <- ENIStatus{Error: errors.New("wait stopped")}
				close(ch)
				return

			case <-time.After(interval):
				// very first poll should be no-wait
				// in case stack has already reached desired status
				// wait from second interation
				if interval == time.Duration(0) {
					interval = pollInterval
				}
			}

			out, err := cli.DescribeNetworkInterfaces(ctx,
				&aws_ec2_v2.DescribeNetworkInterfacesInput{
					Filters: []aws_ec2_v2_types.Filter{
						{
							Name:   aws.String("network-interface-id"),
							Values: []string{eniID},
						},
					},
				},
			)
			if err != nil {
				if eniNotExist(err) {
					log.Printf("ENI does not exist (%v)", err)
					if waitForDelete {
						ch <- ENIStatus{Error: nil}
						close(ch)
						return
					}
				}
				ch <- ENIStatus{Error: err}
				continue
			}

			if len(out.NetworkInterfaces) != 1 {
				if waitForDelete {
					log.Printf("ENI does not exist")
					ch <- ENIStatus{Error: nil}
					close(ch)
					return
				}

				ch <- ENIStatus{Error: fmt.Errorf("expected only 1, unexpected ENI response %+v", out)}
				continue
			}

			eni := out.NetworkInterfaces[0]
			currentStatus := eni.Status
			currentAttachmentStatus := aws_ec2_v2_types.AttachmentStatus("")
			if eni.Attachment != nil {
				currentAttachmentStatus = eni.Attachment.Status
			}
			log.Printf("fetched ENI %s with status %q and attachment status %q (took %v so far)", eniID, currentStatus, currentAttachmentStatus, time.Since(now))

			ch <- ENIStatus{ENI: eni, Error: nil}
			if desired == currentStatus && desiredAttach == currentAttachmentStatus {
				close(ch)
				return
			}

			if first {
				select {
				case <-ctx.Done():
					ch <- ENIStatus{Error: ctx.Err()}
					close(ch)
					return
				case <-stopc:
					ch <- ENIStatus{Error: errors.New("wait stopped")}
					close(ch)
					return
				case <-time.After(initialWait):
				}
				first = false
			}

			// continue for-loop
		}
		ch <- ENIStatus{Error: ctx.Err()}
		close(ch)
	}()
	return ch
}

func eniNotExist(err error) bool {
	if err == nil {
		return false
	}
	return strings.Contains(err.Error(), " does not exist")
}
