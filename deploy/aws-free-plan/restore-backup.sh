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
cleanup() { rm -rf -- "$WORK_DIR"; }
trap cleanup EXIT

tar -tzf "$ARCHIVE" > "$WORK_DIR/contents.txt"
for required in dregg-data/node.key dregg-data/genesis.json; do
  if ! grep -Fxq "$required" "$WORK_DIR/contents.txt"; then
    echo "backup is missing $required" >&2
    exit 1
  fi
done
if grep -Eq '(^|/)\.\.?(/|$)' "$WORK_DIR/contents.txt"; then
  echo "backup contains an unsafe path" >&2
  exit 1
fi

tar -xzf "$ARCHIVE" -C "$WORK_DIR"
systemctl stop dregg-solo.service
if [[ -d /opt/dregg-data ]]; then
  mv /opt/dregg-data "/opt/dregg-data.before-restore-${STAMP}"
fi
mv "$WORK_DIR/dregg-data" /opt/dregg-data
chown -R dregg:dregg /opt/dregg-data
chmod 0700 /opt/dregg-data
chmod 0600 /opt/dregg-data/node.key
systemctl start dregg-solo.service
echo "restored backup; previous data remains at /opt/dregg-data.before-restore-${STAMP}"
