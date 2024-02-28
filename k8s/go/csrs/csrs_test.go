package csrs

import (
	"context"
	"os"
	"testing"
	"time"

	k8s "github.com/gyuho/infra/k8s/go"
)

// TEST_KUBERNETES_CONFIG=~/.kube/config go test -v -run TestList
func TestList(t *testing.T) {
	if os.Getenv("TEST_KUBERNETES_CONFIG") == "" {
		t.Skip("set TEST_KUBERNETES_CONFIG to run this test")
	}

	os.Setenv("KUBECONFIG", os.Getenv("TEST_KUBERNETES_CONFIG"))
	defer os.Unsetenv("KUBECONFIG")

	cli, err := k8s.New(os.Getenv("TEST_KUBERNETES_CONFIG"))
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()
	csrs, err := List(
		ctx,
		WithClientset(cli),
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
		[]string{csrs[0].Name},
		WithClientset(cli),
	); err != nil {
		t.Fatal(err)
	}
}
