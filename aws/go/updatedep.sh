#!/usr/bin/env bash
set -xue

if ! [[ "$0" =~ updatedep.sh ]]; then
    echo "must be run from root"
    exit 255
fi

# go get -u -v ./...

go get -u github.com/aws/aws-sdk-go-v2
go get -u github.com/aws/aws-sdk-go-v2/config
go get -u github.com/aws/aws-sdk-go-v2/credentials
go get -u github.com/aws/aws-sdk-go-v2/service/cloudformation
go get -u github.com/aws/aws-sdk-go-v2/service/ec2
go get -u github.com/aws/aws-sdk-go-v2/service/kms
go get -u github.com/ethereum/go-ethereum
go get -u github.com/aws/aws-sdk-go-v2/service/s3
go get -u github.com/aws/aws-sdk-go-v2/service/secretsmanager
go get -u github.com/aws/aws-sdk-go-v2/service/ssm
go get -u github.com/aws/aws-sdk-go-v2/service/sts
go get -u k8s.io/client-go
go get -u sigs.k8s.io/yaml

go mod tidy -v
