#!/usr/bin/env bash
# Create a consistent, encrypted snapshot of the complete one-node ledger.
# Run on the OCI node with sudo. Only the age recipient (public key) is sent to
# the server; the decryption identity remains off-machine.
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "run with sudo" >&2
  exit 2
fi

if [[ "$#" -ne 2 ]]; then
  echo "usage: sudo $0 <age-recipient> <output-directory>" >&2
  exit 2
fi

AGE_RECIPIENT="$1"
OUTPUT_DIR="$2"
CALLING_USER="${SUDO_USER:-root}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
WORK_DIR="$(mktemp -d /tmp/dregg-backup.XXXXXX)"
ARCHIVE="$WORK_DIR/dregg-${STAMP}.tar.gz"
OUTPUT_PATH="$OUTPUT_DIR/dregg-${STAMP}.tar.gz.age"
NODE_WAS_ACTIVE=0

cleanup() {
  rm -rf -- "$WORK_DIR"
  if [[ "$NODE_WAS_ACTIVE" -eq 1 ]] && ! systemctl is-active --quiet dregg-solo.service; then
    systemctl start dregg-solo.service
  fi
}
trap cleanup EXIT

if systemctl is-active --quiet dregg-solo.service; then
  NODE_WAS_ACTIVE=1
  systemctl stop dregg-solo.service
fi

tar --acls --xattrs -C /opt -czf "$ARCHIVE" dregg-data

if [[ "$NODE_WAS_ACTIVE" -eq 1 ]]; then
  systemctl start dregg-solo.service
fi

install -d -m 0700 -o "$CALLING_USER" -g "$CALLING_USER" "$OUTPUT_DIR"
age --recipient "$AGE_RECIPIENT" --output "$OUTPUT_PATH" "$ARCHIVE"
chown "$CALLING_USER:$CALLING_USER" "$OUTPUT_PATH"
chmod 0600 "$OUTPUT_PATH"
sha256sum "$OUTPUT_PATH" | tee "${OUTPUT_PATH}.sha256"
chown "$CALLING_USER:$CALLING_USER" "${OUTPUT_PATH}.sha256"

echo "$OUTPUT_PATH"
