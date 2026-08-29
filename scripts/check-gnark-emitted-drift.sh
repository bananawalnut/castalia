#!/usr/bin/env bash
# Re-emit every Lean-owned Gnark replay artifact into an empty directory and
# compare the complete filename set and exact bytes with the committed cache.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMMITTED="$ROOT/chain/gnark/emitted"
GENERATED="$(mktemp -d -t dregg-gnark-emitted.XXXXXX)"
trap 'rm -rf "$GENERATED"' EXIT
export DREGG_GNARK_EMITTED_DIR="$GENERATED"

cd "$ROOT/metatheory"
lake env lean --run scripts/gen_gnark_emitted_templates.lean
lake env lean --run scripts/gen_merkle_templates.lean
lake env lean --run scripts/gen_fri_fold_template.lean
lake env lean --run scripts/gen_query_pow_templates.lean
lake env lean --run scripts/gen_selectors_witness.lean

artifact_names() {
  find "$1" -maxdepth 1 -type f \( \
    -name 'leafhash_template.json' -o \
    -name 'inputopen_batch_template.json' -o \
    -name 'inputopen_batch_r*.json' -o \
    -name 'verifier_full.json' -o \
    -name 'selectors_db*.json' -o \
    -name 'selectors_witness_db*.json' -o \
    -name 'merkle_path_bn254_d*.json' -o \
    -name 'fri_fold_template.json' -o \
    -name 'fri_fold_witness.json' -o \
    -name 'query_pow_n*.json' \
  \) -exec basename {} \; | LC_ALL=C sort
}

committed_names="$GENERATED/.committed-names"
generated_names="$GENERATED/.generated-names"
artifact_names "$COMMITTED" > "$committed_names"
artifact_names "$GENERATED" > "$generated_names"

if ! cmp -s "$committed_names" "$generated_names"; then
  echo 'gnark emitted drift: artifact filename set differs' >&2
  diff -u "$committed_names" "$generated_names" || true
  exit 1
fi

while IFS= read -r name; do
  if ! cmp -s "$COMMITTED/$name" "$GENERATED/$name"; then
    echo "gnark emitted drift: bytes differ for $name" >&2
    if command -v sha256sum >/dev/null 2>&1; then
      sha256sum "$COMMITTED/$name" "$GENERATED/$name" >&2
    else
      shasum -a 256 "$COMMITTED/$name" "$GENERATED/$name" >&2
    fi
    exit 1
  fi
done < "$generated_names"

count="$(wc -l < "$generated_names" | tr -d ' ')"
echo "gnark emitted drift: ok — $count canonical artifacts match"
