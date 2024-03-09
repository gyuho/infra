package ec2

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"sort"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	"github.com/gyuho/infra/go/logutil"
	"github.com/olekukonko/tablewriter"
)

type ENI struct {
	ID                         string            `json:"id,omitempty"`
	Name                       string            `json:"name,omitempty"`
	Description                string            `json:"description,omitempty"`
	Status                     string            `json:"status,omitempty"`
	AttachedEC2InstanceID      string            `json:"attached_ec2_instance_id,omitempty"`
	AttachmentID               string            `json:"attachment_id,omitempty"`
	AttachmentStatus           string            `json:"attachment_status,omitempty"`
	AttachmentDeviceIndex      int32             `json:"attachment_device_index,omitempty"`
	AttachmentNetworkCardIndex int32             `json:"attachment_network_card_index,omitempty"`
	PrivateIP                  string            `json:"private_ip,omitempty"`
	PrivateDNS                 string            `json:"private_dns,omitempty"`
	VPCID                      string            `json:"vpc_id,omitempty"`
	SubnetID                   string            `json:"subnet_id,omitempty"`
	AvailabilityZone           string            `json:"availability_zone,omitempty"`
	SecurityGroupIDs           []string          `json:"security_group_ids,omitempty"`
	Tags                       map[string]string `json:"tags,omitempty"`
}

func ConvertENI(raw aws_ec2_v2_types.NetworkInterface) ENI {
	desc := ""
	if raw.Description != nil {
		desc = *raw.Description
	}
	attachedEC2InstanceID := ""
	attachmentID := ""
	if raw.Attachment != nil {
		if raw.Attachment.InstanceId != nil {
			attachedEC2InstanceID = *raw.Attachment.InstanceId
		}
		attachmentID = *raw.Attachment.AttachmentId
	}
	attachmentStatus := ""
	if raw.Attachment != nil {
		attachmentStatus = string(raw.Attachment.Status)
	}
	attachmentDeviceIndex := int32(0)
	if raw.Attachment != nil && raw.Attachment.DeviceIndex != nil {
		attachmentDeviceIndex = *raw.Attachment.DeviceIndex
	}
	attachmentNetworkCardIndex := int32(0)
	if raw.Attachment != nil && raw.Attachment.NetworkCardIndex != nil {
		attachmentNetworkCardIndex = *raw.Attachment.NetworkCardIndex
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
		ID:                         *raw.NetworkInterfaceId,
		Description:                desc,
		Status:                     string(raw.Status),
		AttachedEC2InstanceID:      attachedEC2InstanceID,
		AttachmentID:               attachmentID,
		AttachmentStatus:           attachmentStatus,
		AttachmentDeviceIndex:      attachmentDeviceIndex,
		AttachmentNetworkCardIndex: attachmentNetworkCardIndex,
		PrivateIP:                  privateIP,
		PrivateDNS:                 privateDNS,
		VPCID:                      *raw.VpcId,
		SubnetID:                   *raw.SubnetId,
		AvailabilityZone:           *raw.AvailabilityZone,
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

func (enis ENIs) ToMap() map[string]ENI {
	m := make(map[string]ENI, len(enis))
	for _, eni := range enis {
		m[eni.ID] = eni
	}
	return m
}

func (enis ENIs) Sort() {
	sort.SliceStable(enis, func(i, j int) bool {
		if enis[i].VPCID == enis[j].VPCID {
			if enis[i].SubnetID == enis[j].SubnetID {
				if enis[i].Status == enis[j].Status {
					if enis[i].AttachmentDeviceIndex == enis[j].AttachmentDeviceIndex {
						return enis[i].AttachmentNetworkCardIndex < enis[j].AttachmentNetworkCardIndex
					}
					return enis[i].AttachmentDeviceIndex < enis[j].AttachmentDeviceIndex
				}
				return enis[i].Status < enis[j].Status
			}
			return enis[i].SubnetID < enis[j].SubnetID
		}
		return enis[i].VPCID < enis[j].VPCID
	})
}

func (vss ENIs) String() string {
	rows := make([][]string, 0, len(vss))
	for _, v := range vss {
		tags := "{}"
		if len(v.Tags) > 0 {
			b, err := json.Marshal(v.Tags)
			if err == nil {
				tags = string(b)
			} else {
				tags = fmt.Sprintf("error: %v", err)
			}
		}
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
			tags,
		}
		rows = append(rows, row)
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"name", "eni id", "eni description", "eni status", "private ip", "private dns", "vpc id", "subnet id", "az", "sgs", "tags"})
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}

// List ENIs.
func ListENIs(ctx context.Context, cfg aws.Config, opts ...OpOption) (ENIs, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("listing ENIs",
		"eniIDs", len(ret.eniIDs),
		"filters", ret.filters,
		"tags", ret.tags,
	)

	// ref. https://pkg.go.dev/github.com/aws/aws-sdk-go-v2/service/ec2#DescribeNetworkInterfacesInput
	input := aws_ec2_v2.DescribeNetworkInterfacesInput{}
	if len(ret.eniIDs) > 0 {
		input.NetworkInterfaceIds = ret.eniIDs
	} else if len(ret.filters) > 0 {
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

	raw := make([]aws_ec2_v2_types.NetworkInterface, 0, 10)
	var nextToken *string = nil
	for i := 0; i < 20; i++ {
		copied := input
		copied.NextToken = nextToken
		out, err := cli.DescribeNetworkInterfaces(ctx, &copied)
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
		enis = append(enis, ConvertENI(v))
	}

	if len(ret.tags) > 0 {
		logutil.S().Infow("non-zero tags specified -- filtering ENIs with subset rule", "total", len(enis), "tags", ret.tags)
		filteredENIs := make(ENIs, 0, len(raw))
		for _, eni := range enis {
			if CheckENITags(eni, ret.tags) {
				filteredENIs = append(filteredENIs, eni)
			}
		}
		enis = filteredENIs
	}

	logutil.S().Infow("listed ENIs", "enis", len(enis))
	return enis, nil
}

// CheckENITags checks if the ENI has the expected tags as a "subset".
func CheckENITags(eni ENI, tags map[string]string) bool {
	if len(tags) == 0 { // nothing to check
		return true
	}

	for k, expected := range tags {
		cur, exists := eni.Tags[k]
		if exists && cur == expected {
			continue
		}
		return false
	}
	return true
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
	return ConvertENI(out.NetworkInterfaces[0]), true, nil
}

func GetENIByTagKey(ctx context.Context, cfg aws.Config, tagKey string, tagValue string) (ENI, bool, error) {
	cli := aws_ec2_v2.NewFromConfig(cfg)

	out, err := cli.DescribeNetworkInterfaces(ctx,
		&aws_ec2_v2.DescribeNetworkInterfacesInput{
			Filters: []aws_ec2_v2_types.Filter{
				{
					Name:   aws.String("tag:" + tagKey),
					Values: []string{tagValue},
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
	return ConvertENI(out.NetworkInterfaces[0]), true, nil
}

// Returns false if the ENI does not exist.
func GetENIByName(ctx context.Context, cfg aws.Config, name string) (ENI, bool, error) {
	return GetENIByTagKey(ctx, cfg, "Name", name)
}

// Returns the same order of EC2 attachment index.
func GetENIsByInstanceID(ctx context.Context, cfg aws.Config, instanceID string) (ENIs, error) {
	logutil.S().Infow("getting ENIs by instance ID", "instanceID", instanceID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeInstances(
		ctx,
		&aws_ec2_v2.DescribeInstancesInput{
			InstanceIds: []string{instanceID},
		},
	)
	if err != nil {
		return nil, err
	}
	if len(out.Reservations) != 1 {
		return nil, fmt.Errorf("expected 1 reservation, got %d", len(out.Reservations))
	}

	inst := out.Reservations[0].Instances[0]
	enis := make(ENIs, 0, len(inst.NetworkInterfaces))
	for _, v := range inst.NetworkInterfaces {
		eniID := *v.NetworkInterfaceId
		eni, exists, err := GetENI(ctx, cfg, eniID)
		if err != nil {
			return nil, err
		}
		if !exists {
			return nil, fmt.Errorf("eni %q attached in EC2 but does not exist", eniID)
		}
		enis = append(enis, eni)
	}
	return enis, nil
}

// Creates an ENI for a given subnet and security groups.
func CreateENI(ctx context.Context, cfg aws.Config, name string, subnetID string, sgIDs []string, opts ...OpOption) (ENI, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	tags := ConvertTags(name, ret.tags)
	logutil.S().Infow("creating an ENI", "name", name, "subnetID", subnetID, "securityGroupIDs", sgIDs, "tags", tags)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.CreateNetworkInterface(ctx, &aws_ec2_v2.CreateNetworkInterfaceInput{
		SubnetId:    aws.String(subnetID),
		Groups:      sgIDs,
		Description: aws.String(ret.desc),
		TagSpecifications: []aws_ec2_v2_types.TagSpecification{
			{
				ResourceType: aws_ec2_v2_types.ResourceTypeNetworkInterface,
				Tags:         tags,
			},
		},
	})
	if err != nil {
		return ENI{}, err
	}

	return ConvertENI(*out.NetworkInterface), nil
}

// Returns true if it's deleted. Returns false if it's already deleted.
func DeleteENI(ctx context.Context, cfg aws.Config, eniID string, opts ...OpOption) (bool, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("deleting ENI", "eniID", eniID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.DeleteNetworkInterface(ctx,
		&aws_ec2_v2.DeleteNetworkInterfaceInput{
			NetworkInterfaceId: aws.String(eniID),
		},
	)
	deleted := false
	if eniNotExist(err) {
		err = nil
		logutil.S().Infow("ENI does not exist", "eniID", eniID)
	}
	if err == nil {
		deleted = true
		logutil.S().Infow("successfully deleted ENI", "eniID", eniID)
	} else {
		if ret.retryErrFunc != nil && ret.retryErrFunc(err) {
			logutil.S().Infow("retriable error", "eniID", eniID, "error", err)
			time.Sleep(time.Second)
			return DeleteENI(ctx, cfg, eniID, opts...)
		}
	}
	return deleted, err
}

// Returns true if it's deleted. Returns false if it's already deleted.
func DeleteENIByName(ctx context.Context, cfg aws.Config, eniName string) (bool, error) {
	logutil.S().Infow("deleting an ENI", "eniName", eniName)

	eni, exists, err := GetENIByName(ctx, cfg, eniName)
	if err != nil {
		return false, err
	}
	if !exists {
		return false, errors.New("eni does not exist")
	}

	return DeleteENI(ctx, cfg, eni.ID)
}

func AttachENI(ctx context.Context, cfg aws.Config, eniID string, instanceID string) (string, error) {
	logutil.S().Infow("attaching ENI", "eniID", eniID, "instanceID", instanceID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeInstances(ctx, &aws_ec2_v2.DescribeInstancesInput{
		InstanceIds: []string{instanceID},
	})
	if err != nil {
		return "", err
	}
	if len(out.Reservations) != 1 {
		return "", fmt.Errorf("expected 1 reservation, got %d", len(out.Reservations))
	}
	if len(out.Reservations[0].Instances) != 1 {
		return "", fmt.Errorf("expected 1 instance, got %d", len(out.Reservations[0].Instances))
	}
	inst := out.Reservations[0].Instances[0]
	index := int32(len(inst.NetworkInterfaces))
	logutil.S().Infow("instance has network interfaces", "eniIDs", len(inst.NetworkInterfaces), "index", index)

	attachOut, err := cli.AttachNetworkInterface(ctx,
		&aws_ec2_v2.AttachNetworkInterfaceInput{
			DeviceIndex:        &index,
			InstanceId:         &instanceID,
			NetworkInterfaceId: &eniID,
		},
	)
	if err != nil {
		return "", err
	}
	attachID := *attachOut.AttachmentId

	logutil.S().Infow("successfully attached ENI", "eniID", eniID, "attachmentID", attachID)
	return attachID, nil
}

// Returns true if it's detached. Returns false if it's already detached.
func DetachENI(ctx context.Context, cfg aws.Config, eniID string, force bool) (bool, error) {
	eni, exists, err := GetENI(ctx, cfg, eniID)
	if err != nil {
		return false, err
	}
	if !exists {
		return false, errors.New("eni does not exist")
	}

	logutil.S().Infow("detaching ENI", "eniID", eniID, "attachmentID", eni.AttachmentID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err = cli.DetachNetworkInterface(ctx,
		&aws_ec2_v2.DetachNetworkInterfaceInput{
			AttachmentId: aws.String(eni.AttachmentID),
			Force:        aws.Bool(force),
		},
	)
	if err != nil {
		return false, err
	}

	logutil.S().Infow("successfully detached ENI", "eniID", eniID, "attachmentID", eni.AttachmentID)
	return true, err
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
	pollInterval time.Duration,
) <-chan ENIStatus {
	return pollENI(ctx, stopc, cfg, eniID, false, desired, desiredAttach, pollInterval)
}

func PollENIDelete(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	eniID string,
	pollInterval time.Duration,
) <-chan ENIStatus {
	return pollENI(ctx, stopc, cfg, eniID, true, aws_ec2_v2_types.NetworkInterfaceStatus(""), aws_ec2_v2_types.AttachmentStatus(""), pollInterval)
}

func pollENI(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	eniID string,
	waitForDelete bool,
	desired aws_ec2_v2_types.NetworkInterfaceStatus,
	desiredAttach aws_ec2_v2_types.AttachmentStatus,
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
