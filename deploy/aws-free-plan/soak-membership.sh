#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "run with sudo so the gate can restart dregg-solo.service" >&2
  exit 2
fi
if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
  echo "usage: sudo $0 <public-hostname> <signed-v2-join-request.json> [duration-seconds]" >&2
  exit 2
fi

DREGG_HOSTNAME="$1"
JOIN_REQUEST="$2"
DURATION_SECONDS="${3:-1800}"
EXPECT_FIRST_CREATED="${CASTALIA_SOAK_EXPECT_FIRST_CREATED:-either}"

if [[ ! "$DREGG_HOSTNAME" =~ ^[A-Za-z0-9.-]+$ ]]; then
  echo "invalid public hostname: $DREGG_HOSTNAME" >&2
  exit 2
fi
if [[ ! -f "$JOIN_REQUEST" ]]; then
  echo "signed join request does not exist: $JOIN_REQUEST" >&2
  exit 2
fi
if [[ ! "$DURATION_SECONDS" =~ ^[0-9]+$ ]] || (( DURATION_SECONDS < 60 )); then
  echo "duration must be an integer of at least 60 seconds" >&2
  exit 2
fi
case "$EXPECT_FIRST_CREATED" in
  true|false|either) ;;
  *)
    echo "CASTALIA_SOAK_EXPECT_FIRST_CREATED must be true, false, or either" >&2
    exit 2
    ;;
esac

jq -e '
  .version == 2 and
  .signatureSuite == "Ed25519" and
  (.ownerPublicKey | type == "string" and test("^[0-9a-f]{64}$")) and
  (.signature | type == "string" and length > 0)
' "$JOIN_REQUEST" >/dev/null

API_ROOT="https://${DREGG_HOSTNAME}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf -- "$WORK_DIR"' EXIT

post_join() {
  local output="$1"
  curl --fail-with-body --silent --show-error \
    -H 'content-type: application/json' \
    --data-binary "@${JOIN_REQUEST}" \
    "${API_ROOT}/api/castalia/memberships" >"$output"
}

assert_memory_headroom() {
  local total available percent
  total="$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo)"
  available="$(awk '/^MemAvailable:/ { print $2 }' /proc/meminfo)"
  percent="$((available * 100 / total))"
  if (( percent < 25 )); then
    echo "memory headroom fell to ${percent}%, below the 25% acceptance floor" >&2
    exit 1
  fi
  printf '%s' "$percent"
}

assert_cell() {
  local output="$1"
  curl --fail-with-body --silent --show-error \
    "${API_ROOT}/api/cell/${MEMBERSHIP_CELL_ID}" >"$output"
  jq -e \
    --arg id "$MEMBERSHIP_CELL_ID" \
    --arg owner "$OWNER_PUBLIC_KEY" \
    --arg commitment "$STATE_COMMITMENT" '
      .found == true and
      .id == $id and
      .publicKey == $owner and
      .stateCommitment == $commitment and
      .capabilityCount == 0 and
      .numCapabilities == 0 and
      .hasDelegate == false and
      .hasDelegation == false and
      (.capabilities | length) == 0
    ' "$output" >/dev/null
}

FIRST_RESPONSE="$WORK_DIR/first.json"
post_join "$FIRST_RESPONSE"
jq -e '
  .version == 2 and
  .state == "active" and
  .generation == 0 and
  (.membershipCellId | test("^[0-9a-f]{64}$")) and
  (.ownerPublicKey | test("^[0-9a-f]{64}$")) and
  (.factoryId | test("^[0-9a-f]{64}$")) and
  (.programId | test("^[0-9a-f]{64}$")) and
  (.stateCommitment | test("^[0-9a-f]{64}$")) and
  (.created | type == "boolean")
' "$FIRST_RESPONSE" >/dev/null
if [[ "$EXPECT_FIRST_CREATED" != "either" ]] &&
   [[ "$(jq -r '.created' "$FIRST_RESPONSE")" != "$EXPECT_FIRST_CREATED" ]]; then
  echo "first issuance created flag did not match CASTALIA_SOAK_EXPECT_FIRST_CREATED=$EXPECT_FIRST_CREATED" >&2
  exit 1
fi

MEMBERSHIP_CELL_ID="$(jq -r '.membershipCellId' "$FIRST_RESPONSE")"
OWNER_PUBLIC_KEY="$(jq -r '.ownerPublicKey' "$FIRST_RESPONSE")"
STATE_COMMITMENT="$(jq -r '.stateCommitment' "$FIRST_RESPONSE")"
if [[ "$OWNER_PUBLIC_KEY" != "$(jq -r '.ownerPublicKey' "$JOIN_REQUEST")" ]]; then
  echo "node returned a membership for a different owner" >&2
  exit 1
fi

RETRY_RESPONSE="$WORK_DIR/retry.json"
post_join "$RETRY_RESPONSE"
jq -e \
  --arg id "$MEMBERSHIP_CELL_ID" \
  --arg commitment "$STATE_COMMITMENT" '
    .created == false and
    .membershipCellId == $id and
    .stateCommitment == $commitment and
    .state == "active"
  ' "$RETRY_RESPONSE" >/dev/null
assert_cell "$WORK_DIR/cell-before-restart.json"

STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
START_EPOCH="$(date +%s)"
DEADLINE="$((START_EPOCH + DURATION_SECONDS))"
RESTART_AT="$((START_EPOCH + DURATION_SECONDS / 2))"
RESTARTED=0
CHECKS=0
LOWEST_MEMORY_PERCENT=100

while (( $(date +%s) < DEADLINE )); do
  NOW="$(date +%s)"
  if (( RESTARTED == 0 && NOW >= RESTART_AT )); then
    systemctl restart dregg-solo.service
    for _ in $(seq 1 45); do
      if curl --fail --silent --show-error "${API_ROOT}/status" >/dev/null; then
        break
      fi
      sleep 2
    done
    systemctl is-active --quiet dregg-solo.service
    RESTARTED=1
  fi

  post_join "$WORK_DIR/retry-${CHECKS}.json"
  jq -e \
    --arg id "$MEMBERSHIP_CELL_ID" \
    --arg commitment "$STATE_COMMITMENT" '
      .created == false and
      .membershipCellId == $id and
      .stateCommitment == $commitment and
      .state == "active"
    ' "$WORK_DIR/retry-${CHECKS}.json" >/dev/null
  assert_cell "$WORK_DIR/cell-${CHECKS}.json"
  MEMORY_PERCENT="$(assert_memory_headroom)"
  if (( MEMORY_PERCENT < LOWEST_MEMORY_PERCENT )); then
    LOWEST_MEMORY_PERCENT="$MEMORY_PERCENT"
  fi
  CHECKS="$((CHECKS + 1))"
  sleep 10
done

if (( RESTARTED == 0 )); then
  echo "soak finished without exercising a process restart" >&2
  exit 1
fi

post_join "$WORK_DIR/final-retry.json"
jq -e \
  --arg id "$MEMBERSHIP_CELL_ID" \
  --arg commitment "$STATE_COMMITMENT" '
    .created == false and
    .membershipCellId == $id and
    .stateCommitment == $commitment and
    .state == "active"
  ' "$WORK_DIR/final-retry.json" >/dev/null
assert_cell "$WORK_DIR/cell-after-restart.json"
"$(dirname -- "$0")/preflight.sh" "$DREGG_HOSTNAME" >/dev/null

COMPLETED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
jq -n \
  --arg startedAt "$STARTED_AT" \
  --arg completedAt "$COMPLETED_AT" \
  --arg membershipCellId "$MEMBERSHIP_CELL_ID" \
  --arg ownerPublicKey "$OWNER_PUBLIC_KEY" \
  --arg stateCommitment "$STATE_COMMITMENT" \
  --argjson durationSeconds "$DURATION_SECONDS" \
  --argjson checks "$CHECKS" \
  --argjson lowestMemoryPercent "$LOWEST_MEMORY_PERCENT" '
    {
      schemaVersion: 1,
      startedAt: $startedAt,
      completedAt: $completedAt,
      durationSeconds: $durationSeconds,
      checks: $checks,
      restarted: true,
      lowestMemoryPercent: $lowestMemoryPercent,
      membershipCellId: $membershipCellId,
      ownerPublicKey: $ownerPublicKey,
      stateCommitment: $stateCommitment,
      finalCreated: false
    }
  ' | tee "castalia-membership-soak-${MEMBERSHIP_CELL_ID}.json"
