package fileutil

import "os"

func FileExists(p string) (bool, error) {
	_, err := os.Stat(p)
	if os.IsNotExist(err) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	return true, nil
}

func DirectoryExists(p string) (bool, error) {
	stat, err := os.Stat(p)
	if os.IsNotExist(err) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	if !stat.IsDir() {
		return false, nil
	}
	return true, nil
}
