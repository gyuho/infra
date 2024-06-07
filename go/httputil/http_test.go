package httputil

import (
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"
)

func TestDownloadFileToTmp(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = io.WriteString(w, "Hello, client!")
	}))
	defer ts.Close()

	filePath, err := DownloadFileToTmp(ts.URL, WithTimeout(5*time.Second))
	if err != nil {
		t.Fatalf("DownloadFileToTmp failed: %v", err)
	}
	defer os.Remove(filePath)

	data, err := os.ReadFile(filePath)
	if err != nil {
		t.Fatalf("ReadFile failed: %v", err)
	}
	if string(data) != "Hello, client!" {
		t.Errorf("Downloaded content mismatch: %s", string(data))
	}
}
