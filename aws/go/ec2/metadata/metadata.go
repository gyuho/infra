package metadata

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"github.com/gyuho/infra/aws/go/pkg/logutil"
)

// Serves session token for instance metadata service v2.
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
// e.g., curl -X PUT "http://169.254.169.254/latest/api/token" -H "X-aws-ec2-metadata-token-ttl-seconds: 21600"
const IMDS_V2_SESSION_TOKEN_URI = "http://169.254.169.254/latest/api/token"

// Fetches the IMDS v2 token.
func FetchToken(ctx context.Context) (string, error) {
	req, err := http.NewRequest(http.MethodPut, IMDS_V2_SESSION_TOKEN_URI, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("X-aws-ec2-metadata-token-ttl-seconds", "21600")

	cli := &http.Client{}
	resp, err := cli.Do(req.WithContext(ctx))
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	b, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	return string(b), nil
}

// Fetches instance metadata service v2 with the "path".
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-retrieval.html
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
// e.g., curl -H "X-aws-ec2-metadata-token: $TOKEN" -v http://169.254.169.254/latest/meta-data/public-ipv4
func FetchMetadataByPath(ctx context.Context, path string) (string, error) {
	uri := fmt.Sprintf("http://169.254.169.254/latest/meta-data/%s", path)
	logutil.S().Infow("fetching meta-data", "uri", uri)

	token, err := FetchToken(ctx)
	if err != nil {
		return "", err
	}
	req, err := http.NewRequest(http.MethodGet, uri, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("X-aws-ec2-metadata-token", token)

	cli := &http.Client{}
	resp, err := cli.Do(req.WithContext(ctx))
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	b, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	return string(b), nil
}

// Fetches the instance ID on the host EC2 machine.
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
func FetchInstanceID(ctx context.Context) (string, error) {
	return FetchMetadataByPath(ctx, "instance-id")
}

// Fetches the public hostname of the host EC2 machine.
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
func FetchPublicHostname(ctx context.Context) (string, error) {
	return FetchMetadataByPath(ctx, "public-hostname")
}

// Fetches the public IPv4 address of the host EC2 machine.
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
func FetchPublicIPV4(ctx context.Context) (string, error) {
	return FetchMetadataByPath(ctx, "public-ipv4")
}

// Fetches the availability of the host EC2 machine.
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
func FetchAvailabilityZone(ctx context.Context) (string, error) {
	return FetchMetadataByPath(ctx, "placement/availability-zone")
}

// Fetches the region of the host EC2 machine.
// TODO: fix this...
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
func FetchRegion(ctx context.Context) (string, error) {
	az, err := FetchAvailabilityZone(ctx)
	if err != nil {
		return "", err
	}
	return az[:len(az)-1], nil
}

// Represents the instance action.
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/spot-instance-termination-notices.html#instance-action-metadata
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
type InstanceAction struct {
	Action string `json:"action"`

	// Time in RFC339 format in UTC.
	Time time.Time `json:"time"`
}

// Fetches the spot instance action.
//
// If Amazon EC2 is not stopping or terminating the instance, or if you terminated the instance yourself,
// spot/instance-action is not present in the instance metadata thus returning an HTTP 404 error.
//
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/prepare-for-interruptions.html
func FetchSpotInstanceAction(ctx context.Context) (InstanceAction, error) {
	s, err := FetchMetadataByPath(ctx, "spot/instance-action")
	if err != nil {
		return InstanceAction{}, err
	}
	action := InstanceAction{}
	if err := json.Unmarshal([]byte(s), &action); err != nil {
		return InstanceAction{}, err
	}
	return action, nil
}
