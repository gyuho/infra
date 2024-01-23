package kms

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"

	aws_kms_v2_types "github.com/aws/aws-sdk-go-v2/service/kms/types"
	"github.com/gyuho/infra/go/randutil"
)

func TestKMS(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	keyName := randutil.AlphabetsLowerCase(10)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	out, err := Create(
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

	time.Sleep(10 * time.Second)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = Delete(ctx, cfg, *out.KeyId, 7)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	as, err := ListAliases(ctx, cfg)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	for _, a := range as {
		fmt.Println(*a.AliasName)
	}
}

func TestKMSCrypto(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	keyName := randutil.AlphabetsLowerCase(10)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	out, err := Create(
		ctx,
		cfg,
		keyName,
		aws_kms_v2_types.KeySpecEccSecgP256k1,
		aws_kms_v2_types.KeyUsageTypeSignVerify,
		map[string]string{
			"a": "b",
		},
	)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	time.Sleep(10 * time.Second)

	ctx, cancel = context.WithTimeout(context.Background(), 10*time.Second)
	err = Delete(ctx, cfg, *out.KeyId, 7)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
