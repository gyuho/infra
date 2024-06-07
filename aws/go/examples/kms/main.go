package main

import (
	"context"
	"fmt"
	"time"

	aws_kms_v2_types "github.com/aws/aws-sdk-go-v2/service/kms/types"
	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/aws/go/kms"
	"github.com/gyuho/infra/aws/go/kms/eth"
	"github.com/gyuho/infra/go/randutil"
)

// ref. https://github.com/welthee/go-ethereum-aws-kms-tx-signer
func main() {
	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		panic(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	keyName := randutil.StringAlphabetsLowerCase(10)
	out, err := kms.Create(
		ctx,
		cfg,
		keyName,
		aws_kms_v2_types.KeySpecEccSecgP256k1,
		aws_kms_v2_types.KeyUsageTypeSignVerify,
		map[string]string{
			"a": "b",
		},
	)
	if err != nil {
		panic(err)
	}
	defer func() {
		cctx, ccancel := context.WithTimeout(context.Background(), 10*time.Second)
		err = kms.Delete(cctx, cfg, *out.KeyId, 7)
		ccancel()
		if err != nil {
			panic(err)
		}
	}()

	pubout, err := kms.GetPublicKey(ctx, cfg, *out.KeyId)
	if err != nil {
		panic(err)
	}

	ethAddr, err := eth.DeriveAddress(pubout.PublicKey)
	if err != nil {
		panic(err)
	}
	fmt.Println("ethAddress:", ethAddr)
}
