#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 START_INDEX END_INDEX" >&2
  exit 2
fi

START_INDEX="$1"
END_INDEX="$2"
case "$START_INDEX" in
  ''|*[!0-9]*)
    echo "checkpoint indices must be non-negative integers" >&2
    exit 2
    ;;
esac
case "$END_INDEX" in
  ''|*[!0-9]*)
    echo "checkpoint indices must be non-negative integers" >&2
    exit 2
    ;;
esac
if [ "$START_INDEX" -ge "$END_INDEX" ]; then
  echo "checkpoint start must be less than end" >&2
  exit 2
fi

bash scripts/ci-mathlib-cache.sh
mapfile -t ALL_EMITTERS < <(python3 scripts/emit_descriptors.py --list-emitter-modules)
if [ "${#ALL_EMITTERS[@]}" -lt "$END_INDEX" ]; then
  echo "descriptor emitter list has ${#ALL_EMITTERS[@]} entries; checkpoint requires $END_INDEX" >&2
  exit 2
fi

CHECKPOINT_EMITTERS=("${ALL_EMITTERS[@]:START_INDEX:END_INDEX-START_INDEX}")
if [ "${#CHECKPOINT_EMITTERS[@]}" -eq 0 ]; then
  echo "descriptor emitter checkpoint is empty" >&2
  exit 2
fi

echo "Building descriptor emitter indices [$START_INDEX, $END_INDEX) (${#CHECKPOINT_EMITTERS[@]} modules)"
(
  cd metatheory
  lake build "${CHECKPOINT_EMITTERS[@]}"
)
