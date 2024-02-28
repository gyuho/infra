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
	clientretry "k8s.io/client-go/util/retry"
	"k8s.io/kubectl/pkg/drain"
	taintutils "k8s.io/kubernetes/pkg/util/taints"
	runtimeclient "sigs.k8s.io/controller-runtime/pkg/client"
)

var ErrClientNotFound = errors.New("client not found")

// List nodes with some options.
func List(ctx context.Context, opts ...OpOption) ([]core_v1.Node, error) {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return nil, err
	}

	logutil.S().Infow("listing")

	var nodes []core_v1.Node
	switch {
	case options.clientset != nil:
		lsOpts := meta_v1.ListOptions{}
		if options.fieldSelector != nil {
			lsOpts.FieldSelector = options.fieldSelector.String()
		}
		if options.labelSelector != nil {
			lsOpts.LabelSelector = options.labelSelector.String()
		}
		resp, err := options.clientset.CoreV1().Nodes().List(ctx, lsOpts)
		if err != nil {
			return nil, err
		}
		nodes = resp.Items

	case options.runtimeClient != nil:
		obj := &core_v1.NodeList{}
		err := options.runtimeClient.List(ctx, obj, &runtimeclient.ListOptions{
			FieldSelector: options.fieldSelector,
			LabelSelector: options.labelSelector,
		})
		if err != nil {
			return nil, err
		}
		nodes = obj.Items

	default:
		return nil, ErrClientNotFound
	}

	if len(nodes) > 1 {
		// sort by creation timestamp, the oldest first
		sort.SliceStable(nodes, func(i, j int) bool {
			return nodes[i].CreationTimestamp.Before(&nodes[j].CreationTimestamp)
		})
	}

	if options.conditionType != "" {
		keep := make([]core_v1.Node, 0)
		for _, node := range nodes {
			if matchConditionType(node.Status, options.conditionType) {
				keep = append(keep, node)
			}
		}
		nodes = keep
	}

	return nodes, nil
}

func matchConditionType(status core_v1.NodeStatus, desired core_v1.NodeConditionType) bool {
	for _, cond := range status.Conditions {
		if cond.Status != core_v1.ConditionTrue {
			continue
		}
		if cond.Type == desired {
			return true
		}
	}
	return false
}

// Fetches the node object by name.
// Use "k8s.io/apimachinery/pkg/api/errors.IsNotFound" to decide whether the node exists or not.
func Get(ctx context.Context, name string, opts ...OpOption) (*core_v1.Node, error) {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return nil, err
	}

	logutil.S().Infow("fetching node", "name", name)

	var node *core_v1.Node
	switch {
	case options.clientset != nil:
		resp, err := options.clientset.CoreV1().Nodes().Get(ctx, name, meta_v1.GetOptions{})
		if err != nil {
			return nil, err
		}
		node = resp

	case options.runtimeClient != nil:
		obj := &core_v1.Node{}
		err := options.runtimeClient.Get(ctx, runtimeclient.ObjectKey{Name: name}, obj)
		if err != nil {
			return nil, err
		}
		node = obj

	default:
		return nil, ErrClientNotFound
	}

	return node, nil
}

// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
var defaultBackoff = wait.Backoff{
	Duration: 10 * time.Second,
	Steps:    6,
	Cap:      2 * time.Minute,
}

// Deletes the node object by name and waits for its deletion.
// Returns no error if the node does not exist.
func Delete(ctx context.Context, name string, opts ...OpOption) error {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return err
	}

	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			return delete(ctx, name, opts...)
		},
	)
}

func delete(ctx context.Context, name string, opts ...OpOption) error {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return err
	}

	logutil.S().Infow("deleting", "name", name)

	var err error
	switch {
	case options.clientset != nil:
		err = options.clientset.CoreV1().Nodes().Delete(ctx, name, meta_v1.DeleteOptions{})

	case options.runtimeClient != nil:
		err = options.runtimeClient.Delete(ctx, &core_v1.Node{
			ObjectMeta: meta_v1.ObjectMeta{
				Name: name,
			},
		})

	default:
		return ErrClientNotFound
	}

	if apierrors.IsNotFound(err) {
		logutil.S().Warnw("node not found", "name", name)
		return nil
	}
	return err
}

func ApplyLabels(ctx context.Context, name string, labels map[string]string, opts ...OpOption) error {
	logutil.S().Infow("applying labels", "name", name, "labels", labels)
	p := nodePatch{Metadata: &meta_v1.ObjectMeta{
		Labels: labels,
	}}
	patchBytes, err := json.Marshal(p)
	if err != nil {
		return err
	}
	return strategicMergePatchWithRetries(ctx, name, patchBytes, opts...)
}

func Cordon(ctx context.Context, name string, opts ...OpOption) error {
	logutil.S().Infow("cordoning", "name", name)
	patchBytes, err := json.Marshal(nodePatch{Spec: &core_v1.NodeSpec{
		Unschedulable: true,
	}})
	if err != nil {
		return err
	}
	return strategicMergePatchWithRetries(ctx, name, patchBytes, opts...)
}

// Removes "node.kubernetes.io/unschedulable" taint from the node.
// Sets "spec.unschedulable" to false.
// ref. https://github.com/leptonai/lepton/issues/5257
// ref. "pkg/controller/nodelifecycle/node_lifecycle_controller.go" "markNodeAsReachable"
func Uncordon(ctx context.Context, name string, opts ...OpOption) error {
	logutil.S().Infow("uncordoning", "name", name)
	patchBytes, err := json.Marshal(nodePatch{Spec: &core_v1.NodeSpec{
		Unschedulable: false,
	}})
	if err != nil {
		return err
	}
	return strategicMergePatchWithRetries(ctx, name, patchBytes, opts...)
}

func strategicMergePatchWithRetries(ctx context.Context, name string, patchBytes []byte, opts ...OpOption) error {
	for {
		_, err := Get(ctx, name, opts...)
		if err == nil {
			break
		}
		if apierrors.IsNotFound(err) {
			logutil.S().Warnw("node not found yet -- retrying", "name", name, "error", err)
		} else {
			logutil.S().Warnw("failed to fetch node -- retrying", "name", name, "error", err)
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
			return strategicMergePatchOnce(ctx, name, patchBytes, opts...)
		},
	)
}

type nodePatch struct {
	Metadata *meta_v1.ObjectMeta `json:"metadata,omitempty"`
	Spec     *core_v1.NodeSpec   `json:"spec,omitempty"`
}

func strategicMergePatchOnce(ctx context.Context, name string, patchBytes []byte, opts ...OpOption) error {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return err
	}

	logutil.S().Infow("applying strategic merge patch", "name", name, "patch", string(patchBytes))

	var err error
	switch {
	case options.clientset != nil:
		_, err = options.clientset.CoreV1().Nodes().Patch(ctx, name, types.StrategicMergePatchType, patchBytes, meta_v1.PatchOptions{})

	case options.runtimeClient != nil:
		err = options.runtimeClient.Patch(
			ctx,
			&core_v1.Node{
				ObjectMeta: meta_v1.ObjectMeta{
					Name: name,
				},
			},
			runtimeclient.RawPatch(types.StrategicMergePatchType, patchBytes),
		)

	default:
		return ErrClientNotFound
	}

	return err
}

// ref. "pkg/controller/nodelifecycle/node_lifecycle_controller.go" "markNodeAsReachable" "AddOrUpdateTaintOnNode"
func ApplyTaint(ctx context.Context, name string, taint *core_v1.Taint, opts ...OpOption) error {
	logutil.S().Infow("applying taint", "name", name, "taint", *taint)

	// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			origNode, err := Get(ctx, name, opts...)
			if err != nil {
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

			patchBytes, err := strategicpatch.CreateTwoWayMergePatch(origBytes, patchedBytes, core_v1.Node{})
			if err != nil {
				return err
			}

			cctx, ccancel := context.WithTimeout(ctx, 20*time.Second)
			err = strategicMergePatchOnce(cctx, name, patchBytes, opts...)
			ccancel()
			if err == nil {
				logutil.S().Infow("successfully applied taint", "name", name, "taint", *taint)
			}
			return err
		},
	)
}

// ref. "pkg/controller/nodelifecycle/node_lifecycle_controller.go" "markNodeAsReachable" "AddOrUpdateTaintOnNode"
func DeleteTaint(ctx context.Context, name string, taint *core_v1.Taint, opts ...OpOption) error {
	logutil.S().Infow("deleting taint", "name", name, "taint", *taint)

	// ref. https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/controller_utils.go
	return clientretry.RetryOnConflict(
		defaultBackoff,
		func() error {
			origNode, err := Get(ctx, name, opts...)
			if err != nil {
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

			patchBytes, err := strategicpatch.CreateTwoWayMergePatch(origBytes, patchedBytes, core_v1.Node{})
			if err != nil {
				return err
			}

			cctx, ccancel := context.WithTimeout(ctx, 20*time.Second)
			err = strategicMergePatchOnce(cctx, name, patchBytes, opts...)
			ccancel()
			if err == nil {
				logutil.S().Infow("successfully deleted taint", "name", name, "taint", *taint)
			}
			return err
		},
	)
}

// Drains the node.
func Drain(ctx context.Context, name string, opts ...OpOption) error {
	options := &Op{
		timeout:     15 * time.Second,
		gracePeriod: 10 * time.Second,
	}
	if err := options.applyOpts(opts); err != nil {
		return err
	}
	if options.clientset == nil {
		return errors.New("clientset is required")
	}

	logutil.S().Infow("draining", "name", name)

	// e.g.,
	// k drain ip-10-0-7-236.us-west-2.compute.internal --delete-emptydir-data --ignore-daemonsets
	drainHelper := &drain.Helper{
		Client:             options.clientset,
		Force:              options.force,
		GracePeriodSeconds: int(options.gracePeriod.Seconds()),

		// if false, drain may fail with:
		// cannot delete DaemonSet-managed Pods (use --ignore-daemonsets to ignore): calico-system/calico-node-x84km, calico-system/csi-node-driver-vnx2m, gpu-operator/gpu-operator-node-feature-discovery-worker-qwf85
		IgnoreAllDaemonSets: options.ignoreDaemonSets,

		Timeout:            options.timeout,
		DeleteEmptyDirData: options.deleteEmptyDirData,

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
