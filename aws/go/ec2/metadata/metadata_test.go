package metadata

import (
	"encoding/json"
	"fmt"
	"testing"
)

func TestMetadata(t *testing.T) {
	b := `{"action": "stop", "time": "2023-09-18T08:22:00Z"}`
	ia := InstanceAction{}
	if err := json.Unmarshal([]byte(b), &ia); err != nil {
		t.Fatal(err)
	}
	fmt.Println(ia)
}
