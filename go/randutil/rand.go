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
	alphabetsLowerCase                 = "abcdefghijklmnopqrstuvwxyz"
	alphaLowerNumerics                 = "0123456789abcdefghijklmnopqrstuvwxyz"
	alphaNumericsWithSpecialCharacters = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ?!@#$%^&*()-=_+[]{}';:,.<>"
)

func AlphabetsLowerCase(n int) string {
	return string(randBytes(alphabetsLowerCase, n))
}

func BytesAlphaNumeric(n int) []byte {
	return randBytes(alphaLowerNumerics, n)
}

func StringAlphaNumeric(n int) string {
	return string(randBytes(alphaLowerNumerics, n))
}

func StringAlphaNumericWithSpecialCharacters(n int) string {
	return string(randBytes(alphaNumericsWithSpecialCharacters, n))
}

func randBytes(pattern string, n int) []byte {
	rd := rand.New(rand.NewSource(time.Now().UnixNano()))
	b := make([]byte, n)
	for i := range b {
		b[i] = pattern[rd.Intn(len(pattern))]
	}
	return b
}
