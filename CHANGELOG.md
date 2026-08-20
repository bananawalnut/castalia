# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- Added a protected Linux x86-64 Castalia bootstrap-node release workflow that
  must link and report the verified Lean producer before publishing a checksum,
  provenance statement, and SPDX SBOM.
- Registered the Lean-authored Cert-F descriptor with the canonical descriptor
  emitter and replaced stale workflow-specific mathlib clones with Lake's pinned
  portable Git dependency.
- Moved the seven Lean-generated staged descriptor TSVs from exhausted Git LFS
  bandwidth to checksum-verified hydration from a private, versioned S3 store.
  Screenshots remain on Git LFS.
