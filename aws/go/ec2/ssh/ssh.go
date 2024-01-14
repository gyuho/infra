// Package ssh provides a simple EC2 SSH helper.
// ref. https://github.com/gyuho/ssh-scp-manager/blob/main/src/ssh/aws.rs
package ssh

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"text/template"
)

type Command struct {
	SSHKeyPath string `json:"ssh_key_path"`
	SSHUser    string `json:"ssh_user"`

	Region           string `json:"region"`
	AvailabilityZone string `json:"availability_zone"`

	InstanceID    string `json:"instance_id"`
	InstanceState string `json:"instance_state"`

	// Either "public" or "private".
	IPKind string `json:"ip_kind"`
	IP     string `json:"ip"`

	Profile string `json:"profile"`
}

const tmplCommands = `{{if .SSHKeyPath }}# change SSH key permission
chmod 400 {{ .SSHKeyPath }}

# instance '{{ .InstanceID }}' ({{ .InstanceState }}, {{ .AvailabilityZone }}) -- ip kind '{{ .IPKind }}'
ssh -o "StrictHostKeyChecking no" -i {{ .SSHKeyPath }} {{ .SSHUser }}@{{ .IP }}
ssh -o "StrictHostKeyChecking no" -i {{ .SSHKeyPath }} {{ .SSHUser }}@{{ .IP }} 'tail -10 /var/log/cloud-init-output.log'

# download a remote file to local machine
scp -i {{ .SSHKeyPath }} {{ .SSHUser }}@{{ .IP }}:REMOTE_FILE_PATH LOCAL_FILE_PATH
scp -i {{ .SSHKeyPath }} -r {{ .SSHUser }}@{{ .IP }}:REMOTE_DIRECTORY_PATH LOCAL_DIRECTORY_PATH

# upload a local file to remote machine
scp -i {{ .SSHKeyPath }} LOCAL_FILE_PATH {{ .SSHUser }}@{{ .IP }}:REMOTE_FILE_PATH
scp -i {{ .SSHKeyPath }} -r LOCAL_DIRECTORY_PATH {{ .SSHUser }}@{{ .IP }}:REMOTE_DIRECTORY_PATH

{{end}}# AWS SSM session (requires a running SSM agent)
# https://github.com/aws/amazon-ssm-agent/issues/131
aws ssm start-session {{ .Profile }}--region {{ .Region }} --target {{ .InstanceID }}
aws ssm start-session {{ .Profile }}--region {{ .Region }} --target {{ .InstanceID }} --document-name 'AWS-StartNonInteractiveCommand' --parameters command="sudo tail -10 /var/log/cloud-init-output.log"
aws ssm start-session {{ .Profile }}--region {{ .Region }} --target {{ .InstanceID }} --document-name 'AWS-StartInteractiveCommand' --parameters command="bash -l"
`

func (c Command) String() string {
	profile := c.Profile
	if profile != "" {
		profile = fmt.Sprintf("--profile %s ", c.Profile)
	}

	tpl := template.Must(template.New("tmplCommands").Parse(tmplCommands))
	buf := bytes.NewBuffer(nil)
	if err := tpl.Execute(buf, Command{
		SSHKeyPath: c.SSHKeyPath,
		SSHUser:    c.SSHUser,

		Region:           c.Region,
		AvailabilityZone: c.AvailabilityZone,

		InstanceID:    c.InstanceID,
		InstanceState: c.InstanceState,

		IPKind: c.IPKind,
		IP:     c.IP,

		Profile: profile,
	}); err != nil {
		return err.Error()
	}
	return buf.String()
}

type Commands []Command

func (cs Commands) Sync(filePath string) error {
	if err := os.MkdirAll(filepath.Dir(filePath), 0755); err != nil {
		return err
	}

	buf := bytes.NewBuffer(nil)
	buf.WriteString("#!/bin/bash\n\n")

	for _, c := range cs {
		buf.WriteString(c.String())
		buf.WriteString("\n\n")
	}

	return os.WriteFile(filePath, buf.Bytes(), 0755)
}
