package eks

import (
	"encoding/base64"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/gyuho/infra/go/randutil"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
	"k8s.io/client-go/tools/clientcmd"
	clientcmd_api_v1 "k8s.io/client-go/tools/clientcmd/api/v1"
	"sigs.k8s.io/yaml"
)

type Cluster struct {
	Name   string `json:"name"`
	ARN    string `json:"arn"`
	Region string `json:"region"`

	Version         string `json:"version"`
	PlatformVersion string `json:"platform_version"`
	MothershipState string `json:"mothership_state"`
	Status          string `json:"status"`
	Health          string `json:"health"`

	CreatedAt time.Time `json:"created_at"`

	VPCID       string `json:"vpc_id"`
	ClusterSGID string `json:"cluster_sg_id"`

	Endpoint             string `json:"endpoint"`
	CertificateAuthority string `json:"certificate_authority"`
	OIDCIssuer           string `json:"oidc_issuer"`
}

// Creates a k8s clientset.
func (c Cluster) CreateK8sClient() (*kubernetes.Clientset, *rest.Config, string, error) {
	kubeconfigPath, err := c.WriteKubeconfig("")
	if err != nil {
		return nil, nil, "", err
	}

	restConfig, err := clientcmd.BuildConfigFromFlags("", kubeconfigPath)
	if err != nil {
		return nil, nil, kubeconfigPath, fmt.Errorf("failed to build config from kubconfig %v", err)
	}
	clientset, err := kubernetes.NewForConfig(restConfig)
	if err != nil {
		return nil, nil, kubeconfigPath, err
	}

	return clientset, restConfig, kubeconfigPath, nil
}

// Writes a kubeconfig to disk and returns the kubeconfig file path.
func (c Cluster) WriteKubeconfig(p string) (string, error) {
	kcfg, err := c.Kubeconfig()
	if err != nil {
		return "", err
	}
	b, err := yaml.Marshal(kcfg)
	if err != nil {
		return "", err
	}

	if p == "" {
		p = filepath.Join(os.TempDir(), fmt.Sprintf("kubeconfig-%s", randutil.AlphabetsLowerCase(32)))
	}
	if err = os.WriteFile(p, b, 0644); err != nil {
		return "", err
	}

	return p, nil
}

func (c Cluster) Kubeconfig() (clientcmd_api_v1.Config, error) {
	awsPath, err := exec.LookPath("aws")
	if err != nil {
		return clientcmd_api_v1.Config{}, fmt.Errorf("aws cli not found %w", err)
	}

	decoded, err := base64.StdEncoding.DecodeString(c.CertificateAuthority)
	if err != nil {
		return clientcmd_api_v1.Config{}, fmt.Errorf("failed to decode certificate authority %w", err)
	}

	kcfg := clientcmd_api_v1.Config{
		Clusters: []clientcmd_api_v1.NamedCluster{
			{
				Name: c.ARN,
				Cluster: clientcmd_api_v1.Cluster{
					Server:                   c.Endpoint,
					CertificateAuthorityData: decoded,
				},
			},
		},
		Contexts: []clientcmd_api_v1.NamedContext{
			{
				Name: c.ARN,
				Context: clientcmd_api_v1.Context{
					Cluster:  c.ARN,
					AuthInfo: c.ARN,
				},
			},
		},
		CurrentContext: c.ARN,
		AuthInfos: []clientcmd_api_v1.NamedAuthInfo{
			{
				Name: c.ARN,
				AuthInfo: clientcmd_api_v1.AuthInfo{
					Exec: &clientcmd_api_v1.ExecConfig{
						APIVersion: "client.authentication.k8s.io/v1beta1",
						Command:    awsPath,
						Args: []string{
							"--region",
							c.Region,
							"eks",
							"get-token",
							"--cluster-name",
							c.Name,
							"--output",
							"json",
						},
					},
				},
			},
		},
	}
	return kcfg, nil
}
