package pods

import (
	"context"
	"time"

	"github.com/gyuho/infra/go/logutil"
	core_v1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	meta_v1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/apimachinery/pkg/util/wait"
	"k8s.io/client-go/kubernetes"
	clientretry "k8s.io/client-go/util/retry"
)

// List nodes with some options.
func List(ctx context.Context, clientset *kubernetes.Clientset, opts ...OpOption) ([]core_v1.Pod, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("listing", "namespace", ret.namespace, "labelSelector", ret.labelSelector)
	resp, err := clientset.CoreV1().Pods(ret.namespace).List(ctx, meta_v1.ListOptions{
		LabelSelector: ret.labelSelector,
	})
	if err != nil {
		return nil, err
	}

	if ret.phase != "" {
		keep := make([]core_v1.Pod, 0)
		for _, pod := range resp.Items {
			if !matchPodPhase(pod, ret.phase) {
				continue
			}
			keep = append(keep, pod)
		}
		resp.Items = keep
	}

	return resp.Items, nil
}

func matchPodPhase(pod core_v1.Pod, desired core_v1.PodPhase) bool {
	if pod.Status.Phase == desired {
		return true
	}
	if string(desired) == "Terminating" && pod.ObjectMeta.DeletionTimestamp != nil {
		return true
	}
	return false
}

// Fetches the pod object by name.
// Use "k8s.io/apimachinery/pkg/api/errors.IsNotFound" to decide whether the node exists or not.
func Get(ctx context.Context, clientset *kubernetes.Clientset, namespace string, name string) (*core_v1.Pod, error) {
	logutil.S().Infow("fetching", "namespace", namespace, "name", name)
	return clientset.CoreV1().Pods(namespace).Get(ctx, name, meta_v1.GetOptions{})
}

// Deletes the pod object by name and waits for its deletion.
// Returns no error if the pod does not exist.
func Delete(ctx context.Context, clientset *kubernetes.Clientset, namespace string, name string) error {
	logutil.S().Infow("deleting", "namespace", namespace, "name", name)
	err := clientset.CoreV1().Pods(namespace).Delete(ctx, name, meta_v1.DeleteOptions{})
	if err != nil {
		if apierrors.IsNotFound(err) {
			logutil.S().Warnw("pod not found", "namespace", namespace, "name", name)
			return nil
		}
		return err
	}
	return nil
}

func RemoveFinalizers(ctx context.Context, clientset *kubernetes.Clientset, namespace string, name string) error {
	patch := []byte(`{"metadata":{"finalizers":null}}`)
	logutil.S().Infow("removing finalizer", "namespace", namespace, "name", name)
	return patchWithRetries(ctx, clientset, namespace, name, patch)
}

// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
var defaultBackoff = wait.Backoff{
	Duration: 10 * time.Second,
	Steps:    6,
	Cap:      2 * time.Minute,
}

func patchWithRetries(ctx context.Context, clientset *kubernetes.Clientset, namespace string, name string, patch []byte) error {
	for {
		_, err := Get(ctx, clientset, namespace, name)
		if err == nil {
			break
		}

		if apierrors.IsNotFound(err) {
			logutil.S().Warnw("pod not found yet -- retrying", "name", name, "error", err)
		} else {
			logutil.S().Warnw("failed to fetch pod -- retrying", "name", name, "error", err)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(10 * time.Second):
		}
	}

	// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			return patchOnce(ctx, clientset, namespace, name, patch)
		},
	)
}

func patchOnce(ctx context.Context, clientset *kubernetes.Clientset, namespace string, name string, patch []byte) error {
	logutil.S().Infow("applying patch", "namespace", namespace, "name", name, "patch", string(patch))
	_, err := clientset.CoreV1().Pods(namespace).Patch(ctx, name, types.StrategicMergePatchType, patch, meta_v1.PatchOptions{})
	return err
}
