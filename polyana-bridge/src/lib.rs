//! # polyana-bridge — the dregg ⋈ polyana seam, WIRED
//!
//! Companion to `docs/deos/POLYANA-ALLIANCE.md` and the (illustrative,
//! deliberately un-wired) `docs/deos/polyana-seam-sketch.rs`. This crate is the
//! seam sketch made REAL: it compiles against the actual dregg crates
//! (`dregg-cell`, `dregg-turn`, `dregg-query`) and exposes the small Rust API
//! surface polyana would consume to gain dregg's verified core. polyana keeps
//! its breadth (34 lang families, 13 providers, APE distribution); dregg
//! supplies, at one boundary each, three things polyana's "trust nothing
//! without evidence" thesis is reaching for.
//!
//! The alliance is **dregg-as-verified-core**: dregg exports a Rust crate whose
//! API is the Lean-proven surface (`is_attenuation`, `is_facet_attenuation`, the
//! chained `TurnReceipt`, the MMR non-omission verifier); polyana consumes the
//! Rust, trusting the Lean the way it trusts ring/ed25519-dalek. This crate IS
//! that exported surface, with the polyana-side input shapes mirrored so the
//! seam is a single, additive boundary.
//!
//! ## The three slices (POLYANA-ALLIANCE.md §4), realized
//!
//! - **[`witness`] — Slice 1 (highest value, smallest blast radius).** polyana
//!   already writes a `TraceRecord` per call. At the `pa_witness` boundary,
//!   ALSO emit a real `dregg_turn::TurnReceipt` keyed on the same
//!   `(seq, fn_name, args, ret)`, chained via `previous_receipt_hash`. The
//!   `TraceRecord` stays for human debugging; the receipt is the unforgeable,
//!   non-omitting spine.
//! - **[`caps`] — Slice 3.** polyana's `cap-bundle/default.toml` is already a
//!   capability manifest. Gate every boundary crossing through the **Lean-backed
//!   monotone attenuation law** — `dregg_cell::facet::is_facet_attenuation` over
//!   an `EffectMask` (the effect-set face) and/or `dregg_cell::is_attenuation`
//!   over `AuthRequired` (the auth-kind face) — instead of a hand-rolled
//!   allowlist. A guest asking for more than the bundle grants is refused by the
//!   proven gate, fail-closed.
//! - **[`attest`] — Slice 1's payoff.** Build a `dregg_query::AttestedSlice`
//!   over the receipt log and produce a `dregg_query::AttestedAnswer` carrying a
//!   **non-omission certificate**: a verifying answer is provably computed from
//!   EXACTLY the committed receipt range — nothing hidden, nothing forged,
//!   nothing reordered (the Rust embodiment of `server_cannot_omit_position`).
//!   This is the single thing polyana's evidence story most wants and lacks.
//!
//! ## What this crate is NOT
//!
//! It does not execute polyglot guests, run Lean in polyana's loop, or require
//! byte-equality with polyana's `polyana_bincode` trace format. The fit is
//! *gate / attest / replay / confine*, never *re-implement polyana's runtime*
//! (POLYANA-ALLIANCE.md §3). The Slice 2 confinement seam (`Target::HostPd`)
//! lives in `sel4/dregg-firmament` and is not re-exported here.

pub mod attest;
pub mod caps;
pub mod trace;
pub mod witness;

pub use attest::{AttestBuildError, attest_whole_log, audit_records};
pub use caps::{
    CapBundle, EffectInternError, GateRefusal, gate_auth, gate_effect_set, intern_effects,
};
pub use trace::TraceRecord;
pub use witness::witness_receipt;

// Re-export the dregg-query verification surface so a polyana operator verifies
// an attested answer without depending on dregg-query directly.
pub use dregg_query::{
    AttestError, AttestedAnswer, AttestedSlice, Blake3Mmr, Coverage, Pred, Query, Term,
    answer_whole_log,
};
