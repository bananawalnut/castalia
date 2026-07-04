//! The polyana ⋈ dregg seam, exercised against the REAL dregg primitives.
//!
//! These tests are the difference between the illustrative
//! `docs/deos/polyana-seam-sketch.rs` (local stub types, never compiled) and a
//! wired seam: every assertion below rides a real `dregg_cell` / `dregg_turn` /
//! `dregg_query` type, so a regression in the proven cores is caught here.

use dregg_cell::facet::is_facet_attenuation;
use dregg_cell::{AuthRequired, CellId};
use dregg_query::{Blake3Mmr, Coverage, EffectSummary, Pred, Query, Term, answer_whole_log};
use polyana_bridge::trace::TraceRecord;
use polyana_bridge::{
    CapBundle, GateRefusal, attest_whole_log, audit_records, gate_auth, gate_effect_set,
    intern_effects, witness_receipt,
};

fn agent(b: u8) -> CellId {
    let mut k = [0u8; 32];
    k[0] = b;
    CellId::from_bytes(k)
}

// ───────────────────────────── Slice 3: the proven cap gate ─────────────────

#[test]
fn gate_accepts_subset_refuses_amplification() {
    let held = CapBundle::new(["filesystem:read", "network:localhost", "streaming"]);

    // A subset request is an attenuation — accepted.
    let ok = CapBundle::new(["filesystem:read", "streaming"]);
    assert!(gate_effect_set(&held, &ok).is_ok());

    // The empty ask is the strongest attenuation.
    assert!(gate_effect_set(&held, &CapBundle::default()).is_ok());

    // Asking for an effect not in the bundle (write) is amplification — refused.
    let amp = CapBundle::new(["filesystem:read", "filesystem:write"]);
    assert_eq!(
        gate_effect_set(&held, &amp),
        Err(GateRefusal::NotAnAttenuation)
    );
}

#[test]
fn unknown_effect_token_fails_closed() {
    let held = CapBundle::new(["filesystem:read"]);
    let weird = CapBundle::new(["filesystem:read", "quantum:teleport"]);
    match gate_effect_set(&held, &weird) {
        Err(GateRefusal::UnknownEffect(e)) => assert_eq!(e.0, "quantum:teleport"),
        other => panic!("unknown token must fail closed, got {other:?}"),
    }
}

#[test]
fn gate_decision_is_exactly_the_lean_backed_facet_law() {
    // The bridge gate is not a re-implementation: it must agree bit-for-bit with
    // dregg's proven `is_facet_attenuation` over the interned masks.
    let held = CapBundle::new(["tool-call", "model-call", "network:localhost"]);
    let req = CapBundle::new(["model-call"]);

    let held_mask = intern_effects(&held).unwrap();
    let req_mask = intern_effects(&req).unwrap();

    let gate_ok = gate_effect_set(&held, &req).is_ok();
    assert_eq!(gate_ok, is_facet_attenuation(held_mask, req_mask));
    assert!(gate_ok);
}

#[test]
fn auth_kind_face_narrows_only() {
    // Either ⊇ Signature: narrowing Either→Signature is an attenuation.
    assert!(gate_auth(&AuthRequired::Either, &AuthRequired::Signature).is_ok());
    // Widening Signature→Either is refused by the proven `is_attenuation`.
    assert_eq!(
        gate_auth(&AuthRequired::Signature, &AuthRequired::Either),
        Err(GateRefusal::NotAnAttenuation)
    );
}

// ───────────────────────── Slice 1: chained dregg receipts ──────────────────

fn trace(seq: u64, name: &str) -> TraceRecord {
    TraceRecord::new(
        seq,
        1_000 + seq as u128,
        name,
        format!("args-{seq}").into_bytes(),
        format!("ret-{seq}").into_bytes(),
    )
}

#[test]
fn witness_chains_receipts_and_is_tamper_evident() {
    let who = agent(7);
    let r0 = witness_receipt(&trace(0, "fs.read"), who, [0u8; 32], [1u8; 32], None);
    let r1 = witness_receipt(
        &trace(1, "model.complete"),
        who,
        [1u8; 32],
        [2u8; 32],
        Some(r0.receipt_hash()),
    );

    // The chain link is real: r1 points at r0's committed hash.
    assert_eq!(r1.previous_receipt_hash, Some(r0.receipt_hash()));

    // Distinct calls commit to distinct turn hashes.
    assert_ne!(r0.turn_hash, r1.turn_hash);

    // Tamper-evidence: re-pointing the chain link changes r1's identity
    // (the v3 receipt hash binds `previous_receipt_hash`).
    let mut forged = r1.clone();
    forged.previous_receipt_hash = Some([9u8; 32]);
    assert_ne!(forged.receipt_hash(), r1.receipt_hash());
}

// ──────────────────── Slice 1 payoff: non-omission certificate ──────────────

#[test]
fn audit_log_yields_a_provable_non_omission_certificate() {
    let who = agent(3);
    let mut prev = None;
    let mut receipts = Vec::new();
    for seq in 0..5u64 {
        let r = witness_receipt(
            &trace(seq, "tool.call"),
            who,
            [seq as u8; 32],
            [(seq + 1) as u8; 32],
            prev,
        );
        prev = Some(r.receipt_hash());
        receipts.push(r);
    }

    // Project to the dregg-query fact-base rows, attach one queryable fact.
    let mut records = audit_records(receipts.iter().map(|r| (r, 100u64)));
    records[2].effects.push(EffectSummary::Created {
        agent: hex::encode(who.as_bytes()),
        cell: "spawned-worker".into(),
    });

    let (root, slice) = attest_whole_log(&records).expect("whole-log attestation");

    // `?- created(Agent, Cell, H).`
    let q = Query::new().atom(
        Pred::Created,
        vec![Term::var("Agent"), Term::var("Cell"), Term::var("H")],
    );
    let ans = answer_whole_log(slice, q).expect("answer");

    // The answer VERIFIES against the trusted root: it is provably computed from
    // exactly the committed receipt range — nothing omitted, nothing reordered.
    assert!(ans.verify(&Blake3Mmr, &root).is_ok());
    assert_eq!(ans.coverage, Coverage::WholeLog);
    assert_eq!(ans.rows.len(), 1, "the one Created fact is present");

    // A verifier with the WRONG root rejects it (the root is the only trust).
    let wrong = [0xABu8; 32];
    assert!(ans.verify(&Blake3Mmr, &wrong).is_err());
}

#[test]
fn omitting_a_receipt_breaks_the_certificate() {
    let who = agent(5);
    let mut prev = None;
    let mut receipts = Vec::new();
    for seq in 0..4u64 {
        let r = witness_receipt(&trace(seq, "fs.write"), who, [0u8; 32], [1u8; 32], prev);
        prev = Some(r.receipt_hash());
        receipts.push(r);
    }
    let records = audit_records(receipts.iter().map(|r| (r, 7u64)));
    let (root, _good) = attest_whole_log(&records).unwrap();

    // The server tries to pass off a log with position 2 dropped. Re-attesting
    // the truncated set yields a DIFFERENT root, so a verifier holding the
    // genuine `root` rejects the omission.
    let mut omitted = records.clone();
    omitted.remove(2);
    // Re-densify so attest_whole_log accepts the malformed feed at all.
    for (i, r) in omitted.iter_mut().enumerate() {
        r.chain_index = i as u64;
    }
    let (forged_root, slice) = attest_whole_log(&omitted).unwrap();
    assert_ne!(forged_root, root, "an omission changes the MMR root");

    let q = Query::new().atom(Pred::Revoked, vec![Term::var("Cap"), Term::var("H")]);
    let ans = answer_whole_log(slice, q).unwrap();
    // Verifies against its own forged root, but NOT against the genuine one.
    assert!(ans.verify(&Blake3Mmr, &forged_root).is_ok());
    assert!(ans.verify(&Blake3Mmr, &root).is_err());
}
