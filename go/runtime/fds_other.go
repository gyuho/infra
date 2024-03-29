// Copyright 2015 The etcd Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//go:build !linux

package runtime

import (
	"fmt"
	"runtime"
)

func FDLimit() (uint64, error) {
	return 0, fmt.Errorf("cannot get FDLimit on %s", runtime.GOOS)
}

// "process_open_fds" in prometheus collector
// ref. https://github.com/prometheus/client_golang/blob/main/prometheus/process_collector_other.go
// ref. https://pkg.go.dev/github.com/prometheus/procfs
func FDUsage() (uint64, error) {
	return 0, fmt.Errorf("cannot get FDUsage on %s", runtime.GOOS)
}

func FDUsageSelf() (uint64, error) {
	return 0, fmt.Errorf("cannot get FDUsageSelf on %s", runtime.GOOS)
}
