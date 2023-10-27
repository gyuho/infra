package aws

import (
	"context"

	aws_sts_v2 "github.com/aws/aws-sdk-go-v2/service/sts"
)

func GetCallerIdentity(ctx context.Context) (Identity, error) {
	cfg, err := New(&Config{Region: "us-east-1"})
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
