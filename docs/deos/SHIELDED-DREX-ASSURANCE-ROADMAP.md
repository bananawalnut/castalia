# Shielded DrEX — Assurance Roadmap

The north star: a DrEX where a batch **clears over hidden commitments** and the **only public
output is a proof that a fair, conserving, valid batch happened** — nobody, not even the operator,
learns who traded, the amounts, or the allocations.

This document maps the Lean/AIR/assurance work between what exists today and that north star. For
each of the six components it gives the honest grade (PROVEN / BUILT / NEEDED), a difficulty
(S / M / L / RESEARCH), dependencies, and a citation. Then a sequence, and the single most-valuable
next step.

Grading legend:
- **PROVEN** — a machine-checked Lean theorem (its *statement* audited, not just `#assert_axioms`-clean).
- **BUILT** — a circuit/AIR or Rust artifact that runs, with tested teeth (both polarities).
- **NEEDED** — the gap, with difficulty and what it depends on.

The whole tree's soundness floor is `HashCR` (Poseidon2 sponge collision-resistance) +
`Poseidon2ChipArithSound`, with the BCIKS20 list-decoding core proved for the deployed code and the
deployed FRI knob-ledger columns reading 112 (arity-2 wrap) / 109 (arity-8 leaf mint,
where ~112.6 provably fails — `FriArityTransfer.arity8_error_not_lt_2e112`) / **51 at the
deployed commit column** (`FriDeployedHeightPairing.deployed_wrap_commitBits`) — ⚑ ledger
readings, NOT proven soundness against an adversary; `FriLdtExtractV3` is assumed. Everything below
inherits that floor for its STARK soundness; component 6 states where the shielded objects sit on it.

> **⚑ PQ posture correction (2026-07-14).** The shielded pool's value-commitment is today a Pedersen
> commitment over Ristretto (`cell-crypto/src/value_commitment.rs`), with a Schnorr excess and a
> Bulletproof range — all **discrete-log**. Pedersen *hiding* is perfect/information-theoretic and the
> STARK privacy path is statistical-ZK (`HidingFriPcs`), so **privacy is quantum-safe**. But Pedersen
> *binding* IS discrete-log, and DLog is **Shor-broken**: a quantum adversary recovers the generators'
> DLog relations, re-opens a value-commitment to a larger value, and forges a conservation-satisfying
> batch that **mints** while privacy hides the theft. So the shielded **value-binding / no-mint
> soundness is NOT post-quantum** — a real hole that contradicts dregg's PQ posture (ML-DSA, quantum-safe
> finality, memory `project-pq-metatheory-connected`), and is **not** covered by the PQ metatheory (whose
> floor `MSIS·MLWESearchHard·SchnorrDLHard·HashCR` uses DLog only as the *classical leg of a hybrid
> signature* with an MSIS fallback — the shielded binding has **no** lattice fallback). Component 2 below
> is rewritten to the PQ-correct target (Poseidon2 hash-commitment + fully-in-AIR STARK conservation,
> retiring DLog); see `docs/deos/PQ-SHIELDED-COMMITMENT.md`. The previously-named "full Ristretto
> EC-in-AIR" item is **deleted** — it entrenches the Shor-broken DLog rather than retiring it.

---

## The six components at a glance

| # | Component | PROVEN | BUILT | NEEDED (gap) | Diff. | Depends on |
|---|-----------|--------|-------|--------------|-------|-----------|
| 1 | Ring-clearing AIR | 2-leg spec + fusion spec (already N-general) | 2-leg tight apex **+ N-leg variable-cycle apex with partial-fill `offer ≥ want_min` in-AIR** | fold the N-leg apex under the deployed VK (tracked as component 6) | — | — |
| 2 | **PQ value-commitment** (Poseidon2 hash + in-AIR STARK conservation) | field⇒integer conservation (no-wrap); Poseidon2 value_binding hiding | `value_binding` hash-commitment, in-AIR conservation gate, `VALUE_BITS` range gadget | migrate note-commitment Poseidon2 (not Ristretto); asset-coord + split/merge equality in-AIR; 64-bit in-AIR range; **retire Pedersen/Schnorr/Bulletproof DLog** | **M** (cutover) / **M–L** (64-bit) | Poseidon2 CR + in-AIR range (exist) |
| 3 | **ZK / reveal-nothing (the crux)** | **`View ≈ Sim∘Q` tractable core** (`Market/RevealNothing.lean`): reveal-nothing law, same-leakage indistinguishability, value-binding hiding, `Q`-faithful simulator shell, teeth — conditional on the NAMED `HidingFriPcs` floor | hiding PCS path (ZK=true), minimal PIs, tested | discharge the **named `HidingFriPcs` statistical-ZK floor** (the deployed-bundle PCS simulator) | **RESEARCH** | the named ZK floor |
| 4 | Membership + nullifier in-clearing | `shielded_spend_claim_refines`; deployed nullifier flip | apex `connect` binds legs to spend leaves | bind ring legs to the **deployed** nullifier accumulator (not per-leg toy pre-state) | **M** | deployed accumulator (exists) |
| 5 | Shielded SetField-attestation | — | — | attested-but-hidden per-trader allocation commitment; resolve SetField cohort ambiguity | **L** | 3, 4 |
| 6 | Composition + deployed-assurance | STARK soundness on real floors; fusion spec-proven | 2-leg apex folds green | fold N-leg apex under deployed VK; retire named EC/range residuals | **M→L** | 1, 2, 4 |

---

## 1. The ring-clearing AIR

**PROVEN.** `Market/ShieldedClearing.lean::shielded_ring_clears` — a shielded ring whose matched
cycle is `CycleValid` and settles through the verified executor is simultaneously conserving (per
asset, real ledger), fair (`RingBalanced` + every leg within its committed offer/want), and
private + no-double-spend (each leg a fresh member spend). `LedgerRealizationExt.lean::
shielded_ring_fused_clears` adds the **fusion** clause `LegFused` (the matcher's `node.offerAsset`/
`offerAmount` ARE a spent note's `asset`/`value`), with the load-bearing tooth
`legA_not_fused` (the un-fused demo leg does NOT satisfy it) and `fusedRing` non-vacuity.

**HISTORICALLY BUILT.** The now-retired `shielded_ring_clearing_air.rs` was the 2-leg, tight-cycle
ring-clearing apex. Enforces in-AIR: fusion gates (`offer_asset==asset`, `offer_amount==value`),
the `CycleValid` 2-cycle edges, the tight amount match (`offer_amount[k]==want_min[(k+1)%2]`),
distinct nullifiers (`nf₀!=nf₁` via inverse witness), Pedersen conservation, and the value-binding
recompute anchoring fusion to the spent note under Poseidon2 CR. The apex `connect`s each leg to a
real `prove_shielded_spend_leaf_with_claim`. Teeth tested: non-conserving, wraparound-mint,
out-of-range, double-spend, mis-fusion, mismatched-fold — all UNSAT.

**HISTORICALLY BUILT (N-leg + partial fill).** The now-retired `shielded_ring_clearing_nleg_air.rs` was the
variable-length-cycle generalization, with the **partial-fill inequality**
`want_min[i] ≤ offer_amount[(i+1) mod N]` enforced in-AIR by the borrow-sub range compare (the
circuit twin of `Dregg2.Bignum.le_iff`), pairwise-distinct nullifiers across all N legs, the same
Poseidon2 value-binding fusion, and Pedersen conservation at N legs
(`ring_conserves_pedersen_list`; `legs_noWrap_conservation` is already k-leg). The Lean side is
N-general (`shielded_ring_clears`/`_fused_clears` quantify over the list), so this is the silicon
at that generality. 3-ring, 4-ring, and partial-fill 4-ring apexes fold and verify green
(`honest_3ring_folds_and_verifies` etc.), with teeth in both polarities: non-conserving,
wraparound-mint, double-spend, mis-fusion, under-`want_min`, and mismatched-fold are all UNSAT.
The module fixes the transcript shape `[nf, root, vb]ⁿ` the reveal-nothing theorem (component 3)
quantifies over.

**Residual:** the N-leg apex under the **deployed VK** — a leaf-wrap config today, tracked as
component 6.

---

## 2. PQ value-commitment — Poseidon2 hash + fully-in-AIR STARK conservation (retire DLog)

**⚑ This component was rewritten (2026-07-14) from "value-commitments in-AIR / full Ristretto
EC-in-AIR" to the PQ-correct target.** The old direction entrenched the Shor-broken Pedersen/Ristretto
discrete-log binding; the right direction retires DLog and lands the shielded value-binding on dregg's
existing PQ floors (`HashCR` / Poseidon2 CR, `Poseidon2ChipArithSound`, `HidingFriPcs` statistical-ZK).
Full diagnosis + migration scope: `docs/deos/PQ-SHIELDED-COMMITMENT.md`. **Options:** A (recommended) =
Poseidon2 hash-commitment + in-AIR STARK conservation; B (fallback) = a Module-SIS lattice homomorphic
commitment (PQ + homomorphic, but kilobyte commitments + a second lattice proof system). A dominates —
it reuses machinery the tree already carries and adds no new cryptographic system.

**PROVEN.** `RealCrypto.lean::twoLeg_noWrap_conservation` + `inAir_conservation_refines_pedersen`
— a field conservation gate `Σ value_in ≡ Σ value_out (mod p)` **plus the range bound** upgrades to
integer conservation. (Today this is stated as refining the group-Pedersen `ring_conserves_pedersen_list`;
under Option A the *same* no-wrap conservation stands on the STARK soundness directly — the target is to
re-anchor this refinement onto the Poseidon2 value-commitment instead of `pedTwoGen`, dropping the DLog
`binding` carrier.)

**BUILT — Option A is largely already present.** The three pieces of the PQ path exist in the codebase:
- **Hash value-commitment.** The shielded-spend circuit publishes a **hiding Poseidon2 commitment
  to `(value, asset_type)` jointly**: `value_binding = hash_fact(value, [asset_type, randomness, 0])` (C7,
  `shielded/spend_circuit.rs:39, 116, 131, 180`; PI `[nullifier, merkle_root, value_binding]`), binding
  = Poseidon2 CR, hiding = the randomness blinder — the same joint `(value, asset_type)` binding the
  three-generator Ristretto commitment provides, on a hash floor. Both ring-clearing apexes re-compute
  this binding in-AIR from the fused value/asset/randomness cells
  (`shielded_ring_clearing_nleg_air.rs:34`, `shielded_ring_clearing_air.rs:36`). This *is* the Option-A
  note-value commitment.
- **In-AIR conservation.** `Σ value_in − Σ value_out = 0` is a BabyBear field gate
  (`shielded_ring_clearing_air.rs`, clause (c)).
- **In-AIR range gadget.** Every conservation value is bit-decomposed into `VALUE_BITS` boolean columns
  with a compile-time `RING_LEGS·2^VALUE_BITS ≤ p` no-wrap assertion
  (`shielded_ring_clearing_air.rs::VALUE_BITS`, `RANGE_TARGET_COLS`), tested by
  `wraparound_mint_ring_is_unsat` / `out_of_range_output_is_unsat`. The AIR header itself notes this
  *"moves the shielded pool's per-output Bulletproof range proof from ATTESTED off-AIR to a CIRCUIT
  constraint."* That is the Option-A range gadget, landed for the BabyBear-scale range.

So the AIR already proves value conservation over hash-committed, in-AIR-ranged STARK witnesses. What is
still DLog is the **redundant off-AIR Pedersen aggregate** (`pedTwoGen` coordinate excess + the off-AIR
Schnorr `prove_asset_conservation` + the off-AIR Bulletproof `pool.rs::output_range_proofs` + the
Ristretto `commit_hidden_asset` bytes). This component is the decision to make the in-AIR hash+STARK path
the *sole* value-binding and **delete the DLog aggregate** — not a from-scratch build.

**NEEDED (the Option-A migration).**
- **Note commitment: Poseidon2, not Ristretto.** Make the on-chain leg the Poseidon2 `value_binding`
  (already a PI, and already binding `(value, asset_type)` jointly — see BUILT above) rather than the
  32-byte compressed Ristretto `commitment_bytes`, which `pool.rs::HiddenAssetLeg` still carries
  (`value_commitment.rs::commit_hidden_asset`). The residual is exactly this on-chain-leg promotion.
  **Difficulty S–M.**
- **Conservation + asset-tag + split/merge equality: fully in-AIR, not off-AIR Schnorr.** Delete
  `prove_asset_conservation`/`verify_asset_conservation` (the Schnorr DLog excess). The value conservation
  is already the in-AIR field gate (c); fold the **asset-tag conservation** in as a second in-AIR field
  sum over the witnessed `asset_type` cells (replacing the `H_asset`-component check), and turn the
  split/merge `AssetEqualityProof` (a Chaum-Pedersen equal-DLog proof) into an in-AIR equality constraint
  over the witnessed asset cells. **Difficulty M** — wiring the asset coordinate into the same in-AIR
  conservation the value coordinate already uses.
- **Range: in-AIR 64-bit, not Bulletproof.** Delete `output_range_proofs` (`bulletproofs`). Extend the
  `VALUE_BITS` gadget to the full 64-bit amount via a multi-limb (Bignum) in-AIR range — the N-leg /
  multi-limb keystone `Dregg2.Bignum.legs_noWrap_conservation` already generalizes the no-wrap proof.
  **Difficulty M–L** (the 64-bit widening is the only real depth).
- **Drop the DLog crates from the shielded path.** Once the above land, `curve25519-dalek`, `bulletproofs`,
  and the Schnorr excess leave the shielded value-commitment TCB. (`ed25519-dalek` stays only as the
  *classical leg of the hybrid signature* — that surface IS covered by the PQ metatheory's hybrid combiner
  with an MSIS fallback; the shielded binding is the one with no fallback, so it is the one to retire.)

**Honest framing:** the *value-minting* soundness hole (the one that lets a batch print money) is CLOSED
in-circuit for the deployed field range — but its *binding floor today is DLog* (the `pedTwoGen`/Pedersen
model), which is **Shor-broken**. Option A re-anchors that same closed no-mint property onto Poseidon2 CR
+ STARK soundness, retiring DLog entirely. Most of the machinery already exists; the residual is a cutover
(delete Pedersen/Schnorr/Bulletproof, promote `value_binding` to the on-chain commitment) plus the 64-bit
in-AIR range widening. The old "full Ristretto EC-in-AIR" item is **deleted** — realizing the DLog curve
point in-circuit would deepen the exact assumption Shor breaks.

---

## 3. ⚑ The privacy / zero-knowledge property — "nobody learns what settled" (THE CRUX)

This is the differentiator, and it must be scoped exactly. The clearing-level theorem exists:
`metatheory/Market/RevealNothing.lean` states, and discharges the **tractable core** of, the
reveal-nothing property on the finalized N-leg transcript `[nf, root, vb]ⁿ` — conditional on one
NAMED floor. Read the grades carefully; the honest claim is conditional, not unconditional.

**The honest statement.** Reveal-nothing is **not** "the transcript is independent of the trades" —
that is false (the transcript reveals that a batch cleared, the price, and the conserved totals).
The honest statement is a simulator over a **leakage functor** `Q`:

    ∃ Sim,  View(clearing) ≈ Sim(Q(clearing))

— the public transcript is *simulatable from the public leakage `Q` alone*, so an observer
(including the operator) learns only `Q` (the clearing price, the batch size, the conserved total,
the public tree root) and nothing about the individual trades (who / value / offer-want /
allocation). The `≈` is statistical for the PCS/hiding layer and computational for the whole
system.

**PROVEN (the tractable core — `Market/RevealNothing.lean`, kernel-clean):**
- `RevealBundle.reveal_nothing` (`View c = Sim (Q c)`) and `view_factors_through_leakage` (the view
  factors through `Q` — the natural-transformation form), with the marquee
  `same_leakage_indistinguishable`: two clearings with the SAME leakage `Q` but DIFFERENT private
  trades produce the SAME transcript.
- **Value-binding hiding** (`HidingValueBinding`): for a blinded commitment the randomness absorbs
  the value (`value_hidden`), with teeth (`leakyVB_not_hiding`: a commitment that ignores its
  blinder leaks the value) — the "the `vb` lane reveals nothing" obligation reduced to a named
  hiding carrier.
- **A `Q`-faithful simulator shell** (`canonicalSim`, `shellBundle`): a concrete witness-free
  transcript generator, proven to emit the right batch size and price from the leakage, on which
  same-leakage indistinguishability holds non-vacuously (`shell_indistinguishable` on two genuinely
  different clearings with equal `Q`).
- **Teeth** — `leaky_no_simulator`: a transcript that leaks a private value verbatim admits NO
  simulator, so the law is a genuine, falsifiable constraint.
- **The bridge** — `RevealBundle.toPerfectZK` transports the bundle onto
  `Metatheory.Open.PerfectZK`, so `reveal_nothing` is literally `view_indep_of_witness` on the
  ring-clearing transcript. (`PerfectZK` and the `Dregg2/Privacy.lean` carriers — Pedersen hiding,
  `unlinkable`, `nullifier_hides_identity` — are the generic machinery this instantiates.)

**BUILT (mechanism present, statistical-ZK is a tested config not a proven theorem):**
- The shielded-spend circuit proves through the **hiding** uni-STARK path (`prove_dsl_zk`,
  `HidingFriPcs`, `ZK = true` — `shielded/spend_circuit.rs`, `shielded/mod.rs`). Value, owner, key,
  Merkle path, randomness, leaf commitment live **only in the witness** under the hiding PCS. The
  circuit exposes exactly **3 PIs**: `[nullifier, merkle_root, value_binding]`, where `value_binding`
  is a *hiding* Poseidon2 commitment to the value (blinded by the note randomness).
- The ring-clearing apexes expose `[nf, root, vb]` per leg — `[nf₀, root₀, vb₀, nf₁, root₁, vb₁]`
  at 2 legs, `[nf, root, vb]ⁿ` at N — and nothing else. All plaintext (values, offer/want,
  out_val/out_blind, range bits) is witness-only.
- The shielded pool (`pool.rs`) hides value + owner + **asset type** jointly (`HiddenAssetLeg`,
  `commit_hidden_asset`), with a transcript (`pool_message`) that binds no cleartext asset type.

**NAMED FLOOR (graded, an explicit structure field — NOT a `sorry`, NOT proven):**

`RevealBundle.reveal_law` carries the reveal-nothing law as a *bundle field*. For the DEPLOYED
bundle — whose `view` is the real Poseidon2/FRI transcript — that field is the **`HidingFriPcs`
statistical-ZK + Poseidon2 hash-hiding + nullifier-unlinkability** floor: the PCS simulator object,
which is not a Lean theorem. The deployed bundle is not constructed; every reveal-nothing
consequence above is *conditional on a bundle satisfying `reveal_law`*, exactly the way the linking
tower's forgery bound is conditional on `HashCR`. `HidingFriPcs` / `ZK = true` is a **tested config
choice, not a proven simulator theorem**; the nullifier-unlinkability reduction to the deployed
nullifier derivation lives inside the same floor. Discharging (or further decomposing and grading)
this floor is the remaining obligation. **Difficulty RESEARCH.**

**Honest one-line grade for component 3:** reveal-nothing at the clearing level is **proven
conditional on the named `HidingFriPcs` statistical-ZK floor** — the statement, the same-leakage
indistinguishability, the value-binding hiding, the `Q`-faithful simulator shell, and the teeth are
machine-checked; the deployed-FRI PCS simulator is the named, un-discharged floor. Do not read this
as "reveal-nothing is proved" unconditionally; the honest claim is "the transcript is simulatable
from public leakage, machine-checked, conditional on the named PCS-ZK floor."

---

## 4. Membership + nullifier in-clearing

**PROVEN.** `ShieldedClearing.lean::ShieldedLeg.refines` (from `shielded_spend_claim_refines`) — a
valid shielded-spend claim `[nullifier, merkle_root, value_binding]` refines a sound VM step:
AUTHORIZED (spends a real committed member note) + NO-DOUBLE-SPEND (fresh → joins spent set → never
re-spendable). The deployed nullifier flip (memory `project-vk-epoch-nullifier-flip`) forces
membership + freshness at the deployed descriptor (`noteSpendVmDescriptor2R24`, sorted-Merkle
accumulator, `nf ∉ pre.nullifiers` via `absent` + `aafi_insert`).

**BUILT.** The apex binds each ring leg to a real spend leaf via in-circuit `connect`
(`bind_leg_node`); a leg claiming a tuple no verifying spend backs is a `connect` conflict ⇒ UNSAT
(`mismatched_fold_does_not_bind`, `forged_spend_leg_never_mints_a_leaf`).

**NEEDED.** Bind the ring's legs to the **deployed** nullifier accumulator inside the clearing AIR.
Today each leg's `pre`/`post` is a per-leg toy `ShieldedState`; the freshness is proved against that
local state, not against the one deployed sorted-Merkle accumulator that the rest of the system
advances. The fusion at the spend-leaf is real; the *accumulator threading* — the clearing apex
consuming and advancing the single canonical nullifier root — is the weld. **Difficulty M.** Depends
on the deployed accumulator (exists) + the observer-threading residual named in the nullifier-flip
memory (item-3: `ShadowNullifierAccumulator` into `WireState`). Also open there: in-STARK
non-membership bound to the committed limb (light-client attestation).

---

## 5. Shielded SetField-attestation

**Grade: NEEDED (design).** Neither Lean spec nor circuit exists for the shielded allocation.

The goal: settle the **per-trader allocation attested-but-hidden** — prove a valid allocation
happened (each trader received exactly its cleared amount of its wanted asset) **without revealing
the allocation**. Today the clearing proves the *aggregate* is fair and conserving; the individual
trader→allocation map is not itself committed-and-hidden as a first-class object.

**NEEDED.** A shielded allocation-commitment settlement: each trader's post-batch position as a
hidden commitment, with a proof it equals `receivedAmount`/`receivedAsset` from the cleared cycle
(the fair-clearing already proves those quantities — `clearing_respects_limits`,
`cycle_individuallyRational`). Plus resolving the **SetField cohort ambiguity** (named): which
cohort a SetField write attributes to, so an allocation cannot be silently reassigned. **Difficulty
L.** Depends on 3 (the hiding property must be stated before "attested-but-hidden allocation" is
meaningful) and 4 (allocations land as note-creates against the accumulator).

---

## 6. Composition + deployed-assurance

**PROVEN.** The AIRs are STARK-sound on the real floors (`HashCR` + `Poseidon2ChipArithSound`,
BCIKS20 transcribed for deployed code; the FRI ledger's weakest deployed column is **51**
(`FriDeployedHeightPairing.deployed_wrap_commitBits`) — ⚑ a ledger reading, not a proven
bound against an adversary, and ~112.6 provably fails at the arity-8 mint
(`FriArityTransfer.arity8_error_not_lt_2e112`)). The fusion is spec-proven end-to-end
(`shielded_ring_fused_clears`), and the in-AIR conservation refines the Lean group-Pedersen
conservation (`inAir_conservation_refines_pedersen`) — the deployed field gate forces what the Lean
proves for the value-mint hazard (no toy stand-in: the `refVC` additive toy and `refTreeRoot` linear
hash are *retired* by `RealCrypto.lean`).

**BUILT.** The 2-leg apex folds and verifies green (`honest_shielded_2ring_folds_and_verifies`), the
N-leg apex folds and verifies at 3 and 4 legs including a partial-fill ring
(`honest_3ring_folds_and_verifies`, `honest_partial_fill_4ring_folds_and_verifies`), and the cleared
claim round-trips.

**NEEDED / named residuals (what is NOT yet clean):**
- The N-leg apex under the **deployed VK** (composition 1 + 6): both apexes (2-leg and N-leg) fold
  as leaf-wrap configs, not under the deployed epoch VK. **Difficulty M→L.**
- **⚑ PQ residual (real, not just resolution).** The shielded value-**binding** today rests on DLog
  (Pedersen/Ristretto + Schnorr excess + Bulletproof range, `cell-crypto/src/value_commitment.rs`,
  `pool.rs`), which is **Shor-broken** — a genuine hole in the PQ posture, NOT covered by the PQ
  metatheory (component 2, `PQ-SHIELDED-COMMITMENT.md`). The fix is the Option-A cutover to the Poseidon2
  hash-commitment + fully-in-AIR STARK conservation (`value_binding` + the in-AIR conservation gate + the
  `VALUE_BITS` range, most of which is built), retiring DLog. The 64-bit in-AIR range is the remaining
  depth. Privacy is already quantum-safe (perfect hiding + statistical ZK) and needs no change.
- The uniform-price/optimality layer is MODEL-proved, not ledger-realized (DREGGFI-VISION §7,
  ZK-AUCTION-SUITE §8) — individual-rationality fairness IS ledger-realized; say which.

---

## Sequence — what unlocks what

```
        ┌──────────────────────────────────────────────────────────────┐
        │  Component 3: View ≈ Sim∘Q — tractable core PROVEN           │
        │  (Market/RevealNothing.lean); remaining = discharge the      │
        │  NAMED HidingFriPcs statistical-ZK floor (RESEARCH)          │
        └───────────────▲──────────────────────────────▲──────────────┘
                        │ transcript fixed (built)      │ named floor
   ┌────────────────────┴───┐                  ┌────────┴─────────────┐
   │ 1. N-leg ring AIR      │                  │ 6. deployed-VK fold  │
   │    BUILT (+ ≥ in-AIR)  │──── app wiring   │    + retire residuals│
   └────────────────────────┘                  └──────────────────────┘
   ┌────────────────────────┐   ┌───────────────────────┐
   │ 4. accumulator bind (M)│   │ 2. PQ value-commitment │
   │    (deployed nullifier)│   │  hash+in-AIR (M), retire│
   │                        │   │  DLog; 64-bit range(M-L)│
   └────────────────────────┘   └───────────────────────┘
                                  └── 5. shielded allocation (L, after 3+4)
```

- **1 (N-leg AIR)** is BUILT (`shielded_ring_clearing_nleg_air.rs`) with the partial-fill inequality
  in-AIR; it fixes the transcript component 3 quantifies over. Its deployed-VK fold is component 6.
- **4 (accumulator bind)** makes the double-spend gate deployed-real, not per-leg toy. Independent, M.
- **2 (PQ value-commitment)** is a *posture-alignment* item, NOT a faithfulness upgrade: the shielded
  value-binding is DLog (Shor-broken) today, so this retires DLog onto Poseidon2 CR + in-AIR STARK
  conservation. Most is built (`value_binding`, in-AIR conservation, `VALUE_BITS` range); the residual is
  a cutover + the 64-bit in-AIR range. The old "Ristretto EC-in-AIR" item is deleted (it entrenched DLog).
- **3 (the ZK theorem)** has its statement and tractable core proven (`Market/RevealNothing.lean`);
  what gates the honest unconditional "nobody learns what settled" claim is the named
  `HidingFriPcs` statistical-ZK floor.
- **5 (shielded allocation)** waits on 3+4.

---

## The single most-valuable next step

**Two, in priority order:**

1. **The PQ value-commitment cutover (component 2).** A real security residual, not a mere
   faithfulness upgrade: the shielded value-binding is discrete-log (Shor-broken) today. Most of the
   Option-A machinery is built (`value_binding`, the in-AIR conservation gate, the `VALUE_BITS`
   range); the work is the cutover (delete Pedersen/Schnorr/Bulletproof, promote `value_binding` to
   the on-chain commitment) plus the 64-bit in-AIR range widening. On the posture-critical path.

2. **The accumulator bind (component 4).** Binding the ring legs to the deployed sorted-Merkle
   nullifier accumulator turns the per-leg toy freshness into the deployed double-spend gate.
   Difficulty M; the accumulator exists.

The crux's remaining depth — discharging the named `HidingFriPcs` statistical-ZK floor (component 3)
— stays RESEARCH-grade and is the item that would turn the conditional reveal-nothing theorem into
an unconditional one. The deployed-VK fold of the N-leg apex (component 6) and the shielded
allocation (component 5) follow.

---

## Honest 4-line summary

1. **PROVEN:** the private-matching *clearing spec* is machine-checked and non-vacuous — a shielded
   ring clears conserving + fair + no-double-spend + **fused** to real hidden notes
   (`shielded_ring_clears`, `shielded_ring_fused_clears`), over real Pedersen (DLog binding) and real
   Poseidon2 (sponge CR), toys retired; the value-mint / wraparound hole is closed *in-circuit*.
2. **BUILT:** both ring-clearing apexes fold green with tested teeth — the 2-leg tight-cycle apex
   and the **N-leg variable-cycle apex with the partial-fill `offer ≥ want_min` inequality in-AIR**
   (`shielded_ring_clearing_nleg_air.rs`; non-conserving, wraparound, double-spend, mis-fusion,
   under-`want_min`, mismatched-fold all UNSAT) — proving through the hiding PCS with only
   `[nf, root, vb]ⁿ` exposed and all plaintext witness-only.
3. **THE CRUX (component 3) is proven at its tractable core, conditionally:**
   `Market/RevealNothing.lean` machine-checks the `View ≈ Sim∘Q` reveal-nothing law,
   same-leakage indistinguishability, value-binding hiding, a `Q`-faithful simulator shell, and
   teeth on the N-leg transcript — **conditional on the named `HidingFriPcs` statistical-ZK floor**
   (the deployed-FRI PCS simulator, not a Lean theorem). Discharging that floor is the frontier.
4. **Also NEEDED:** binding legs to the *deployed* nullifier accumulator (M), **the PQ
   value-commitment cutover — the shielded binding is discrete-log (Shor-broken) today; retire
   Pedersen/Schnorr/Bulletproof onto the Poseidon2 hash-commitment + fully-in-AIR STARK conservation
   (M, mostly built) + a 64-bit in-AIR range (M–L)**, the N-leg apex under the deployed VK (M→L),
   and the shielded attested-but-hidden allocation (L). Honest posture: the AIRs and the spec are
   real; shielded *privacy* is quantum-safe (perfect hiding + statistical ZK) but shielded
   *value-binding* is NOT post-quantum (a real posture hole, `PQ-SHIELDED-COMMITMENT.md`); and the
   reveal-nothing theorem is conditional on its named PCS-ZK floor — do not overclaim either.
