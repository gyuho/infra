package randutil

import (
	"math/rand"
	"time"
)

func Intn(n int) int {
	rd := rand.New(rand.NewSource(time.Now().UnixNano()))
	return rd.Intn(n)
}

const (
	alphabets     = "abcdefghijklmnopqrstuvwxyz"
	alphaNumerics = "0123456789abcdefghijklmnopqrstuvwxyz"
)

func String(n int) string {
	return string(Bytes(n))
}

func Bytes(n int) []byte {
	rd := rand.New(rand.NewSource(time.Now().UnixNano()))
	b := make([]byte, n)
	for i := range b {
		b[i] = alphabets[rd.Intn(len(alphabets))]
	}
	return b
}

func StringAlphaNumeric(n int) string {
	return string(BytesAlphaNumeric(n))
}

func BytesAlphaNumeric(n int) []byte {
	rd := rand.New(rand.NewSource(time.Now().UnixNano()))
	b := make([]byte, n)
	for i := range b {
		b[i] = alphaNumerics[rd.Intn(len(alphabets))]
	}
	return b
}
