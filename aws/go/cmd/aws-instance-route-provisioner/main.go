// Instance route provisioner for AWS.
package main

import (
	"context"
	"fmt"
	"math/rand"
	"os"
	"time"

	aws "github.com/gyuho/infra/aws/go"
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

	idTagKey   string
	idTagValue string

	kindTagKey   string
	kindTagValue string

	localInstancePublishTagKey string

	routeTableIDs      []string
	useLocalSubnetCIDR bool
	destinationCIDR    string
)

// Do not use "aws:" for custom tag creation, as it's not allowed.
// e.g., aws:autoscaling:groupName
// Only use "aws:autoscaling:groupName" for querying.
const asgNameTagKey = "autoscaling:groupName"

func init() {
	cobra.EnablePrefixMatching = true

	cmd.PersistentFlags().StringVar(&region, "region", "us-east-1", "region to provision the ENI in")
	cmd.PersistentFlags().IntVar(&initialWaitRandomSeconds, "initial-wait-random-seconds", 0, "maximum number of seconds to wait (value chosen at random with the range, highly recommend setting value >=60 because EC2 tags take awhile to pupulate)")

	cmd.PersistentFlags().StringVar(&localInstancePublishTagKey, "local-instance-publish-tag-key", "AWS_INSTANCE_ROUTE_PROVISIONER_ROUTES", "tag key to create with the resource value to the local EC2 instance")

	cmd.PersistentFlags().StringSliceVar(&routeTableIDs, "route-table-ids", nil, "route table IDs to create routes")
	cmd.PersistentFlags().BoolVar(&useLocalSubnetCIDR, "use-local-subnet-cidr", true, "true to fetch local subnet CIDR for routes")
	cmd.PersistentFlags().StringVar(&destinationCIDR, "destination-cidr", "", "destination CIDR block for the routes (if not empty, overwrite --use-local-subnet-cidr)")
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

	logutil.S().Infow("fetching instance tags to get the asg name",
		"region", region,
		"instanceID", localInstanceID,
		"asgNameTagKey", asgNameTagKey,
	)

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

		// TODO: handle retries
		ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
		err := ec2.CreateRouteByInstanceID(ctx, cfg, rtbID, destinationCIDR, localInstanceID)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to create route", "error", err)
			os.Exit(1)
		}

		logutil.S().Infow("created route",
			"routeTableID", rtbID,
			"destinationCIDR", destinationCIDR,
			"instanceID", localInstanceID,
		)
	}

	// routes := make([]ec2.Route, 0, len(routeTableIDs))
	// TODO: describe route table and get routes
	// TODO: encode and publish
}
