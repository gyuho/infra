package s3

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/randutil"

	aws_s3_v2_types "github.com/aws/aws-sdk-go-v2/service/s3/types"
)

func TestS3Public(t *testing.T) {
	if os.Getenv("RUN_AWS_S3_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	publicBucket := randutil.AlphabetsLowerCase(10)

	// Create twice to ensure the second create does not fail so we can safely retry on
	// creation.
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(
		ctx,
		cfg,
		publicBucket,
		WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
	)
	cancel()

	defer func() { // clean up
		for i := 0; i < 2; i++ { // test twice to ensure the second delete does not fail
			ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
			err = DeleteObjects(ctx, cfg, publicBucket, "")
			cancel()
			if err != nil {
				t.Fatal(err)
			}
		}

		for i := 0; i < 2; i++ { // test twice to ensure the second delete does not fail
			ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
			err = DeleteBucket(ctx, cfg, publicBucket)
			cancel()
			if err != nil {
				t.Fatal(err)
			}
		}
	}()

	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(
		ctx,
		cfg,
		publicBucket,
		WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
		WithBucketBlockPublicACLs(false),
		WithBucketBlockPublicPolicy(false),
		WithBucketIgnorePublicACLs(true),
		WithBucketRestrictPublicBuckets(false),

		// PutBucketAcl with aws_s3_v2_types.BucketCannedACLPublicRead will fail:
		// "AccessControlListNotSupported: The bucket does not allow ACLs"
		WithBucketPolicy(`{
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
		t.Fatal(err)
	}

	localFile1, s3Key1 := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10)), filepath.Join(randutil.AlphabetsLowerCase(10), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile1)
	err = os.WriteFile(localFile1, []byte(randutil.AlphabetsLowerCase(100)), 0644)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = PutObject(
		ctx,
		cfg,
		localFile1,
		publicBucket,
		s3Key1,
		WithObjectACL(aws_s3_v2_types.ObjectCannedACLPublicRead),
		WithMetadata(map[string]string{"a": "b"}),
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile2, s3Key2 := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10)), filepath.Join(randutil.AlphabetsLowerCase(10), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile2)
	err = os.WriteFile(localFile2, []byte(randutil.AlphabetsLowerCase(100)), 0644)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = PutObject(ctx, cfg, localFile2, publicBucket, s3Key2, WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1New := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile1New)
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = GetObject(ctx, cfg, publicBucket, s3Key1, localFile1New)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
