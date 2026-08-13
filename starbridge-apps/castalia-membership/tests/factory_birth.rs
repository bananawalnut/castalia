use dregg_app_framework::{AgentCipherclerk, AppCipherclerk, Effect, EmbeddedExecutor};
use dregg_types::CellId;
use starbridge_castalia_membership::{
    CHANGED_AT_SLOT, CastaliaMemberApplicationV1, GENERATION_SLOT, MembershipStatus, STATUS_SLOT,
    castalia_membership_factory, castalia_membership_program, field_from_u64,
    membership_birth_token_id,
};

const AUTHORITY_SEED: [u8; 32] = [0x41; 32];
const OWNER: [u8; 32] = [0x52; 32];

fn app(
    factory: &starbridge_castalia_membership::CastaliaMembershipFactory,
) -> CastaliaMemberApplicationV1 {
    CastaliaMemberApplicationV1 {
        factory_id: factory.factory_vk(),
        program_id: factory.child_program_vk(),
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
fn factory_birth_installs_and_enforces_exact_method_dispatched_program() {
    let cclerk = AppCipherclerk::new(
        AgentCipherclerk::from_key_bytes(AUTHORITY_SEED.into()),
        [0x62; 32],
    );
    let authority = cclerk.public_key().0;
    let factory = castalia_membership_factory(authority).unwrap();
    let application = app(&factory);
    let params = factory.creation_params(&application).unwrap();
    let exec = EmbeddedExecutor::new(&cclerk, "default");

    exec.deploy_factory_with_full_child_program_v2(
        factory.descriptor().clone(),
        castalia_membership_program(authority),
        factory.program_vk_recipe(),
    )
    .expect("checked full-program factory deploys");

    exec.with_ledger_mut(|ledger| {
        ledger
            .get_mut(&cclerk.cell_id())
            .unwrap()
            .state
            .set_balance(100_000_000);
    });
    let token = membership_birth_token_id(factory.factory_vk(), application.commitment(), 7);
    let birth = cclerk.create_from_factory(factory.factory_vk(), authority, token, params);
    exec.submit_turn(&birth)
        .expect("canonical membership birth commits");
    let member = CellId::derive_raw(&authority, &token);

    let installed = exec.with_ledger_mut(|ledger| ledger.get(&member).unwrap().program.clone());
    assert_eq!(installed, castalia_membership_program(authority));

    let wrong_method = cclerk.make_action(
        member,
        "invented",
        vec![
            Effect::SetField {
                cell: member,
                index: STATUS_SLOT as usize,
                value: field_from_u64(MembershipStatus::Active as u64),
            },
            Effect::SetField {
                cell: member,
                index: GENERATION_SLOT as usize,
                value: field_from_u64(1),
            },
            Effect::SetField {
                cell: member,
                index: CHANGED_AT_SLOT as usize,
                value: field_from_u64(application.created_at + 1),
            },
        ],
    );
    exec.submit_action(&cclerk, wrong_method)
        .expect_err("factory-born program must default-deny an unknown lifecycle method");
}
