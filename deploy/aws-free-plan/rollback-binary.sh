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
if [[ ! -L /opt/dregg/bin/dregg-node ]]; then
  echo "current release symlink is unavailable" >&2
  exit 1
fi
CURRENT_TARGET="$(readlink -f /opt/dregg/bin/dregg-node)"

systemctl stop dregg-solo.service
ln -sfn "$TARGET" /opt/dregg/bin/dregg-node.rollback
mv -Tf /opt/dregg/bin/dregg-node.rollback /opt/dregg/bin/dregg-node
systemctl start dregg-solo.service
for _ in $(seq 1 45); do
  if STATUS="$(curl -fsS http://127.0.0.1:8420/status)" \
    && jq -e '.healthy == true and .state_producer == "lean"' <<<"$STATUS" >/dev/null; then
    echo "activated previous verified release $SHA without modifying the ledger"
    exit 0
  fi
  sleep 2
done

systemctl stop dregg-solo.service
ln -sfn "$CURRENT_TARGET" /opt/dregg/bin/dregg-node.rollback-failed
mv -Tf /opt/dregg/bin/dregg-node.rollback-failed /opt/dregg/bin/dregg-node
systemctl start dregg-solo.service
echo "rollback target failed health checks; restored $CURRENT_TARGET" >&2
exit 1
