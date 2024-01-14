#!/usr/bin/env bash
set -xue

if ! [[ "$0" =~ updatedep.sh ]]; then
    echo "must be run from root"
    exit 255
fi

# go get -u -v ./...

go get -u k8s.io/client-go

go mod tidy -v
