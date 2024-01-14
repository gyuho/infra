package k8s

import (
	"github.com/gyuho/infra/go/logutil"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/tools/clientcmd"
)

func New(kubeconfig string) (*kubernetes.Clientset, error) {
	logutil.S().Infow("loading kubeconfig", "kubeconfig", kubeconfig)
	restConfig, err := clientcmd.BuildConfigFromFlags("", kubeconfig)
	if err != nil {
		return nil, err
	}
	return kubernetes.NewForConfig(restConfig)
}
