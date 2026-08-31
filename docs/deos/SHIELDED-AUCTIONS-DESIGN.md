# Shielded Auctions on dregg — the MPC / ZK / sealed-bid design

*Scholar (auction & mechanism-design + ZK/MPC-auction literature) · ideator (dregg-native shielded
auctions) · honest (graded against what is proved vs. spec vs. unbuilt). Present-tense, what-is. Every
dregg claim carries a grade. The honest edges are named in §6, not buried.*

The one-line thesis: **dregg holds the specification of the auction the private-markets
literature calls the frontier — a single-phase, no-committee, sealed-bid clearing over hidden
commitments — as a machine-checked Lean theorem (`shielded_ring_clears`, fused by
`shielded_ring_fused_clears`), AND the ring-clearing circuit that realizes it, at 2-leg and N-leg,
with its residuals named (§2.4). This doc maps the space, places dregg in it honestly, and says
what to build next.**

---

## 1. Research survey — the private / shielded auction space

### 1.1 Mechanism design: which sealed-bid rule, and what it costs

An auction rule is a pair *(who wins, what they pay)*. The choice determines what bidders do.

| Rule | Winner pays | Bidder strategy | Truthful (DSIC)? | Note |
| :-- | :-- | :-- | :-- | :-- |
| **First-price sealed-bid** | own bid | **bid-shade** (bid < value) | No | winner shades to keep surplus; must guess rivals |
| **Second-price (Vickrey)** | highest *losing* bid | **bid true value** | **Yes** (dominant strategy) | payment independent of own bid ⇒ no shading incentive |
| **Uniform-price** (multi-unit) | one clearing price for all | bid true value on marginal unit; **demand-reduce** on inframarginal | No (multi-unit) | susceptible to demand reduction by large bidders |

The classical result (Vickrey 1961): a second-price auction is **incentive-compatible** because the
winner's payment is set by *others'* bids, so truthful revelation is a weakly dominant strategy — bid
shading disappears. First-price forces every bidder into a Bayesian guessing game about rivals, which is
strategically costly and leaks efficiency. In the multi-unit setting, **uniform-price** auctions sell
identical units at one clearing price (usually the lowest winning bid); they are *not* generally DSIC —
large bidders **demand-reduce** (shade inframarginal units to push the clearing price down) — but they
are simple, envy-free (everyone pays the same price), and the standard choice for treasury-style
multi-winner allocation.
[Vickrey / DSIC: metricgate.com second-price calculator; Fiveable game-theory auction properties;
uniform-price demand reduction: Springer, *Uniform Price Auctions: Equilibria and Efficiency*.]

### 1.2 Batch auctions: making time discrete to kill the speed race

**Budish Frequent Batch Auctions (FBA)** (Budish–Cramton–Shim, *QJE* 2015) replace the continuous
limit-order book with discrete short intervals, each cleared as a **uniform-price** call auction. The
point: in continuous time, arbitrage is a latency race won by whoever is microseconds faster; discretize
time and clear a batch at one price, and **speed stops mattering — competition moves to price.**
[academic.oup.com/qje/article/130/4/1547.]

**CoW Protocol** ports FBA to DeFi: batch user orders, solve them together, execute every trade in a
pair at one **Uniform Clearing Price (UCP)**. Because the whole batch clears at one price, *intra-batch
execution order is irrelevant* — sandwich/front-running MEV that lives on ordering is structurally
undermined. The residual weakness: **CoW's solvers see every order** while they search for the clearing
solution. Privacy is a trust assumption on the solver, not a cryptographic guarantee.
[cow.fi/learn/understanding-batch-auctions; cow.fi/learn/is-mev-good-or-bad.]

### 1.3 The three privacy tiers, and the trust each carries

Private auctions want *the matching to happen without a party who can see the bids*. Three families,
increasing in how much trust they remove:

**(a) Threshold-encryption (Shutter-style) — "Medium" privacy, a committee of trust.**
Bids are encrypted to a collective public key; a committee of *n* Keypers holds key fragments. After the
inclusion window closes, a *t*-of-*n* threshold of them produce decryption shares and the bids are
revealed for clearing. This defeats mempool front-running (bots can't read encrypted bids), but security
rests on **fewer than *t* Keypers colluding** — the committee is a standing trusted party that *does* see
the plaintext, just later. [shutter.network; blog.shutter.network/shuttertee.]

**(b) MPC — "High" privacy, distributed but liveness-bound.**
Bidders secret-share their bids across a set of non-colluding nodes; the nodes jointly compute the
outcome (e.g. `max(bids)`, or a full order-book match) without any single node reconstructing a bid.
Handles *complex* evaluations — multi-criteria procurement, quality×price, a full dark-pool match where
no node sees the book. Costs: communication-heavy, and requires **node liveness** (the committee must
stay up and honest). Protocols like zkHawk pause a smart contract to invoke an MPC round on secret-shared
inputs, then resume; Arcium-style MPC networks compute the winner and clearing price with no node seeing
individual values. [researchgate zkHawk; H-1000/sealed-bid-auction-mpc; Arcium.]

**(c) ZK-sealed-bid — "Very High" privacy, no committee at all (the frontier).**
Bidders publish *commitments* (Pedersen) and *ZK proofs* that their bid is well-formed (in range, funded,
protocol-conformant) without revealing the value. Winner selection and clearing are proved in
zero-knowledge over the commitments. **No committee ever holds the plaintext** — there is nothing to
decrypt. Cryptobazaar (NDSS 2026) is a recent SOTA: private sealed-bid auctions "at scale," on-chain
verification of validity and winner selection with individual bids never revealed.
[ndss-symposium.org Cryptobazaar; arxiv 2606.14939 *Censorship-Resistant Sealed-Bid Auctions*.]

### 1.4 The manipulation vectors, and what each tier resists

| Vector | What it is | Resisted by |
| :-- | :-- | :-- |
| **Sniping** | last-instant bids exploiting continuous time | batch/uniform-price clearing (time discretized, one price) |
| **Front-running / MEV** | reading a bid and preempting it | encrypted mempool (Shutter) / commitments (ZK) / batch UCP |
| **Bid-shading** | bidding below value under first-price | second-price/Vickrey rule (truthful) |
| **Non-reveal griefing** | commit-reveal bidder refuses to open a losing bid | **single-phase** (no reveal round to grief) |
| **Operator/committee collusion** | the party who *can* decrypt peeks or leaks | **ZK-sealed-bid** (no party holds plaintext) |

### 1.5 Where the state of the art is stuck

Two walls define the frontier:

1. **The commit-reveal tax.** The classic sealed-bid design is two-phase: commit a hash, then *reveal*.
   Reveal rounds carry **non-reveal griefing** (a bidder who sees they're losing simply never opens,
   forcing fallback/penalty logic), a second gas transaction, and a UX that favors bots that reliably
   automate the reveal. The "holy grail" is **single-phase**: one message with a validity proof, no
   reveal round. [arxiv censorship-resistant sealed-bid auctions.]

2. **The committee that ZK removes.** Every *deployed* private auction that avoids commit-reveal today
   leans on a standing trusted set that *does* see the plaintext: Shutter's Keypers, an MPC node
   committee, Penumbra's validator set decrypting the per-block aggregate. Threshold cryptography
   *distributes* the trust but does not *eliminate* it — the committee remains a collusion target and a
   liveness dependency. **ZK-sealed-bid is the one construction that deletes the committee outright**,
   because clearing happens over commitments and there is nothing to decrypt. That is precisely the seam
   dregg is built to occupy.

---

## 2. dregg's position — rung-3 is the ZK-sealed-bid frontier (as a proved spec)

dregg's private-markets program is **DrEX** (Dragon's EXchange), a rung-structured stack
(`docs/deos/DREX-DESIGN.md`, synthesized in `docs/deos/DREGGFI-VISION.md`). Four assets already exist in
the tree that together *are* a shielded auction, each graded honestly:

### 2.1 The shielded pool — hidden value, owner, AND asset (`circuit-prove/src/shielded/pool.rs`)
A multi-asset (ZSA-style) shielded pool. Each leg is an opaque Pedersen commitment
`commit_hidden_asset(value, asset_type, blinding) = v·V + at·H_asset + r·R`; an observer learns neither
amount, owner, nor asset type. Per-asset conservation is carried by a single homomorphic **Schnorr
excess proof** (`Σ C_in − Σ C_out = r_excess·R` ⇒ both the value and asset-tag components cancel), with
per-output Bulletproof **range proofs** closing the wrapped-negative-value inflation hole, and a
Chaum-Pedersen **asset-equality proof** for split/merge. One nullifier set across all assets gates
double-spend. **Grade: built + tested both polarities** (Rust, over p3 `HidingFriPcs`); the STARK side
is the DSL-emitted `shielded_spend_circuit`, no hand-written AIR.

### 2.2 Sealed no-peek auction — commit→reveal, proved (`metatheory/Dregg2/Intent/SealedAuction.lean`)
A verified sealed-bid **first-price** auction over the real executor. `sealOf b = Blake3(bidder ‖ sign ‖
|value| ‖ nonce)` mirrors the running Rust `compute_commitment_hash`. The keystones, non-vacuous over the
real BLAKE3 CR kernel (not `True`):
- `reveal_binds_committed` — under collision-resistance, a committed seal opens to *exactly* the bid that
  sealed it: **no peek-then-switch** (the anti-front-running tooth).
- `uncommitted_cannot_win` — a party whose seal was never committed can never be the winner.
- phase gates (`reveal_requires_reveal_phase`, `settle_requires_reveal_phase`), `settle_atomic`,
  `settle_conserves` — the award rides `Ring.settleRing` (all-or-nothing, value-neutral).

**Grade: Lean-proved, non-vacuous, ledger-realized** through `recKExecAsset`. This is the *two-phase*
commit-reveal design — it still carries the reveal round §1.5 calls a tax. It is the baseline dregg
*surpasses* with rung-3, not the frontier itself.

### 2.3 Uniform-price optimality + envy-freeness (`metatheory/Market/{Fairness,Optimality,Priced}.lean`)
The fair-clearing rule dregg proves for a two-sided batch:
- `clearing_respects_limits` / `settlement_from_sender_within_offer` (`Market/Fairness.lean`) — the
  give-and-receive individual-rationality half, with refusing teeth (`overdebit_refused`,
  `wrongAsset_refused`). **Ledger-realized** over `settleRing`.
- `uniform_price_no_arbitrage`, `uniform_price_envy_free`, `uniform_price_optimal`
  (`Market/Optimality.lean`) — all legs of a two-sided batch clear at **one price** ⇒ every leg is
  value-neutral, admits no strictly-improving unilateral deviation, and same-direction legs face the
  identical rate. This is the Budish-FBA "single price ⇔ no-arbitrage" property, machine-checked. **Grade:
  Lean-proved but at the priced-`Fill` *model* level (`Market/Priced.lean`, real ℚ prices), at the
  single-participant / pairwise core — NOT ledger-realized (model `Conserves`/`netFlow`, not
  `settleRing`), and NOT k-coalition TTC-core stable.** Say which: IR is ledger-verified;
  uniform-price/envy-free is model-verified.

### 2.4 The rung-3 keystone — private matching over hidden commitments (`metatheory/Market/ShieldedClearing.lean`)
**This is the frontier object.** `shielded_ring_clears` welds three proven towers into one statement: a
shielded ring whose matched cycle is `CycleValid` and that settles through the verified executor is
simultaneously **(a) conserving** per asset on the real ledger, **(b) fair** — structurally balanced and
every leg within its committed offer/want, and **(c) private + no-double-spend** — every leg spends a
real committed *member note* (owner/value hidden inside `HidingFriPcs`), whose nullifier was fresh and can
never be re-spent. The companion `shielded_ring_value_conserves_hidden` proves the homomorphic excess is
zero *over the commitments alone* — a verifier confirms the ring minted no value **without learning a
single amount**. Non-vacuous both poles: a concrete two-leg shielded ring clears fair+private; a re-used
nullifier / over-debit / wrong-asset / value-mint is refused.

**This deletes the committee.** DrEX's front-running prevention today rests on the `intent/src/trustless.rs`
7-layer batch, whose step 3 is a *t*-of-*n* **threshold-decryption ceremony** (Shamir over GF(256), real
`combine_shares`) — the residual trust the module is candid about, and the same committee Shutter/Penumbra
carry. Matching over hidden commitments removes it entirely: the matcher reads *committed* `MatchNode`
claims, settles by spending nullifiers, and checks conservation over Pedersen commitments. **No party ever
holds the plaintext or the ordering power.**

**Grade — the honest heart of this doc: rung-3 is a Lean spec WITH its circuit realization; the
residuals are named.** `shielded_ring_clears` is a real machine-checked theorem, and the fusion its
original statement left open — the constraint tying `node.offerAsset`/`offerAmount` to the hidden
note's `asset`/`value` — is welded at both altitudes. In Lean: `LegFused` + `shielded_ring_fused_clears`
(`metatheory/Market/LedgerRealizationExt.lean:279`, `:302`) — every fused leg's committed offer IS a
fresh member-spend note, so the matcher clears the real note, not a `MatchNode` beside it. In circuit:
the now-retired `shielded_ring_clearing_air.rs` (2-leg, tight cycle) and
`shielded_ring_clearing_nleg_air.rs` (N-leg, variable-length cycle + the partial-fill
`offer ≥ want_min` inequality in-circuit), registered at `circuit-prove/src/lib.rs:52-53`. Both fold
`prove_shielded_spend_leaf_with_claim` leaves into a clearing descriptor that enforces the fusion
(`offer_asset == asset`, `offer_amount == value`, the value bound to the spent note through the
re-computed Poseidon2 `value_binding`), the `CycleValid` ring edges, pairwise-distinct nullifiers, and
Pedersen conservation with an in-circuit bit-decomposition range gadget closing the wraparound mint.
**Named residuals** (per the modules' own headers): the conservation runs over the two-coordinate
`pedTwoGen (v, r)` abstraction, not real in-circuit Ristretto point arithmetic; amounts above
`2^VALUE_BITS` still lean on the off-AIR Bulletproof; the blinding coordinate's group-scalar reduction
rides the off-AIR Schnorr excess. The module realizes the proved spec; the residuals are labeled, not
asserted closed.

**dregg's precise position:** the literature's frontier is single-phase, no-committee ZK-sealed-bid
clearing. dregg has that as a *proved specification* (rung-3) with a *built* ring-clearing circuit at
both sizes (§2.4), resting on a *built and tested* shielded pool (§2.1) and a *proved* fair-clearing
rule (§2.3) — a combination `DREGGFI-VISION.md §3` argues **no deployed system holds together**: beats
CoW (orders visible to solvers), Penumbra (validator flow-decryption), Shutter (Keyper committee), and
dark pools (operator peek), because there is *no instant and no party* that holds the plaintext. What
separates this from a running product is in-circuit clearing-price selection, the ledger-realization
weld for the uniform-price layer, and the §2.4 residuals — integration and labeled seams, not a missing
apex.

---

## 3. Ranked dregg-native shielded auction primitives — what to build

Ranked shielded-first. Each: **mechanism · dregg primitive · why it beats the committee · honest gap ·
build cost.** Grades: `PROVED-SPEC` (Lean theorem) · `BUILT` (Rust, tested) ·
`MODEL-PROVED` (Lean, model-level) · `DESIGN` (not started).

### #1 — Single-phase ZK-sealed-bid uniform-price clearing (the marquee)
- **Mechanism.** Bidders publish a Pedersen commitment to (price, quantity) + a ZK validity proof (range
  + funded). One batch clears at a **uniform price** over the hidden commitments in a single message —
  **no reveal round, no committee.** Winner-set and clearing price are proved in-circuit.
- **Rests on.** `shielded_ring_clears` (rung-3 private matching) + `uniform_price_optimal`
  (`Market/Optimality.lean`) + the shielded pool (`pool.rs`). All three exist.
- **Beats the committee.** Kills *non-reveal griefing* (no reveal phase — the §1.5 holy grail), *sniping*
  (uniform-price batch), and the *committee* (nothing to decrypt) in one object. This is the union
  `DREGGFI-VISION.md §3` says only dregg holds.
- **Honest gap.** The ring-clearing circuit and the offer↔note fusion are `BUILT` (§2.4). What remains:
  the uniform-price layer is `MODEL-PROVED`, not ledger-realized; the single-phase clearing must prove
  *winner selection* (max/clearing-price) in-circuit — the sealed-auction winner logic is Lean over
  revealed bids (`winnerOf`), not a ZK circuit; plus the §2.4 residuals (in-circuit EC, amounts above
  `2^VALUE_BITS`).
- **Build cost.** **Medium→Large.** In-circuit clearing-price selection + the ledger-realization weld;
  the apex and fusion exist.

### #2 — Shielded batch auction for the launchpad / fair-launch (highest near-term value)
- **Mechanism.** A token launch (`docs/deos/DREGG-LAUNCHPAD-DESIGN.md`) runs as a **shielded uniform-price
  batch**: participants commit sealed bids to the launch pool; the sale clears at one price; **the operator
  never sees a bid before clearing**, so it cannot self-allocate, tip insiders, or front-run the book.
- **Rests on.** The shielded pool (`pool.rs`, `BUILT`) + sealed commitments (`SealedAuction`, `PROVED`) +
  uniform-price clearing (`Optimality.lean`, `MODEL-PROVED`).
- **Beats the committee.** Fair-launch fairness is exactly *"no operator peek"* — the launchpad's core
  promise. A Shutter-style committee could still collude to peek at the raise; a ZK-sealed batch cannot.
- **Honest gap.** A near-term version can ship on the **proved two-phase** sealed auction (§2.2, real
  today) with uniform-price settlement — deferring the single-phase rung-3 AIR. That is a *deployable*
  shielded launch now, with the reveal-round tax, upgradable to single-phase when #1 lands.
- **Build cost.** **Medium** for the two-phase version (compose existing proved pieces + launchpad wiring);
  **Medium→Large** for single-phase (inherits #1's remaining in-circuit selection build).

### #3 — Shielded DrEX private-matching DEX (the flagship application)
- **Mechanism.** The DrEX multilateral ring matcher (`intent/src/solver.rs`: Johnson circuits +
  Shapley-Scarf top-trading-cycles) runs over **shielded notes** — a private, MEV-resistant DEX where
  matching happens inside the proof over hidden commitments, cleared at a uniform price.
- **Rests on.** `shielded_ring_clears` + the real matcher + the shielded pool.
- **Beats the committee.** The whole `DREGGFI-VISION.md §4` DrEX claim: *private and fair without trusting
  any operator, committee, or sequencer*, residual leakage = timing + anonymity-set size + intentional
  clearing-price disclosure. No dark pool, encrypted mempool, or batch-auction DEX can claim that.
- **Honest gap.** The ring-clearing circuit exists (§2.4); remaining is #1's in-circuit clearing-price
  selection, plus welding the model-level priced/optimal rungs (4–6) back to `settleRing` (the
  ledger-realization weld, named in `DREGGFI-VISION.md §7`), plus the exchange integration itself.
- **Build cost.** **Large** (this is #1 realized as a live exchange).

### #4 — Shielded-Vickrey sealed auction for RWA / NFT (truthful, single-lot)
- **Mechanism.** A single-lot sealed auction paying the **second price** — bidders bid true value (DSIC,
  §1.1), and the winner pays the highest *losing* bid. Requires computing the second-highest bid **in
  zero-knowledge over the commitments** without revealing any bid, including the winner's.
- **Rests on.** The shielded pool + a ZK comparison/second-max circuit (new). The sealed-auction
  scaffolding (`SealedAuction.lean`) proves first-price winner selection; second-price needs the
  *runner-up* extracted privately.
- **Beats the committee.** Truthfulness is a *mechanism-design* win (no bid-shading), and doing it in ZK
  removes the classic Vickrey failure: a corrupt auctioneer who fakes the second price or leaks bids. A
  ZK second-price proof makes the clearing-price honest *and* private.
- **Honest gap.** dregg proves **first-price** today (`winnerOf` = max). Second-price is a **new
  mechanism** — the private second-max computation is exactly the MPC/ZK sweet spot the literature flags,
  and dregg has *neither* the Lean spec nor the circuit for it yet. `DESIGN`.
- **Build cost.** **Medium** (Lean second-price spec — a `runnerUpOf` + truthfulness theorem) then
  **Large** (the ZK second-max circuit). Sequenceable after #1's in-circuit selection proves the pattern.

### #5 — Private OTC / RFQ (shielded bilateral quote)
- **Mechanism.** A request-for-quote where responders submit sealed quotes; the requester clears against
  the best without the quote book being public. A degenerate (2-party, single-pair) shielded ring.
- **Rests on.** `shielded_ring_clears` specialized to a bilateral cycle (`demoShieldedRing` is exactly a
  2-leg swap) + the shielded pool.
- **Beats the committee.** OTC desks *are* the operator-peek problem; a shielded RFQ removes the desk's
  informational edge.
- **Honest gap.** The 2-leg ring circuit **is built** — `shielded_ring_clearing_air.rs` is exactly this
  bilateral instance (2 legs, 1 pair, tight cycle). What remains is product wiring, not circuit research.
- **Build cost.** **Small** — the minimal rung-3 circuit exists; the build is the RFQ surface around it.

### #6 — Dark pool via MPC (where ZK does *not* suffice)
- **Mechanism.** A private *order book* where no single party sees resting orders — continuous or
  frequent-batch matching over secret-shared orders across a non-colluding node set (§1.3b).
- **Rests on.** MPC (secret-sharing + distributed match), *alongside* dregg's ZK — **not** the ZK fold.
- **The honest MPC-vs-ZK question (see §4).** dregg's rung-3 fold **subsumes** the *single-clearing-batch*
  dark pool: a sealed batch cleared over commitments is a dark pool with *no* nodes. Where MPC is
  genuinely needed is a **persistent, cross-time order book** — resting orders that must be *matched
  against future orders* without ever being revealed or committed to a public clearing. There, no single
  ZK proof spans the interaction; a live MPC committee holding order shares does. This is the one primitive
  where MPC earns a seat next to dregg's ZK.
- **Honest gap.** dregg has **no MPC layer today** — the `trustless.rs` threshold-decryption is committee
  crypto, not an MPC *matching* engine. `DESIGN`, and a genuinely new dependency.
- **Build cost.** **Large / new-subsystem.** Only justified if a persistent shielded book (not a batch) is
  a product requirement; the batch primitives (#1–#3) cover most of the demand without it.

---

## 4. The MPC-vs-ZK decision: does the ZK fold subsume MPC?

**Mostly yes — with one honest exception.** dregg's rung-3 is a *non-interactive proof over commitments*:
one prover produces one proof that a batch cleared conserving+fair+private. Any auction whose privacy
requirement is *"no party sees the bids during a single clearing"* is subsumed by the ZK fold — sealed-bid
auctions, uniform-price batches, launchpad sales, RFQ, and single-batch dark pools all reduce to §3
#1–#5. **This is strictly stronger than MPC on trust**: MPC distributes a live committee that *could*
reconstruct; the ZK fold has *no committee and no liveness dependency* — the proof stands alone.

MPC earns a seat in exactly one place: a **persistent private order book** (§3 #6), where orders must rest
and match *against future orders* without any public commit-and-clear boundary. There, the interaction
spans time and parties in a way one proof does not capture, and a live secret-shared book is the natural
tool. Everything else — every *batch* mechanism — the ZK fold does better, because it deletes the party
MPC only distributes. **Recommendation: build the ZK fold (rung-3) as the spine; treat MPC as an optional,
much later dark-pool subsystem, not a dependency of the auction program.**

---

## 5. The mechanism-design recommendation

**Ship uniform-price as the default clearing rule; add shielded-Vickrey as a single-lot specialization
later; do not build MPC for the auction program.**

- **Uniform-price** is the right default. It is envy-free (one price for all), it is the Budish-FBA
  discipline that makes sniping and ordering-MEV worthless, and — decisively for dregg — **it is the rule
  dregg has already proved optimal and no-arbitrage** (`uniform_price_optimal`, `Market/Optimality.lean`).
  Its known weakness (multi-unit demand reduction, §1.1) is a second-order strategic cost, acceptable for a
  batch DEX / launchpad and far outweighed by having a machine-checked optimality theorem to stand on. It
  composes directly with rung-3 (both reason over `MatchNode`/commitments).

- **Shielded-Vickrey** is the *truthfulness* upgrade and a genuine differentiator for **single-lot** RWA /
  NFT sales (§3 #4), where DSIC matters and demand-reduction is moot (one unit). It requires a **new
  second-price mechanism** (dregg proves first-price today) *and* a private second-max ZK circuit. Worth
  doing — the private second-price computation is a textbook ZK/MPC sweet spot — but *after* the
  in-circuit clearing-price selection proves the private-selection pattern (the ring-clearing circuit
  pattern already exists, §2.4). Do not lead with it.

- **MPC** is not recommended for the auction program (§4). It re-introduces the committee the ZK fold
  exists to delete. Reserve it strictly for a future persistent dark-pool book if that ever becomes a
  product requirement.

The uniform-price recommendation is the honest one: it is what dregg can *prove*, it composes with the
frontier primitive, and it dodges the operator-peek and reveal-round failures in one rule.

---

## 6. The single most-valuable thing to build next

**The ring-clearing circuit exists at both sizes (§2.4). Build the in-circuit clearing-price selection,
then lift the fold to the launchpad shielded batch (§3 #2).**

Rationale, honestly:
- Rung-3 (`shielded_ring_clears` / `shielded_ring_fused_clears`) is the *entire differentiator* — the
  proved spec of the frontier the literature is stuck short of — and its circuit realization is `BUILT`:
  the 2-leg tight cycle and the N-leg variable cycle with the partial-fill inequality in-circuit, fused
  to the hidden notes. The critical path to the marquee single-phase auction (§3 #1) is now **in-circuit
  winner/clearing-price selection** (today Lean over revealed bids, `winnerOf`) and the
  **ledger-realization weld** for the uniform-price layer (model-proved, not `settleRing`-realized).
  Everything downstream (DrEX DEX, RFQ, shielded launchpad single-phase) waits on those two, not on an
  apex.
- The named circuit residuals — in-circuit Ristretto point arithmetic (conservation today runs over the
  two-coordinate `pedTwoGen` abstraction) and amounts above `2^VALUE_BITS` (off-AIR Bulletproof) — are
  labeled seams to close alongside, not blockers to the selection build.
- The **first product** to carry it is the **launchpad shielded batch** (§3 #2): it has a concrete
  near-term user (fair token launches), its fairness promise *is* "no operator peek," and it can ship a
  deployable two-phase version on today's proved pieces *while* the single-phase selection circuit is
  built — so value lands before the research finishes.

Concretely, the next lane: **the in-circuit clearing-price selection** — prove the uniform clearing
price / winner set over the committed bids inside the fold, composing with the built ring-clearing
descriptor. That circuit turns the marquee Lean theorem plus its built clearing fold into a running
single-phase private auction.

---

*Grade summary: shielded pool = BUILT+tested · sealed first-price auction = Lean-PROVED, ledger-realized ·
uniform-price optimality = Lean-PROVED, model-level · rung-3 private matching = PROVED-SPEC + circuit
BUILT (2-leg tight + N-leg partial-fill, fused; in-circuit EC and >2^VALUE_BITS range = named residuals)
· in-circuit clearing-price selection = DESIGN · shielded-Vickrey = DESIGN · MPC dark pool =
DESIGN/deferred. The frontier is a proved specification with its clearing circuit built; the named
remaining builds are in-circuit price selection and the ledger-realization weld.*

*Sources: DREX-DESIGN.md, DREGGFI-VISION.md, DREGG-LAUNCHPAD-DESIGN.md; `Market/ShieldedClearing.lean`,
`Market/LedgerRealizationExt.lean`, `Market/Optimality.lean`, `Market/Fairness.lean`,
`Dregg2/Intent/SealedAuction.lean`, `circuit-prove/src/shielded/pool.rs`,
retired `shielded_ring_clearing_air.rs`, retired `shielded_ring_clearing_nleg_air.rs`,
`intent/src/trustless.rs`. External: Budish–Cramton–Shim QJE 2015; Vickrey 1961; CoW Protocol docs;
Shutter Network; Cryptobazaar (NDSS 2026); zkHawk; Penumbra ZSwap; arxiv 2606.14939.*
