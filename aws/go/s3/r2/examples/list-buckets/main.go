package main

import (
	"context"
	"os"
	"time"

	"github.com/gyuho/infra/aws/go/s3"
	"github.com/gyuho/infra/aws/go/s3/r2"
	"github.com/gyuho/infra/go/logutil"
)

/*
CLOUDFLARE_ACCOUNT_ID="" \
CLOUDFLARE_ACCESS_KEY_ID=""\
CLOUDFLARE_ACCESS_KEY_SECRET="" \
go run main.go
*/
func main() {
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	cfg, err := r2.NewAWSCompatibleConfig(
		ctx,
		os.Getenv("CLOUDFLARE_ACCOUNT_ID"),
		os.Getenv("CLOUDFLARE_ACCESS_KEY_ID"),
		os.Getenv("CLOUDFLARE_ACCESS_KEY_SECRET"),
		r2.WithRegion("apac"),
	)
	cancel()
	if err != nil {
		panic(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	buckets, err := s3.ListBuckets(ctx, cfg)
	cancel()
	if err != nil {
		panic(err)
	}

	for _, bucket := range buckets {
		logutil.S().Infof("bucket %s (created %s)", bucket.Name, bucket.Created)
	}
}
