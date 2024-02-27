package version

import (
	"encoding/json"
	"fmt"
	"time"

	"github.com/spf13/cobra"
)

func init() {
	cobra.EnablePrefixMatching = true
}

func NewCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "version",
		Short: "Prints out the version.",
		Long:  "",
		Args:  cobra.NoArgs,
		Run:   cmdFunc,
	}

	return cmd
}

func cmdFunc(cmd *cobra.Command, args []string) {
	fmt.Println(Version())
}

var (
	// GitCommit is the git commit on build.
	GitCommit = ""
	// ReleaseVersion is the release version.
	ReleaseVersion = ""
	// BuildTime is the build timestamp.
	BuildTime = ""
)

func init() {
	now := time.Now()
	if ReleaseVersion == "" {
		ReleaseVersion = fmt.Sprintf(
			"%d%02d%02d%02d%02d",
			now.Year(),
			int(now.Month()),
			now.Day(),
			now.Hour(),
			now.Minute(),
		)
	}
	if BuildTime == "" {
		BuildTime = now.UTC().String()
	}
}

type version struct {
	GitCommit      string `json:"git-commit"`
	ReleaseVersion string `json:"release-version"`
	BuildTime      string `json:"build-time"`
}

// Version returns the version string.
func Version() string {
	vv := version{
		GitCommit:      GitCommit,
		ReleaseVersion: ReleaseVersion,
		BuildTime:      BuildTime,
	}
	b, err := json.Marshal(vv)
	if err != nil {
		panic(err)
	}
	return string(b)
}
