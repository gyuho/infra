package ec2

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"sort"
	"strings"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	"github.com/olekukonko/tablewriter"
)

// Creates a route in the route table for the specified instance, with its primary ENI.
func CreateRouteByInstanceID(ctx context.Context, cfg aws.Config, rtbID string, destinationCIDR string, instanceID string, opts ...OpOption) error {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("creating a route in the route table", "routeTableID", rtbID, "destinationCIDR", destinationCIDR, "instanceID", instanceID)

	cli := aws_ec2_v2.NewFromConfig(cfg)

	// this does not fail even if the destination CIDR is the same, so long as the target instance/ENI is the same
	// (you can run this multiple times)
	cout, cerr := cli.CreateRoute(
		ctx,
		&aws_ec2_v2.CreateRouteInput{
			RouteTableId:         &rtbID,
			DestinationCidrBlock: &destinationCIDR,
			InstanceId:           &instanceID,
		},
	)
	if cerr != nil {
		// fail when the different nat instance already set up this route
		// need some manual fix when the old nat instance goes down
		// e.g.,
		// "operation error EC2: CreateRoute, https response error StatusCode: 400, api error RouteAlreadyExists: The route identified by 10.0.80.0/20 already exists."
		//
		// and also fails if the instance has multiple ENIs
		// e.g.,
		// operation error EC2: CreateRoute, https response error StatusCode: 400
		// api error InvalidInstanceID: There are multiple interfaces attached to instance 'i-08d0d1c7144304719'. Please specify an interface ID for the operation instead.
		//
		// no need to handle error when the request has the same route table ID + cidr as existing one
		// duplicate applies do not incur error in the EC2 API
		if strings.Contains(cerr.Error(), destinationCIDR+" already exists") {
			logutil.S().Warnw("failed to create route due to conflict", "error", cerr.Error())

			if ret.overwrite {
				logutil.S().Infow("deleting route to overwrite",
					"routeTableID", rtbID,
					"destinationCIDR", destinationCIDR,
				)
				if _, derr := cli.DeleteRoute(
					ctx,
					&aws_ec2_v2.DeleteRouteInput{
						RouteTableId:         &rtbID,
						DestinationCidrBlock: &destinationCIDR,
					},
				); derr != nil {
					return derr
				}

				logutil.S().Infow("retry to create route again after delete")
				cout, cerr = cli.CreateRoute(
					ctx,
					&aws_ec2_v2.CreateRouteInput{
						RouteTableId:         &rtbID,
						DestinationCidrBlock: &destinationCIDR,
						InstanceId:           &instanceID,
					},
				)
				if cerr != nil {
					return cerr
				}
			}
		}
	}
	if cerr != nil {
		return cerr
	}

	// duplicate applies do not incur error in the EC2 API
	success := false
	if cout.Return != nil {
		success = *cout.Return
	}
	if !success {
		return errors.New("failed to create route")
	}

	logutil.S().Infow("successfully created a route in the route table", "routeTableID", rtbID, "destinationCIDR", destinationCIDR, "instanceID", instanceID)
	return nil
}

// Creates a route in the route table for the specified ENI.
func CreateRouteByENI(ctx context.Context, cfg aws.Config, rtbID string, destinationCIDR string, eniID string) error {
	logutil.S().Infow("creating a route in the route table", "routeTableID", rtbID, "destinationCIDR", destinationCIDR, "eniID", eniID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.CreateRoute(
		ctx,
		&aws_ec2_v2.CreateRouteInput{
			RouteTableId:         &rtbID,
			DestinationCidrBlock: &destinationCIDR,
			NetworkInterfaceId:   &eniID,
		},
	)
	if err != nil {
		return err
	}

	success := false
	if out.Return != nil {
		success = *out.Return
	}
	if !success {
		return errors.New("failed to create route")
	}

	logutil.S().Infow("successfully created a route in the route table", "routeTableID", rtbID, "destinationCIDR", destinationCIDR, "eniID", eniID)
	return nil
}

// List route tables for the VPC.
func ListRouteTablesByVPC(ctx context.Context, cfg aws.Config, vpcID string) (RouteTables, error) {
	logutil.S().Infow("listing route tables for VPC", "vpcID", vpcID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeRouteTables(
		ctx,
		&aws_ec2_v2.DescribeRouteTablesInput{
			Filters: []aws_ec2_v2_types.Filter{
				{ // --filters Name=vpc-id,Values=vpc-0f4e5eacbd41e24e3
					Name:   aws.String("vpc-id"),
					Values: []string{vpcID},
				},
			},
		},
	)
	if err != nil {
		return nil, err
	}

	rtbs := toRouteTables(out.RouteTables...)
	all := make(map[string]struct{}, len(rtbs))
	for _, rtb := range rtbs {
		for _, s := range rtb.RouteTableAssociations {
			if s.SubnetID != "" {
				all[s.SubnetID] = struct{}{}
			}
		}
	}
	subnetIDs := make([]string, 0, len(all))
	for subnetID := range all {
		subnetIDs = append(subnetIDs, subnetID)
	}
	sout, err := cli.DescribeSubnets(
		ctx,
		&aws_ec2_v2.DescribeSubnetsInput{
			SubnetIds: subnetIDs,
		},
	)
	if err != nil {
		return nil, err
	}
	subnetIDToAZ := make(map[string]string, len(rtbs))
	for _, subnet := range sout.Subnets {
		subnetIDToAZ[*subnet.SubnetId] = *subnet.AvailabilityZone
	}
	for i := range rtbs {
		for j := range rtbs[i].RouteTableAssociations {
			rtbs[i].RouteTableAssociations[j].AvailabilityZone = subnetIDToAZ[rtbs[i].RouteTableAssociations[j].SubnetID]
		}
	}

	return rtbs, nil
}

// Get routes and route table for the give route table ID.
func GetRouteTable(ctx context.Context, cfg aws.Config, rtbID string) (RouteTable, error) {
	logutil.S().Infow("listing routes in the route table", "routeTableID", rtbID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DescribeRouteTables(
		ctx,
		&aws_ec2_v2.DescribeRouteTablesInput{
			RouteTableIds: []string{rtbID},
		},
	)
	if err != nil {
		return RouteTable{}, err
	}
	if len(out.RouteTables) != 1 {
		return RouteTable{}, errors.New("route table not found")
	}
	rtb := out.RouteTables[0]
	rtbs := toRouteTables(rtb)
	return rtbs[0], nil
}

// Deletes "blackhole" state routes in the route table.
func DeleteBlackholeRoutes(ctx context.Context, cfg aws.Config, rtbID string) error {
	rtb, err := GetRouteTable(ctx, cfg, rtbID)
	if err != nil {
		return err
	}
	for _, route := range rtb.Routes {
		if route.State != "blackhole" {
			continue
		}
		logutil.S().Warnw("found blackhole route",
			"routeTableID", rtbID,
			"destinationCIDR", route.DestinationCIDRBlock,
		)
		if err := DeleteRouteByDestinationCIDR(ctx, cfg, rtbID, route.DestinationCIDRBlock); err != nil {
			return err
		}
	}
	return nil
}

// Deletes a route in the route table for the specified destination CIDR.
func DeleteRouteByDestinationCIDR(ctx context.Context, cfg aws.Config, rtbID string, destinationCIDR string) error {
	logutil.S().Infow("deleting a route in the route table", "routeTableID", rtbID, "destinationCIDR", destinationCIDR)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	_, err := cli.DeleteRoute(
		ctx,
		&aws_ec2_v2.DeleteRouteInput{
			RouteTableId:         &rtbID,
			DestinationCidrBlock: &destinationCIDR,
		},
	)
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully deleted a route in the route table", "routeTableID", rtbID, "destinationCIDR", destinationCIDR)
	return nil
}

type Route struct {
	RouteTableID string `json:"route_table_id"`
	RouteOrigin  string `json:"route_origin,omitempty"`
	// State is set to "blackhole" if the previously mapped
	// ENI or EC2 instance got deleted.
	State                string `json:"state,omitempty"`
	DestinationCIDRBlock string `json:"destination_cidr_block"`
	InstanceID           string `json:"instance_id,omitempty"`
	ENI                  string `json:"eni,omitempty"`
	NATGateway           string `json:"nat_gateway,omitempty"`
	LocalGateway         string `json:"local_gateway,omitempty"`
	Gateway              string `json:"gateway,omitempty"`
}

type Routes []Route

func toRoutes(rtbID string, rts ...aws_ec2_v2_types.Route) Routes {
	routes := make(Routes, 0, len(rts))
	for _, r := range rts {
		instanceID := ""
		if r.InstanceId != nil {
			instanceID = *r.InstanceId
		}
		eni := ""
		if r.NetworkInterfaceId != nil {
			eni = *r.NetworkInterfaceId
		}
		nat := ""
		if r.NatGatewayId != nil {
			nat = *r.NatGatewayId
		}
		local := ""
		if r.LocalGatewayId != nil {
			local = *r.LocalGatewayId
		}
		gateway := ""
		if r.GatewayId != nil {
			gateway = *r.GatewayId
		}

		routes = append(routes, Route{
			RouteTableID:         rtbID,
			RouteOrigin:          string(r.Origin),
			State:                string(r.State),
			DestinationCIDRBlock: *r.DestinationCidrBlock,

			InstanceID: instanceID,
			ENI:        eni,

			NATGateway:   nat,
			LocalGateway: local,
			Gateway:      gateway,
		})
	}

	sort.SliceStable(routes, func(i, j int) bool {
		if routes[i].RouteTableID == routes[j].RouteTableID {
			if routes[i].DestinationCIDRBlock == routes[j].DestinationCIDRBlock {
				return routes[i].InstanceID < routes[j].InstanceID
			}
			return routes[i].DestinationCIDRBlock < routes[j].DestinationCIDRBlock
		}
		return routes[i].RouteTableID < routes[j].RouteTableID
	})
	return routes
}

func (rts Routes) String() string {
	rows := make([][]string, 0, len(rts))
	for _, v := range rts {
		rows = append(rows, []string{
			v.RouteTableID,
			v.RouteOrigin,
			v.State,
			v.DestinationCIDRBlock,
			v.InstanceID,
			v.ENI,
			v.NATGateway,
			v.LocalGateway,
			v.Gateway,
		},
		)
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"route table id", "route origin", "state", "destination cidr block", "instance id", "eni", "nat gateway", "local gateway", "gateway"})
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}

type RouteTable struct {
	ID                     string                 `json:"id"`
	Name                   string                 `json:"name"`
	VPCID                  string                 `json:"vpc_id"`
	Routes                 Routes                 `json:"routes"`
	RouteTableAssociations RouteTableAssociations `json:"route_table_associations"`
	Tags                   map[string]string      `json:"tags"`
}

type RouteTables []RouteTable

func (rtbs RouteTables) Sort() {
	sort.SliceStable(rtbs, func(i, j int) bool {
		if rtbs[i].ID == rtbs[j].ID {
			if rtbs[i].VPCID == rtbs[j].VPCID {
				return len(rtbs[i].Routes) < len(rtbs[j].Routes)
			}
			return rtbs[i].VPCID < rtbs[j].VPCID
		}
		return rtbs[i].ID < rtbs[j].ID
	})
}

type RouteTableAssociation struct {
	ID               string `json:"id"`
	RouteTableID     string `json:"route_table_id"`
	SubnetID         string `json:"subnet_id"`
	AvailabilityZone string `json:"availability_zone"`
	State            string `json:"state"`
	Gateway          string `json:"gateway"`
	Main             bool   `json:"main"`
}

type RouteTableAssociations []RouteTableAssociation

func toRouteTables(rtbs ...aws_ec2_v2_types.RouteTable) RouteTables {
	routeTables := make(RouteTables, 0, len(rtbs))
	for _, r := range rtbs {
		routeTableName := ""
		tags := make(map[string]string)
		for _, tg := range r.Tags {
			if *tg.Key == "Name" {
				routeTableName = *tg.Value
			}
			tags[*tg.Key] = *tg.Value
		}

		routes := toRoutes(*r.RouteTableId, r.Routes...)
		sort.SliceStable(routes, func(i, j int) bool {
			if routes[i].RouteTableID == routes[j].RouteTableID {
				if routes[i].DestinationCIDRBlock == routes[j].DestinationCIDRBlock {
					return routes[i].InstanceID < routes[j].InstanceID
				}
				return routes[i].DestinationCIDRBlock < routes[j].DestinationCIDRBlock
			}
			return routes[i].RouteTableID < routes[j].RouteTableID
		})
		associations := toRouteTableAssociations(r.Associations...)
		sort.SliceStable(associations, func(i, j int) bool {
			if associations[i].RouteTableID == associations[j].RouteTableID {
				if associations[i].SubnetID == associations[j].SubnetID {
					return associations[i].State < associations[j].State
				}
				return associations[i].SubnetID < associations[j].SubnetID
			}
			return associations[i].RouteTableID < associations[j].RouteTableID
		})

		routeTables = append(routeTables, RouteTable{
			ID:                     *r.RouteTableId,
			Name:                   routeTableName,
			VPCID:                  *r.VpcId,
			Routes:                 routes,
			RouteTableAssociations: associations,
			Tags:                   tags,
		})
	}

	routeTables.Sort()
	return routeTables
}

func toRouteTableAssociations(associations ...aws_ec2_v2_types.RouteTableAssociation) RouteTableAssociations {
	ss := make(RouteTableAssociations, 0, len(associations))
	for _, r := range associations {
		subnetID := ""
		if r.SubnetId != nil {
			subnetID = *r.SubnetId
		}
		gateway := ""
		if r.GatewayId != nil {
			gateway = *r.GatewayId
		}
		main := false
		if r.Main != nil {
			main = *r.Main
		}

		ss = append(ss, RouteTableAssociation{
			ID:           *r.RouteTableAssociationId,
			RouteTableID: *r.RouteTableId,
			SubnetID:     subnetID,
			State:        string(r.AssociationState.State),
			Gateway:      gateway,
			Main:         main,
		})
	}
	return ss
}

func (rtbs RouteTables) String() string {
	rows := make([][]string, 0, len(rtbs))
	for _, rtb := range rtbs {
		tags := make([]string, 0)
		for k, v := range rtb.Tags {
			// TODO: remove this in Go 1.22
			// ref. https://go.dev/blog/loopvar-preview
			k, v := k, v
			tags = append(tags, fmt.Sprintf("%s=%s", k, v))
		}
		sort.Strings(tags)

		associations := make([]string, 0)
		for _, v := range rtb.RouteTableAssociations {
			s := fmt.Sprintf("subnet=%s", v.SubnetID)
			if v.SubnetID == "" {
				s = fmt.Sprintf("main=%v", v.Main)
			}
			associations = append(associations, s)
		}
		sort.Strings(associations)

		rows = append(rows, []string{
			rtb.ID,
			rtb.VPCID,
			fmt.Sprint(len(rtb.Routes)),
			strings.Join(associations, "\n"),
			strings.Join(tags, "\n"),
		},
		)
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"route table id", "vpc id", "routes", "route table associations", "tags"})
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}
