package ec2

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/randutil"

	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

func TestEBS(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	volName := "volume" + randutil.AlphabetsLowerCase(10)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	volID, err := CreateVolume(ctx, cfg, volName, WithTags(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("volume ID:", volID)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Minute)
	ch := PollVolume(
		ctx,
		make(chan struct{}),
		cfg,
		volID,
		WithVolumeState(aws_ec2_v2_types.VolumeStateAvailable),
		WithInterval(10*time.Second),
	)
	for v := range ch {
		fmt.Println("volume:", v)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = DeleteVolume(ctx, cfg, volID)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Minute)
	ch = PollVolume(
		ctx,
		make(chan struct{}),
		cfg,
		volID,
		WithVolumeState(aws_ec2_v2_types.VolumeStateDeleted),
		WithInterval(10*time.Second),
	)
	for v := range ch {
		fmt.Println("volume:", v)
	}
}
