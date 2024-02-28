package csrs

import (
	"context"
	"errors"
	"sort"
	"strings"
	"time"

	"github.com/gyuho/infra/go/logutil"

	certs_v1 "k8s.io/api/certificates/v1"
	core_v1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	meta_v1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/util/wait"
	clientretry "k8s.io/client-go/util/retry"
	runtimeclient "sigs.k8s.io/controller-runtime/pkg/client"
)

var ErrClientNotFound = errors.New("client not found")

// List CSRs with some options.
func List(ctx context.Context, opts ...OpOption) ([]certs_v1.CertificateSigningRequest, error) {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return nil, err
	}

	logutil.S().Infow("listing")

	var csrs []certs_v1.CertificateSigningRequest
	switch {
	case options.clientset != nil:
		csrList, err := options.clientset.CertificatesV1().CertificateSigningRequests().List(ctx, meta_v1.ListOptions{})
		if err != nil {
			return nil, err
		}
		csrs = csrList.Items

	case options.runtimeClient != nil:
		obj := &certs_v1.CertificateSigningRequestList{}
		err := options.runtimeClient.List(ctx, obj, &runtimeclient.ListOptions{})
		if err != nil {
			return nil, err
		}
		csrs = obj.Items

	default:
		return nil, ErrClientNotFound
	}

	if len(options.usernames) > 0 {
		logutil.S().Infow("filtering CSRs by usernames", "usernames", options.usernames)
		keep := make([]certs_v1.CertificateSigningRequest, 0)
		for _, csr := range csrs {
			if _, ok := options.usernames[csr.Spec.Username]; ok {
				keep = append(keep, csr)
			}
		}
		csrs = keep
	}

	if options.selectPendings {
		logutil.S().Debugw("selecting only the CSRs with pending condition")
		keep := make([]certs_v1.CertificateSigningRequest, 0)
		for _, csr := range csrs {
			if !IsPending(csr) {
				continue
			}
			keep = append(keep, csr)
		}
		csrs = keep
	}

	if options.minCreateAge > 0 {
		logutil.S().Debugw("selecting with minimum create age", "age", options.minCreateAge)
		now := time.Now()
		keep := make([]certs_v1.CertificateSigningRequest, 0)
		for _, csr := range csrs {
			dur := now.Sub(csr.CreationTimestamp.Time)
			if dur < options.minCreateAge {
				continue
			}
			keep = append(keep, csr)
		}
		csrs = keep
	}

	// sort by creation timestamp, the oldest first
	sort.SliceStable(csrs, func(i, j int) bool {
		return csrs[i].CreationTimestamp.Before(&csrs[j].CreationTimestamp)
	})
	return csrs, nil
}

// Fetches the CSR object by name.
// Use "k8s.io/apimachinery/pkg/api/errors.IsNotFound" to decide whether the node exists or not.
func Get(ctx context.Context, name string, opts ...OpOption) (*certs_v1.CertificateSigningRequest, error) {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return nil, err
	}

	logutil.S().Infow("fetching", "name", name)

	var csr *certs_v1.CertificateSigningRequest
	switch {
	case options.clientset != nil:
		resp, err := options.clientset.
			CertificatesV1().
			CertificateSigningRequests().
			Get(ctx, name, meta_v1.GetOptions{})
		if err != nil {
			return nil, err
		}
		csr = resp

	case options.runtimeClient != nil:
		obj := &certs_v1.CertificateSigningRequest{}
		err := options.runtimeClient.Get(ctx, runtimeclient.ObjectKey{Name: name}, obj)
		if err != nil {
			return nil, err
		}
		csr = obj

	default:
		return nil, ErrClientNotFound
	}

	return csr, nil
}

func IsPending(csr certs_v1.CertificateSigningRequest) bool {
	// CSR is pending when
	return len(csr.Status.Conditions) == 0 && len(csr.Status.Certificate) == 0
}

var defaultBackoff = wait.Backoff{
	Duration: 10 * time.Second,
	Steps:    6,
	Cap:      2 * time.Minute,
}

func Delete(ctx context.Context, names []string, opts ...OpOption) error {
	for _, name := range names {
		if err := clientretry.RetryOnConflict(defaultBackoff, func() error {
			return delete(ctx, name, opts...)
		}); err != nil {
			return err
		}
	}
	return nil
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
		err = options.clientset.
			CertificatesV1().
			CertificateSigningRequests().
			Delete(ctx, name, meta_v1.DeleteOptions{})

	case options.runtimeClient != nil:
		err = options.runtimeClient.Delete(ctx, &certs_v1.CertificateSigningRequest{
			ObjectMeta: meta_v1.ObjectMeta{
				Name: name,
			},
		})

	default:
		return ErrClientNotFound
	}

	if apierrors.IsNotFound(err) {
		logutil.S().Warnw("csr not found", "name", name)
		return nil
	}
	return err
}

// Approves all CSRs by name in a goroutine pool, to speed up the node join process.
func Approve(ctx context.Context, names []string, opts ...OpOption) error {
	for _, csrName := range names {
		if err := clientretry.RetryOnConflict(defaultBackoff, func() error {
			return approve(ctx, csrName, opts...)
		}); err != nil {
			return err
		}
	}
	return nil
}

// ref. https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/kubectl/pkg/cmd/certificates/certificates.go
func approve(ctx context.Context, name string, opts ...OpOption) error {
	options := &Op{}
	if err := options.applyOpts(opts); err != nil {
		return err
	}

	logutil.S().Infow("approving", "name", name)

	updated, err := Get(ctx, name, opts...)
	if err != nil {
		return err
	}
	updated.Status.Conditions = append(updated.Status.Conditions,
		certs_v1.CertificateSigningRequestCondition{
			Type:               certs_v1.CertificateApproved,
			Status:             core_v1.ConditionTrue,
			Reason:             "KubectlApprove",
			Message:            "This CSR was approved by kubectl certifcate approve.",
			LastUpdateTime:     meta_v1.Now(),
			LastTransitionTime: meta_v1.Now(),
		},
	)

	switch {
	case options.clientset != nil:
		_, err = options.clientset.
			CertificatesV1().
			CertificateSigningRequests().
			UpdateApproval(
				ctx,
				name,
				updated,
				meta_v1.UpdateOptions{
					FieldManager: "machine-manager-k8s-cert-approve",
				},
			)

	case options.runtimeClient != nil:
		err = options.runtimeClient.SubResource("approval").Update(ctx, updated, &runtimeclient.SubResourceUpdateOptions{
			UpdateOptions: runtimeclient.UpdateOptions{
				FieldManager: "machine-manager-k8s-cert-approve",
			},
		})

	default:
		return ErrClientNotFound
	}

	if err == nil {
		logutil.S().Infow("approved", "name", name)
	} else {
		// e.g.,
		// "CertificateSigningRequest.certificates.k8s.io \"csr-42pnj\" is invalid: status.conditions[1].type: Duplicate value: \"Approved\""
		if strings.Contains(err.Error(), "Duplicate value") && strings.Contains(err.Error(), "Approved") {
			logutil.S().Infow("duplicate CSR approve", "name", name)
			return nil
		}
	}

	return err
}
