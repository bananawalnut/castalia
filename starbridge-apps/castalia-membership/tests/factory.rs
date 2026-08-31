use dregg_cell::{AuthRequired, CapGrant, CapTarget, CellMode};
use dregg_types::CellId;
use starbridge_castalia_membership::{
    CastaliaMemberApplicationV1, MembershipStatus, STATUS_SLOT,
    castalia_membership_child_program_vk, castalia_membership_factory,
    castalia_membership_factory_vk, membership_creation_params, membership_initial_fields,
    validate_membership_birth,
};

const AUTHORITY: [u8; 32] = [0x41; 32];
const OWNER: [u8; 32] = [0x52; 32];

fn application() -> CastaliaMemberApplicationV1 {
    CastaliaMemberApplicationV1 {
        factory_id: castalia_membership_factory_vk(AUTHORITY),
        program_id: castalia_membership_child_program_vk(AUTHORITY),
        official_dregg_cell_id: CellId::from_bytes([0x22; 32]),
        owner_pubkey: OWNER,
        application_kind: 7,
        application_version: 1,
        application_nonce: 99,
        membership_class: 2,
        jurisdiction_code: 840,
        application_flags: 0,
        created_at: 1_700_000_000,
    }
}

#[test]
fn canonical_factory_birth_is_accepted() {
    let app = application();
    let factory = castalia_membership_factory(AUTHORITY).unwrap();
    let params = factory.creation_params(&app).unwrap();

    assert_eq!(
        factory.factory_vk(),
        castalia_membership_factory_vk(AUTHORITY)
    );
    assert_eq!(
        factory.child_program_vk(),
        castalia_membership_child_program_vk(AUTHORITY)
    );
    assert_eq!(params.mode, CellMode::Sovereign);
    assert!(params.initial_caps.is_empty());
    assert_eq!(params.owner_pubkey, AUTHORITY);
    assert_eq!(params.initial_fields, membership_initial_fields(&app));
    assert!(factory.validate_birth(&app, &params).is_ok());
}

#[test]
fn wrong_owner_or_child_program_is_rejected() {
    let app = application();

    let mut wrong_owner = membership_creation_params(&app, AUTHORITY).unwrap();
    wrong_owner.owner_pubkey = [0x99; 32];
    assert!(validate_membership_birth(&app, AUTHORITY, &wrong_owner).is_err());

    let mut wrong_program = membership_creation_params(&app, AUTHORITY).unwrap();
    wrong_program.program_vk = Some([0x88; 32]);
    assert!(validate_membership_birth(&app, AUTHORITY, &wrong_program).is_err());
}

#[test]
fn member_identity_is_committed_while_castalia_authority_owns_the_cell() {
    let app = application();
    let params = membership_creation_params(&app, AUTHORITY).unwrap();

    assert_eq!(app.owner_pubkey, OWNER);
    assert_eq!(params.owner_pubkey, AUTHORITY);
    assert_ne!(app.owner_pubkey, params.owner_pubkey);
}

#[test]
fn hosted_mode_and_any_initial_capability_are_rejected() {
    let app = application();

    let mut hosted = membership_creation_params(&app, AUTHORITY).unwrap();
    hosted.mode = CellMode::Hosted;
    assert!(validate_membership_birth(&app, AUTHORITY, &hosted).is_err());

    let mut capped = membership_creation_params(&app, AUTHORITY).unwrap();
    capped.initial_caps.push(CapGrant {
        target: CapTarget::SelfCell,
        max_permissions: AuthRequired::Signature,
        attenuatable: false,
    });
    assert!(validate_membership_birth(&app, AUTHORITY, &capped).is_err());
}

#[test]
fn altered_missing_extra_reordered_or_duplicate_fields_are_rejected() {
    let app = application();

    let mut altered = membership_creation_params(&app, AUTHORITY).unwrap();
    altered.initial_fields[STATUS_SLOT as usize].1 = MembershipStatus::Active as u64;
    assert!(validate_membership_birth(&app, AUTHORITY, &altered).is_err());

    let mut missing = membership_creation_params(&app, AUTHORITY).unwrap();
    missing.initial_fields.pop();
    assert!(validate_membership_birth(&app, AUTHORITY, &missing).is_err());

    let mut extra = membership_creation_params(&app, AUTHORITY).unwrap();
    extra.initial_fields.push((16, 1));
    assert!(validate_membership_birth(&app, AUTHORITY, &extra).is_err());

    let mut reordered = membership_creation_params(&app, AUTHORITY).unwrap();
    reordered.initial_fields.swap(0, 1);
    assert!(validate_membership_birth(&app, AUTHORITY, &reordered).is_err());

    let mut duplicate = membership_creation_params(&app, AUTHORITY).unwrap();
    duplicate.initial_fields[1].0 = duplicate.initial_fields[0].0;
    assert!(validate_membership_birth(&app, AUTHORITY, &duplicate).is_err());
}

#[test]
fn application_must_name_the_castalia_factory_and_authority_bound_program() {
    let mut wrong_factory = application();
    wrong_factory.factory_id = [0x77; 32];
    assert!(membership_creation_params(&wrong_factory, AUTHORITY).is_err());

    let mut wrong_program = application();
    wrong_program.program_id = [0x66; 32];
    assert!(membership_creation_params(&wrong_program, AUTHORITY).is_err());
}

#[test]
fn zero_identity_material_is_rejected() {
    let mut no_cell = application();
    no_cell.official_dregg_cell_id = CellId::from_bytes([0; 32]);
    assert!(membership_creation_params(&no_cell, AUTHORITY).is_err());

    let mut no_owner = application();
    no_owner.owner_pubkey = [0; 32];
    assert!(membership_creation_params(&no_owner, AUTHORITY).is_err());
}

#[test]
fn birth_params_for_one_application_cannot_validate_another() {
    let first = application();
    let mut second = first;
    second.application_nonce += 1;

    let first_params = membership_creation_params(&first, AUTHORITY).unwrap();
    assert!(validate_membership_birth(&second, AUTHORITY, &first_params).is_err());
}
