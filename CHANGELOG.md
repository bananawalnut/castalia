# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Fixed

- Aligned the generated cargo-dist Release workflow SPDX header with cargo-dist 0.31.0 output — restores the release plan drift gate for issue #5.
- Added the RED-only authority lifecycle transition wire and inert pre-A3b gate contract for issue #28; the production schema and gate remain absent.
- Added canonical world-scoped authority lifecycle identities, commitments, and issuer-key bindings for issue #28 without activating lifecycle mutation.
