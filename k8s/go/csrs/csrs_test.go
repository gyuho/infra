package csrs

import (
	"context"
	"os"
	"testing"
	"time"

	k8s "github.com/gyuho/k8s/go"
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
		WithListPendings(true),
	)
	if err != nil {
		t.Fatal(err)
	}

	if len(csrs) == 0 {
		t.Skip("no pending CSR")
	}

	names := make([]string, 0, len(csrs))
	for _, csr := range csrs {
		names = append(names, csr.Name)
	}

	if err := Approve(
		ctx,
		cli,
		names,
		WithApproveInterval(10*time.Second),
		WithApproveLimit(1),
	); err != nil {
		t.Fatal(err)
	}
}
