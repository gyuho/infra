package ec2

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
	"github.com/gyuho/infra/go/crypto"
	"github.com/gyuho/infra/go/randutil"
)

func TestKeyPair(t *testing.T) {
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
	tags := map[string]string{
		"a": "b",
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	kid, err := CreateRSAKeyPair(ctx, cfg, keyName, tags)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	time.Sleep(time.Second)

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteKeyPair(ctx, cfg, kid)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteKeyPair(ctx, cfg, kid)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	_, pub, err := crypto.NewRSAKey(2048)
	if err != nil {
		t.Fatal(err)
	}

	pubKeyPath := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10)+".pub")
	defer os.RemoveAll(pubKeyPath)
	if err = os.WriteFile(pubKeyPath, []byte(pub), 0644); err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	kid2, err := ImportKeyPair(ctx, cfg, pubKeyPath, keyName, tags)
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteKeyPair(ctx, cfg, kid2)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithTimeout(context.Background(), time.Minute)
	err = DeleteKeyPair(ctx, cfg, kid2)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
}
