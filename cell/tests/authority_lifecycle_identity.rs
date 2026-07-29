//! Canonical authority lifecycle identity contract for Castalia #28 C02.

use dregg_cell::authority_lifecycle::{
    AUTHORITY_LIFECYCLE_KEY_DOMAIN_V1, AUTHORITY_LIFECYCLE_TOKEN_DOMAIN_V1,
    AUTHORITY_VALUE_DOMAIN_V1, AuthorityIdentityError, AuthorityLifecycleIdentityClaimV1,
    AuthorityLifecycleIdentityV1, AuthorityLifecycleKindV1, AuthorityScopeV1, AuthorityValueKindV1,
    IssuerLifecycleIdentityV1, IssuerLifecycleRecordInputV1, validate_issuer_lifecycle_records_v1,
    validate_reserved_authority_identity_claims_v1, world_scoped_value_digest_v1,
};

const REGISTRY_ID: [u8; 32] = [0x11; 32];
const RECEIVER_AUDIENCE: &[u8] = b"dregg://castalia";

fn scope() -> AuthorityScopeV1 {
    AuthorityScopeV1::new(&REGISTRY_ID, RECEIVER_AUDIENCE).expect("canonical authority scope")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn exact_domains_world_digest_key_bytes_commitment_and_token_vector_are_stable() {
    assert_eq!(AUTHORITY_VALUE_DOMAIN_V1, "castalia-authority-value-v1");
    assert_eq!(
        AUTHORITY_LIFECYCLE_KEY_DOMAIN_V1,
        "castalia-authority-lifecycle-key-v1"
    );
    assert_eq!(
        AUTHORITY_LIFECYCLE_TOKEN_DOMAIN_V1,
        "castalia-authority-lifecycle-token-v1"
    );

    let value_digest = world_scoped_value_digest_v1(
        &scope(),
        AuthorityValueKindV1::Resource,
        b"dregg://castalia/gallery/artwork-7",
    );
    assert_eq!(
        hex(&value_digest),
        "5c4f36cf692d2ecb2d093110de6ee8745e9dd9dd86d6fd2103f8ffc694133c20"
    );

    let identity = AuthorityLifecycleIdentityV1::derive(
        scope(),
        AuthorityLifecycleKindV1::Resource,
        value_digest,
    );
    assert_eq!(
        hex(identity.canonical_key_bytes()),
        concat!(
            "1111111111111111111111111111111111111111111111111111111111111111",
            "00",
            "64726567673a2f2f63617374616c6961",
            "00",
            "7265736f75726365",
            "00",
            "5c4f36cf692d2ecb2d093110de6ee8745e9dd9dd86d6fd2103f8ffc694133c20"
        )
    );
    assert_eq!(
        hex(&identity.key_commitment()),
        "99d406b155abc016386cf7af942e8dc56bf8ae7f8d21726d525074ed51fbf3d2"
    );
    assert_eq!(
        hex(&identity.token_id()),
        "cd7037207dc69850836db14f89a43922c2a0c9db541143122a872bacde7308bd"
    );
}

#[test]
fn every_closed_lifecycle_kind_has_one_exact_lowercase_identity() {
    let expected = [
        (AuthorityLifecycleKindV1::Issuer, b"issuer".as_slice()),
        (
            AuthorityLifecycleKindV1::AuthorityRoot,
            b"authority-root".as_slice(),
        ),
        (AuthorityLifecycleKindV1::Namespace, b"namespace".as_slice()),
        (AuthorityLifecycleKindV1::Resource, b"resource".as_slice()),
        (
            AuthorityLifecycleKindV1::CredentialTail,
            b"credential-tail".as_slice(),
        ),
    ];
    assert_eq!(AuthorityLifecycleKindV1::ALL.len(), expected.len());

    let digest = [0xA5; 32];
    let mut token_ids = std::collections::BTreeSet::new();
    for (kind, bytes) in expected {
        assert_eq!(kind.as_bytes(), bytes);
        assert_eq!(AuthorityLifecycleKindV1::try_from_bytes(bytes), Ok(kind));
        assert!(
            token_ids
                .insert(AuthorityLifecycleIdentityV1::derive(scope(), kind, digest).token_id())
        );
    }

    for unknown_or_alias in [
        b"".as_slice(),
        b"Issuer".as_slice(),
        b"authority_root".as_slice(),
        b"credential-tail\0".as_slice(),
        b"unknown".as_slice(),
    ] {
        assert_eq!(
            AuthorityLifecycleKindV1::try_from_bytes(unknown_or_alias),
            Err(AuthorityIdentityError::UnknownLifecycleKind)
        );
    }
}

#[test]
fn registry_and_receiver_audience_must_already_be_stable_and_canonical() {
    assert_eq!(
        AuthorityScopeV1::new(&REGISTRY_ID[..31], RECEIVER_AUDIENCE),
        Err(AuthorityIdentityError::MalformedRegistryId)
    );
    assert_eq!(
        AuthorityScopeV1::new(&[0x11; 33], RECEIVER_AUDIENCE),
        Err(AuthorityIdentityError::MalformedRegistryId)
    );

    for malformed_or_alias in [
        b"".as_slice(),
        b"dregg://".as_slice(),
        b"DREGG://castalia".as_slice(),
        b"dregg://Castalia".as_slice(),
        b"dregg://castalia/".as_slice(),
        b"dregg://castalia%2f".as_slice(),
        b"dregg://castalia\0".as_slice(),
        b" dregg://castalia".as_slice(),
    ] {
        assert_eq!(
            AuthorityScopeV1::new(&REGISTRY_ID, malformed_or_alias),
            Err(AuthorityIdentityError::MalformedReceiverAudience),
            "audience alias/malformed value must reject: {malformed_or_alias:?}"
        );
    }
}

#[test]
fn value_kinds_are_closed_and_world_scope_changes_every_digest() {
    let expected = [
        (AuthorityValueKindV1::Subject, b"subject".as_slice()),
        (AuthorityValueKindV1::Operation, b"operation".as_slice()),
        (AuthorityValueKindV1::Resource, b"resource".as_slice()),
        (AuthorityValueKindV1::Issuer, b"issuer".as_slice()),
        (AuthorityValueKindV1::Root, b"root".as_slice()),
        (AuthorityValueKindV1::Namespace, b"namespace".as_slice()),
        (
            AuthorityValueKindV1::CredentialTail,
            b"credential-tail".as_slice(),
        ),
        (
            AuthorityValueKindV1::AuthorityRoot,
            b"authority-root".as_slice(),
        ),
    ];
    assert_eq!(AuthorityValueKindV1::ALL.len(), expected.len());
    for (kind, bytes) in expected {
        assert_eq!(kind.as_bytes(), bytes);
        assert_eq!(AuthorityValueKindV1::try_from_bytes(bytes), Ok(kind));
    }
    assert_eq!(
        AuthorityValueKindV1::try_from_bytes(b"Resource"),
        Err(AuthorityIdentityError::UnknownValueKind)
    );

    let original =
        world_scoped_value_digest_v1(&scope(), AuthorityValueKindV1::Resource, b"same-value");
    let other_registry = AuthorityScopeV1::new(&[0x22; 32], RECEIVER_AUDIENCE).unwrap();
    let other_audience = AuthorityScopeV1::new(&REGISTRY_ID, b"dregg://other").unwrap();
    assert_ne!(
        original,
        world_scoped_value_digest_v1(
            &other_registry,
            AuthorityValueKindV1::Resource,
            b"same-value"
        )
    );
    assert_ne!(
        original,
        world_scoped_value_digest_v1(
            &other_audience,
            AuthorityValueKindV1::Resource,
            b"same-value"
        )
    );
}

#[test]
fn issuer_identity_recomputes_canonical_id_from_exact_ed25519_public_key() {
    let issuer = IssuerLifecycleIdentityV1::derive(scope(), &[0x42; 32]).unwrap();
    assert_eq!(
        issuer.issuer_key_id(),
        "dregg-issuer:blake3:7af6fbdcd24706fd84d5bc0d1571f90be1a1b226c6152c15bbbc2d8505bcb207"
    );
    assert_eq!(
        hex(&issuer.identity().token_id()),
        "12fcbec20f2a945f7fc810277206e2d1c163733e7331cca8b30ebe1d0f81f556"
    );
    assert_eq!(issuer.public_key(), &[0x42; 32]);
    assert_eq!(
        IssuerLifecycleIdentityV1::validate(
            scope(),
            issuer.issuer_key_id().as_bytes(),
            &[0x42; 32]
        ),
        Ok(issuer.clone())
    );

    assert_eq!(
        IssuerLifecycleIdentityV1::derive(scope(), &[0x42; 31]),
        Err(AuthorityIdentityError::MalformedIssuerPublicKey)
    );
    let uppercase = issuer.issuer_key_id().to_ascii_uppercase();
    assert_eq!(
        IssuerLifecycleIdentityV1::validate(scope(), uppercase.as_bytes(), &[0x42; 32]),
        Err(AuthorityIdentityError::NonCanonicalIssuerKeyId)
    );
    assert_eq!(
        IssuerLifecycleIdentityV1::validate(
            scope(),
            b"dregg-issuer:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &[0x42; 32]
        ),
        Err(AuthorityIdentityError::IssuerKeyIdMismatch)
    );
}

#[test]
fn reserved_recognition_is_pure_and_rejects_alias_duplicate_and_collision_claims() {
    let identity = AuthorityLifecycleIdentityV1::derive(
        scope(),
        AuthorityLifecycleKindV1::Namespace,
        [0x77; 32],
    );
    let canonical = AuthorityLifecycleIdentityClaimV1::from(&identity);
    assert_eq!(
        validate_reserved_authority_identity_claims_v1(std::slice::from_ref(&canonical)),
        Ok(vec![identity.clone()])
    );

    let mut key_alias = canonical.clone();
    key_alias.canonical_key_bytes.push(0);
    assert_eq!(
        validate_reserved_authority_identity_claims_v1(&[key_alias]),
        Err(AuthorityIdentityError::NonCanonicalLifecycleKey)
    );

    assert_eq!(
        validate_reserved_authority_identity_claims_v1(&[canonical.clone(), canonical.clone()]),
        Err(AuthorityIdentityError::DuplicateLifecycleIdentity)
    );

    let other = AuthorityLifecycleIdentityV1::derive(
        scope(),
        AuthorityLifecycleKindV1::Resource,
        [0x88; 32],
    );
    let mut colliding = AuthorityLifecycleIdentityClaimV1::from(&other);
    colliding.token_id = canonical.token_id;
    assert_eq!(
        validate_reserved_authority_identity_claims_v1(&[canonical, colliding]),
        Err(AuthorityIdentityError::LifecycleTokenCollision)
    );
}

#[test]
fn issuer_record_set_rejects_duplicate_ids_duplicate_keys_and_claimed_collisions() {
    let issuer_a = IssuerLifecycleIdentityV1::derive(scope(), &[0x42; 32]).unwrap();
    let issuer_b = IssuerLifecycleIdentityV1::derive(scope(), &[0x43; 32]).unwrap();
    let canonical = vec![
        IssuerLifecycleRecordInputV1::from(&issuer_a),
        IssuerLifecycleRecordInputV1::from(&issuer_b),
    ];
    assert_eq!(
        validate_issuer_lifecycle_records_v1(scope(), &canonical),
        Ok(vec![issuer_a.clone(), issuer_b.clone()])
    );

    assert_eq!(
        validate_issuer_lifecycle_records_v1(
            scope(),
            &[
                IssuerLifecycleRecordInputV1::from(&issuer_a),
                IssuerLifecycleRecordInputV1::from(&issuer_a),
            ]
        ),
        Err(AuthorityIdentityError::DuplicateIssuerKeyId)
    );

    let mut duplicate_key = IssuerLifecycleRecordInputV1::from(&issuer_b);
    duplicate_key.public_key = issuer_a.public_key().to_vec();
    assert_eq!(
        validate_issuer_lifecycle_records_v1(
            scope(),
            &[IssuerLifecycleRecordInputV1::from(&issuer_a), duplicate_key]
        ),
        Err(AuthorityIdentityError::DuplicateIssuerPublicKey)
    );

    let mut colliding_id = IssuerLifecycleRecordInputV1::from(&issuer_b);
    colliding_id.issuer_key_id = issuer_a.issuer_key_id().as_bytes().to_vec();
    assert_eq!(
        validate_issuer_lifecycle_records_v1(
            scope(),
            &[IssuerLifecycleRecordInputV1::from(&issuer_a), colliding_id]
        ),
        Err(AuthorityIdentityError::IssuerKeyIdCollision)
    );
}
