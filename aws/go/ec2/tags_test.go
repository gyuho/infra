package ec2

import (
	"reflect"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	aws_ec2_v2_types "github.com/aws/aws-sdk-go-v2/service/ec2/types"
)

func Test_toTags(t *testing.T) {
	tt := []struct {
		testName string
		name     string
		m        map[string]string
		want     []aws_ec2_v2_types.Tag
	}{
		{
			testName: "empty map",
			name:     "",
			m:        map[string]string{},
			want:     []aws_ec2_v2_types.Tag{},
		},
		{
			testName: "single key-value pair",
			name:     "",
			m:        map[string]string{"key1": "value1"},
			want:     []aws_ec2_v2_types.Tag{{Key: aws.String("key1"), Value: aws.String("value1")}},
		},
		{
			testName: "multiple key-value pairs",
			name:     "",
			m:        map[string]string{"key1": "value1", "key2": "value2", "key3": "value3"},
			want: []aws_ec2_v2_types.Tag{
				{Key: aws.String("key1"), Value: aws.String("value1")},
				{Key: aws.String("key2"), Value: aws.String("value2")},
				{Key: aws.String("key3"), Value: aws.String("value3")},
			},
		},
		{
			testName: "multiple key-value pairs unsorted",
			name:     "",
			m:        map[string]string{"key3": "value3", "key2": "value2", "key1": "value1"},
			want: []aws_ec2_v2_types.Tag{
				{Key: aws.String("key1"), Value: aws.String("value1")},
				{Key: aws.String("key2"), Value: aws.String("value2")},
				{Key: aws.String("key3"), Value: aws.String("value3")},
			},
		},
		{
			testName: "multiple key-value pairs unsorted, with name",
			name:     "hello",
			m:        map[string]string{"key3": "value3", "key2": "value2", "key1": "value1"},
			want: []aws_ec2_v2_types.Tag{
				{Key: aws.String("Name"), Value: aws.String("hello")},
				{Key: aws.String("key1"), Value: aws.String("value1")},
				{Key: aws.String("key2"), Value: aws.String("value2")},
				{Key: aws.String("key3"), Value: aws.String("value3")},
			},
		},
	}
	for i, tc := range tt {
		t.Run(tc.testName, func(t *testing.T) {
			got := toTags(tc.name, tc.m)
			if !reflect.DeepEqual(got, tc.want) {
				t.Errorf("#%d: toTags(%v) = %v, want %v", i, tc.m, got, tc.want)
			}
		})
	}
}
