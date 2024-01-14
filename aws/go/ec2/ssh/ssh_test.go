package ssh

import (
	"fmt"
	"testing"
)

func TestSSH(t *testing.T) {
	s := Command{
		SSHKeyPath:       "/path/to/ssh/key",
		SSHUser:          "ec2-user",
		Region:           "us-east-1",
		AvailabilityZone: "us-east-1a",
		InstanceID:       "i-1234567890abcdef0",
		InstanceState:    "running",
		IPKind:           "public",
		IP:               "1.2.3.4",
		Profile:          "default",
	}
	fmt.Println(s.String())

	s.SSHKeyPath = ""
	s.Profile = ""
	fmt.Println(s.String())
}
