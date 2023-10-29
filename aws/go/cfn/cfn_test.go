package cfn

import (
	"context"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"

	aws_cloudformation_v2_types "github.com/aws/aws-sdk-go-v2/service/cloudformation/types"
)

// go test -v -run TestCFN -timeout 10m
func TestCFN(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	b, err := os.ReadFile("tests/ec2_instance_role.yaml")
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	stackID, err := CreateStack(
		ctx,
		cfg,
		"test-stack",
		string(b),
		map[string]string{
			"Id":                   "123",
			"KmsCmkArn":            "arn:aws:kms:us-west-2:123:key/456",
			"S3BucketName":         "test-bucket",
			"S3BucketDbBackupName": "test-bucket-db-backup",
		},
		map[string]string{
			"test": "test",
			"a":    "b",
		},
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("stack id: %s", stackID)

	ctx, cancel = context.WithTimeout(context.Background(), 5*time.Minute)
	ch := Poll(
		ctx,
		make(chan struct{}),
		cfg,
		stackID,
		aws_cloudformation_v2_types.StackStatusCreateComplete,
		30*time.Second,
		10*time.Second,
	)
	for s := range ch {
		t.Logf("stack status: %+v", s)
	}
	cancel()

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = DeleteStack(ctx, cfg, stackID)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 5*time.Minute)
	ch = Poll(
		ctx,
		make(chan struct{}),
		cfg,
		stackID,
		aws_cloudformation_v2_types.StackStatusDeleteComplete,
		30*time.Second,
		10*time.Second,
	)
	for s := range ch {
		t.Logf("stack status: %+v", s)
	}
	cancel()

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = DeleteStack(ctx, cfg, stackID)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = DeleteStack(ctx, cfg, stackID)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
