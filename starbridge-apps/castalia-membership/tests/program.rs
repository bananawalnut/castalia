use dregg_cell::EvalContext;
use dregg_cell::program::TransitionMeta;
use dregg_cell::state::CellState;
use dregg_types::CellId;
use starbridge_castalia_membership::{
    APPLICATION_FLAGS_SLOT, APPLICATION_KIND_SLOT, APPLICATION_NONCE_SLOT,
    APPLICATION_VERSION_SLOT, CASTALIA_MEMBERSHIP_SCHEMA_VERSION, CHANGED_AT_SLOT,
    COMMITMENT_SLOT_START, CREATED_AT_SLOT, CastaliaMemberApplicationV1, GENERATION_SLOT,
    JURISDICTION_CODE_SLOT, MAGIC_CASTMEM1, MAGIC_SLOT, MEMBERSHIP_CLASS_SLOT, MembershipStatus,
    SCHEMA_VERSION_SLOT, STATUS_SLOT, castalia_membership_child_program_vk,
    castalia_membership_factory_vk, castalia_membership_program, field_from_u64,
    membership_initial_fields, symbol,
};

const AUTHORITY: [u8; 32] = [0x41; 32];

fn application() -> CastaliaMemberApplicationV1 {
    CastaliaMemberApplicationV1 {
        factory_id: castalia_membership_factory_vk(AUTHORITY),
        program_id: castalia_membership_child_program_vk(AUTHORITY),
        official_dregg_cell_id: CellId::from_bytes([0x22; 32]),
        owner_pubkey: AUTHORITY,
        application_kind: 7,
        application_version: 3,
        application_nonce: 99,
        membership_class: 4,
        jurisdiction_code: 840,
        application_flags: 0x55,
        created_at: 1_000,
    }
}

fn state(status: MembershipStatus, generation: u64, changed_at: u64) -> CellState {
    let mut state = CellState::new(0);
    for (index, value) in membership_initial_fields(&application()) {
        state.set_field(index as usize, field_from_u64(value));
    }
    state.set_field(STATUS_SLOT as usize, field_from_u64(status as u64));
    state.set_field(GENERATION_SLOT as usize, field_from_u64(generation));
    state.set_field(CHANGED_AT_SLOT as usize, field_from_u64(changed_at));
    state
}

fn evaluate(
    method: &str,
    old: &CellState,
    new: &CellState,
    sender: Option<[u8; 32]>,
) -> Result<(), Box<dregg_cell::ProgramError>> {
    let context = EvalContext {
        sender,
        ..Default::default()
    };
    castalia_membership_program(AUTHORITY)
        .evaluate_with_meta(
            new,
            Some(old),
            Some(&context),
            &TransitionMeta::new(symbol(method), 0),
        )
        .map_err(Box::new)
}

#[test]
fn schema_constants_slots_and_status_codes_are_fixed() {
    assert_eq!(MAGIC_CASTMEM1, u64::from_le_bytes(*b"CASTMEM1"));
    assert_eq!(CASTALIA_MEMBERSHIP_SCHEMA_VERSION, 1);
    assert_eq!(
        [
            MAGIC_SLOT,
            SCHEMA_VERSION_SLOT,
            APPLICATION_KIND_SLOT,
            APPLICATION_VERSION_SLOT,
            APPLICATION_NONCE_SLOT,
            MEMBERSHIP_CLASS_SLOT,
            JURISDICTION_CODE_SLOT,
            APPLICATION_FLAGS_SLOT,
            COMMITMENT_SLOT_START,
            STATUS_SLOT,
            GENERATION_SLOT,
            CREATED_AT_SLOT,
            CHANGED_AT_SLOT,
        ],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 13, 14, 15]
    );
    assert_eq!(MembershipStatus::Pending as u64, 0);
    assert_eq!(MembershipStatus::Active as u64, 1);
    assert_eq!(MembershipStatus::Suspended as u64, 2);
    assert_eq!(MembershipStatus::Revoked as u64, 3);
    assert_eq!(MembershipStatus::Expired as u64, 4);
}

#[test]
fn application_commitment_uses_explicit_fixed_order_encoding() {
    let app = application();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"castalia/member-application/v1\0");
    bytes.extend_from_slice(&app.factory_id);
    bytes.extend_from_slice(&app.program_id);
    bytes.extend_from_slice(app.official_dregg_cell_id.as_bytes());
    bytes.extend_from_slice(&app.owner_pubkey);
    for value in [
        app.application_kind,
        app.application_version,
        app.application_nonce,
        app.membership_class,
        app.jurisdiction_code,
        app.application_flags,
        app.created_at,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    assert_eq!(app.canonical_bytes(), bytes);
    assert_eq!(app.commitment(), *blake3::hash(&bytes).as_bytes());

    let mut changed = app;
    changed.application_nonce += 1;
    assert_ne!(changed.commitment(), app.commitment());
}

#[test]
fn commitment_slots_are_four_little_endian_u64_limbs() {
    let app = application();
    let commitment = app.commitment();
    let fields = membership_initial_fields(&app);
    for limb in 0..4 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&commitment[limb * 8..(limb + 1) * 8]);
        assert_eq!(
            fields[(COMMITMENT_SLOT_START as usize) + limb],
            (
                (COMMITMENT_SLOT_START as u32) + limb as u32,
                u64::from_le_bytes(bytes)
            )
        );
    }
}

#[test]
fn authority_sender_is_required_and_missing_sender_fails_closed() {
    let old = state(MembershipStatus::Pending, 0, 1_000);
    let new = state(MembershipStatus::Active, 1, 1_001);
    assert!(evaluate("activate", &old, &new, Some(AUTHORITY)).is_ok());
    assert!(evaluate("activate", &old, &new, Some([0x99; 32])).is_err());
    assert!(evaluate("activate", &old, &new, None).is_err());
}

#[test]
fn immutable_application_and_commitment_substitution_are_rejected() {
    let old = state(MembershipStatus::Pending, 0, 1_000);
    let mut metadata_mutation = state(MembershipStatus::Active, 1, 1_001);
    metadata_mutation.set_field(MEMBERSHIP_CLASS_SLOT as usize, field_from_u64(5));
    assert!(evaluate("activate", &old, &metadata_mutation, Some(AUTHORITY)).is_err());

    let mut commitment_substitution = state(MembershipStatus::Active, 1, 1_001);
    commitment_substitution.set_field(COMMITMENT_SLOT_START as usize, field_from_u64(123));
    assert!(evaluate("activate", &old, &commitment_substitution, Some(AUTHORITY)).is_err());
}

#[test]
fn unknown_method_and_stale_or_skipped_generation_are_rejected() {
    let old = state(MembershipStatus::Pending, 4, 1_000);
    let valid = state(MembershipStatus::Active, 5, 1_001);
    assert!(evaluate("invented", &old, &valid, Some(AUTHORITY)).is_err());

    let stale = state(MembershipStatus::Active, 4, 1_001);
    assert!(evaluate("activate", &old, &stale, Some(AUTHORITY)).is_err());
    let skipped = state(MembershipStatus::Active, 6, 1_001);
    assert!(evaluate("activate", &old, &skipped, Some(AUTHORITY)).is_err());
}

#[test]
fn changed_at_must_strictly_advance() {
    let old = state(MembershipStatus::Pending, 0, 1_000);
    let unchanged = state(MembershipStatus::Active, 1, 1_000);
    assert!(evaluate("activate", &old, &unchanged, Some(AUTHORITY)).is_err());
    let rewound = state(MembershipStatus::Active, 1, 999);
    assert!(evaluate("activate", &old, &rewound, Some(AUTHORITY)).is_err());
}

#[test]
fn lifecycle_methods_admit_only_their_exact_status_transitions() {
    let valid = [
        (
            "activate",
            MembershipStatus::Pending,
            MembershipStatus::Active,
        ),
        (
            "suspend",
            MembershipStatus::Active,
            MembershipStatus::Suspended,
        ),
        (
            "resume",
            MembershipStatus::Suspended,
            MembershipStatus::Active,
        ),
        (
            "revoke",
            MembershipStatus::Pending,
            MembershipStatus::Revoked,
        ),
        (
            "revoke",
            MembershipStatus::Active,
            MembershipStatus::Revoked,
        ),
        (
            "revoke",
            MembershipStatus::Suspended,
            MembershipStatus::Revoked,
        ),
        (
            "expire",
            MembershipStatus::Pending,
            MembershipStatus::Expired,
        ),
        (
            "expire",
            MembershipStatus::Active,
            MembershipStatus::Expired,
        ),
        (
            "expire",
            MembershipStatus::Suspended,
            MembershipStatus::Expired,
        ),
    ];
    for (method, from, to) in valid {
        assert!(
            evaluate(
                method,
                &state(from, 10, 100),
                &state(to, 11, 101),
                Some(AUTHORITY)
            )
            .is_ok(),
            "{method}: {from:?} -> {to:?}"
        );
    }

    let invalid = [
        (
            "activate",
            MembershipStatus::Suspended,
            MembershipStatus::Active,
        ),
        (
            "suspend",
            MembershipStatus::Pending,
            MembershipStatus::Suspended,
        ),
        ("resume", MembershipStatus::Active, MembershipStatus::Active),
        (
            "revoke",
            MembershipStatus::Revoked,
            MembershipStatus::Revoked,
        ),
        (
            "expire",
            MembershipStatus::Expired,
            MembershipStatus::Expired,
        ),
        (
            "activate",
            MembershipStatus::Revoked,
            MembershipStatus::Active,
        ),
        (
            "resume",
            MembershipStatus::Expired,
            MembershipStatus::Active,
        ),
    ];
    for (method, from, to) in invalid {
        assert!(
            evaluate(
                method,
                &state(from, 10, 100),
                &state(to, 11, 101),
                Some(AUTHORITY)
            )
            .is_err(),
            "unexpectedly admitted {method}: {from:?} -> {to:?}"
        );
    }
}

#[test]
fn out_of_range_status_is_rejected() {
    let old = state(MembershipStatus::Pending, 0, 100);
    let mut new = state(MembershipStatus::Active, 1, 101);
    new.set_field(STATUS_SLOT as usize, field_from_u64(99));
    assert!(evaluate("activate", &old, &new, Some(AUTHORITY)).is_err());
}
