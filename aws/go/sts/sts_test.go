package sts

import (
	"context"
	"os"
	"testing"
	"time"
)

func TestGetCallerIdentity(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	id, err := GetCallerIdentity(ctx)
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("%+v", id)
}
