package csrs

import (
	"os"
	"time"

	k8s "github.com/gyuho/infra/k8s/go"

	"k8s.io/client-go/kubernetes"
	runtimeclient "sigs.k8s.io/controller-runtime/pkg/client"
)

type Op struct {
	clientset     *kubernetes.Clientset
	runtimeClient runtimeclient.Client

	usernames      map[string]struct{}
	selectPendings bool
	minCreateAge   time.Duration
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) error {
	for _, opt := range opts {
		opt(op)
	}
	if op.clientset == nil && op.runtimeClient == nil {
		var err error
		op.clientset, err = k8s.New(os.Getenv("KUBECONFIG"))
		if err != nil {
			return err
		}
	}
	return nil
}

func WithClientset(clientset *kubernetes.Clientset) OpOption {
	return func(op *Op) {
		op.clientset = clientset
	}
}

func WithRuntimeClient(runtimeClient runtimeclient.Client) OpOption {
	return func(op *Op) {
		op.runtimeClient = runtimeClient
	}
}

func WithUsernames(names map[string]struct{}) OpOption {
	return func(op *Op) {
		op.usernames = names
	}
}

func WithSelectPendings(b bool) OpOption {
	return func(op *Op) {
		op.selectPendings = b
	}
}

// WithMinCreateAge sets the minimum creation age threshold.
// e.g., "WithMinCreateAge(1h)" only returns CSRs that have been created for at least 1 hour.
func WithMinCreateAge(d time.Duration) OpOption {
	return func(op *Op) {
		op.minCreateAge = d
	}
}
