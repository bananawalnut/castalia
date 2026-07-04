# dregg ⋈ polyana — the substrate alliance

> polyana: *"does not trust platforms, dependencies, models, tools, or itself without evidence."*
> dregg: *"a turn is the exercise of an attenuable proof-carrying token over owned state, leaving a verifiable receipt."*

These are the same thesis stated from two ends. polyana (David's, `~/pug/polyana`) is an **evidence-native, capability-gated, sandbox-tiered, durable-replay runtime substrate for polyglot agents/tools** — one control plane, 34 language families, a ladder of sandbox tiers, a durable log, capability gates at every boundary. dregg is the **formally-verified core of that exact claim**: a proven capability algebra, an unforgeable receipt, a machine-checked non-omission certificate, a sandboxed-firmament target ladder, and a recovery monitor built from the same artifact-over-assertion discipline.

A previous skeptic called the fit a category error. The skeptic never opened polyana. This document opens it and makes the case fully, with `file:line` on both sides — and honestly, marking where the two compose and where the seam genuinely is.

The shape of the alliance: **polyana is the breadth (34 langs × 13 providers × the cross-OS APE distribution); dregg is the verified depth (the cap algebra, the receipt identity, the non-omission proof) that polyana's "trust nothing without evidence" thesis is reaching for.** polyana already built, by independent convergence, the *Rust* shape of nearly every dregg primitive. dregg is what those shapes become when you want them *proven* rather than *tested*.

---

## 1. The primitive map (file:line, both sides)

Each row: a polyana primitive, its dregg twin, and the one-line reason they are the same object.

### 1.1 "capability gates at every boundary" ↔ dregg caps + `is_attenuation` + ToolGateway

| polyana | dregg |
|---|---|
| `EnforcementLevel` (8-tier isolation floor) — `src/core/src/provider.rs:81-140` | `AuthRequired` rights lattice — `cell/src/capability.rs` |
| `EffectIntent` (ToolCall/ModelCall/Network/Filesystem/PreviewLoad) — `src/policy/src/intent.rs:22-81` | `allowed_effects` on the cap + the executor's effect vocabulary |
| `check_intent()` (pure policy gate, empty-allowlist = deny-all) — `src/policy/src/check.rs:31-110` | `deleg_admit(g, now, tool, old, new)` (5 conjuncts, fail-closed) — `sdk/src/tool_gateway.rs:138-144` |
| `PolicyRegistry` (tenant→policy, F-TRI-036 confused-deputy guard) — `src/policy/src/registry.rs:34-162` | `ToolGateway` (admit→meter→execute) — `sdk/src/tool_gateway.rs:354-830` |
| `BiscuitScope::{TenantInvoke,AuditInvoke}` + `Principal` — `src/core/src/biscuit.rs:135-177`, `src/auth-wit/src/lib.rs:74-84` | the capability + `is_attenuation(held, granted) = granted.is_narrower_or_equal(held)` — `cell/src/capability.rs:614-615` |
| "refuses silent isolation downgrades" — `provider.rs:728-763` (`choose()` filters `>= min_enforcement`) | the monotone-restriction lattice law: `is_narrower_or_equal` is the *only* legal cap motion — `cell/src/capability.rs:401,432,489` |

**Why it's the same object.** polyana already uses **biscuits** — the macaroon/biscuit Datalog token that *is the literal historical seed of dregg* ("macaroons/biscuits → biscuit's Datalog became the derivation circuit"). polyana's `check_intent` is dregg's `deleg_admit` without the proof: both are pure, fail-closed, allowlist-deny-by-default predicates over a scoped token. polyana's "refuse silent downgrade" (`min_enforcement` filter) is dregg's "the only legal cap motion is `granted ⊆ held`" — except dregg's version is *proven* monotone in Lean and tested byte-identically against the Lean crown (`tool_gateway_admit_mirrors_lean_delegadmit`, `sdk/src/tool_gateway.rs:846-867`). **Every polyana boundary crossing is, structurally, a cap-gated dregg turn.**

### 1.2 "many sandbox tiers" ↔ the firmament `Target` distance axis + host-PD

| polyana | dregg |
|---|---|
| `EnforcementLevel`: `None < OsSandbox < JsIsolate < Container < JvmSandbox < WasmSandbox < WasmFullSandbox < Kernel` — `src/core/src/provider.rs:81-140,930-943` | `Target`: `Local{slot} / Distributed{cell} / Surface{cell} / HostPd{pd}` — `sel4/dregg-firmament/src/lib.rs:229-310` |
| `ExecutionProvider` trait (9 methods; wasmtime/wasmi/native/v8/graal/jvm/container/firecracker/bpf) — `src/core/src/provider.rs:520-663` | `Confinement` + `confine_child()` (macOS Seatbelt SBPL deny-default; Linux unshare+seccomp+Landlock) — `sel4/dregg-firmament/src/sandbox.rs:64-97,138-153` |
| seccomp `BASELINE_ALLOW_LIST` (34 syscalls; F-TRI-054 omits kill/tgkill/clone/clone3) — `src/cage-primitives/src/seccomp.rs:122-162` | `HostPdBacking::invoke()` validates op authority via `is_attenuation`, probes the Endpoint peer-addr — `sel4/dregg-firmament/src/host_pd.rs:99-131` |
| `instantiate_with_caps()` binds seccomp+Landlock at fork — `src/core/src/provider.rs:560-568` | the kernel-owned `ValidityTable` (slot→epoch+kind+rights; PD cannot forge an entry) — `sel4/dregg-firmament/src/process_kernel.rs:269-301` |

**Why it's the same object.** polyana's tier *ladder* (wasmtime → seccomp+landlock native → V8 → JVM → container → Firecracker → kernel-eBPF) is dregg's firmament **distance axis** seen as isolation strength instead of network distance. They are two projections of one parameter: "how far is the authority surface from the caller, and what bounds hold there." dregg's `Target::HostPd` is *literally* polyana's `OsSandbox` tier — a forked, OS-sandboxed child whose **only channel is the Endpoint** (`lib.rs:264-273`), confined by the same Seatbelt/seccomp/Landlock primitives polyana uses in `cage-primitives`. The difference dregg adds: the cap-confined Endpoint is the *sole authority surface* (the child cannot open files / reach net / exec), and the `ValidityTable` makes the cap **unforgeable from inside the PD** — a kernel-memory guarantee polyana's `PolicyRegistry` approximates in user-space `Arc<RwLock<HashMap>>`. dregg's tier ladder is the same ladder with a *verified* floor.

### 1.3 "evidence-native" / `pa_witness` / `pa_attest` ↔ dregg receipts + blocklace + non-omission certificate

| polyana | dregg |
|---|---|
| `TraceRecord` (seq, fn_name, args, ret; byte-equal across providers) — `src/core/src/provider.rs:324-336` | `Receipt` = `TurnReceipt` (turn/forest/effect hashes, pre/post state roots, `previous_receipt_hash` chain) + lazy `TurnProof` — `sdk/src/receipt.rs:89-99` |
| `CanonicalValue` (F32Bits/F64Bits keep NaN; BTreeMap keys sorted for byte-equality) — `src/core/src/provider.rs:227-245` | sorted-Poseidon2 canonical commitment everywhere (the dregg3 unification) |
| `EventKind` (10 kinds incl. `EffectDenied` audit, `HostHandoff`) — `src/durable/src/lib.rs:133-250` | the blocklace append-only DAG (Ed25519 feed integrity, total order + finality) — `blocklace/src/lib.rs:337+` |
| `audit-mcp` `pa_attest` / ed25519 federation `AttestationToken` — `src/audit-mcp/src/lib.rs:1-88`, `src/server/src/lib.rs:126-129` | the **attested non-omission certificate**: `AttestedAnswer` + MMR range opening against the receipt-log root — `dregg-query/src/lib.rs:12-30,48` |

**Why it's the same object.** polyana's evidence is a `TraceRecord` written to a durable log and (for federation) ed25519-signed. dregg's evidence is a `Receipt` whose identity is a cryptographic commitment to the turn, chained (`previous_receipt_hash`), and — the part polyana does not yet have — covered by a **machine-checked non-omission certificate**: `dregg-query` answers carry an MMR range opening so "a verifying answer is provably computed from EXACTLY the committed receipt range — nothing hidden, nothing forged, nothing reordered" (`dregg-query/src/lib.rs:13-16`), the Rust embodiment of `metatheory/Dregg2/Lightclient/MMR.lean`'s `server_cannot_omit_position`. **polyana's evidence is dregg's receipt-identity made *unforgeable* and the audit log made *provably non-omitting*.** `pa_witness` emitting a dregg receipt instead of a `TraceRecord` is the single highest-leverage seam (§3).

### 1.4 "durable replay" ↔ blocklace log + `collapse` (symbolic/Full) + orthogonal persistence

| polyana | dregg |
|---|---|
| `ReplayEngine` (fold events left-to-right → `WorkflowState`; identical logs → equal result) — `src/durable/src/replay_engine.rs:17-43` | `collapse()` re-runs recorded symbolic turns under Full execution; re-derives byte-identical receipts — `turn/src/collapse.rs:171-182` |
| `polyana_bincode` legacy config (`serialize(a)==serialize(b)` iff semantically equal) — `src/bincode-helper/src/lib.rs:1-55` | `WitnessMode::{Full,Symbolic}` (Symbolic defers Merkle materialization; admission gates *never* deferred) — `turn/src/collapse.rs:99-138` |
| `validate_provider_substitution()` / `validate_host_substitution()` (replay refuses downgrade) — `src/durable/src/replay_engine.rs:135-251` | the turn tape is the durable fact; the witness is the derived artifact (orthogonal persistence) |

**Why it's the same object.** Both replay from an append-only event log; both enforce determinism by canonical byte-equal encoding (`polyana_bincode` legacy fixint ↔ dregg's sorted-Poseidon2 / pinned cost+timestamp). polyana's "a workflow on Wasmtime today is byte-equal-replayable on Wasmi tomorrow" is dregg's `collapse`: re-run the recorded tape, re-derive the *identical* receipt, or raise an integrity event. The Houyhnhnm orthogonal-persistence story (`docs/deos/DISTRIBUTED-TIMETRAVEL-SEMANTICS.md`) is the shared north star — and dregg adds the load-bearing distinction polyana states as a principle but cannot enforce: **admission is mode-independent** (`collapse.rs:23-34`). Symbolic mode skips computing the Merkle root; it *never* skips a legality decision. polyana's "no fast path that drops the audit trail" becomes a *proof obligation* rather than a code-review rule.

### 1.5 "one control plane" ↔ the dregg node + ToolGateway-as-router

| polyana | dregg |
|---|---|
| `ServerState` (actors, workflows, providers, policy, federation, raft, signing key) — `src/server/src/lib.rs:111-189` | the dregg node + the firmament `Router` (dispatches by `Target`) — `sel4/dregg-firmament/src/lib.rs` |
| `ProviderRegistry::choose()` data-plane dispatch — `src/core/src/provider.rs:676-854` | `ToolGateway` admit→enqueue→execute→results-back; `enqueue()` → `RoutedHandle` (EventualRef promise) — `sdk/src/tool_gateway.rs:511-735` |
| axum router + biscuit-auth middleware + WebSocket events — `src/server/src/lib.rs:1-35` | the metered cap-gated worker with method-scoped biscuit credential — `sdk/src/tool_gateway.rs:638-698` |

**Why it's the same object.** polyana's control plane *picks a provider and routes an invocation*; dregg's ToolGateway *gates a cap and routes a turn*. polyana's `choose()` (filter by enforcement floor, walk preferred, fall back to host chain) and dregg's `enqueue → drive_executor → resolve` are the same admit-then-route pipeline. The polyana control plane could **be** a dregg node: tool/guest invocations become routed, cap-gated, receipted turns, and the EventualRef promise (`RoutedHandle`, `tool_gateway.rs:309-324`) is exactly the partial-turn / CapTP-pipelining seam dregg already built (`docs/deos/...`, the partial-turn memory).

### 1.6 the recovery monitor (artifact-over-assertion) ↔ `pa_doctor` / `pa_health` / supportbot

| polyana | dregg |
|---|---|
| `HealthDaemon` poll loop + `HealthSampler` probes (load_component/instantiate) — `src/health-daemon/src/lib.rs:107-193` | `RecoveryMonitor::tick()` — reads the **live artifact**, never the claim — `sel4/dregg-firmament/src/recovery_monitor.rs:333-427` |
| `HealthStatus` tri-state (Healthy/Degraded/Unhealthy/Unknown) — `src/health-daemon/src/lib.rs:50-58` | `Subsystem::probe()` (live artifact) vs `claim()` (self-report), separated so divergence is detectable — `recovery_monitor.rs:125-152` |
| supportbot "consumes evidence bundles, never invents root causes" — `src/core/src/supportbot.rs` | `Divergence::RecoveryNotHolding` (claim says recovered, artifact refutes) + loop-guard `Escalation` — `recovery_monitor.rs:156-184` |

**Why it's the same object — and why David in particular loves it.** This is the *identical discipline*, born from the same scar. polyana's CLAUDE.md records a **15-hour WSL wedge** where a watcher "watched the symptom path, not the recoverable system" (`recovery-monitor-watched-symptom-path-not-recoverable-system`). dregg's `recovery_monitor.rs` keystone (lines 23-38) is written *to that exact failure*: "THE MONITOR NEVER READS THE SUBSYSTEM'S CLAIM. It reads the subsystem's `probe` — the live artifact... When the subsystem claims `Healthy` but the probe says `Wedged`, the monitor emits `Divergence::RecoveryNotHolding` — the council's exact signal from the 15-hour wedge." dregg also adds the loop-guard polyana's incident *wanted*: after `max_attempts` re-wedges in a window, **escalate instead of looping forever**, fail-closed until supervisory reset (`recovery_monitor.rs:214,333-336`). This is the recovery monitor polyana would have built; dregg already built it and made it recursive (`MonitorSubsystem`, `recovery_monitor.rs:442-508` — turtles all the way up).

### 1.7 `pa_federation` ↔ dregg federation + firmament "one cap across distance"

| polyana | dregg |
|---|---|
| ape-to-ape federation, ed25519-attested peers (re-register pubkey on restart) — `src/server/src/lib.rs:126-129`; `src/ape-to-ape-federation/` | `Federation` (committee, epoch, threshold, local seat; `id = H(sorted(members) ‖ epoch)`) — `federation/src/federation.rs:69-151` |
| `HostHandoff` event (cross-host dispatch + WIT-world projection) — `src/durable/src/lib.rs` | `Target::Distributed{cell}` / `Target::Surface{cell}` — ONE cap handle across distance — `sel4/dregg-firmament/src/lib.rs:1-46,238-262` |

**Why it's the same object.** polyana federates by ed25519-signing a peer handoff; dregg federates by a threshold committee with epoch-derived identity and a **proven** quorum gate. The firmament thesis (`lib.rs:1-46`) is the unification polyana's federation is reaching for: an seL4 cap, a distributed dregg cap, and a window-surface cap are the *same abstraction at different points on a distance parameter n*. The app holds ONE `Capability{target, rights}` and attenuates/delegates/revokes it the same way regardless of distance — exactly what polyana wants when an APE on host A hands work to an APE on host B.

---

## 2. The alliance architecture — dregg as polyana's verified core

```
                  polyana (the BREADTH)
   ┌──────────────────────────────────────────────────────────┐
   │  34 lang families · 13 providers · GraalVM polyglot · APE  │
   │  .com cross-OS distribution · pa-mcp surfaces · SDK fleet  │
   │                                                            │
   │   control plane (ServerState / axum / ProviderRegistry)    │
   │           │ admit            │ route           │ replay     │
   └───────────┼──────────────────┼─────────────────┼───────────┘
               ▼                  ▼                 ▼
   ┌──────────────────────────────────────────────────────────┐
   │   dregg (the verified DEPTH)                               │
   │                                                            │
   │  cap algebra        ToolGateway      receipt + non-omission│
   │  is_attenuation  ·  admit→route→  ·  + MMR cert (Lean)     │
   │  (Lean-proven)      results-back     blocklace finality    │
   │                                                            │
   │  firmament Target ladder            recovery monitor       │
   │  Local·Distributed·Surface·HostPd   probe-not-claim,       │
   │  (one cap across distance)          fail-closed escalate   │
   └──────────────────────────────────────────────────────────┘
```

dregg is the layer polyana's seven core principles *terminate in*:

| polyana principle | dregg makes it… |
|---|---|
| 1. Probe before assuming | the recovery monitor: probe the **artifact**, never the claim |
| 2. Declare before executing | the cap manifest = `allowed_effects` on an attenuable cap; `deleg_admit` enforces it |
| 3. Sandbox before trusting | `Target::HostPd` — the cap-confined Endpoint as the *sole* authority surface, `ValidityTable`-unforgeable |
| 4. Checkpoint before side effects | `WitnessMode` + `collapse`: the turn tape is the durable fact, admission is mode-independent |
| 5. Hash before caching | the receipt **is** a content-address commitment; sorted-Poseidon2 canonical form |
| 6. Explain before repairing | the non-omission certificate: the evidence bundle is *provably complete* |
| 7. Constrain before optimizing | the monotone-restriction lattice law, proven in Lean |

**dregg is the verified evidence kernel + the proven cap algebra + the recovery monitor that polyana's "trust nothing without evidence" thesis was built to deserve.** polyana converged on the Rust *shape* of every one of these by independent engineering. dregg is what those shapes are when you stop testing them and start *proving* them.

---

## 3. The honest seams — where they compose, where the seam is

This is the real fit, told honestly.

**Where they compose cleanly (Rust ↔ Rust):**
- dregg's `cell`, `sdk` (ToolGateway, receipt), `blocklace`, `dregg-query`, `federation`, and `sel4/dregg-firmament` are all **plain Rust crates**. polyana is a Rust workspace (17+ provider crates). A polyana provider or middleware can `use dregg_sdk::tool_gateway::ToolGateway` directly. No FFI, no language boundary.
- polyana already speaks **biscuits** (`src/core/src/biscuit.rs`) — the same token family dregg grew out of. The cap-token wire format is shared ancestry, not a translation.
- polyana's `Target`-shaped tier ladder maps onto dregg's `Target` enum 1:1 for the OS-sandbox tier (`HostPd`) and the federation tier (`Distributed`).

**Where the seam genuinely is (be honest):**
1. **Lean is not in polyana's loop.** dregg's guarantees are *Lean-proven* (`metatheory/`); polyana ships Rust+GraalVM+APE and proves things by *runtime-proven function gates* and IRL tests. The Lean proofs do not run in polyana's CI and should not — the seam is: **dregg exports a Rust crate whose API is the Lean-proven surface (`is_attenuation`, the MMR verifier, `deleg_admit`), and polyana consumes the Rust, trusting the Lean the way it trusts ring/ed25519-dalek.** The proof is dregg's to maintain; the API is the contract.
2. **dregg's commitment is Poseidon2/STARK-shaped; polyana's is bincode/blake3-shaped.** dregg's receipt commitment targets the circuit; polyana's `TraceRecord` is `polyana_bincode` legacy. The seam: pa_witness can emit a dregg `Receipt` *alongside* its `TraceRecord` (additive, §4 slice 1) — the receipt is the unforgeable one; the trace stays for human debugging. They are not byte-identical and need not be.
3. **dregg's executor is one verified executor; polyana's is 13 providers.** dregg does not run Java-in-APE or V8 isolates. The seam is *not* "dregg executes polyana's guests" — it's "dregg **gates and receipts** polyana's guest invocations." polyana keeps its breadth (the 34 langs); dregg supplies the cap gate, the receipt, the non-omission proof, and — for the OS-sandbox tier specifically — a verified `HostPd` confinement that can *replace* `cage-primitives` rather than duplicate it.
4. **dregg's circuit is mid-campaign.** The light-client unfoolability apex is an active campaign (`docs/CIRCUIT-FUNCTIONAL-CORRECTNESS.md`), not finished. The honest claim to David: the **cap algebra and the non-omission certificate are proven today**; the full in-circuit witnessing is in progress. The alliance can adopt the proven layers now without waiting on the circuit apex.

**What dregg must NOT claim:** that it executes polyglot guests, that the circuit is done, or that byte-equality with polyana's trace format is required. The fit is *gate / attest / replay / confine*, not *re-implement polyana's runtime*.

---

## 4. The smallest adoptable seam — sequenced by value

Three slices, each independently shippable, ordered by value-per-effort. Each is **one boundary**, additive, non-breaking.

### Slice 1 (highest value, smallest blast radius): `pa_witness` emits a dregg `Receipt`

polyana already writes a `TraceRecord` per call (`provider.rs:324-336`) and an `EffectDenied` audit event. The seam: at the `audit-mcp` / `pa_witness` boundary, *also* construct a dregg `TurnReceipt` (`sdk/src/receipt.rs:89-99`) keyed on the same `(seq, fn_name, args, ret)`, chained via `previous_receipt_hash`. Then a polyana operator can run `dregg-query`'s attested-answer path (`dregg-query/src/lib.rs:48`) over the receipt log and get a **provable non-omission certificate** — "this audit answer is computed from exactly the committed receipt range, nothing hidden." This is the single thing polyana's evidence story most wants and does not have, and it is purely additive: the `TraceRecord` stays for humans, the `Receipt` is the unforgeable spine. **Value: turns "evidence-native" from a discipline into a proof.**

### Slice 2: polyana's `OsSandbox` tier = a dregg firmament `HostPd`

polyana's `cage-primitives` (seccomp `BASELINE_ALLOW_LIST`, Landlock, F-TRI-054 denials) and dregg's `sandbox.rs` `confine_child()` are ~the same primitives. The seam: a polyana `ExecutionProvider` whose enforcement tier is `OsSandbox` delegates confinement to `dregg_firmament::sandbox::confine_child` + reaches the guest over the firmament Endpoint (`Target::HostPd`). The guest's *only* authority surface becomes the cap-confined Endpoint, and the cap is `ValidityTable`-unforgeable. polyana gets a **verified** OS-sandbox tier — confinement plus an unforgeable cap, instead of a user-space allowlist. **Value: the one tier where dregg's verified confinement strictly dominates can replace, not duplicate.**

### Slice 3: polyana's `cap-bundle` gated by dregg caps

polyana's `cap-bundle/default.toml` is *already a capability manifest* (filesystem-read, network-localhost, deterministic-window, streaming) mirroring `src/core/src/capability.rs` constructors. The seam: parse the cap-bundle into a dregg `AuthRequired` and gate every boundary crossing through `is_attenuation(held, granted)` (`cell/src/capability.rs:614`) + `deleg_admit` (`sdk/src/tool_gateway.rs:138`). A guest requesting more than the bundle grants is refused by the **Lean-proven** monotone gate, not a hand-rolled check. **Value: the whole boundary-gate story gets a proven algebra; the cap-bundle becomes an attenuable dregg cap.**

**Sequence rationale:** Slice 1 is pure addition at one MCP surface (no provider changes, immediate evidence payoff). Slice 2 is one provider crate and the highest *security* payoff (verified confinement). Slice 3 is the broadest reach but touches every boundary, so it goes last. Adoption is attenuation, at both ends — exactly the dregg product ladder (`dregg-auth = the gateway; adoption IS attenuation`).

---

## 5. Why David loves it

David's whole thesis — *trust nothing without evidence* — is a **distrust discipline reaching for a verified kernel it didn't have time to build.** polyana proves things by runtime function-gates and IRL tests (PIN-3: "commit without IRL test" forbidden); that is rigorous, but it is *testing*, not *proof*. dregg hands David:

- **A formally-verified evidence kernel.** The non-omission certificate (`server_cannot_omit_position`, machine-checked in Lean) is the thing that makes "evidence-native" *true* rather than *asserted*. polyana's audit trail becomes provably complete.
- **A proven cap algebra.** `is_attenuation` is the monotone-restriction law David's `check_intent` / `min_enforcement` / biscuit scopes all approximate — proven once, in Lean, byte-tested against the Rust. "Refuses silent downgrades" stops being a code-review rule and becomes a theorem.
- **The recovery monitor he already paid for.** The 15-hour WSL wedge that scarred polyana's CLAUDE.md is the *exact* failure dregg's `recovery_monitor.rs` is written against — probe the artifact, not the claim; escalate instead of looping; recursive supervision. dregg built the monitor polyana's incident proves it needed.
- **Shared blood.** polyana runs on biscuits; dregg grew out of biscuits. The same object-cap lineage David and @emberian co-developed (the LICENSE co-dev clause, the dogfood targets) shows up as a *technical* convergence, not just a social one. Two people who think in capabilities, building the two halves of one substrate.

The honest pitch: **polyana is the substrate that runs everything; dregg is the proof that it can be trusted.** They are not competitors and not a category error — they are the breadth and the depth of a single sentence: *"a turn is the exercise of an attenuable proof-carrying token over owned state, leaving a verifiable receipt"* is what *"trust nothing without evidence"* compiles to when you demand the evidence be a proof.

---

## Appendix: concrete seam sketch — now WIRED

A compiling-shape sketch of Slice 1 lives at `docs/deos/polyana-seam-sketch.rs` (illustrative — names match the cited types). It shows a polyana-side `pa_witness` boundary constructing a dregg `TurnReceipt` from a polyana `TraceRecord`, and gating the call through `is_attenuation`.

⚑ The sketch is now **realized** as the `polyana-bridge` crate (workspace member) — the Rust API surface polyana consumes, built against the real dregg crates:

- **Slice 3** — `gate_effect_set` / `gate_auth` enforce the cap-bundle through the **proven** `dregg_cell::facet::is_facet_attenuation` (effect-set face) and `dregg_cell::is_attenuation` (auth-kind face); unknown effect tokens fail closed. The bit assignment is the bridge's stable intern of polyana's effect vocabulary; the *algebra* is dregg's Lean-backed monotone law (`polyana-bridge/src/caps.rs`).
- **Slice 1** — `witness_receipt` builds a real chained `dregg_turn::TurnReceipt` whose `dregg-receipt-v3` hash binds `previous_receipt_hash` (tamper-evident chain) from a polyana `TraceRecord` (`polyana-bridge/src/witness.rs`).
- **Slice 1's payoff** — `audit_records` + `attest_whole_log` assemble a `dregg_query::AttestedSlice`; `answer_whole_log(...).verify(&Blake3Mmr, &root)` is the **non-omission certificate** over the audit log (`server_cannot_omit_position`), proven in `tests/seam.rs` to verify against the genuine root and reject an omission or a wrong root (`polyana-bridge/src/attest.rs`).

Slice 2 (the `Target::HostPd` confinement tier) remains in `sel4/dregg-firmament`, not re-exported by the bridge. The whole seam is tested in `polyana-bridge/tests/seam.rs` (7 tests, green).

## Provenance

polyana inspected read-only at `~/pug/polyana` (commit state 2026-06-13; substrate snapshot 2026-06-11, 34 lang families). dregg citations verified against HEAD. No polyana files modified; no churning dregg crates touched. All `file:line` citations confirmed live at write time.
