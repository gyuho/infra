package s3

import (
	"bytes"
	"context"
	"fmt"
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

	// Path-style bucket URL.
	// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#path-style-access
	URL string
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
			v.URL,
		}
		rows = append(rows, row)
	}

	buf := bytes.NewBuffer(nil)
	tb := tablewriter.NewWriter(buf)
	tb.SetAutoWrapText(false)
	tb.SetAlignment(tablewriter.ALIGN_LEFT)
	tb.SetCenterSeparator("*")
	tb.SetHeader([]string{"name", "created", "url"})
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
		buckets = append(buckets, Bucket{
			Name:    *b.Name,
			Created: *b.CreationDate,

			// note that this is different for Cloudflare R2 buckets
			// ref. https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#path-style-access
			URL: fmt.Sprintf("https://s3.%s.amazonaws.com/%s", cfg.Region, *b.Name),
		})
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

const (
	bucketAlreadyExists     = "BucketAlreadyExists"
	bucketAlreadyOwnedByYou = "BucketAlreadyOwnedByYou"
)

// CreateBucket creates a bucket.
// Shared with Cloudflare R2 API https://developers.cloudflare.com/r2/api/s3/api/.
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
	if ret.bucketACL != nil {
		acl = *ret.bucketACL
	}

	// default is "Bucket owner enforced"
	ownership := aws_s3_v2_types.ObjectOwnershipBucketOwnerEnforced
	if ret.objectOwnership != nil {
		ownership = *ret.objectOwnership
	}

	logutil.S().Infow("creating bucket", "bucket", bucketName, "acl", acl, "ownership", ownership)
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
		if strings.Contains(err.Error(), bucketAlreadyExists) {
			logutil.S().Warnw("bucket already exists -- proceed to update bucket policy", "bucket", bucketName, "error", err)
			err = nil
		}
		if err != nil && strings.Contains(err.Error(), bucketAlreadyOwnedByYou) {
			logutil.S().Warnw("bucket already exists -- proceed to update bucket policy", "bucket", bucketName, "error", err)
			err = nil
		}
		if err != nil {
			return err
		}
	}
	logutil.S().Infow("successfully created bucket", "bucket", bucketName)

	if !ret.skipBucketPolicy {
		// block everything by default
		publicAccessBlock := aws_s3_v2.PutPublicAccessBlockInput{
			Bucket: &bucketName,
			PublicAccessBlockConfiguration: &aws_s3_v2_types.PublicAccessBlockConfiguration{
				BlockPublicAcls:       aws.Bool(true),
				BlockPublicPolicy:     aws.Bool(true),
				IgnorePublicAcls:      aws.Bool(true),
				RestrictPublicBuckets: aws.Bool(true),
			},
		}

		if ret.bucketBlockPublicACLs != nil {
			publicAccessBlock.PublicAccessBlockConfiguration.BlockPublicAcls = ret.bucketBlockPublicACLs
		}
		if ret.bucketBlockPublicPolicy != nil {
			publicAccessBlock.PublicAccessBlockConfiguration.BlockPublicPolicy = ret.bucketBlockPublicPolicy
		}
		if ret.bucketIgnorePublicACLs != nil {
			publicAccessBlock.PublicAccessBlockConfiguration.IgnorePublicAcls = ret.bucketIgnorePublicACLs
		}
		if ret.bucketRestrictPublicBuckets != nil {
			publicAccessBlock.PublicAccessBlockConfiguration.RestrictPublicBuckets = ret.bucketRestrictPublicBuckets
		}

		logutil.S().Infow("applying public access block", "bucket", bucketName)
		_, err = cli.PutPublicAccessBlock(ctx, &publicAccessBlock)
		if err != nil {
			return err
		}

		if ret.bucketPolicy != "" {
			logutil.S().Infow("applying bucket policy", "bucket", bucketName)
			_, err = cli.PutBucketPolicy(ctx, &aws_s3_v2.PutBucketPolicyInput{
				Bucket: &bucketName,
				Policy: &ret.bucketPolicy,
			})
			if err != nil {
				return err
			}
		}
	}

	if ret.serverSideEncryption {
		logutil.S().Infow("applying server-side encryption", "bucket", bucketName)
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
	}

	if ret.lifecycle != nil && len(ret.lifecycle) > 0 {
		logutil.S().Infow("applying object lifecycle configuration", "bucket", bucketName, "lifecycle", ret.lifecycle)
		if err := PutBucketObjectExpireConfiguration(ctx, cfg, bucketName, ret.lifecycle); err != nil {
			return err
		}
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
	objects, err := ListObjects(ctx, cfg, bucketName, WithPrefix(pfx))
	if err != nil {
		return err
	}
	if len(objects.Objects) == 0 {
		logutil.S().Infow("no objects to delete", "bucket", bucketName, "prefix", pfx)
		return nil
	}

	objIDs := make([]aws_s3_v2_types.ObjectIdentifier, 0, len(objects.Objects))
	for _, obj := range objects.Objects {
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

// DeleteObject deletes an object.
func DeleteObject(ctx context.Context, cfg aws.Config, bucketName string, s3Key string) error {
	logutil.S().Infow("deleting object", "bucket", bucketName, "s3Key", s3Key)
	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err := cli.DeleteObject(ctx, &aws_s3_v2.DeleteObjectInput{
		Bucket: &bucketName,
		Key:    &s3Key,
	})
	if err != nil {
		return err
	}

	logutil.S().Infow("successfully deleted object", "bucket", bucketName, "s3Key", s3Key)
	return nil
}

type Objects struct {
	Objects []aws_s3_v2_types.Object

	// NextContinuationToken is sent when isTruncated is true, which means there are
	// more keys in the bucket that can be listed. The next list requests to Amazon S3
	// can be continued with this NextContinuationToken. NextContinuationToken is
	// obfuscated and is not a real key.
	// ref. https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html
	NextContinuationToken string
}

// ListObjects deletes objects in a bucket by the prefix.
// If empty, deletes all.
func ListObjects(ctx context.Context, cfg aws.Config, bucketName string, opts ...OpOption) (Objects, error) {
	options := &Op{}
	options.applyOpts(opts)

	logutil.S().Infow("listing objects in bucket", "bucket", bucketName, "maxKeys", options.limit, "prefix", options.prefix)
	cli := aws_s3_v2.NewFromConfig(cfg)

	objects := make([]aws_s3_v2_types.Object, 0)
	token := ""
	for {
		input := &aws_s3_v2.ListObjectsV2Input{
			Bucket: &bucketName,
		}
		if options.prefix != "" {
			input.Prefix = &options.prefix
		}
		if token != "" {
			input.ContinuationToken = &token
		}

		out, err := cli.ListObjectsV2(ctx, input)
		if err != nil {
			return Objects{}, err
		}
		logutil.S().Infow("listed objects", "maxKeys", out.MaxKeys, "truncated", out.IsTruncated, "contents", len(out.Contents))

		if out.IsTruncated != nil && *out.IsTruncated && out.NextContinuationToken != nil && *out.NextContinuationToken != "" {
			token = *out.NextContinuationToken
			logutil.S().Infow("list has more objects, received non-empty continuation token")
		} else {
			token = ""
		}

		if len(out.Contents) == 0 {
			break
		}

		objects = append(objects, out.Contents...)
		if options.limit > 0 && len(objects) >= options.limit {
			logutil.S().Infow("received enough objects -- truncating", "limit", options.limit, "totalObjects", len(objects))
			objects = objects[:options.limit]
			break
		}

		if token == "" {
			logutil.S().Infow("no next page")
			break
		}
	}

	if len(objects) > 1 {
		sort.SliceStable(objects, func(i, j int) bool {
			return (*objects[i].Key) < (*objects[j].Key)
		})
	}

	logutil.S().Infow("successfully listed bucket", "bucket", bucketName, "objects", len(objects))
	return Objects{
		Objects:               objects,
		NextContinuationToken: token,
	}, nil
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
					Days: &days,
				},
				AbortIncompleteMultipartUpload: &aws_s3_v2_types.AbortIncompleteMultipartUpload{
					DaysAfterInitiation: &days,
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

	logutil.S().Infow("uploading file", "localFilePath", localFilePath, "bucket", bucketName, "s3Key", s3Key)

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
	if ret.objectACL != nil {
		logutil.S().Infow("putting object with acl", "bucket", bucketName, "s3Key", s3Key, "acl", *ret.objectACL)
		input.ACL = *ret.objectACL
	}
	cli := aws_s3_v2.NewFromConfig(cfg)
	_, err = cli.PutObject(ctx, input)
	if err != nil {
		return err
	}

	if ret.objectACL != nil {
		logutil.S().Infow("applying put object acl", "bucket", bucketName, "s3Key", s3Key, "acl", *ret.objectACL)
		_, err = cli.PutObjectAcl(ctx, &aws_s3_v2.PutObjectAclInput{
			Bucket: &bucketName,
			Key:    &s3Key,
			ACL:    *ret.objectACL,
		})
		if err != nil {
			return err
		}
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
	if headOut == nil {
		return fmt.Errorf("object does not exist: %s/%s", bucketName, s3Key)
	}
	size := int64(0)
	if headOut != nil && headOut.ContentLength != nil && *headOut.ContentLength > 0 {
		size = *headOut.ContentLength
	}
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
	limit                 int
	prefix                string
	nextContinuationToken string

	bucketRegion string

	bucketACL       *aws_s3_v2_types.BucketCannedACL
	objectACL       *aws_s3_v2_types.ObjectCannedACL
	objectOwnership *aws_s3_v2_types.ObjectOwnership

	// does not work for Cloudflare R2
	bucketPolicy                string
	bucketBlockPublicACLs       *bool
	bucketBlockPublicPolicy     *bool
	bucketIgnorePublicACLs      *bool
	bucketRestrictPublicBuckets *bool

	serverSideEncryption bool

	// map prefix to expiration days
	// works for Cloudflare R2
	lifecycle map[string]int32

	metadata map[string]string

	preSignDuration time.Duration

	skipBucketPolicy bool
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithLimit(v int) OpOption {
	return func(op *Op) {
		op.limit = v
	}
}

func WithPrefix(v string) OpOption {
	return func(op *Op) {
		op.prefix = v
	}
}

func WithNextContinuationToken(v string) OpOption {
	return func(op *Op) {
		op.nextContinuationToken = v
	}
}

func WithBucketRegion(r string) OpOption {
	return func(op *Op) {
		op.bucketRegion = r
	}
}

func WithBucketACL(v aws_s3_v2_types.BucketCannedACL) OpOption {
	return func(op *Op) {
		op.bucketACL = &v
	}
}

func WithObjectACL(v aws_s3_v2_types.ObjectCannedACL) OpOption {
	return func(op *Op) {
		op.objectACL = &v
	}
}

func WithObjectOwnership(v aws_s3_v2_types.ObjectOwnership) OpOption {
	return func(op *Op) {
		op.objectOwnership = &v
	}
}

func WithBucketPolicy(policy string) OpOption {
	return func(op *Op) {
		op.bucketPolicy = policy
	}
}

func WithBucketBlockPublicACLs(b bool) OpOption {
	return func(op *Op) {
		op.bucketBlockPublicACLs = &b
	}
}

func WithBucketBlockPublicPolicy(b bool) OpOption {
	return func(op *Op) {
		op.bucketBlockPublicPolicy = &b
	}
}

func WithBucketIgnorePublicACLs(b bool) OpOption {
	return func(op *Op) {
		op.bucketIgnorePublicACLs = &b
	}
}

func WithBucketRestrictPublicBuckets(b bool) OpOption {
	return func(op *Op) {
		op.bucketRestrictPublicBuckets = &b
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

func WithPreSignDuration(d time.Duration) OpOption {
	return func(op *Op) {
		op.preSignDuration = d
	}
}

func WithSkipBucketPolicy(b bool) OpOption {
	return func(op *Op) {
		op.skipBucketPolicy = b
	}
}
