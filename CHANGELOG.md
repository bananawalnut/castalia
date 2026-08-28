# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Added

- Added an OCI-specific permissionless-membership acceptance record covering
  the protected ARM64 release, DNS, runtime verification, deterministic retry,
  encrypted backup/restore, and clean-profile Wallet recovery without storing
  secrets in the repository.
- Added protected x86-64 and ARM64 Castalia bootstrap artifacts plus a hardened Oracle Cloud Always Free deployment package — production nodes can install a verified Lean-backed binary without compiling from a mutable server checkout.
- The protected bootstrap release now publishes its verified x86-64 Lean kernel seed under the canonical content-keyed name, making subsequent clean CI builds reproducible without weakening the verified-producer gate.

### Changed

- Derived the Mina verifier's production root geometry and proof-budget ratchets from the emitted root proof — descriptor changes now fail explicitly instead of silently retaining the previous query, layer, and opening counts.
- Updated the Forge gas sanity ratchet for Foundry 1.8 while retaining the stronger early-versus-late constant-cost assertion.

### Fixed

- Repaired root AIR, root-FRI, WASM release, and deployment verification gates — the permissionless membership node can be built and checked against the production proof and fail closed during install, restart, backup, and rollback operations.
- Refreshed the Mina FRI-preamble protocol ratchets from the canonical seven-instance root and derived its opened-value permutation count instead of reporting the retired proof shape.
- Seeded the upgradeable Groth16 verifier from the generated canonical settlement key, eliminating a stale copied key that broke proof attestation and cross-chain digest checks.
- Kept documentation CI's Lean scan exhaustive without accidentally treating archived metatheory Markdown as live documentation.
