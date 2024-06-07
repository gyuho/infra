package httputil

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"github.com/gyuho/infra/go/randutil"
)

func DownloadFileToTmp(url string, opts ...OpOption) (string, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	client := http.Client{
		Timeout: ret.timeout,
	}

	resp, err := client.Get(url)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("failed to download file: %s", resp.Status)
	}

	file := filepath.Join(os.TempDir(), randutil.StringAlphabetsLowerCase(10))
	f, err := os.Create(file)
	if err != nil {
		return "", err
	}
	defer f.Close()

	_, err = io.Copy(f, resp.Body)
	if err != nil {
		return "", err
	}
	return file, nil
}

func ReadAll(url string) ([]byte, error) {
	resp, err := http.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("failed to GET: %s", resp.Status)
	}

	return io.ReadAll(resp.Body)
}

type Op struct {
	timeout time.Duration
}

type OpOption func(*Op)

func (op *Op) applyOpts(opts []OpOption) {
	for _, opt := range opts {
		opt(op)
	}
}

func WithTimeout(dur time.Duration) OpOption {
	return func(op *Op) {
		op.timeout = dur
	}
}
