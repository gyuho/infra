package httputil

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"

	"github.com/gyuho/infra/go/randutil"
)

func DownloadFileToTmp(url string) (string, error) {
	resp, err := http.Get(url)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("failed to download file: %s", resp.Status)
	}

	file := filepath.Join(os.TempDir(), randutil.AlphabetsLowerCase(10))
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
