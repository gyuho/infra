
# Statis provisioning on AWS

The static EBS volume can ensure data persistency in case of hardware failures. The static TLS certificate enables the static node ID. The idea is to create them only once for each node, and whenever its instance gets terminated, release the resources so they can reused for the new instance. We want to maintain the invariant that each node is one-to-one mapped to a set of resources, via an external storage.

## Conditions and requirements

- No separate control plane -- each bootstrapping daemon should be able to claim or release resources
- Consistent, reliable metadata storage -- resource mapping should be stored in a reliable and fault-tolerant way, and must provide strong consistent reads (e.g., S3)
- Idempotent resource creation -- do not create unnecessary resources, instead reuse the available ones
- Resource tagging for delete -- all resources should be uniquely tagged for each workflow, with asynchronous deletion in mind
- Optional cross-AZ redundancy -- most users only run a single node validator especially when the validation is tied to the node identity (node ID, staking TLS certificates)

## AWS zonal resources and failover

EBS volume is zone-specific thus not to be reused in other zones: When a node becomes unhealthy (e.g., availability zone outages), ASG will launch a new instance but won't be able to reuse the EBS volume if launched in a different availability zone. Due to the complexity of maintaining cross-zonal resources (e.g., staking TLS certificates), we decide not to implement automatic zonal failover. The TLS certificates will be tied to the zone-specific EBS volume, but uploaded to S3 bucket with disaster discovery tools -- `avalanched` should accept the S3 keys to reuse the certificates created in other availability zones.

The data in the EBS volume are interchangeable across different node entities, and the volumes may remain unused if there is no remaining EC2 instance in the zone, but not to be deleted unless the user deletes the whole cluster. The worst thing that can happen is wasted resources and AWS bills: A user creates the cluster with the size `N` and permanently scales down to `N - 5` -- then the `5` number of EBS volumes remain unused, which can be cleaned by another tool.

Meanwhile the staking TLS certificates can be uploaded to S3 for cross-zonal access, and (once created) must be used at all times, so long as the ASG desired capacity remains the same (or increases): In case of EC2 instance replacement, the TLS certificates must be reused by the upcoming new EC2 instance. Which means the provisioner must ensure the uniqueness of the TLS certificates and track its availability in real time: If a staking certificate is being (or about to be) used by a bootstrapping node, it must not be reused in other nodes.

## Provision logic

1. Starts a daemon on an EC2 instance launch
2. Use `aws-volume-provisioner` to find if the local EC2 instance has any EBS volume attached
  - If no attached volume is found, create an EBS volume and attach it to the instance
  - If a volume is found, attach it to the instance
3. Use `aws-volume-mounter` to mount the EBS volume to the local instance
4. Generate or load the existing staking certificate from the mounted data volume

## FAQ: What about ENI? What about IPv6?

Reserving a static ENI with a dedicated IP for each node will be helpful for peer status monitoring. However, similar to EBS, the ENI is a zonal resource which cannot be reused if a new EC2 instance is launched in a different availability zone. The same applies to IPv6 as it is bound to a specific subnet range. This adds operational complexity and resource overheads in addition to the EBS volumes. We can revisit when AWS supports elastic IPs in IPv6.

## FAQ: What about EIP?

Elastic IP (IPv4) can be used to reserve a static IP for each node. And indeed this is the most viable option on AWS. However, we are looking to scale the Avalanche network to millions of machines. Thus we want to avoid the use of limited IPv4 address space if possible. This also adds operational complexity and incurs extra AWS bills. Given that changing node IPs does not have any impact on the new upcoming Avalanche nodes, we choose not to enforce static IPs on the nodes.

## FAQ: What about Kubernetes? What about CSI EBS plugin?

Indeed, Kubernetes/EKS with CSI plugin can do all of this. However, the goal of this project is to operate a node in most affordable way. We want to achieve the same with more efficiency (and less spending).
