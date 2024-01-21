package csrs

import (
	"context"
	"os"
	"testing"
	"time"

	k8s "github.com/gyuho/infra/k8s/go"
)

// KUBECONFIG=~/.kube/config go test -v -run TestList
func TestList(t *testing.T) {
	if os.Getenv("KUBECONFIG") == "" {
		t.Skip("set KUBECONFIG to run this test")
	}

	cli, err := k8s.New(os.Getenv("KUBECONFIG"))
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()
	csrs, err := List(
		ctx,
		cli,
		WithSelectPendings(true),
	)
	if err != nil {
		t.Fatal(err)
	}

	if len(csrs) == 0 {
		t.Skip("no pending CSR")
	}

	if err := Approve(
		ctx,
		cli,
		[]string{csrs[0].Name},
	); err != nil {
		t.Fatal(err)
	}
}
