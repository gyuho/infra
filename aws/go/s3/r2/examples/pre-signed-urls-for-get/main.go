package main

import (
	"context"
	"fmt"
	"os"
	"strconv"
	"time"

	"github.com/gyuho/infra/aws/go/s3"
	"github.com/gyuho/infra/aws/go/s3/r2"
	"github.com/gyuho/infra/go/logutil"

	"github.com/dustin/go-humanize"
)

/*
CLOUDFLARE_ACCOUNT_ID="" \
CLOUDFLARE_ACCESS_KEY_ID="" \
CLOUDFLARE_ACCESS_KEY_SECRET="" \
go run main.go stanworld 432000
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

	privateBucket := os.Args[1]
	lifetime := os.Args[2]
	parsedLifetime, err := strconv.ParseInt(lifetime, 10, 64)
	if err != nil {
		panic(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 3*time.Minute)
	objects, err := s3.ListObjects(ctx, cfg, privateBucket)
	cancel()
	if err != nil {
		panic(err)
	}

	for _, obj := range objects.Objects {
		key, size := *obj.Key, *obj.Size
		logutil.S().Infow("object", "key", key, "size", humanize.Bytes(uint64(size)))

		time.Sleep(5 * time.Second)

		ctx, cancel = context.WithTimeout(context.Background(), 3*time.Minute)
		preSignedURLForGet, err := s3.CreatePreSignedURLForGet(ctx, cfg, privateBucket, key, parsedLifetime)
		cancel()
		if err != nil {
			panic(err)
		}

		fmt.Printf("%q, %s\n%s\n\n", key, humanize.Bytes(uint64(size)), preSignedURLForGet)
	}
}
