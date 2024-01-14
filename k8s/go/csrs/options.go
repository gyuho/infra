package csrs

import "time"

type Op struct {
	usernames    map[string]struct{}
	listPendings bool

	approveInterval time.Duration
	approveLimit    int

	deleteInterval time.Duration
	deleteLimit    int
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithUsernames(names map[string]struct{}) OpOption {
	return func(op *Op) {
		op.usernames = names
	}
}

func WithListPendings(b bool) OpOption {
	return func(op *Op) {
		op.listPendings = b
	}
}

func WithApproveInterval(d time.Duration) OpOption {
	return func(op *Op) {
		op.approveInterval = d
	}
}

func WithApproveLimit(n int) OpOption {
	return func(op *Op) {
		op.approveLimit = n
	}
}

func WithDeleteInterval(d time.Duration) OpOption {
	return func(op *Op) {
		op.deleteInterval = d
	}
}

func WithDeleteLimit(n int) OpOption {
	return func(op *Op) {
		op.deleteLimit = n
	}
}
