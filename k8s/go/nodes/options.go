package nodes

import "time"

type Op struct {
	labelSelector string

	timeout            time.Duration
	gracePeriod        time.Duration
	force              bool
	ignoreDaemonSets   bool
	deleteEmptyDirData bool
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
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

func WithTimeout(d time.Duration) OpOption {
	return func(op *Op) {
		op.timeout = d
	}
}

func WithGracePeriod(d time.Duration) OpOption {
	return func(op *Op) {
		op.gracePeriod = d
	}
}

func WithForce(b bool) OpOption {
	return func(op *Op) {
		op.force = b
	}
}

func WithIgnoreDaemonSets(b bool) OpOption {
	return func(op *Op) {
		op.ignoreDaemonSets = b
	}
}

func WithDeleteEmptyDirData(b bool) OpOption {
	return func(op *Op) {
		op.deleteEmptyDirData = b
	}
}
