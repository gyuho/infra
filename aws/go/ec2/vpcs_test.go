package ec2

import (
	"context"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
)

func TestListVPCs(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	vpcs, err := ListVPCs(ctx, cfg)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	for _, vpc := range vpcs {
		t.Logf("VPC ID: %q (%s)\n", vpc.ID, vpc.State)
		for _, s := range vpc.Subnets {
			t.Logf("subnet %s in availability zone: %s", s.ID, s.AvailabilityZone)
		}
	}
}
