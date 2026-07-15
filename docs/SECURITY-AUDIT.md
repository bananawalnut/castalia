# Production dependency audit

Castalia's production security-audit boundary is the two packages opted into cargo-dist:

- `dregg-cli` (`dregg`)
- `dregg-node` (`dregg-node`)

The root `Cargo.lock` intentionally also captures the separately shipped GPUI/deos desktop workspace. Auditing that entire lockfile conflates desktop-only dependencies with the production binaries. `scripts/production_audit_lock.py` instead reads locked Cargo metadata for the Linux production target, traverses normal and build dependencies from the two production package roots, rejects missing roots, and emits a temporary lockfile containing only that reachable closure. Dev dependencies are excluded.

CI runs the script's regression tests, generates `/tmp/castalia-production.lock`, and passes that file to `cargo audit`. Existing assessed advisory dispositions remain sourced from `audit.toml`; the quick-xml advisories are not ignored.

## Local verification

```sh
python3 scripts/test_production_audit_lock.py -v
python3 scripts/production_audit_lock.py \
  --target x86_64-unknown-linux-gnu \
  --output /tmp/castalia-production.lock

mapfile -t IDS < <(grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' audit.toml | sort -u)
IGNORE_ARGS=()
for id in "${IDS[@]}"; do IGNORE_ARGS+=(--ignore "$id"); done
cargo audit --file /tmp/castalia-production.lock "${IGNORE_ARGS[@]}"
```

The `mapfile` snippet requires Bash (the CI runner uses Bash). On macOS, use a Bash installation or construct the repeated `--ignore` arguments equivalently.

## Claim boundary

A green `Security Audit` check means the locked dependency closure of the production CLI and node packages contains no unignored RustSec vulnerabilities for the audited Linux target. It does not mean:

- the GPUI/deos desktop installer lane is green or vulnerability-free;
- every package captured by the root lockfile is production-reachable;
- informational RustSec warnings have been eliminated; or
- Castalia has completed an independent security audit.

Desktop installer and desktop dependency findings remain owned by the dedicated Starbridge workflow and issue lane. If a desktop package becomes cargo-dist production output, add it to `PRODUCTION_PACKAGES` in the script in the same change; the tests and fail-closed root lookup make that policy boundary explicit.
