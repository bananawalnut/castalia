use dregg_cell::{Cell, CellConfig, Ledger};
use dregg_federation::frost::{MlDsaPublicKey, MlDsaSigningKey};
use dregg_node::membership_inspection::{
    AuthenticatedCellInspection, CastaliaMembershipExpectation, MembershipInspectionPolicy,
    inspect_castalia_membership, verify_authenticated_cell,
};
use dregg_persist::federation::{QuorumSignature, StoredAttestedRoot};
use dregg_types::{FederationId, SigningKey, sign};
use starbridge_castalia_membership::{
    CastaliaMemberApplicationV1, MembershipStatus, castalia_membership_factory_vk,
    castalia_membership_program, field_from_u64, membership_birth_token_id,
    membership_initial_fields,
};

const AUTHORITY: [u8; 32] = [0x41; 32];
const MEMBER_KEY: [u8; 32] = [0x52; 32];

fn membership_application() -> CastaliaMemberApplicationV1 {
    CastaliaMemberApplicationV1 {
        factory_id: castalia_membership_factory_vk(AUTHORITY),
        program_id: starbridge_castalia_membership::castalia_membership_child_program_vk(AUTHORITY),
        official_dregg_cell_id: dregg_types::CellId::from_bytes([0x22; 32]),
        owner_pubkey: MEMBER_KEY,
        application_kind: 7,
        application_version: 1,
        application_nonce: 99,
        membership_class: 2,
        jurisdiction_code: 840,
        application_flags: 0,
        created_at: 1_700_000_000,
    }
}

fn membership_cell(status: MembershipStatus) -> Cell {
    let application = membership_application();
    let token = membership_birth_token_id(application.factory_id, application.commitment(), 7);
    let mut cell = Cell::from_config(
        AUTHORITY,
        token,
        CellConfig::hosted().with_program(castalia_membership_program(AUTHORITY)),
    );
    for (index, value) in membership_initial_fields(&application) {
        cell.state.set_field(index as usize, field_from_u64(value));
    }
    cell.state.set_field(12, field_from_u64(status as u64));
    if status == MembershipStatus::Active {
        cell.state.set_field(13, field_from_u64(1));
        cell.state.set_field(15, field_from_u64(1_700_000_010));
    }
    cell
}

fn genuine_fixture() -> (
    Cell,
    AuthenticatedCellInspection,
    MembershipInspectionPolicy,
) {
    let cell = Cell::new_hosted([0x41; 32], [0x42; 32]);
    let cell_id = *cell.id().as_bytes();
    let cell_bytes = postcard::to_stdvec(&cell).unwrap();
    let mut ledger = Ledger::new();
    ledger.insert_cell(cell.clone()).unwrap();
    let leaves = dregg_persist::canonical_ledger_leaves(&ledger);
    let merkle_root = dregg_persist::canonical_ledger_root_from_leaves(&leaves);

    let seeds = [1u8, 2, 3];
    let signing_keys: Vec<_> = seeds
        .iter()
        .map(|seed| SigningKey::from_bytes(&[*seed; 32]))
        .collect();
    let committee = signing_keys
        .iter()
        .map(SigningKey::public_key)
        .collect::<Vec<_>>();
    let pq_keys = seeds
        .iter()
        .map(|seed| MlDsaSigningKey::from_seed(&[*seed; 32]))
        .collect::<Vec<_>>();
    let ml_dsa_committee = pq_keys
        .iter()
        .map(|(public, _)| public.clone())
        .collect::<Vec<_>>();
    let block_id = [0x55; 32];
    let vote_message = dregg_types::finalization_vote_signing_message(&block_id, &merkle_root);
    let finalization_quorum = signing_keys
        .iter()
        .zip(pq_keys.iter())
        .map(|(key, (pq_public, pq_secret))| QuorumSignature {
            voter: key.public_key(),
            signature: sign(key, &vote_message),
            ml_dsa_pubkey: pq_public.0.to_vec(),
            pq_signature: pq_secret.sign(&vote_message).unwrap(),
        })
        .collect();
    let federation_id = FederationId([0x33; 32]);
    let attested_root = StoredAttestedRoot {
        merkle_root,
        note_tree_root: None,
        nullifier_set_root: None,
        height: 42,
        timestamp: 1_700_000_000,
        blocklace_block_id: Some(block_id),
        finality_round: Some(9),
        quorum_signatures: vec![],
        threshold_qc: None,
        threshold: 3,
        federation_id,
        receipt_stream_root: None,
        finalization_quorum,
    };
    let inspection = AuthenticatedCellInspection {
        cell_id,
        cell_bytes,
        leaves,
        attested_root,
    };
    let policy = MembershipInspectionPolicy {
        federation_id,
        committee,
        ml_dsa_committee,
        minimum_height: 40,
        now_unix_seconds: 1_700_000_020,
        maximum_age_seconds: 60,
        maximum_future_skew_seconds: 5,
    };
    (cell, inspection, policy)
}

#[test]
fn accepts_exact_cell_bytes_under_genuine_pinned_hybrid_quorum() {
    let (cell, inspection, policy) = genuine_fixture();
    let verified = verify_authenticated_cell(&inspection, &policy).unwrap();
    assert_eq!(verified, cell);
}

#[test]
fn rejects_each_untrusted_binding_and_count_only_attestation() {
    let (_, inspection, policy) = genuine_fixture();

    let mut hostile = inspection.clone();
    hostile.cell_bytes.push(0);
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.cell_id[0] ^= 1;
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.cell_bytes[0] ^= 1;
    hostile.cell_id[0] ^= 1;
    hostile.leaves[0].0 = hostile.cell_id;
    hostile.leaves[0].1 = *blake3::hash(&hostile.cell_bytes).as_bytes();
    hostile.attested_root.merkle_root =
        dregg_persist::canonical_ledger_root_from_leaves(&hostile.leaves);
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.leaves[0].1[0] ^= 1;
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.attested_root.finalization_quorum.clear();
    hostile.attested_root.threshold = 1;
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile_policy = policy.clone();
    hostile_policy.federation_id = FederationId([0x99; 32]);
    assert!(verify_authenticated_cell(&inspection, &hostile_policy).is_err());

    let mut hostile_policy = policy.clone();
    hostile_policy.minimum_height = 43;
    assert!(verify_authenticated_cell(&inspection, &hostile_policy).is_err());

    let mut hostile_policy = policy.clone();
    hostile_policy.now_unix_seconds = 1_700_000_061;
    assert!(verify_authenticated_cell(&inspection, &hostile_policy).is_err());

    let mut hostile_policy = policy;
    hostile_policy.now_unix_seconds = 1_699_999_994;
    assert!(verify_authenticated_cell(&inspection, &hostile_policy).is_err());
}

#[test]
fn rejects_wrong_or_misaligned_pq_roster_without_classical_downgrade() {
    let (_, inspection, policy) = genuine_fixture();
    let mut hostile = policy.clone();
    hostile.ml_dsa_committee.clear();
    assert!(verify_authenticated_cell(&inspection, &hostile).is_err());

    let mut hostile = policy;
    hostile.ml_dsa_committee[0] = MlDsaPublicKey([0u8; 1952]);
    assert!(verify_authenticated_cell(&inspection, &hostile).is_err());
}

#[test]
fn rejects_forged_duplicate_noncommittee_and_wrong_root_votes() {
    let (_, inspection, policy) = genuine_fixture();

    let mut hostile = inspection.clone();
    hostile.attested_root.finalization_quorum[0].signature.0[0] ^= 1;
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.attested_root.finalization_quorum[1] =
        hostile.attested_root.finalization_quorum[0].clone();
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.attested_root.finalization_quorum[0].voter =
        SigningKey::from_bytes(&[0x91; 32]).public_key();
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection.clone();
    hostile.attested_root.blocklace_block_id = Some([0x99; 32]);
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());

    let mut hostile = inspection;
    hostile.attested_root.threshold = 0;
    assert!(verify_authenticated_cell(&hostile, &policy).is_err());
}

#[test]
fn interprets_exact_active_castmem1_cell_against_member_key_application() {
    let application = membership_application();
    let cell = membership_cell(MembershipStatus::Active);
    let membership = inspect_castalia_membership(
        &cell,
        &CastaliaMembershipExpectation {
            authority_public_key: AUTHORITY,
            application,
            birth_nonce: 7,
            required_status: Some(MembershipStatus::Active),
        },
    )
    .unwrap();
    assert_eq!(membership.member_public_key, MEMBER_KEY);
    assert_eq!(membership.status, MembershipStatus::Active);
    assert_eq!(membership.generation, 1);
    assert_eq!(membership.changed_at, 1_700_000_010);
}

#[test]
fn rejects_wrong_key_authority_program_fields_status_and_timestamps() {
    let application = membership_application();
    let expectation = CastaliaMembershipExpectation {
        authority_public_key: AUTHORITY,
        application,
        birth_nonce: 7,
        required_status: Some(MembershipStatus::Active),
    };

    let mut wrong = expectation.clone();
    wrong.application.owner_pubkey = [0x99; 32];
    assert!(
        inspect_castalia_membership(&membership_cell(MembershipStatus::Active), &wrong).is_err()
    );

    let mut wrong = expectation.clone();
    wrong.authority_public_key = [0x99; 32];
    assert!(
        inspect_castalia_membership(&membership_cell(MembershipStatus::Active), &wrong).is_err()
    );

    let mut wrong = expectation.clone();
    wrong.birth_nonce = 8;
    assert!(
        inspect_castalia_membership(&membership_cell(MembershipStatus::Active), &wrong).is_err()
    );

    let mut cell = membership_cell(MembershipStatus::Active);
    cell.program = dregg_cell::CellProgram::None;
    assert!(inspect_castalia_membership(&cell, &expectation).is_err());

    let mut cell = membership_cell(MembershipStatus::Active);
    cell.state.set_field(5, field_from_u64(3));
    assert!(inspect_castalia_membership(&cell, &expectation).is_err());

    assert!(
        inspect_castalia_membership(&membership_cell(MembershipStatus::Suspended), &expectation,)
            .is_err()
    );

    let mut cell = membership_cell(MembershipStatus::Active);
    cell.state.set_field(15, field_from_u64(1_699_999_999));
    assert!(inspect_castalia_membership(&cell, &expectation).is_err());
}
