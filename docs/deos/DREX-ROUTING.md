# DrEX cross-chain ROUTING — the ring-of-locks

*Present-tense, what-is. How a DrEX trade that spans chains actually clears and settles: the
three custody modes it distinguishes, the ring-of-locks that dissolves the liquidity-provider
problem, the deposit→clear→release lifecycle, the built timeout/refund escrow, and — named plainly —
the cross-vault atomic release that is the load-bearing open piece. Every claim carries its
build/grade. Honest at its edges (§4, §5); not a promise that cross-chain trading is solved.*

> One-line thesis: **a cross-chain DrEX trade needs no pre-funded liquidity pool and no bridge
> validators — the counterparties' own locks ARE the liquidity, and the multilateral ring
> RE-ASSIGNS those locks under a single proof of the clearing.** Custody is real (the asset sits in
> a proof-gated vault while it trades), but it is surrendered to a *contract that only releases on a
> proof*, never to a custodian or a validator set. The trust that remains — cross-vault atomicity and
> per-chain liveness — is named and graded, not hidden.

---

## 0. Where this fits (and what it is NOT)

DrEX's clearing engine is the `metatheory/Market/` rung ladder (`DREX-DESIGN.md`): the ring/CoW
matcher clears a book multilaterally, fair + conserving, through the proved kernel executor. This
document is about the **routing of foreign value into and out of that engine** across chains — the
layer between "an intent to trade a TSLA-token for ETH" and "the trade cleared and each side got its
asset on its own chain."

It builds on two families of pieces that already exist:

- **the read-only holdings model** — prove you hold an asset on a chain and *reference* it for
  governance weight, eligibility, or collateral, **without moving it** (`INTERCHAIN-MODEL.md`,
  `PROOF-OF-HOLDINGS.md`). This is `prove, don't lock`. **It is not the trading model** — see §1(c).
- **the lock/mirror pieces** — surrender a foreign asset into a proof-gated vault, mint a native
  mirror, and redeem by burning it back out (`TOKEN-MIRROR-BRIDGE.md`,
  `chain/contracts/DreggVault.sol`, `bridge/src/solana_trustless.rs`). This IS the trading model —
  §1(b).

The genuinely-open design problem this doc works through — never designed before — is **how the ring
matcher routes over a set of locks that live on different chains**, and how those locks are released
atomically once the ring clears. §4 is the honest frontier.

---

## 1. THE THREE CUSTODY MODES (be precise — they are not interchangeable)

DrEX touches value in three distinct ways. Conflating them is the error `INTERCHAIN-MODEL.md`
previously invited by letting "prove, don't lock" read as universal. It is not.

### (a) NATIVE dregg assets — self-custody, trade freely

`$DREGG`-in-dregg and shielded notes are native to dregg's value layer. They live as balances /
committed notes in the `effect_vm` ledger and are spent by the owner's own key (or an attenuated
biscuit). There is no vault and no mirror: the asset is already inside the machine that clears the
trade. The ring settles these directly on the real executor ledger (`settleRing → recKExec`,
`Market/LedgerRealizationExt.lean`). **Self-custody throughout.**

### (b) FOREIGN assets for TRADING — LOCK → MIRROR → trade → burn → RELEASE

To trade an asset that lives on another chain (an ERC-20, ETH, an SPL token, a tokenized stock), the
holder **locks it into that chain's proof-gated vault**, DrEX **mints a native mirror**, the mirror
**trades inside dregg**, and on exit the mirror is **burned and the underlying released**. This is
real custody: the asset genuinely sits in the vault while it trades.

- **The lock is what prevents the double-spend.** You cannot trade the mirror inside dregg *and*
  keep spending the underlying on its home chain — the vault holds it. That is the point of the lock,
  exactly as `TOKEN-MIRROR-BRIDGE.md` states: *surrendering custody into a vault is the case where an
  escrow genuinely prevents a double-spend.*
- **Custody is surrendered to a PROOF-GATED CONTRACT, not a custodian.** `DreggVault.sol` releases
  only against a valid spend proof, a fresh nullifier, a recognized note-tree root, and a solvency
  check (`withdraw()`, §3). No multisig decides; no oracle attests the release. So this is **not
  "no custody"** — it is *custody without a custodian*: the contract is the only party that can move
  the funds, and it moves them only on a proof.
- On Solana the inbound lock's *proof* is itself consensus-verified
  (`bridge/src/solana_trustless.rs::verify_lock_proof_consensus_anchored` →
  `mint_against_lock_proof_anchored`: ≥2/3 stake-weighted vote check, bank-hash recompute, accounts
  inclusion, PoH linking, anchored to a governance-pinned weak-subjectivity checkpoint). The mint is
  gated on that, not on an oracle signature.

### (c) HOLDINGS for GOVERNANCE / eligibility / collateral — PROVE, don't lock

To carry governance weight, meet an eligibility gate, or reference collateral you own, you **prove**
you hold the asset over your own account and **keep custody** — the asset never moves. A stake-weighted
Solana holdings proof, an ERC-20 storage proof, a Cosmos bank-balance proof, each bound
non-custodially to your key, yields a *read-only weight*, not a transfer (`INTERCHAIN-MODEL.md` §Two
directions, `dregg-governance/`, the light clients). **This is the model that never locks.** It grants
no spendable value inside dregg and moves nothing — so it cannot be the mechanism for a trade that
must actually deliver an asset to a counterparty.

> **The distinction, one line:** mode (c) *reads* a holding (no custody moves); mode (b) *trades* a
> holding (custody moves into a proof-gated vault). "Prove, don't lock" is (c). Trading is (b).

---

## 2. THE RING-OF-LOCKS (the core routing idea)

The standard cross-chain-swap problem is *liquidity*: to give Alice ETH for her TSLA-token you need
ETH sitting somewhere ready to pay her — a pre-funded liquidity provider, or a bridge that moved ETH
onto TSLA's chain. Both are the honeypot. The ring-of-locks removes the pre-funded pool entirely.

**The mechanism:**

1. **Everyone locks their own asset into their own chain's vault.** Alice locks TSLA into the vault on
   Robinhood Chain; Bob locks ETH into the vault on Base; Carol locks USDC into the vault on Solana.
   Each lock mints a native mirror into dregg's unified value layer.
2. **The ring matcher clears over the unified mirror ledger.** `intent/src/solver.rs` (`RingSolver`:
   Johnson's elementary-circuits + Shapley-Scarf top-trading-cycles) finds the clearing *cycle* —
   Alice→Bob→Carol→Alice — where each participant's offered mirror is exactly the next's wanted
   mirror, in sufficient amount. The settled amount on each leg is the *receiver's* declared minimum
   (`validate_ring`: `amount = receiver.want_min_amount`), and every participant provably receives the
   asset it wanted in ≥ its minimum (individual rationality, `cycle_individuallyRational` /
   `Dregg2/Intent/Ring.lean`).
3. **One dregg proof attests the clearing.** The matched cycle settles through the verified executor
   (`settleRing pre (settlementsOf nodes) = some post`), producing a settled state root — the rung-8
   `DrexClearing` bundle (`Market/CrossChainSettlement.lean`). The clearing is conserving
   (`settleRing_conserves`: mints/burns nothing) and atomic (`settleRing_atomic`: a failing leg leaves
   no settled state).
4. **Each chain's vault RELEASES to the ring-matched recipient, gated on that proof.** Alice's TSLA
   lock is released *to Bob* (whoever the ring matched to receive TSLA); Bob's ETH lock is released to
   Carol; Carol's USDC to Alice. Each release runs the vault's proof-gated `withdraw()` against the
   one settled clearing root.

**The key property:** there is **no pre-funded LP and no bridge validator anywhere in this path.**
The counterparties' locks *are* the liquidity; the ring simply **re-assigns** them — A's locked X goes
to whoever the ring matched to receive X. Release is gated on a proof of the clearing, not on any
party's decision.

**Contrast, explicitly:**

- **A bridge** answers "release these funds?" by *trusting a validator set* to vote. That set is the
  attack surface drained for hundreds of millions (Ronin, Wormhole, …). Here no one votes; the vault
  checks a proof.
- **An aggregator / solver-routed DEX** (1inch, CoW, UniswapX) routes over *fragmented, pre-funded*
  liquidity and trusts an off-chain solver/filler to route honestly, with no correctness proof
  (`DREX-DESIGN.md §1`). Here liquidity is unified (one mirror ledger), the counterparties supply it,
  and the clearing carries a proof.

---

## 3. THE LIFECYCLE (deposit → batch-clear → proof → release)

```text
  chain A vault        chain B vault        chain C vault         dregg value layer
  ────────────         ────────────         ────────────          ─────────────────
  Alice locks TSLA     Bob locks ETH        Carol locks USDC
      │                    │                    │
      │  deposit()         │  depositETH()      │  lock+consensus proof
      ▼                    ▼                    ▼
   mirror-TSLA          mirror-ETH           mirror-USDC   ──►  unified mirror ledger
                                                                     │
                                             DEPOSIT WINDOW closes    ▼
                                                          ┌──────────────────────┐
                                                          │ BATCH-CLEAR (the ring)│  ← ~one proving
                                                          │ solver.rs: cycle over │    cycle; FBA
                                                          │ hidden commitments    │    anti-MEV cadence
                                                          │ (rung-3 shielded)     │    (batch + shielded)
                                                          └──────────┬───────────┘
                                                                     ▼
                                                     the DREGG CLEARING PROOF
                                                     (rung-8 settled root, DrexClearing)
      ┌──────────────────────────────────────────────────────────┘
      ▼                    ▼                    ▼
   withdraw()           withdraw()           withdraw()      RELEASE WINDOW: each vault
   → TSLA to Bob        → ETH to Carol       → USDC to Alice verifies the proof + releases
   (proof-gated)        (proof-gated)        (proof-gated)   to the ring-matched recipient
```

1. **Deposit window (lock).** Each participant locks into their chain's vault
   (`DreggVault.deposit(token, amount, noteCommitment)` / `.depositETH(noteCommitment)`), which
   records a note commitment and mints the mirror. The deposit window bounds *which locks a batch can
   match* — locks that arrive after it wait for the next batch.
2. **Batch-clear (the ring).** At the batch boundary the matcher runs *once* over the batch's mirrors
   — one clearing cycle per proving round. Running in discrete batches rather than continuously is the
   anti-MEV cadence (Budish frequent-batch-auction style, `DREX-DESIGN.md §1`): within a batch there is
   no intra-batch ordering to exploit, and the marquee rung-3 matching is over *hidden commitments*
   (`shielded_ring_clears`, `Market/ShieldedClearing.lean`) so no operator sees an order to front-run
   and there is no decrypt committee.
3. **The dregg clearing proof.** The cleared cycle produces the rung-8 settled root
   (`DrexClearing` → `settleDrex`, `Market/CrossChainSettlement.lean`), the single object every vault
   gates on. Multi-hop is native: the ring *is* a cycle, so an N-party chain of wants clears in one
   proof — there is no sequence of pairwise hops to make atomic.
4. **Release window (verify + release).** Each chain's vault verifies the clearing proof and runs
   `withdraw(token, amount, recipient, proof)`. The vault's checks are real and fail-closed:
   the proof verifies (`_verifySp1`, fail-closed on a codeless verifier), the nullifier is fresh
   (no double-withdraw), the proof commits to a recognized note-tree root (`isKnownRoot`, a
   Tornado-style recent-root ring buffer), and **solvency holds** — `amount ≤ tokenBalances[token]`,
   so a vault never releases more of a token than was locked into it.

---

## 4. THE HARD OPEN FRONTIER (each designed honestly, graded)

This is where the design is genuinely unfinished. The clearing is proved; the *routing of releases
across heterogeneous vaults* is not. Graded plainly.

### (a) CROSS-CHAIN ATOMICITY — the load-bearing open piece

**The property we want:** the ring spans chains, so every vault's release must be gated on the SAME
clearing-root proof, all-or-nothing — either every leg releases to its matched recipient or none does.

**What is actually proved:** atomicity *of the clearing computation* is a theorem. The ring settles
through the executor as a single turn (`settleRing_atomic`: a failing leg leaves no settled state;
`drex_fill_cross_chain_settleable` + the continuity gate `settleDrex_continuity_broken`: a fill
settles only as the exact continuation of the proven root, and a mis-anchored fill fails-closed). So
**dregg is a single settlement authority**: the clearing either commits at the dregg layer or it does
not, atomically.

**What is NOT yet solved:** atomicity *of the releases* across independent vaults on different chains.
Each vault verifies the settled root independently; nothing ties "vault A released" to "vault B
released." The failure mode is concrete: the clearing settles, vault A (Base) releases Bob's ETH, but
vault B's chain (Robinhood Chain) is down or censoring, so Alice's TSLA never releases to Bob. Bob is
now short. **This is the open distributed-commit rung** (`DREX-DESIGN.md §6`, RESEARCH).

**The escrow (BUILT — `chain/contracts/DreggVault.sol`, the `escrow*` surface):** release and refund
are the two branches of one timed escrow, exactly the two-branch design this section named.

- **Commit phase.** `escrowDeposit`/`escrowDepositETH` lock funds under a caller-chosen `escrowId`
  with a per-deposit `deadline`. Escrow funds are accounted in `escrowedBalances`, **disjoint** from
  the generic `tokenBalances` pool, so neither surface can drain the other.
- **Settle phase — release.** `escrowRelease` pays the ring-matched recipient, gated on a fill proof
  whose public outputs name **this** escrow (`escrowId`/token/amount/recipient/`clearingRoot`) *and*
  on `settlement.isProvenRoot(clearingRoot)` — the rung-8 accept-path (dregg actually settled that
  clearing root). A vault can only pay the counterparty the ring actually matched. No deadline check:
  a real fill proof wins over a timeout, but only while the escrow is still `Locked`.
- **Timeout phase — refund.** `escrowRefund` reclaims the lock to the depositor once
  `block.timestamp > deadline` — the timeout IS the condition, no proof needed, so refund is always
  reachable with no external dependency and a lock can never be stuck. (The design's stronger
  variant — a refund gated on a dregg-issued *no-fill proof* — is not built; the deadline-only
  branch is what landed.)

A deposit reaches **exactly one** terminal state — `Locked → Released` XOR `Locked → Refunded`, on
one idempotent-guarded `EscrowStatus` state machine (status flips before the transfer,
checks-effects-interactions + `nonReentrant`), so a released escrow can never be refunded and
vice-versa. **That gives atomicity per leg against the dregg clearing.** The residual it does *not*
remove: a chain that settles the root but
then censors the `withdraw` transaction delays that leg — this is a per-chain *liveness/censorship*
assumption (does your destination chain include your tx?), **not** a validator-trust assumption. And
the genuinely hard case — the clearing settles and releases on A but B's chain is permanently
unavailable, so A paid out while B cannot — is only fully closed by either a coordinator (which we
refuse — it reintroduces a trusted party) or a cross-vault "all-legs-settled" attestation (which
pushes the problem into cross-chain messaging). The honest frame: **the clearing is atomic
(proved); cross-vault release is single-settlement-authority + per-chain timeout/refund (built),
and full heterogeneous-vault atomic release is RESEARCH.**

**Grade.** Clearing atomicity: **PROVED** (`settleRing_atomic`, `settleDrex` continuity gate).
Timeout/refund escrow: **BUILT** — `DreggVault.sol`'s `escrow*` surface (two-branch timed escrow,
`Locked → Released` XOR `Refunded`, deadline-gated refund with no external dependency,
`escrowedBalances` disjoint from `tokenBalances`). The no-fill-*proof*-gated refund refinement:
UNBUILT (the deadline-only branch is what landed). Full cross-vault atomic release: **RESEARCH**.

### (b) NO-FILL RETURN — withdraw a lock that didn't clear

If your batch produces no ring that includes you, you must get your asset back. In the current
decoupled design the mirror is minted on lock regardless of clearing, so the round trip already
exists as an *ordinary* exit: burn the un-traded mirror (`Effect::Burn`) and `withdraw` the
underlying against your own spend proof — the mirror is yours, so its withdraw proof is yours to
make. The *batch-scoped* version is the escrow surface: an `escrowDeposit` names an `escrowId` and a
`deadline`, and the depositor reclaims it after the deadline if no fill cleared it (the refund branch
of §4(a)).

**Grade.** The withdraw-my-own-mirror round trip: **BUILT** primitive (`DreggVault.withdraw` +
`Effect::Burn`), pending the mint-authority executor wiring `TOKEN-MIRROR-BRIDGE.md` names
(`holds_mint_authority`). Batch-scoped no-fill refund: **BUILT** (`escrowRefund`, the §4(a) refund
branch — deadline-gated).

### (c) PARTIAL CROSS-CHAIN FILLS

A leg may fill only partially (the ring clears the receiver's `want_min`, not the full offer). The
*accounting* is kernel-real: rung-5 partial fills lower to conserving kernel transfers of the exact
per-leg amount (`partialFill_cycle_ledger_realized`, `Market/LedgerRealizationExt.lean`; the priced
substrate `Market/Priced.lean`). The vault side is the wiring: a partially-filled lock must release
the *filled* portion to the counterparty and leave the *residual* re-lockable (a residual note) or
refundable. `DreggVault.withdraw` already releases an arbitrary `amount ≤ tokenBalances`, so
partial release is a withdraw for the filled amount plus a residual note for the remainder.

**Grade.** Partial-fill accounting: **PROVED** (rung-5, kernel-real). Vault-side partial release +
residual-note wiring: **UNBUILT** (named build).

### (d) DEPTH WITHOUT A RING — the proven-solvent pool as fallback venue

A lone order with no counterparty in the batch cannot clear as a ring (`¬ Conserves [f]` on its own).
The optional fallback is the rung-6 liquidity pool: a *pre-funded, provably-never-insolvent* venue the
residual clears against (`residual_clears_against_pool`, `Market/Liquidity.lean`). Its guarantee — the
thing an AMM does not ship — is `pool_solvent_forever`: no per-asset reserve is ever negative along
*any* schedule of valid fills (a ∀-adversary solvency theorem), and an overdraw is refused
(`overdraw_refused`) rather than draining the pool. This *reintroduces* a pre-funded LP — but a
*proven-solvent* one, used only as the depth-of-last-resort for orders the ring could not pair, not as
the primary path.

**Grade.** Pool solvency: **PROVED** at model scope (`Pool = AssetId → ℚ`, not yet welded to
`settleRing`); the AMM pricing curve above it and the pool-as-live-vault-backed-venue: **UNBUILT**.

### (e) SETTLE-OUT CHAIN SELECTION

Each leg releases on the chain it locked on — the fill's settled root is verified by *that* chain's
vault/verifier. Rung-8 settles a whole DrEX fill's root onto one target chain
(`drex_fill_cross_chain_settleable`); the verifiers exist — `DreggSettlement.sol` is **LIVE** on
Base-Sepolia, the CosmWasm and Solana (alt_bn128) twins are **DEMONSTRATED** in test. What is not wired
is *proof generation*: turning a real cleared fill into a fresh Groth16 settlement proof whose
`final_root` is the cleared post-state (blocked at HEAD on a fixture-geometry bug a sibling is fixing,
`CrossChainSettlement.lean §HONEST GRADE`).

**Grade.** Single-target settle: **PROVED** at spec + verifier **LIVE (EVM)** / **DEMONSTRATED
(Cosmos, Solana)**. DrEX-fill → settlement proof-gen wiring: **UNBUILT** (named, blocked).

---

## 5. THE HONEST SECURITY STORY

**What the ring-of-locks genuinely buys:**

- **No trusted custodian.** Vaults are proof-gated contracts. `DreggVault.withdraw` releases only on a
  verified proof + fresh nullifier + recognized root + solvency, and refuses to deploy against a
  codeless verifier (fail-closed). No multisig, no oracle, holds the release decision.
- **No bridge validators.** Release is *proof-checked*, not *vote-decided*. This is the whole security
  point of `INTERCHAIN-MODEL.md`: dregg replaces a validator vote with a proof the other side checks
  itself.
- **Unified liquidity.** One mirror ledger, no fragmented per-venue LP; the counterparties' locks are
  the liquidity and the ring re-assigns them.
- **MEV-resistant.** Discrete batches (no intra-batch ordering value) + shielded matching over hidden
  commitments (rung-3: no operator peek, no decrypt committee).

**What it honestly does NOT buy — the counterweights (named, not buried):**

- **The vault holds the asset while it trades — real custody.** Mode (b) is *custody without a
  custodian*, not *no custody*. The asset sits in a smart contract; **smart-contract risk replaces
  validator risk**. If the vault is buggy or exploited, funds are at risk. This is a different, not an
  absent, trust surface.
- **Cross-vault atomic release is the load-bearing OPEN piece.** Clearing atomicity is proved, and
  the per-leg timeout/refund escrow is built (`DreggVault.sol` `escrow*`, §4(a)) — a deposit that
  never clears is depositor-reclaimable after its deadline, so no lock is ever stuck. What the escrow
  does *not* give: all-or-nothing release across heterogeneous chains — a chain that pays out leg A
  while leg B's chain stays permanently unavailable still strands the cross-leg counterparty. **Do
  not claim cross-chain atomic trading is solved. Per-leg safety is built; cross-vault atomicity is
  RESEARCH.**
- **The mirror rests on the vault's on-chain honesty + the proof system.** The whole model assumes the
  vault contract is correct and the proof system is sound. The proof system is Groth16 over BN254 on a
  **single-party dev ceremony** (toxic-waste-known, not mainnet MPC). `DreggVault`'s on-chain note
  tree is currently a **keccak placeholder** pending Poseidon2-circuit alignment — until the on-chain
  tree is the real incremental Poseidon2 tree, root agreement rests on the federation faithfully
  mirroring deposits, not yet purely on-circuit. The Solana lock proof is consensus-verified but
  **off-circuit** (re-executor-grade) and anchored to an operator-configured weak-subjectivity
  checkpoint.

**The precise claim, everywhere:** *not* "trustless, custody-free cross-chain trading" — but
"cross-chain trading with **no custodian and no bridge validators**, over **unified counterparty-supplied
liquidity**, where the clearing is a machine-checked proof — with the **remaining trust (proof-gated
smart-contract honesty, per-chain liveness, and the open cross-vault atomic release) named,
graded, and being driven toward zero.**"

---

## What is BUILT vs OPEN (the load-bearing table)

| piece | status |
|---|---|
| Proof-gated EVM vault (`DreggVault.sol`: deposit/withdraw, nullifier, root-history, solvency, fail-closed verifier) | **BUILT** (keccak note-tree placeholder; SP1-wrapped proof) |
| Consensus-verified Solana lock → mint (`solana_trustless.rs`, `solana-lock/`) | **BUILT** to `ConsensusVerified` (anchored, off-circuit, live vote-feed pending) |
| The lock→mint-mirror mechanism (`live_supply ≤ currently_locked`, `Effect::Mint`/`Burn`) | **BUILT** (mint-authority executor wiring named, `TOKEN-MIRROR-BRIDGE.md`) |
| The ring matcher / router (`solver.rs`: Johnson + TTC, individual rationality, predicate validation) | **BUILT** |
| Ring clears fair + conserving + private (rung-3 `shielded_ring_clears`) | **PROVED** (spec) + **HISTORICALLY BUILT** circuit — the now-retired 2-leg and N-leg ring-clearing AIRs (`shielded_ring_clearing_air.rs` and `shielded_ring_clearing_nleg_air.rs`) folded shielded-spend leaves with in-AIR fusion/conservation/range; the endpoint-carrying outer descriptor is BUILT for the 2-leg apex (`metatheory/Market/ShieldedRingEndpointDescriptor.lean` — `kernel_endpoints` + `receipt_transition` proved — plus the deployed Rust twin with forged-endpoint KATs); the trace refinement `ShieldedRingDescriptorRefines` and the N-leg endpoint surface stay named |
| Partial-fill accounting (rung-5 `partialFill_cycle_ledger_realized`) | **PROVED** (kernel-real) |
| Proven-solvent fallback pool (rung-6 `pool_solvent_forever`) | **PROVED** (model scope; live venue + AMM curve open) |
| Cross-chain settle-out (rung-8 `drex_fill_cross_chain_settleable`) | **PROVED** (spec) + verifier **LIVE (EVM)** / **DEMONSTRATED (Cosmos, Solana)** |
| Vault ↔ DrEX wiring (lock → mirror the ring trades → cleared cycle → per-vault release proofs) | **UNBUILT** (proof-gen blocked on fixture-geometry, §4(e)) |
| **Atomicity / liveness timeout-refund escrow** (the vaults' reclaim path) | **BUILT** — `DreggVault.sol` `escrow*`: `Locked → Released` XOR `Refunded`, deadline-gated refund, `escrowedBalances` disjoint (§4(a); the no-fill-proof refund refinement stays UNBUILT) |
| Full cross-vault atomic release across heterogeneous chains | **RESEARCH** (open distributed-commit, `DREX-DESIGN.md §6`) |
| Partial-release + residual-note vault wiring | **UNBUILT** (§4(c)) |

---

## See also

- `docs/deos/INTERCHAIN-MODEL.md` — the read-only holdings/governance model (mode (c), `prove, don't
  lock`); the per-chain maturity table.
- `docs/deos/TOKEN-MIRROR-BRIDGE.md` — the lock→mint-mirror mechanism (mode (b)) and its honest trust
  model.
- `docs/deos/DREX-DESIGN.md` — the clearing engine, the scholar survey, the rung ladder, and §6 (the
  cross-chain atomic-ring open problem).
- `docs/deos/DREGGFI-VISION.md` — the substrate frame + the settle-anywhere unified position.
- `chain/contracts/DreggVault.sol` · `bridge/src/solana_trustless.rs` · `intent/src/solver.rs` ·
  `metatheory/Market/{ShieldedClearing,CrossChainSettlement,Liquidity,LedgerRealizationExt}.lean`.
