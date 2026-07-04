//! Slice 1 — `pa_witness` emits a real, chained dregg `TurnReceipt`.
//!
//! At polyana's `pa_witness` / `audit-mcp` boundary, in addition to writing a
//! `TraceRecord`, construct a real `dregg_turn::TurnReceipt` keyed on the same
//! call, chained to the previous receipt via `previous_receipt_hash`. The
//! `TraceRecord` stays for human debugging; the receipt is the unforgeable,
//! non-omitting spine (POLYANA-ALLIANCE.md §1.3, §4 Slice 1).
//!
//! The receipt's identity (`receipt_hash`) is the real dregg `dregg-receipt-v3`
//! BLAKE3 commitment computed by `dregg_turn::TurnReceipt::receipt_hash` — it
//! binds `previous_receipt_hash`, so the chain is tamper-evident: re-pointing
//! any link changes every downstream hash. (A STARK `TurnProof` attaches lazily
//! in the full dregg path; the receipt is born proofless, which is exactly
//! polyana's "evidence now, proof additive" posture.)

use crate::trace::TraceRecord;
use dregg_cell::CellId;
use dregg_turn::TurnReceipt;

const TURN_HASH_TAG: &[u8] = b"polyana-bridge/turn-v1";
const EFFECTS_HASH_TAG: &[u8] = b"polyana-bridge/effects-v1";

fn tagged(tag: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(tag);
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    *h.finalize().as_bytes()
}

/// Build a chained dregg `TurnReceipt` from a polyana `TraceRecord`.
///
/// - `agent` — the acting cell (polyana's `Principal` projected to a `CellId`).
/// - `pre_state_root` / `post_state_root` — the call's effect on owned state
///   (the workflow/actor state roots polyana already snapshots for replay).
/// - `previous_receipt_hash` — `None` for the first receipt in a feed, else the
///   prior receipt's `receipt_hash()`; this is the chain link / non-omission
///   spine.
///
/// The `turn_hash` commits the `(fn_name, args)` call; `effects_hash` commits
/// the canonical return. `action_count` is 1 (one provider call = one turn).
pub fn witness_receipt(
    trace: &TraceRecord,
    agent: CellId,
    pre_state_root: [u8; 32],
    post_state_root: [u8; 32],
    previous_receipt_hash: Option<[u8; 32]>,
) -> TurnReceipt {
    let turn_hash = tagged(
        TURN_HASH_TAG,
        &[trace.fn_name.as_bytes(), &trace.args_canonical],
    );
    let effects_hash = tagged(EFFECTS_HASH_TAG, &[&trace.ret_canonical]);
    // forest_hash: in full dregg this is the turn's effect-forest commitment;
    // here it binds the same call shape as turn_hash (the bridge does not
    // re-derive a forest, only witnesses the call).
    let forest_hash = turn_hash;

    TurnReceipt {
        turn_hash,
        forest_hash,
        pre_state_hash: pre_state_root,
        post_state_hash: post_state_root,
        timestamp: trace.timestamp_ns as i64,
        effects_hash,
        computrons_used: 0,
        action_count: 1,
        previous_receipt_hash,
        agent,
        ..Default::default()
    }
}
