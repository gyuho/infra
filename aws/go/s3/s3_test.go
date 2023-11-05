package s3

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/randutil"
)

func TestS3(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	bucketName := randutil.String(10)

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(ctx, cfg, bucketName, WithPublicRead(true))
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(ctx, cfg, bucketName, WithPublicRead(true))
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1, s3Key1 := filepath.Join(os.TempDir(), randutil.String(10)), filepath.Join(randutil.String(10), randutil.String(10))
	defer os.RemoveAll(localFile1)
	err = os.WriteFile(localFile1, randutil.Bytes(100), 0644)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = PutObject(ctx, cfg, localFile1, bucketName, s3Key1, WithPublicRead(true), WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	existOut, err := ObjectExists(ctx, cfg, bucketName, s3Key1)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("%+v", *existOut)

	localFile2, s3Key2 := filepath.Join(os.TempDir(), randutil.String(10)), filepath.Join(randutil.String(10), randutil.String(10))
	defer os.RemoveAll(localFile2)
	err = os.WriteFile(localFile2, randutil.Bytes(100), 0644)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = PutObject(ctx, cfg, localFile2, bucketName, s3Key2, WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	existOut, err = ObjectExists(ctx, cfg, bucketName, s3Key2)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("%+v", *existOut)

	localFile1New := filepath.Join(os.TempDir(), randutil.String(10))
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = GetObject(ctx, cfg, bucketName, s3Key1, localFile1New)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile2New := filepath.Join(os.TempDir(), randutil.String(10))
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = GetObject(ctx, cfg, bucketName, s3Key2, localFile2New)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	localFile1b, err := os.ReadFile(localFile1)
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

	localFile2b, err := os.ReadFile(localFile2)
	if err != nil {
		t.Fatal(err)
	}
	localFile2Newb, err := os.ReadFile(localFile2New)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(localFile2b, localFile2Newb) {
		t.Fatalf("localFile2b != localFile2Newb: %s != %s", string(localFile2b), string(localFile2Newb))
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteObjects(ctx, cfg, bucketName, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteObjects(ctx, cfg, bucketName, "")
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteBucket(ctx, cfg, bucketName)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteBucket(ctx, cfg, bucketName)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
