package pods

import (
	"os"

	k8s "github.com/gyuho/infra/k8s/go"

	core_v1 "k8s.io/api/core/v1"
	k8s_fields "k8s.io/apimachinery/pkg/fields"
	k8s_labels "k8s.io/apimachinery/pkg/labels"
	"k8s.io/client-go/kubernetes"
	runtimeclient "sigs.k8s.io/controller-runtime/pkg/client"
)

type Op struct {
	clientset     *kubernetes.Clientset
	runtimeClient runtimeclient.Client

	namespace     string
	fieldSelector k8s_fields.Selector
	labelSelector k8s_labels.Selector
	phase         core_v1.PodPhase
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

func WithNamespace(s string) OpOption {
	return func(op *Op) {
		op.namespace = s
	}
}

// WithFieldSelector sets the field selector requirements.
// Use https://pkg.go.dev/k8s.io/apimachinery/pkg/fields#Selector
func WithFieldSelector(s k8s_fields.Selector) OpOption {
	return func(op *Op) {
		op.fieldSelector = s
	}
}

// WithLabelSelector sets the labels matching requirements.
// Use https://pkg.go.dev/k8s.io/apimachinery/pkg/labels#Selector
// and https://pkg.go.dev/k8s.io/apimachinery/pkg/labels#NewRequirement.
func WithLabelSelector(s k8s_labels.Selector) OpOption {
	return func(op *Op) {
		op.labelSelector = s
	}
}

func WithPhase(cond core_v1.PodPhase) OpOption {
	return func(op *Op) {
		op.phase = cond
	}
}
