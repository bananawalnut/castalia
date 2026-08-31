# LayerZero concept map: what dregg has, where, and what differs

The LayerZero V2 protocol (docs.layerzero.network/v2/concepts/protocol/protocol-overview)
decomposes cross-chain messaging into a small set of primitives. This maps each
one onto the dregg mechanism that provides the same guarantee, with the honest
deltas. Grounded at HEAD; companion to `docs/deos/ETH-NATIVE-WRAP.md`.

The one-line difference first: **LayerZero transports *attestations about*
messages (a DVN committee votes that a payload hash is real); dregg transports
*proofs of* state transitions** (a whole-history recursive STARK any verifier
can check). Where LayerZero's trust knob is "X of Y of N verifiers," dregg's
settlement path needs no committee at all — and where dregg *does* still use a
committee (asset mirroring, optimistic bridges), that is called out below, not
hidden.

## The map

| LayerZero primitive | What it does there | dregg mechanism | Where |
|---|---|---|---|
| **Endpoint** (immutable per-chain contract; `send()` / `lzReceive()`) | Entry/exit point for omnichain messages | Settlement contracts: `IDreggSettlement.settle(a,b,c, genesisRoot, finalRoot, numTurns, chainDigest)` for whole-history proofs; `DreggVault` / `DreggCredentialGate` for value + credentials | `chain/contracts/` |
| **Message packet + Message Libraries** | Standardized packet a library encodes per sender config | The wire envelope `WholeChainProofBytes` (publics + VK anchor) and the EVM calldata encoding (`Groth16Calldata`, `EthPublicInputs::to_calldata`) | `circuit-prove/src/ivc_turn_chain.rs`, `bridge/src/ethereum.rs` |
| **DVNs / Security Stack (X of Y of N)** | A configurable committee delivers payload hashes; threshold ⇒ "verified" | Replaced by the proof itself: `verify_turn_chain_recursive_from_parts` — VK pin + batch-STARK FRI verify + segment tooth. Any full or light client re-checks it; there is no threshold to configure because there is no attestor to trust | `circuit-prove/src/ivc_turn_chain.rs:3587`, `lightclient/src/lib.rs` (`verify_history`) |
| **Executor** (permissioned-by-config caller of `lzReceive`) | Triggers delivery, pays dst gas | Untrusted submitter: anyone may relay settlement calldata; the contract verifies the proof, and `EthBridgeState` enforces continuity + monotone height regardless of who submits | `bridge/src/ethereum.rs:373` |
| **Nonce / exactly-once channel ordering** | Per-channel lazy-nonce tracking prevents replay/loss | Nullifiers (on-chain `usedNullifiers` in the vault/gate; in-protocol the deployed sorted-Merkle nullifier accumulator, `noteSpendVmDescriptor2R24`) for exactly-once spends; chain continuity (`genesis_root → final_root`, monotone `num_turns`) for ordering | `chain/contracts/DreggVault.sol`, `bridge/src/ethereum.rs`, `metatheory/Dregg2/Exec/RecordKernel.lean`, `circuit/src/descriptor_ir2.rs` |
| **VM agnosticity** | Endpoints implemented per VM | The proof's publics are BabyBear (31-bit) limbs — losslessly embeddable in any scalar field (BN254 today; BLS12-381 etc. possible); verification lands wherever a Groth16-class verifier runs | `docs/deos/ETH-NATIVE-WRAP.md` |
| **Immutability / permissionlessness** | Immutable endpoint contracts, open send/receive | Same property, carried by the verifier contract + the VK pin: a settlement is valid iff the proof verifies against the pinned circuit shape, full stop | `chain/contracts/IDreggSettlement.sol` |

## The honest rows — where a committee or a named seam remains

- **Asset mirroring (pump.fun $DREGG → the shielded pool):** the Solana-side
  lock *settle* is observed by an **oracle/validator-set threshold
  attestation** — functionally an X-of-Y DVN (the lock program itself is built:
  `solana-lock/`). The trustless consensus-verified read path is built and
  anchored (governance-pinned `WeakSubjectivityAnchor`, no caller-supplied
  stake table) — `bridge/` (`TOKEN-MIRROR-BRIDGE.md`,
  `TRUSTLESS-SOLANA-BRIDGE.md` / `SOLANA-SUCCINCT-WRAPPER.md`).
- **The EVM `outboundMessageRoot` leg:** **fail-closed**, not committee-run —
  `DreggSettlement.settle` reverts on any non-zero `outboundMessageRoot`
  (`MessageRootNotProofBound`), so no operator can record a message root the
  proof does not carry. The named seam that remains is proof-binding itself
  (the 26th-public-input obligation): until the root is a proof lane, the
  message leg refuses rather than trusts. `chain/contracts/DreggSettlement.sol`.
- **Midnight:** native proof-carrying is foreclosed by Midnight's architecture
  (fixed-VK per entry point, no general verification primitive), so the bridge
  there is optimistic + a **1-of-N watchtower fraud proof** — strictly stronger
  than X-of-Y (one honest watcher suffices, because the objective evidence is a
  circuit proof). This was implemented by the now-retired `midnight_verified.rs` watchtower.
- **Inbound (reading other chains):** "be a full client of the other chain" is
  the design stance, and the verified cores exist: `eth-lightclient/` verifies
  the Altair sync-committee BLS12-381 aggregate signature and the SSZ
  committee-rotation branch (with ETH-Base support in `base.rs` /
  `base_fault_proof.rs`), and `bridge/src/solana_trustless.rs` carries the
  consensus-anchored ≥2/3-stake verify path. The named seam: the deposit
  listener (`chain/src/listener.rs`) still trusts RPC confirmations on its
  polling path — the verified cores are the teeth it does not yet consult.

## What LayerZero has that dregg does not

- **Deployed ubiquity.** Endpoints on 100+ chains, live DVN/executor markets.
  dregg's EVM settlement is a contract set + a real Groth16 wrap prover — a
  real proof settles against the gnark-generated Solidity verifier in Foundry —
  but with a dev-ceremony trusted setup and no public-chain deployment yet
  (`docs/deos/ETH-NATIVE-WRAP.md`, residuals in
  `WRAP-NATIVE-HASH-DECISION.md` §CURRENT STATE).
- **Arbitrary app-to-app messaging (OApp).** LayerZero moves any payload
  between contracts; dregg's cross-chain surface is purposive — settlement,
  value, credentials. (Anything expressible as a dregg turn is provable, but
  dregg does not aim to be a generic contract-to-contract message bus.)

## Why "wrap/bridge your token" questions usually dissolve here

A LayerZero-style token bridge exists because chain A cannot check what
happened on chain B, so a committee escrows-and-attests. dregg's settlement
object is a *proof of everything that happened* — any chain with a pairing
precompile can verify dregg state transitions directly (one Groth16-class
check; measured 626k gas for the 25-lane commitment-extended settle), and
dregg-side assets stay in the holder's custody under the
shielded pool rather than moving into a bridge wallet. The remaining genuine
committee (the SPL mirror's lock oracle) is scoped to *inbound value*, and has
a designed trustless replacement.
