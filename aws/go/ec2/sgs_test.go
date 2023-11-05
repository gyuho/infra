package ec2

import (
	"context"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"

	aws_sdk "github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

func TestListSGs(t *testing.T) {
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

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	sgs, err := ListSGs(ctx, cfg, aws_ec2_v2_types.Filter{
		Name:   aws_sdk.String("vpc-id"),
		Values: []string{vpcs[0].ID},
	})
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	for _, v := range sgs {
		t.Logf("Security group: %q (%s, %s)\n", v.ID, v.Name, v.Description)
	}
}
