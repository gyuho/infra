package ec2

import (
	"context"
	"errors"
	"os"
	"strings"
	"time"

	"github.com/gyuho/infra/go/fileutil"
	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

// CreateRSAKeyPair creates a new RSA key pair for EC2.
func CreateRSAKeyPair(ctx context.Context, cfg aws.Config, keyName string, tags map[string]string) (string, error) {
	logutil.S().Infow("creating key pair", "keyName", keyName)

	// delete 'Name', error with "api error InvalidParameterValue: Duplicate tag key 'Name' specified"
	delete(tags, "Name")
	ts := ConvertTags("", tags)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.CreateKeyPair(ctx, &aws_ec2_v2.CreateKeyPairInput{
		KeyName: &keyName,

		KeyFormat: aws_ec2_v2_types.KeyFormatPem,
		KeyType:   aws_ec2_v2_types.KeyTypeRsa,

		TagSpecifications: []aws_ec2_v2_types.TagSpecification{
			{
				ResourceType: aws_ec2_v2_types.ResourceTypeKeyPair,
				Tags:         ts,
			},
		},
	})
	if err != nil {
		return "", err
	}
	keyID := *out.KeyPairId

	logutil.S().Infow("created key pair", "keyID", keyID)
	return keyID, nil
}

// ImportKeyPair imports a public key.
func ImportKeyPair(ctx context.Context, cfg aws.Config, pubKeyPath string, keyName string, tags map[string]string) (string, error) {
	logutil.S().Infow("importing key pair", "pubKeyPath", pubKeyPath, "keyName", keyName)

	fileExists, err := fileutil.FileExists(pubKeyPath)
	if err != nil {
		return "", err
	}
	if !fileExists {
		return "", errors.New("public key file does not exist")
	}
	b, err := os.ReadFile(pubKeyPath)
	if err != nil {
		return "", err
	}

	// delete 'Name', error with "api error InvalidParameterValue: Duplicate tag key 'Name' specified"
	delete(tags, "Name")
	ts := ConvertTags("", tags)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.ImportKeyPair(ctx, &aws_ec2_v2.ImportKeyPairInput{
		KeyName:           &keyName,
		PublicKeyMaterial: b,
		TagSpecifications: []aws_ec2_v2_types.TagSpecification{
			{
				ResourceType: aws_ec2_v2_types.ResourceTypeKeyPair,
				Tags:         ts,
			},
		},
	})
	if err != nil {
		return "", err
	}
	keyID := *out.KeyPairId

	time.Sleep(500 * time.Millisecond)

	dout, err := cli.DescribeKeyPairs(ctx, &aws_ec2_v2.DescribeKeyPairsInput{
		KeyPairIds: []string{keyID},
	})
	if err != nil {
		return "", err
	}
	if len(dout.KeyPairs) != 1 {
		return "", errors.New("failed to describe key pair")
	}
	if *dout.KeyPairs[0].KeyName != keyName {
		return "", errors.New("key pair name mismatch")
	}

	logutil.S().Infow("imported key pair", "keyID", keyID)
	return keyID, nil
}

// DeleteKeyPair deletes an EC2 key pair.
func DeleteKeyPair(ctx context.Context, cfg aws.Config, keyID string) error {
	logutil.S().Infow("deleting key pair", "keyID", keyID)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.DeleteKeyPair(ctx, &aws_ec2_v2.DeleteKeyPairInput{
		KeyPairId: &keyID,
	})
	if err != nil {
		if strings.Contains(err.Error(), "does not exist") {
			logutil.S().Warnw("key pair already deleted", "keyID", keyID, "error", err)
			return nil
		}
		return err
	}
	deleted := *out.Return
	if !deleted {
		return errors.New("failed to delete key pair")
	}

	logutil.S().Infow("deleted key pair", "keyID", keyID)
	return nil
}
