package aws

import (
	"context"
	"errors"
	"fmt"
	"time"

	aws_v2 "github.com/aws/aws-sdk-go-v2/aws"
	config_v2 "github.com/aws/aws-sdk-go-v2/config"
)

type Config struct {
	DebugAPICalls bool
	Region        string
}

func New(cfg *Config) (awsCfg aws_v2.Config, err error) {
	if cfg == nil {
		return aws_v2.Config{}, errors.New("got empty config")
	}
	if cfg.Region == "" {
		return aws_v2.Config{}, fmt.Errorf("missing region")
	}

	optFns := []func(*config_v2.LoadOptions) error{
		(func(*config_v2.LoadOptions) error)(config_v2.WithRegion(cfg.Region)),
	}
	if cfg.DebugAPICalls {
		lvl := aws_v2.LogSigning |
			aws_v2.LogRetries |
			aws_v2.LogRequest |
			aws_v2.LogRequestWithBody |
			aws_v2.LogResponse |
			aws_v2.LogResponseWithBody
		optFns = append(optFns, (func(*config_v2.LoadOptions) error)(config_v2.WithClientLogMode(lvl)))
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	awsCfg, err = config_v2.LoadDefaultConfig(ctx, optFns...)
	cancel()
	if err != nil {
		return aws_v2.Config{}, fmt.Errorf("failed to load config %v", err)
	}

	return awsCfg, nil
}
