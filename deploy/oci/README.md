# Castalia Dregg on OCI Always Free

This deploys one Castalia-operated Dregg node as a **bootstrap network**. It is
a real durable ledger, but it is a committee of one: there is no quorum,
Byzantine fault tolerance, or automatic failover until more independent nodes
join.

## Zero-cost guardrails

Keep the Oracle account on Free Tier and create only resources carrying the
**Always Free eligible** label:

- shape: `VM.Standard.A1.Flex` (Arm/AArch64);
- size: 2 OCPUs and 12 GB RAM;
- image: Ubuntu 24.04 AArch64;
- boot volume: 100 GB, in the tenancy home region;
- one ephemeral public IPv4 address;
- no paid load balancer, database, NAT gateway, reserved public IP, or extra
  logging retention.

Set an OCI budget alert at `$1` if the console permits it, but do not upgrade
the account to Pay As You Go. OCI may reclaim an idle Always Free VM, so the
encrypted off-machine backup below is part of launch—not a later improvement.

## 1. Create the VM

Use `deploy/oci/cloud-init.yaml` as the instance cloud-init/user-data. Create a
VCN with an internet gateway and allow inbound:

| Protocol | Port | Source | Purpose |
|---|---:|---|---|
| TCP | 22 | the operator's current IP | SSH |
| TCP | 80 | `0.0.0.0/0` | ACME HTTP validation |
| TCP | 443 | `0.0.0.0/0` | Wallet HTTPS API |

Do **not** open TCP 8420 or UDP 9420. Dregg binds 8420 to loopback, and a solo
node has no gossip peer.

Wait for host preparation after the first SSH login:

```bash
cloud-init status --wait
```

Install the repository-pinned Rust and Lean toolchain managers as the `ubuntu`
user. Reconnect after each installer if its PATH update is not immediately
visible:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
curl https://elan.lean-lang.org/elan-init.sh -sSf | sh -s -- -y
source "$HOME/.cargo/env"
export PATH="$HOME/.elan/bin:$PATH"
```

## 2. Upload the exact working source

The permissionless membership implementation may not yet exist on a published
branch, so deploy the exact reviewed worktree rather than silently cloning
another commit.

On the VM:

```bash
sudo chown ubuntu:ubuntu /opt/dregg-src
```

From the development machine:

```bash
rsync -az --delete \
  --exclude .git \
  --exclude target \
  --exclude '*/target' \
  --exclude 'metatheory/.lake' \
  --exclude 'dregg-lean-ffi/libdregg_lean.a' \
  ./ ubuntu@PUBLIC_IP:/opt/dregg-src/
```

The existing macOS Lean archive must not be copied into a Linux/AArch64 build.

## 3. Build the verified node before it becomes live

Run this while the machine is still only a build host:

```bash
cd /opt/dregg-src
./scripts/bootstrap.sh
DREGG_REQUIRE_LEAN=1 cargo build --locked --release -p dregg-node
```

The first Lean bootstrap is slow on two OCPUs. It is nevertheless essential:
`DREGG_REQUIRE_LEAN=1` makes a marshal-only build fail instead of producing a
degraded binary.

Record the exact artifact. The uploaded worktree intentionally excludes `.git`,
so the local source revision and dirty diff must be recorded separately in the
deployment log:

```bash
sha256sum target/release/dregg-node
```

## 4. Install with free HTTPS

A real domain is not required for the bootstrap. If the instance IP is
`203.0.113.10`, an IP-derived hostname such as
`203.0.113.10.sslip.io` resolves to it without a DNS account.

```bash
cd /opt/dregg-src
sudo deploy/oci/install.sh \
  PUBLIC_IP.sslip.io \
  /opt/dregg-src/target/release/dregg-node
```

The service intentionally:

- runs `--federation-mode solo` with a committee size of one;
- keeps the node HTTP port on `127.0.0.1:8420`;
- omits `--enable-faucet` and automatic operator joins;
- retains archival history while the bootstrap ledger is small;
- leaves the verified Lean producer enabled;
- exposes only membership creation, membership-cell reads, and `/status`;
- persists identity, genesis, and the full redb ledger in
  `/opt/dregg-data`.

Verify it:

```bash
deploy/oci/preflight.sh PUBLIC_IP.sslip.io
```

## 5. Make the first encrypted off-machine backup

Generate an `age` identity on the operator machine and never upload the private
identity to OCI:

```bash
mkdir -p ~/.config/castalia
age-keygen -o ~/.config/castalia/dregg-backup.agekey
age-keygen -y ~/.config/castalia/dregg-backup.agekey
```

Pass only the printed `age1...` recipient to the VM:

```bash
sudo deploy/oci/create-encrypted-backup.sh \
  age1REPLACE_WITH_PUBLIC_RECIPIENT \
  /home/ubuntu/dregg-backups
```

Copy both the `.age` file and its `.sha256` file off the VM. The backup briefly
stops Dregg so the redb database, `node.key`, and `genesis.json` form one
consistent recovery point. Test decryption and restoration before onboarding a
real member.

## 6. Wallet cutover and acceptance

The Wallet currently defaults to the local development node. Before building
the production extension:

1. in the Wallet worktree, run
   `npm run package:production-extension -- https://PUBLIC_IP.sslip.io`;
2. load `build/castalia-wallet-production` as an unpacked extension in Chrome;
   the packager pins the endpoint and grants only that HTTPS host permission;
3. export and import the local Wallet recovery file first if the existing
   Member Key must be retained;
4. join once and verify `Membership active`;
5. restart the OCI VM and verify the same membership cell remains active;
6. export a Wallet recovery file, clear/reload the Wallet, import it, and verify
   the same Member Key and membership resolve.

The membership created on the local development ledger does not migrate to the
OCI ledger. The production membership is created once on this new federation.

## Operations

```bash
sudo systemctl status dregg-solo caddy
sudo journalctl -u dregg-solo -n 100 --no-pager
curl -fsS https://PUBLIC_IP.sslip.io/status | jq
```

Never set `DREGG_ALLOW_UNVERIFIED_CONSENSUS=1` or
`DREGG_LEAN_PRODUCER=0` on this host.
