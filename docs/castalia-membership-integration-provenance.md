# Castalia membership integration provenance

Issue: [bananawalnut/castalia#33](https://github.com/bananawalnut/castalia/issues/33)

## Clean integration boundary

- Integration base: `bananawalnut/castalia` `origin/main` at `89494bd81279cb65b74f527e2bd94b4a4fe74d06`.
- Reconstructed prototype evidence: uncommitted `feat/castalia-membership` worktree observed at `c08967ca2bf2d5fcf25561e070f9a46efc54774b`, whose merge-base with the integration base is the exact base above.
- D0 authority: merged Castalia Wallet PR [#3](https://github.com/bananawalnut/castalia-wallet/pull/3), merge commit `08e32d65169dea74d59362938a16f4aa8cb7e06b`.
- Repository-owned D0 inputs consumed here are limited to the reviewed `CASTMEM1` contract and cell-derivation vector. Their SHA-256 digests at the D0 merge commit are:
  - `docs/contracts/castalia-membership-v1.json`: `e1e38eaa671061c976009f0a4d4126512a4a72105ca7f44a541f66230c2117b4`
  - `docs/contracts/castalia-membership-cell-derivation-v1.vector.json`: `d90e232ba20bd11f89b87d29ed6f839fb2d45a72c25a780c0dd9feb41ff33d40`

The dirty prototype also contains unrelated node, persistence, macaroon, proving, and generated native-archive changes. Those files are evidence only and are not copied wholesale. Membership source is reconstructed commit-by-commit in this isolated branch; generated native archives are excluded.

## Identity boundary

The institutional membership cell is created through canonical `Effect::CreateCellFromFactory`, so its identity is `CellId::derive_raw(owner_pubkey, token_id)`. For this cell, `owner_pubkey` is the Castalia authority. The immutable application separately commits the member Ed25519 owner key. `applicantOfficialDreggCellId` and `membershipCellId` are distinct and non-circular.

The legacy `dregg_create_from_factory` factory-VK/cell-name/receipt-nonce identity path is forbidden for this integration.
