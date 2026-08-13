use starbridge_castalia_membership::{
    CASTALIA_MEMBERSHIP_SCHEMA_VERSION, MAGIC_CASTMEM1, MembershipStatus,
    castalia_membership_child_program_vk, castalia_membership_factory_vk,
    membership_birth_token_id, membership_cell_id,
};

const VECTOR: &str = include_str!("../vectors/castalia-membership-application-v1.json");
const AUTHORITY: [u8; 32] = [0x41; 32];
const OWNER: [u8; 32] = [0x52; 32];
const OFFICIAL_CELL: [u8; 32] = [0x22; 32];

fn value_start<'a>(json: &'a str, key: &str) -> &'a str {
    let needle = format!("\"{key}\"");
    let mut matches = json.match_indices(&needle);
    let (key_offset, _) = matches
        .next()
        .unwrap_or_else(|| panic!("missing key {key}"));
    assert!(matches.next().is_none(), "duplicate key {key}");
    let rest = &json[key_offset + needle.len()..];
    let colon = rest
        .find(':')
        .unwrap_or_else(|| panic!("missing colon for {key}"));
    rest[colon + 1..].trim_start()
}

fn string_value(json: &str, key: &str) -> String {
    let rest = value_start(json, key);
    assert!(rest.starts_with('"'), "{key} is not a JSON string");
    let end = rest[1..]
        .find('"')
        .unwrap_or_else(|| panic!("unterminated string for {key}"));
    let value = &rest[1..end + 1];
    assert!(!value.contains('\\'), "escaped strings are not accepted");
    value.to_owned()
}

fn u64_value(json: &str, key: &str) -> u64 {
    let needle = format!("\"{key}\"");
    let key_offset = json
        .find(&needle)
        .unwrap_or_else(|| panic!("missing key {key}"));
    let rest = &json[key_offset + needle.len()..];
    let colon = rest
        .find(':')
        .unwrap_or_else(|| panic!("missing colon for {key}"));
    let rest = rest[colon + 1..].trim_start();
    let end = rest
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(rest.len());
    assert!(end > 0, "{key} is not an unsigned integer");
    rest[..end].parse().unwrap()
}

fn hex_bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "hex must have an even length");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(text, 16).unwrap_or_else(|_| panic!("invalid hex byte {text}"))
        })
        .collect()
}

fn bytes32(value: &str) -> [u8; 32] {
    hex_bytes(value).try_into().expect("expected 32-byte hex")
}

fn canonical_bytes(factory_id: [u8; 32], child_program_vk: [u8; 32], nonce: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"castalia/member-application/v1\0");
    bytes.extend_from_slice(&factory_id);
    bytes.extend_from_slice(&child_program_vk);
    bytes.extend_from_slice(&OFFICIAL_CELL);
    bytes.extend_from_slice(&OWNER);
    for value in [7, 3, nonce, 2, 840, 5, 1_700_000_000] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn initial_fields(json: &str) -> Vec<(u64, u64)> {
    let rest = value_start(json, "initialFields");
    let end = rest.find(']').expect("unterminated initialFields array");
    let array = &rest[..=end];
    let mut fields = Vec::new();
    let mut cursor = array;
    while let Some(index_at) = cursor.find("\"index\"") {
        cursor = &cursor[index_at..];
        let index = u64_value(cursor, "index");
        let value_at = cursor.find("\"value\"").expect("field missing value");
        cursor = &cursor[value_at..];
        let value = u64_value(cursor, "value");
        fields.push((index, value));
        cursor = &cursor["\"value\"".len()..];
    }
    fields
}

#[test]
fn application_factory_vector_is_independently_verifiable() {
    assert_eq!(
        string_value(VECTOR, "schema"),
        "castalia-membership-application-v1"
    );
    assert_eq!(bytes32(&string_value(VECTOR, "authority")), AUTHORITY);
    assert_eq!(bytes32(&string_value(VECTOR, "ownerPubkey")), OWNER);
    assert_eq!(
        bytes32(&string_value(VECTOR, "officialDreggCellId")),
        OFFICIAL_CELL
    );
    assert_eq!(u64_value(VECTOR, "applicationKind"), 7);
    assert_eq!(u64_value(VECTOR, "applicationVersion"), 3);
    assert_eq!(u64_value(VECTOR, "applicationNonce"), 99);
    assert_eq!(u64_value(VECTOR, "membershipClass"), 2);
    assert_eq!(u64_value(VECTOR, "jurisdictionCode"), 840);
    assert_eq!(u64_value(VECTOR, "applicationFlags"), 5);
    assert_eq!(u64_value(VECTOR, "createdAt"), 1_700_000_000);

    let factory_id = bytes32(&string_value(VECTOR, "factoryId"));
    let child_program_vk = bytes32(&string_value(VECTOR, "childProgramVk"));
    assert_eq!(factory_id, castalia_membership_factory_vk(AUTHORITY));
    assert_eq!(
        child_program_vk,
        castalia_membership_child_program_vk(AUTHORITY)
    );

    let canonical = canonical_bytes(factory_id, child_program_vk, 99);
    assert_eq!(
        hex_bytes(&string_value(VECTOR, "canonicalBytesHex")),
        canonical
    );
    let commitment = *blake3::hash(&canonical).as_bytes();
    assert_eq!(
        bytes32(&string_value(VECTOR, "applicationCommitment")),
        commitment
    );

    let limbs: Vec<u64> = commitment
        .chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    let expected_fields = vec![
        (0, MAGIC_CASTMEM1),
        (1, CASTALIA_MEMBERSHIP_SCHEMA_VERSION),
        (2, 7),
        (3, 3),
        (4, 99),
        (5, 2),
        (6, 840),
        (7, 5),
        (8, limbs[0]),
        (9, limbs[1]),
        (10, limbs[2]),
        (11, limbs[3]),
        (12, MembershipStatus::Pending as u64),
        (13, 0),
        (14, 1_700_000_000),
        (15, 1_700_000_000),
    ];
    assert_eq!(initial_fields(VECTOR), expected_fields);

    let birth_nonce = u64_value(VECTOR, "birthNonce");
    let mut birth_bytes = Vec::new();
    birth_bytes.extend_from_slice(b"castalia/membership-birth-token/v1\0");
    birth_bytes.extend_from_slice(&factory_id);
    birth_bytes.extend_from_slice(&commitment);
    birth_bytes.extend_from_slice(&birth_nonce.to_le_bytes());
    let birth_token = *blake3::hash(&birth_bytes).as_bytes();
    assert_eq!(
        bytes32(&string_value(VECTOR, "membershipBirthTokenId")),
        birth_token
    );
    assert_eq!(
        membership_birth_token_id(factory_id, commitment, birth_nonce),
        birth_token
    );

    let mut cell_material = Vec::with_capacity(64);
    cell_material.extend_from_slice(&AUTHORITY);
    cell_material.extend_from_slice(&birth_token);
    let cell_id = blake3::derive_key("dregg-cell-id-v1", &cell_material);
    assert_eq!(bytes32(&string_value(VECTOR, "membershipCellId")), cell_id);
    assert_eq!(
        membership_cell_id(AUTHORITY, factory_id, commitment, birth_nonce).as_bytes(),
        &cell_id
    );

    let tampered = canonical_bytes(factory_id, child_program_vk, 100);
    assert_ne!(blake3::hash(&tampered), blake3::hash(&canonical));
}
