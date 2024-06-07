package s3

import (
	"bytes"
	"context"
	"net/http"
	"os"
	"path/filepath"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/httputil"
	"github.com/gyuho/infra/go/randutil"

	aws_s3_v2_types "github.com/aws/aws-sdk-go-v2/service/s3/types"
)

func TestS3PrivatePreSigned(t *testing.T) {
	if os.Getenv("RUN_AWS_S3_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	privateBucket := randutil.StringAlphabetsLowerCase(10)

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	err = CreateBucket(
		ctx,
		cfg,
		privateBucket,
		WithObjectOwnership(aws_s3_v2_types.ObjectOwnershipBucketOwnerPreferred),
		WithBucketBlockPublicACLs(false),
	)
	cancel()

	defer func() { // clean up
		ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
		err = DeleteObjects(ctx, cfg, privateBucket, "")
		if err != nil {
			t.Errorf("DeleteObjects: %v", err)
		}

		err = DeleteBucket(ctx, cfg, privateBucket)
		cancel()
		if err != nil {
			t.Errorf("DeleteBucket: %v", err)
		}
	}()

	if err != nil {
		t.Fatal(err)
	}

	localFile, s3Key := filepath.Join(os.TempDir(), randutil.StringAlphabetsLowerCase(10)), filepath.Join(randutil.StringAlphabetsLowerCase(10), randutil.StringAlphabetsLowerCase(10))
	defer os.RemoveAll(localFile)

	localFileb := []byte(randutil.StringAlphabetsLowerCase(100))
	err = os.WriteFile(localFile, localFileb, 0644)
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 3*time.Minute)
	defer cancel()
	err = PutObject(ctx, cfg, localFile, privateBucket, s3Key, WithMetadata(map[string]string{"a": "b"}))
	if err != nil {
		t.Fatal(err)
	}

	preSignedURLForGet, err := CreatePreSignedURLForGet(ctx, cfg, privateBucket, s3Key, 0)
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(time.Second)
	tmpFileForGet, err := httputil.DownloadFileToTmp(preSignedURLForGet)
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpFileForGet)
	tmpFileb, err := os.ReadFile(tmpFileForGet)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(tmpFileb, localFileb) {
		t.Fatalf("tmpFileb != localFileb: %s != %s", string(tmpFileb), string(localFileb))
	}

	s3KeyForPut := filepath.Join(randutil.StringAlphabetsLowerCase(10), randutil.StringAlphabetsLowerCase(10))
	preSignedURLForPut, err := CreatePreSignedURLForPut(ctx, cfg, privateBucket, s3KeyForPut, 0)
	if err != nil {
		t.Fatal(err)
	}
	putReq, err := http.NewRequest(http.MethodPut, preSignedURLForPut, bytes.NewReader(localFileb))
	if err != nil {
		t.Fatal(err)
	}
	resp, err := http.DefaultClient.Do(putReq)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()

	preSignedURLForGet, err = CreatePreSignedURLForGet(ctx, cfg, privateBucket, s3KeyForPut, 0)
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(time.Second)
	tmpFileFromPut, err := httputil.DownloadFileToTmp(preSignedURLForGet)
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpFileFromPut)
	tmpFileb, err = os.ReadFile(tmpFileFromPut)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(tmpFileb, localFileb) {
		t.Fatalf("tmpFileb != localFileb: %s != %s", string(tmpFileb), string(localFileb))
	}

	preSignedURLForDelete, err := CreatePreSignedURLForDelete(ctx, cfg, privateBucket, s3KeyForPut, 0)
	if err != nil {
		t.Fatal(err)
	}
	deleteReq, err := http.NewRequest(http.MethodDelete, preSignedURLForDelete, nil)
	if err != nil {
		t.Fatal(err)
	}
	resp, err = http.DefaultClient.Do(deleteReq)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()

	time.Sleep(time.Second)

	// on delete, the download should fail
	if _, err = httputil.DownloadFileToTmp(preSignedURLForGet); err == nil {
		t.Fatalf("expected error, got nil")
	}
}
