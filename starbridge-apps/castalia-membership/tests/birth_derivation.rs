use dregg_types::CellId;
use starbridge_castalia_membership::{membership_birth_token_id, membership_cell_id};

const AUTHORITY: [u8; 32] = [0x41; 32];
const FACTORY_ID: [u8; 32] = [0x61; 32];
const APPLICATION_COMMITMENT: [u8; 32] = [0xab; 32];

#[test]
fn birth_derivation_matches_the_d0_contract_vector() {
    let token = membership_birth_token_id(FACTORY_ID, APPLICATION_COMMITMENT, 7);
    assert_eq!(
        token,
        [
            0x81, 0x26, 0xc2, 0x37, 0xdf, 0x9b, 0xc1, 0x81, 0x0a, 0x0c, 0x93, 0xeb, 0xa5, 0x66,
            0x1d, 0x33, 0x6e, 0x1e, 0x34, 0xc3, 0x5a, 0xbb, 0xe6, 0xcc, 0x3d, 0xef, 0xce, 0x56,
            0x52, 0x50, 0xd3, 0xe3,
        ]
    );
    assert_eq!(
        membership_cell_id(AUTHORITY, FACTORY_ID, APPLICATION_COMMITMENT, 7),
        CellId::from_bytes([
            0xac, 0x43, 0xa3, 0xdd, 0x95, 0xa6, 0x5e, 0x24, 0xfa, 0xf7, 0x1f, 0xf9, 0x89, 0x9c,
            0x96, 0x85, 0x81, 0xab, 0x1a, 0xd3, 0x6a, 0x79, 0x86, 0xa4, 0xa2, 0xbe, 0x4d, 0x7d,
            0x59, 0xe2, 0x08, 0xfb,
        ])
    );
}

#[test]
fn durable_birth_nonce_changes_both_token_and_cell_identity() {
    assert_ne!(
        membership_birth_token_id(FACTORY_ID, APPLICATION_COMMITMENT, 7),
        membership_birth_token_id(FACTORY_ID, APPLICATION_COMMITMENT, 8)
    );
    assert_ne!(
        membership_cell_id(AUTHORITY, FACTORY_ID, APPLICATION_COMMITMENT, 7),
        membership_cell_id(AUTHORITY, FACTORY_ID, APPLICATION_COMMITMENT, 8)
    );
}
