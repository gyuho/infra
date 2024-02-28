#!/usr/bin/env bash
set -xue

if ! [[ "$0" =~ updatedep.sh ]]; then
    echo "must be run from root"
    exit 255
fi

# go get -u -v ./...

go get -u golang.org/x/sync
go get -u k8s.io/api
go get -u k8s.io/apimachinery
go get -u k8s.io/client-go
go get -u k8s.io/kubectl
go get -u k8s.io/kubernetes
go get -u sigs.k8s.io/controller-runtime

go mod tidy -v
