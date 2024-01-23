// Package kms implements KMS utils.
package kms

import (
	"context"
	"strings"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_kms_v2 "github.com/aws/aws-sdk-go-v2/service/kms"
	aws_kms_v2_types "github.com/aws/aws-sdk-go-v2/service/kms/types"
)

// Creates a KMS key.
func Create(
	ctx context.Context,
	cfg aws.Config,
	keyName string,
	keySpec aws_kms_v2_types.KeySpec,
	keyUsage aws_kms_v2_types.KeyUsageType,
	tags map[string]string) (*aws_kms_v2_types.KeyMetadata, error) {
	logutil.S().Infow("creating key", "keyName", keyName, "keySpec", keySpec, "keyUsage", keyUsage)

	tss := make([]aws_kms_v2_types.Tag, 0)
	for k, v := range tags {
		// TODO: remove this in Go 1.22
		// ref. https://go.dev/blog/loopvar-preview
		k, v := k, v

		// otherwise, error with "TagException: Duplicate tag keys"
		if k == "Name" {
			continue
		}

		tss = append(tss, aws_kms_v2_types.Tag{
			TagKey:   &k,
			TagValue: &v,
		})
	}

	cli := aws_kms_v2.NewFromConfig(cfg)
	out, err := cli.CreateKey(ctx, &aws_kms_v2.CreateKeyInput{
		Description: &keyName,
		KeySpec:     keySpec,
		KeyUsage:    keyUsage,
		Tags:        tss,
	})
	if err != nil {
		return nil, err
	}
	keyID := *out.KeyMetadata.KeyId

	logutil.S().Infow("successfully created key", "keyName", keyName, "keySpec", keySpec, "keyUsage", keyUsage, "keyID", keyID)
	return out.KeyMetadata, nil
}

// Describes the key either by its key ID or ARN.
func Describe(ctx context.Context, cfg aws.Config, id string) (*aws_kms_v2_types.KeyMetadata, error) {
	logutil.S().Infow("describing a key", "id", id)
	cli := aws_kms_v2.NewFromConfig(cfg)
	out, err := cli.DescribeKey(ctx, &aws_kms_v2.DescribeKeyInput{
		KeyId: &id,
	})
	if err != nil {
		return nil, err
	}
	return out.KeyMetadata, err
}

func GetPublicKey(ctx context.Context, cfg aws.Config, id string) (*aws_kms_v2.GetPublicKeyOutput, error) {
	logutil.S().Infow("getting public key", "id", id)
	cli := aws_kms_v2.NewFromConfig(cfg)
	out, err := cli.GetPublicKey(ctx, &aws_kms_v2.GetPublicKeyInput{
		KeyId: &id,
	})
	if err != nil {
		return nil, err
	}
	return out, err
}

// Delete schedules to delete a key by ID.
func Delete(ctx context.Context, cfg aws.Config, keyID string, pendingWindowInDays int32) error {
	if pendingWindowInDays < 7 {
		pendingWindowInDays = 7
	}
	logutil.S().Infow("scheduling to delete key", "keyID", keyID, "pendingWindowInDays", pendingWindowInDays)

	cli := aws_kms_v2.NewFromConfig(cfg)
	_, err := cli.ScheduleKeyDeletion(ctx, &aws_kms_v2.ScheduleKeyDeletionInput{
		KeyId:               &keyID,
		PendingWindowInDays: &pendingWindowInDays,
	})
	if err != nil {
		if strings.Contains(err.Error(), "is pending deletion") {
			logutil.S().Warnw("key already scheduled for deletion", "error", err)
			return nil
		}
		return err
	}

	logutil.S().Infow("successfully scheduled to delete key", "keyID", keyID)
	return err
}

// ListAliases lists all aliases.
func ListAliases(ctx context.Context, cfg aws.Config) ([]aws_kms_v2_types.AliasListEntry, error) {
	logutil.S().Infow("listing key aliases")

	as := make([]aws_kms_v2_types.AliasListEntry, 0)
	cli := aws_kms_v2.NewFromConfig(cfg)
	marker := ""
	for {
		input := &aws_kms_v2.ListAliasesInput{}
		if marker != "" {
			input.Marker = &marker
		}
		aliases, err := cli.ListAliases(ctx, input)
		if err != nil {
			return nil, err
		}

		as = append(as, aliases.Aliases...)
		if !aliases.Truncated {
			break
		}

		marker = *aliases.NextMarker
	}
	return as, nil
}
