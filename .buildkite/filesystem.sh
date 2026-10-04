#!/usr/bin/env bash
set -euo pipefail
rustup toolchain install nightly-2026-06-21 --profile minimal
rustup component add clippy rustfmt --toolchain nightly-2026-06-21
export RUSTUP_TOOLCHAIN=nightly-2026-06-21

case "${1:?gate required}" in
  native)
    temp_dir="$(mktemp -d)"
    trap 'rm -rf "$temp_dir"' EXIT
    cp -R castalia-filesystem-core "$temp_dir/core"
    cp .buildkite/core-harness.toml "$temp_dir/Cargo.toml"
    cp .buildkite/core-harness.lock "$temp_dir/Cargo.lock"
    cargo fmt --manifest-path "$temp_dir/Cargo.toml" --all -- --check
    cargo clippy --manifest-path "$temp_dir/Cargo.toml" -p castalia-filesystem-core --all-targets --locked -- -D warnings
    cargo test --manifest-path "$temp_dir/Cargo.toml" -p castalia-filesystem-core --locked
    ;;
  wasm)
    rustup target add wasm32-unknown-unknown
    if [[ "$(wasm-pack --version 2>/dev/null || true)" != "wasm-pack 0.14.0" ]]; then
      cargo install wasm-pack --version 0.14.0 --locked
    fi
    if [[ "$(wasm-bindgen --version 2>/dev/null || true)" != "wasm-bindgen 0.2.127" ]]; then
      cargo install wasm-bindgen-cli --version 0.2.127 --locked
    fi
    temp_dir="$(mktemp -d)"
    trap 'rm -rf "$temp_dir"' EXIT
    wasm-pack build castalia-filesystem-wasm --target nodejs --release --mode no-install --no-opt --out-dir "$temp_dir/node-pkg" -- --locked
    node castalia-filesystem-wasm/tests/parity.mjs "$temp_dir/node-pkg"
    ;;
  *) echo "unknown Files source gate" >&2; exit 2 ;;
esac
