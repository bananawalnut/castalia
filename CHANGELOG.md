# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Fixed

- Scoped the Security Audit gate to the locked dependency closure of the cargo-dist production binaries, keeping GPUI desktop findings on their separate installer lane without ignoring the quick-xml advisories; also updated production-reachable `crossbeam-epoch` to 0.9.20.
- Aligned the generated cargo-dist Release workflow SPDX header with cargo-dist 0.31.0 output — restores the release plan drift gate for issue #5.
