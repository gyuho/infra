package s3

import (
	"context"
	"fmt"
	"net/http"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	v4 "github.com/aws/aws-sdk-go-v2/aws/signer/v4"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

// Presigner encapsulates the Amazon Simple Storage Service (Amazon S3) presign actions
// used in the examples.
// It contains PresignClient, a client that is used to presign requests to Amazon S3.
// Presigned requests contain temporary credentials and can be made from any HTTP client.
type Presigner struct {
	PresignClient *s3.PresignClient
}

// NewPresigner creates a new Presigner.
// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/example_s3_Scenario_PresignedUrl_section.html
func NewPresigner(cfg aws.Config) *Presigner {
	cli := s3.NewFromConfig(cfg)
	return &Presigner{
		PresignClient: s3.NewPresignClient(cli),
	}
}

// Presign creates a presigned request based on the http method that can be used
// to grant temporary public access via a URL.
//
// The presigned request is valid for the specified number of seconds.
// The default lifetime is 900s, or 15 minutes.
//
// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/example_s3_Scenario_PresignedUrl_section.html
func (presigner Presigner) Presign(ctx context.Context, bucket string, objectKey string, lifetimeSecs int64, httpMethod string) (*v4.PresignedHTTPRequest, error) {
	switch httpMethod {
	case http.MethodGet:
		return presigner.PresignClient.PresignGetObject(ctx, &s3.GetObjectInput{
			Bucket: aws.String(bucket),
			Key:    aws.String(objectKey),
		}, func(opts *s3.PresignOptions) {
			opts.Expires = time.Duration(lifetimeSecs * int64(time.Second))
		})

	case http.MethodPut:
		return presigner.PresignClient.PresignPutObject(ctx, &s3.PutObjectInput{
			Bucket: aws.String(bucket),
			Key:    aws.String(objectKey),
		}, func(opts *s3.PresignOptions) {
			opts.Expires = time.Duration(lifetimeSecs * int64(time.Second))
		})

	case http.MethodDelete:
		return presigner.PresignClient.PresignDeleteObject(ctx, &s3.DeleteObjectInput{
			Bucket: aws.String(bucket),
			Key:    aws.String(objectKey),
		}, func(opts *s3.PresignOptions) {
			opts.Expires = time.Duration(lifetimeSecs * int64(time.Second))
		})

	default:
		return nil, fmt.Errorf("unsupported method: %s", httpMethod)
	}
}

// CreatePreSignedURLForGet creates a presigned URL to get an object from a bucket.
// The presigned request is valid for the specified number of seconds.
// The default lifetime is 900s, or 15 minutes.
// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/example_s3_Scenario_PresignedUrl_section.html
func CreatePreSignedURLForGet(ctx context.Context, cfg aws.Config, bucket string, objectkey string, lifetimeSecs int64) (string, error) {
	presigner := NewPresigner(cfg)
	req, err := presigner.Presign(ctx, bucket, objectkey, lifetimeSecs, http.MethodGet)
	if err != nil {
		return "", err
	}
	return req.URL, nil
}

// CreatePreSignedURLForPut creates a presigned URL to put an object in a bucket.
// The presigned request is valid for the specified number of seconds.
// The default lifetime is 900s, or 15 minutes.
// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/example_s3_Scenario_PresignedUrl_section.html
func CreatePreSignedURLForPut(ctx context.Context, cfg aws.Config, bucket string, objectkey string, lifetimeSecs int64) (string, error) {
	presigner := NewPresigner(cfg)
	req, err := presigner.Presign(ctx, bucket, objectkey, lifetimeSecs, http.MethodPut)
	if err != nil {
		return "", err
	}
	return req.URL, nil
}

// CreatePreSignedURLForDelete creates a presigned URL to delete an object from a bucket.
// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/example_s3_Scenario_PresignedUrl_section.html
func CreatePreSignedURLForDelete(ctx context.Context, cfg aws.Config, bucket string, objectkey string, lifetimeSecs int64) (string, error) {
	presigner := NewPresigner(cfg)
	req, err := presigner.Presign(ctx, bucket, objectkey, lifetimeSecs, http.MethodDelete)
	if err != nil {
		return "", err
	}
	return req.URL, nil
}
