// IP provisioner for AWS.
// See https://github.com/ava-labs/ip-manager/tree/main/aws-ip-provisioner/src for the original Rust code.
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
	"github.com/gyuho/infra/go/fileutil"
	"github.com/gyuho/infra/go/logutil"

	"github.com/spf13/cobra"
)

const appName = "aws-ip-provisioner"

var cmd = &cobra.Command{
	Use:        appName,
	Short:      appName,
	Aliases:    []string{"ip-provisioner"},
	SuggestFor: []string{"ip-provisioner"},
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

	curEIPsFile string
)

// Do not use "aws:" for custom tag creation, as it's not allowed.
// e.g., aws:autoscaling:groupName
// Only use "aws:autoscaling:groupName" for querying.
const asgNameTagKey = "autoscaling:groupName"

func init() {
	cobra.EnablePrefixMatching = true

	cmd.PersistentFlags().StringVar(&region, "region", "us-east-1", "region to provision the EIP in")
	cmd.PersistentFlags().IntVar(&initialWaitRandomSeconds, "initial-wait-random-seconds", 60, "maximum number of seconds to wait (value chosen at random with the range, highly recommend setting value >=60 because EC2 tags take awhile to pupulate)")

	cmd.PersistentFlags().StringVar(&idTagKey, "id-tag-key", "Id", "key for the EIP 'Id' tag")
	cmd.PersistentFlags().StringVar(&idTagValue, "id-tag-value", "", "value for the EIP 'Id' tag key")

	cmd.PersistentFlags().StringVar(&kindTagKey, "kind-tag-key", "Kind", "key for the EIP 'Kind' tag")
	cmd.PersistentFlags().StringVar(&kindTagValue, "kind-tag-value", "aws-ip-provisioner", "value for the EIP 'Kind' tag key")

	cmd.PersistentFlags().StringVar(&localInstancePublishTagKey, "local-instance-publish-tag-key", "AWS_IP_PROVISIONER_EIPS", "tag key to create with the resource value to the local EC2 instance")

	cmd.PersistentFlags().StringVar(&curEIPsFile, "current-eips-file", "/data/current-eips.json", "file path to write the current EIP (useful for paused instances)")
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
	logutil.S().Infow("starting 'aws-ip-provisioner'", "initialWait", initialWait)

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
	_, asgNameTagValue, err := ec2.WaitInstanceTagValue(ctx, cfg, localInstanceID, "aws:autoscaling:groupName")
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

	// a single EC2 instance can have multiple EIPs
	// ref. https://repost.aws/knowledge-center/secondary-private-ip-address
	logutil.S().Infow("checking if EIP is already associated", "localInstanceID", localInstanceID)
	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	curAssociated, err := ec2.ListEIPs(
		ctx,
		cfg,
		ec2.WithFilters(map[string][]string{
			"instance-id": {localInstanceID},
		}),
	)
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to list EIPs", "error", err)
		os.Exit(1)
	}
	// TODO: limit a single EIP per instance?
	if len(curAssociated) > 0 {
		logutil.S().Warnw("EIP already associated to this instance -- may get charged extra", "eips", len(curAssociated))
	}

	eipsToAssociate := make(ec2.EIPs, 0)
	logutil.S().Infow("checking if EIPs file exists locally", "file", curEIPsFile)
	exists, err := fileutil.FileExists(curEIPsFile)
	if err != nil {
		logutil.S().Warnw("failed to check if EIPs file exists locally", "error", err)
		os.Exit(1)
	}
	if exists {
		logutil.S().Infow("found EIPs file locally", "file", curEIPsFile)
		eipsToAssociate, err = ec2.LoadEIPs(curEIPsFile)
		if err != nil {
			logutil.S().Warnw("failed to load EIPs", "error", err)
			os.Exit(1)
		}
	} else {
		logutil.S().Infow("no EIP file found locally", "file", curEIPsFile)
		ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
		eip, err := ec2.AllocateEIP(ctx, cfg, asgNameTagValue, ec2.WithTags(map[string]string{
			idTagKey:      idTagValue,
			kindTagKey:    kindTagValue,
			asgNameTagKey: asgNameTagValue,
		}))
		cancel()
		if err != nil {
			logutil.S().Warnw("failed to allocate EIP", "error", err)
			os.Exit(1)
		}
		eipsToAssociate = append(eipsToAssociate, eip)
	}
	if err := eipsToAssociate.Sync(curEIPsFile); err != nil {
		logutil.S().Warnw("failed to sync EIP", "error", err)
		os.Exit(1)
	}
	logutil.S().Infow("successfully synced EIP", "eips", eipsToAssociate)

	needsAssociate := make(map[ec2.EIP]struct{})
	for _, eip := range eipsToAssociate {
		alreadyAssociated := false
		for _, addr := range curAssociated {
			allocationID := *addr.AllocationId
			publicIP := *addr.PublicIp
			logutil.S().Infow("found EIP associated to this instance", "allocationID", allocationID, "publicIP", publicIP)

			if eip.AllocationID == allocationID && eip.PublicIP == publicIP {
				logutil.S().Infow("EIP already associated to this instance -- no need to re-associate", "eip", eipsToAssociate)
				alreadyAssociated = true
				break
			}
		}
		if !alreadyAssociated {
			needsAssociate[eip] = struct{}{}
		}
	}
	if len(needsAssociate) > 0 {
		for eip := range needsAssociate {
			// re-association wouldn't fail when "AllowReassociation" is set to true
			logutil.S().Infow("associating EIP to this instance", "eip", eip.AllocationID, "localInstanceID", localInstanceID)
			ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
			err = ec2.AssociateEIPByInstanceID(ctx, cfg, eip.AllocationID, localInstanceID)
			cancel()
			if err != nil {
				logutil.S().Warnw("failed to associate EIP", "error", err)
				os.Exit(1)
			}
		}
	} else {
		logutil.S().Infow("no EIPs to associate (already associated)")
	}

	s := eipsToAssociate.String()
	logutil.S().Infow("successfully associated or loaded EIP", "eip", s)

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	err = ec2.CreateTags(
		ctx,
		cfg,
		[]string{localInstanceID},
		map[string]string{
			localInstancePublishTagKey: s,
		})
	cancel()
	if err != nil {
		logutil.S().Warnw("failed to create tags", "error", err)
		os.Exit(1)
	}
}
