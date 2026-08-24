#!/usr/bin/env bash
# Install an already-built, Lean-linked dregg-node on an OCI Ubuntu host.
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "run with sudo: sudo $0 <public-hostname> <verified-dregg-node-binary>" >&2
  exit 2
fi

if [[ "$#" -ne 2 ]]; then
  echo "usage: sudo $0 <public-hostname> <verified-dregg-node-binary>" >&2
  exit 2
fi

DREGG_HOSTNAME="$1"
BINARY_PATH="$2"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [[ ! "$DREGG_HOSTNAME" =~ ^[A-Za-z0-9.-]+$ ]]; then
  echo "invalid public hostname: $DREGG_HOSTNAME" >&2
  exit 2
fi

if [[ ! -f "$BINARY_PATH" || ! -x "$BINARY_PATH" ]]; then
  echo "verified dregg-node binary is missing or not executable: $BINARY_PATH" >&2
  exit 2
fi

if ! command -v caddy >/dev/null; then
  echo "caddy is not installed; wait for cloud-init or install the Ubuntu caddy package" >&2
  exit 1
fi

if ! id dregg >/dev/null 2>&1; then
  useradd --system --no-create-home --shell /usr/sbin/nologin dregg
fi

install -d -m 0755 -o root -g root /opt/dregg /opt/dregg/bin
install -d -m 0700 -o dregg -g dregg /opt/dregg-data
install -d -m 0755 -o root -g dregg /etc/dregg
install -d -m 0750 -o caddy -g caddy /var/log/caddy

systemctl stop dregg-solo.service 2>/dev/null || true
install -m 0755 -o root -g root "$BINARY_PATH" /opt/dregg/bin/dregg-node

if [[ ! -f /opt/dregg-data/node.key ]]; then
  runuser -u dregg -- /opt/dregg/bin/dregg-node init --data-dir /opt/dregg-data
fi
chown -R dregg:dregg /opt/dregg-data
chmod 0700 /opt/dregg-data
chmod 0600 /opt/dregg-data/node.key

install -m 0644 -o root -g root "$SCRIPT_DIR/dregg-solo.service" /etc/systemd/system/dregg-solo.service
install -m 0644 -o root -g root "$SCRIPT_DIR/caddy/Caddyfile" /etc/caddy/Caddyfile
printf 'DREGG_HOSTNAME=%s\n' "$DREGG_HOSTNAME" > /etc/dregg/public.env
chmod 0644 /etc/dregg/public.env

install -d -m 0755 -o root -g root /etc/systemd/system/caddy.service.d
cat > /etc/systemd/system/caddy.service.d/dregg-env.conf <<'EOF'
[Service]
EnvironmentFile=/etc/dregg/public.env
EOF

env DREGG_HOSTNAME="$DREGG_HOSTNAME" caddy validate --config /etc/caddy/Caddyfile
systemctl daemon-reload
systemctl enable --now dregg-solo.service
systemctl enable caddy.service
systemctl restart caddy.service

for attempt in $(seq 1 900); do
  if STATUS="$(curl -fsS "https://${DREGG_HOSTNAME}/status" 2>/dev/null)" \
    && jq -e '
      .federation_mode == "solo" and
      .state_producer == "lean" and
      .lean_producer == true and
      .healthy == true and
      .consensus_live == true
    ' <<<"$STATUS" >/dev/null; then
    echo "Castalia Dregg is live at https://${DREGG_HOSTNAME}"
    exit 0
  fi
  if ! systemctl is-active --quiet dregg-solo.service; then
    systemctl --no-pager --full status dregg-solo.service || true
    journalctl --no-pager -u dregg-solo.service -n 80 || true
    echo "verified Dregg service exited before readiness" >&2
    exit 1
  fi
  if (( attempt % 30 == 0 )); then
    echo "waiting for verified node readiness (${attempt}s / 900s)"
    journalctl --no-pager -u dregg-solo.service -n 20 || true
  fi
  sleep 1
done

echo "services were installed, but the verified public health check did not become ready within 900 seconds" >&2
echo "inspect: systemctl status dregg-solo caddy" >&2
exit 1
