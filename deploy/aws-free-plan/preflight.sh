#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: $0 <public-hostname>" >&2
  exit 2
fi
DREGG_HOSTNAME="$1"

systemctl is-active --quiet dregg-solo.service
systemctl is-active --quiet caddy.service

SERVICE_ENV="$(systemctl show dregg-solo.service --property=Environment --value)"
if grep -Eq 'DREGG_ALLOW_UNVERIFIED_CONSENSUS=1|DREGG_LEAN_PRODUCER=0' <<<"$SERVICE_ENV"; then
  echo "unsafe development executor flags are present" >&2
  exit 1
fi

LOCAL_STATUS="$(curl -fsS http://127.0.0.1:8420/status)"
PUBLIC_STATUS="$(curl -fsS "https://${DREGG_HOSTNAME}/status")"
jq -e '.federation_mode == "solo"' <<<"$LOCAL_STATUS" >/dev/null
jq -e '.state_producer == "lean"' <<<"$LOCAL_STATUS" >/dev/null
jq -e '.healthy == true and .consensus_live == true' <<<"$LOCAL_STATUS" >/dev/null
jq -e '.healthy == true and .consensus_live == true' <<<"$PUBLIC_STATUS" >/dev/null

LOCAL_PUBLIC_KEY="$(jq -r '.public_key // empty' <<<"$LOCAL_STATUS")"
PUBLIC_PUBLIC_KEY="$(jq -r '.public_key // empty' <<<"$PUBLIC_STATUS")"
if [[ -z "$LOCAL_PUBLIC_KEY" || "$LOCAL_PUBLIC_KEY" != "$PUBLIC_PUBLIC_KEY" ]]; then
  echo "public and loopback node identities do not match" >&2
  exit 1
fi
if ss -ltnp | grep -Eq '0\.0\.0\.0:8420|\[::\]:8420'; then
  echo "port 8420 is exposed publicly" >&2
  exit 1
fi
if [[ "$(stat -c '%a' /opt/dregg-data/node.key)" != "600" ]]; then
  echo "node.key permissions are not 600" >&2
  exit 1
fi

for path in / /api/faucet /api/cells /api/federations /api/operator; do
  code="$(curl -sS -o /dev/null -w '%{http_code}' "https://${DREGG_HOSTNAME}${path}")"
  if [[ "$code" != "404" ]]; then
    echo "unreviewed public path $path returned HTTP $code instead of 404" >&2
    exit 1
  fi
done

JOIN_CODE="$(curl -sS -o /dev/null -w '%{http_code}' \
  -H 'content-type: application/json' \
  -d '{}' \
  "https://${DREGG_HOSTNAME}/api/castalia/memberships")"
if [[ "$JOIN_CODE" != "400" ]]; then
  echo "membership route did not fail closed on an invalid request (HTTP $JOIN_CODE)" >&2
  exit 1
fi

MEM_TOTAL="$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo)"
MEM_AVAILABLE="$(awk '/^MemAvailable:/ { print $2 }' /proc/meminfo)"
AVAILABLE_PERCENT="$((MEM_AVAILABLE * 100 / MEM_TOTAL))"
if (( AVAILABLE_PERCENT < 25 )); then
  echo "memory headroom is ${AVAILABLE_PERCENT}%, below the 25% acceptance floor" >&2
  exit 1
fi

echo "AWS bootstrap preflight passed with ${AVAILABLE_PERCENT}% memory available"
jq '{public_key, federation_mode, state_producer, healthy, consensus_live, dag_height, latest_height, peer_count}' <<<"$PUBLIC_STATUS"
