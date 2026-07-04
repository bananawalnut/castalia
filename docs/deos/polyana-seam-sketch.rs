//! polyana ⋈ dregg — Slice 1 seam sketch (ILLUSTRATIVE — now WIRED in `polyana-bridge`)
//!
//! Companion to `docs/deos/POLYANA-ALLIANCE.md`. This file is a documentation
//! artifact: it is NOT a crate member, is NOT compiled by CI, and exists to show
//! the *shape* of the smallest adoptable seam with names that match the real
//! cited types on both sides. It does not import anything (the type names are
//! sketched locally) so it never breaks a build and never churns a crate.
//!
//! ⚑ REALIZED: the shape below is now a compiling, tested crate at
//! `polyana-bridge/` (workspace member). It rides the REAL dregg primitives —
//! `dregg_cell::facet::is_facet_attenuation` over an `EffectMask` and
//! `dregg_cell::is_attenuation` over `AuthRequired` (Slice 3), a real chained
//! `dregg_turn::TurnReceipt` (Slice 1), and a `dregg_query::AttestedAnswer`
//! non-omission certificate over the audit log (Slice 1's payoff) — and the
//! whole seam is exercised in `polyana-bridge/tests/seam.rs` (7 tests). This
//! sketch stays as the illustrative narrative; `polyana-bridge` is the truth.
//!
//! The seam: at polyana's `pa_witness` / `audit-mcp` boundary, in addition to
//! writing a `TraceRecord` (polyana `src/core/src/provider.rs:324-336`), also
//! emit a dregg `TurnReceipt` (dregg `sdk/src/receipt.rs:89-99`) chained via
//! `previous_receipt_hash`. Then `dregg-query`'s attested-answer path
//! (`dregg-query/src/lib.rs:48`) yields a PROVABLE non-omission certificate over
//! the polyana audit log — the thing "evidence-native" most wants and lacks.
//!
//! Purely additive: the TraceRecord stays for human debugging; the Receipt is
//! the unforgeable, non-omitting spine.

// ─────────────────────────────────────────────────────────────────────────────
// polyana side (mirrors `polyana_core::provider`) — what polyana already has.
// ─────────────────────────────────────────────────────────────────────────────

/// polyana's per-call evidence record. Real: `src/core/src/provider.rs:324-336`.
/// Byte-equal across providers via `CanonicalValue` (sorted BTreeMap keys, NaN
/// bit-preserving floats) so the same call replays identically on wasmtime/wasmi.
pub struct TraceRecord {
    pub seq: u64,
    pub timestamp_ns: u128,
    pub fn_name: String,
    pub args_canonical: Vec<u8>, // bincode-legacy of Vec<CanonicalValue>
    pub ret_canonical: Vec<u8>,  // bincode-legacy of Result<Vec<CanonicalValue>, String>
}

/// polyana's per-tenant cap manifest, parsed from `cap-bundle/default.toml`
/// (filesystem-read / network-localhost / deterministic-window / streaming).
/// Real bundle: `cap-bundle/default.toml`; constructors `src/core/src/capability.rs`.
pub struct CapBundle {
    pub effects: Vec<String>, // e.g. ["filesystem:read", "network:localhost"]
}

// ─────────────────────────────────────────────────────────────────────────────
// dregg side (mirrors the cited dregg crates) — what dregg supplies.
// ─────────────────────────────────────────────────────────────────────────────

/// dregg's canonical proof-of-execution. Real: `sdk/src/receipt.rs:89-99`.
/// In dregg this carries turn/forest/effect hashes, pre/post state roots, the
/// agent cell, the federation binding, and the `previous_receipt_hash` chain
/// link; a `TurnProof` (the composed full-turn STARK) attaches lazily.
pub struct TurnReceipt {
    pub turn_hash: [u8; 32],
    pub effects_hash: [u8; 32],
    pub pre_state_root: [u8; 32],
    pub post_state_root: [u8; 32],
    pub previous_receipt_hash: [u8; 32], // the chain link → non-omission spine
}

/// dregg's rights lattice element. Real: `cell/src/capability.rs` (`AuthRequired`).
/// The ONLY legal cap motion is monotone restriction.
pub struct AuthRequired {
    pub allowed_effects: Vec<String>,
}

impl AuthRequired {
    /// Real: `cell/src/capability.rs` — `granted.is_narrower_or_equal(held)`.
    /// Sketch: granted ⊆ held on the effect set.
    pub fn is_narrower_or_equal(&self, held: &AuthRequired) -> bool {
        self.allowed_effects.iter().all(|e| held.allowed_effects.contains(e))
    }
}

/// dregg's Lean-proven monotone gate. Real: `cell/src/capability.rs:614-615`,
/// byte-tested against the Lean crown (`sdk/src/tool_gateway.rs:846-867`).
pub fn is_attenuation(held: &AuthRequired, granted: &AuthRequired) -> bool {
    granted.is_narrower_or_equal(held)
}

// ─────────────────────────────────────────────────────────────────────────────
// THE SEAM — one boundary. polyana's pa_witness, dregg-receipted.
// ─────────────────────────────────────────────────────────────────────────────

/// Slice 3 fragment: gate a guest's requested caps against the cap-bundle via
/// the dregg *proven* gate instead of a hand-rolled allowlist check. A guest
/// asking for more than the bundle grants is refused by the monotone law.
pub fn gate_request(bundle: &CapBundle, requested: &CapBundle) -> Result<(), &'static str> {
    let held = AuthRequired { allowed_effects: bundle.effects.clone() };
    let granted = AuthRequired { allowed_effects: requested.effects.clone() };
    if is_attenuation(&held, &granted) {
        Ok(())
    } else {
        // dregg's anti-ghost tooth: a refusal is a value, never a panic, and it
        // advances no counter / spends nothing. Mirrors `GatewayRefusal`.
        Err("requested caps exceed cap-bundle grant (not an attenuation)")
    }
}

/// Slice 1: at the pa_witness boundary, emit a dregg TurnReceipt alongside the
/// polyana TraceRecord, chained to the previous receipt. The blake3 hashing here
/// is a sketch; the real dregg path commits with sorted-Poseidon2 and attaches a
/// TurnProof. `dregg-query` then proves the answer is computed from EXACTLY the
/// committed receipt range (`server_cannot_omit_position`, Lean-checked).
pub fn pa_witness_dregg_receipt(
    trace: &TraceRecord,
    pre_state_root: [u8; 32],
    post_state_root: [u8; 32],
    previous_receipt_hash: [u8; 32],
) -> TurnReceipt {
    // Sketch hash: in dregg this is the canonical turn commitment.
    let turn_hash = sketch_hash(&[trace.fn_name.as_bytes(), &trace.args_canonical]);
    let effects_hash = sketch_hash(&[&trace.ret_canonical]);
    TurnReceipt {
        turn_hash,
        effects_hash,
        pre_state_root,
        post_state_root,
        previous_receipt_hash, // chain link: the non-omission spine over the log
    }
}

/// Placeholder for the canonical commitment. Real dregg = sorted-Poseidon2.
fn sketch_hash(_parts: &[&[u8]]) -> [u8; 32] {
    [0u8; 32]
}

// ─────────────────────────────────────────────────────────────────────────────
// What this buys, in one comment:
//
//   polyana keeps its breadth (34 langs, 13 providers, APE distribution).
//   dregg supplies, at ONE boundary each:
//     • Slice 1 — a chained, unforgeable Receipt + a PROVABLE non-omission
//       certificate over the audit log (pa_witness → dregg-query attested).
//     • Slice 2 — a verified OsSandbox tier: Target::HostPd, cap-confined
//       Endpoint as sole authority surface, ValidityTable-unforgeable cap.
//     • Slice 3 — every boundary gated by the LEAN-PROVEN is_attenuation,
//       not a hand-rolled allowlist.
//
//   "trust nothing without evidence" compiles to
//   "a turn is the exercise of an attenuable proof-carrying token over owned
//    state, leaving a verifiable receipt."
// ─────────────────────────────────────────────────────────────────────────────
