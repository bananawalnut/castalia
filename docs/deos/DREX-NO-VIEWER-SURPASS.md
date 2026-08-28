# DrEX No-Viewer Surpass — the FHE-Clearing Path

**What this is.** A research + design note asking one question honestly: can DrEX *genuinely*
surpass every extant private-DEX design on the one axis they all fail — **no party ever sees the
order book** — and if so, by what mechanism, at what real cost, on what timeline. The thesis under
test: a DrEX **batch uniform-price auction** is the one clearing mechanism FHE can plausibly
compute, so DrEX can do **FHE-clearing (nobody decrypts the orders) + a STARK of correctness**,
giving a **no-viewer, post-quantum, verifiable** private DEX.

The answer, up front, is not hype: the thesis **holds — but only for the uniform-price batch, not
for the ring/TTC clearing that DrEX's built AIR actually implements**, and the near-term win is
smaller and cheaper than "verifiable FHE" makes it sound, because the FHE evaluation on *public
input ciphertexts is deterministic and therefore publicly re-checkable for free*.

---

## 5-line summary (the gate)

1. **Is FHE-clearing feasible for a batch auction, and at what size?** Yes, for the *uniform-price*
   mechanism (comparison/monotone-structured), at **N ≈ 32–512 orders per batch on one server at
   minute cadence** — published FHE sorts land 128 real elements in ~22 s (CKKS) and 64 in ~36 s
   (rank-sort); a full uniform-price clear is that plus a prefix-scan. It **breaks** at N in the
   thousands (tens of minutes → hours) and for exact-integer TFHE (slower than approximate CKKS).
2. **The real timeline.** A *non-verifiable* FHE-clearing prototype (tfhe-rs/CKKS + Zama threshold
   decryption) is buildable in ~**6–12 months**; a threshold-decryption-correctness proof rung in
   ~**1–2 years**; a *succinct verifiable-FHE* STARK for light clients is research-grade,
   ~**2–4 years**; production-hardened PQ FHE DEX ~**3–4 years**.
3. **The mechanism catch (load-bearing).** DrEX's *built* clearing is a multilateral **ring / TTC
   cycle** (graph-hard cycle enumeration — `solver.rs`, `shielded_ring_clearing_air.rs`), which is
   **NOT FHE-tractable**. The FHE rung must ride DrEX's *uniform-price* layer, which dregg has
   Lean-proved optimal/envy-free but only at the **model level** (`Market/Optimality.lean`), not
   ledger-realized. So the surpass requires a mechanism switch, not just an FHE port.
4. **The cheap-verifiability insight.** Because the FHE circuit is deterministic and the input
   ciphertexts are public commitments, correct *evaluation* needs no expensive verifiable-FHE — any
   observer re-runs the public circuit and checks the output ciphertext. Only the final *threshold
   decryption* of the public clearing price needs a proof. This de-risks the hard part.
5. **The honest surpass claim.** *"DrEX-FHE is the first DEX whose no-viewer is ADVERSARIAL: orders
   are folded under encryption and the crossing runs in output-boundary MPC among the federation
   parties, revealing only `(p*,V*)`. Below the `t`-of-`n` threshold no coalition of parties learns
   any order — a cryptographic bound, not a policy — and there is no standing master decryption key;
   `≥ t` colluding parties can reconstruct (the honest caveat: 'nobody even if all collude' is
   impossible for clearing over hidden data). Clearing correctness is a post-quantum STARK."* True at
   each rung as stated below, never overclaimed (and never underclaimed — it IS cryptographic, not
   just policy). **Verdict: the FHE thesis survives real numbers, conditioned on switching to the
   uniform-price mechanism and accepting a multi-year ladder — the batch structure is what makes it
   possible at all, and output-boundary MPC (`OUTPUT-BOUNDARY-MPC.md`) is what makes the no-viewer
   adversarial rather than a committee's discretion.**

---

## 1. Why every extant private DEX has a viewer (and DrEX today too)

`docs/deos/DREX-DESIGN.md` already names the field's stuck trust point per design: CoW = the
permissioned solver (orders visible to solvers), Penumbra ZSwap = the validator threshold committee,
dYdX = the block proposer's off-chain match, Shutter = the Keyper committee, Aztec = the sequencer.
Every one delegates the clearing/ordering step to a party that **sees the orders** (or an aggregate
of them) and, classically, could be broken by a quantum adversary at the crypto layer.

DrEX's own ladder, read from code, is a *viewer-shrink* sequence — not yet a viewer-delete:

| Rung (code) | Who sees the orders | Grade |
|---|---|---|
| `intent/src/solver.rs`, `matcher.rs` | the solver, in **cleartext** (full book) | transparent |
| `intent/src/trustless.rs` (7-layer batch) | a **t-of-n threshold-decryption committee** (Shamir/GF(256)) after batch close | BUILT — threshold-*decrypt*, not MPC-*compute* |
| `tee-verify/src/` (Nitro / SEV-SNP) | a **single attested enclave** binary (`measurement`) | ATTESTED, not PROVED |
| `metatheory/Market/ShieldedClearing.lean` + ring AIRs | **nobody** (matching over hidden commitments) | PROVEN spec / BUILT circuit — but see §4 |

The shielded rung already deletes the viewer for the *ring* clearing: `shielded_ring_clears`
(`ShieldedClearing.lean:164`) proves a ring clears conserving + fair + no-double-spend over hidden
commitments, exposing only `[nullifier, root, value_binding]` per leg; the built AIRs
(`shielded_ring_clearing_air.rs`, `_nleg_air.rs`) realize it. **So DrEX can already clear with no
viewer — for the ring mechanism.** The gap this note addresses is different and specific: the ring
is graph-hard and cannot be computed under FHE, and the *uniform-price* mechanism the literature
calls the frontier is the one FHE *can* compute but dregg has not yet realized under encryption.

---

## 2. FHE feasibility for a batch auction — real numbers

### 2.1 What the clearing actually needs

A **uniform-price call auction** clears at a single price `p*` where cumulative demand meets
cumulative supply, with every filled order transacting at `p*` (envy-free; the rule dregg proved
in `Market/Optimality.lean`: `uniform_price_no_arbitrage` / `uniform_price_envy_free` /
`uniform_price_optimal`). The clearing is **comparison- and monotone-structured**:

- Either **sort** bids (descending price) and asks (ascending price), then a prefix-scan finds the
  crossing — `O(N log²N)` compare-exchanges (bitonic), each = 1 comparison + 2 selects; or
- **Scan candidate prices** (the clearing price is always one of the submitted limits): for each of
  N candidate prices compute encrypted cumulative buy/sell volume and find the crossing —
  `O(N²)` independent comparisons, fully parallelizable.

This is exactly the operation profile FHE is *least bad* at: encrypted integer **comparison** and
**select**, plus additive accumulation. There is no data-dependent control flow, no pointer chasing,
and latency-tolerance is inherent (a periodic batch can take minutes). That is the whole thesis, and
it is correct about the *shape*.

### 2.2 Per-operation cost (cited, current libraries)

TFHE-rs (Zama), default params `PARAM_MESSAGE_2_CARRY_2_KS32_PBS_TUNIFORM_2M128` (≥128-bit,
IND-CPA-D, bootstrap failure ≤ 2⁻¹²⁸), on AWS `hpc7a.96xlarge` (AMD EPYC 9R14 @ 2.60 GHz), CPU:

- `FheUint16` equality ≈ **31 ms**; `FheUint16` min ≈ **96 ms** [Zama CPU integer benchmarks].
- Greater-than / `ge` sits between these; a compare-exchange (compare + conditional swap) is
  ≈ **3 FHE ops ≈ 150–300 ms** per pairwise step at 16-bit precision on one core.
- **H100 GPU** and the FPGA "HPU" cut this roughly an order of magnitude (Zama publishes GPU/HPU
  tables on H100; exact per-op figures not reproduced here — treat as ~10–30× CPU).

FHE is **lattice/LWE**, hence **post-quantum by construction**: TFHE/BGV/BFV/CKKS all rest on
Ring-/Module-LWE, the same hardness family as NIST ML-KEM (FIPS 203) / ML-DSA (FIPS 204). This is
the PQ leg of the thesis and it is unambiguous [FHE textbook; LWE/PQC literature].

### 2.3 End-to-end clearing envelope (published sort benchmarks = ground truth)

Rather than multiply per-op estimates (which understate real bit-precision costs), the honest anchor
is published *end-to-end encrypted sorts*, which already bundle the comparisons a clearing needs:

| N (orders/batch) | Encrypted sort/clear, one server | Source |
|---|---|---|
| 4 | ~0.2 s | rank-sort, real numbers (ePrint 2025/1170) |
| 64 | **~36 s** (rank-sort) — vs 409 s bitonic (PEGASUS) | ePrint 2025/1170 / 2021/551 |
| 128 | **~22 s** (CKKS approximate, precision 1e-3, 3 GB) | CiC 3(1):32 "Lightweight sorting in CKKS" |
| 512 | ~single-digit **minutes** (extrapolated `N log²N`, GPU) | derived from above |
| thousands | **tens of minutes → hours** — breaks the minute cadence | — |

Reading this honestly:

- **N = 32–128 clears in tens of seconds** on one modern server today. A batch auction with a
  1–5 minute cadence is comfortably FHE-tractable at this size **right now**, with existing
  open-source libraries (tfhe-rs, or a CKKS stack for the approximate variant).
- **N = 512 is single-digit minutes** with GPU acceleration — feasible for a latency-tolerant batch,
  tight but real.
- **N in the thousands breaks the model** — the clearing no longer fits a minute cadence. The escape
  is **sharding the book by pair** (each trading pair is an independent uniform-price clear, so N is
  per-pair, not global) and **coarse price ticks** (fewer candidate prices to scan).
- **Exact-integer TFHE is slower than approximate CKKS.** A DEX needs exact conservation, which
  argues for TFHE (exact) with a range/precision discipline, paying the higher constant — or CKKS
  for price discovery with an exact-integer settlement pass. This is a real design fork (§5).

**Verdict on 2:** the batch-auction-is-FHE-tractable claim is **TRUE at useful sizes (N ≤ a few
hundred per pair, minute cadence)** and the batch mechanism is *precisely* what makes it so — a
continuous limit-order book, with its per-tick data-dependent matching, is not FHE-computable at any
useful rate. The mechanism choice is load-bearing, and the thesis got it right.

---

## 3. Threshold-FHE — shrinking the committee to "decrypt the price"

Under FHE the committee never computes on plaintext; it holds only **shares of the decryption key**
and is invoked once per batch to threshold-decrypt the **public result** (the clearing price, and the
public per-order fill quantities that a uniform-price auction outputs by design). It never touches an
individual order.

State of the art [Zama TKMS / `zama-ai/threshold-fhe` / NIST-TC submission]:

- Zama ships a **Threshold Key-Management System (TKMS)** with threshold versions of **TFHE, BGV, and
  BFV**, and a 250+-page technical spec. Current committee ≈ **13 MPC nodes, honest-majority**, with
  a published roadmap to **~100+ nodes** (HSM integration, PQ ZK on the decryption). Testnet live;
  Zama's fhEVM mainnet on Ethereum targeted end-2025.
- **⚠ Correction — plain threshold-FHE is NOT no-viewer against committee collusion.** A plain
  threshold-FHE committee holds shares of a decryption key that decrypts *any* ciphertext. So a
  colluding `≥ t` subset can apply the decryption protocol to a submitted **order** ciphertext and
  read it. Under plain threshold-FHE, "the committee only decrypts `p*`" is a **policy** statement
  (they choose to), not a cryptographic guarantee — FHE hides inputs from a party *without* the key,
  but the committee *has* the key. This is the codex Round-4 threshold-trust correction, and it is
  why the no-viewer posture is delivered by **output-boundary MPC** (`OUTPUT-BOUNDARY-MPC.md`), not
  by plain threshold decryption: the parties partial-decrypt only the aggregate INTO additive shares
  for one clearing and open only `(p*,V*)`, with **no standing master key** against order
  ciphertexts. The bound is then `t`-of-`n` and adversarial: *below* the threshold no coalition
  learns any order (cryptographic, not policy); `≥ t` colluding parties still can (the honest
  ceiling — "nobody even if all collude" is impossible for clearing over hidden data). A dishonest
  subset can also force a *wrong* price (a liveness/integrity fault caught by the correctness proof
  in §4); privacy (below `t`) and correctness are stated separately.
- **IND-CPA-D caveat:** threshold decryption leaks a little through decryption noise; the deployed
  params must be the noise-flooded IND-CPA-D-secure set (tfhe-rs default already targets this).

So threshold-FHE shrinks the *decryption* to **"open the public clearing price,"** but the no-viewer
guarantee against colluding parties is the **`t`-of-`n` output-boundary-MPC bound**, not a property of
the decryption committee alone. That is the honest crux of the surpass.

---

## 4. The proof of correctness — and the cheap path most miss

How does the public verify the FHE-clearing was correct **without seeing the orders**? There are
three options, and the cheapest is the one the thesis under-weights:

**Option A — deterministic public re-evaluation (cheap, buildable now).** The FHE circuit is
*deterministic*, and the input ciphertexts (the encrypted orders) are **public commitments** posted
to the batch. Therefore any observer can **re-run the identical public FHE circuit on the identical
public input ciphertexts and check the output ciphertext byte-for-byte** — no ZK needed for the
evaluation at all. Correctness of the *clearing computation* is free by determinism. The only thing
that still needs a proof is the **threshold decryption** of the final public result: a
threshold-decryption-correctness proof (each share proven consistent with the committee's public
verification keys), which is a small, well-understood ZK statement. This collapses the "verifiable
FHE" problem to "verifiable threshold decryption + public re-evaluation," which is **buildable in
~1–2 years**, not research-grade.

**Option B — succinct verifiable-FHE STARK (research-grade, for light clients).** If a light client
will not re-run the whole FHE circuit, you need a succinct proof that the committed circuit,
evaluated on the committed input ciphertexts, yields the committed output ciphertext. This is the
**SNARK-FHE** paradigm and it is expensive: proving even a single TFHE bootstrap is heavy
[HELIOPOLIS, ASIACRYPT 2024; "Proving correct TFHE bootstrapping using plonky2," ePrint 2024/451;
"Verifiable FHE via lattice SNARKs," CiC 1(1):24]. Recent FHE-friendly SNARKs help — **Phalanx**
(CCS 2025) reports 3× lower multiplicative depth than FRI-based SNARKs, 61.4 MB proofs, 2.8 s verify
— but proving a *full clearing circuit's* FHE ops succinctly is still 2–4 years out. **This composes
with dregg's existing STARK stack**: the clearing-correctness relation dregg proves in
`ShieldedClearing.lean` (conservation, fairness, no-double-spend) is exactly the *statement* such a
STARK would carry; the new work is proving it over *FHE-evaluated* rather than *plaintext-witnessed*
values.

**Option C — input well-formedness proofs (required regardless).** Independently of A/B, each
encrypted order must ship a **ZK proof of plaintext knowledge + range** (the ciphertext encrypts a
valid, in-range order and the trader is funded) — otherwise a malformed ciphertext poisons the
clear. Zama's stack already uses ZKPoK on FHE inputs; this is standard and additive.

**Design recommendation:** ship **Option A + C first** (deterministic re-evaluation + input ZKPoK +
threshold-decryption proof), which gives full public verifiability today without research-grade
verifiable-FHE, and add **Option B** later purely for light-client succinctness. This is the single
most important de-risking in this note: *DrEX does not need to solve verifiable-FHE to get a
no-viewer verifiable clear.*

---

## 5. The FHE-clearing + STARK architecture for DrEX

**Scope first (honest).** The FHE rung clears the **uniform-price single-pair batch** (or N
independent per-pair batches). The **multilateral ring / TTC** clearing that DrEX's `solver.rs` and
`shielded_ring_clearing_air.rs` implement is a graph problem — cycle enumeration (Johnson +
Shapley-Scarf TTC) over an encrypted compatibility graph — which is **not FHE-tractable** and stays
on the classical shielded-STARK path. So the FHE surpass is *the uniform-price product*, and the
ring remains a separate (already no-viewer, already PQ-private) product. Do not conflate them.

**The pipeline (one batch, one pair):**

1. **Submit.** Traders post `Enc(price, qty, side)` ciphertexts (TFHE for exact settlement, or CKKS
   for approximate price discovery) + an input ZKPoK (Option C) + a funding commitment. Ciphertexts
   are public, committed into the batch root. Nobody can read an order.
2. **Clear (FHE).** A public, deterministic FHE circuit computes the crossing price `p*` and each
   order's fill quantity, entirely on ciphertexts. Cost = §2.3 (tens of seconds at N ≤ 128). Output:
   `Enc(p*)`, `Enc(fill_i)`. No party sees any input.
3. **Verify-evaluation (Option A).** Anyone re-runs the identical circuit on the identical public
   input ciphertexts and checks the output ciphertexts match. Free by determinism.
4. **Cross-at-the-boundary (output-boundary MPC).** Rather than threshold-decrypt the curve, the `n`
   parties partial-decrypt only the aggregate INTO additive shares and run the crossing in MPC,
   revealing **only** `(p*, V*)` — never an order, never a curve coefficient, and with **no standing
   master key** against order ciphertexts (`OUTPUT-BOUNDARY-MPC.md`). This is the adversarial-no-viewer
   step (a `t`-of-`n` bound, not a policy), and it dissolves the BFV→TFHE scheme-switch seam. (The
   uniform-price fills are public outputs by design.)
5. **Settle + prove (dregg STARK).** Fills settle through the verified executor kernel
   (`verified_settle.rs` → `recKExecAsset`), and the clearing carries the **existing dregg STARK
   statement** — conservation + fairness + no-double-spend, the relation `shielded_ring_clears`
   already proves — now instantiated for the uniform-price mechanism (`uniform_price_optimal` from
   `Market/Optimality.lean`, which currently lives only at the model level and must be
   ledger-realized). The public transcript is `{batch root, p*, fills, decryption proof, clearing
   STARK}` — and the reveal-nothing property is the same **Component 3 "crux" theorem** the shielded
   roadmap already names as RESEARCH (`SHIELDED-DREX-ASSURANCE-ROADMAP.md`).

**PQ posture the FHE must match.** The **Option-A cutover of `docs/deos/PQ-SHIELDED-COMMITMENT.md`
is landed** (its §5): the authoritative value-commitment is the Poseidon2 `hash_fact` value-binding
(`HashCR`; retired `spend_circuit.rs::value_binding`, re-computed in-AIR by both
ring AIRs), conservation is the in-AIR field gate, and the Schnorr/Bulletproof DLog path is retired
from the TCB. So FHE (lattice/LWE, PQ) + STARK (Poseidon2 / FRI, PQ) + hash-commitment (PQ) already
compose post-quantum on the settlement path. The PQ residual the FHE rung must respect is the
**Option-B aggregation-fold carrier** — the PQ-additive commitment for aggregating
independently-produced commitments, a separate named frontier — plus the 64-bit in-AIR range
widening. The residual is the aggregation fold, not the value-binding.

---

## 6. The honest incremental ladder

Each rung is a real, shippable step with an honest grade, difficulty, and rough timeline. The
no-viewer claim is stated so it is **true at that rung**.

| Rung | Mechanism / what dregg has | Who sees orders | Fair-by-proof? | PQ? | Difficulty · timeline |
|---|---|---|---|---|---|
| **1. Trusted/bonded solver** (current DrEX) | `solver.rs` ring matcher + `shielded_ring_clearing_air.rs` STARK; `trustless.rs` bonds/batch | Solver sees cleartext book (rung-1); shielded rung already deletes viewer *for the ring* | **Yes** — clearing is a machine-checked STARK (`shielded_ring_clears`) | Proof PQ (Poseidon2/FRI); value-binding PQ (Poseidon2 `HashCR`, Option-A cutover landed) | Shippable **now** |
| **2. TEE-attested solver** | `tee-verify/` (Nitro + SEV-SNP), `attest_data`, `oracle_mark` | **One attested enclave** binary (`measurement`), offline-checkable | Yes (same STARK) + enclave identity | Attestation sigs ECDSA-P384 (classical); order privacy PQ if FHE-inside | **Low** · built; integration weeks–months |
| **3. Threshold-decrypt / MPC clearing** | `trustless.rs` 7-layer (Shamir/GF(256) threshold-*decrypt* — BUILT); true MPC-*compute* (Renegade-style, no single viewer) — **NEEDED** | Threshold-decrypt: committee sees book after batch close. MPC-compute: no single party (secret-shared) | Yes (STARK on the solve) | Threshold-decrypt classical; can be PQ | **Medium** · decrypt built; MPC-compute ~1–2 yr |
| **4. Additive fold + output-boundary MPC** (the surpass) | fold PoC + MPC-crossing PoC BUILT (`fhegg-fhe/`, `OUTPUT-BOUNDARY-MPC.md`); rides `Market/Optimality.lean` uniform-price (model-proved) + threshold partial-decrypt-into-shares + the landed Option-A PQ-commitment cutover | **Nobody below `t`** — parties hold only additive shares; only `(p*,V*)` open; no standing master key; `≥ t` collusion reconstructs (honest ceiling) | Yes — Option A re-eval + STARK boundary check (comparator outside TCB) | **Fully PQ** (LWE-FHE + Poseidon2/FRI + hash-commit) | **Med-High** · PoC now; production partial-decrypt-into-shares + malicious-secure online 1–2 yr; succinct 2–4 yr |

**What each rung needs from here:**

- **Rung 2:** wire `tee-verify` around the solver so the enclave runs the match; view shrinks to
  `measurement`. Grade honestly **ATTESTED, not PROVED** (HW vendor root + side-channel residual).
- **Rung 3:** to reach *no single viewer* you must build **MPC-compute** (secret-shared clearing) —
  `trustless.rs` today is threshold-*decrypt*, so the committee still sees the book after close.
  This is the Renegade comparison; it is real work, not a wiring job.
- **Rung 4:** switch the mechanism to **uniform-price** (ledger-realize `uniform_price_optimal`),
  run the fold under the additive BFV carrier and the crossing in **output-boundary MPC** among the
  federation parties (`OUTPUT-BOUNDARY-MPC.md` — the fold + MPC-crossing PoC is built and measured:
  sub-10 ms fold, ~1–7 ms crossing, correctness == plaintext, reveal-only-`(p*,V*)` demonstrated),
  ship Options A + C (the Option-A PQ-commitment cutover is landed). The remaining production work is the threshold
  partial-decrypt-into-shares + the malicious-secure online phase; the assurance ladder is the
  multi-year part, but the no-viewer is now an *adversarial threshold bound*, not a policy.

---

## 7. The precise surpass claim (true at each rung, never overclaimed)

> **DrEX-FHE (rung 4) is a DEX whose no-viewer is ADVERSARIAL, bounded by a `t`-of-`n` threshold.
> Orders are folded under encryption; the crossing runs in output-boundary MPC among the `n`
> federation parties, revealing only the clearing price `p*` and cleared volume `V*`. BELOW the
> threshold `t`, no coalition of parties learns any order or curve coefficient — a cryptographic
> bound, not a policy — and there is NO standing master decryption key against order ciphertexts. `≥
> t` colluding parties CAN reconstruct (the honest ceiling). Correct clearing is a post-quantum STARK
> an observer re-checks; the committee is optional at the two lower rungs.**

Contrast, stated fairly:

- **Penumbra (ZSwap):** classical threshold committee decrypts the batch **aggregate** (individual
  amounts hidden, but a committee decrypts, and the crypto is discrete-log — not PQ). DrEX-FHE keeps
  even the *aggregate demand curve* in shares; only `(p*,V*)` open; below threshold that is
  cryptographic, not committee discretion; PQ.
- **Renegade:** an MPC committee **jointly computes on secret-shared orders** — collectively holds
  the orders, interactive, liveness-bound, classical. DrEX-FHE folds under encryption and runs the
  *crossing only* in MPC over the aggregate (not the orders), opening only `(p*,V*)`; PQ.
- **Aztec:** the sequencer sees. DrEX-FHE has no sighted sequencer.
- **CoW / Shutter:** solver / Keyper committee sees. DrEX-FHE has neither.

**Honesty guards on the claim:** (a) it is *rung-4* — rungs 1–3 still have a viewer of decreasing
size, and the doc says so; (b) it holds for the **uniform-price** mechanism, not the ring/TTC, which
stays classical-shielded; (c) **the no-viewer is `t`-of-`n`, not absolute** — below the threshold it
is a cryptographic bound (no order learnable, only `(p*,V*)`); `≥ t` colluding parties can
reconstruct, and "nobody even if all collude" is impossible for clearing over hidden data
(`OUTPUT-BOUNDARY-MPC.md §3`); what is removed vs. plain threshold-FHE is the *standing* key liability;
a dishonest subset can also force a *wrong* price (an integrity fault the STARK/decryption-proof
catches), so privacy (below `t`) and correctness are stated separately; (d) IND-CPA-D noise-flooding
params are assumed; (e) the value-binding side of the PQ claim is landed (`PQ-SHIELDED-COMMITMENT.md`
Option A: Poseidon2 `HashCR` + in-AIR conservation, DLog retired from the TCB) — the remaining PQ
residual is the Option-B aggregation-fold carrier, not the value-binding.

---

## 8. Verdict

The FHE thesis **survives real numbers.** The batch uniform-price auction is genuinely the clearing
mechanism FHE can compute — comparison-structured, latency-tolerant, no data-dependent control flow —
and at **N ≤ a few hundred orders per pair at minute cadence it is tractable today** on a single
server with open-source libraries (128 elements sort in ~22 s CKKS / 64 in ~36 s rank-sort). It is
**post-quantum by construction** (lattice/LWE). The committee shrinks to *decrypt one scalar*. And
the correctness proof is **cheaper than feared** because deterministic public re-evaluation makes the
evaluation self-verifying, leaving only a small threshold-decryption proof for the near term.

The honest costs: (1) it requires **switching DrEX's clearing mechanism** from the built ring/TTC
(graph-hard, not FHE-computable) to the uniform-price layer dregg has proved only at model level;
(2) the **assurance ladder is multi-year** — a working non-verifiable FHE clear in ~6–12 months, a
proof-carrying one in ~1–2 years, succinct-for-light-clients in ~2–4; (3) the value-binding PQ
cutover is landed (`PQ-SHIELDED-COMMITMENT.md` Option A), so the *end-to-end PQ* residual is the
Option-B aggregation-fold carrier plus the 64-bit range widening — both named. None of that is fatal,
and all of it is scheduled sharpening on a chosen trajectory rather than a surprise.

DrEX can surpass the no-viewer limitation via FHE — enabled specifically by the batch mechanism — and
the honest way to say it is: **the batch structure is what turns "compute a clear under FHE" from
impossible into a minute-scale job at real sizes; the surpass is the uniform-price product; the ring
stays the classical-shielded product; and the ladder is honest rungs, not a leap.**

---

## Sources

- Zama, TFHE-rs CPU integer benchmarks (default `PARAM_MESSAGE_2_CARRY_2_KS32_PBS_TUNIFORM_2M128`,
  ≥128-bit IND-CPA-D): https://docs.zama.org/tfhe-rs/get-started/benchmarks/cpu/cpu-integer-operations
- Zama, TFHE-rs GPU benchmarks (H100): https://docs.zama.org/tfhe-rs/get-started/benchmarks/gpu
- "Lightweight sorting in approximate homomorphic encryption" (CKKS; 128 elements ~22 s, precision
  1e-3, 3 GB), CiC 3(1):32: https://cic.iacr.org/p/3/1/32
- "Optimized Rank Sort for Encrypted Real Numbers" (64 elements ~35.79 s), ePrint 2025/1170:
  https://eprint.iacr.org/2025/1170.pdf
- "Efficient Sorting of Homomorphic Encrypted Data with k-way Sorting Network" (bitonic 64 = 409 s
  PEGASUS), ePrint 2021/551: https://eprint.iacr.org/2021/551.pdf
- Zama Threshold Key-Management System (TKMS): https://www.zama.org/post/introducing-zama-threshold-key-management-system-tkms
- Zama threshold-fhe (threshold MPC for TFHE/BGV/BFV): https://github.com/zama-ai/threshold-fhe
- Zama Protocol overview / 13→100-node committee roadmap: https://blockeden.xyz/blog/2026/01/05/zama-protocol/
- "Verifiable FHE via Lattice-based SNARKs," CiC 1(1):24: https://cic.iacr.org/p/1/1/24
- HELIOPOLIS: Verifiable Computation over Homomorphically Encrypted Data (ASIACRYPT 2024).
- "Towards Verifiable FHE in Practice: Proving Correct TFHE Bootstrapping using plonky2," ePrint
  2024/451.
- "FHE-SNARK vs. SNARK-FHE," ePrint 2025/302.
- Phalanx: An FHE-Friendly SNARK (CCS 2025): https://dl.acm.org/doi/10.1145/3719027.3765226
- Fhenix CoFHE + Sealed-Bid Auction demo (Arbitrum Sepolia testnet): https://www.fhenix.io/ ,
  https://sealedbids.fhenix.io/
- Inco Lightning (live on Base mainnet; TEE fast path + FHE+MPC): https://www.inco.org/
- FHE ⊂ post-quantum (LWE/Ring-LWE, same family as ML-KEM/ML-DSA): "The Beginner's Textbook for FHE,"
  arXiv 2503.05136; LWE/PQC literature.
- dregg (this repo): `intent/src/solver.rs`, `intent/src/trustless.rs`, `intent/src/matcher.rs`,
  `intent/src/drex_routing.rs`, `intent/src/verified_settle.rs`, `tee-verify/src/{lib,attested_data,
  oracle_mark,snp}.rs`, `metatheory/Market/ShieldedClearing.lean`, `metatheory/Market/Optimality.lean`,
  retired `shielded_ring_clearing_air.rs` and `shielded_ring_clearing_nleg_air.rs`,
  `docs/deos/{DREX-DESIGN,SHIELDED-AUCTIONS-DESIGN,SHIELDED-DREX-ASSURANCE-ROADMAP,
  PQ-SHIELDED-COMMITMENT,OUTPUT-BOUNDARY-MPC}.md`,
  `fhegg-fhe/src/{additive,mpc}.rs`, `fhegg-fhe/{ADDITIVE-FOLD-ENVELOPE,MEASURED-ENVELOPE}.md`,
  `federation/src/threshold_decrypt.rs`, `intent/src/trustless.rs`.
