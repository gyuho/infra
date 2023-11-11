// ENI provisioner for AWS.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/aws/go/ec2"
	"github.com/gyuho/infra/aws/go/ec2/metadata"
	"github.com/gyuho/infra/go/fileutil"
	"github.com/gyuho/infra/go/logutil"

	"github.com/spf13/cobra"
)

const appName = "aws-eni-provisioner"

var cmd = &cobra.Command{
	Use:        appName,
	Short:      appName,
	Aliases:    []string{"eni-provisioner"},
	SuggestFor: []string{"eni-provisioner"},
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

	subnetID string
	sgIDs    []string

	curENIsFile string
)

// Do not use "aws:" for custom tag creation, as it's not allowed.
// e.g., aws:autoscaling:groupName
// Only use "aws:autoscaling:groupName" for querying.
const asgNameTagKey = "autoscaling:groupName"

func init() {
	cobra.EnablePrefixMatching = true

	cmd.PersistentFlags().StringVar(&region, "region", "us-east-1", "region to provision the ENI in")
	cmd.PersistentFlags().IntVar(&initialWaitRandomSeconds, "initial-wait-random-seconds", 60, "maximum number of seconds to wait (value chosen at random with the range, highly recommend setting value >=60 because EC2 tags take awhile to pupulate)")

	cmd.PersistentFlags().StringVar(&idTagKey, "id-tag-key", "Id", "key for the ENI 'Id' tag")
	cmd.PersistentFlags().StringVar(&idTagValue, "id-tag-value", "", "value for the ENI 'Id' tag key")

	cmd.PersistentFlags().StringVar(&kindTagKey, "kind-tag-key", "Kind", "key for the ENI 'Kind' tag")
	cmd.PersistentFlags().StringVar(&kindTagValue, "kind-tag-value", "aws-eni-provisioner", "value for the ENI 'Kind' tag key")

	cmd.PersistentFlags().StringVar(&localInstancePublishTagKey, "local-instance-publish-tag-key", "AWS_ENI_PROVISIONER_ENIS", "tag key to create with the resource value to the local EC2 instance")

	cmd.PersistentFlags().StringVar(&subnetID, "subnet-id", "", "subnet ID to create the ENI in (leave empty to use the same as the instance)")
	cmd.PersistentFlags().StringSliceVar(&sgIDs, "security-group-ids", nil, "security group IDs to create the ENI in (leave empty to use the same as the instance)")

	cmd.PersistentFlags().StringVar(&curENIsFile, "current-enis-file", "/data/current-enis.json", "file path to write the current ENIs (useful for paused instances)")
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
	logutil.S().Infow("starting 'aws-eni-provisioner'", "initialWait", initialWait)

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

	if subnetID == "" {
		subnetID = *localInstance.SubnetId
		logutil.S().Infow("subnet ID not provided, use the local instance", "subnetID", subnetID)
	}
	if len(sgIDs) == 0 {
		for _, sg := range localInstance.SecurityGroups {
			sgIDs = append(sgIDs, *sg.GroupId)
		}
		logutil.S().Infow("security group ID not provided, use the local instance", "securityGroupIDs", sgIDs)
	}

	// a single EC2 instance can have multiple ENIs
	logutil.S().Infow("checking which ENIs are already associated (using instance ID based EC2 query)", "localInstanceID", localInstanceID)
	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	curAttached1, err := ec2.GetENIsByInstanceID(
		ctx,
		cfg,
		localInstanceID,
	)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to list ENIs by instance ID", "error", err)
		os.Exit(1)
	}
	if len(curAttached1) > 0 {
		for _, eni := range curAttached1 {
			logutil.S().Infow("currently attached ENI (from instance ID based EC2 query)",
				"eniID", eni.ID,
				"privateIP", eni.PrivateIP,
				"attachmentDeviceIndex", eni.AttachmentDeviceIndex,
				"attachmentNetworkCardIndex", eni.AttachmentNetworkCardIndex,
			)
		}
	} else {
		logutil.S().Infow("no ENI attached (from instance ID based EC2 query)")
	}

	logutil.S().Infow("checking which ENIs are already associated (using tag-based ENI query)", "localInstanceID", localInstanceID)
	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	curAttached2, err := ec2.ListENIs(
		ctx,
		cfg,
		ec2.WithFilters(map[string][]string{
			// attachment.instance-id - The ID of the instance to which the network interface is attached.
			"attachment.instance-id": {localInstanceID},
		}),
	)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to list ENIs by tags", "error", err)
		os.Exit(1)
	}
	if len(curAttached2) > 0 {
		for _, eni := range curAttached2 {
			logutil.S().Infow("currently attached ENI (from tag-based ENI query)",
				"eniID", eni.ID,
				"privateIP", eni.PrivateIP,
				"attachmentDeviceIndex", eni.AttachmentDeviceIndex,
				"attachmentNetworkCardIndex", eni.AttachmentNetworkCardIndex,
			)
		}
	} else {
		logutil.S().Infow("no ENI attached (from tag-based ENI query)")
	}

	enisToAttach := make([]string, 0)
	logutil.S().Infow("checking if ENIs file exists locally", "file", curENIsFile)
	exists, err := fileutil.FileExists(curENIsFile)
	if err != nil {
		logutil.S().Warnw("failed to check if ENIs file exists locally", "error", err)
		os.Exit(1)
	}
	if exists {
		logutil.S().Infow("found ENIs file locally", "file", curENIsFile)
		b, err := os.ReadFile(curENIsFile)
		if err != nil {
			logutil.S().Warnw("failed to read ENIs file", "error", err)
			os.Exit(1)
		}
		if err := json.Unmarshal(b, &enisToAttach); err != nil {
			logutil.S().Warnw("failed to load ENIs file", "error", err)
			os.Exit(1)
		}
	} else {
		logutil.S().Infow("no ENIs file found locally", "file", curENIsFile)
		ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
		created, err := ec2.CreateENI(
			ctx,
			cfg,
			asgNameTagValue,
			subnetID,
			sgIDs,
			ec2.WithTags(map[string]string{
				idTagKey:      idTagValue,
				kindTagKey:    kindTagValue,
				asgNameTagKey: asgNameTagValue,
			}),
		)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to create ENI", "error", err)
			os.Exit(1)
		}
		enisToAttach = append(enisToAttach, created.ID)
	}
	logutil.S().Infow("successfully created/loaded ENIs", "enis", enisToAttach)

	enisFileContents, err := json.Marshal(enisToAttach)
	if err != nil {
		logutil.S().Warnw("failed to marshal ENIs", "error", err)
		os.Exit(1)
	}
	if err := os.WriteFile(curENIsFile, enisFileContents, 0644); err != nil {
		logutil.S().Warnw("failed to write ENIs file", "error", err)
		os.Exit(1)
	}
	logutil.S().Infow("successfully synced ENIs", "enis", enisToAttach)

	alreadyAttached := make(map[string]struct{})
	for _, eni := range curAttached1 {
		alreadyAttached[eni.ID] = struct{}{}
	}
	for _, eniID := range enisToAttach {
		if _, ok := alreadyAttached[eniID]; ok {
			logutil.S().Infow("ENI already attached to this instance -- no need to re-attach", "eniID", eniID)
			continue
		}

		logutil.S().Infow("ENI not attached to this instance -- attaching", "eniID", eniID)
		ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
		_, err = ec2.AttachENI(ctx, cfg, eniID, localInstanceID)
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to attach ENI", "error", err)
			os.Exit(1)
		}
	}

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	err = ec2.CreateTags(
		ctx,
		cfg,
		[]string{localInstanceID},
		map[string]string{
			localInstancePublishTagKey: string(enisFileContents),
		})
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to create tags", "error", err)
		os.Exit(1)
	}

	logutil.S().Infow("checking after ENIs are attached", "localInstanceID", localInstanceID)
	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	curAttached, err := ec2.GetENIsByInstanceID(
		ctx,
		cfg,
		localInstanceID,
	)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to list ENIs by instance ID", "error", err)
		os.Exit(1)
	}
	if len(curAttached) > 0 {
		for _, eni := range curAttached {
			logutil.S().Infow("currently attached ENI",
				"eniID", eni.ID,
				"privateIP", eni.PrivateIP,
				"attachmentDeviceIndex", eni.AttachmentDeviceIndex,
				"attachmentNetworkCardIndex", eni.AttachmentNetworkCardIndex,
			)
		}
	} else {
		logutil.S().Infow("no ENI attached")
	}
}
