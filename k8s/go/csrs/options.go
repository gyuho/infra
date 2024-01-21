package csrs

type Op struct {
	usernames      map[string]struct{}
	selectPendings bool
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
