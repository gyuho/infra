package ec2

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2 "github.com/aws/aws-sdk-go-v2/service/ec2"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
	aws_sts_v2 "github.com/aws/aws-sdk-go-v2/service/sts"
)

// Creates an AMI based off an instance.
func CreateImage(ctx context.Context, cfg aws.Config, instanceID string, name string, tags map[string]string) (string, error) {
	logutil.S().Infow("creating an AMI", "instanceID", instanceID, "name", name)

	ts := ConvertTags(name, tags)
	cli := aws_ec2_v2.NewFromConfig(cfg)
	out, err := cli.CreateImage(
		ctx,
		&aws_ec2_v2.CreateImageInput{
			InstanceId: &instanceID,

			Name:        &name,
			Description: &name,

			TagSpecifications: []aws_ec2_v2_types.TagSpecification{
				{
					ResourceType: aws_ec2_v2_types.ResourceTypeImage,
					Tags:         ts,
				},
			},
		},
	)
	if err != nil {
		return "", err
	}

	imgID := *out.ImageId
	logutil.S().Infow("successfully created an AMI", "instanceID", instanceID, "name", name, "imageID", imgID)
	return imgID, nil
}

func PollImageUntilAvailable(ctx context.Context, cfg aws.Config, imageID string, interval time.Duration) (aws_ec2_v2_types.Image, error) {
	logutil.S().Infow("polling an AMI", "imageID", imageID)
	cli := aws_ec2_v2.NewFromConfig(cfg)

	start := time.Now()
	cnt := 0
	for {
		itv := interval
		if cnt == 0 {
			itv = time.Second
		}
		select {
		case <-ctx.Done():
			return aws_ec2_v2_types.Image{}, ctx.Err()
		case <-time.After(itv):
			cnt++
		}

		out, err := cli.DescribeImages(ctx, &aws_ec2_v2.DescribeImagesInput{
			ImageIds: []string{imageID},
		})
		if err != nil {
			return aws_ec2_v2_types.Image{}, err
		}
		if len(out.Images) > 1 {
			return aws_ec2_v2_types.Image{}, errors.New("multiple images found")
		}
		if len(out.Images) < 1 {
			return aws_ec2_v2_types.Image{}, errors.New("image not found")
		}
		img := out.Images[0]

		state := img.State

		elapsed := time.Since(start)
		logutil.S().Infow("polling image",
			"imageID", imageID,
			"state", state,
			"took", elapsed,
		)
		if state == aws_ec2_v2_types.ImageStateAvailable {
			return img, nil
		}
	}
}

type ImageShareTarget struct {
	AccountID string `json:"account_id"`
	Region    string `json:"region"`
}

type Image struct {
	AccountID        string   `json:"account_id"`
	Region           string   `json:"region"`
	ID               string   `json:"id"`
	SharedAccountIDs []string `json:"shared_account_ids"`
}

// Shares an AMI with another account and across regions.
func ShareImage(ctx context.Context, cfg aws.Config, imageID string, targets ...ImageShareTarget) (map[string]Image, error) {
	stsCli := aws_sts_v2.NewFromConfig(cfg)
	stsOut, err := stsCli.GetCallerIdentity(ctx, &aws_sts_v2.GetCallerIdentityInput{})
	if err != nil {
		return nil, err
	}
	accountID := *stsOut.Account

	logutil.S().Infow("sharing an AMI",
		"accountID", accountID,
		"region", cfg.Region,
		"imageID", imageID,
	)

	cli := aws_ec2_v2.NewFromConfig(cfg)
	imgOut, err := cli.DescribeImages(ctx, &aws_ec2_v2.DescribeImagesInput{
		ImageIds: []string{imageID},
	})
	if err != nil {
		return nil, err
	}
	if len(imgOut.Images) > 1 {
		return nil, errors.New("multiple images found")
	}
	if len(imgOut.Images) < 1 {
		return nil, errors.New("image not found")
	}
	sourceImg := imgOut.Images[0]
	if sourceImg.State != aws_ec2_v2_types.ImageStateAvailable {
		return nil, errors.New("image not available")
	}

	imgs := make(map[string]Image)
	imgs[cfg.Region] = Image{
		AccountID: accountID,
		Region:    cfg.Region,
		ID:        imageID,
	}

	// copy routine
	for _, target := range targets {
		if target.Region == cfg.Region {
			continue
		}
		if _, ok := imgs[target.Region]; ok {
			logutil.S().Infow("already copied or exists in the region", "targetRegion", target.Region)
			continue
		}

		name2 := "copy." + *sourceImg.Name
		logutil.S().Infow("copying an AMI within the same account cross region first",
			"fromAccount", accountID,
			"targetAccount", target.AccountID,
			"fromRegion", cfg.Region,
			"targetRegion", target.Region,
			"name", name2,
		)

		targetCfg := cfg
		targetCfg.Region = target.Region
		targetCli := aws_ec2_v2.NewFromConfig(targetCfg)
		copyOut, err := targetCli.CopyImage(ctx, &aws_ec2_v2.CopyImageInput{
			Name:          &name2,
			CopyImageTags: aws.Bool(true),
			SourceImageId: &imageID,
			SourceRegion:  &cfg.Region,
		})
		if err != nil {
			return nil, err
		}

		imgID2 := *copyOut.ImageId
		img := Image{
			AccountID: accountID,
			Region:    target.Region,
			ID:        imgID2,
		}
		imgs[target.Region] = img

		logutil.S().Infow("copied an AMI within the same account cross region first",
			"fromAccount", accountID,
			"targetAccount", target.AccountID,
			"fromRegion", cfg.Region,
			"targetRegion", target.Region,
			"name", name2,
			"imageID", imgID2,
		)
	}

	logutil.S().Infow("waiting for minutes for copied AMIs to be available", "imagesToPoll", len(imgs))
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-time.After(3 * time.Minute):
	}

	for region, img := range imgs {
		copiedCfg := cfg
		copiedCfg.Region = region

		_, err = PollImageUntilAvailable(ctx, copiedCfg, img.ID, time.Minute)
		if err != nil {
			return nil, err
		}
	}

	logutil.S().Infow("now sharing AMIs with other accounts", "allImages", len(imgs))
	for _, target := range targets {
		if target.AccountID == accountID {
			continue
		}

		sourceImg, ok := imgs[target.Region]
		if !ok {
			return nil, fmt.Errorf("image not found for the region %q", target.Region)
		}

		// different account ID but same region requires sharing
		logutil.S().Infow("now sharing an AMI with another account",
			"region", target.Region,
			"sourceImage", sourceImg.ID,
			"targetAccountID", target.AccountID,
		)
		copiedCfg := cfg
		copiedCfg.Region = target.Region
		cli2 := aws_ec2_v2.NewFromConfig(copiedCfg)

		// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/sharingamis-explicit.html#sharingamis-aws-cli
		_, err = cli2.ModifyImageAttribute(ctx, &aws_ec2_v2.ModifyImageAttributeInput{
			ImageId: &sourceImg.ID,
			LaunchPermission: &aws_ec2_v2_types.LaunchPermissionModifications{
				Add: []aws_ec2_v2_types.LaunchPermission{
					{
						UserId: &target.AccountID,
					},
				},
			},
		})
		if err != nil {
			return nil, err
		}

		found := false
		for i := 0; i < 10; i++ {
			out, err := cli2.DescribeImageAttribute(ctx, &aws_ec2_v2.DescribeImageAttributeInput{
				Attribute: aws_ec2_v2_types.ImageAttributeNameLaunchPermission,
				ImageId:   &sourceImg.ID,
			})
			if err != nil {
				return nil, err
			}

			for _, perm := range out.LaunchPermissions {
				logutil.S().Infow("launch permission",
					"region", target.Region,
					"sourceImage", sourceImg,
					"userId", *perm.UserId,
				)
				if *perm.UserId == target.AccountID {
					found = true
					break
				}
			}
			if found {
				break
			}

			time.Sleep(5 * time.Second)
		}
		if !found {
			return nil, errors.New("failed to share an AMI -- expected launch permission not found")
		}

		sourceImg.SharedAccountIDs = append(sourceImg.SharedAccountIDs, target.AccountID)
		imgs[target.Region] = sourceImg
	}

	return imgs, nil
}
