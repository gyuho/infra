module github.com/gyuho/infra/linux/go

go 1.21.3

require (
	github.com/gyuho/infra/aws/go v0.0.0-00010101000000-000000000000
	k8s.io/utils v0.0.0-20230726121419-3b25d923346b
)

require (
	go.uber.org/multierr v1.10.0 // indirect
	go.uber.org/zap v1.26.0 // indirect
)

replace github.com/gyuho/infra/aws/go => ../../aws/go
