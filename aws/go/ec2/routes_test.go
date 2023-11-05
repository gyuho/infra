package ec2

import (
	"context"
	"fmt"
	"os"
	"reflect"
	"testing"
	"time"

	aws "github.com/gyuho/infra/aws/go"
)

func TestSortRouteTables(t *testing.T) {
	tt := []struct {
		input    RouteTables
		expected RouteTables
	}{
		{
			input:    RouteTables{{ID: "2"}, {ID: "1"}, {ID: "0"}},
			expected: RouteTables{{ID: "0"}, {ID: "1"}, {ID: "2"}},
		},
		{
			input:    RouteTables{{ID: "a"}, {ID: "b"}, {ID: "c"}},
			expected: RouteTables{{ID: "a"}, {ID: "b"}, {ID: "c"}},
		},
		{
			input:    RouteTables{{ID: "b"}, {ID: "c"}, {ID: "a"}},
			expected: RouteTables{{ID: "a"}, {ID: "b"}, {ID: "c"}},
		},
		{
			input:    RouteTables{{ID: "b"}, {ID: "a"}, {ID: "c"}},
			expected: RouteTables{{ID: "a"}, {ID: "b"}, {ID: "c"}},
		},
		{
			input:    RouteTables{{ID: "b"}, {ID: "a", VPCID: "b"}, {ID: "a", VPCID: "a"}},
			expected: RouteTables{{ID: "a", VPCID: "a"}, {ID: "a", VPCID: "b"}, {ID: "b"}},
		},
	}
	for i, tv := range tt {
		tv.input.Sort()
		if !reflect.DeepEqual(tv.input, tv.expected) {
			t.Errorf("#%d: expected %v, got %v", i, tv.expected, tv.input)
		}
	}
}

func TestListRoutes(t *testing.T) {
	if os.Getenv("RUN_AWS_TESTS") != "1" {
		t.Skip()
	}

	cfg, err := aws.New(&aws.Config{
		Region: "us-east-1",
	})
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	rtbs, err := ListRouteTablesByVPC(ctx, cfg, "vpc-0f4e5eacbd41e24e3")
	cancel()
	if err != nil {
		t.Fatal(err)
	}
	fmt.Println(rtbs.String())

	for _, rtb := range rtbs {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		rtb, err := GetRouteTable(ctx, cfg, rtb.ID)
		cancel()
		if err != nil {
			t.Fatal(err)
		}
		fmt.Println(rtb.Routes.String())
	}
}
