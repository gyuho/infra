package httputil

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
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

	f, err := os.CreateTemp(os.TempDir(), "download")
	if err != nil {
		return "", err
	}
	defer f.Close()

	if _, err = io.Copy(f, resp.Body); err != nil {
		return "", err
	}
	return f.Name(), nil
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
