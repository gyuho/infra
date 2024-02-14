package pods

import (
	core_v1 "k8s.io/api/core/v1"
)

type Op struct {
	namespace     string
	labelSelector string
	phase         core_v1.PodPhase
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithNamespace(s string) OpOption {
	return func(op *Op) {
		op.namespace = s
	}
}

// WithLabelSelector sets the labels matching requirements.
// Use https://pkg.go.dev/k8s.io/apimachinery/pkg/labels#NewSelector
// and https://pkg.go.dev/k8s.io/apimachinery/pkg/labels#NewRequirement.
func WithLabelSelector(s string) OpOption {
	return func(op *Op) {
		op.labelSelector = s
	}
}

func WithPhase(cond core_v1.PodPhase) OpOption {
	return func(op *Op) {
		op.phase = cond
	}
}
