package envelope

import (
	"bytes"
	"context"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/aws/go/kms"

	aws_kms_v2_types "github.com/aws/aws-sdk-go-v2/service/kms/types"
	"github.com/gyuho/infra/go/randutil"
)

func TestEnvelope(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	keyName := randutil.StringAlphabetsLowerCase(10)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	out, err := kms.Create(
		ctx,
		cfg,
		keyName,
		aws_kms_v2_types.KeySpecSymmetricDefault,
		aws_kms_v2_types.KeyUsageTypeEncryptDecrypt,
		map[string]string{
			"a": "b",
		},
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	plaintext := randutil.BytesAlphabetsLowerCaseNumeric(3 * 1024 * 1024)
	aadTag := randutil.BytesAlphabetsLowerCaseNumeric(32)

	ctx, cancel = context.WithTimeout(context.Background(), 15*time.Second)
	encrypted, err := SealAES256(ctx, cfg, *out.KeyId, plaintext, aadTag)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), 15*time.Second)
	decrypted, err := UnsealAES256(ctx, cfg, *out.KeyId, encrypted, aadTag)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(plaintext, decrypted) {
		t.Fatalf("plaintext and decrypted are not equal: %x != %x", plaintext, decrypted)
	}

	time.Sleep(3 * time.Second)
	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = kms.Delete(ctx, cfg, *out.KeyId, 7)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	time.Sleep(2 * time.Second)
	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = kms.Delete(ctx, cfg, *out.Arn, 7)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
