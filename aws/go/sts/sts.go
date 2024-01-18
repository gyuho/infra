// Package sts implements STS utils.
package sts

import (
	"context"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/logutil"

	aws_v2 "github.com/aws/aws-sdk-go-v2/aws"
	config_v2 "github.com/aws/aws-sdk-go-v2/config"
	credentials_v2 "github.com/aws/aws-sdk-go-v2/credentials"
	aws_sts_v2 "github.com/aws/aws-sdk-go-v2/service/sts"
)

func GetCallerIdentity(ctx context.Context) (Identity, error) {
	cfg, err := aws.New(&aws.Config{Region: "us-east-1"})
	if err != nil {
		return Identity{}, err
	}
	cli := aws_sts_v2.NewFromConfig(cfg)

	out, err := cli.GetCallerIdentity(ctx, &aws_sts_v2.GetCallerIdentityInput{})
	if err != nil {
		return Identity{}, err
	}

	return Identity{
		AccountID: *out.Account,
		RoleARN:   *out.Arn,
		UserID:    *out.UserId,
	}, nil
}

type Identity struct {
	AccountID string `json:"account_id"`
	RoleARN   string `json:"role_arn"`
	UserID    string `json:"user_id"`
}

func AssumeRole(ctx context.Context, roleARN string, accessKey string, secretKey string, durationSecs int32) (*aws_sts_v2.AssumeRoleOutput, error) {
	logutil.S().Infow("assuming role", "arn", roleARN, "durationSecs", durationSecs)

	if durationSecs > 43200 { // 12-hour max
		logutil.S().Warnw("durationSecs is too long, setting to 43200", "durationSecs", durationSecs)
		durationSecs = 43200
	}

	cfg, err := config_v2.LoadDefaultConfig(
		ctx,
		config_v2.WithCredentialsProvider(credentials_v2.StaticCredentialsProvider{
			Value: aws_v2.Credentials{
				AccessKeyID:     accessKey,
				SecretAccessKey: secretKey,
			},
		}),
	)
	if err != nil {
		return nil, err
	}
	cli := aws_sts_v2.NewFromConfig(cfg)

	input := &aws_sts_v2.AssumeRoleInput{
		RoleArn:         &roleARN,
		RoleSessionName: aws_v2.String("AssumedRoleSession"),
		DurationSeconds: aws_v2.Int32(durationSecs),
	}
	return cli.AssumeRole(ctx, input)
}
