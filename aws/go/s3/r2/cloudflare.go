package r2

import (
	"context"
	"fmt"

	aws_v2 "github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
)

// Creates a new Cloudflare client compatible with AWS S3.
// ref. https://developers.cloudflare.com/r2/examples/aws/aws-sdk-go/
// ref. https://developers.cloudflare.com/r2/api/s3/api/
func NewAWSCompatibleConfig(ctx context.Context, accountID string, accessKeyID string, accessKeySecret string, opts ...OpOption) (aws_v2.Config, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	cfURL := fmt.Sprintf("https://%s.r2.cloudflarestorage.com", accountID)
	if ret.region == "eu" {
		cfURL = fmt.Sprintf("https://%s.eu.r2.cloudflarestorage.com", accountID)
	}
	resolver := aws_v2.EndpointResolverWithOptionsFunc(func(service, region string, options ...interface{}) (aws_v2.Endpoint, error) {
		return aws_v2.Endpoint{
			URL: cfURL,
		}, nil
	})

	return config.LoadDefaultConfig(ctx,
		config.WithEndpointResolverWithOptions(resolver),
		config.WithRegion(ret.region),
		config.WithCredentialsProvider(credentials.NewStaticCredentialsProvider(accessKeyID, accessKeySecret, "")),
	)
}

type Op struct {
	region string
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithRegion(region string) OpOption {
	return func(op *Op) {
		op.region = region
	}
}
