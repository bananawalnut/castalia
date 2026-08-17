#!/usr/bin/env bash
# Install the exact Lean toolchain and prepare the pinned descriptor source tree.
# This is intentionally narrower than scripts/bootstrap.sh: descriptor hydration
# does not need the Rust FFI archive or any repository mutation.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
META="$ROOT/metatheory"
ELAN_VERSION="v4.2.3"
TEMP_BASE="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
TEMP_DIR="$(mktemp -d "${TEMP_BASE%/}/castalia-descriptor-bootstrap.XXXXXX")"
trap 'rm -rf "$TEMP_DIR"' EXIT

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)
    ELAN_TARGET="x86_64-unknown-linux-gnu"
    ELAN_ARCHIVE_SHA256="df0b2b3a439961ffcbb3985214365ffe40f49bc871df04dff268c7d8e21ca8b2"
    ;;
  Linux-aarch64|Linux-arm64)
    ELAN_TARGET="aarch64-unknown-linux-gnu"
    ELAN_ARCHIVE_SHA256="cb69af0803b04157bc30201c29c12fca882bb3ad8b43476b8d2d3064810bc3ac"
    ;;
  Darwin-x86_64)
    ELAN_TARGET="x86_64-apple-darwin"
    ELAN_ARCHIVE_SHA256="10d037a69731c0593723e018130c5f54afde175796b4af8ba1317e561e55598c"
    ;;
  Darwin-arm64|Darwin-aarch64)
    ELAN_TARGET="aarch64-apple-darwin"
    ELAN_ARCHIVE_SHA256="7cae4c03b2f0de4053fb04a91359d5804551e6e37a6ddd1b2e0097dc561ae4a9"
    ;;
  *)
    echo "unsupported elan installer platform: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

ELAN_ARCHIVE="$TEMP_DIR/elan.tar.gz"
ELAN_ARCHIVE_URL="https://github.com/leanprover/elan/releases/download/${ELAN_VERSION}/elan-${ELAN_TARGET}.tar.gz"
curl --proto '=https' --tlsv1.2 -fsSL "$ELAN_ARCHIVE_URL" -o "$ELAN_ARCHIVE"
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL_ELAN_ARCHIVE_SHA256="$(sha256sum "$ELAN_ARCHIVE" | awk '{print $1}')"
else
  ACTUAL_ELAN_ARCHIVE_SHA256="$(shasum -a 256 "$ELAN_ARCHIVE" | awk '{print $1}')"
fi
if [[ "$ACTUAL_ELAN_ARCHIVE_SHA256" != "$ELAN_ARCHIVE_SHA256" ]]; then
  echo "elan installer archive checksum mismatch" >&2
  echo "expected: $ELAN_ARCHIVE_SHA256" >&2
  echo "actual:   $ACTUAL_ELAN_ARCHIVE_SHA256" >&2
  exit 1
fi

tar -xzf "$ELAN_ARCHIVE" -C "$TEMP_DIR"
"$TEMP_DIR/elan-init" -y --default-toolchain "$(cat "$META/lean-toolchain")"
export PATH="${ELAN_HOME:-$HOME/.elan}/bin:$PATH"
if [[ -n "${GITHUB_PATH:-}" ]]; then
  printf '%s\n' "${ELAN_HOME:-$HOME/.elan}/bin" >> "$GITHUB_PATH"
fi

EXPECTED_MATHLIB_REV="$(
  sed -n 's/^rev = "\([0-9a-f]\{40\}\)"$/\1/p' "$META/lakefile.toml" | head -1
)"
if [[ -z "$EXPECTED_MATHLIB_REV" ]]; then
  echo "could not read pinned mathlib revision from metatheory/lakefile.toml" >&2
  exit 1
fi

(
  cd "$META"
  # Limit the mathlib cache to the import closure of the seven staged emitters.
  # A bare `cache get` downloads the whole of Mathlib, most of which is unrelated.
  lake exe cache get \
    EmitRotationV3.lean \
    EmitWideTransferProbe.lean \
    EmitWideRegistryProbe.lean \
    EmitUMemCohort.lean \
    EmitUMemCohortMulti.lean \
    EmitWideUMemWeldRegistryProbe.lean \
    EmitRotationV3SetFieldValue8.lean
  ACTUAL_MATHLIB_REV="$(git -C .lake/packages/mathlib rev-parse HEAD)"
  if [[ "$ACTUAL_MATHLIB_REV" != "$EXPECTED_MATHLIB_REV" ]]; then
    echo "mathlib checkout mismatch" >&2
    echo "expected: $EXPECTED_MATHLIB_REV" >&2
    echo "actual:   $ACTUAL_MATHLIB_REV" >&2
    exit 1
  fi

  # The emitter modules import local Dregg2 modules. Mathlib's cache only
  # provides dependency oleans, so a fresh checkout must compile those roots
  # before export-only hydration or publication can invoke the emitters.
  #
  # Build the roots sequentially: the umbrella `lake build Dregg2` also builds
  # thousands of unrelated proof modules and can exhaust a runner's memory.
  DREGG2_EMITTER_ROOTS=(
    Dregg2.Circuit.Emit.EffectVmEmitRotation
    Dregg2.Circuit.Emit.EffectVmEmitRotationR
    Dregg2.Circuit.Emit.EffectVmEmitRotationCaveat
    Dregg2.Circuit.Emit.EffectVmEmitRotationV3
    Dregg2.Circuit.Emit.CapOpenEmit
    Dregg2.Circuit.Emit.CapOpenTurnPins
    Dregg2.Circuit.RotatedKernelRefinementExercise
    Dregg2.Circuit.RotatedKernelRefinementCapOpenAvail
    Dregg2.Circuit.Emit.EffectVmEmitRotationWide
    Dregg2.Circuit.Emit.HeapOpenEmit
    Dregg2.Circuit.Emit.FieldsOpenEmit
    Dregg2.Circuit.Emit.AvailWireMembers
    Dregg2.Circuit.Emit.AvailWideMembers
    Dregg2.Circuit.Emit.AvailWideFeeMember
    Dregg2.Circuit.Emit.AccumulatorInsertEmit
    Dregg2.Circuit.Emit.CarrierComposed
    Dregg2.Circuit.Emit.EffectVmEmitUMemCohort
    Dregg2.Circuit.Emit.EffectVmEmitUMemCohortMulti
    Dregg2.Circuit.Emit.EffectVmEmitUMemWeldWide
    Dregg2.Deos.BareCohortFloorRefuseWide
    Dregg2.Deos.DischargeSatDescriptor
    Dregg2.Deos.VaultSatDescriptor
    Dregg2.Deos.SettleEscrowSatDescriptor
    Dregg2.Circuit.Emit.EffectVmEmitRotationV3Refused
  )
  for module in "${DREGG2_EMITTER_ROOTS[@]}"; do
    lake build "+$module"
  done
)
