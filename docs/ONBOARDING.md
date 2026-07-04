# ONBOARDING — orient fast (dregg + DreggNet)

You are a new Claude session, a new human, or a new agent. Read **this one doc**
and you will know the shape: what dregg is, what DreggNet is, the two repos and
how they split, where the live infrastructure is, and the exact "read these
first" path into the depth. Everything here is grounded to the real current
state as of 2026-06-28 — it does not aspirational-ize. Where something is staged,
partial, or single-token-gated, it says so.

The one-sentence through-line: **dregg is the verified, open substrate that says
what was promised, paid, and owed — verifiably; DreggNet is the private layer
that actually runs the workload, serves it, hosts it, and bills for it.**

---

## 1. What dregg is

dregg ("Dragon's Egg") is a **formally verified, distributed object-capability
operating system**. The kernel is a Lean 4 program with machine-checked
soundness, and it is the *exact* function the running node executes. Every state
transition is gated by an unforgeable capability, leaves a verifiable receipt,
and carries a STARK proof a light client can check without re-running history.

The whole system in one sentence: *a light client holding one root knows every
transition in the whole history was authorized, conservative, fresh, and
correctly committed — re-executing nothing.*

The four words of the model ([`docs/OVERVIEW.md`](OVERVIEW.md)):

- **Cell** — the unit of state and identity. Holds four substances: *value*
  (per-asset balances), *state* (slots + nonce), *authority* (a capability
  tree), *evidence* (append-only ledgers). A card, a document, an agent, a room,
  a service, you — each is a cell.
- **Capability** — a directed edge carrying attenuated rights. You *hold* a
  capability iff you can *produce* its witness — never merely be named in a
  table. Authority is production under non-forgeability, not possession of a key.
- **Turn** — one authorized inference step: a forest of actions executed as a
  journalled transaction, each action gated by the kernel before it commits.
- **Receipt** — a turn's verifiable witness; it binds the whole post-state, so
  tampering a field the effect did not legitimately write makes the turn
  unprovable (the anti-ghost property).

On top of the kernel, **deos** is the agentic desktop userlayer — the same
proofs made visual: a window *is* a capability, an interaction *is* a verified
turn. (Naming: **robigalia** the project · **dregg** the kernel · **deos** the
desktop. *deos runs on dregg runs in robigalia.*)

### Read these first (dregg depth, in order)

1. [`docs/OVERVIEW.md`](OVERVIEW.md) — the four-word model + the map. Start here.
2. [`docs/atlas/atlas.html`](atlas/atlas.html) — the visual atlas of the whole
   system (open it in a browser). The fastest way to *see* the shape.
3. [`metatheory/CONSTRUCTIVE-KNOWLEDGE.md`](../metatheory/CONSTRUCTIVE-KNOWLEDGE.md)
   — the conceptual spine: why a capability *is* constructive knowledge.
4. [`metatheory/docs/DREGG-CALCULUS.md`](../metatheory/docs/DREGG-CALCULUS.md) —
   the calculus the kernel is built on.
5. [`docs/KERNEL.md`](KERNEL.md) — the verified kernel: the verbs, the executor,
   the guarantees. The skeptic-facing proof ledger is
   [`metatheory/CLAIMS.md`](../metatheory/CLAIMS.md) (what is proved, what is not).
6. [`docs/reference/`](reference/) — the **grounded what-is** for every
   subsystem, each pinned to `file:line` at HEAD. When a memory/summary and a
   reference doc disagree, **the reference wins**.

### If you want to *use* or *build*, not just understand

- [`QUICKSTART.md`](../QUICKSTART.md) — 15 minutes: a node running + a real turn
  signed on `localhost`.
- [`docs/guide/`](guide/) — the developer onramp (build-with-dregg, what you can
  build, deos-from-the-web, the agent quickstart for LLMs).
- [`docs/MANUAL/`](MANUAL/) — the author's user + developer manuals.
- The SDKs: `sdk/` (Rust core), `sdk-py/` (Python), `sdk-ts/` (TypeScript).

---

## 2. What DreggNet is

DreggNet is the **cloud / operator layer** — the thing that takes dregg's open,
trustless rail and runs real workloads on real metal, serves them, and bills for
them. dregg says *what was promised, paid, and owed*, verifiably; DreggNet
*delivers it*. The substrate is the open rail; the operated infra is the moat.

What composes there (repo `~/dev/DreggNet`):

- **polyana** — the real polyglot execution engine (git submodule
  `akapug/polyana`, co-developed with pug): 34 language families, sandbox tiers
  from `wasmtime`/`v8` to `firecracker`/`native+seccomp+landlock`, durable
  replay, capability gates at every boundary. This is what makes "durable
  container execution for agents" real, not a model.
- **net/** — the serving + transport layer (the Elide `httpe` HTTP gateway +
  WireGuard/Tailscale mesh). ember's own work; proprietary, Linux-targeted.
- **the bridge + control plane** — turn a funded dregg `execution-lease` into a
  running polyana workload, metered per-period, settled over dregg's Payable
  rail; schedule + provision + dispatch over the private overlay.
- **the surfaces** — the Discord bot (community front door), the
  Fly-compatible machines API gateway, portal.dregg.studio (see + verify the
  network), the CLI/SDKs.

### The live infrastructure (as of 2026-06-28)

The fabric today is a **2-node federation** (target: 5), a self-hosted headscale
mesh, and a home compute backend:

| node | host | role | overlay | state |
|---|---|---|---|---|
| **edge** (node-0) | AWS `34.224.208.52` (t3.medium, EIP) | the public door: Caddy (TLS + basic-auth), gateway, control, postgres, headscale + DERP relay, a `dregg-node` | `100.64.0.1` | live |
| **persvati** (node-1) | ember's 24-core home box | the engine room: the polyana compute backend (`:8021/fulfill`) + STARK proving + a `dregg-node` | `100.64.0.2` | live |
| homelab ×3 | pug (target) | additional consensus nodes + compute backends | — | the path to 5-quorum |

- **Federation:** two independent boxes in full BFT mode, finalizing by signed
  quorum over the overlay. Cross-node deterministic finality is *verified* (a
  turn submitted on the edge finalized + appeared on persvati with the identical
  state commitment). Honest caveat: at n=2 the threshold is 2, so `f=0` (no
  fault tolerance — both must be online). Fault tolerance begins at n=5
  (threshold 4, `f=1`). See `DreggNet/deploy/FEDERATION.md`.
- **Mesh:** self-hosted headscale at `headscale.dreggnet.fg-goose.online` (live,
  real Let's Encrypt cert; `/health` → `{"status":"pass"}`). Nodes join the
  private WireGuard overlay (`100.64.0.0/10`) and never expose a public port.
- **Discord bot:** built, shipped to the edge, wired end-to-end, and
  **token-gated** — it flips live the moment a real `DISCORD_TOKEN` is dropped
  (the one remaining step). Source: `breadstuffs/discord-bot/`.
- **portal.dregg.studio:** the public read-only web portal — see the live cell
  network and trustlessly verify a cell *in your own browser* (a wasm light
  client in the tab). Read-only v1; **coming up** (DNS + cert propagating);
  drive-actions (lease/pay/turn) are the next rung.
- **Solana bridge:** **devnet / oracle-attested** today ($DREGG proper lives on
  Solana *mainnet*; devnet proves the plumbing). Mainnet-trustless is ahead. See
  `breadstuffs/docs/deos/{SOLANA-DEVNET,TRUSTLESS-SOLANA-BRIDGE}.md`.

Domains: `dregg.net` is kept pristine for production; `fg-goose.online` is the
infra catch-all (`dreggnet.fg-goose.online` = the gated operator surface,
`headscale.dreggnet.fg-goose.online` = the mesh control); `dregg.studio` carries
the public portal.

### Read these next (DreggNet depth)

The operational + user-facing docs live in the **DreggNet repo**, not here:

- **`DreggNet/docs/OPERATING.md`** — how to run/operate the live network
  (topology, join the fabric, deploy/restart, cost, runbook index). For
  operators.
- **`DreggNet/docs/USING-DREGGNET.md`** — what you can *do* on the network: the
  Discord bot, the portal, the SDKs, the browser extension. For the community.
- `DreggNet/README.md` + `DreggNet/ARCHITECTURE.md` — the open-core split and the
  build ladder.
- `DreggNet/deploy/` — the operator reference set: `FABRIC-JOIN.md`,
  `FEDERATION.md`, `PERSVATI-BACKEND.md`, `COMPUTE-OFFERING.md`,
  `ARCHITECTURE-COMPUTE-BACKEND.md`, `staging/` (the deploy mechanics).

---

## 3. The two repos + the open-core split

| | what it is | license | where |
|---|---|---|---|
| **dregg** (breadstuffs) | the verified ocap substrate — kernel, value layer, intent ring, Payable, the execution-lease (the on-substrate, light-client-witnessed record of a workload + metering + payment) | **AGPL-3.0, public** | `~/dev/breadstuffs` → `github.com/emberian/dregg` |
| **DreggNet** | the thing that actually runs the workload, serves it, hosts it, and bills for it | **proprietary, private** | `~/dev/DreggNet` → `github.com/emberian/DreggNet` |

The line: **dregg's half is verifiable; DreggNet's half is the operated, paid
reality.** Neither claim outruns the other. The open substrate is the trustless
rail anyone can run; the execution + infrastructure is the moat. polyana is a
third party (Apache-2.0, co-dev with pug) consumed by DreggNet, not absorbed.

On disk the two are **siblings** (`~/dev/breadstuffs` + `~/dev/DreggNet`); some
DreggNet path-deps (`dregg-verify`, `demo/stripe-receiver`) resolve against that
layout, so keep them side by side.

---

## 4. The orient path in one glance

```
            ┌─ you are here: ONBOARDING.md ──────────────────────────┐
            │                                                         │
   want the SUBSTRATE?                              want the NETWORK?
            │                                                         │
   OVERVIEW.md → atlas.html →                    DreggNet/README.md →
   CONSTRUCTIVE-KNOWLEDGE.md →                   DreggNet/ARCHITECTURE.md →
   KERNEL.md → reference/                        OPERATING.md (run it) /
            │                                    USING-DREGGNET.md (use it)
   want to BUILD?                                         │
   QUICKSTART.md → guide/ → MANUAL/              deploy/ runbooks · FEDERATION.md
```

For a fresh Claude session specifically: orient from the durable record
(`REORIENT.md` → `HORIZONLOG.md` → the memory index), read for the *shape*, then
**verify code vs HEAD for the state** — memories are dated and point-in-time, and
`docs/reference/` (grounded to `file:line`) is the source of truth for what each
subsystem *is*.

---

*Honest-state footnote: 2-node federation (→5); the Discord bot is wired +
token-gated; the portal is read-only v1 coming up; the Solana bridge is
devnet/oracle with mainnet-trustless ahead; the house-capacity circuit welds are
partly staged (the executor tooth is real today, the light-client circuit tooth
is its named shadow — see the memory index). This doc is dated 2026-06-28; verify
against HEAD.*
