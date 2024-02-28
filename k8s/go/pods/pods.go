package pods

import (
	"context"
	"errors"

	"github.com/gyuho/infra/go/logutil"
	core_v1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	meta_v1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/types"
	runtimeclient "sigs.k8s.io/controller-runtime/pkg/client"
)

var ErrClientNotFound = errors.New("client not found")

// List nodes with some options.
func List(ctx context.Context, opts ...OpOption) ([]core_v1.Pod, error) {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return nil, err
	}

	logutil.S().Infow("listing", "namespace", options.namespace)

	var pods []core_v1.Pod
	switch {
	case options.clientset != nil:
		lsOpts := meta_v1.ListOptions{}
		if options.fieldSelector != nil {
			lsOpts.FieldSelector = options.fieldSelector.String()
		}
		if options.labelSelector != nil {
			lsOpts.LabelSelector = options.labelSelector.String()
		}
		resp, err := options.clientset.CoreV1().Pods(options.namespace).List(ctx, lsOpts)
		if err != nil {
			return nil, err
		}
		pods = resp.Items

	case options.runtimeClient != nil:
		obj := &core_v1.PodList{}
		err := options.runtimeClient.List(ctx, obj, &runtimeclient.ListOptions{
			Namespace:     options.namespace,
			FieldSelector: options.fieldSelector,
			LabelSelector: options.labelSelector,
		})
		if err != nil {
			return nil, err
		}
		pods = obj.Items

	default:
		return nil, ErrClientNotFound
	}

	if options.phase != "" {
		// TODO: Do this instead:
		// failedSelector := fields.OneTermEqualSelector("status.phase", "Failed")
		// podListOptions := client.ListOptions{
		// 	Namespace:     namespace,
		// 	FieldSelector: failedSelector,
		// }
		keep := make([]core_v1.Pod, 0)
		for _, pod := range pods {
			if !matchPodPhase(pod, options.phase) {
				continue
			}
			keep = append(keep, pod)
		}
		pods = keep
	}

	return pods, nil
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
func Get(ctx context.Context, namespace string, name string, opts ...OpOption) (*core_v1.Pod, error) {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return nil, err
	}

	logutil.S().Infow("fetching", "namespace", namespace, "name", name)

	var pod *core_v1.Pod
	switch {
	case options.clientset != nil:
		resp, err := options.clientset.CoreV1().Pods(namespace).Get(ctx, name, meta_v1.GetOptions{})
		if err != nil {
			return nil, err
		}
		pod = resp

	case options.runtimeClient != nil:
		obj := &core_v1.Pod{}
		err := options.runtimeClient.Get(ctx, runtimeclient.ObjectKey{Namespace: namespace, Name: name}, obj)
		if err != nil {
			return nil, err
		}
		pod = obj

	default:
		return nil, ErrClientNotFound
	}

	return pod, nil
}

// Deletes the pod object by name and waits for its deletion.
// Returns no error if the pod does not exist.
func Delete(ctx context.Context, namespace string, name string, opts ...OpOption) error {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return err
	}

	logutil.S().Infow("deleting", "namespace", namespace, "name", name)

	var err error
	switch {
	case options.clientset != nil:
		err = options.clientset.CoreV1().Pods(namespace).Delete(ctx, name, meta_v1.DeleteOptions{})

	case options.runtimeClient != nil:
		err = options.runtimeClient.Delete(ctx, &core_v1.Pod{
			ObjectMeta: meta_v1.ObjectMeta{
				Namespace: namespace,
				Name:      name,
			},
		})

	default:
		return ErrClientNotFound
	}

	if apierrors.IsNotFound(err) {
		logutil.S().Warnw("pod not found", "namespace", namespace, "name", name)
		return nil
	}
	return err
}

var removeFinalizersPatch = []byte(`{"metadata":{"finalizers":null}}`)

func RemoveFinalizers(ctx context.Context, namespace string, name string, opts ...OpOption) error {
	logutil.S().Infow("removing finalizer", "namespace", namespace, "name", name)
	return strategicMergePatchOnce(ctx, namespace, name, removeFinalizersPatch, opts...)
}

func strategicMergePatchOnce(ctx context.Context, namespace string, name string, patch []byte, opts ...OpOption) error {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return err
	}

	logutil.S().Infow("applying strategic merge patch", "namespace", namespace, "name", name, "patch", string(patch))

	var err error
	switch {
	case options.clientset != nil:
		_, err = options.clientset.CoreV1().Pods(namespace).Patch(ctx, name, types.StrategicMergePatchType, patch, meta_v1.PatchOptions{})

	case options.runtimeClient != nil:
		err = options.runtimeClient.Patch(
			ctx,
			&core_v1.Pod{
				ObjectMeta: meta_v1.ObjectMeta{
					Namespace: namespace,
					Name:      name,
				},
			},
			runtimeclient.RawPatch(types.StrategicMergePatchType, patch),
		)

	default:
		return ErrClientNotFound
	}

	return err
}
