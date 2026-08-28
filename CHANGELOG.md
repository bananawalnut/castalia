# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Added

- Added protected x86-64 and ARM64 Castalia bootstrap artifacts plus a hardened Oracle Cloud Always Free deployment package — production nodes can install a verified Lean-backed binary without compiling from a mutable server checkout.

### Changed

- Derived the Mina verifier's production root geometry and proof-budget ratchets from the emitted root proof — descriptor changes now fail explicitly instead of silently retaining the previous query, layer, and opening counts.

### Fixed

- Repaired root AIR, root-FRI, WASM release, and deployment verification gates — the permissionless membership node can be built and checked against the production proof and fail closed during install, restart, backup, and rollback operations.
