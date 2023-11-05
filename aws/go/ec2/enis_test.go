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

func TestENIs(t *testing.T) {
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
	_, exists, err := GetENIByName(ctx, cfg, randutil.String(10))
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	if exists {
		t.Fatal("ENI should not exist")
	}

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	eniByName, exists, err := GetENIByName(ctx, cfg, "ingress-satellite-node-1")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	fmt.Printf("%v %+v\n", exists, eniByName)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	eni, err := GetPrimaryENIByInstanceID(ctx, cfg, "i-06f2a4326ae4ea0c4")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("ENI ID:", *eni.NetworkInterfaceId)
	fmt.Println("ENI privateIP:", *eni.PrivateIpAddress)
	fmt.Printf("%+v\n", eni)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	enis, err := ListENIs(ctx, cfg)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	for i, v := range enis {
		t.Logf("ENI: %q (%s, %s)\n", v.ID, v.PrivateDNS, v.Status)

		if i > 0 {
			continue
		}

		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		eni, exists, err := GetENI(ctx, cfg, v.ID)
		if err != nil {
			t.Fatal(err)
		}
		if !exists {
			t.Fatal("ENI should exist")
		}
		t.Logf("ENI: %+v\n", eni)

		ch := PollENI(
			ctx,
			make(chan struct{}),
			cfg,
			v.ID,
			aws_ec2_v2_types.NetworkInterfaceStatusInUse,
			aws_ec2_v2_types.AttachmentStatusAttached,
			5*time.Second,
			time.Second,
		)
		for ev := range ch {
			t.Logf("ENI event: %+v", ev)
		}
	}
}
