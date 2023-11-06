// Volume provisioner for AWS.
// See https://github.com/ava-labs/volume-manager for the original Rust code.
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

const appName = "aws-volume-provisioner"

var cmd = &cobra.Command{
	Use:        appName,
	Short:      appName,
	Aliases:    []string{"volume-provisioner"},
	SuggestFor: []string{"volume-provisioner"},
	Run:        cmdFunc,
}

var (
	region                    string
	initialWwaitRandomSeconds int
	idTagKey                  string
)

func init() {
	cobra.EnablePrefixMatching = true

	cmd.PersistentFlags().StringVar(&region, "region", "us-east-1", "region to provision the volume in")
	cmd.PersistentFlags().IntVar(&initialWwaitRandomSeconds, "initial-wait-random-seconds", 60, "maximum number of seconds to wait (value chosen at random with the range, highly recommend setting value >=60 because EC2 tags take awhile to pupulate)")
	cmd.PersistentFlags().StringVar(&idTagKey, "id-tag-key", "Id", "key for the EBS volume 'Id' tag (must be set via EC2 tags, or used for EBS volume creation)")

	// TODO
}

func main() {
	if err := cmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "%q failed %v\n", appName, err)
		os.Exit(1)
	}
	os.Exit(0)
}

func cmdFunc(cmd *cobra.Command, args []string) {
	fmt.Println(1)
}
