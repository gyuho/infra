package csrs

import "time"

type Op struct {
	usernames      map[string]struct{}
	selectPendings bool
	minCreateAge   time.Duration
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
