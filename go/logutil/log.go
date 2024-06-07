package logutil

import (
	"sync"

	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
)

var (
	loggerMu sync.RWMutex
	logger   *zap.Logger
)

func init() {
	lg, err := GetDefaultZapLoggerConfig().Build()
	if err != nil {
		panic(err)
	}

	SetZapLogger(lg)
}

func SetZapLogger(lg *zap.Logger) {
	loggerMu.Lock()
	defer loggerMu.Unlock()

	logger = lg
	zap.ReplaceGlobals(lg)
}

func L() *zap.Logger {
	loggerMu.RLock()
	defer loggerMu.RUnlock()

	return logger
}

func S() *zap.SugaredLogger {
	loggerMu.RLock()
	defer loggerMu.RUnlock()

	return logger.Sugar()
}

// GetDefaultZapLoggerConfig returns a new default zap logger configuration.
func GetDefaultZapLoggerConfig() zap.Config {
	return zap.Config{
		Level: zap.NewAtomicLevelAt(zap.InfoLevel),

		Development: false,
		Sampling: &zap.SamplingConfig{
			Initial:    100,
			Thereafter: 100,
		},

		Encoding: "json",

		// copied from "zap.NewProductionEncoderConfig" with some updates
		EncoderConfig: zapcore.EncoderConfig{
			TimeKey:        "ts",
			LevelKey:       "level",
			NameKey:        "logger",
			CallerKey:      "caller",
			MessageKey:     "msg",
			StacktraceKey:  "stacktrace",
			LineEnding:     zapcore.DefaultLineEnding,
			EncodeLevel:    zapcore.LowercaseLevelEncoder,
			EncodeTime:     zapcore.ISO8601TimeEncoder,
			EncodeDuration: zapcore.StringDurationEncoder,
			EncodeCaller:   zapcore.ShortCallerEncoder,
		},

		// Use "/dev/null" to discard all
		OutputPaths:      []string{"stderr"},
		ErrorOutputPaths: []string{"stderr"},
	}
}
