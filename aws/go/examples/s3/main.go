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
)

func main() {
	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		panic(err)
	}

	bucketName := randutil.String(20)
	logutil.S().Infow("bucket name", "bucket", bucketName)

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	err = s3.CreateBucket(ctx, cfg, bucketName, s3.WithPublicRead(true))
	cancel()
	if err != nil {
		panic(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.CreateBucket(ctx, cfg, bucketName, s3.WithPublicRead(true))
	cancel()
	if err != nil {
		panic(err)
	}

	localFile1, s3Key1 := filepath.Join(os.TempDir(), randutil.String(10)), filepath.Join(randutil.String(10), randutil.String(10))
	defer os.RemoveAll(localFile1)
	err = os.WriteFile(localFile1, randutil.Bytes(100), 0644)
	if err != nil {
		panic(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.PutObject(ctx, cfg, localFile1, bucketName, s3Key1, s3.WithPublicRead(true), s3.WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		panic(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	out, err := s3.ObjectExists(ctx, cfg, bucketName, s3Key1)
	cancel()
	if err != nil {
		panic(err)
	}
	logutil.S().Infow("object exists", "exists", out)

	localFile2, s3Key2 := filepath.Join(os.TempDir(), randutil.String(10)), filepath.Join(randutil.String(10), randutil.String(10))
	defer os.RemoveAll(localFile2)
	err = os.WriteFile(localFile2, randutil.Bytes(100), 0644)
	if err != nil {
		panic(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.PutObject(ctx, cfg, localFile2, bucketName, s3Key2, s3.WithMetadata(map[string]string{"a": "b"}))
	cancel()
	if err != nil {
		panic(err)
	}

	localFile1New := filepath.Join(os.TempDir(), randutil.String(10))
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.GetObject(ctx, cfg, bucketName, s3Key1, localFile1New)
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
	err = s3.DeleteObjects(ctx, cfg, bucketName, "")
	cancel()
	if err != nil {
		panic(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = s3.DeleteBucket(ctx, cfg, bucketName)
	cancel()
	if err != nil {
		panic(err)
	}
}
