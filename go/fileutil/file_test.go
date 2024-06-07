package fileutil

import (
	"os"
	"testing"
)

func TestFileExists(t *testing.T) {
	f, err := os.CreateTemp("", "test-file-exists")
	if err != nil {
		t.Fatal(err)
	}
	defer os.Remove(f.Name())

	exists, err := FileExists(f.Name())
	if err != nil {
		t.Fatal(err)
	}
	if !exists {
		t.Error("FileExists() returned false for existing file")
	}

	nonExistentFile := "non-existent-file.txt"
	exists, err = FileExists(nonExistentFile)
	if err != nil {
		t.Fatal(err)
	}
	if exists {
		t.Error("FileExists() returned true for non-existent file")
	}
}

func TestDirectoryExists(t *testing.T) {
	t.Run("DirectoryExists", func(t *testing.T) {
		dir, err := os.MkdirTemp("", "test-dir")
		if err != nil {
			t.Fatal(err)
		}
		defer os.RemoveAll(dir)
		exists, err := DirectoryExists(dir)
		if err != nil {
			t.Fatal(err)
		}
		if !exists {
			t.Errorf("DirectoryExists() returned false for existing directory")
		}
	})
	t.Run("DirectoryDoesNotExist", func(t *testing.T) {
		exists, err := DirectoryExists("non-existent-dir")
		if err != nil {
			t.Fatal(err)
		}
		if exists {
			t.Errorf("DirectoryExists() returned true for non-existent directory")
		}
	})
	t.Run("FileIsDirectory", func(t *testing.T) {
		f, err := os.CreateTemp("", "test-file")
		if err != nil {
			t.Fatal(err)
		}
		defer os.Remove(f.Name())
		exists, err := DirectoryExists(f.Name())
		if err != nil {
			t.Fatal(err)
		}
		if exists {
			t.Errorf("DirectoryExists() returned true for a file")
		}
	})
}
