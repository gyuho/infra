package main

import (
	"context"
	"fmt"
	"time"

	"github.com/gyuho/infra/aws/go/sts"
)

func main() {
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	id, err := sts.GetCallerIdentity(ctx)
	cancel()
	if err != nil {
		panic(err)
	}

	fmt.Println("ID:", id)
}
