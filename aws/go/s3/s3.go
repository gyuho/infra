// Package s3 implements S3 utils.
package s3

import (
	"bytes"
	"context"
	"io"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/gyuho/infra/go/logutil"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_s3_v2 "github.com/aws/aws-sdk-go-v2/service/s3"
	aws_s3_v2_types "github.com/aws/aws-sdk-go-v2/service/s3/types"
	"github.com/dustin/go-humanize"
	"github.com/olekukonko/tablewriter"
)

type Bucket struct {
	Name    string
	Created time.Time
}

type Buckets []Bucket

func (buckets Buckets) String() string {
	sort.SliceStable(buckets, func(i, j int) bool {
		return buckets[i].Name < buckets[j].Name
	})

	rows := make([][]string, 0, len(buckets))
	for _, v := range buckets {
		row := []string{
			v.Name,
			v.Created.String(),
		}
		rows = append(rows, row)
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"name", "created"})
	tb.AppendBulk(rows)
	tb.Render()

	return buf.String()
}

// ListBuckets lists all buckets.
func ListBuckets(ctx context.Context, cfg aws.Config) (Buckets, error) {
	logutil.S().Infow("listing buckets")

	cli := aws_s3_v2.NewFromConfig(cfg)
	out, err := cli.ListBuckets(ctx, &aws_s3_v2.ListBucketsInput{})
	if err != nil {
		return nil, err
	}

	logutil.S().Infow("listed buckets", "buckets", len(out.Buckets))
	buckets := make([]Bucket, 0, len(out.Buckets))
	for _, b := range out.Buckets {
		buckets = append(buckets, Bucket{Name: *b.Name, Created: *b.CreationDate})
	}
	return Buckets(buckets), nil
}

// BucketExists checks if a bucket exists.
func BucketExists(ctx context.Context, cfg aws.Config, bucketName string) (bool, error) {
	logutil.S().Infow("checking if bucket exists", "bucket", bucketName)

	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err := cli.HeadBucket(ctx, &aws_s3_v2.HeadBucketInput{
		Bucket: &bucketName,
	})
	if err != nil {
		if strings.Contains(err.Error(), "NotFound") {
			return false, nil
		}
		return false, err
	}

	logutil.S().Infow("confirmed that bucket exists", "bucket", bucketName)
	return true, nil
}

// CreateBucket creates a bucket.
func CreateBucket(ctx context.Context, cfg aws.Config, bucketName string, opts ...OpOption) error {
	ret := &Op{}
	ret.applyOpts(opts)

	// https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#canned-acl
	// vs. "public-read"
	//
	// cannot
	// Bucket cannot have public ACLs set with BlockPublicAccess enabled
	// acl = aws_s3_v2_types.BucketCannedACLPublicRead
	acl := aws_s3_v2_types.BucketCannedACLPrivate

	// default is "Bucket owner enforced"
	ownership := aws_s3_v2_types.ObjectOwnershipBucketOwnerEnforced
	if ret.publicRead {
		ownership = aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred
	}

	logutil.S().Infow("creating bucket", "bucket", bucketName, "acl", acl)
	input := &aws_s3_v2.CreateBucketInput{
		Bucket:          &bucketName,
		ACL:             acl,
		ObjectOwnership: ownership,
	}

	// setting LocationConstraint to us-east-1 fails with InvalidLocationConstraint. This region is handled differerntly and must be omitted.
	// https://github.com/boto/boto3/issues/125
	if cfg.Region != "us-east-1" {
		input.CreateBucketConfiguration = &aws_s3_v2_types.CreateBucketConfiguration{
			LocationConstraint: aws_s3_v2_types.BucketLocationConstraint(cfg.Region),
		}
	}

	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err := cli.CreateBucket(ctx, input)
	if err != nil {
		// if already exists, ignore
		if strings.Contains(err.Error(), "BucketAlreadyExists") {
			logutil.S().Warnw("bucket already exists", "bucket", bucketName, "error", err)
			err = nil
		}
		if err != nil && strings.Contains(err.Error(), "BucketAlreadyOwnedByYou") {
			logutil.S().Warnw("bucket already exists", "bucket", bucketName, "error", err)
			err = nil
		}
		if err != nil {
			return err
		}
	}
	logutil.S().Infow("successfully created the bucket", "bucket", bucketName)

	if ret.publicRead {
		logutil.S().Infow("setting public access block", "bucket", bucketName)
		_, err = cli.PutPublicAccessBlock(ctx, &aws_s3_v2.PutPublicAccessBlockInput{
			Bucket: &bucketName,
			PublicAccessBlockConfiguration: &aws_s3_v2_types.PublicAccessBlockConfiguration{
				BlockPublicAcls:       false,
				BlockPublicPolicy:     false,
				IgnorePublicAcls:      true,
				RestrictPublicBuckets: false,
			},
		})
		if err != nil {
			return err
		}
		logutil.S().Infow("successfully set public access block", "bucket", bucketName)

		policy := `{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "PublicReadGetObject",
            "Effect": "Allow",
            "Principal": "*",
            "Action": "s3:GetObject",
            "Resource": "arn:aws:s3:::` + bucketName + `/*"
        }
    ]
}`
		logutil.S().Infow("setting public bucket read policy", "bucket", bucketName)
		_, err = cli.PutBucketPolicy(ctx, &aws_s3_v2.PutBucketPolicyInput{
			Bucket: &bucketName,
			Policy: &policy,
		})
		if err != nil {
			return err
		}
		logutil.S().Infow("successfully set public bucket read policy", "bucket", bucketName)

		// PutBucketAcl with aws_s3_v2_types.BucketCannedACLPublicRead will fail here:
		// "AccessControlListNotSupported: The bucket does not allow ACLs"
	}

	if !ret.publicRead {
		logutil.S().Infow("setting private permission", "bucket", bucketName)
		_, err = cli.PutPublicAccessBlock(ctx, &aws_s3_v2.PutPublicAccessBlockInput{
			Bucket: &bucketName,
			PublicAccessBlockConfiguration: &aws_s3_v2_types.PublicAccessBlockConfiguration{
				BlockPublicAcls:       true,
				BlockPublicPolicy:     true,
				IgnorePublicAcls:      true,
				RestrictPublicBuckets: true,
			},
		})
		if err != nil {
			return err
		}
		logutil.S().Infow("successfully set private permission", "bucket", bucketName)
	}

	if ret.serverSideEncryption {
		logutil.S().Infow("setting server-side encryption", "bucket", bucketName)
		_, err = cli.PutBucketEncryption(ctx, &aws_s3_v2.PutBucketEncryptionInput{
			Bucket: &bucketName,
			ServerSideEncryptionConfiguration: &aws_s3_v2_types.ServerSideEncryptionConfiguration{
				Rules: []aws_s3_v2_types.ServerSideEncryptionRule{
					{
						ApplyServerSideEncryptionByDefault: &aws_s3_v2_types.ServerSideEncryptionByDefault{
							SSEAlgorithm: aws_s3_v2_types.ServerSideEncryptionAes256,
						},
					},
				},
			},
		})
		if err != nil {
			return err
		}
		logutil.S().Infow("successfully applied server-side encryption", "bucket", bucketName)
	}

	if ret.lifecycle != nil && len(ret.lifecycle) > 0 {
		logutil.S().Infow("setting lifecycle", "bucket", bucketName, "lifecycle", ret.lifecycle)
		if err := PutBucketObjectExpireConfiguration(ctx, cfg, bucketName, ret.lifecycle); err != nil {
			return err
		}
		logutil.S().Infow("successfully set lifecycle", "bucket", bucketName, "lifecycle", ret.lifecycle)
	}

	return nil
}

// DeleteBucket deletes a bucket.
func DeleteBucket(ctx context.Context, cfg aws.Config, bucketName string) error {
	logutil.S().Infow("deleting bucket", "bucket", bucketName)
	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err := cli.DeleteBucket(ctx, &aws_s3_v2.DeleteBucketInput{
		Bucket: &bucketName,
	})
	if err != nil {
		if strings.Contains(err.Error(), "NoSuchBucket") {
			logutil.S().Warnw("bucket does not exist", "bucket", bucketName, "error", err)
			return nil
		}
		if strings.Contains(err.Error(), "bucket does not exist") {
			logutil.S().Warnw("bucket does not exist", "bucket", bucketName, "error", err)
			return nil
		}
		return err
	}

	logutil.S().Infow("successfully deleted bucket", "bucket", bucketName)
	return nil
}

// DeleteObjects deletes objects in a bucket by the prefix.
// If empty, deletes all.
func DeleteObjects(ctx context.Context, cfg aws.Config, bucketName string, pfx string) error {
	logutil.S().Infow("deleting objects in bucket", "bucket", bucketName, "prefix", pfx)
	objects, err := ListObjects(ctx, cfg, bucketName, pfx)
	if err != nil {
		return err
	}
	if len(objects) == 0 {
		logutil.S().Infow("no objects to delete", "bucket", bucketName, "prefix", pfx)
		return nil
	}

	objIDs := make([]aws_s3_v2_types.ObjectIdentifier, 0, len(objects))
	for _, obj := range objects {
		objIDs = append(objIDs, aws_s3_v2_types.ObjectIdentifier{
			Key: obj.Key,
		})
	}

	logutil.S().Infow("deleting", "objects", len(objIDs))
	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err = cli.DeleteObjects(ctx, &aws_s3_v2.DeleteObjectsInput{
		Bucket: &bucketName,
		Delete: &aws_s3_v2_types.Delete{
			Objects: objIDs,
		},
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully deleted objects in bucket", "bucket", bucketName, "objects", len(objIDs))
	return nil
}

// ListObjects deletes objects in a bucket by the prefix.
// If empty, deletes all.
func ListObjects(ctx context.Context, cfg aws.Config, bucketName string, pfx string) ([]aws_s3_v2_types.Object, error) {
	logutil.S().Infow("listing objects in bucket", "bucket", bucketName, "prefix", pfx)
	cli := aws_s3_v2.NewFromConfig(cfg)

	objects := make([]aws_s3_v2_types.Object, 0)
	token := ""
	for {
		input := &aws_s3_v2.ListObjectsInput{
			Bucket: &bucketName,
		}
		if pfx != "" {
			input.Prefix = &pfx
		}
		if token != "" {
			input.Marker = &token
		}

		out, err := cli.ListObjects(ctx, input)
		if err != nil {
			return nil, err
		}
		logutil.S().Infow("listed objects", "maxKeys", out.MaxKeys, "contents", len(out.Contents))

		if out.MaxKeys == 0 {
			break
		}
		if len(out.Contents) == 0 {
			break
		}

		objects = append(objects, out.Contents...)

		if out.NextMarker != nil && *out.NextMarker != "" {
			token = *out.NextMarker
		}
		if token == "" {
			break
		}
	}

	if len(objects) > 1 {
		sort.SliceStable(objects, func(i, j int) bool {
			return objects[i].LastModified.Nanosecond() < objects[j].LastModified.Nanosecond()
		})
	}

	logutil.S().Infow("successfully listed bucket", "bucket", bucketName, "objects", len(objects))
	return objects, nil
}

// Applies bucket expire policy to a bucket.
func PutBucketObjectExpireConfiguration(ctx context.Context, cfg aws.Config, bucketName string, pfxToExpirationDays map[string]int32) error {
	logutil.S().Infow("putting bucket object expire configuration", "bucket", bucketName, "pfxToExpirationDays", pfxToExpirationDays)

	rules := make([]aws_s3_v2_types.LifecycleRule, 0, len(pfxToExpirationDays))
	for pfx, days := range pfxToExpirationDays {
		logutil.S().Infow("adding rule", "days", days, "prefix", pfx)
		rules = append(rules,
			aws_s3_v2_types.LifecycleRule{
				Status: aws_s3_v2_types.ExpirationStatusEnabled,
				Filter: &aws_s3_v2_types.LifecycleRuleFilterMemberPrefix{
					Value: pfx,
				},
				Expiration: &aws_s3_v2_types.LifecycleExpiration{
					Days: days,
				},
				AbortIncompleteMultipartUpload: &aws_s3_v2_types.AbortIncompleteMultipartUpload{
					DaysAfterInitiation: days,
				},
			},
		)
	}

	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err := cli.PutBucketLifecycleConfiguration(ctx, &aws_s3_v2.PutBucketLifecycleConfigurationInput{
		Bucket: &bucketName,
		LifecycleConfiguration: &aws_s3_v2_types.BucketLifecycleConfiguration{
			Rules: rules,
		},
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully put bucket object expire configuration", "bucket", bucketName)
	return nil
}

// PutObject uploads a file to a bucket.
func PutObject(ctx context.Context, cfg aws.Config, localFilePath string, bucketName string, s3Key string, opts ...OpOption) error {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("uploading file", "localFilePath", localFilePath, "bucket", bucketName, "s3Key", s3Key, "publicRead", ret.publicRead)

	f, err := os.OpenFile(localFilePath, os.O_RDONLY, 0444)
	if err != nil {
		return err
	}
	defer f.Close()

	input := &aws_s3_v2.PutObjectInput{
		Bucket:   &bucketName,
		Key:      &s3Key,
		Body:     f,
		Metadata: ret.metadata,
	}
	if ret.publicRead {
		input.ACL = aws_s3_v2_types.ObjectCannedACLPublicRead
	}

	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err = cli.PutObject(ctx, input)
	if err != nil {
		return err
	}

	if ret.publicRead {
		_, err = cli.PutObjectAcl(ctx, &aws_s3_v2.PutObjectAclInput{
			Bucket: &bucketName,
			Key:    &s3Key,
			ACL:    aws_s3_v2_types.ObjectCannedACLPublicRead,
		})
		if err != nil {
			return err
		}
		logutil.S().Infow("successfully applied public read permission", "bucket", bucketName, "s3Key", s3Key)
	}

	logutil.S().Infow("successfully uploaded file", "localFilePath", localFilePath, "bucket", bucketName, "s3Key", s3Key)
	return nil
}

// ObjectExists checks if an object exists.
func ObjectExists(ctx context.Context, cfg aws.Config, bucketName string, s3Key string) (*aws_s3_v2.HeadObjectOutput, error) {
	logutil.S().Infow("checking if s3 key exists", "bucket", bucketName, "s3Key", s3Key)

	cli := aws_s3_v2.NewFromConfig(cfg)
	out, err := cli.HeadObject(ctx, &aws_s3_v2.HeadObjectInput{
		Bucket: &bucketName,
		Key:    &s3Key,
	})
	if err != nil {
		if strings.Contains(err.Error(), "NotFound") {
			return nil, nil
		}
		return nil, err
	}

	logutil.S().Infow("successfully confirmed that s3 key exists", "bucket", bucketName, "s3Key", s3Key)
	return out, nil
}

// GetObject downloads a file from a bucket.
func GetObject(ctx context.Context, cfg aws.Config, bucketName string, s3Key string, localFilePath string) error {
	headOut, err := ObjectExists(ctx, cfg, bucketName, s3Key)
	if err != nil {
		return err
	}
	size := headOut.ContentLength
	logutil.S().Infow("downloading file",
		"bucket", bucketName,
		"s3Key", s3Key,
		"localFilePath", localFilePath,
		"size", humanize.Bytes(uint64(size)),
	)

	cli := aws_s3_v2.NewFromConfig(cfg)
	out, err := cli.GetObject(ctx, &aws_s3_v2.GetObjectInput{
		Bucket: &bucketName,
		Key:    &s3Key,
	})
	if err != nil {
		return err
	}
	defer out.Body.Close()

	f, err := os.OpenFile(localFilePath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0644)
	if err != nil {
		return err
	}
	defer f.Close()

	if _, err = io.Copy(f, out.Body); err != nil {
		return err
	}

	logutil.S().Infow("successfully downloaded file", "bucket", bucketName, "s3Key", s3Key, "localFilePath", localFilePath)
	return nil
}

type Op struct {
	publicRead           bool
	serverSideEncryption bool

	// map prefix to expiration days
	lifecycle map[string]int32

	metadata map[string]string
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithPublicRead(b bool) OpOption {
	return func(op *Op) {
		op.publicRead = b
	}
}

func WithServerSideEncryption(b bool) OpOption {
	return func(op *Op) {
		op.serverSideEncryption = b
	}
}

func WithLifecycle(m map[string]int32) OpOption {
	return func(op *Op) {
		op.lifecycle = m
	}
}

func WithMetadata(m map[string]string) OpOption {
	return func(op *Op) {
		op.metadata = m
	}
}
