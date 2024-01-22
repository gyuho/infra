package nodes

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sort"
	"time"

	"github.com/gyuho/infra/go/logutil"
	core_v1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	meta_v1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/apimachinery/pkg/util/strategicpatch"
	"k8s.io/apimachinery/pkg/util/wait"
	"k8s.io/client-go/kubernetes"
	clientretry "k8s.io/client-go/util/retry"
	"k8s.io/kubectl/pkg/drain"
	taintutils "k8s.io/kubernetes/pkg/util/taints"
)

// List nodes with some options.
func List(ctx context.Context, clientset *kubernetes.Clientset, opts ...OpOption) ([]core_v1.Node, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("listing nodes", "labelSelector", ret.labelSelector)
	resp, err := clientset.CoreV1().Nodes().List(ctx, meta_v1.ListOptions{
		LabelSelector: ret.labelSelector,
	})
	if err != nil {
		return nil, err
	}

	if len(resp.Items) > 1 {
		// sort by creation timestamp, the oldest first
		sort.SliceStable(resp.Items, func(i, j int) bool {
			return resp.Items[i].CreationTimestamp.Before(&resp.Items[j].CreationTimestamp)
		})
	}

	return resp.Items, nil
}

// Fetches the node object by name, and returns false and no error if not found.
func Get(ctx context.Context, clientset *kubernetes.Clientset, name string) (*core_v1.Node, bool, error) {
	logutil.S().Infow("fetching node", "name", name)
	node, err := clientset.CoreV1().Nodes().Get(ctx, name, meta_v1.GetOptions{})
	if err != nil {
		if apierrors.IsNotFound(err) {
			logutil.S().Warnw("node not found", "name", name)
			return nil, false, nil
		}
		return nil, false, err
	}
	return node, true, nil
}

// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
var defaultBackoff = wait.Backoff{
	Duration: 10 * time.Second,
	Steps:    6,
	Cap:      2 * time.Minute,
}

// Deletes the node object by name and waits for its deletion.
// Returns no error if the node does not exist.
func Delete(ctx context.Context, clientset *kubernetes.Clientset, name string) error {
	logutil.S().Infow("deleting", "name", name)
	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			err := clientset.CoreV1().Nodes().Delete(ctx, name, meta_v1.DeleteOptions{})
			if err != nil {
				if apierrors.IsNotFound(err) {
					logutil.S().Warnw("node not found", "name", name)
					return nil
				}
				return err
			}
			return nil
		},
	)
}

func ApplyLabels(ctx context.Context, clientset *kubernetes.Clientset, name string, labels map[string]string) error {
	logutil.S().Infow("applying labels", "name", name, "labels", labels)
	return patchWithRetries(ctx, clientset, name, nodePatch{Metadata: &meta_v1.ObjectMeta{
		Labels: labels,
	}})
}

func Cordon(ctx context.Context, clientset *kubernetes.Clientset, name string) error {
	logutil.S().Infow("cordoning", "name", name)
	return patchWithRetries(ctx, clientset, name, nodePatch{Spec: &core_v1.NodeSpec{
		Unschedulable: true,
	}})
}

// Removes "node.kubernetes.io/unschedulable" taint from the node.
// Sets "spec.unschedulable" to false.
// ref. https://github.com/leptonai/lepton/issues/5257
// ref. "pkg/controller/nodelifecycle/node_lifecycle_controller.go" "markNodeAsReachable"
func Uncordon(ctx context.Context, clientset *kubernetes.Clientset, name string) error {
	logutil.S().Infow("uncordoning", "name", name)
	return patchWithRetries(ctx, clientset, name, nodePatch{Spec: &core_v1.NodeSpec{
		Unschedulable: false,
	}})
}

func patchWithRetries(ctx context.Context, clientset *kubernetes.Clientset, name string, patch nodePatch) error {
	for {
		_, exists, err := Get(ctx, clientset, name)
		if err == nil && exists {
			break
		}

		logutil.S().Warnw("node not found yet -- retrying", "name", name, "error", err)
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
			return patchOnce(ctx, clientset, name, patch)
		},
	)
}

func patchOnce(ctx context.Context, clientset *kubernetes.Clientset, name string, patch nodePatch) error {
	patchBytes, err := json.Marshal(patch)
	if err != nil {
		return err
	}
	logutil.S().Infow("applying patch", "name", name, "patch", string(patchBytes))
	_, err = clientset.CoreV1().Nodes().Patch(ctx, name, types.StrategicMergePatchType, patchBytes, meta_v1.PatchOptions{})
	return err
}

type nodePatch struct {
	Metadata *meta_v1.ObjectMeta `json:"metadata,omitempty"`
	Spec     *core_v1.NodeSpec   `json:"spec,omitempty"`
}

// ref. "pkg/controller/nodelifecycle/node_lifecycle_controller.go" "markNodeAsReachable" "AddOrUpdateTaintOnNode"
func ApplyTaint(ctx context.Context, clientset *kubernetes.Clientset, name string, taint *core_v1.Taint) error {
	logutil.S().Infow("applying taint", "name", name, "taint", *taint)

	// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			origNode, exists, err := Get(ctx, clientset, name)
			if err != nil || !exists {
				return fmt.Errorf("failed to get node %q: %v", name, err)
			}
			copied := origNode.DeepCopy()

			patchedNode, updated, err := taintutils.AddOrUpdateTaint(copied, taint)
			if err != nil {
				return errors.New("failed to update taint of node")
			}
			if !updated {
				logutil.S().Infow("taint already exists -- no need to patch", "name", name, "taint", *taint)
				return nil
			}

			origNode.ResourceVersion = ""
			origBytes, err := json.Marshal(origNode)
			if err != nil {
				return err
			}

			copied.Spec.Taints = patchedNode.Spec.Taints
			patchedBytes, err := json.Marshal(copied)
			if err != nil {
				return err
			}

			patch, err := strategicpatch.CreateTwoWayMergePatch(origBytes, patchedBytes, core_v1.Node{})
			if err != nil {
				return err
			}

			cctx, ccancel := context.WithTimeout(ctx, 20*time.Second)
			_, err = clientset.CoreV1().Nodes().Patch(cctx, name, types.StrategicMergePatchType, patch, meta_v1.PatchOptions{})
			ccancel()
			if err == nil {
				logutil.S().Infow("successfully applied taint", "name", name, "taint", *taint)
			}
			return err
		},
	)
}

// ref. "pkg/controller/nodelifecycle/node_lifecycle_controller.go" "markNodeAsReachable" "AddOrUpdateTaintOnNode"
func DeleteTaint(ctx context.Context, clientset *kubernetes.Clientset, name string, taint *core_v1.Taint) error {
	logutil.S().Infow("deleting taint", "name", name, "taint", *taint)

	// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			origNode, exists, err := Get(ctx, clientset, name)
			if err != nil || !exists {
				return fmt.Errorf("failed to get node %q: %v", name, err)
			}
			copied := origNode.DeepCopy()

			patchedNode, updated, err := taintutils.RemoveTaint(copied, taint)
			if err != nil {
				return errors.New("failed to update taint of node")
			}
			if !updated {
				logutil.S().Infow("taint not found -- no need to patch", "name", name, "taint", *taint)
				return nil
			}

			logutil.S().Warnw("found taint -- deleting", "name", name, "taint", *taint)
			origNode.ResourceVersion = ""
			origBytes, err := json.Marshal(origNode)
			if err != nil {
				return err
			}

			copied.Spec.Taints = patchedNode.Spec.Taints
			patchedBytes, err := json.Marshal(copied)
			if err != nil {
				return err
			}

			patch, err := strategicpatch.CreateTwoWayMergePatch(origBytes, patchedBytes, core_v1.Node{})
			if err != nil {
				return err
			}

			cctx, ccancel := context.WithTimeout(ctx, 20*time.Second)
			_, err = clientset.CoreV1().Nodes().Patch(cctx, name, types.StrategicMergePatchType, patch, meta_v1.PatchOptions{})
			ccancel()
			if err == nil {
				logutil.S().Infow("successfully deleted taint", "name", name, "taint", *taint)
			}
			return err
		},
	)
}

// Drains the node.
func Drain(ctx context.Context, clientset *kubernetes.Clientset, name string, opts ...OpOption) error {
	ret := &Op{
		timeout:     15 * time.Second,
		gracePeriod: 10 * time.Second,
	}
	ret.applyOpts(opts)

	logutil.S().Infow("draining", "name", name)

	// e.g.,
	// k drain ip-10-0-7-236.us-west-2.compute.internal --delete-emptydir-data --ignore-daemonsets
	drainHelper := &drain.Helper{
		Client:             clientset,
		Force:              ret.force,
		GracePeriodSeconds: int(ret.gracePeriod.Seconds()),

		// if false, drain may fail with:
		// cannot delete DaemonSet-managed Pods (use --ignore-daemonsets to ignore): calico-system/calico-node-x84km, calico-system/csi-node-driver-vnx2m, gpu-operator/gpu-operator-node-feature-discovery-worker-qwf85
		IgnoreAllDaemonSets: ret.ignoreDaemonSets,

		Timeout:            ret.timeout,
		DeleteEmptyDirData: ret.deleteEmptyDirData,

		Out:    os.Stdout,
		ErrOut: os.Stderr,
	}
	err := drain.RunNodeDrain(drainHelper, name)
	if err != nil {
		return err
	}

	logutil.S().Warnw("successfully drained", "name", name)
	return nil
}

func IsReady(node *core_v1.Node) bool {
	for _, cond := range node.Status.Conditions {
		if cond.Type == core_v1.NodeReady && cond.Status == core_v1.ConditionTrue {
			return true
		}
	}
	return false
}
