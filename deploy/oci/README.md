# Castalia Dregg on OCI Always Free

This package deploys one Castalia-operated Dregg bootstrap node on Oracle
Cloud Infrastructure. It is a durable ledger, but it is a committee of one:
there is no quorum, Byzantine fault tolerance, or automatic failover until
independent nodes join.

## Why OCI is the zero-cash target

Oracle's current Always Free documentation gives an Always Free tenancy a
total of **2 OCPUs and 12 GB RAM** on the Arm-based
`VM.Standard.A1.Flex` shape. That is the only zero-cash hosted option under
consideration with enough memory for Dregg. The release workflow therefore
produces and runtime-verifies an AArch64 artifact in GitHub Actions; the OCI
host never compiles source.

The tradeoffs are material:

- A1 capacity can be unavailable in the tenancy's home region.
- Oracle may reclaim an instance it classifies as idle over a seven-day
  window.
- One node is not fault tolerant, so an encrypted off-machine recovery copy is
  a launch requirement.
- A self-hosted machine avoids cloud capacity limits but inherits local power,
  network, and physical-host failure domains.
- 1984 Hosting is a reasonable paid fallback, but it is not free. Use at least
  its 8 GB VPS class and the separately verified x86-64 release artifact.

Self-hosting can run the same x86-64 release bundle and the hardened systemd /
Caddy boundary in `deploy/aws-free-plan`, but it is production-suitable only
when the machine has at least 8 GB dedicated RAM, reliable power, a stable
public route for 80/443, and no competing build workload. The current shared
development host does not meet that isolation requirement, so it is a recovery
target rather than the primary node.

## Zero-cost resource boundary

Before creating anything, confirm every selected resource is marked
**Always Free eligible** and the cost estimate is `$0`:

- home region only;
- `VM.Standard.A1.Flex`, 2 OCPUs, 12 GB RAM;
- Ubuntu 24.04 AArch64 image;
- one boot volume, no more than the tenancy's remaining Always Free block
  storage allowance;
- one directly assigned public IP and no paid load balancer;
- no NAT gateway, database, paid monitoring, or paid backup service.

Do not upgrade the account merely to obtain A1 capacity. Configure a `$1`
budget alert where available. If the account or requested resources are not
shown as Always Free eligible, abort rather than provision a paid substitute.

## 1. Create the host

Create a public VCN/subnet with an internet gateway. Apply
`deploy/oci/cloud-init.yaml` as cloud-init/user-data. Restrict the OCI network
security list or NSG to:

| Protocol | Port | Source | Purpose |
|---|---:|---|---|
| TCP | 22 | operator CIDR only | SSH |
| TCP | 80 | `0.0.0.0/0` | ACME HTTP validation |
| TCP | 443 | `0.0.0.0/0` | Wallet HTTPS API |

Do not open TCP 8420 or UDP 9420. Dregg binds 8420 to loopback, and a solo node
has no gossip peer. Assign the instance's public address to
`dregg.zenith-research.ca`, preferably using a persistent public-IP object if
the account's cost estimate remains `$0`.

Wait for preparation:

```bash
cloud-init status --wait
uname -m
```

`uname -m` must print `aarch64`.

## 2. Download the protected ARM64 release

Download these four assets from the immutable Castalia bootstrap release:

```text
dregg-node-linux-aarch64
dregg-node-linux-aarch64.sha256
provenance-linux-aarch64.json
dregg-node-linux-aarch64.spdx.json
```

Record the release tag and its exact 40-character Git revision. Do not upload
a local build, source checkout, or artifact from an unprotected workflow.

Upload this deployment package and the four release files to the host. Then
install them as one fail-closed set:

```bash
sudo deploy/oci/install.sh \
  dregg.zenith-research.ca \
  ./dregg-node-linux-aarch64 \
  ./dregg-node-linux-aarch64.sha256 \
  ./provenance-linux-aarch64.json \
  ./dregg-node-linux-aarch64.spdx.json \
  REPLACE_WITH_EXACT_40_HEX_RELEASE_REVISION
```

The installer rejects the wrong architecture, digest, target, revision,
producer, federation mode, or SBOM. It stores binaries by SHA-256, switches the
live symlink atomically, and rolls back to the previous verified binary if the
new version fails readiness.

The service intentionally:

- uses a federation size of one and `--federation-mode solo`;
- exposes the node only through Caddy on HTTPS;
- exposes only `/api/castalia/memberships`, `/api/cell/*`, and `/status`;
- omits faucet, Control, gossip, and automatic operator joins;
- persists identity, genesis, wells, and the redb ledger in
  `/opt/dregg-data`;
- requires the verified Lean state producer at runtime.

Verify the installed boundary:

```bash
sudo deploy/oci/preflight.sh dregg.zenith-research.ca
```

## 3. Create and decrypt-test the off-machine backup

Generate an age identity on the operator machine. Never upload its private
identity to OCI:

```bash
mkdir -p ~/.config/castalia
age-keygen -o ~/.config/castalia/dregg-backup.agekey
age-keygen -y ~/.config/castalia/dregg-backup.agekey
```

Pass only the printed `age1...` recipient to the host:

```bash
sudo deploy/oci/create-encrypted-backup.sh \
  age1REPLACE_WITH_PUBLIC_RECIPIENT \
  /home/ubuntu/dregg-backups
```

Copy the `.age` file and `.sha256` off the VM. Verify the checksum, decrypt it
on the operator machine, and inspect that it contains the complete
`dregg-data` directory, including `node.key`, `genesis.json`, both well keys,
and the redb ledger. Do this before onboarding the first production member.

Before every binary upgrade, repeat the backup and decrypt test. Set
`CASTALIA_ENCRYPTED_BACKUP_CONFIRMED=YES` only for the installation command
after that check; upgrades otherwise fail closed.

## 4. Acceptance and cutover

Run the 30-minute issuance/retry/restart soak with a Wallet-produced signed v2
request. The script fails if memory headroom drops below 25%:

```bash
sudo deploy/oci/soak-membership.sh \
  dregg.zenith-research.ca \
  ./signed-v2-join-request.json \
  1800
```

Then package the Wallet with only the production endpoint:

```bash
npm run package:production-extension -- https://dregg.zenith-research.ca
```

In a clean Chrome profile, complete the owner acceptance checklist for wallet
creation, `.castalia-recovery` export/import, issuance, idempotent retry,
lock/unlock persistence, node restart, and backup restoration. The local
development membership does not migrate to this new ledger; the same Member
Key deterministically reacquires one membership on the production federation.

## Operations

```bash
sudo systemctl status dregg-solo caddy
sudo journalctl -u dregg-solo -n 100 --no-pager
curl -fsS https://dregg.zenith-research.ca/status | jq
sudo deploy/oci/rollback-binary.sh PREVIOUS_RELEASE_SHA256
sudo deploy/oci/restore-backup.sh ./decrypted-castalia-dregg.tar.gz
```

Never set `DREGG_ALLOW_UNVERIFIED_CONSENSUS=1` or
`DREGG_LEAN_PRODUCER=0` on this host.

## Provider references

- [OCI Always Free resources](https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm)
- [OCI Free Tier lifecycle](https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier.htm)
- [OCI public IP address types](https://docs.oracle.com/en-us/iaas/Content/Network/Tasks/managingpublicIPs.htm)
- [1984 Hosting price list](https://1984.hosting/product/pricelist/?l=en)
