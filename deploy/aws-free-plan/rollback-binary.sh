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
for attempt in $(seq 1 900); do
  if STATUS="$(curl -fsS http://127.0.0.1:8420/status 2>/dev/null)" \
    && jq -e '
      .federation_mode == "solo" and
      .state_producer == "lean" and
      .lean_producer == true and
      .healthy == true and
      .consensus_live == true
    ' <<<"$STATUS" >/dev/null; then
    echo "activated previous verified release $SHA without modifying the ledger"
    exit 0
  fi
  if ! systemctl is-active --quiet dregg-solo.service; then
    systemctl --no-pager --full status dregg-solo.service || true
    journalctl --no-pager -u dregg-solo.service -n 80 || true
    echo "rollback target exited before readiness" >&2
    break
  fi
  if (( attempt % 30 == 0 )); then
    echo "waiting for verified rollback readiness (${attempt}s / 900s)"
    journalctl --no-pager -u dregg-solo.service -n 20 || true
  fi
  sleep 1
done

systemctl stop dregg-solo.service
ln -sfn "$CURRENT_TARGET" /opt/dregg/bin/dregg-node.rollback-failed
mv -Tf /opt/dregg/bin/dregg-node.rollback-failed /opt/dregg/bin/dregg-node
systemctl start dregg-solo.service
echo "rollback target failed health checks; restored $CURRENT_TARGET" >&2
exit 1
