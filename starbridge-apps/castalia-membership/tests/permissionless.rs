use dregg_app_framework::{AgentCipherclerk, AppCipherclerk, EmbeddedExecutor};
use serde_json::Value;
use sha2::{Digest, Sha256};
use starbridge_castalia_membership::{
    CASTALIA_PERMISSIONLESS_ACTIVE, CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION,
    CASTALIA_PERMISSIONLESS_POLICY, MAGIC_CASTMEM2, permissionless_membership_cell_id,
    permissionless_membership_factory, permissionless_membership_initial_fields,
    permissionless_membership_token_id, validate_permissionless_membership_cell,
};

const RELAY_SEED: [u8; 32] = [0x31; 32];
const MEMBER_SEED: [u8; 32] = [0x52; 32];

fn hex32(value: [u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn public_factory_birth_is_member_owned_active_and_idempotently_addressed() {
    let relay = AppCipherclerk::new(
        AgentCipherclerk::from_key_bytes(RELAY_SEED.into()),
        [0x62; 32],
    );
    let member = AgentCipherclerk::from_key_bytes(MEMBER_SEED.into());
    let owner = member.public_key().0;
    let factory = permissionless_membership_factory();
    let executor = EmbeddedExecutor::new(&relay, "default");
    executor
        .deploy_factory_with_full_child_program_v2(
            factory.descriptor().clone(),
            starbridge_castalia_membership::permissionless_membership_program(),
            factory.program_vk_recipe(),
        )
        .expect("public factory deploys");
    executor.with_ledger_mut(|ledger| {
        ledger
            .get_mut(&relay.cell_id())
            .expect("relay cell")
            .state
            .set_balance(100_000_000);
    });

    let birth = relay.create_from_factory(
        factory.factory_vk(),
        owner,
        permissionless_membership_token_id(),
        factory.creation_params(owner).expect("canonical params"),
    );
    executor
        .submit_turn(&birth)
        .expect("membership birth commits");

    let membership_id = permissionless_membership_cell_id(owner);
    executor.with_ledger_mut(|ledger| {
        let membership = ledger.get(&membership_id).expect("membership cell");
        validate_permissionless_membership_cell(membership, owner)
            .expect("exact public membership");
        assert_eq!(membership.public_key(), &owner);
        assert_eq!(membership.id(), membership_id);
    });
}

#[test]
fn public_membership_state_is_exact_active_v2_without_application_fields() {
    let fields = permissionless_membership_initial_fields();
    assert_eq!(fields.len(), 16);
    assert_eq!(fields[0], (0, MAGIC_CASTMEM2));
    assert_eq!(
        fields[1],
        (1, CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION)
    );
    assert_eq!(fields[2], (2, CASTALIA_PERMISSIONLESS_POLICY));
    assert_eq!(fields[12], (12, CASTALIA_PERMISSIONLESS_ACTIVE));
    assert_eq!(fields[13], (13, 0));
    assert!(fields[3..12].iter().all(|(_, value)| *value == 0));
    assert!(fields[14..].iter().all(|(_, value)| *value == 0));
}

#[test]
fn zero_owner_and_noncanonical_params_are_rejected() {
    let factory = permissionless_membership_factory();
    assert!(factory.creation_params([0; 32]).is_err());

    let owner = [0x52; 32];
    let mut altered = factory.creation_params(owner).expect("canonical params");
    altered.initial_fields[12].1 = 0;
    assert!(factory.validate_birth(owner, &altered).is_err());
}

#[test]
fn permissionless_contract_identifiers_are_stable() {
    let owner = [0x71; 32];
    assert_eq!(
        hex32(starbridge_castalia_membership::permissionless_membership_factory_vk()),
        "7ad3af1ba0e83ad560a881780295706073c1a0c9fe8656310051f62444903554"
    );
    assert_eq!(
        hex32(starbridge_castalia_membership::permissionless_membership_child_program_vk()),
        "6c37adae385c40894127e766deb9aff54e4cd01b0ccf01aff1ac7c12e24441fd"
    );
    assert_eq!(
        hex32(permissionless_membership_token_id()),
        "7f66eec85e99cd49ef3c8d733b8c489defe0a721f03fb2c3dd4bea04b1710d1f"
    );
    assert_eq!(
        hex32(permissionless_membership_cell_id(owner).0),
        "e4eea3e7352a5c8591508e880ce095421e472946dd3c1936e3efa05870447522"
    );
}

#[test]
fn canonical_vector_and_checksum_match_the_live_contract() {
    const VECTOR: &str =
        include_str!("../../../docs/vectors/castalia-permissionless-membership-v2.vector.json");
    const CHECKSUM: &str =
        include_str!("../../../docs/vectors/castalia-permissionless-membership-v2.vector.sha256");
    let digest = format!("{:x}", Sha256::digest(VECTOR.as_bytes()));
    assert_eq!(digest, CHECKSUM.trim());

    let value: Value = serde_json::from_str(VECTOR).expect("canonical vector JSON");
    let owner = [0x71; 32];
    assert_eq!(
        value["factoryId"],
        hex32(starbridge_castalia_membership::permissionless_membership_factory_vk())
    );
    assert_eq!(
        value["programId"],
        hex32(starbridge_castalia_membership::permissionless_membership_child_program_vk())
    );
    assert_eq!(
        value["tokenId"],
        hex32(permissionless_membership_token_id())
    );
    assert_eq!(
        value["membershipCellId"],
        hex32(permissionless_membership_cell_id(owner).0)
    );
}
