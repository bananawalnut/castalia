#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 1 || "$#" -gt 2 ]]; then
  echo "usage: $0 <status-url> [timeout-seconds]" >&2
  exit 2
fi

STATUS_URL="$1"
TIMEOUT_SECONDS="${2:-900}"

if [[ ! "$STATUS_URL" =~ ^https?://[^[:space:]]+/status$ ]]; then
  echo "status URL must be an http(s) /status endpoint" >&2
  exit 2
fi
if [[ ! "$TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] \
  || (( TIMEOUT_SECONDS < 1 || TIMEOUT_SECONDS > 3600 )); then
  echo "timeout must be an integer from 1 through 3600 seconds" >&2
  exit 2
fi

for attempt in $(seq 1 "$TIMEOUT_SECONDS"); do
  if STATUS="$(curl -fsS "$STATUS_URL" 2>/dev/null)" \
    && jq -e '
      .federation_mode == "solo" and
      .state_producer == "lean" and
      .lean_producer == true and
      .healthy == true and
      .consensus_live == true
    ' <<<"$STATUS" >/dev/null; then
    printf '%s\n' "$STATUS"
    exit 0
  fi

  if ! systemctl is-active --quiet dregg-solo.service; then
    systemctl --no-pager --full status dregg-solo.service || true
    journalctl --no-pager -u dregg-solo.service -n 80 || true
    echo "verified Dregg service exited before readiness" >&2
    exit 1
  fi

  if (( attempt % 30 == 0 )); then
    echo "waiting for verified node readiness (${attempt}s / ${TIMEOUT_SECONDS}s)" >&2
    journalctl --no-pager -u dregg-solo.service -n 20 || true
  fi
  sleep 1
done

systemctl --no-pager --full status dregg-solo.service || true
journalctl --no-pager -u dregg-solo.service -n 80 || true
echo "verified Dregg service did not become ready within ${TIMEOUT_SECONDS} seconds" >&2
exit 1
