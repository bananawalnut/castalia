//! RED contract for Castalia #28 C01.
//!
//! A3a is validation-only. These tests deliberately name the production schema and
//! inert-gate API that later commits must provide; at C01 the module is absent, so
//! this integration target must fail to compile specifically on that missing API.

use ed25519_dalek::{Signer, SigningKey};

use dregg_turn::authority_lifecycle::{
    AUTHORITY_LIFECYCLE_SCHEMA_VERSION_V1, AuthorityLifecycleActionDataV1,
    AuthorityLifecycleActionV1, AuthorityLifecycleBoundaryCounters,
    AuthorityLifecycleCapabilityMarker, AuthorityLifecycleCurrentCellV1,
    AuthorityLifecycleGateError, AuthorityLifecycleInertGate, AuthorityLifecycleKindV1,
    AuthorityLifecyclePreconditionError, AuthorityLifecycleRegistryViewV1,
    AuthorityLifecycleSignatureError, AuthorityLifecycleStatusV1, AuthorityLifecycleTargetWireV1,
    AuthorityLifecycleTransitionV1, AuthorityLifecycleWireError, GenericAuthorityEffectV1,
    ReservedAuthorityIdentityV1, decode_authority_lifecycle_transition_v1,
    reject_reserved_authority_generic_effect, validate_authority_lifecycle_preconditions_v1,
    verify_authority_lifecycle_signatures_v1,
};

const REGISTRY_ID: [u8; 32] = [0x11; 32];
const RECEIVER_AUDIENCE: &[u8] = b"dregg://castalia";
const ROOT_COMMITMENT: [u8; 32] = [0x22; 32];
const TARGET_COMMITMENT: [u8; 32] = [0x33; 32];
const NEW_TARGET_COMMITMENT: [u8; 32] = [0x44; 32];
const NEW_ROOT_COMMITMENT: [u8; 32] = [0x55; 32];
const TRANSITION_ID: [u8; 32] = [0x66; 32];
const TARGET_CELL_ID: [u8; 32] = [0x77; 32];
const VALUE_DIGEST: [u8; 32] = [0x88; 32];
const REGISTRY_EPOCH: u64 = 7;
const ROOT_NONCE: u64 = 11;
const TARGET_NONCE: u64 = 13;

fn current_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[0xA1; 32])
}

fn next_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[0xB2; 32])
}

fn target(kind: AuthorityLifecycleKindV1) -> AuthorityLifecycleTargetWireV1 {
    AuthorityLifecycleTargetWireV1 {
        kind: kind.wire_code(),
        value_digest: VALUE_DIGEST,
        cell_id: TARGET_CELL_ID,
        old_nonce: TARGET_NONCE,
        old_commitment: TARGET_COMMITMENT,
        new_status: AuthorityLifecycleStatusV1::Active.wire_code(),
        new_epoch: REGISTRY_EPOCH,
        new_generation: TARGET_NONCE + 1,
        new_commitment: NEW_TARGET_COMMITMENT,
        superseded_reference: [0u8; 32],
    }
}

fn unsigned_transition(
    action: AuthorityLifecycleActionV1,
    action_data: AuthorityLifecycleActionDataV1,
    kind: AuthorityLifecycleKindV1,
) -> AuthorityLifecycleTransitionV1 {
    AuthorityLifecycleTransitionV1 {
        schema_version: AUTHORITY_LIFECYCLE_SCHEMA_VERSION_V1,
        registry_id: REGISTRY_ID,
        receiver_audience: RECEIVER_AUDIENCE.to_vec(),
        action: action.wire_code(),
        action_data,
        targets: vec![target(kind)],
        old_registry_epoch: REGISTRY_EPOCH,
        old_root_nonce: ROOT_NONCE,
        old_root_commitment: ROOT_COMMITMENT,
        new_root_nonce: ROOT_NONCE + 1,
        new_root_commitment: NEW_ROOT_COMMITMENT,
        transition_id: TRANSITION_ID,
        registry_signature: [0u8; 64],
        next_registry_signature: None,
    }
}

fn signed(mut transition: AuthorityLifecycleTransitionV1) -> AuthorityLifecycleTransitionV1 {
    transition.registry_signature = current_signing_key()
        .sign(&transition.canonical_signing_bytes())
        .to_bytes();
    transition
}

fn canonical_vectors() -> Vec<AuthorityLifecycleTransitionV1> {
    let issuer_id =
        b"dregg-issuer:blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_vec();
    let next_issuer_id =
        b"dregg-issuer:blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_vec();

    let add = unsigned_transition(
        AuthorityLifecycleActionV1::AddIssuerKey,
        AuthorityLifecycleActionDataV1::AddIssuerKey {
            issuer_key_id: issuer_id.clone(),
            public_key: [0x01; 32],
        },
        AuthorityLifecycleKindV1::Issuer,
    );
    let rotate = unsigned_transition(
        AuthorityLifecycleActionV1::RotateIssuerKey,
        AuthorityLifecycleActionDataV1::RotateIssuerKey {
            current_issuer_key_id: issuer_id.clone(),
            next_issuer_key_id: next_issuer_id,
            next_public_key: [0x02; 32],
        },
        AuthorityLifecycleKindV1::Issuer,
    );
    let revoke = unsigned_transition(
        AuthorityLifecycleActionV1::RevokeIssuerKey,
        AuthorityLifecycleActionDataV1::RevokeIssuerKey {
            issuer_key_id: issuer_id,
        },
        AuthorityLifecycleKindV1::Issuer,
    );
    let root = unsigned_transition(
        AuthorityLifecycleActionV1::SetAuthorityRootLifecycle,
        AuthorityLifecycleActionDataV1::SetAuthorityRootLifecycle,
        AuthorityLifecycleKindV1::AuthorityRoot,
    );
    let namespace = unsigned_transition(
        AuthorityLifecycleActionV1::SetNamespaceLifecycle,
        AuthorityLifecycleActionDataV1::SetNamespaceLifecycle,
        AuthorityLifecycleKindV1::Namespace,
    );
    let resource = unsigned_transition(
        AuthorityLifecycleActionV1::SetResourceLifecycle,
        AuthorityLifecycleActionDataV1::SetResourceLifecycle,
        AuthorityLifecycleKindV1::Resource,
    );
    let tail = unsigned_transition(
        AuthorityLifecycleActionV1::RevokeCredentialTailDigest,
        AuthorityLifecycleActionDataV1::RevokeCredentialTailDigest,
        AuthorityLifecycleKindV1::CredentialTail,
    );
    let mut registry_rotation = unsigned_transition(
        AuthorityLifecycleActionV1::RotateRegistrySigningKeyEpoch,
        AuthorityLifecycleActionDataV1::RotateRegistrySigningKeyEpoch {
            next_epoch: REGISTRY_EPOCH + 1,
            next_public_key: next_signing_key().verifying_key().to_bytes(),
        },
        AuthorityLifecycleKindV1::AuthorityRoot,
    );
    registry_rotation.registry_signature = current_signing_key()
        .sign(&registry_rotation.canonical_signing_bytes())
        .to_bytes();
    registry_rotation.next_registry_signature = Some(
        next_signing_key()
            .sign(&registry_rotation.canonical_signing_bytes())
            .to_bytes(),
    );

    vec![
        signed(add),
        signed(rotate),
        signed(revoke),
        signed(root),
        signed(namespace),
        signed(resource),
        signed(tail),
        registry_rotation,
    ]
}

fn registry_view() -> AuthorityLifecycleRegistryViewV1 {
    AuthorityLifecycleRegistryViewV1 {
        registry_id: REGISTRY_ID,
        receiver_audience: RECEIVER_AUDIENCE.to_vec(),
        registry_epoch: REGISTRY_EPOCH,
        registry_signing_key: current_signing_key().verifying_key().to_bytes(),
        root_nonce: ROOT_NONCE,
        root_commitment: ROOT_COMMITMENT,
        cells: vec![AuthorityLifecycleCurrentCellV1 {
            cell_id: TARGET_CELL_ID,
            nonce: TARGET_NONCE,
            commitment: TARGET_COMMITMENT,
        }],
    }
}

#[test]
fn canonical_transition_vectors_have_stable_strict_wire_roundtrips() {
    for transition in canonical_vectors() {
        let bytes = transition
            .to_wire_bytes()
            .expect("canonical vector encodes");
        let decoded =
            decode_authority_lifecycle_transition_v1(&bytes).expect("canonical vector decodes");
        assert_eq!(decoded, transition);
        assert_eq!(
            decoded.to_wire_bytes().expect("decoded vector re-encodes"),
            bytes,
            "canonical wire encoding must be byte-stable"
        );
    }
}

#[test]
fn malformed_and_unknown_wire_values_reject_closed() {
    let canonical = canonical_vectors().remove(0);

    let mut unknown_version = canonical.clone();
    unknown_version.schema_version = AUTHORITY_LIFECYCLE_SCHEMA_VERSION_V1 + 1;
    assert_eq!(
        decode_authority_lifecycle_transition_v1(
            &unknown_version
                .to_wire_bytes()
                .expect("raw version vector encodes")
        ),
        Err(AuthorityLifecycleWireError::UnknownVersion)
    );

    let mut unknown_action = canonical.clone();
    unknown_action.action = u8::MAX;
    assert_eq!(
        decode_authority_lifecycle_transition_v1(
            &unknown_action
                .to_wire_bytes()
                .expect("raw action vector encodes")
        ),
        Err(AuthorityLifecycleWireError::UnknownAction)
    );

    for status in [0, u8::MAX] {
        let mut unknown_status = canonical.clone();
        unknown_status.targets[0].new_status = status;
        assert_eq!(
            decode_authority_lifecycle_transition_v1(
                &unknown_status
                    .to_wire_bytes()
                    .expect("raw status vector encodes")
            ),
            Err(AuthorityLifecycleWireError::UnknownStatus)
        );
    }

    let bytes = canonical.to_wire_bytes().expect("canonical vector encodes");
    assert_eq!(
        decode_authority_lifecycle_transition_v1(&bytes[..bytes.len() - 1]),
        Err(AuthorityLifecycleWireError::Malformed)
    );
    let mut trailing = bytes;
    trailing.push(0);
    assert_eq!(
        decode_authority_lifecycle_transition_v1(&trailing),
        Err(AuthorityLifecycleWireError::TrailingData)
    );
}

#[test]
fn stale_registry_root_and_target_epoch_nonce_preconditions_reject() {
    let canonical = canonical_vectors().remove(0);
    let view = registry_view();

    let mut stale_registry_epoch = canonical.clone();
    stale_registry_epoch.old_registry_epoch -= 1;
    assert_eq!(
        validate_authority_lifecycle_preconditions_v1(&stale_registry_epoch, &view),
        Err(AuthorityLifecyclePreconditionError::StaleRegistryEpoch)
    );

    let mut stale_root_nonce = canonical.clone();
    stale_root_nonce.old_root_nonce -= 1;
    assert_eq!(
        validate_authority_lifecycle_preconditions_v1(&stale_root_nonce, &view),
        Err(AuthorityLifecyclePreconditionError::StaleRootNonce)
    );

    let mut stale_target_nonce = canonical;
    stale_target_nonce.targets[0].old_nonce -= 1;
    assert_eq!(
        validate_authority_lifecycle_preconditions_v1(&stale_target_nonce, &view),
        Err(AuthorityLifecyclePreconditionError::StaleTargetNonce)
    );
}

#[test]
fn registry_and_next_key_signature_substitution_rejects() {
    let canonical = canonical_vectors().remove(3);
    assert_eq!(
        verify_authority_lifecycle_signatures_v1(
            &canonical,
            &current_signing_key().verifying_key().to_bytes()
        ),
        Ok(())
    );

    let mut substituted = canonical;
    substituted.transition_id[0] ^= 1;
    assert_eq!(
        verify_authority_lifecycle_signatures_v1(
            &substituted,
            &current_signing_key().verifying_key().to_bytes()
        ),
        Err(AuthorityLifecycleSignatureError::InvalidRegistrySignature)
    );

    let rotation = canonical_vectors().remove(7);
    assert_eq!(
        verify_authority_lifecycle_signatures_v1(
            &rotation,
            &current_signing_key().verifying_key().to_bytes()
        ),
        Ok(())
    );
    let mut substituted_next = rotation;
    let other = signed(unsigned_transition(
        AuthorityLifecycleActionV1::SetNamespaceLifecycle,
        AuthorityLifecycleActionDataV1::SetNamespaceLifecycle,
        AuthorityLifecycleKindV1::Namespace,
    ));
    substituted_next.next_registry_signature = Some(other.registry_signature);
    assert_eq!(
        verify_authority_lifecycle_signatures_v1(
            &substituted_next,
            &current_signing_key().verifying_key().to_bytes()
        ),
        Err(AuthorityLifecycleSignatureError::InvalidNextRegistrySignature)
    );
}

#[test]
fn every_generic_effect_attack_on_reserved_namespace_rejects() {
    let reserved = ReservedAuthorityIdentityV1 {
        cell_id: TARGET_CELL_ID,
        owner_public_key: [0xC3; 32],
        token_id: [0xD4; 32],
    };
    let attacks = [
        GenericAuthorityEffectV1::CreateCell {
            public_key: reserved.owner_public_key,
            token_id: reserved.token_id,
        },
        GenericAuthorityEffectV1::SetField {
            cell: reserved.cell_id,
        },
        GenericAuthorityEffectV1::SetFieldExt {
            cell: reserved.cell_id,
        },
        GenericAuthorityEffectV1::TransferOwner {
            cell: reserved.cell_id,
        },
        GenericAuthorityEffectV1::CreateCellFromFactory {
            public_key: reserved.owner_public_key,
            token_id: reserved.token_id,
        },
        GenericAuthorityEffectV1::SetVerificationKey {
            cell: reserved.cell_id,
        },
        GenericAuthorityEffectV1::CallerCrafted {
            cell: reserved.cell_id,
        },
    ];

    for attack in attacks {
        assert!(
            reject_reserved_authority_generic_effect(&attack, &reserved).is_err(),
            "generic effect {attack:?} must not target a reserved identity"
        );
    }
}

#[test]
fn a3a_gate_rejects_before_every_admission_and_durability_boundary() {
    let transition = canonical_vectors().remove(0);
    let mut counters = AuthorityLifecycleBoundaryCounters {
        admissions: 2,
        queue_insertions: 3,
        journal_writes: 5,
        store_writes: 7,
        receipts_created: 11,
        finalized_roots: 13,
        replay_publications: 17,
    };
    let before = counters.clone();
    let gate = AuthorityLifecycleInertGate::a3a_validation_only();

    let markers = [
        None,
        Some(AuthorityLifecycleCapabilityMarker::A3aValidationOnly),
        Some(AuthorityLifecycleCapabilityMarker::Disabled),
        Some(AuthorityLifecycleCapabilityMarker::Unknown {
            name: b"unknown".to_vec(),
            version: 1,
        }),
        Some(AuthorityLifecycleCapabilityMarker::A3bApplicationReplay { version: 2 }),
        Some(AuthorityLifecycleCapabilityMarker::A3bApplicationReplay { version: 1 }),
    ];

    for marker in markers {
        let result = gate.admit(&transition, marker.as_ref(), &mut counters);
        assert_eq!(result, Err(AuthorityLifecycleGateError::A3bInactive));
        assert_eq!(
            counters, before,
            "rejection must occur before admission, queue, journal/store persistence, receipt, finalization, and replay publication"
        );
    }
}
