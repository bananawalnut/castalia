# Castalia Dregg on the AWS Free Account Plan

This package deploys one Castalia-operated, verified-Lean Dregg bootstrap node
at `dregg.zenith-research.ca`. It is a durable committee of one: it has no
Byzantine fault tolerance, quorum independence, or automatic failover.

## Non-negotiable cost boundary

Before running any command, the AWS account console must explicitly show
**Free account plan**. Do not upgrade to the Paid plan. The deployment script
requires `CASTALIA_AWS_FREE_PLAN_CONFIRMED=YES`; that flag records the operator's
console check but cannot replace it.

The default is `t3.small` with standard CPU credits, a 20 GB encrypted gp3
volume, one Elastic IP, and no NAT gateway, load balancer, database, detailed
monitoring, or paid log service. Delete the complete stack before the Free Plan
ends. Migrate at day 120 or 25% credits remaining, whichever happens first.

## 1. Provision the isolated host

Configure AWS CLI credentials for the intended account. Supply an existing EC2
key pair and the operator's exact public IPv4 CIDR:

```sh
export AWS_REGION=us-west-2
export CASTALIA_AWS_FREE_PLAN_CONFIRMED=YES
deploy/aws-free-plan/deploy-stack.sh KEY_NAME OPERATOR_IP/32
```

The stack exposes only SSH from that CIDR and public ports 80/443. Dregg ports
8420 and 9420 are never in the security group. Wait for host setup:

```sh
ssh ubuntu@PUBLIC_IP cloud-init status --wait
```

Create the DNS A record only after the stack output is known:

```text
dregg.zenith-research.ca A PUBLIC_IP
```

## 2. Download, verify, and upload the protected CI artifact

Use the successful `Castalia bootstrap node` workflow run for the exact commit.
Download the `castalia-bootstrap-node-linux-x86_64` artifact. It must contain:

- `dregg-node`
- `dregg-node.sha256`
- `provenance.json`
- `dregg-node.spdx.json`

Do not upload source and do not compile on EC2. Copy the release bundle and this
deployment package to the host, then install:

```sh
sudo deploy/aws-free-plan/install.sh \
  dregg.zenith-research.ca \
  ./dregg-node \
  ./dregg-node.sha256 \
  ./provenance.json \
  ./dregg-node.spdx.json \
  DEPLOYED_COMMIT_40_HEX
```

The installer independently checks the binary checksum, Lean-producer
provenance, revision, and SPDX document before placing the binary in a
content-addressed release directory. Upgrades refuse to proceed until
`CASTALIA_ENCRYPTED_BACKUP_CONFIRMED=YES` records that an off-machine backup was
decrypted and inspected.

On first install the verified binary creates a production committee-of-one
genesis bound to its new node key. That genesis contains only the issuer well,
fee well, and a funded private relay cell used to sponsor membership births. It
contains no faucet, demo identities, demo balances, `.devnet` marker, or
Starbridge seed cells.

## 3. Preflight and 30-minute acceptance soak

Run on the node:

```sh
sudo deploy/aws-free-plan/preflight.sh dregg.zenith-research.ca
```

Preflight requires a healthy solo consensus service, the Lean state producer,
matching public/loopback identity, 25% memory headroom, private port 8420, and
404 responses from every unreviewed public route. The malformed Join probe must
fail closed with HTTP 400.

Use the production Wallet package to create the owner membership. For the
machine-enforced soak, capture that exact signed v2 join request from the
acceptance run (it contains only the public key and signature, never the private
key or passphrase), copy it to the node, and run:

```sh
sudo deploy/aws-free-plan/soak-membership.sh \
  dregg.zenith-research.ca \
  ./signed-v2-join-request.json \
  1800
```

The gate issues or reacquires the membership, proves retries return the same
cell with `created: false`, reads the same public cell every ten seconds,
requires at least 25% available memory, restarts `dregg-solo.service` halfway
through, rechecks the membership, and runs preflight again. It writes a JSON
soak record in the current directory. Set
`CASTALIA_SOAK_EXPECT_FIRST_CREATED=true` when this is the first-ever issuance,
or `false` when deliberately repeating acceptance for an existing key; the
default accepts either initial state but still requires every retry to be
idempotent.

If `t3.small` cannot keep at least 25% memory available or remains unstable,
redeploy with:

```sh
export CASTALIA_INSTANCE_TYPE=c7i-flex.large
deploy/aws-free-plan/deploy-stack.sh KEY_NAME OPERATOR_IP/32
```

Repeat the entire soak after replacement.

## 4. Backup, restore, and rollback gate

Generate an age identity only on the operator machine and send the public
`age1...` recipient to the node. Create a consistent encrypted backup:

```sh
sudo deploy/aws-free-plan/create-encrypted-backup.sh \
  age1PUBLIC_RECIPIENT \
  /home/ubuntu/dregg-backups
```

Copy both the `.age` file and `.sha256` file off EC2. Verify the checksum,
decrypt it off-machine, inspect that it contains `node.key`, `genesis.json`,
`issuer-well.key`, `fee-well.key`, and the durable ledger, then test restoration
with `restore-backup.sh`. The restore script rejects unsafe archive entries and
keeps the replaced data directory as a timestamped rollback copy.

Binary rollback is ledger-preserving:

```sh
sudo deploy/aws-free-plan/rollback-binary.sh PREVIOUS_BINARY_SHA256
```

After the clean-profile and recovery checks pass, fill in
[`ACCEPTANCE-RECORD.md`](ACCEPTANCE-RECORD.md), including hashes for the release,
wallet package, soak record, and decrypt-tested off-machine backup.

## Public boundary

Caddy exposes only:

- `POST /api/castalia/memberships`
- `GET /api/cell/*`
- `GET /status`

No faucet, Control route, operator API, explorer, gossip port, or build tool is
part of the production host. Never set `DREGG_ALLOW_UNVERIFIED_CONSENSUS=1` or
`DREGG_LEAN_PRODUCER=0`.

## Free-plan exit

Follow [OCI migration](MIGRATE-TO-OCI.md) at day 120 or when 25% of AWS credits
remain. Keep `dregg.zenith-research.ca` unchanged, verify the restored membership
on OCI, and then delete the CloudFormation stack, Elastic IP, and EBS volume.
