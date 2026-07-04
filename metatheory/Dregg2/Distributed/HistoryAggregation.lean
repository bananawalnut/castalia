/-
# Dregg2.Distributed.HistoryAggregation — the FOLD model under the IVC accumulator.

**What this is.** The whole-chain IVC accumulator (`circuit/src/ivc_turn_chain.rs`) folds a
sequence of finalized-turn proofs into ONE recursive proof attesting "all turns `1..K` executed
correctly AND the finalized state root advanced correctly from genesis to final, in that order."
This module is the EXECUTABLE/DECLARATIVE model that fold is supposed to attest — stated over the
VERIFIED executor (`Exec.RecordKernel.recCexec`, the same machine `BlocklaceFinality.executeTau`
drives) and the GENUINE per-turn state commitment (`Circuit.StateCommit.recStateCommit`, the
injective §8 full-state root the whole-turn triangle pins — `whole_turn_circuit_pins_intent_fold`).

It is the *meaning* of the chain. `RecursiveAggregation.lean` adds the SNARK recursion layer on top:
it names the inner-proof-soundness + recursive-verifier-soundness hypotheses (the part you cannot
prove in Lean — plonky3/pickles FRI), and shows that, UNDER those named hypotheses, an aggregate
proof's validity is exactly `AggregateAttests` from this file — so a light client that checks only
the succinct aggregate learns the whole history is correct.

**The two binding facts modeled here** (the `TurnChainBindingAir` of `ivc_turn_chain.rs:188`):
  1. **Per-step correctness** — each fold step `(pre, turn, post)` is a GENUINE `recCexec` step
     (its `commits` field), so the step proof, when sound, attests the verified executor actually
     ran that turn.
  2. **The temporal tooth** — `new_root[i] == old_root[i+1]` (`ivc_turn_chain.rs:246`): step `i`'s
     post-root is step `i+1`'s pre-root. Reorder / drop / insert ⇒ the chain breaks (UNSAT).

**The headline (`wellformed_attests_whole_history`):** a `WellFormedChain` from a genesis state
yields — for EVERY turn in the chain — a real `recCexec` step (the turn executed correctly per the
verified executor), the chain is correctly ordered (each post-root = next pre-root), and the whole
chain is a `Run recChainedSystem` from genesis whose final state is the genuine fold of the history,
so `recChained_run_conserves` (no mint/burn over the entire history) applies.

`#assert_axioms`-clean (⊆ {propext, Classical.choice, Quot.sound}).
Verified with `lake build Dregg2.Distributed.HistoryAggregation`.
-/
import Dregg2.Circuit.StateCommit
import Dregg2.Exec.ConsensusExec

namespace Dregg2.Distributed.HistoryAggregation

open Dregg2.Exec (RecChainedState recCexec recChainedSystem recChained_run_conserves recTotal)
open Dregg2.Execution (System Run)
open Dregg2.Circuit.StateCommit (recStateCommit recStateCommit_binds recStateCommit_binds_kernel
  compressInjective compressNInjective cellLeafInjective RestHashIffFrame AccountsWF cellDigest)

/-- The all-zero turn — `Turn` has no `Inhabited` instance, so we name the canonical default
turn-context used to commit the genesis/empty-chain root. -/
def zeroTurn : Dregg2.Exec.Turn := ⟨0, 0, 0, 0⟩

/-! ## 0. The §8 state-commitment portal (the genuine per-turn root).

`recStateCommit k t` is the injective full-state commitment from the whole-turn triangle — the ONE
authenticated per-turn state root the running prover folds (`StateCommit.lean:196`). It is
parametric in the Poseidon portal functions; we carry them as section variables exactly as
`StateCommit`/`WholeTurnTriangle` do, plus the single collision-resistance carrier
`compressInjective cmb` the binding lemma needs (REALIZABLE — Poseidon 2-to-1 CR). -/

section Portal

variable (CH : Dregg2.Exec.CellId → Dregg2.Exec.Value → ℤ)
variable (RH : Dregg2.Exec.RecordKernelState → ℤ)
variable (cmb : ℤ → ℤ → ℤ)
variable (compress : ℤ → ℤ → ℤ)
variable (compressN : List ℤ → ℤ)

/-- **`stateRoot k t`** — the genuine §8 full-state commitment of kernel `k` under the turn-context
`t` (the prover-folded per-turn root). `recStateCommit` with the portal fixed. A turn `t` advances
`(stateRoot k t) ↦ (stateRoot k' t')`; the chain binds these roots. -/
def stateRoot (k : Dregg2.Exec.RecordKernelState) (t : Dregg2.Exec.Turn) : ℤ :=
  recStateCommit CH RH cmb compress compressN k t

/-! ## 1. One fold step — a finalized turn + the roots it advances.

The Rust `FinalizedTurn` (`ivc_turn_chain.rs:100`) carries a whole-turn proof whose PI exposes
`(OLD_COMMIT, NEW_COMMIT)`. Here a `ChainStep` carries the pre-state chained record, the turn, and
the post-state chained record, together with the EXECUTOR WITNESS `recCexec pre turn = some post` —
so this is a real verified step, not an asserted one. Its roots are the GENUINE `stateRoot` of the
pre/post kernels. -/

/-- A single fold step: a genuine `recCexec` transition `(pre, turn) ↦ post`, plus the receipt logs
the chained state carries. Modeled directly over the verified executor so the step's roots are the
real commitments of states the executor reached — what an honest inner step proof, when
sound, attests. -/
structure ChainStep where
  /-- The pre-state chained record (kernel + receipt log). -/
  pre  : RecChainedState
  /-- The turn applied this step. -/
  turn : Dregg2.Exec.Turn
  /-- The post-state chained record. -/
  post : RecChainedState
  /-- **The executor witness**: `recCexec pre turn = some post`. -/
  commits : recCexec pre turn = some post

/-- The step's pre-state root (the §8 commitment of the pre-kernel). The Rust `old_root`. -/
def ChainStep.oldRoot (s : ChainStep) : ℤ := stateRoot CH RH cmb compress compressN s.pre.kernel s.turn

/-- The step's post-state root (the §8 commitment of the post-kernel). The Rust `new_root`. -/
def ChainStep.newRoot (s : ChainStep) : ℤ := stateRoot CH RH cmb compress compressN s.post.kernel s.turn

/-! ## 2. The temporal tooth — `new_root[i] == old_root[i+1]`.

`TurnChainBindingAir` constraint 1 (`ivc_turn_chain.rs:246`): each step's `new_root` must be the
NEXT step's `old_root`. A reordered/dropped/inserted turn breaks this and is UNSAT. -/

/-- **`Continues s s'`** — the temporal tooth between adjacent steps: `s.newRoot = s'.oldRoot`
(`new_root[i] == old_root[i+1]`). Its failure is the `TurnChainError::ChainBreak` rejection. -/
def Continues (s s' : ChainStep) : Prop :=
  ChainStep.newRoot CH RH cmb compress compressN s = ChainStep.oldRoot CH RH cmb compress compressN s'

/-- **`ChainBound steps`** — every adjacent pair satisfies the temporal tooth. The whole sequence is
the genuine finalized order (no reorder/drop/insert at the root level). -/
def ChainBound : List ChainStep → Prop
  | []            => True
  | [_]           => True
  | s :: s' :: rest => Continues CH RH cmb compress compressN s s' ∧ ChainBound (s' :: rest)

/-! ## 3. State-level continuity + the well-formed chain.

The Rust chain binding is over ROOTS; the executor model adds the underlying STATE continuity (step
`i`'s post-state IS step `i+1`'s pre-state — `RecChainedState` equality), which §5 shows the
root-level tooth recovers under CR. `lastStateOf` is the state the chain reaches from genesis. -/

/-- **`StateChained g steps`** — the steps form a contiguous executor run from genesis `g`: the first
step's pre-state is `g`, and each step's post-state is the next step's pre-state. -/
def StateChained (g : RecChainedState) : List ChainStep → Prop
  | []        => True
  | s :: rest => s.pre = g ∧ StateChained s.post rest

/-- **`lastStateOf g steps`** — the state the chain reaches from genesis `g`: genesis if empty,
else the last step's `post`. (Defined structurally so the run keystone can name the endpoint.) -/
def lastStateOf (g : RecChainedState) : List ChainStep → RecChainedState
  | []        => g
  | s :: rest => lastStateOf s.post rest

/-- **`WellFormedChain g steps`** — the steps are a genuine executor chain from genesis `g` AND the
root-level temporal tooth holds. -/
structure WellFormedChain (g : RecChainedState) (steps : List ChainStep) : Prop where
  /-- State-level continuity from genesis (each `recCexec` post is the next pre). -/
  chained : StateChained g steps
  /-- Root-level temporal tooth (the `TurnChainBindingAir` continuity constraint). -/
  bound   : ChainBound CH RH cmb compress compressN steps

/-! ## 4. The genuine final root of the whole history.

The accumulator's final claim is "`final_root` = the genuine fold of the whole history"
(`ivc_turn_chain.rs:18`): the §8 commitment of the kernel reached by folding `recCexec` over all the
turns. `lastStateOf g steps` IS that folded state; its commitment is the genuine final root. -/

/-- **`foldedFinalRoot g steps`** — the genuine §8 final root: commit the folded post-kernel
(`lastStateOf`) under the last step's turn-context (the `NEW_COMMIT` the accumulator exposes; the
empty chain commits genesis under `default`). -/
def foldedFinalRoot (g : RecChainedState) (steps : List ChainStep) : ℤ :=
  match steps.getLast? with
  | none   => stateRoot CH RH cmb compress compressN g.kernel zeroTurn
  | some s => stateRoot CH RH cmb compress compressN (lastStateOf g steps).kernel s.turn

/-! ## 5. The CR recovery — the ROOT tooth recovers STATE continuity.

The accumulator's verifier only sees ROOTS, not states. Under collision-resistance of the commitment
(`compressInjective cmb`, via `recStateCommit_binds`), the root-level tooth recovers the underlying
kernel continuity. This is why the LIGHT CLIENT, seeing only roots, learns state
continuity — the §8 root is an injective full-state commitment. -/

/-- **`seam_roots_chain` (the easy direction).** State-level continuity at a seam ENTAILS
the root-level tooth: if `s.post = s'.pre` and the turn-contexts agree at the seam, their roots
chain. So an honest accumulator never asserts the tooth separately — it is free from execution. -/
theorem seam_roots_chain (s s' : ChainStep)
    (hstate : s.post = s'.pre) (hturn : s.turn = s'.turn) :
    ChainStep.newRoot CH RH cmb compress compressN s
      = ChainStep.oldRoot CH RH cmb compress compressN s' := by
  unfold ChainStep.newRoot ChainStep.oldRoot
  rw [hstate, hturn]

/-- **`root_tooth_pins_state` (THE CR RECOVERY).** Under collision-resistance of the
commitment combiner, the ROOT-level tooth pins the underlying full-state COMMITMENT: if two steps
share a turn-context and their seam roots agree, then their cell-digests AND rest-hashes agree
(`cellDigest s.post = cellDigest s'.pre ∧ RH s.post = RH s'.pre`). That is exactly the binding
`recStateCommit_binds` provides — the §8 root is an injective commitment to (live-cell digest, rest
hash), i.e. to the WHOLE kernel (every cell binds via `cellLeafInjective`; the 16 non-cell fields via
`RestHashIffFrame`). So a light client that sees only the matching roots GENUINELY learns the states
chained, up to CR. This is the load-bearing fact that makes "verify the succinct aggregate"
sufficient: the root IS the full-state commitment. -/
theorem root_tooth_pins_state (hCmb : compressInjective cmb) (s s' : ChainStep)
    (hturn : s.turn = s'.turn)
    (htooth : ChainStep.newRoot CH RH cmb compress compressN s
                = ChainStep.oldRoot CH RH cmb compress compressN s') :
    cellDigest CH compress compressN s.post.kernel s'.turn
        = cellDigest CH compress compressN s'.pre.kernel s'.turn
      ∧ RH s.post.kernel = RH s'.pre.kernel := by
  unfold ChainStep.newRoot ChainStep.oldRoot stateRoot at htooth
  rw [hturn] at htooth
  exact recStateCommit_binds CH RH cmb compress compressN hCmb
          s.post.kernel s'.pre.kernel s'.turn htooth

/-- **`root_tooth_pins_kernel` (THE STRENGTHENED CR RECOVERY — state-equality, not just commitment).**
The critique's precise gap: `root_tooth_pins_state` recovers only `cellDigest`+`RH` EQUALITY
(commitment-level), not the `s.post.kernel = s'.pre.kernel` STATE equality `StateChained` needs — so
the headline still took state continuity as a separate prover hypothesis. This closes it: under the
FULL standard Poseidon CR set + the PROVED-preserved `AccountsWF` structural invariant on BOTH seam
kernels, an agreeing root-tooth at a matched turn-context forces the WHOLE kernel equal
(`recStateCommit_binds_kernel`: `RH` recovers the 15 non-cell fields incl. `accounts`, then the cell
digest recovers `cell` over the now-common carrier). So a light client seeing only the matching roots
GENUINELY learns the adjacent KERNELS coincide — up to CR — not merely their commitments. (The
receipt LOG is the one `RecChainedState` component the §8 state root does NOT bind; see
`KernelChained` below — conservation rides on the kernel alone, so the log is conservation-irrelevant,
and that is the exact, named residual rather than a hidden hypothesis.) -/
theorem root_tooth_pins_kernel
    (hCmb : compressInjective cmb) (hCompress : compressInjective compress)
    (hCompressN : compressNInjective compressN) (hLeaf : cellLeafInjective CH)
    (hRest : RestHashIffFrame RH) (s s' : ChainStep)
    (hwf : AccountsWF s.post.kernel) (hwf' : AccountsWF s'.pre.kernel)
    (hturn : s.turn = s'.turn)
    (htooth : ChainStep.newRoot CH RH cmb compress compressN s
                = ChainStep.oldRoot CH RH cmb compress compressN s') :
    s.post.kernel = s'.pre.kernel := by
  unfold ChainStep.newRoot ChainStep.oldRoot stateRoot at htooth
  rw [hturn] at htooth
  exact recStateCommit_binds_kernel CH RH cmb compress compressN hCmb hCompress hCompressN hLeaf hRest
          s.post.kernel s'.pre.kernel s'.turn hwf hwf' htooth

/-! ## 6. THE HEADLINE — a well-formed chain attests the WHOLE history. -/

/-- **`every_turn_executed_correctly`.** For EVERY turn in the chain, the turn executed
correctly per the verified executor: each `ChainStep.commits` IS the `recCexec` witness, so it holds
by construction. The first headline conjunct, made explicit. -/
theorem every_turn_executed_correctly (steps : List ChainStep) :
    ∀ s ∈ steps, recCexec s.pre s.turn = some s.post :=
  fun s _ => s.commits

/-- **`wellformed_is_run` (the run-level keystone).** A state-chained sequence from genesis
`g` IS a `Run recChainedSystem` from `g` to `lastStateOf g steps`: each step is a
`recChainedSystem.Step` (`⟨turn, commits⟩`), composed along `StateChained`. So ALL of `RecordKernel`'s
run-level theorems apply to the whole history. -/
theorem wellformed_is_run (g : RecChainedState) (steps : List ChainStep)
    (hch : StateChained g steps) :
    Run recChainedSystem g (lastStateOf g steps) := by
  induction steps generalizing g with
  | nil => exact Run.refl (S := recChainedSystem) g
  | cons s rest ih =>
    obtain ⟨hpre, hrest⟩ := hch
    subst hpre
    have hstep : recChainedSystem.Step s.pre s.post := ⟨s.turn, s.commits⟩
    simpa [lastStateOf] using Run.step (S := recChainedSystem) hstep (ih s.post hrest)

/-- **`wellformed_history_conserves` (KEYSTONE).** Value is conserved across the WHOLE
folded history: the ledger total at the folded endpoint equals the genesis total. The aggregate
attests a no-mint/no-burn history of arbitrary length. Rides `recChained_run_conserves`
over the run keystone. The light client, trusting only the aggregate, inherits this. -/
theorem wellformed_history_conserves (g : RecChainedState) (steps : List ChainStep)
    (hch : StateChained g steps) :
    recTotal (lastStateOf g steps).kernel = recTotal g.kernel :=
  recChained_run_conserves (wellformed_is_run g steps hch)

/-! ### Conservation-over-history from KERNEL continuity alone (the §8 root binds the kernel, not the log).

`StateChained` is full `RecChainedState` equality at every seam — it includes the receipt LOG, which
the §8 state commitment does NOT bind (`recStateCommit`/`cellDigest`/`RH` are pure functions of the
kernel; `RecChainedState.log` is uncommitted). A light client verifying roots can therefore recover
KERNEL continuity (`root_tooth_pins_kernel`) but not log continuity. The headline content the critique
demands — conservation-over-history — needs ONLY kernel continuity (`recTotal` reads the kernel), so we
factor it through `KernelChained` and DERIVE it from the verified root teeth, with no `StateChained`
hypothesis. (Full `Run`/`StateChained` — which a few downstream theorems use for run-level facts beyond
conservation — is what additionally needs the log; named, not hidden.) -/

/-- The kernel projection of `recCexec`: a committed chained step commits the underlying kernel step
`recKExec pre.kernel turn = some post.kernel`. Read off the `recCexec` match (the success arm sets
`post.kernel := k'`). -/
theorem recCexec_kernel {s s' : RecChainedState} {t : Dregg2.Exec.Turn}
    (h : recCexec s t = some s') : Dregg2.Exec.recKExec s.kernel t = some s'.kernel := by
  unfold recCexec at h
  split at h
  · next k' heq => simp only [Option.some.injEq] at h; rw [← h]; exact heq
  · exact absurd h (by simp)

/-- **`KernelChained g steps`** — the steps form a contiguous executor run from genesis `g` AT THE
KERNEL LEVEL (the receipt log set aside): the first step's pre-KERNEL is `g`'s, and each step's
post-kernel is the next step's pre-kernel. This is exactly what the §8 root tooth recovers under CR
(`root_tooth_pins_kernel`) — and exactly what conservation needs. -/
def KernelChained (g : RecChainedState) : List ChainStep → Prop
  | []        => True
  | s :: rest => s.pre.kernel = g.kernel ∧ KernelChained s.post rest

/-- **`KernelGenesisPin g steps`** — the head step's pre-KERNEL is genesis `g`'s (vacuous if empty).
Genesis is a PUBLIC, agreed value (the chain's declared start, pinned in `Aggregate.genesisRoot`), so
this is an honest input, NOT the malicious-prover surface (which is the INTER-step continuity, derived
below). Named as its own predicate so the conservation headlines share one stable hypothesis type. -/
def KernelGenesisPin (g : RecChainedState) : List ChainStep → Prop
  | []        => True
  | s :: _    => s.pre.kernel = g.kernel

/-- `StateChained` (full-state) entails `KernelChained` (its kernel projection): equal states have
equal kernels. So `KernelChained` is the strictly weaker continuity the commitment can actually
deliver, and every honest `StateChained` chain is a `KernelChained` chain. -/
theorem kernelChained_of_stateChained (g : RecChainedState) (steps : List ChainStep)
    (hch : StateChained g steps) : KernelChained g steps := by
  induction steps generalizing g with
  | nil => trivial
  | cons s rest ih =>
    obtain ⟨hpre, hrest⟩ := hch
    exact ⟨by rw [hpre], ih s.post hrest⟩

/-- **`kernelChained_conserves` (KEYSTONE — conservation rides KERNEL continuity, no log).** Value is
conserved across the whole history given only KERNEL continuity from genesis: the ledger total at the
folded endpoint equals the genesis total. Proved by direct induction on the per-step kernel
conservation (`recKExec_conserves` over each `ChainStep.commits`, projected by `recCexec_kernel`) —
NOT via `Run`, so it never touches the receipt log. This is the conservation the §8 root genuinely
buys a light client, since the commitment binds the kernel. -/
theorem kernelChained_conserves (g : RecChainedState) (steps : List ChainStep)
    (hkc : KernelChained g steps) :
    recTotal (lastStateOf g steps).kernel = recTotal g.kernel := by
  induction steps generalizing g with
  | nil => rfl
  | cons s rest ih =>
    obtain ⟨hpre, hrest⟩ := hkc
    -- this step conserves at the kernel level …
    have hstep : recTotal s.post.kernel = recTotal s.pre.kernel :=
      Dregg2.Exec.recKExec_conserves s.pre.kernel s.post.kernel s.turn (recCexec_kernel s.commits)
    -- … and the tail conserves from `s.post` by IH.
    have htail : recTotal (lastStateOf s.post rest).kernel = recTotal s.post.kernel := ih s.post hrest
    rw [show lastStateOf g (s :: rest) = lastStateOf s.post rest from rfl, htail, hstep, hpre]

/-- **`SeamStruct steps`** — the per-seam STRUCTURAL facts (NOT continuity, NOT roots): at every
adjacent pair the turn-contexts match AND both seam kernels are `AccountsWF`. These are exactly the
two non-cryptographic side facts `root_tooth_pins_kernel` consumes beyond the tooth itself: turn-match
holds under the accumulator's NoOp-padding (`ivc_turn_chain.rs:325`), and `AccountsWF` is the
structural invariant every `recKExec` PRESERVES (`StateCommit.recKExec_preserves_AccountsWF`). The
TOOTH itself is NOT here — it comes from `ChainBound`, which a verified `bindingProof` supplies. So
`SeamStruct` is the honest, non-prover-controlled structural envelope; continuity is DERIVED. -/
def SeamStruct : List ChainStep → Prop
  | []            => True
  | [_]           => True
  | s :: s' :: rest =>
      (s.turn = s'.turn ∧ AccountsWF s.post.kernel ∧ AccountsWF s'.pre.kernel)
      ∧ SeamStruct (s' :: rest)

/-- **`kernelChained_of_verified` (THE DERIVATION — state continuity FROM verification).** From the
genesis pin `s₀.pre.kernel = g.kernel` (genesis is public/agreed — NOT the malicious surface), the
verified root tooth `ChainBound` (what a verified `bindingProof` delivers, = `AggregateAttests.ordered`),
and the structural envelope `SeamStruct` (matched turns + `AccountsWF`), the whole chain is
`KernelChained`. Each inter-step seam `s.post.kernel = s'.pre.kernel` is DERIVED by
`root_tooth_pins_kernel` from the `ChainBound` tooth — it is no longer the prover-supplied `StateChained`
hypothesis the critique flagged. Kernel continuity (hence conservation) follows from the VERIFIED root
under the CR floor, with only the genesis pin + structural envelope as honest inputs. -/
theorem kernelChained_of_verified
    (hCmb : compressInjective cmb) (hCompress : compressInjective compress)
    (hCompressN : compressNInjective compressN) (hLeaf : cellLeafInjective CH)
    (hRest : RestHashIffFrame RH)
    (g : RecChainedState) :
    ∀ steps : List ChainStep,
      KernelGenesisPin g steps →
      ChainBound CH RH cmb compress compressN steps →
      SeamStruct steps →
      KernelChained g steps
  | [], _, _, _ => trivial
  | [s], hgen, _, _ => ⟨hgen, trivial⟩
  | s :: s' :: rest, hgen, hbound, hstruct => by
    obtain ⟨htooth, hboundrest⟩ := hbound
    obtain ⟨⟨hturn, hwf, hwf'⟩, hstructrest⟩ := hstruct
    -- the seam `s.post.kernel = s'.pre.kernel` is DERIVED from the verified `ChainBound` tooth.
    have hseam : s.post.kernel = s'.pre.kernel :=
      root_tooth_pins_kernel CH RH cmb compress compressN hCmb hCompress hCompressN hLeaf hRest
        s s' hwf hwf' hturn htooth
    refine ⟨hgen, ?_⟩
    -- recurse on the tail from `s.post`; the next genesis pin is the derived seam (`s'.pre = s.post`).
    exact kernelChained_of_verified hCmb hCompress hCompressN hLeaf hRest s.post (s' :: rest)
      hseam.symm hboundrest hstructrest

/-- **`verified_history_conserves` (THE HEADLINE CLOSURE — conservation from VERIFICATION ALONE).**
Conservation across the WHOLE history with NO `StateChained` hypothesis: given the genesis pin, the
VERIFIED root tooth `ChainBound` (= `AggregateAttests.ordered`), and the structural envelope
`SeamStruct`, the ledger total at the folded endpoint equals the genesis total. Composes
`kernelChained_of_verified` (verified tooth ⇒ kernel continuity, via the strengthened
`root_tooth_pins_kernel`) with `kernelChained_conserves` (kernel continuity ⇒ conservation). This is
the precise statement the critique asked for: "trusting the aggregate trusts a no-mint/no-burn history"
now follows from the VERIFIED root, not from the prover's honesty about state continuity. -/
theorem verified_history_conserves
    (hCmb : compressInjective cmb) (hCompress : compressInjective compress)
    (hCompressN : compressNInjective compressN) (hLeaf : cellLeafInjective CH)
    (hRest : RestHashIffFrame RH)
    (g : RecChainedState) (steps : List ChainStep)
    (hgen : KernelGenesisPin g steps)
    (hbound : ChainBound CH RH cmb compress compressN steps)
    (hstruct : SeamStruct steps) :
    recTotal (lastStateOf g steps).kernel = recTotal g.kernel :=
  kernelChained_conserves g steps
    (kernelChained_of_verified CH RH cmb compress compressN hCmb hCompress hCompressN hLeaf hRest
      g steps hgen hbound hstruct)

/-- **`chainBound_of_stateChained`.** State-level continuity entails the root-level temporal
tooth at every seam WHERE THE TURN-CONTEXTS AGREE. We state the per-seam fact directly via
`seam_roots_chain`; whole-list `ChainBound` follows when each seam's turn-contexts agree (the
configuration the accumulator's NoOp-padding establishes — see `ivc_turn_chain.rs:325`). This makes
`WellFormedChain.bound` derivable from `chained` at matched seams, so an honest chain is well-formed
without separately asserting the binding. -/
theorem chainBound_of_stateChained (s s' : ChainStep) (rest : List ChainStep)
    (hcont : StateChained s.pre (s :: s' :: rest))
    (hturn : s.turn = s'.turn) :
    Continues CH RH cmb compress compressN s s' := by
  obtain ⟨_, hpre', _⟩ := hcont
  exact seam_roots_chain CH RH cmb compress compressN s s' hpre'.symm hturn

/-- **`wellformed_attests_whole_history` (THE HEADLINE).** A well-formed chain from genesis
`g` GENUINELY attests the whole history:
  (1) **every turn executed correctly** — for every step, `recCexec pre turn = some post` (the
      verified executor ran that turn);
  (2) **the chain is correctly ordered** — the root-level temporal tooth holds (`ChainBound`), so no
      reorder/drop/insert;
  (3) **the final root is the genuine fold of the whole history** — `foldedFinalRoot` commits the
      `lastStateOf`, the kernel reached by folding `recCexec` over ALL the turns, and the whole chain
      is a `Run recChainedSystem` from `g` to that state (so conservation et al. apply).
This is the meaning the IVC accumulator's `WholeChainProof` claims; `RecursiveAggregation.lean`
shows the SNARK aggregate, under the named soundness hypotheses, delivers EXACTLY this. -/
theorem wellformed_attests_whole_history (g : RecChainedState) (steps : List ChainStep)
    (hwf : WellFormedChain CH RH cmb compress compressN g steps) :
    (∀ s ∈ steps, recCexec s.pre s.turn = some s.post)         -- (1) every turn correct
      ∧ ChainBound CH RH cmb compress compressN steps          -- (2) correctly ordered
      ∧ Run recChainedSystem g (lastStateOf g steps)           -- (3) genuine fold = a real run …
      ∧ foldedFinalRoot CH RH cmb compress compressN g steps
          = match steps.getLast? with
            | none   => stateRoot CH RH cmb compress compressN g.kernel zeroTurn
            | some s => stateRoot CH RH cmb compress compressN (lastStateOf g steps).kernel s.turn :=
  ⟨every_turn_executed_correctly steps, hwf.bound,
   wellformed_is_run g steps hwf.chained, rfl⟩

/-! ## 7. NON-VACUITY — the binding tooth has TEETH (witnessed BOTH ways).

`WellFormedChain` is not empty (a real 1-step chain over the teeth genesis is well-formed), and the
temporal tooth REJECTS a broken order (two steps whose seam roots differ are NOT
`ChainBound`). Both witnessed below over the concrete `ConsensusExec.teethGenesis`. -/

open Dregg2.Exec.ConsensusExec (teethGenesis honestTurn tamperedTurn)

/-- A genuine honest step over the teeth genesis: cell 0 transfers 10 to cell 1. The `commits` field
is discharged by `decide` — `recCexec teethGenesis honestTurn` really is `some _`. -/
def honestStep : ChainStep where
  pre := teethGenesis
  turn := honestTurn
  post := (recCexec teethGenesis honestTurn).get (by decide)
  commits := (Option.some_get _).symm

/-- **`honest_chain_wellformed` (non-vacuity, positive).** The single-step honest chain is
state-chained from genesis: `WellFormedChain` is INHABITED by a real executor run, so the headline is
not vacuous. (We exhibit `StateChained`; `ChainBound` on a singleton is `True`.) -/
theorem honest_chain_wellformed :
    WellFormedChain CH RH cmb compress compressN teethGenesis [honestStep] :=
  { chained := ⟨rfl, trivial⟩, bound := trivial }

/-- **`honest_step_executes` (non-vacuity).** The honest step's turn commits:
`recCexec teethGenesis honestTurn` is `some`. So `wellformed_is_run`/`…_conserves` apply to a REAL
non-empty history, not a vacuous `none`. -/
theorem honest_step_executes :
    (recCexec teethGenesis honestTurn).isSome = true := by decide

/-- **`honest_chain_kernelChained` (non-vacuity of the kernel-continuity path).** The honest
single-step chain is `KernelChained` from genesis — exhibited by projecting its `StateChained` witness
(`kernelChained_of_stateChained`). So the kernel-continuity conservation path (`kernelChained_conserves`
/ `verified_history_conserves`) fires on a REAL chain, not a vacuous one. -/
theorem honest_chain_kernelChained :
    KernelChained teethGenesis [honestStep] :=
  kernelChained_of_stateChained teethGenesis [honestStep] ⟨rfl, trivial⟩

/-- **`honest_kernelChained_conserves` (the conservation ENGINE fires).** The honest chain conserves
the ledger total through the KERNEL-continuity path (`kernelChained_conserves`, the conservation engine
the verification headline rides) — `recTotal` at the endpoint equals genesis (`100`, cell-0 debit 10 =
cell-1 credit 10). This witnesses that the NEW conservation-from-kernel-continuity machinery delivers a
TRUE conservation fact on a real executor run, independent of any `StateChained`/`Run`. -/
theorem honest_kernelChained_conserves :
    recTotal (lastStateOf teethGenesis [honestStep]).kernel = recTotal teethGenesis.kernel :=
  kernelChained_conserves teethGenesis [honestStep] honest_chain_kernelChained

/-- **`honest_total_is_100` (the conserved value is REAL, not a formal husk).** Reading the conclusion:
the honest chain's endpoint ledger total is exactly `100` — the genuine conserved supply. So the
conservation the kernel-continuity path proves is a TRUE arithmetic fact about a real run. -/
theorem honest_total_is_100 :
    recTotal (lastStateOf teethGenesis [honestStep]).kernel = 100 := by
  rw [honest_kernelChained_conserves]; decide

/-- **`tooth_rejects_broken_order` (THE ANTI-GHOST TOOTH, witnessed negative).** A reordered
/ spliced chain is NOT `ChainBound`: if the first step's `newRoot` differs from the second's
`oldRoot`, the `Continues` tooth fails, so `ChainBound` is false. We witness this abstractly: for ANY
two steps whose seam roots disagree, `ChainBound [s, s']` is `False` — exactly the
`TurnChainError::ChainBreak` rejection (`ivc_turn_chain.rs:138`), proving the binding is non-vacuous
(it separates a continuous order from a tampered one). -/
theorem tooth_rejects_broken_order (s s' : ChainStep)
    (hbreak : ChainStep.newRoot CH RH cmb compress compressN s
                ≠ ChainStep.oldRoot CH RH cmb compress compressN s') :
    ¬ ChainBound CH RH cmb compress compressN [s, s'] := by
  intro h
  exact hbreak h.1

end Portal

/-! ## 8. Axiom hygiene. -/

#assert_axioms Dregg2.Distributed.HistoryAggregation.root_tooth_pins_state
#assert_axioms Dregg2.Distributed.HistoryAggregation.root_tooth_pins_kernel
#assert_axioms Dregg2.Distributed.HistoryAggregation.wellformed_is_run
#assert_axioms Dregg2.Distributed.HistoryAggregation.wellformed_history_conserves
#assert_axioms Dregg2.Distributed.HistoryAggregation.wellformed_attests_whole_history
#assert_axioms Dregg2.Distributed.HistoryAggregation.tooth_rejects_broken_order
-- the §8-root binds the KERNEL (not the log): conservation-over-history from VERIFICATION, not from a
-- prover-supplied StateChained hypothesis (the critique's CRITICAL-3 closure):
#assert_axioms Dregg2.Distributed.HistoryAggregation.recCexec_kernel
#assert_axioms Dregg2.Distributed.HistoryAggregation.kernelChained_of_stateChained
#assert_axioms Dregg2.Distributed.HistoryAggregation.kernelChained_conserves
#assert_axioms Dregg2.Distributed.HistoryAggregation.kernelChained_of_verified
#assert_axioms Dregg2.Distributed.HistoryAggregation.verified_history_conserves
-- non-vacuity of the kernel-continuity conservation path (witnessed on a real executor run):
#assert_axioms Dregg2.Distributed.HistoryAggregation.honest_chain_kernelChained
#assert_axioms Dregg2.Distributed.HistoryAggregation.honest_kernelChained_conserves
#assert_axioms Dregg2.Distributed.HistoryAggregation.honest_total_is_100

end Dregg2.Distributed.HistoryAggregation
