package main

import (
	"context"
	"fmt"
	"time"

	aws "github.com/gyuho/infra/aws/go"
)

func main() {
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	id, err := aws.GetCallerIdentity(ctx)
	cancel()
	if err != nil {
		panic(err)
	}

	fmt.Println("ID:", id)
}
