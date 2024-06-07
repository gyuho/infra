package main

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/aws/go/s3"
	"github.com/gyuho/infra/go/logutil"
	"github.com/gyuho/infra/go/randutil"

	aws_s3_v2_types "github.com/aws/aws-sdk-go-v2/service/s3/types"
)

func main() {
	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		panic(err)
	}

	publicBucket := randutil.StringAlphabetsLowerCase(20)
	logutil.S().Infow("bucket name", "bucket", publicBucket)

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	err = s3.CreateBucket(
		ctx,
		cfg,
		publicBucket,
		s3.WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
		s3.WithBucketBlockPublicACLs(false),
		s3.WithBucketBlockPublicPolicy(false),
		s3.WithBucketIgnorePublicACLs(true),
		s3.WithBucketRestrictPublicBuckets(false),

		// PutBucketAcl with aws_s3_v2_types.BucketCannedACLPublicRead will fail:
		// "AccessControlListNotSupported: The bucket does not allow ACLs"
		s3.WithBucketPolicy(`{
	"Version": "2012-10-17",
	"Statement": [
		{
			"Sid": "PublicReadGetObject",
			"Effect": "Allow",
			"Principal": "*",
			"Action": "s3:GetObject",
			"Resource": "arn:aws:s3:::`+publicBucket+`/*"
		}
	]
}`),
	)
	cancel()
	if err != nil {
		panic(err)
	}

	localFile1, s3Key1 := filepath.Join(os.TempDir(), randutil.StringAlphabetsLowerCase(10)), filepath.Join(randutil.StringAlphabetsLowerCase(10), randutil.StringAlphabetsLowerCase(10))
	defer os.RemoveAll(localFile1)
	err = os.WriteFile(localFile1, randutil.BytesAlphabetsLowerCaseNumeric(100), 0644)
	if err != nil {
		panic(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.PutObject(
		ctx,
		cfg,
		localFile1,
		publicBucket,
		s3Key1,
		s3.WithObjectACL(aws_s3_v2_types.ObjectCannedACLPublicRead),
		s3.WithMetadata(map[string]string{"a": "b"}),
	)
	cancel()
	if err != nil {
		panic(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	out, err := s3.ObjectExists(ctx, cfg, publicBucket, s3Key1)
	cancel()
	if err != nil {
		panic(err)
	}
	logutil.S().Infow("object exists", "exists", out)

	localFile2, s3Key2 := filepath.Join(os.TempDir(), randutil.StringAlphabetsLowerCase(10)), filepath.Join(randutil.StringAlphabetsLowerCase(10), randutil.StringAlphabetsLowerCase(10))
	defer os.RemoveAll(localFile2)
	err = os.WriteFile(localFile2, randutil.BytesAlphabetsLowerCaseNumeric(100), 0644)
	if err != nil {
		panic(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.PutObject(ctx, cfg, localFile2, publicBucket, s3Key2, s3.WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		panic(err)
	}

	localFile1New := filepath.Join(os.TempDir(), randutil.StringAlphabetsLowerCase(10))
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.GetObject(ctx, cfg, publicBucket, s3Key1, localFile1New)
	cancel()
	if err != nil {
		panic(err)
	}

	localFile1b, err := os.ReadFile(localFile1)
	if err != nil {
		panic(err)
	}
	localFile1Newb, err := os.ReadFile(localFile1New)
	if err != nil {
		panic(err)
	}
	if !bytes.Equal(localFile1b, localFile1Newb) {
		panic(fmt.Errorf("localFile1b != localFile1Newb: %s != %s", string(localFile1b), string(localFile1Newb)))
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.DeleteObjects(ctx, cfg, publicBucket, "")
	cancel()
	if err != nil {
		panic(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.DeleteBucket(ctx, cfg, publicBucket)
	cancel()
	if err != nil {
		panic(err)
	}
}
