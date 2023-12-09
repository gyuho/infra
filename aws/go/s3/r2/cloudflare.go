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
func NewAWSCompatibleConfig(ctx context.Context, region string, accountID string, accessKeyID string, accessKeySecret string) (aws_v2.Config, error) {
	cfURL := fmt.Sprintf("https://%s.r2.cloudflarestorage.com", accountID)
	if region == "eu" {
		cfURL = fmt.Sprintf("https://%s.eu.r2.cloudflarestorage.com", accountID)
	}
	resolver := aws_v2.EndpointResolverWithOptionsFunc(func(service, region string, options ...interface{}) (aws_v2.Endpoint, error) {
		return aws_v2.Endpoint{
			URL: cfURL,
		}, nil
	})

	return config.LoadDefaultConfig(ctx,
		config.WithEndpointResolverWithOptions(resolver),
		config.WithCredentialsProvider(credentials.NewStaticCredentialsProvider(accessKeyID, accessKeySecret, "")),
	)
}
