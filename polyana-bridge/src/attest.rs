//! Slice 1's payoff — a provable NON-OMISSION certificate over the audit log.
//!
//! Once polyana's audit feed is a chain of dregg receipts ([`crate::witness`]),
//! an operator can answer over it with a `dregg_query::AttestedAnswer` carrying
//! a range opening against the receipt-log MMR root. A verifying answer is
//! provably computed from EXACTLY the committed receipt range — nothing hidden,
//! nothing forged, nothing reordered. This is the Rust embodiment of
//! `metatheory/Dregg2/Lightclient/MMR.lean`'s `server_cannot_omit_position`,
//! the thing polyana's "evidence-native" most wants and does not yet have
//! (POLYANA-ALLIANCE.md §1.3, §4 Slice 1).
//!
//! This module assembles the prover side: polyana receipts → `ReceiptRecord`s →
//! an MMR → an `AttestedSlice` over the WHOLE log prefix. The caller then runs
//! `dregg_query::answer_whole_log(slice, query)` and ships the answer; any
//! verifier checks it with only a trusted root via `AttestedAnswer::verify`.

use dregg_query::{AttestedSlice, Blake3Mmr, Height, Mmr, RangeCertificate, ReceiptRecord};
use dregg_turn::TurnReceipt;
use thiserror::Error;

/// Failure assembling the attested slice.
#[derive(Debug, Error)]
pub enum AttestBuildError {
    #[error("cannot attest an empty receipt log")]
    Empty,
    #[error("receipt {slot} carries a non-dense chain_index {got} (expected {want})")]
    NonDenseIndex { slot: usize, got: u64, want: u64 },
    #[error(transparent)]
    Receipt(#[from] dregg_query::receipt::ReceiptError),
}

/// Project a chain of dregg receipts into `dregg_query::ReceiptRecord`s — the
/// fact-base rows the attested query evaluates over.
///
/// `chain_index` is the dense position (0-based by feed order — the key the MMR
/// commits). `height` is taken per receipt (polyana's commit height; pass the
/// same value for solo/tentative rows). Effect summaries are left empty here —
/// the non-omission certificate is over the receipt-hash MMR and is independent
/// of effect enrichment; a caller that wants queryable facts attaches
/// `EffectSummary`s to the returned rows.
pub fn audit_records<'a>(
    entries: impl IntoIterator<Item = (&'a TurnReceipt, Height)>,
) -> Vec<ReceiptRecord> {
    entries
        .into_iter()
        .enumerate()
        .map(|(i, (r, height))| ReceiptRecord {
            chain_index: i as u64,
            receipt_hash: hex::encode(r.receipt_hash()),
            height,
            agent: hex::encode(r.agent.as_bytes()),
            effects: Vec::new(),
        })
        .collect()
}

/// Build the MMR over a receipt log and open the WHOLE prefix `[0, len-1]`,
/// returning the root-pinned commitment and the certified slice. The slice fed
/// to `dregg_query::answer_whole_log` yields an answer whose `verify` against
/// this same root is the unqualified "provably omitted nothing" claim for a
/// monotone query.
///
/// Requires the records to carry dense, in-order `chain_index`es (what
/// [`audit_records`] produces); a gap or reorder is refused so a malformed feed
/// cannot mint a certificate.
pub fn attest_whole_log(
    records: &[ReceiptRecord],
) -> Result<([u8; 32], AttestedSlice), AttestBuildError> {
    if records.is_empty() {
        return Err(AttestBuildError::Empty);
    }
    let mut mmr = Mmr::new(Blake3Mmr);
    for (slot, r) in records.iter().enumerate() {
        let want = slot as u64;
        if r.chain_index != want {
            return Err(AttestBuildError::NonDenseIndex {
                slot,
                got: r.chain_index,
                want,
            });
        }
        mmr.push(r.receipt_hash_bytes()?);
    }
    let root = mmr.root();
    let hi = records.len() as u64 - 1;
    let (_values, opening) = mmr.open_range(0, hi);
    let slice = AttestedSlice {
        receipts: records.to_vec(),
        cert: RangeCertificate {
            root,
            lo: 0,
            hi,
            opening,
        },
    };
    Ok((root, slice))
}
