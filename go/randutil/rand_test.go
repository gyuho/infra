package randutil

import (
	"fmt"
	"testing"
)

func TestRand(t *testing.T) {
	prev := ""
	for i := 0; i < 10; i++ {
		v := AlphabetsLowerCase(5)
		fmt.Println(v)

		if prev == "" {
			prev = v
			continue
		}
		if prev == v {
			t.Fatal("not random")
		}
	}
}
