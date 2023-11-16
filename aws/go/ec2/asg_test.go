package ec2

import (
	"context"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/randutil"
)

func TestListInstancesByASG(t *testing.T) {
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
	instances, err := ListInstancesByASG(ctx, cfg, randutil.AlphabetsLowerCase(10))
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	if len(instances) > 0 {
		t.Fatalf("expected 0 instances, got %d", len(instances))
	}
}
