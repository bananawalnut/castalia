use dregg_cell::{ChildVkStrategy, FactoryRegistry};
use dregg_types::CellId;
use starbridge_castalia_membership::{
    CastaliaMemberApplicationV1, MembershipBirthError, castalia_membership_child_program_vk,
    castalia_membership_factory, castalia_membership_program,
};

const AUTHORITY: [u8; 32] = [0x41; 32];
const ATTACKER: [u8; 32] = [0xa7; 32];
const OWNER: [u8; 32] = [0x52; 32];

fn application(authority: [u8; 32], factory_id: [u8; 32]) -> CastaliaMemberApplicationV1 {
    CastaliaMemberApplicationV1 {
        factory_id,
        program_id: castalia_membership_child_program_vk(authority),
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
fn zero_authority_is_rejected() {
    assert_eq!(
        castalia_membership_factory([0; 32]).unwrap_err(),
        MembershipBirthError::MissingAuthority
    );
}

#[test]
fn authority_is_bound_into_factory_identity() {
    let castalia = castalia_membership_factory(AUTHORITY).unwrap();
    let attacker = castalia_membership_factory(ATTACKER).unwrap();
    assert_ne!(castalia.factory_vk(), attacker.factory_vk());
    assert_ne!(castalia.child_program_vk(), attacker.child_program_vk());
}

#[test]
fn attacker_factory_birth_cannot_validate_under_castalia_factory() {
    let castalia = castalia_membership_factory(AUTHORITY).unwrap();
    let attacker = castalia_membership_factory(ATTACKER).unwrap();
    let attacker_app = application(ATTACKER, attacker.factory_vk());
    let attacker_params = attacker.creation_params(&attacker_app).unwrap();

    assert!(
        attacker
            .validate_birth(&attacker_app, &attacker_params)
            .is_ok()
    );
    assert!(
        castalia
            .validate_birth(&attacker_app, &attacker_params)
            .is_err()
    );
}

#[test]
fn invalid_application_cannot_produce_issuable_params() {
    let factory = castalia_membership_factory(AUTHORITY).unwrap();

    let wrong_factory = application(AUTHORITY, [0x77; 32]);
    assert!(factory.creation_params(&wrong_factory).is_err());

    let mut wrong_program = application(AUTHORITY, factory.factory_vk());
    wrong_program.program_id = [0x66; 32];
    assert!(factory.creation_params(&wrong_program).is_err());

    let mut no_cell = application(AUTHORITY, factory.factory_vk());
    no_cell.official_dregg_cell_id = CellId::from_bytes([0; 32]);
    assert!(factory.creation_params(&no_cell).is_err());

    let mut no_owner = application(AUTHORITY, factory.factory_vk());
    no_owner.owner_pubkey = [0; 32];
    assert!(factory.creation_params(&no_owner).is_err());
}

#[test]
fn d0_application_values_fail_closed_before_birth() {
    let factory = castalia_membership_factory(AUTHORITY).unwrap();
    let valid = application(AUTHORITY, factory.factory_vk());

    let mut wrong_kind = valid;
    wrong_kind.application_kind = 8;
    assert_eq!(
        factory.creation_params(&wrong_kind),
        Err(MembershipBirthError::UnsupportedApplicationKind)
    );

    let mut wrong_version = valid;
    wrong_version.application_version = 3;
    assert_eq!(
        factory.creation_params(&wrong_version),
        Err(MembershipBirthError::UnsupportedApplicationVersion)
    );

    let mut zero_nonce = valid;
    zero_nonce.application_nonce = 0;
    assert_eq!(
        factory.creation_params(&zero_nonce),
        Err(MembershipBirthError::MissingApplicationNonce)
    );

    for membership_class in [0, 3, u64::MAX] {
        let mut invalid = valid;
        invalid.membership_class = membership_class;
        assert_eq!(
            factory.creation_params(&invalid),
            Err(MembershipBirthError::UnsupportedMembershipClass)
        );
    }

    let mut unknown_flags = valid;
    unknown_flags.application_flags = 1;
    assert_eq!(
        factory.creation_params(&unknown_flags),
        Err(MembershipBirthError::UnknownApplicationFlags)
    );
}

#[test]
fn sealed_deployment_installs_only_the_authority_bound_full_child_program() {
    let factory = castalia_membership_factory(AUTHORITY).unwrap();
    let mut registry = FactoryRegistry::new();
    let factory_vk = factory.deploy_checked(&mut registry).unwrap();
    let descriptor = registry.get(&factory_vk).unwrap();

    assert_eq!(
        descriptor.child_vk_strategy,
        Some(ChildVkStrategy::Fixed(Some(
            castalia_membership_child_program_vk(AUTHORITY)
        )))
    );
    assert_eq!(
        descriptor.child_program_vk,
        Some(castalia_membership_child_program_vk(AUTHORITY))
    );
    assert_eq!(
        registry.full_child_program(&factory_vk),
        Some(&castalia_membership_program(AUTHORITY))
    );
}

#[test]
fn checked_deployment_is_idempotent_but_refuses_descriptor_conflicts() {
    let factory = castalia_membership_factory(AUTHORITY).unwrap();
    let mut registry = FactoryRegistry::default();
    assert_eq!(
        factory.deploy_checked(&mut registry),
        Ok(factory.factory_vk())
    );
    assert_eq!(
        factory.deploy_checked(&mut registry),
        Ok(factory.factory_vk())
    );
    assert_eq!(
        registry.full_child_program(&factory.factory_vk()),
        Some(&castalia_membership_program(AUTHORITY)),
        "checked deployment must bind the exact method-dispatched child program"
    );

    let mut conflicting = registry.get(&factory.factory_vk()).unwrap().clone();
    conflicting.creation_budget = Some(1);
    let mut poisoned = FactoryRegistry::default();
    poisoned.deploy(conflicting);
    assert_eq!(
        factory.deploy_checked(&mut poisoned),
        Err(MembershipBirthError::FactoryDeploymentMismatch)
    );
}
