package csrs

import (
	"context"
	"sort"
	"strings"
	"time"

	"github.com/gyuho/infra/go/logutil"
	"golang.org/x/sync/errgroup"
	certs_v1 "k8s.io/api/certificates/v1"
	core_v1 "k8s.io/api/core/v1"
	meta_v1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/client-go/kubernetes"
)

// List CSRs with some options.
func List(ctx context.Context, clientset *kubernetes.Clientset, opts ...OpOption) ([]certs_v1.CertificateSigningRequest, error) {
	ret := &Op{}
	ret.applyOpts(opts)

	logutil.S().Infow("listing CSRs")
	csrList, err := clientset.CertificatesV1().CertificateSigningRequests().List(ctx, meta_v1.ListOptions{})
	if err != nil {
		return nil, err
	}

	if len(ret.usernames) > 0 {
		logutil.S().Infow("filtering CSRs by usernames", "usernames", ret.usernames)

		filtered := make([]certs_v1.CertificateSigningRequest, 0)
		for _, csr := range csrList.Items {
			if _, ok := ret.usernames[csr.Spec.Username]; ok {
				filtered = append(filtered, csr)
			}
		}

		csrList.Items = filtered
	}

	if ret.listPendings {
		logutil.S().Infow("filtering CSRs by pending condition")

		filtered := make([]certs_v1.CertificateSigningRequest, 0)
		for _, csr := range csrList.Items {
			if len(csr.Status.Conditions) > 0 {
				continue
			}
			if len(csr.Status.Certificate) > 0 {
				continue
			}
			filtered = append(filtered, csr)
		}

		csrList.Items = filtered
	}

	sort.SliceStable(csrList.Items, func(i, j int) bool {
		return csrList.Items[i].CreationTimestamp.Before(&csrList.Items[j].CreationTimestamp)
	})
	return csrList.Items, nil
}

func Approve(ctx context.Context, clientset *kubernetes.Clientset, names []string, opts ...OpOption) error {
	ret := &Op{
		approveInterval: time.Second,
	}
	ret.applyOpts(opts)

	logutil.S().Infow("approving CSRs", "names", names)
	g, ctx := errgroup.WithContext(ctx)
	for i, name := range names {
		if ret.approveLimit > 0 && i >= ret.approveLimit {
			logutil.S().Infow("reached CSR approve limit", "limit", ret.approveLimit)
			break
		}

		if i > 0 {
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-time.After(ret.approveInterval):
			}
		}

		//  // https://golang.org/doc/faq#closures_and_goroutines
		csrName := name

		g.Go(func() error {
			return approve(ctx, clientset, csrName)
		})
	}
	return g.Wait()
}

// ref. https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/kubectl/pkg/cmd/certificates/certificates.go
func approve(ctx context.Context, clientset *kubernetes.Clientset, name string) error {
	logutil.S().Infow("approving CSR", "name", name)

	csr, err := clientset.
		CertificatesV1().
		CertificateSigningRequests().
		Get(ctx, name, meta_v1.GetOptions{})
	if err != nil {
		return err
	}

	csr.Status.Conditions = append(csr.Status.Conditions,
		certs_v1.CertificateSigningRequestCondition{
			Type:               certs_v1.CertificateApproved,
			Status:             core_v1.ConditionTrue,
			Reason:             "KubectlApprove",
			Message:            "This CSR was approved by kubectl certifcate approve.",
			LastUpdateTime:     meta_v1.Now(),
			LastTransitionTime: meta_v1.Now(),
		},
	)
	_, err = clientset.
		CertificatesV1().
		CertificateSigningRequests().
		UpdateApproval(
			ctx,
			name,
			csr,
			meta_v1.UpdateOptions{
				FieldManager: "machine-manager-k8s-cert-approve",
			},
		)
	if err == nil {
		logutil.S().Infow("approved CSR", "name", name)
	} else {
		// e.g.,
		// "CertificateSigningRequest.certificates.k8s.io \"csr-42pnj\" is invalid: status.conditions[1].type: Duplicate value: \"Approved\""
		if strings.Contains(err.Error(), "Duplicate value") && strings.Contains(err.Error(), "Approved") {
			logutil.S().Infow("duplicate CSR approve", "name", name)
			return nil
		}
		logutil.S().Warnw("failed to approve CSR", "name", name, "err", err)
	}

	return err
}

func Delete(ctx context.Context, clientset *kubernetes.Clientset, names []string, opts ...OpOption) error {
	ret := &Op{
		deleteInterval: time.Second,
	}
	ret.applyOpts(opts)

	logutil.S().Infow("deleting CSRs", "names", names)
	g, ctx := errgroup.WithContext(ctx)
	for i, name := range names {
		if ret.deleteLimit > 0 && i >= ret.deleteLimit {
			logutil.S().Infow("reached CSR delete limit", "limit", ret.deleteLimit)
			break
		}

		if i > 0 {
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-time.After(ret.deleteInterval):
			}
		}

		//  // https://golang.org/doc/faq#closures_and_goroutines
		csrName := name

		g.Go(func() error {
			return delete(ctx, clientset, csrName)
		})
	}
	return g.Wait()
}

func delete(ctx context.Context, clientset *kubernetes.Clientset, name string) error {
	logutil.S().Infow("deleting CSR", "name", name)

	err := clientset.
		CertificatesV1().
		CertificateSigningRequests().
		Delete(ctx, name, meta_v1.DeleteOptions{})

	if err == nil {
		logutil.S().Infow("deleted CSR", "name", name)
	} else {
		logutil.S().Warnw("failed to delete CSR", "name", name, "err", err)
	}

	return err
}
