package aws

import (
	"context"
	"os"
	"testing"
	"time"
)

func TestTest(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	if _, err := New(nil); err == nil {
		t.Fatal("expected error, got nil")
	}
	cfg, err := New(&Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Credentials == nil {
		t.Skip("cannot create session; nil Credentials")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	id, err := GetCallerIdentity(ctx)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("%+v", id)
}
