#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 || "$#" -ne 1 ]]; then
  echo "usage: sudo $0 <previous-release-sha256>" >&2
  exit 2
fi
SHA="$1"
if [[ ! "$SHA" =~ ^[0-9a-f]{64}$ ]]; then
  echo "invalid release SHA-256" >&2
  exit 2
fi
TARGET="/opt/dregg/releases/$SHA/dregg-node"
if [[ ! -x "$TARGET" ]]; then
  echo "verified previous binary is unavailable: $TARGET" >&2
  exit 1
fi

systemctl stop dregg-solo.service
ln -sfn "$TARGET" /opt/dregg/bin/dregg-node.rollback
mv -Tf /opt/dregg/bin/dregg-node.rollback /opt/dregg/bin/dregg-node
systemctl start dregg-solo.service
echo "activated previous verified release $SHA without modifying the ledger"
