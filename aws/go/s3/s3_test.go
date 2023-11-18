package s3

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/httputil"
	"github.com/gyuho/infra/go/randutil"

	aws_s3_v2_types "github.com/aws/aws-sdk-go-v2/service/s3/types"
)

func TestS3PrivatePreSigned(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	buckets, err := ListBuckets(ctx, cfg)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	for _, bucket := range buckets {
		t.Logf("bucket %s (created %s)", bucket.Name, bucket.Created)
	}

	privateBucket := randutil.AlphabetsLowerCase(10)

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(
		ctx,
		cfg,
		privateBucket,
		WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
		WithBucketBlockPublicACLs(false),
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(
		ctx,
		cfg,
		privateBucket,
		WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
		WithBucketBlockPublicACLs(false),
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1, s3Key1 := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10)), filepath.Join(randutil.AlphabetsLowerCase(10), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile1)
	localFile1b := []byte(randutil.AlphabetsLowerCase(100))
	err = os.WriteFile(localFile1, localFile1b, 0644)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = PutObject(ctx, cfg, localFile1, privateBucket, s3Key1, WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile2, s3Key2 := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10)), filepath.Join(randutil.AlphabetsLowerCase(10), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile2)
	localFile2b := []byte(randutil.AlphabetsLowerCase(100))
	err = os.WriteFile(localFile2, localFile2b, 0644)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = PutObject(ctx, cfg, localFile2, privateBucket, s3Key2, WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1New := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile1New)
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = GetObject(ctx, cfg, privateBucket, s3Key1, localFile1New)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile2New := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile2New)
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = GetObject(ctx, cfg, privateBucket, s3Key2, localFile2New)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1Newb, err := os.ReadFile(localFile1New)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(localFile1b, localFile1Newb) {
		t.Fatalf("localFile1b != localFile1Newb: %s != %s", string(localFile1b), string(localFile1Newb))
	}

	localFile2Newb, err := os.ReadFile(localFile2New)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(localFile2b, localFile2Newb) {
		t.Fatalf("localFile2b != localFile2Newb: %s != %s", string(localFile2b), string(localFile2Newb))
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	preSignedURL, err := CreateGetPreSignedURL(ctx, cfg, privateBucket, s3Key2)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(time.Second)
	tmpFile, err := httputil.DownloadFileToTmp(preSignedURL)
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpFile)

	tmpFileb, err := os.ReadFile(tmpFile)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(tmpFileb, localFile2Newb) {
		t.Fatalf("tmpFileb != localFile2Newb: %s != %s", string(tmpFileb), string(localFile2Newb))
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteObjects(ctx, cfg, privateBucket, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteObjects(ctx, cfg, privateBucket, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteBucket(ctx, cfg, privateBucket)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteBucket(ctx, cfg, privateBucket)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}

func TestS3Public(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	publicBucket := randutil.AlphabetsLowerCase(10)

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(
		ctx,
		cfg,
		publicBucket,
		WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
	)
	cancel()
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

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteObjects(ctx, cfg, publicBucket, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteObjects(ctx, cfg, publicBucket, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteBucket(ctx, cfg, publicBucket)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteBucket(ctx, cfg, publicBucket)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}

func downloadFileToTmp(url string) (string, error) {
	resp, err := http.Get(url)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("failed to download file: %s", resp.Status)
	}

	file := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
	f, err := os.Create(file)
	if err != nil {
		return "", err
	}
	defer f.Close()

	_, err = io.Copy(f, resp.Body)
	if err != nil {
		return "", err
	}
	return file, nil
}
