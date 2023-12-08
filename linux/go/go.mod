module github.com/gyuho/infra/linux/go

go 1.21.5

replace github.com/gyuho/infra/go => ../../go

require (
	github.com/gyuho/infra/go v0.0.0-00010101000000-000000000000
	k8s.io/utils v0.0.0-20231127182322-b307cd553661
)

require (
	go.uber.org/multierr v1.11.0 // indirect
	go.uber.org/zap v1.26.0 // indirect
)
