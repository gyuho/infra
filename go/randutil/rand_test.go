package randutil

import (
	"testing"
	"time"
)

func TestRand(t *testing.T) {
	now := time.Now()
	t.Logf("seeding with %v", now)

	SetSeed(now.UnixNano())

	prev := ""
	for i := 0; i < 10; i++ {
		v := string(BytesAlphabetsLowerCase(5))
		t.Log(v)

		if prev == "" {
			prev = v
			continue
		}
		if prev == v {
			t.Fatal("not random")
		}
	}
}
