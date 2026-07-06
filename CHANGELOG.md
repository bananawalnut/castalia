# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Fixed

- Gated the trustless Solana `MirrorMint` import to its test-utils minting paths — restores the dregg-bridge Clippy advisory gate without broadening production API usage.
- Reworded Clippy-sensitive doc continuations in the shielded-transfer and escrow-market docs — restores the issue #3 Clippy advisory gate.
- Aligned the generated cargo-dist Release workflow SPDX header with cargo-dist 0.31.0 output — restores the release plan drift gate for issue #5.
