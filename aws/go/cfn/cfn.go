// Package cfn implements Cloudformation utils.
package cfn

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/gyuho/infra/aws/go/pkg/ctxutil"
	"github.com/gyuho/infra/aws/go/pkg/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_cloudformation_v2 "github.com/aws/aws-sdk-go-v2/service/cloudformation"
	aws_cloudformation_v2_types "github.com/aws/aws-sdk-go-v2/service/cloudformation/types"
	"github.com/dustin/go-humanize"
)

func GetStack(
	ctx context.Context,
	cfg aws.Config,
	stackName string,
) (string, error) {
	logutil.S().Infow("getting stack", "stackName", stackName)

	cli := aws_cloudformation_v2.NewFromConfig(cfg)
	out, err := cli.DescribeStacks(ctx, &aws_cloudformation_v2.DescribeStacksInput{
		StackName: &stackName,
	})
	if err != nil {
		return "", err
	}
	if len(out.Stacks) != 1 {
		return "", fmt.Errorf("expected only 1 stack; got %v", len(out.Stacks))
	}
	return *out.Stacks[0].StackId, nil
}

func CreateStack(
	ctx context.Context,
	cfg aws.Config,
	stackName string,
	templateBody string,
	params map[string]string,
	tags map[string]string,
) (string, error) {
	logutil.S().Infow("creating stack", "stack-name", stackName)

	cli := aws_cloudformation_v2.NewFromConfig(cfg)
	input := &aws_cloudformation_v2.CreateStackInput{
		StackName: &stackName,

		Capabilities: []aws_cloudformation_v2_types.Capability{aws_cloudformation_v2_types.CapabilityCapabilityNamedIam},
		OnFailure:    aws_cloudformation_v2_types.OnFailureDelete,

		TemplateBody: &templateBody,

		Parameters: NewParameters(params),
		Tags:       NewTags(tags),
	}
	out, err := cli.CreateStack(ctx, input)
	if err != nil {
		if strings.Contains(err.Error(), "AlreadyExistsException") {
			logutil.S().Warnw("stack already exists -- returning the stack ID", "error", err)
			out, err := cli.DescribeStacks(ctx, &aws_cloudformation_v2.DescribeStacksInput{
				StackName: &stackName,
			})
			if err != nil {
				logutil.S().Warnw("describing already existing stack failed", "error", err)
			} else {
				if len(out.Stacks) != 1 {
					logutil.S().Warnw("expected describing already existing stack returning 1", "stacks", len(out.Stacks))
				} else {
					return *out.Stacks[0].StackId, err
				}
			}
		}
		return "", err
	}
	logutil.S().Infow("requests to create stack", "stack-id", *out.StackId)

	return *out.StackId, nil
}

func DeleteStack(
	ctx context.Context,
	cfg aws.Config,
	stackName string,
) error {
	logutil.S().Infow("deleting stack", "stack-name", stackName)

	cli := aws_cloudformation_v2.NewFromConfig(cfg)
	_, err := cli.DeleteStack(ctx, &aws_cloudformation_v2.DeleteStackInput{
		StackName: &stackName,
	})
	if err != nil {
		if StackNotExist(err) {
			logutil.S().Warnw("stack does not exist; ignoring",
				"stack-name", stackName,
				"err", err,
			)
			return nil
		}
		return err
	}

	logutil.S().Infow("delete stack requested", "stack-name", stackName)
	return nil
}

// StackStatus represents the CloudFormation status.
type StackStatus struct {
	Stack aws_cloudformation_v2_types.Stack
	Error error
}

// Poll periodically fetches the stack status
// until the stack becomes the desired state.
// TODO: check retryable errors "dial tcp: lookup cloudformation.us-east-1.amazonaws.com: no such host"
func Poll(
	ctx context.Context,
	stopc chan struct{},
	cfg aws.Config,
	stackID string,
	desiredStackStatus aws_cloudformation_v2_types.StackStatus,
	initialWait time.Duration,
	pollInterval time.Duration,
) <-chan StackStatus {
	now := time.Now()

	logutil.S().Infow("polling stack",
		"stackID", stackID,
		"want", string(desiredStackStatus),
		"initialWait", initialWait.String(),
		"pollInterval", pollInterval.String(),
		"ctxTimeLeft", ctxutil.TimeLeftTillDeadline(ctx),
	)
	ch := make(chan StackStatus, 10)
	cli := aws_cloudformation_v2.NewFromConfig(cfg)

	go func() {
		// very first poll should be no-wait
		// in case stack has already reached desired status
		// wait from second interation
		interval := time.Duration(0)

		prevStatusReason, first := "", true
		for ctx.Err() == nil {
			select {
			case <-ctx.Done():
				logutil.S().Warnw("wait aborted, ctx done", "err", ctx.Err())
				ch <- StackStatus{Error: ctx.Err()}
				close(ch)
				return

			case <-stopc:
				logutil.S().Warnw("wait stopped, stopc closed", "err", ctx.Err())
				ch <- StackStatus{Error: errors.New("wait stopped")}
				close(ch)
				return

			case <-time.After(interval):
				// very first poll should be no-wait
				// in case stack has already reached desired status
				// wait from second interation
				if interval == time.Duration(0) {
					interval = pollInterval
				}
			}

			output, err := cli.DescribeStacks(
				ctx,
				&aws_cloudformation_v2.DescribeStacksInput{
					StackName: aws.String(stackID),
				},
			)
			if err != nil {
				if StackNotExist(err) {
					if desiredStackStatus == aws_cloudformation_v2_types.StackStatusDeleteComplete {
						logutil.S().Infow("stack is already deleted as desired; exiting", "err", err)
						ch <- StackStatus{Error: nil}
						close(ch)
						return
					}

					logutil.S().Warnw("stack does not exist; aborting", "err", err)
					ch <- StackStatus{Error: err}
					close(ch)
					return
				}

				logutil.S().Warnw("describe stack failed; retrying", "err", err)
				ch <- StackStatus{Error: err}
				continue
			}

			if len(output.Stacks) != 1 {
				logutil.S().Warnw("expected only 1 stack; retrying", "stacks", fmt.Sprintf("%v", output))
				ch <- StackStatus{Error: fmt.Errorf("unexpected stack response %+v", *output)}
				continue
			}

			stack := output.Stacks[0]
			currentStatus := stack.StackStatus
			currentStatusReason := ""
			if stack.StackStatusReason != nil {
				currentStatusReason = *stack.StackStatusReason
			}
			if prevStatusReason == "" {
				prevStatusReason = currentStatusReason
			} else if currentStatusReason != "" && prevStatusReason != currentStatusReason {
				prevStatusReason = currentStatusReason
			}

			logutil.S().Infow("poll",
				"stack-name", *stack.StackName,
				"desired", string(desiredStackStatus),
				"current", string(currentStatus),
				"current-reason", currentStatusReason,
				"started", humanize.RelTime(now, time.Now(), "ago", "from now"),
				"ctx-time-left", ctxutil.TimeLeftTillDeadline(ctx),
			)
			if desiredStackStatus != aws_cloudformation_v2_types.StackStatusDeleteComplete &&
				currentStatus == aws_cloudformation_v2_types.StackStatusDeleteComplete {
				logutil.S().Warnw("stack failed thus deleted; aborting")
				ch <- StackStatus{
					Stack: stack,
					Error: fmt.Errorf("stack failed thus deleted (previous status reason %q, current stack status %q, current status reason %q)",
						prevStatusReason,
						currentStatus,
						currentStatusReason,
					)}
				close(ch)
				return
			}

			if desiredStackStatus == aws_cloudformation_v2_types.StackStatusDeleteComplete &&
				currentStatus == aws_cloudformation_v2_types.StackStatusDeleteFailed {
				logutil.S().Warnw("delete stack failed; aborting")
				ch <- StackStatus{
					Stack: stack,
					Error: fmt.Errorf("failed to delete stack (previous status reason %q, current stack status %q, current status reason %q)",
						prevStatusReason,
						currentStatus,
						currentStatusReason,
					)}
				close(ch)
				return
			}

			ch <- StackStatus{Stack: stack, Error: nil}
			if currentStatus == desiredStackStatus {
				logutil.S().Infow("desired stack status; done", "current-stack-status", string(currentStatus))
				close(ch)
				return
			}

			if first {
				logutil.S().Infow("sleeping", "initial-wait", initialWait.String())
				select {
				case <-ctx.Done():
					logutil.S().Warnw("wait aborted, ctx done", "err", ctx.Err())
					ch <- StackStatus{Error: ctx.Err()}
					close(ch)
					return
				case <-stopc:
					logutil.S().Warnw("wait stopped, stopc closed", "err", ctx.Err())
					ch <- StackStatus{Error: errors.New("wait stopped")}
					close(ch)
					return
				case <-time.After(initialWait):
				}
				first = false
			}

			// continue for-loop
		}

		logutil.S().Warnw("wait aborted, ctx done", "err", ctx.Err())
		ch <- StackStatus{Error: ctx.Err()}
		close(ch)
	}()
	return ch
}

// StackCreateFailed return true if cloudformation status indicates its creation failure.
//
//	CREATE_IN_PROGRESS
//	CREATE_FAILED
//	CREATE_COMPLETE
//	ROLLBACK_IN_PROGRESS
//	ROLLBACK_FAILED
//	ROLLBACK_COMPLETE
//	DELETE_IN_PROGRESS
//	DELETE_FAILED
//	DELETE_COMPLETE
//	UPDATE_IN_PROGRESS
//	UPDATE_COMPLETE_CLEANUP_IN_PROGRESS
//	UPDATE_COMPLETE
//	UPDATE_ROLLBACK_IN_PROGRESS
//	UPDATE_ROLLBACK_FAILED
//	UPDATE_ROLLBACK_COMPLETE_CLEANUP_IN_PROGRESS
//	UPDATE_ROLLBACK_COMPLETE
//	REVIEW_IN_PROGRESS
//
// ref. https://docs.aws.amazon.com/AWSCloudFormation/latest/APIReference/API_Stack.html
func StackCreateFailed(status string) bool {
	return !strings.HasPrefix(status, "REVIEW_") && !strings.HasPrefix(status, "CREATE_")
}

// StackNotExist returns true if cloudformation errror indicates
// that the stack has already been deleted.
// This message is Go client specific.
// e.g. ValidationError: Stack with id AWSTESTER-155460CAAC98A17003-CF-STACK-VPC does not exist\n\tstatus code: 400, request id: bf45410b-b863-11e8-9550-914acc220b7c
func StackNotExist(err error) bool {
	if err == nil {
		return false
	}
	return strings.Contains(err.Error(), "ValidationError:") && strings.Contains(err.Error(), " does not exist")
}

func NewParameters(m map[string]string) (params []aws_cloudformation_v2_types.Parameter) {
	for k, v := range m {
		k, v := k, v
		params = append(params, aws_cloudformation_v2_types.Parameter{
			ParameterKey:   &k,
			ParameterValue: &v,
		})
	}
	return params
}

func NewTags(m map[string]string) (tags []aws_cloudformation_v2_types.Tag) {
	for k, v := range m {
		k, v := k, v
		tags = append(tags, aws_cloudformation_v2_types.Tag{Key: &k, Value: &v})
	}
	return tags
}
