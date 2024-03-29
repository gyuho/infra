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

// Package runtime implements utility functions for runtime systems.
package runtime

import (
	"os"
	"syscall"

	"github.com/prometheus/procfs"
)

func FDLimit() (uint64, error) {
	var rlimit syscall.Rlimit
	if err := syscall.Getrlimit(syscall.RLIMIT_NOFILE, &rlimit); err != nil {
		return 0, err
	}
	return rlimit.Cur, nil
}

// "process_open_fds" in prometheus collector
// ref. https://github.com/prometheus/client_golang/blob/main/prometheus/process_collector_other.go
// ref. https://pkg.go.dev/github.com/prometheus/procfs
func FDUsage() (uint64, error) {
	procs, err := procfs.AllProcs()
	if err != nil {
		return 0, err
	}
	total := uint64(0)
	for _, proc := range procs {
		l, err := proc.FileDescriptorsLen()
		if err != nil {
			return 0, err
		}
		total += uint64(l)
	}
	return total, nil
}

func FDUsageSelf() (uint64, error) {
	return countFiles("/proc/self/fd")
}

// countFiles reads the directory named by dirname and returns the count.
func countFiles(dirname string) (uint64, error) {
	f, err := os.Open(dirname)
	if err != nil {
		return 0, err
	}
	list, err := f.Readdirnames(-1)
	f.Close()
	if err != nil {
		return 0, err
	}
	return uint64(len(list)), nil
}
