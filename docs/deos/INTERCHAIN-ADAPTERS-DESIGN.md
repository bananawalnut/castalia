# Interchain Adapters: dregg as a proof-carrying interoperability backend

*Draft — 2026-07-11. Companion to `docs/deos/LAYERZERO-CONCEPT-MAP.md` and
`docs/FINDING-chain-participation-census.md`. The Lean-modeling and code sections
are filled in from the in-tree audit; this file is the design frame.*

## The thesis

dregg does not need to pick one interop standard. Every cross-chain standard is,
underneath, a way to answer one question — **"did event E really happen on chain
C?"** — under some trust model. dregg already produces a succinct validity proof
of its own state transitions (`WholeChainProof`, verified by
`lightclient::verify_history`). So dregg's native role in any interop standard is
the **verification backend**: the thing that answers "did E happen" by *proof*
rather than by committee vote or optimistic assumption.

**Participation is proof-of-holdings, not a wrap.** Before the standards survey, the
governing rule for *who participates*: a `$DREGG` holder votes / carries governance
weight by PROVING they hold it over their own account — custody intact, nothing locked
or wrapped (`docs/deos/PROOF-OF-HOLDINGS.md`, built + Lean-verified). The lock/mirror
adapters below are the EXCEPTION: they import *spendable value* or post a *slashable
bond*, never a prerequisite for weight. Every adapter that follows is a
value/state-movement rail; none of them is the door to participation.

Rather than reimplement Tendermint / GRANDPA / Ethereum consensus inside our own
formal proofs, we **model the interaction**: an `InterchainAdapter` names, for
each standard, (1) what dregg checks in-circuit or on-chain, and (2) what it
*assumes* about the foreign chain — and a single soundness theorem is
parameterized over that assumption. Where we build a real light client (ETH
sync-committee, Solana Tower-BFT — already real in Rust), the assumption
discharges to a theorem; where in-circuit verification is foreclosed (Midnight),
the assumption is carried by an optimistic + 1-of-N watchtower, which dregg's
proof *makes objective*. This is the same shape as ember's bar: **be a full
client where you can, fall back to optimistic-with-proof-as-evidence where you
can't, never a blind committee wrap.**

## The trust-model lattice (every standard is one of these)

Ordered from strongest to weakest; the adapter tags each integration with its rung.

1. **Proof / light-client** — verify the foreign chain's consensus + a state
   inclusion proof. Trust = the foreign chain's own security + our verifier's
   correctness. Family: IBC (Tendermint light client), zk-light-clients
   (Succinct/Telepathy, Polymer, Electron, Lagrange), zkIBC.
   *dregg fit:* build the light client; this is the bar. Solana Tower-BFT already
   real in Rust (off-circuit); the ETH sync-committee client is built
   (`eth-lightclient/`, BLS12-381 aggregate verify via blst).
2. **Optimistic + fraud proof** — assume valid, allow a bonded challenge within a
   window; 1 honest watcher suffices. Family: **Optics/Nomad → Hyperlane**
   optimistic modules, Across intents.
   *dregg fit:* ALREADY BUILT for Midnight (`Watchtower::examine`); dregg's STARK
   is the objective fraud-proof evidence, turning 2/3-committee trust into 1-of-N.
   This is the *strongest available* posture for any chain we can't yet
   light-client.
3. **Threshold / committee attestation** — an M-of-N set signs "E happened."
   Trust = honest majority of a permissioned set. Family: LayerZero DVNs,
   Wormhole guardians, Axelar validators, CCIP DON.
   *dregg fit:* the fallback of last resort (the SPL mirror's lock oracle is here
   today). Where a standard lets us register a **custom verifier** (LayerZero
   permissionless DVN, Hyperlane custom ISM, Axelar Interchain Amplifier), dregg
   plugs in as *one* verifier whose vote is backed by a real proof — upgrading the
   committee's weakest link.
4. **RPC / finality-tag trust** — trust a node's `finalized` tag or N
   confirmations. Not a real interop trust model; the current ETH/Base inbound
   listener sits here and must be lifted to rung 1 or 2 (the rung-1 client
   exists in-tree — `eth-lightclient/` — so the lift is wiring, not a new client).

## The standards, and where dregg's proof plugs in (2026 survey)

Every committee-default standard *also* exposes a seam to slot a cryptographic
verifier in — that seam is the whole opportunity. Ranked by fit for a
proof-carrying system:

| Standard | Default trust | The verifier seam | Permissionless? | dregg fit |
|---|---|---|---|---|
| **Hyperlane ISM** | per-app modular (multisig default) | `IInterchainSecurityModule.verify(metadata, message)` — 2 fns, `verify` is non-view (stateful: verify one epoch proof, cheap lookups after) | **YES** — per-recipient opt-in, no governance; `CCIP_READ` moduleType routes arbitrary proof metadata w/ zero relayer changes; `POLYMER=12` enum precedent | **FLAGSHIP.** `DreggProofISM.verify` = a dregg settlement attests the message. The default-path zk-ISM slot is UNCLAIMED. |
| **LayerZero DVN** | X-of-Y-of-N committee | on-chain DVN adapter verifies your proof → `ReceiveUln302.verify(header, payloadHash, confirmations)` | **YES** — 77 DVN providers live; Polyhedra/Lagrange already do the zk-adapter pattern | **FASTEST DEMO.** dregg as a *required* DVN = conjunctive AND factor. Distribution (OApps configuring us) is the real work. |
| **IBC 08-wasm client** | Tendermint light client (cryptographic) | a CosmWasm `ClientState`/`ConsensusState` impl of ICS-02 (`verifyClientMessage`, `verifyMembership`, `checkForMisbehaviour`) | governance store per-counterparty (sanctioned, not open) | **DEEPEST.** `ClientState` pins dregg's circuit vkey; `VerifyClientMessage` runs `verify_history`. IBC Eureka already ships `SP1ICS07Tendermint.sol` (Tendermint-in-SP1 → Groth16) — our exact shape. CAVEAT: must solve fork-choice (validity ≠ canonicity) + reciprocal client hosting. |
| **OIF / ERC-7683 intents** | optimistic (Across) / agnostic | conforming `IInputOracle`/`hasAttested` proof verifier; the new (2026-05) `Witness` variable role is a first-class proof slot | **YES** — OIF default settlement is *already Hyperlane ISM-based* | dregg as the "the fill happened" settlement oracle; replaces the optimistic window with a validity proof. |
| **Wormhole NTT transceiver** | 13-of-19 guardians | app-layer custom transceiver verifies your proof, deployer sets M-of-N | open contract, **app-layer only** (core VAA path closed) | conjunctive 2/2 w/ guardians (Boundless/RISC0 precedent, live Aug 2025). Not transport-level. |
| **Axelar Amplifier** | PoS validators (47) | 3 CosmWasm contracts (Verifier/Gateway/Prover) on Axelar | governance + 50k AXL bond (gated) | substitutes committee on ingress, but egress still flows through the prover multisig. Deepest but slowest. |
| **Chainlink CCIP** | permissioned DON | — (OffRamp accepts one DON-signed path; RMN blessing now dormant) | **NO seam** | app-level self-verify inside `ccipReceive` only. |

**THE NOMAD LAW (hard design constraint on every `verify()` we ship).** Nomad's
$190M hack was a vacuity bug: a proxy init set `confirmAt[0x00]=1`, so every
*unproven* message (whose stored proof defaulted to `0x00`) verified as accepted —
"unproven maps to an accepted value." Every dregg ISM/DVN/client `verify` MUST be
proven to REJECT on the zero/default/⊥ input, with a test of that polarity. This
is the same discipline as [[feedback-dont-launder-vacuity-as-honest]] — a spec
must be false on the empty witness — and dregg's fail-closed default. It is the
single most important property of the outbound integrations.

## dregg's two directions

- **Outbound (dregg → chain C): dregg is the prover.** A foreign chain's security
  module / verifier contract consumes dregg's proof. Integration = implement the
  standard's verifier interface (e.g. a Hyperlane ISM, a LayerZero DVN adapter, an
  IBC client-state) whose `verify` calls dregg's on-chain settlement verifier
  (`DreggSettlement`, the 25-lane Groth16 check). One integration per standard's
  *verifier seam*, all resting on the same wrap prover.
- **Inbound (chain C → dregg): dregg is the verifier.** dregg checks C's finality
  before crediting state. Integration = a light client (rung 1) or a watchtower
  (rung 2) per chain, every one routing its mint through the committed
  consume-once nullifier gate (the double-mint fix on record in
  `BRIDGE-ARCHITECTURE-SOUNDNESS.md`).

## The permissionless seams worth targeting first

The high-value integration points are standards that expose a **bring-your-own-
verifier** seam, because dregg plugs in without anyone's permission:

- **Hyperlane ISM** — a contract implementing the `InterchainSecurityModule`
  interface decides whether to accept a message. dregg provides a `DreggProofISM`:
  accept iff a dregg settlement attests the message. This is the flagship — a
  two-function contract, per-recipient, with the default-path zk-ISM slot unclaimed.
- **LayerZero permissionless DVN** — register dregg as a DVN in an app's X-of-Y-of-N
  stack; dregg's vote is proof-backed.
- **Axelar Interchain Amplifier** — register a custom verifier for a new chain
  connection.
- **ERC-7683 cross-chain intents** — dregg as the settlement-correctness oracle for
  intent fills.

## What to extend (grounded in the in-tree audit) — do NOT invent a parallel shape

**Rust — the trust dials exist and the `InterchainAdapter` trait unifies them.**
The trait lives in `bridge/src/interchain_adapter.rs` (re-exported with
`DialAdapter`/`TrustDial`/`TrustRung`/`ChainAttestation` from `bridge/src/lib.rs`),
built over these pieces:
- Trust-tier enums, one per lane, all collapsing to the `consensus_verified: bool`
  the mint gate reads: `LockProofTrust::{StructureOnly, ConsensusVerified}`
  (`bridge/src/solana_trustless.rs:202`), `SnarkSystem::{Groth16Bn254, PlonkBn254,
  BindingOnly}` (`bridge/src/ethereum.rs:107`), the optimistic `Verdict::Fraud`
  from `Watchtower::examine` (retired `midnight_gateway.rs`, former line 230), and
  `FinalizedAttestation` committee quorum (`lightclient/src/lib.rs:598`).
- `trait ProofVerifier` (`bridge/src/verifier.rs:111`) is the verification seam to
  add a foreign-ISM impl to; the RPC-transport trait family (`EthRpc`,
  `SolanaRpc`, `MinaRpc`, `SubstrateRpcClient`) is the observation seam.
- `PortableActionBinding {nullifier, recipient, destination_federation, amount}`
  (retired `action_binding.rs`, former line 78) is the canonical full-fidelity cross-chain
  payload the adapter carries.
- **The single mint authority every inbound path already routes through:**
  `TurnExecutor::bridge_mint_against_lock` (`turn/src/executor/bridge_ledger.rs:261`)
  — trust gate (`TrustTooLow` if `!consensus_verified`), committed consume-once
  `lock_nullifier` against the same `note_nullifiers` set `NoteSpend` rides
  (double-mint fix), conservation backstop. A new adapter feeds THIS; it is the
  shared hypothesis, not a per-adapter concern.

The Rust `InterchainAdapter` is the trait these relayers implement: its
attestation is one of the tiers above → `consensus_verified` (or a
richer rung) → `bridge_mint_against_lock`, carrying a `PortableActionBinding`.

**Lean — the "assume foreign chain finalized X ⟹ dregg mint sound" theorem
ALREADY EXISTS as the §8 CryptoPortal hypothesis.** Extend these, in order:
- `metatheory/Metatheory/SettlementSoundness.lean`: `SettlePred` (`:127`),
  `BindsLiveAuthority` (`:137`, a TYPED hypothesis never an axiom), keystone
  `settlement_soundness` (`:153`), the non-tautological `deployedSettle` (`:289`)
  + `deployedSettle_binds_live_authority` (`:305`), and the vacuity tooth
  `branchSettle_NOT_binds` (`:408`). A foreign-finality settlement is a NEW
  `SettlePred` we prove inhabits `BindsLiveAuthority`.
- `metatheory/Dregg2/Circuit/Spec/bridgeinboundmint.lean`: the §8 CryptoPortal
  hypothesis IS `foreignFinal` — "the other-chain confirmation governs WHEN the
  bridge may move; conservation holds regardless." Reuse `InboundMintSpec`,
  `execBridgeMintA_iff_spec`, and especially `bridgeMint_supply_delta` (a committed
  inbound mint leaves every asset's supply exactly unchanged — the negative-capable
  well absorbs the bridged supply) + the forgery-rejection `bridgeMint_rejects_*`.
  The foreign confirmation enters as a `Prop`-carrier (the `portalStep` pattern,
  `metatheory/Dregg2/Exec/EffectsPaired.lean:392`).
- The double-spend gate's Lean twin: `noteSpend_no_double_spend`
  (`metatheory/Dregg2/Exec/EffectsPaired.lean`) + the accumulator bridge
  (`NullifierAccumulatorKernelBridge.lean`).

The Lean `InterchainAdapter` (`metatheory/Metatheory/Bridge/InterchainAdapter.lean`,
decision layer in `metatheory/Dregg2/Bridge/InterchainAdapterDecision.lean`) is a
structure carrying
`foreignFinal : ForeignHeader → Prop` (= the §8 CryptoPortal `Prop`-carrier: an
assumed oracle for committee/optimistic rungs, a discharged theorem for
light-client rungs like the Solana stake-verified path) + `inclusion` + a
`TrustRung`, with the top theorem an instance of `settlement_soundness` /
`execBridgeMintA_iff_spec` parameterized over the adapter — each chain discharges
its own `foreignFinal` at its rung and inherits credit-soundness + conservation.

## Recommended build order

Grounded in "permissionless seam × shipped precedent × continues existing dregg work":

1. **Lean `InterchainAdapter`** — BUILT: `metatheory/Metatheory/Bridge/InterchainAdapter.lean`
   (decision layer in `metatheory/Dregg2/Bridge/InterchainAdapterDecision.lean`), the
   abstraction through-line. A structure over `foreignFinal`/`inclusion`/`TrustRung`
   instantiating `SettlePred`+`BindsLiveAuthority` and the §8 CryptoPortal
   `bridgeinboundmint` shape, with the four in-tree tiers
   (`LockProofTrust`/`SnarkSystem`/`Verdict`/`FinalizedAttestation`) as its rung
   variants. Model-work, no foreign chain reimplemented — exactly ember's ask.
2. **Hyperlane `DreggProofISM.sol`** (the flagship) — BUILT:
   `chain/contracts/DreggProofISM.sol`, a two-function ISM whose
   `verify()` gates on a `DreggSettlement`-recorded message root + Merkle
   inclusion, AND-ed with a message-id check via an Aggregation ISM (anti-replay).
   THE NOMAD LAW is a hard gate: a test proving `verify` reverts on the
   zero/default input. Routed via `CCIP_READ` moduleType so no relayer change is
   needed. Outbound (dregg is the prover).
3. **LayerZero `DreggDVN` adapter** — BUILT: `chain/contracts/DreggDVN.sol`, an
   on-chain DVN adapter verifying the same settlement proof, registerable
   permissionlessly as a required DVN. Shares the verifier contract with lane 2.
4. **ETH sync-committee inbound light client** (rung 1) — BUILT: `eth-lightclient/`
   is a real Altair sync-committee light client verifying the BLS12-381 aggregate
   G2 signature via blst (sync_aggregate / finality / nomad_law / end_to_end tests
   plus the ethereum bls12-381 KATs); governance's proven-foreign-holding path
   consumes it (`dregg-governance/src/proven_foreign_holding.rs`). NAMED SEAM:
   the bridge's ETH/Base inbound mint listener is not yet wired through it —
   lifting that listener off RPC-trust onto this client, routing its mint through
   `bridge_mint_against_lock`, is the remaining wiring. Inbound (dregg is the
   verifier).
5. **IBC 08-wasm client** (later, deepest) — a CosmWasm ICS-02 client pinning
   dregg's vkey; needs the fork-choice/misbehaviour story (validity ≠ canonicity)
   that lanes 1–4 don't. Sanctioned per-counterparty, not a quick plug-in.

Each outbound lane (2, 3, 5-outbound) rests on the SAME wrap prover
(`docs/deos/ETH-NATIVE-WRAP.md`) and the SAME `DreggSettlement` verifier — one
crypto core, many standard-shaped adapters. Each inbound lane routes through the
one committed consume-once mint gate.

## From day 0 (in code now) vs. roadmap-before-mainnet

**In the tree today, green + tested:**
- dregg's own recursive-STARK finality + light client (`lightclient::verify_history`).
- The gnark wrap (STARK→Groth16 over the FRI verifier) that makes the settlement
  verifier run on EVM: `chain/gnark/` (`fri_verifier.go`, `challenger_bn254.go`,
  `settlement_snark_test.go`) — `chain/` rides it (`docs/deos/ETH-NATIVE-WRAP.md`).
- EVM settlement contract at the real 25-lane proof shape (`DreggSettlement`), with
  a historical proven-root registry; outbound message roots are FAIL-CLOSED refused
  until proof-bound (the residual below).
- A Hyperlane ISM (`DreggProofISM`) and a LayerZero DVN (`DreggDVN`) — both
  Nomad-law-gated, verifying message inclusion under a proven message root (of
  which, today, there are none — fail-closed).
- The Lean `InterchainAdapter` (foreign finality as a hypothesis; credit-soundness
  + conservation composed from the existing settlement/mint theorems, axiom-clean,
  non-vacuous) and the Rust `InterchainAdapter` trait unifying the four trust dials
  into the committed consume-once mint gate.
- The ETH sync-committee inbound light client (`eth-lightclient/`, rung 1): Altair
  sync-committee verification with the BLS12-381 aggregate via blst, Nomad-law and
  end-to-end tested.
- The optimistic + 1-of-N watchtower bridge (Midnight, `Watchtower::examine`) and a
  real Solana Tower-BFT consensus verifier (Rust); a Stripe DECO/zkTLS inbound proof.

**Roadmap before mainnet:**
- **Proof-binding the outbound message root** — the one honest residual below.
- Wiring the ETH/Base inbound mint listener through the built `eth-lightclient/`
  and `bridge_mint_against_lock` (the rung-4 → rung-1 lift; wiring, not a new client).
- IBC 08-wasm client (deeper; needs the fork-choice/misbehaviour story).

## The one honest residual: proof-binding the message root

The adapters gate a message's inclusion under a keccak Merkle **outbound message
root** via `DreggSettlement.isProvenMessageRoot`. The 25-lane proof carries NO
outbound-message commitment, and the contract is FAIL-CLOSED on exactly that fact:
`settle` reverts on any non-zero `outboundMessageRoot` (`MessageRootNotProofBound`)
and `isProvenMessageRoot` returns false for every input
(`chain/contracts/DreggSettlement.sol:46`). An operator-attested registry — record
the settler-supplied root whenever a valid proof lands — would be an operator-trust
hole (the settler could attest an arbitrary root and forge message inclusion
through the adapters); no such path exists. So the proven-STATE half is by-proof;
the message leg is refused, not attested, until the proof binds it.

Keccak is deliberate and boundary-only: it is the native ~30-gas EVM hash, versus
~50-100× for dregg's Poseidon2/BLAKE3 on-chain. This tree is a small purpose-built
commitment over just the span's outbound cross-chain messages — NOT dregg's
note/state tree rehashed — so it neither duplicates the dual-commitment
architecture nor imposes bulk rehashing.

Making the leg fully proof-carrying is a hash-family choice, and it **resolves to
a standard answer** rather than an open decision. The tradeoff is pure
cost-placement: keccak is cheap on the EVM (~30-gas opcode) but expensive inside a
proof; Poseidon2 is cheap inside the proof but expensive on the EVM. Gas is the
*recurring* cost (paid by every verifier, forever); prover time is the *one-time*
cost (paid once per settlement by the operator). So optimize for gas:

**DECISION: a keccak message root, computed inside the gnark BN254 wrap** (NOT the
BabyBear STARK — keccak in a 31-bit field is brutal; in the BN254 wrap it is a
bounded one-time cost), exposed as a wrap public input. This is exactly the
SP1/RISC0 pattern for EVM-friendly output. The EVM then does cheap keccak
inclusion, which is what the adapters already implement.

Crucially this rides the existing wrap: the gnark FRI-verifier wrap is built
(`chain/gnark/`), but its claim lanes carry no message commitment. The remaining
work is threading — the fold's segment accumulator absorbs a per-turn
outbound-message commitment (the turn already commits its emitted effects at
descriptor PI `EFFECTS_HASH`), the apex exposes it as additional claim lanes, the
shrink + gnark `SettlementCircuit` bind those lanes as extra public inputs (a new
Groth16 VK), and `DreggSettlement` then checks `outboundMessageRoot` against the
proof's lanes before recording. Until then the adapters reject all message
inclusion — fail-closed — and nothing else in the adapters changes when the leg
lands. (Alternatives — a Poseidon2 message root, or folding per-message inclusion
into the wrap and exposing the commitment directly — stay on record but lose on
the gas axis for the general many-messages case.)

## Non-goals

- Not reimplementing foreign consensus inside dregg's formal proofs (we model the
  *interaction* + assumption, discharging to a real light client only where we
  build one).
- Not a new bridge token or wrapped-asset custody model (assets stay in the
  shielded pool; see LAYERZERO-CONCEPT-MAP.md — no bridge wallet to vote/settle).
  Governance weight comes from proof-of-holdings over the holder's own account
  (`docs/deos/PROOF-OF-HOLDINGS.md`), never from an escrow.
