# Castalia permissionless membership v2

Status: implemented contract for Wallet-to-Dregg Join

## Purpose

Base Castalia membership is open to anyone. The active flow has no application, individual/institution class, admission authority, Pending state, or Control dependency. A Wallet proves possession of its Member Key and a Dregg node relays the exact public factory birth.

The legacy authority-owned `CASTMEM1` factory remains additive for stored-data and decoder compatibility. It is not used by v2 Join.

## Identity and addressing

- Member Key: Ed25519 raw 32-byte public key.
- Join transcript: `b"castalia/permissionless-membership-join/v2\0" || owner_public_key`.
- Factory ID: `BLAKE3(b"castalia/permissionless-membership-factory/v2\0")`.
- Token ID: `BLAKE3(b"castalia/permissionless-membership-cell/v2\0")`.
- Cell ID: `CellId::derive_raw(owner_public_key, token_id)`.

The resulting address is one-per-key and makes retries idempotent. The cell's public key is the Member Key. The relay node is neither the membership owner nor an admission authority.

## Exact birth

The cell is Sovereign, has no capabilities or delegation, carries the canonical immutable Cases program, and has these exact fields:

| Slot | Value |
| --- | --- |
| 0 | `u64::from_le_bytes(*b"CASTMEM2")` |
| 1 | schema `2` |
| 2 | public self-issuance policy `1` |
| 3–11 | `0` |
| 12 | Active `1` |
| 13 | generation `0` |
| 14–15 | `0` |

Every slot is immutable. Base membership cannot be suspended or revoked. Roles, moderation, sanctions, services, and consequential permissions belong in separate cells.

## HTTP contract

Public request:

```http
POST /api/castalia/memberships
Content-Type: application/json

{
  "version": 2,
  "ownerPublicKey": "<64 lowercase-compatible hex>",
  "signatureSuite": "Ed25519",
  "signature": "<64-byte unpadded base64url>"
}
```

The node verifies the signature before mutation. A funded node relay cell signs a `CreateCellFromFactory` turn, and blocklace finalization is the sole writer of the ledger mutation, attested root, and durable commit record. The response contains version, deterministic membership cell ID, owner key, `active`, generation zero, factory/program IDs, state commitment, optional receipt hash, and whether this call created the cell. An existing exact cell returns `created: false`; a conflicting or non-canonical cell fails closed.

The endpoint waits until the finalized cell is visible before returning success. Recovery reconstructs it from the canonical commit log and checkpoints; a completed Join therefore survives restart without creating a checkpoint/root mismatch.

## Verification matrix

| Layer | Positive evidence | Negative evidence |
| --- | --- | --- |
| Contract | public birth is owner-bound, Active, exact, deterministic | zero owner and altered fields reject |
| Node | signature verifies and stable factory composes on every executor | substituted owner/signature rejects |
| Mutation | first call finalizes the v2 cell through blocklace | unfunded relay, unavailable consensus, and executor rejection fail closed |
| Idempotency | retry returns the same cell | conflicting stored cell fails integrity validation |
| Durability | issued cell restores from the canonical commit log/checkpoint | root mismatch fails closed |
| Wallet | owner/token/address/fields/program/caps/both delegation forms/commitment all verify | any drift rejects before Web success |

Focused tests:

```text
cargo test -p starbridge-castalia-membership --test permissionless
cargo test -p dregg-node castalia_membership::tests --lib
```

## Deployment boundary

The local acceptance profile runs the Castalia Dregg node on loopback port `8420`. Production requires an operated HTTPS endpoint and should add a light-client or quorum-pinned inclusion proof so Wallet need not trust a single node's JSON projection.
