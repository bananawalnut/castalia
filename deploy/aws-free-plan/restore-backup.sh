#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 || "$#" -ne 1 ]]; then
  echo "usage: sudo $0 <decrypted-castalia-dregg-tar.gz>" >&2
  exit 2
fi
ARCHIVE="$1"
if [[ ! -f "$ARCHIVE" ]]; then
  echo "backup archive not found: $ARCHIVE" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d /opt/castalia-restore.XXXXXX)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
# Invoked by the EXIT trap below.
# shellcheck disable=SC2329
cleanup() { rm -rf -- "$WORK_DIR"; }
trap cleanup EXIT

tar -tzf "$ARCHIVE" > "$WORK_DIR/contents.txt"
tar -tvzf "$ARCHIVE" > "$WORK_DIR/verbose-contents.txt"
for required in \
  dregg-data/node.key \
  dregg-data/genesis.json \
  dregg-data/dregg.redb \
  dregg-data/issuer-well.key \
  dregg-data/fee-well.key; do
  if ! grep -Fxq "$required" "$WORK_DIR/contents.txt"; then
    echo "backup is missing $required" >&2
    exit 1
  fi
done
if grep -Eq '(^/|(^|/)\.\.?(/|$))' "$WORK_DIR/contents.txt" \
  || grep -Evq '^dregg-data(/|$)' "$WORK_DIR/contents.txt"; then
  echo "backup contains an unsafe path" >&2
  exit 1
fi
if awk 'substr($1, 1, 1) != "d" && substr($1, 1, 1) != "-" { exit 1 }' \
  "$WORK_DIR/verbose-contents.txt"; then
  :
else
  echo "backup contains a link, device, or other unsupported entry" >&2
  exit 1
fi

tar --no-same-owner --no-same-permissions -xzf "$ARCHIVE" -C "$WORK_DIR"
if [[ -L "$WORK_DIR/dregg-data" || ! -d "$WORK_DIR/dregg-data" ]]; then
  echo "backup data root is not a regular directory" >&2
  exit 1
fi
systemctl stop dregg-solo.service
PREVIOUS_DATA=""
if [[ -d /opt/dregg-data ]]; then
  PREVIOUS_DATA="/opt/dregg-data.before-restore-${STAMP}"
  mv /opt/dregg-data "$PREVIOUS_DATA"
fi
mv "$WORK_DIR/dregg-data" /opt/dregg-data
chown -R dregg:dregg /opt/dregg-data
chmod 0700 /opt/dregg-data
chmod 0600 \
  /opt/dregg-data/node.key \
  /opt/dregg-data/issuer-well.key \
  /opt/dregg-data/fee-well.key
if systemctl start dregg-solo.service; then
  echo "restored backup; previous data remains at ${PREVIOUS_DATA:-none}"
  exit 0
fi

FAILED_DATA="/opt/dregg-data.failed-restore-${STAMP}"
mv /opt/dregg-data "$FAILED_DATA"
if [[ -n "$PREVIOUS_DATA" && -d "$PREVIOUS_DATA" ]]; then
  mv "$PREVIOUS_DATA" /opt/dregg-data
  systemctl start dregg-solo.service
fi
echo "restored data failed to start and was moved to $FAILED_DATA; previous data was reactivated" >&2
exit 1
