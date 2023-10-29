// Package ssm implements SSM client.
package ssm

import (
	"context"
	"errors"
	"time"

	"github.com/gyuho/infra/aws/go/pkg/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ssm_v2 "github.com/aws/aws-sdk-go-v2/service/ssm"
	aws_ssm_v2_types "github.com/aws/aws-sdk-go-v2/service/ssm/types"
)

var docName string = "AWS-RunShellScript"

// Runs a non-interactive command on the remote machine, and returns the command ID.
// e.g.,
// aws ssm start-session --target ${EC2_INSTANCE_ID} --document-name 'AWS-StartNonInteractiveCommand' --parameters command="sudo iptables -t nat -L"
func SendCommands(ctx context.Context, cfg aws.Config, cmds []string, instanceIDs ...string) (string, error) {
	logutil.S().Debugw("sending command", "cmds", cmds, "instanceIDs", instanceIDs)
	cli := aws_ssm_v2.NewFromConfig(cfg)

	input := &aws_ssm_v2.SendCommandInput{
		DocumentName: &docName,

		Parameters: map[string][]string{
			"commands": cmds,

			// "workingDirectory": {""},

			// time in seconds for a command to complete before it is considered to have failed
			"executionTimeout": {"3600"},
		},

		InstanceIds: instanceIDs,
	}
	out, err := cli.SendCommand(ctx, input)
	if err != nil {
		return "", err
	}
	if out.Command == nil {
		return "", errors.New("command nil")
	}

	cmdID := *out.Command.CommandId
	logutil.S().Infow("command sent", "cmdID", cmdID)

	return cmdID, nil
}

type Output struct {
	CommandID  string
	InstanceID string
	Stdout     string
	Stderr     string
}

// Runs a non-interactive command on the remote machine, and returns the command ID, stdout, stderr.
func SendCommandsWithOutputs(ctx context.Context, cfg aws.Config, cmds []string, instanceIDs ...string) ([]Output, error) {
	cmdID, err := SendCommands(ctx, cfg, cmds, instanceIDs...)
	if err != nil {
		return nil, err
	}
	outputs := make([]Output, 0)
	for _, instanceID := range instanceIDs {
		var output Output
		for {
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(5 * time.Second):
			}

			cctx2, ccancel2 := context.WithTimeout(ctx, 30*time.Second)
			stdout, stderr, status, err := Check(cctx2, cfg, cmdID, instanceID)
			ccancel2()
			if err != nil {
				return nil, err
			}

			if status == aws_ssm_v2_types.CommandInvocationStatusSuccess {
				output = Output{
					CommandID:  cmdID,
					InstanceID: instanceID,
					Stdout:     stdout,
					Stderr:     stderr,
				}
				break
			}
		}
		outputs = append(outputs, output)
	}
	return outputs, nil
}

// Checks the status and outputs of a command.
func Check(ctx context.Context, cfg aws.Config, cmdID string, instanceID string) (string, string, aws_ssm_v2_types.CommandInvocationStatus, error) {
	logutil.S().Infow("checking command status", "cmdID", cmdID)
	cli := aws_ssm_v2.NewFromConfig(cfg)
	input := &aws_ssm_v2.GetCommandInvocationInput{
		CommandId:  &cmdID,
		InstanceId: &instanceID,
	}
	out, err := cli.GetCommandInvocation(ctx, input)
	if err != nil {
		return "", "", "", err
	}

	stdout := ""
	if out.StandardOutputContent != nil {
		stdout = *out.StandardOutputContent
	}
	stderr := ""
	if out.StandardErrorContent != nil {
		stderr = *out.StandardErrorContent
	}
	status := out.Status

	logutil.S().Infow("command status", "cmdID", cmdID, "status", status)
	return stdout, stderr, status, nil
}
