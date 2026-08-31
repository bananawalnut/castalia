# Castalia permissionless membership OCI acceptance record

Complete this record only after every gate passes. Do not put private keys,
passphrases, OCI credentials, tenancy secrets, or the contents of the recovery
export here.

## Release and infrastructure

- Acceptance UTC:
- Owner/operator:
- OCI home region:
- OCI instance OCID (non-secret):
- OCI shape (`VM.Standard.A1.Flex`, 2 OCPUs, 12 GB RAM):
- Dregg deployed commit (40 lowercase hex):
- Protected release tag and workflow run:
- `dregg-node-linux-aarch64` SHA-256:
- Provenance JSON SHA-256:
- SPDX SBOM SHA-256:
- Reserved public IPv4 address:
- `dregg.zenith-research.ca` A target:
- `castalia.zenith-research.ca` CNAME target:
- Wallet commit:
- Wallet hosted workflow run and artifact name:
- Wallet production package content-tree SHA-256:
- Web commit:

## Membership and runtime acceptance

- Membership cell ID:
- Membership owner public key:
- First issuance returned `created: true`:
- Idempotent retry returned the same cell with `created: false`:
- Invalid signature, wrong owner, modified field, and malformed response tests:
- Node restart preserved the same membership:
- 30-minute soak record filename and SHA-256:
- Lowest available-memory percentage during soak:
- Public route boundary preflight result:
- Verified Lean producer reported at runtime:

## Recovery and clean-profile acceptance

- Encrypted backup filename and SHA-256:
- Off-machine backup location:
- Backup decrypt-test UTC:
- Restore-test UTC and result:
- `.castalia-recovery` export/import UTC and result:
- Reload, lock, and unlock preserved the same identity and membership:
- Clean Chrome profile acceptance UTC and result:
- Confirmed no login cookie, Castalia session, Matrix token, or Control approval:

## Operator sign-off

- Previous verified binary retained and rollback tested:
- Node key, genesis, and ledger restore produced the same membership:
- Final operator sign-off:
