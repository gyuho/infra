package ssm

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
)

func TestSSM(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	// aws ssm start-session --target ${EC2_INSTANCE_ID} --document-name 'AWS-StartNonInteractiveCommand' --parameters command="sudo iptables -t nat -L"
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	outputs, err := SendCommandsWithOutputs(ctx, cfg, []string{"sudo iptables -t nat -L"}, "i-06f2a4326ae4ea0c4")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("stdout:", outputs[0].Stdout)
	fmt.Println("stdout:", outputs[0].Stderr)

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	outputs, err = SendCommandsWithOutputs(
		ctx,
		cfg,
		[]string{
			"sudo tail -10 /var/log/cloud-init-output.log",
			"sudo iptables -t nat -L",
			"df -h",
		},
		"i-06f2a4326ae4ea0c4")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println("stdout:", outputs[0].Stdout)
	fmt.Println("stdout:", outputs[0].Stderr)
}
