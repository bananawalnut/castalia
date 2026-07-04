# Performance Characterization — DreggNet-as-a-cloud

The perf-engineering foundation for DreggNet-as-a-cloud: what exists to measure,
what the cloud must measure, and how it scales WIDE / VERTICALLY / via
FEDERATION. This is the inventory + the plan the bench-building lanes execute
against. It is read-mostly; it adds no benches itself.

DreggNet-as-a-cloud is the agent-business substrate: an agent pays, opens a
lease, runs a workload in a cap-bounded sandbox, gets metered, and the lease is
reaped — every step a verified turn over owned state leaving a receipt. Perf is
therefore measured across two repos:

- `breadstuffs` — the dregg kernel/circuit/turn substrate (`dregg-perf` + the
  per-crate criterion benches). This is the proving/verifying/commit floor.
- `DreggNet` — the cloud service layer (lease lifecycle, sandbox tiers,
  durable store, gateway/webapp, bridge). This is the workload/throughput floor.

---

## 1. Bench inventory — what exists today

### 1.1 breadstuffs — the `dregg-perf` crate

`perf/` (`dregg-perf`, `perf/Cargo.toml`) is the production turn-proof perf
suite: 16 criterion benches + 4 binaries, every one over the PUBLIC API of the
live commit/prove path. Each bench is SMOKE (default, tiny input) vs FULL
(`PERF_FULL=1`, the persvati capture input) — `perf/src/lib.rs::perf_full()`.

| Bench (`perf/benches/`) | Measures | Live function |
| --- | --- | --- |
| `symbolic_collapse.rs` | THE headline interactivity number: N transfer turns Symbolic vs Full + `World::collapse(N)` | `World::commit_turn` (Symbolic/Full), `World::collapse` |
| `executor_turn.rs` | bare Rust executor hot path (the floor under every turn) | `TurnExecutor::execute` |
| `embedded_commit.rs` | verified Lean kernel commit (node / seL4-executor-PD hot path) | `shadow_exec_full_forest_auth` |
| `lean_ffi_turn.rs` | per-turn FFI tax: `execute_via_lean` JSON round-trip vs bare Rust (text table, `harness=false`) | `execute_via_lean` vs `TurnExecutor::execute` |
| `turn_witness_vs_proving.rs` | the proving multiplier, witness-only vs full-prove side by side | `execute` / `generate_effect_vm_trace` / `prove_full_turn` / `verify_full_turn` |
| `prove.rs` / `verify.rs` / `turn_proof.rs` | rotated full-turn prove + light-client verify | `prove_full_turn` / `verify_full_turn` / `prove_vm_descriptor2` |
| `cohort_circuit.rs` | rotated IR-v2 multi-table batch STARK per effect-cohort (transfer 5-table / map-write chip / umem no-chip / absent chip) | `prove_vm_descriptor2(_umem)` / `verify_vm_descriptor2` |
| `recursion_fold.rs` | recursive aggregation fold (2/8/32/128 leaves) | `prove_tree_fold_v2` / `verify_tree_fold_v2` |
| `trace_gen.rs` | witness generation (base trace + descriptor matrix) | `generate_effect_vm_trace`, `descriptor_recursion_matrix` |
| `commitment.rs` | per-cell state commitment (v8 + v9 rotated, the rotation long pole) | `compute_canonical_state_commitment(_v9)` |
| `poseidon2.rs` | the in-circuit hash primitive (permute / 2→1 / sponge) | `Poseidon2State::permute`, `hash_2_to_1`, `hash_many` |
| `membrane.rs` | "invite someone to my computer": fork / mint cap / drive gated / stitch + roundtrip | `World::fork`, `SharedFork::construct`, `commit_turn_gated`, `Stitch::settle` |
| `data_plane.rs` | cap-gated receipted message Bus (enqueue / drain / publish fan-out S∈{4,32,128}) | `captp::data_plane::Bus` |
| `ui_projection.rs` | deos first-paint DATA cost (gpui-free; GPU paint is on persvati) | `demo_world`, `Shell::compose_scene/compose`, `AffordanceSurface::project_for` |

Binaries: `perf-report` (full-system report on the real commit path),
`proof-sizes` (wire-byte regression vs committed baseline), `perf-summary`,
`orchestration-demo` (the end-to-end multi-agent polis loop, every leg
wall-clock timed). Docs: `perf/BENCHMARKS.md`; baselines under
`perf/baselines/<name>/` (latest: `smoke-2026-06-22-m2max`, M2 Max,
git `fb0c01e2`).

### 1.2 breadstuffs — per-crate criterion benches

| Bench | Measures |
| --- | --- |
| `bridge/benches/solana_verify_bench.rs` | trustless Solana lock-proof verify, O(votes): Ed25519/vote + Merkle fold + PoH chain, at 100/500/1500 votes (mainnet ~1500). placeholder vs real-wire-witness paths. |
| `bridge/benches/presentation_bench.rs` | bridge presentation proof: fast constraint-check / Poseidon2 STARK / IVC / macaroon→factset / end-to-end mint→attenuate→prove→verify |
| `circuit/benches/stark_bench.rs` | STARK prove/verify/size (Merkle depth 4/8/16/32), Poseidon2, BabyBear field ops, IVC accumulate |
| `circuit/benches/proof_benchmarks.rs` | comprehensive prove/verify/compose (note-spend, derivation 1/8/32, IVC, non-revocation 1/4/8, chunked auth) |
| `circuit/benches/practical_benchmarks.rs` | end-to-end workflows (token ops, caveat chains 5/10/20, federation turn, revocation registry, headline E2E pipeline) |
| `token/benches/token_bench.rs` | Macaroon/Biscuit mint/verify/attenuate, attenuation chains 1/5/10/20, revocation lookup over 1K/10K/100K sets |
| `macaroon/benches/macaroon_bench.rs` | macaroon create/verify/serde/encode, third-party flow, HMAC chain |
| `federation/benches/consensus_bench.rs` | BLS partial-sign / aggregate / verify at committee 3/5/7 + full 5-node round |
| `hints/benches/criterion.rs` | threshold crypto (KZG setup, BLS aggregate, SNARK hint gen/verify) |
| `wire/benches/wire_bench.rs` | wire codec encode/decode + STARK-over-wire (~24 KiB proof) |
| `tokenizer/benches/tokenizer_bench.rs` | SealedSecret keypair/seal/unseal (37 B + 4 KiB payloads) |
| `pg-dregg/benches/pg_dregg_bench.rs` | dregg-in-Postgres: write-gate (cold chain-verify vs hot LRU), RLS gate over 100/1000 rows, durable workflow 16/128 steps + crash-recover, gate-vs-handrolled-ACL |
| `dregg-lean-ffi/benches/direct_vs_json_overhead.rs` | per-turn FFI: JSON marshal path vs no-copy direct path (text table, not criterion) |

(`circuit-prove/` has no benches dir; the recursion-prove surface is measured
through `perf/`.)

### 1.3 DreggNet — the cloud service layer

DreggNet's bench infrastructure lives mostly in `polyana` (the execution
substrate, its own Apache-2.0 workspace) and the vendored Elide `net/` stack.
The DreggNet-native service crates (`exec` / `durable` / `bridge` / `control` /
`gateway` / `webapp`) currently have NO benches — they are covered only by
integration tests (e.g. `bridge/tests/lease_watcher.rs`,
`bridge/tests/lease_drives_durable_workflow.rs`). **This is the largest gap.**

| Bench | Measures |
| --- | --- |
| `net/httpe/benches/http_alloc.rs` + `alloc_count.rs` | gateway allocation counts (cache store/lookup, header parse zero-alloc, cookie jar); counting global allocator |
| `net/transport/benches/profile.rs` | sustained transport: echo throughput, RTT p50/p90/p99/p999, connect-disconnect churn; `--servers/--clients/--conns/--payload`, JSON out, remote-capable |
| `net/transport/benches/quic_io.rs` | UDP backends: blocking baseline vs io_uring (RecvMsgMulti+GSO/GRO) vs SO_REUSEPORT multi-core; ops/sec + p50/p99 (Linux io_uring) |
| `polyana/benches/src/bench_server_request.rs` | end-to-end HTTP req/sec (axum in-process: `/health`, `/api/v1/actors`) |
| `polyana/benches/src/bench_durable_eventstore.rs` | durable event-store append + replay-fold at 100/1k/10k events (events/sec) |
| `polyana/benches/src/bench_wasm_instantiate.rs` | wasm compile cold / instantiate fresh-store / instantiate pre-pooled (the InstancePool payoff) |
| `polyana/benches/src/bench_provider_comparison.rs` | **wasmtime vs wasmi** cold-start + warm-call (the cap-tier cost split: Cranelift JIT vs pure-Rust interpreter) |
| `polyana/benches/src/bench_policy_decide.rs` | tenant policy hot path (compile / check_intent / authorize_persist) — every cross-boundary effect |
| `polyana/benches/src/bench_capability_set.rs` + `bench_cap_floor.rs` | capability linear scan + R1 cap-floor matcher at set sizes 1/16/64/100 (every boundary crossing) |
| `polyana/benches/src/bench_mailbox.rs` | actor mailbox push/pop, fill/drain 16/256/4096, multi-priority, capacity churn |
| `polyana/benches/src/bench_artifact_store.rs` | content-addressed store (Blake3 dedup) at 1KB/64KB/1MB (bytes/sec) |
| `polyana/src/bench/benches/{wasmtime_instantiate,cross_provider,replay_determinism,handle_codec}.rs` | instantiate hot path, `dyn` provider vtable cost, JCS canonical-bytes (~5µs budget), handle↔ArtifactId codec (<10ns target) |
| `polyana/src/bench-substrate/benches/substrate.rs` | scenario runner with p99.9/p99.99 tail-latency (the ADR-acceptance four-9s envelope) |
| `polyana/benches/graal-bridge-perf/src/main.rs` | subprocess vs in-process JNI bridge; ADR-0053 gate ≥5x (target 10x) |
| `polyana/src/graalvm-polyglot-provider/benches/wave_m_v2d.rs` | JNI hot-path canonical eval (feature-gated) |

**Cap-tiers (sandbox execution model)** — `bridge/src/lib.rs::CapGrade`:
- `Sandboxed` — wasmi (pure-Rust interpreter; lowest cold-start; numeric-only;
  UNMETERED today) or wasmtime (Cranelift JIT; full Component-Model; fuel-metered
  ~50M units default).
- `Caged` — native process under seccomp+Landlock (Linux); includes native
  CPython (`python3` subprocess).
- `MicroVm` — firecracker hardware isolation (KVM).
  (v8/Node steered around — double-`OwnedIsolate` SIGSEGV; GraalVM not wired.)

**Lease lifecycle** — `bridge/src/lib.rs::Lease` + `LeaseWatcher`:
open (`Lease::funded(lessee, cap_grade, asset, budget_units, per_period_units)`)
→ fund (`budget_units>0`, entered into the watcher feed) → run
(`fulfill` spawns a durable polyana workflow; `cap_grade` picks the provider) →
meter (each durable step charges `per_period_units` vs `budget_units`) → reap
(success / over-budget `StandingObligation` lapse `ReapReason::OverBudget` / or
unfunded reaped before start — no unpaid work claimed).

---

## 2. Known numbers (dated baselines)

Last-known numbers, M2 Max unless noted. These are POINT-IN-TIME; treat µs/× as
approximate-unverified until re-captured (the standing perf-doc rule). Sources:
`.docs-history-noclaude/PERFORMANCE.md`, `perf/baselines/smoke-2026-06-22-m2max/`,
`HORIZONLOG.md`.

**Kernel / commit (the WIDE-scaling floor):**
- executor turn (`TurnExecutor::execute`, 2-cell transfer): **~8.2 µs**
- witness-only execute (node admit, no SNARK): **~7 µs**
- embedded Lean kernel commit (`forest_auth_transfer`): **~157 µs**; + verdict decode **~159 µs**
- per-cell commitment: v8 **~225 ms**, v9 rotated **~157 ms** (the rotation long pole)
- Poseidon2: permute **~1.37 µs**, 2→1 **~2.8 µs**, sponge-8 **~4.7 µs**

**Prove / verify (the per-turn proof cost):**
- witness-gen (effect-vm trace): **~319 µs**
- full rotated turn prove: **~147 ms**; verify (light-client): **~149 ms**
- proving multiplier (prove ÷ witness-only): **~21,000×**
- rotated descriptor leg (transfer 5-table) prove **~52 ms** / verify **~3.9 ms**
- map-op (Poseidon2 chip) prove **~227 ms** vs umem (no chip) **~14.9 ms** (~15× chip cost)
- absent (non-membership) prove **~137 ms**
- recursion fold prove: 2-leaf **~10.2 ms** → 8 **~14 ms** → 32 **~35.6 ms** → 128 **~98 ms**; verify ~constant **~2.2–2.8 ms**
- proof sizes: descriptor IR-v2 **120.4 KiB**, rotated R=24 transfer **144.1 KiB**,
  full-turn wire **~169 KiB** (v1 single-table baseline was 350.5 KiB → −65.6%; verify 16.8 ms → 5.0 ms, 3.4×)

**Interactivity (the headline):**
- symbolic vs full batch (N=8): full ~850 ms/turn, symbolic ~110 µs/turn →
  **~7,000×** per-turn fast-path speedup; collapse a one-time ~2.9 s at publish
- membrane: fork ~3.2 µs, stitch settle ~72 ns, mint/drive ~1.4 s / ~938 ms (Full commit), roundtrip ~4.2 s

**Bridge:** Solana verify is O(votes); ~1500-vote mainnet tally is the linear
ceiling the Option-B succinct O(1) wrapper exists to retire
(`docs/deos/SOLANA-SUCCINCT-WRAPPER.md`). (The "~33µs/vote" figure is a
benchmark-derived per-vote estimate, not a hardcoded constant.)

**DATED, do NOT cite as live:** the per-turn-commitment **"~370 ms → ~2.85 µs,
~130,000×"** note (`metatheory/docs/CODEX-DISCHARGE-SKELETON.md:2505`, from
`turn_profile.rs`) is an OLD profiler comment, not a committed benchmark. The
live, measured commit-cost story is the executor/embedded-commit + symbolic
numbers above.

**Soundness floors (the targets perf must not undercut):** FRI ~130-bit
conjectured; state commitment 8-felt ~124-bit (the 1-felt ~31-bit light-client
limb is the known-broken floor being widened —
`.docs-history-noclaude/FAITHFUL-STATE-COMMITMENT.md`).

---

## 3. Gaps — what is NOT measured

1. **The DreggNet cloud service layer has no benches.** `exec`/`durable`/
   `bridge`/`control`/`gateway`/`webapp` are integration-tested, not perf-timed.
   No number exists for: lease lifecycle throughput (open→fund→run→meter→reap
   leases/sec), gateway req/sec end-to-end (httpe→ingress→workload), durable
   checkpoint/resume cost at the DreggNet layer, or webapp (agent-served route)
   req/sec.
2. **No macro/throughput load-gen harness.** Everything in `dregg-perf` and
   `polyana/benches` is a micro-bench (single op, criterion). There is no
   harness that drives N concurrent leases/workloads/agents and reports
   aggregate throughput + tail latency under contention.
3. **No WIDE (multi-node) measurement.** Everything runs in-process on one box.
   N-node throughput, federation gossip/sync, and cross-federation bridge cost
   are unmeasured (one staging box today).
4. **Cap-tier workload latency is not measured end-to-end.** `bench_provider_
   comparison` times wasm instantiate + warm-call in isolation; there is no
   "run a representative agent workload under each tier (native-CPython /
   wasmtime / wasmi), wall-to-wall through the lease" number.
5. **Symbolic/collapse, FFI, and many `dregg-perf` benches lack a committed FULL
   baseline.** The only captured baseline is SMOKE (`smoke-2026-06-22-m2max`).
6. **No flamegraph/`perf` bottleneck-isolation artifacts** are committed; the
   bottleneck per axis is inferred from micro-bench ratios, not profiled.

---

## 4. The plan — characterizing DreggNet-as-a-cloud

### 4.1 The metrics that matter

A cloud is judged on latency, throughput, cost-per-unit-work, and tail
behaviour under load. For DreggNet:

| Metric | Definition | Where measured | Status |
| --- | --- | --- | --- |
| Workload exec latency | wall-to-wall to run one agent workload, per cap-tier (native-CPython / wasmtime / wasmi) | new DreggNet macro harness over `bridge`+`polyana` | GAP (micro only) |
| Workload throughput | workloads/sec at saturation, per tier | new macro harness | GAP |
| Lease lifecycle throughput | leases/sec through open→fund→run→meter→reap | new `bridge` bench over `LeaseWatcher` | GAP |
| Durable checkpoint/resume | append + replay-fold cost; crash→recover→resume | `polyana bench_durable_eventstore` (exists); DreggNet `durable` (gap) | PARTIAL |
| Gateway req/sec | httpe→ingress→workload end-to-end | `net/transport profile.rs` + `polyana bench_server_request` (component); end-to-end (gap) | PARTIAL |
| Webapp req/sec | agent-declared route served under a lease | new `webapp` bench | GAP |
| Per-turn kernel cost | executor turns/sec + commitment cost | `dregg-perf` executor_turn / embedded_commit / commitment | HAVE |
| Proof / aggregate cost | prove + verify + recursion fold | `dregg-perf` prove/verify/cohort/recursion_fold | HAVE |
| Bridge verify | trustless cross-chain lock-proof | `bridge/benches/solana_verify_bench` | HAVE |
| Policy / cap-gate cost | per-boundary admit/deny | `polyana bench_policy_decide` / `bench_cap_floor` | HAVE |

### 4.2 Scaling axis — WIDE (horizontal)

What to vary: **N cells, N agents, N concurrent leases/workloads, N nodes.**
Measure aggregate throughput vs N and find where it goes sublinear (the
contention point).

Concretely:
- **N cells** — executor turns/sec as the ledger grows (1 → 10³ → 10⁶ cells).
  Probe: does the sparse-Merkle + cap-root cache keep commitment O(touched), or
  does `Ledger::root()` cost grow with total cells? (`commitment.rs` is the unit;
  needs a populated-ledger sweep.)
- **N agents / N concurrent leases** — leases/sec and p50/p99 workload latency
  as concurrent funded leases climb. Where does `LeaseWatcher::fulfill` serialize?
- **N workloads per tier** — workloads/sec under wasmtime (InstancePool depth
  vs cold Cranelift) and wasmi (unmetered → wall-clock timeout risk) and
  native-CPython (subprocess spawn).
- **N nodes** — aggregate cluster throughput (deferred to the fleet; modeled
  analytically now, see §4.4).

**Candidate contention points to instrument explicitly:**
- the shared ledger / `Ledger::root()` recompute,
- the nullifier set (spend-uniqueness check growth),
- the scheduler (`control` provisioning / `LeaseWatcher` feed),
- the gateway (httpe accept loop + per-workload ingress),
- the durable store (append serialization + replay-fold),
- the policy/cap-floor linear scan (`bench_cap_floor` shows tail-of-set cost).

### 4.3 Scaling axis — VERTICAL (per-node)

What to find: at saturation on ONE box, **turns/sec and workloads/sec**, and
**which subsystem is the bottleneck.** Candidates, with the prime suspect first:
- **prover** — at ~147 ms/turn full-prove vs ~8 µs execute, proving is ~21,000×
  the executor. A node that proves every turn is prover-bound; the symbolic /
  collapse split (~7,000× fast path) and recursion fold (amortize many turns
  into one verify) are the levers. Measure: turns/sec proven vs admitted.
- **executor / commitment** — turns/sec admitted (no proof): the v9 rotated
  per-cell commit (~157 ms full, but ~µs incremental) is the long pole; confirm
  incremental commitment stays O(touched).
- **sandbox spawn** — wasmtime cold (Cranelift) vs pre-pooled vs wasmi vs
  native-CPython subprocess: cold-start dominates short workloads.
- **durable I/O** — append + fsync + replay-fold throughput.
- **gateway** — accept/parse/route req/sec (zero-alloc header parse already
  asserted in `http_alloc`).

Method: saturate each in isolation, then run the realistic mix (§4.5) and
flamegraph to confirm the predicted bottleneck.

### 4.4 Scaling axis — FEDERATION

DreggNet is a federation of nodes; cross-federation work and gossip have a cost
the single box cannot measure. Define now, measure on the fleet:
- **cross-federation bridge cost** — settling/verifying a turn that references
  another federation's state (the `bridge` verify + the Solana-style O(votes) vs
  succinct O(1) tradeoff applies here too).
- **blocklace gossip / sync cost** — per-node bandwidth + CPU to gossip the DAG
  and converge; how it grows with federation size and churn.
- **federation throughput vs node count** — does adding nodes add aggregate
  throughput (independent cells, parallel proving) or contend (shared settlement)?

**Honest scope:** there is ONE staging box today. Federation scaling is
**modeled analytically now** (per-node numbers × node count, minus the gossip
overhead measured at small N) with the real multi-node bench **DEFERRED to the
fleet.** Name it as analytic until the fleet exists; do not report modeled
numbers as measured.

### 4.5 Methodology

- **Micro:** criterion, as today. Every new primitive lands with a SMOKE/FULL
  criterion bench in the owning crate (the `dregg-perf` pattern).
- **Macro/throughput:** a NEW load-gen harness (the named gap) that drives N
  concurrent leases/workloads/agents against a running DreggNet and reports
  throughput + p50/p99/p99.9 tail (reuse the `polyana bench-substrate` four-9s
  envelope + `net/transport profile.rs` JSON/remote shape).
- **Realistic workload mix — the agent-business loop:** pay → lease →
  run → meter → reap, repeated by N agents. This is the load-gen's default
  scenario; it exercises bridge + lease + sandbox + durable + commit + (optional)
  prove in one realistic ratio. `perf/src/bin/orchestration_demo.rs` is the
  in-process seed of this loop (parent embodies, spawns attenuated worker,
  in-scope commits / over-scope rejected, outputs settle conservingly) — promote
  it to a load-gen scenario.
- **Bottleneck isolation:** flamegraph / `perf` on the saturated node; commit the
  flamegraph as an artifact per axis so the bottleneck is profiled, not inferred.
- **Regression tracking:** extend the committed-baseline pattern
  (`perf/baselines/`, `proof-sizes --json`) to the macro numbers; capture FULL
  baselines on persvati, not just SMOKE on a laptop.

### 4.6 Targets — what "good" looks like

Starting targets to size the harness and flag regressions (refine once the FULL
baseline lands; these are the bar, not measurements):

**Cheap box (one staging node, e.g. M2-class / small cloud VM):**
- executor admit: ≥ 10⁴ turns/sec (≈ the ~8 µs/turn floor, single-threaded).
- workload exec latency: wasmi/wasmtime warm < 10 ms; native-CPython < 100 ms
  (subprocess spawn dominated); cold wasmtime amortized by the InstancePool.
- lease lifecycle: ≥ 10³ leases/sec through open→fund→meter→reap (sans the
  workload run itself).
- gateway: ≥ 10⁴ req/sec on the zero-alloc fast path; webapp route p99 < 50 ms.
- proving: a node should SUSTAIN proving without it being the latency a user
  feels — i.e. admit interactively (~µs, symbolic), prove asynchronously
  (~147 ms) and aggregate (recursion fold) off the hot path. Target: prove
  throughput ≥ the turn-admit rate the box is expected to settle, NOT per-turn.
- durable checkpoint/resume: append ≥ 10⁴ events/sec; resume bounded by
  replay-fold (keep it linear, snapshot to cap it).

**Fleet (federation of N nodes):**
- aggregate throughput grows ~linearly in N for independent cells/leases
  (the WIDE goal); settlement/gossip overhead sublinear in N.
- cross-federation bridge verify amortized by the succinct O(1) wrapper, not the
  O(votes) linear path, before it is on any hot path.
- federation converges (gossip sync) within a bounded staleness window as N and
  churn grow — the number the fleet bench must produce.

---

## 5. Execution order for the bench lanes

1. **DreggNet service-layer micro-benches** (close gap §3.1): `bridge` lease
   lifecycle, `durable` checkpoint/resume, `gateway`/`webapp` req/sec, per-tier
   workload latency. Highest value — zero coverage today.
2. **The macro/throughput load-gen harness** (gap §3.2) with the agent-business
   loop as the default scenario; promote `orchestration_demo`.
3. **WIDE sweeps** (gap §3.3/§4.2): populated-ledger commitment, N-concurrent
   leases, per-tier workload saturation; instrument the named contention points.
4. **VERTICAL saturation + flamegraphs** (gap §3.6/§4.3): confirm the prover /
   commitment / spawn / durable / gateway bottleneck per workload class.
5. **FULL baselines** (gap §3.5) on persvati for the existing `dregg-perf` suite.
6. **Federation** (gap §3.3/§4.4): analytic model now; fleet bench when the
   fleet exists.
