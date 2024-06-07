package randutil

import (
	"math/rand"
	"sync"
	"time"
)

const (
	alphabetsLowerCase                    = "abcdefghijklmnopqrstuvwxyz"
	alphabetsLowerCaseNumeric             = "0123456789" + alphabetsLowerCase
	alphabetsNumericWithSpecialCharacters = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ?!@#$%^&*()~-=_+[]{}';:,.<>|\\/"
)

func BytesAlphabetsLowerCase(n int) []byte {
	return randBytes(alphabetsLowerCase, n)
}

func StringAlphabetsLowerCase(n int) string {
	return string(BytesAlphabetsLowerCase(n))
}

func BytesAlphabetsLowerCaseNumeric(n int) []byte {
	return randBytes(alphabetsLowerCaseNumeric, n)
}

func StringAlphabetsLowerCaseNumeric(n int) string {
	return string(BytesAlphabetsLowerCaseNumeric(n))
}

func BytesAlphabetsNumericWithSpecialCharacters(n int) []byte {
	return randBytes(alphabetsNumericWithSpecialCharacters, n)
}

func StringAlphabetsNumericWithSpecialCharacters(n int) string {
	return string(BytesAlphabetsNumericWithSpecialCharacters(n))
}

var (
	rnd = rand.New(rand.NewSource(time.Now().UnixNano()))
	mu  sync.Mutex
)

func SetSeed(seed int64) {
	mu.Lock()
	defer mu.Unlock()
	rnd.Seed(seed)
}

func randBytes(pattern string, n int) []byte {
	mu.Lock()
	defer mu.Unlock()

	b := make([]byte, n)
	for i := range b {
		b[i] = pattern[rnd.Intn(len(pattern))]
	}
	return b
}
