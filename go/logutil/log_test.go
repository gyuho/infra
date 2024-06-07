package logutil

import (
	"testing"

	"go.uber.org/zap"
)

func TestSetZapLogger(t *testing.T) {
	cfg := zap.NewProductionConfig()
	cfg.Level = zap.NewAtomicLevelAt(zap.DebugLevel)
	lg, err := cfg.Build()
	if err != nil {
		t.Fatalf("Failed to build logger: %v", err)
	}

	SetZapLogger(lg)

	if L() != lg {
		t.Errorf("expected logger to be set to %v, but got %v", lg, L())
	}

	L().Info("done")
}

func TestL(t *testing.T) {
	cfg := zap.NewProductionConfig()
	cfg.Level = zap.NewAtomicLevelAt(zap.DebugLevel)
	lg, err := cfg.Build()
	if err != nil {
		t.Fatalf("Failed to build logger: %v", err)
	}

	SetZapLogger(lg)

	retrievedLogger := L()
	if retrievedLogger != lg {
		t.Errorf("expected logger to be %v, but got %v", lg, retrievedLogger)
	}

	S().Infow("done")
}
