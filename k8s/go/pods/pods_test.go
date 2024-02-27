package pods

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/gyuho/infra/go/randutil"
	k8s "github.com/gyuho/infra/k8s/go"

	core_v1 "k8s.io/api/core/v1"
	meta_v1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

func TestMatchPodPhase(t *testing.T) {
	pod := core_v1.Pod{
		Status: core_v1.PodStatus{
			Phase: core_v1.PodRunning,
		},
		ObjectMeta: meta_v1.ObjectMeta{
			DeletionTimestamp: nil,
		},
	}

	// Test case 1: Matching phase
	if !matchPodPhase(pod, core_v1.PodRunning) {
		t.Errorf("Expected matchPodPhase to return true, but got false")
	}

	// Test case 2: Non-matching phase
	if matchPodPhase(pod, core_v1.PodPending) {
		t.Errorf("Expected matchPodPhase to return false, but got true")
	}

	// Test case 3: Matching "Terminating" phase with deletion timestamp
	pod.ObjectMeta.DeletionTimestamp = &meta_v1.Time{}
	if !matchPodPhase(pod, core_v1.PodPhase("Terminating")) {
		t.Errorf("Expected matchPodPhase to return true, but got false")
	}

	// Test case 4: Matching "Terminating" phase without deletion timestamp
	pod.ObjectMeta.DeletionTimestamp = nil
	if matchPodPhase(pod, core_v1.PodPhase("Terminating")) {
		t.Errorf("Expected matchPodPhase to return false, but got true")
	}
}

// TEST_KUBERNETES_CONFIG=~/.kube/config go test -v -run TestPatch
func TestPatch(t *testing.T) {
	if os.Getenv("TEST_KUBERNETES_CONFIG") == "" {
		t.Skip("TEST_KUBERNETES_CONFIG is not set")
	}

	os.Setenv("KUBECONFIG", os.Getenv("TEST_KUBERNETES_CONFIG"))
	defer os.Unsetenv("KUBECONFIG")

	cli, err := k8s.New(os.Getenv("KUBECONFIG"))
	if err != nil {
		t.Fatal(err)
	}

	podName := randutil.AlphabetsLowerCase(10)
	finalizerName := "my.finalizer.name/hello"

	podSpec := &core_v1.Pod{
		ObjectMeta: meta_v1.ObjectMeta{
			Name:       podName,
			Namespace:  "default",
			Finalizers: []string{finalizerName},
		},
		Spec: core_v1.PodSpec{
			Containers: []core_v1.Container{
				{
					Name:    "busybox",
					Image:   "busybox:latest",
					Command: []string{"sleep", "infinity"},
				},
			},
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	_, err = cli.CoreV1().Pods("default").Create(ctx, podSpec, meta_v1.CreateOptions{})
	cancel()
	if err != nil {
		t.Fatal(err)
	}

	defer func() {
		cctx, ccancel := context.WithTimeout(context.Background(), 30*time.Second)
		derr := Delete(cctx, cli, "default", podName)
		ccancel()
		if derr != nil {
			t.Fatal(derr)
		}
	}()

	for {
		cctx, ccancel := context.WithTimeout(context.Background(), 30*time.Second)
		pod, err := Get(cctx, cli, "default", podName)
		ccancel()
		if err != nil {
			t.Fatal(err)
		}
		if len(pod.Finalizers) > 0 {
			break
		}
		time.Sleep(1 * time.Second)
	}

	cctx, ccancel := context.WithTimeout(context.Background(), 30*time.Second)
	err = RemoveFinalizers(cctx, cli, "default", podName)
	ccancel()
	if err != nil {
		t.Fatal(err)
	}

	cctx, ccancel = context.WithTimeout(context.Background(), 30*time.Second)
	pod, err := Get(cctx, cli, "default", podName)
	ccancel()
	if err != nil {
		t.Fatal(err)
	}
	if len(pod.Finalizers) != 0 {
		t.Fatalf("Finalizers not removed from pod %s", podName)
	}
}
