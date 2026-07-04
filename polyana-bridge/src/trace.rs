//! polyana's per-call evidence record — the seam INPUT.
//!
//! Mirrors `polyana_core::provider::TraceRecord`
//! (`~/pug/polyana/src/core/src/provider.rs:324-336`). polyana already produces
//! one of these per provider call; it is byte-equal across providers via
//! `CanonicalValue` (sorted `BTreeMap` keys, NaN-bit-preserving floats) so the
//! same call replays identically on wasmtime/wasmi. The bridge consumes it
//! read-only and never modifies polyana's own record — Slice 1 is purely
//! additive (POLYANA-ALLIANCE.md §3, §4).

/// polyana's per-call evidence record. The fields the bridge keys a dregg
/// receipt on: the monotone sequence number, the canonical argument and return
/// bytes, and the human-facing call name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceRecord {
    /// Monotone per-actor sequence number. Becomes the receipt's dense
    /// `chain_index` in [`crate::attest`].
    pub seq: u64,
    /// Capture time in nanoseconds. Pinned into the receipt timestamp.
    pub timestamp_ns: u128,
    /// The provider function invoked (e.g. `"fs.read"`, `"model.complete"`).
    pub fn_name: String,
    /// `polyana_bincode`-legacy canonical encoding of the arguments.
    pub args_canonical: Vec<u8>,
    /// `polyana_bincode`-legacy canonical encoding of the result.
    pub ret_canonical: Vec<u8>,
}

impl TraceRecord {
    /// Convenience constructor for tests / call sites that already hold the
    /// canonical byte buffers.
    pub fn new(
        seq: u64,
        timestamp_ns: u128,
        fn_name: impl Into<String>,
        args_canonical: Vec<u8>,
        ret_canonical: Vec<u8>,
    ) -> Self {
        Self {
            seq,
            timestamp_ns,
            fn_name: fn_name.into(),
            args_canonical,
            ret_canonical,
        }
    }
}
