package ctxutil

import (
	"context"
	"strings"
	"testing"
	"time"
)

func TestTimeLeftTillDeadline(t *testing.T) {
	left := TimeLeftTillDeadline(context.TODO())
	if left != "∞" {
		t.Fatalf("expected ∞, got %s", left)
	}

	ctx, cancel := context.WithTimeout(context.Background(), time.Hour+time.Second+555*time.Millisecond)
	left = TimeLeftTillDeadline(ctx)
	if !strings.HasPrefix(left, "1h") {
		t.Fatalf("expected 1h..., got %s", left)
	}
	cancel()

	left = TimeLeftTillDeadline(ctx)
	if left != "ctx error (context canceled)" {
		t.Fatalf("expected ctx error, got %s", left)
	}
}
