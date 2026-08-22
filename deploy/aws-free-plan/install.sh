#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "run with sudo" >&2
  exit 2
fi
if [[ "$#" -ne 6 ]]; then
  echo "usage: sudo $0 <hostname> <binary> <sha256-file> <provenance-json> <spdx-sbom-json> <expected-git-revision>" >&2
  exit 2
fi

DREGG_HOSTNAME="$1"
BINARY_PATH="$2"
CHECKSUM_PATH="$3"
PROVENANCE_PATH="$4"
SBOM_PATH="$5"
EXPECTED_REVISION="$6"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [[ ! "$DREGG_HOSTNAME" =~ ^[A-Za-z0-9.-]+$ ]]; then
  echo "invalid public hostname: $DREGG_HOSTNAME" >&2
  exit 2
fi
if [[ ! "$EXPECTED_REVISION" =~ ^[0-9a-f]{40}$ ]]; then
  echo "expected Git revision must be exactly 40 lowercase hexadecimal characters" >&2
  exit 2
fi
for path in "$BINARY_PATH" "$CHECKSUM_PATH" "$PROVENANCE_PATH" "$SBOM_PATH"; do
  if [[ ! -f "$path" ]]; then
    echo "release artifact is missing: $path" >&2
    exit 2
  fi
done

EXPECTED_SHA="$(awk 'NR == 1 { print $1 }' "$CHECKSUM_PATH")"
if [[ ! "$EXPECTED_SHA" =~ ^[0-9a-f]{64}$ ]]; then
  echo "checksum file does not begin with a lowercase SHA-256 digest" >&2
  exit 2
fi
ACTUAL_SHA="$(sha256sum "$BINARY_PATH" | awk '{ print $1 }')"
if [[ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]]; then
  echo "binary checksum mismatch" >&2
  exit 1
fi
jq -e --arg sha "$EXPECTED_SHA" --arg revision "$EXPECTED_REVISION" \
  '.schemaVersion == 1 and
   .artifact == "castalia-bootstrap-node-linux-x86_64" and
   .binary.name == "dregg-node" and
   .binary.sha256 == $sha and
   .stateProducer == "lean" and
   .leanProducer == true and
   .federationMode == "solo" and
   .build.requireLean == true and
   .build.target == "x86_64-unknown-linux-gnu" and
   .revision == $revision' \
  "$PROVENANCE_PATH" >/dev/null
jq -e '.spdxVersion == "SPDX-2.3" and (.packages | length) > 0' "$SBOM_PATH" >/dev/null
SBOM_SHA="$(sha256sum "$SBOM_PATH" | awk '{ print $1 }')"
if [[ "$(jq -r '.sbom.sha256 // empty' "$PROVENANCE_PATH")" != "$SBOM_SHA" ]]; then
  echo "SBOM checksum does not match release provenance" >&2
  exit 1
fi

if ! id dregg >/dev/null 2>&1; then
  useradd --system --no-create-home --shell /usr/sbin/nologin dregg
fi
install -d -m 0755 -o root -g root /opt/dregg /opt/dregg/bin /opt/dregg/releases
install -d -m 0755 -o root -g dregg /etc/dregg
install -d -m 0750 -o caddy -g caddy /var/log/caddy

PREVIOUS_TARGET=""
if [[ -L /opt/dregg/bin/dregg-node ]]; then
  PREVIOUS_TARGET="$(readlink -f /opt/dregg/bin/dregg-node)"
fi
if [[ -n "$PREVIOUS_TARGET" && "${CASTALIA_ENCRYPTED_BACKUP_CONFIRMED:-}" != "YES" ]]; then
  echo "Refusing upgrade without a decrypt-tested, off-machine encrypted backup." >&2
  echo "Set CASTALIA_ENCRYPTED_BACKUP_CONFIRMED=YES only after completing that backup." >&2
  exit 2
fi

RELEASE_DIR="/opt/dregg/releases/$EXPECTED_SHA"
install -d -m 0755 -o root -g root "$RELEASE_DIR"
install -m 0755 -o root -g root "$BINARY_PATH" "$RELEASE_DIR/dregg-node"
install -m 0644 -o root -g root "$CHECKSUM_PATH" "$RELEASE_DIR/dregg-node.sha256"
install -m 0644 -o root -g root "$PROVENANCE_PATH" "$RELEASE_DIR/provenance.json"
install -m 0644 -o root -g root "$SBOM_PATH" "$RELEASE_DIR/dregg-node.spdx.json"

if [[ ! -e /opt/dregg-data ]]; then
  "$RELEASE_DIR/dregg-node" init --data-dir /opt/dregg-data --solo-genesis
elif [[ ! -f /opt/dregg-data/node.key ]]; then
  echo "existing data directory has no node.key; refusing to replace its identity" >&2
  exit 1
elif [[ ! -f /opt/dregg-data/genesis.json ]]; then
  if [[ -e /opt/dregg-data/dregg.redb ]]; then
    echo "existing ledger has no genesis.json; refusing to invent chain identity" >&2
    exit 1
  fi
  "$RELEASE_DIR/dregg-node" init --data-dir /opt/dregg-data --solo-genesis
fi
for required in node.key genesis.json issuer-well.key fee-well.key; do
  if [[ ! -f "/opt/dregg-data/$required" ]]; then
    echo "production solo data is missing $required" >&2
    exit 1
  fi
done
chown -R dregg:dregg /opt/dregg-data
chmod 0700 /opt/dregg-data
chmod 0600 /opt/dregg-data/node.key

# Validate the exact public route boundary before stopping a working node or
# replacing any live configuration.
env DREGG_HOSTNAME="$DREGG_HOSTNAME" caddy validate --config "$SCRIPT_DIR/caddy/Caddyfile"

if systemctl cat dregg-solo.service >/dev/null 2>&1; then
  systemctl stop dregg-solo.service
fi
ln -sfn "$RELEASE_DIR/dregg-node" /opt/dregg/bin/dregg-node.next
mv -Tf /opt/dregg/bin/dregg-node.next /opt/dregg/bin/dregg-node

install -m 0644 -o root -g root "$SCRIPT_DIR/dregg-solo.service" /etc/systemd/system/dregg-solo.service
install -m 0644 -o root -g root "$SCRIPT_DIR/caddy/Caddyfile" /etc/caddy/Caddyfile
printf 'DREGG_HOSTNAME=%s\n' "$DREGG_HOSTNAME" > /etc/dregg/public.env
chmod 0644 /etc/dregg/public.env
install -d -m 0755 -o root -g root /etc/systemd/system/caddy.service.d
install -m 0644 -o root -g root /dev/null /etc/systemd/system/caddy.service.d/dregg-env.conf
printf '[Service]\nEnvironmentFile=/etc/dregg/public.env\n' > /etc/systemd/system/caddy.service.d/dregg-env.conf

systemctl daemon-reload
systemctl enable dregg-solo.service caddy.service
START_OK=1
if ! systemctl restart dregg-solo.service; then
  echo "new Dregg release failed to start" >&2
  START_OK=0
fi
if ! systemctl restart caddy.service; then
  echo "Caddy failed to restart with the reviewed route boundary" >&2
  START_OK=0
fi

READY=0
if [[ "$START_OK" -eq 1 ]]; then
  for _ in $(seq 1 45); do
    if curl -fsS "https://${DREGG_HOSTNAME}/status" >/dev/null; then
      READY=1
      break
    fi
    sleep 2
  done
fi
if [[ "$READY" -eq 1 ]] && "$SCRIPT_DIR/preflight.sh" "$DREGG_HOSTNAME"; then
  echo "Castalia Dregg release $EXPECTED_SHA is live at https://${DREGG_HOSTNAME}"
  exit 0
fi

echo "new release failed health checks" >&2
if [[ -n "$PREVIOUS_TARGET" && -x "$PREVIOUS_TARGET" ]]; then
  ln -sfn "$PREVIOUS_TARGET" /opt/dregg/bin/dregg-node.previous
  mv -Tf /opt/dregg/bin/dregg-node.previous /opt/dregg/bin/dregg-node
  systemctl restart dregg-solo.service
  echo "rolled back to $PREVIOUS_TARGET" >&2
fi
exit 1
