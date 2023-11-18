package cloudflare

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/gyuho/infra/aws/go/s3"
	"github.com/gyuho/infra/go/httputil"
	"github.com/gyuho/infra/go/randutil"

	aws_s3_v2_types "github.com/aws/aws-sdk-go-v2/service/s3/types"
)

func TestCloudflarePrivate(t *testing.T) {
	if os.Getenv("RUN_CLOUDFLARE_TESTS") != "1" {
		t.Skip()
	}

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	cfg, err := NewAWSCompatibleConfig(
		ctx,
		"",
		os.Getenv("CLOUDFLARE_ACCOUNT_ID"),
		os.Getenv("CLOUDFLARE_ACCESS_KEY_ID"),
		os.Getenv("CLOUDFLARE_ACCESS_KEY_SECRET"),
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	buckets, err := s3.ListBuckets(ctx, cfg)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	for _, bucket := range buckets {
		t.Logf("bucket %s (created %s)", bucket.Name, bucket.Created)
	}

	privateBucket := randutil.AlphabetsLowerCase(10)

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.CreateBucket(
		ctx,
		cfg,
		privateBucket,
		s3.WithObjectOwnership(aws_s3_v2_types.ObjectOwnership("")), // otherwise, "Header 'x-amz-object-ownership' with value 'BucketOwnerPreferred' not implemented"
		s3.WithSkipBucketPolicy(true),                               // otherwise, "NotImplemented: PutPublicAccessBlock not implemented"
		s3.WithServerSideEncryption(false),
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.CreateBucket(
		ctx,
		cfg,
		privateBucket,
		s3.WithObjectOwnership(aws_s3_v2_types.ObjectOwnership("")), // otherwise, "Header 'x-amz-object-ownership' with value 'BucketOwnerPreferred' not implemented"
		s3.WithSkipBucketPolicy(true),                               // otherwise, "NotImplemented: PutPublicAccessBlock not implemented"
		s3.WithServerSideEncryption(false),
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
	err = s3.PutObject(ctx, cfg, localFile1, privateBucket, s3Key1, s3.WithMetadata(map[string]string{"a": "b"}))
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
	err = s3.PutObject(ctx, cfg, localFile2, privateBucket, s3Key2, s3.WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1New := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile1New)
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.GetObject(ctx, cfg, privateBucket, s3Key1, localFile1New)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile2New := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
	defer os.RemoveAll(localFile2New)
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.GetObject(ctx, cfg, privateBucket, s3Key2, localFile2New)
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
	preSignedURL, err := s3.CreateGetPreSignedURL(ctx, cfg, privateBucket, s3Key2)
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
	err = s3.DeleteObjects(ctx, cfg, privateBucket, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.DeleteObjects(ctx, cfg, privateBucket, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.DeleteBucket(ctx, cfg, privateBucket)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.DeleteBucket(ctx, cfg, privateBucket)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
