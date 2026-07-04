# deos-hermes — Hermes as a confined deos agent

Hermes (Nous Research's self-improving agent, `~/pug/hermes-agent`) is a real,
capable tool-calling agent. **deos-hermes** integrates it as a *confined* deos
agent: every Hermes tool-call becomes a cap-gated, metered, **receipted** dregg
turn on the verified executor — or an in-band refusal Hermes sees. This is the
ADOS thesis (*a turn is the exercise of an attenuable proof-carrying token over
owned state, leaving a verifiable receipt*) realized with a real agent, so
Hermes can assist deos development without being trusted.

The enforcement is entirely the **proven** `ToolGateway` (`sdk/src/tool_gateway.rs`,
mirroring `metatheory/Dregg2/Apps/ToolAccessDelegation.lean`). This crate builds
no new policy and no new crypto — only the ACP↔gate seam.

## 1. How Hermes exposes itself over ACP

Hermes ships an ACP (Agent Client Protocol) adapter at
`~/pug/hermes-agent/acp_adapter`. ACP is a JSON-RPC stdio protocol (the one Zed
speaks to external agents). The shape relevant to confinement:

- **Entry** (`acp_adapter/entry.py`): `python -m acp_adapter.entry` / `hermes acp`
  runs `acp.run_agent(HermesACPAgent(), use_unstable_protocol=True)` over stdio.
  stdout is reserved for JSON-RPC frames; logging goes to stderr.
- **Sessions** (`acp_adapter/session.py`): each ACP session maps to a Hermes
  `AIAgent` instance, persisted to `~/.hermes/state.db`. `session/new`,
  `load_session`, `resume_session`, `fork_session`, `list_sessions`.
- **Prompt → stream** (`acp_adapter/server.py::prompt`): the editor sends
  `session/prompt`; Hermes runs its agent loop in a worker thread and streams
  back `session/update` notifications — agent message text, thoughts, plan
  updates, and **tool-call lifecycle** events.
- **Tool-call lifecycle** (`acp_adapter/events.py`, `tools.py`): when Hermes
  starts a tool, `make_tool_progress_cb` emits a `ToolCallStart`
  (`build_tool_start` → `tool_call_id`, `name`, `kind`, `raw_input`/args,
  `locations`); `make_step_cb` emits `ToolCallComplete` with the result. Each
  Hermes tool classifies into an ACP **kind** via `tools.py::TOOL_KIND_MAP` /
  `get_tool_kind`: `read | edit | execute | fetch | search | other`.
- **Permission gate** (`acp_adapter/permissions.py`): the load-bearing
  interception point. Before a *dangerous* tool-call runs, Hermes calls
  `conn.request_permission(session_id, tool_call=<ToolCallUpdate>, options=[…])`
  — the ACP **client** decides allow/deny and returns an `AllowedOutcome`
  (`option_id`) or a rejection. `_OPTION_ID_TO_HERMES` maps the option id back
  to Hermes's `once | session | always | deny`. **This is exactly the seam where
  deos substitutes the `ToolGateway` for a human allow/deny prompt.**

So ACP already gives us (a) a structured description of every tool-call
(`tool_call_id`, `name`, `kind`, `raw_input`) and (b) a request/response gate
(`request_permission` → `AllowedOutcome`) deos can answer with the gateway's
verdict.

## 2. The bridge design

```
  Hermes process (hermes acp, stdio JSON-RPC)
        │  session/update tool_call  +  session/request_permission
        ▼
  deos-side ACP CLIENT  ──parse──▶  acp::ToolCallRequest { session, id, name, kind, args }
        │
        ▼
  bridge::HermesGateway
        │  kind ─▶ grant_registry::GrantRegistry  (deos's per-kind ToolGrant: scope+rate+deadline)
        │  lazily ToolGateway::admit(runtime, root_token, grant)   ← cap-gated worker, verified executor
        ▼
  ToolGateway::invoke(tool_id, now, work)         ← the PROVEN delegAdmit gate
        │
        ├─ ADMITTED → metered turn COMMITS  → ToolReceipt(turn_hash) → PermissionOutcome::Allow
        └─ REFUSED  → no turn, no spend     → GatewayRefusal          → PermissionOutcome::Reject(reason)
        │
        ▼
  deos-side ACP CLIENT  ──reply──▶  AllowedOutcome{allow_once} | deny   (back to Hermes)
```

- **Per-tool grant.** deos is the GRANTOR: it pins a `ToolGrant` (SCOPE ∧
  DEADLINE ∧ RATE) per Hermes `ToolKind`. `grant_registry::GrantRegistry`
  expresses the standard confinement — tight rate ceilings on the dangerous
  classes (`Execute` 20, `Edit` 30), generous on read-only (`Read` 200,
  `Search` 100), `Fetch` 50, and the tightest default `Other` 10. An unknown
  tool falls into `Other`: **fail-closed by classification.** Each kind has a
  distinct `tool_id` (in-band scope key) and a distinct `tool_method` (the
  executor verb the worker's biscuit credential covers).
- **One worker per kind.** `HermesGateway` lazily `admit`s a cap-gated
  `ToolGateway` worker per kind on first use, each metering its own `calls_made`
  counter under its own mandate. deos can deny a whole class (rate 0) without
  touching another.
- **Both polarities are real.** An admitted call commits a genuine receipted
  turn (`turn_hash` is the receipt id deos returns to Hermes / the editor).
  A refused call is an in-band `Reject` naming the leg that bit (scope /
  deadline / rate), and submits NO turn — the anti-ghost tooth.

## 3. What is built (this crate)

`cd deos-hermes && cargo build && cargo test` (and `cargo run` for the live-shaped loop).

- `src/acp.rs` — the ACP wire subset: `ToolKind` (byte-faithful to Hermes's
  `TOOL_KIND_MAP`), `ToolCallRequest`, `PermissionOutcome`.
- `src/grant_registry.rs` — `GrantRegistry`: per-KIND floors **and** tighter
  per-TOOL grants (`MandateKey`, tightest-wins), each its own cap-gated,
  independently-metered worker.
- `src/bridge.rs` — `HermesGateway::admit_call` (the seam) + `admit_with_work`
  (explicit override). Routes a call to its `MandateKey`'s worker and rides the
  tool's side-effect on the metered turn.
- `src/tool_effects.rs` — translates a tool-call's payload into a `Vec<Effect>`
  witness (the path written, the URL fetched, the command run) that rides the
  SAME metered turn, so the receipt witnesses WHAT the call did, not just the meter.
- `src/acp_client.rs` — the REAL ndjson JSON-RPC ACP **client**: drives
  `initialize` → `session/new` → `session/prompt`, consumes `session/update`,
  and answers each `session/request_permission` via the gateway. Transport-
  agnostic (`AcpPeer`): a live `hermes-acp` subprocess (`AcpTransport`) OR the
  mock peer.
- `src/mock_peer.rs` — `MockHermesPeer`: replays the real `acp_adapter` message
  shapes (initialize/new_session responses, `session/update` tool_call events,
  `session/request_permission` requests with real `ToolCallUpdate` payloads,
  `PromptResponse` with `stopReason`), scripted with a tool-call list.
- `src/mandate.rs` — `Mandate`: the mandate inspector (grants, budgets spent,
  receipts, refusals) — ADOS made legible.
- `src/surface.rs` — `AgentDockModel`: the dock view-model + the ready-to-mount
  `CockpitSurface` recipe (no gpui dep).
- `tests/seam.rs` — both-polarity on the **real verified executor**.
- `tests/acp_loop.rs` — the FULL client ↔ mock-peer ↔ gateway loop end-to-end,
  over the wire shape: every permission gated, side-effects ride the turn,
  per-tool grants meter independently, the inspector + dock model render.
- `src/main.rs` — `cargo run` (mock loop) / `cargo run -- live` (subprocess).

**REAL:** the entire `ToolGateway` path (`admit` + `invoke` on the verified Lean
executor, genuine `TurnReceipt`s); the ACP ndjson JSON-RPC transport + the live
subprocess spawner; the riding effects; the per-tool grants; the inspector.
**MOCK (honest):** the Hermes *peer* in the tested end-to-end loop. The live
`hermes-acp` install in this environment is broken (its venv lacks the `acp`
Python module — `python -m acp_adapter.entry` raises `ModuleNotFoundError: No
module named 'acp'`), and a working one needs a model provider + credentials.
So the tested loop drives the SAME client against `MockHermesPeer`, which
replays the real `acp_adapter` shapes; the live path runs the identical driver.

## 4. Roadmap — remaining to a fully confined deos-hermes agent

1. **Fix / wire the live `hermes-acp` install.** The client + subprocess
   spawner are real; once `hermes-acp`'s venv carries the `acp` module and a
   provider is configured, `cargo run -- live` drives a real Hermes session
   through the gate, no code change.
2. **The sandbox-PD confinement (firmament/seL4).** Replace the bare `Command`
   in `AcpTransport::spawn_hermes` with a `spawn_hermes_in_pd(host_pd, cwd_cap,
   net_cap)` that launches Hermes into a confined protection-domain (the host-PD
   the firmament work boots — see `project-firmament-sel4-boots`): its
   filesystem a cap-scoped VFS (`FirmamentFs`), its network an explicit net-cap.
   The gate is the *intent* authority; the PD is the *ambient* authority —
   defense in depth. The `AcpClient` driver is unchanged (see `src/surface.rs`).
3. **Mount the dock surface in starbridge-v2.** `src/surface.rs` carries the
   `AgentDockModel` view-model + the mounting recipe; the lift is a gpui-gated
   `HermesDockSurface : CockpitSurface` that renders the model and runs the
   client on a background thread. Tie `deadline` to the live dregg block height.
4. **Per-command allowlists.** Extend per-tool grants from a rate ceiling to a
   cell-program command allowlist (e.g. `terminal` restricted to a verb set),
   binding the allowed argument shape into the worker cell's program.
