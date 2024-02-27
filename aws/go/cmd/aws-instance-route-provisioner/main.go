// Instance route provisioner for AWS.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/aws/go/cmd/version"
	"github.com/gyuho/infra/aws/go/ec2"
	"github.com/gyuho/infra/aws/go/ec2/metadata"
	"github.com/gyuho/infra/go/logutil"

	"github.com/spf13/cobra"
)

const appName = "aws-instance-route-provisioner"

var cmd = &cobra.Command{
	Use:        appName,
	Short:      appName,
	Aliases:    []string{"instance-route-provisioner"},
	SuggestFor: []string{"instance-route-provisioner"},
	Run:        cmdFunc,
}

var (
	region                   string
	initialWaitRandomSeconds int

	routeTableIDs         []string
	useLocalSubnetCIDR    bool
	destinationCIDR       string
	deleteBlackholeRoutes bool
	overwrite             bool

	localInstancePublishTagKey string
)

// Do not use "aws:" for custom tag creation, as it's not allowed.
// e.g., aws:autoscaling:groupName
// Only use "aws:autoscaling:groupName" for querying.
const asgNameTagKey = "autoscaling:groupName"

func init() {
	cobra.EnablePrefixMatching = true
	cmd.AddCommand(version.NewCommand())

	cmd.PersistentFlags().StringVar(&region, "region", "us-east-1", "region to provision the ENI in")
	cmd.PersistentFlags().IntVar(&initialWaitRandomSeconds, "initial-wait-random-seconds", 10, "maximum number of seconds to wait (value chosen at random with the range, highly recommend setting value >=60 because EC2 tags take awhile to pupulate)")

	cmd.PersistentFlags().StringSliceVar(&routeTableIDs, "route-table-ids", nil, "route table IDs to create routes")
	cmd.PersistentFlags().BoolVar(&useLocalSubnetCIDR, "use-local-subnet-cidr", true, "true to fetch local subnet CIDR for routes")
	cmd.PersistentFlags().StringVar(&destinationCIDR, "destination-cidr", "", "destination CIDR block for the routes (if not empty, overwrite --use-local-subnet-cidr)")
	cmd.PersistentFlags().BoolVar(&deleteBlackholeRoutes, "delete-blackhole-routes", true, "true to delete blackhole routes in the route tables")
	cmd.PersistentFlags().BoolVar(&overwrite, "overwrite", true, "true to overwrite if routes are in conflict (e.g., already mapped to different instance)")

	cmd.PersistentFlags().StringVar(&localInstancePublishTagKey, "local-instance-publish-tag-key", "AWS_INSTANCE_ROUTE_PROVISIONER_ROUTES", "tag key to create with the resource value to the local EC2 instance")
}

func main() {
	if err := cmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "%q failed %v\n", appName, err)
		os.Exit(1)
	}
	os.Exit(0)
}

func cmdFunc(cmd *cobra.Command, args []string) {
	initialWait := time.Duration(rand.Intn(initialWaitRandomSeconds)) * time.Second
	logutil.S().Infow("starting 'aws-instance-route-provisioner'", "initialWait", initialWait)
	time.Sleep(initialWait)

	if len(routeTableIDs) == 0 {
		logutil.S().Warnw("empty route table ID")
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	localInstanceID, err := metadata.FetchInstanceID(ctx)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to fetch EC2 instance ID", "error", err)
		os.Exit(1)
	}

	cfg, err := aws.New(&aws.Config{
		Region: region,
	})
	if err != nil {
		logutil.S().Warnw("failed to create aws config", "error", err)
		os.Exit(1)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Minute)
	localInstance, asgNameTagValue, err := ec2.WaitInstanceTagValue(ctx, cfg, localInstanceID, "aws:autoscaling:groupName")
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to get asg tag value in time", "error", err)
		os.Exit(1)
	}
	if asgNameTagValue == "" {
		logutil.S().Warnw("failed to get asg tag value in time")
		os.Exit(1)
	}
	logutil.S().Infow("found asg tag", "key", asgNameTagKey, "value", asgNameTagValue)

	if destinationCIDR == "" {
		if !useLocalSubnetCIDR {
			logutil.S().Warnw("destination CIDR block not provided, and --use-local-subnet-cidr is false")
			os.Exit(1)
		}

		logutil.S().Infow("destination CIDR block not provided, so fetching the local subnet's CIDR block")
		ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
		subnet, err := ec2.GetSubnet(ctx, cfg, *localInstance.SubnetId)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to get subnet", "error", err)
			os.Exit(1)
		}
		destinationCIDR = subnet.CIDRBlock
		logutil.S().Infow("using the local subnet CIDR", "subnetID", subnet.ID, "destinationCIDR", destinationCIDR)
	}

	for _, rtbID := range routeTableIDs {
		logutil.S().Infow("creating route",
			"routeTableID", rtbID,
			"destinationCIDR", destinationCIDR,
			"instanceID", localInstanceID,
		)

		if deleteBlackholeRoutes {
			ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
			derr := ec2.DeleteBlackholeRoutes(ctx, cfg, rtbID)
			cancel()
			if derr != nil {
				logutil.S().Warnw("failed to delete blackhole routes", "error", derr)
				os.Exit(1)
			}
		}

		ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
		cerr := ec2.CreateRouteByInstanceID(ctx, cfg, rtbID, destinationCIDR, localInstanceID, ec2.WithOverwrite(overwrite))
		cancel()
		if cerr != nil {
			logutil.S().Warnw("failed to create route", "error", cerr)
			os.Exit(1)
		}

		logutil.S().Infow("created route",
			"routeTableID", rtbID,
			"destinationCIDR", destinationCIDR,
			"instanceID", localInstanceID,
		)
	}

	time.Sleep(2 * time.Second)

	routes := make([]Route, 0, len(routeTableIDs))
	for _, rtbID := range routeTableIDs {
		ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
		rtb, err := ec2.GetRouteTable(ctx, cfg, rtbID)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to get route table", "error", err)
			os.Exit(1)
		}

		instanceRouteFound := false
		for _, route := range rtb.Routes {
			logutil.S().Infow("route", "routeTableID", rtbID, "destinationCIDR", route.DestinationCIDRBlock, "instanceID", route.InstanceID)

			if route.InstanceID == localInstanceID {
				instanceRouteFound = true
				routes = append(routes, Route{
					RouteTableID:         route.RouteTableID,
					DestinationCIDRBlock: route.DestinationCIDRBlock,
					InstanceID:           route.InstanceID,
					ENI:                  route.ENI,
				})
			}
		}
		if !instanceRouteFound {
			logutil.S().Warnw("route not found", "routeTableID", rtbID, "expectedDestinationCIDR", destinationCIDR, "instanceID", localInstanceID)
			os.Exit(1)
		}
	}

	routesContents, err := json.Marshal(routes)
	if err != nil {
		logutil.S().Warnw("failed to marshal routes", "error", err)
		os.Exit(1)
	}
	// ref. https://docs.aws.amazon.com/config/latest/APIReference/API_Tag.html
	if len(routesContents) > 256 {
		routesContents = routesContents[:255:255]
	}

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	err = ec2.CreateTags(
		ctx,
		cfg,
		[]string{localInstanceID},
		map[string]string{
			localInstancePublishTagKey: string(routesContents),
		})
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to create tags", "error", err)
		os.Exit(1)
	}
}

// EC2 tag value limits are 256 characters,
// so we define a custom, subset type of ec2.Route
// ref. https://docs.aws.amazon.com/config/latest/APIReference/API_Tag.html
type Route struct {
	RouteTableID         string `json:"rtb"`
	DestinationCIDRBlock string `json:"cidr"`
	InstanceID           string `json:"ec2,omitempty"`
	ENI                  string `json:"eni,omitempty"`
}
