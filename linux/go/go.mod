module github.com/gyuho/infra/linux/go

go 1.23

replace github.com/gyuho/infra/go => ../../go

require (
	github.com/gyuho/infra/go v0.0.0-00010101000000-000000000000
	k8s.io/utils v0.0.0-20241104163129-6fe5fd82f078
)

require (
	go.uber.org/multierr v1.11.0 // indirect
	go.uber.org/zap v1.27.0 // indirect
)
