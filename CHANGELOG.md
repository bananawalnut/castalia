# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Fixed

- Aligned the generated cargo-dist Release workflow SPDX header with cargo-dist 0.31.0 output — restores the release plan drift gate for issue #5.
- Added the intentional RED contract for Castalia's strict live `dga1_` authority profile (issue #27 C01).
- Added canonical operation/resource grammar and private positive-conjunctive profile analysis for the strict live `dga1_` authority path (issue #27 C02).
- Added the sealed, exact-resource strict live-authority verifier result with finite validity and issuer/credential-tail binding (issue #27 C03).
- Added bounded strict live-authority decoding, redacted failures, and best-effort buffer overwrite — to reject hostile presentations before allocating credential trees or exposing authority values (issue #27 C04).
- Documented strict live-authority compatibility, decode/copy limits and final contract gates — so integrations preserve legacy behavior and do not mistake a verified presentation for durable or production authority (issue #27 C05).
