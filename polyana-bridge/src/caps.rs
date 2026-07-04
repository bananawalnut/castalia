//! Slice 3 — gate a polyana cap-bundle through dregg's Lean-backed attenuation.
//!
//! polyana's `cap-bundle/default.toml` (filesystem-read / network-localhost /
//! deterministic-window / streaming) is already a capability manifest mirroring
//! `polyana_core::capability`. Today polyana enforces it with a hand-rolled,
//! fail-closed allowlist check (`check_intent`, empty-allowlist = deny-all). The
//! seam: enforce it instead with the dregg primitive that is *proven* monotone
//! in Lean and byte-tested against the Lean crown — so "refuses silent
//! downgrades" stops being a code-review rule and becomes a theorem
//! (POLYANA-ALLIANCE.md §1.1, §4 Slice 3).
//!
//! Two faces of the one monotone law are exposed:
//!
//! - **the effect-set face** ([`gate_effect_set`]) — the cap-bundle is a SET of
//!   effect tokens; the bridge interns them into a `dregg_cell::facet::EffectMask`
//!   and gates with the real `dregg_cell::facet::is_facet_attenuation`
//!   (`granted & held == granted`, i.e. bitwise subset). This is the cap-bundle's
//!   natural shape.
//! - **the auth-kind face** ([`gate_auth`]) — when a boundary also narrows the
//!   *authorization kind* (signature / proof / either / …), the bridge gates with
//!   `dregg_cell::is_attenuation` over `AuthRequired`.
//!
//! ## Honest note on the bit assignment
//!
//! The EffectMask BIT POSITIONS used here are the bridge's own stable intern of
//! polyana's effect vocabulary — they are NOT dregg's protocol-effect bits
//! (`EFFECT_TRANSFER`, `EFFECT_GRANT_CAPABILITY`, …), and a bridge mask is never
//! handed to the kernel. What is load-bearing and borrowed-from-dregg is the
//! *algebra*: `is_facet_attenuation` is the proven monotone-restriction gate.
//! An unknown token fails closed (a requested effect the bridge cannot intern
//! cannot be proven an attenuation, so it is refused) — dregg's
//! safe-by-inexpressibility discipline.

use dregg_cell::facet::{EffectMask, is_facet_attenuation};
use dregg_cell::{AuthRequired, is_attenuation};
use thiserror::Error;

/// polyana's per-tenant capability manifest, as a set of effect tokens.
/// Parsed from `cap-bundle/default.toml` on the polyana side.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapBundle {
    /// e.g. `["filesystem:read", "network:localhost"]`.
    pub effects: Vec<String>,
}

impl CapBundle {
    pub fn new(effects: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            effects: effects.into_iter().map(Into::into).collect(),
        }
    }
}

/// An effect token polyana presented that the bridge has no stable intern for.
/// Fail-closed: an un-interned token in a *request* is refused, not waved
/// through (it cannot be proven ⊆ the held grant).
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("unknown polyana effect token (no stable EffectMask intern): {0:?}")]
pub struct EffectInternError(pub String);

/// A boundary crossing refused by the proven monotone gate.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GateRefusal {
    /// A requested effect token has no intern (fail-closed).
    #[error(transparent)]
    UnknownEffect(#[from] EffectInternError),
    /// The request is not an attenuation of the held grant — it asks for more
    /// authority than the cap-bundle confers. dregg's anti-ghost discipline: a
    /// refusal is a value, never a panic, and advances no counter / spends
    /// nothing (mirrors `GatewayRefusal`).
    #[error("requested caps exceed the held grant (not an attenuation)")]
    NotAnAttenuation,
}

/// The bridge's stable intern of polyana's documented effect vocabulary. Each
/// token gets one distinct bit; the SET-membership of a cap-bundle becomes a
/// bitmask the proven `is_facet_attenuation` law operates on. Extend this table
/// as polyana's vocabulary grows (≤ 32 tokens — `EffectMask` is `u32`).
const POLYANA_EFFECT_BITS: &[(&str, EffectMask)] = &[
    // EffectIntent kinds (polyana `src/policy/src/intent.rs`).
    ("tool-call", 1 << 0),
    ("model-call", 1 << 1),
    ("preview-load", 1 << 2),
    // cap-bundle filesystem face.
    ("filesystem:read", 1 << 3),
    ("filesystem:write", 1 << 4),
    // cap-bundle network face.
    ("network:localhost", 1 << 5),
    ("network:any", 1 << 6),
    // cap-bundle presentation face.
    ("deterministic-window", 1 << 7),
    ("streaming", 1 << 8),
];

fn bit_of(token: &str) -> Option<EffectMask> {
    POLYANA_EFFECT_BITS
        .iter()
        .find(|(t, _)| *t == token)
        .map(|(_, b)| *b)
}

/// Intern a cap-bundle's effect tokens into a single `EffectMask`. Returns the
/// first un-interned token as an error (fail-closed — see [`EffectInternError`]).
pub fn intern_effects(bundle: &CapBundle) -> Result<EffectMask, EffectInternError> {
    let mut mask: EffectMask = 0;
    for tok in &bundle.effects {
        match bit_of(tok) {
            Some(b) => mask |= b,
            None => return Err(EffectInternError(tok.clone())),
        }
    }
    Ok(mask)
}

/// Gate a `requested` cap-bundle against the `held` grant via the **proven**
/// monotone effect-set law (`dregg_cell::facet::is_facet_attenuation`).
///
/// `held` is assumed already-trusted (it is the operator-installed bundle); an
/// un-interned token there is also refused so a mis-typed grant can never widen
/// authority by accident. `requested` is the (possibly hostile) guest ask.
/// `Ok(())` exactly when `requested ⊆ held` on the effect set.
pub fn gate_effect_set(held: &CapBundle, requested: &CapBundle) -> Result<(), GateRefusal> {
    let held_mask = intern_effects(held)?;
    let granted_mask = intern_effects(requested)?;
    if is_facet_attenuation(held_mask, granted_mask) {
        Ok(())
    } else {
        Err(GateRefusal::NotAnAttenuation)
    }
}

/// Gate the *authorization-kind* face: `Ok(())` exactly when `granted` is an
/// attenuation of `held` per the proven `dregg_cell::is_attenuation`
/// (`granted.is_narrower_or_equal(held)`). Used when a boundary narrows not just
/// the effect set but the auth requirement itself.
pub fn gate_auth(held: &AuthRequired, granted: &AuthRequired) -> Result<(), GateRefusal> {
    if is_attenuation(held, granted) {
        Ok(())
    } else {
        Err(GateRefusal::NotAnAttenuation)
    }
}
