//! Castalia membership composition and public self-issuance.
//!
//! The legacy v1 factory remains available when genesis pins an institutional
//! authority. The v2 factory is public, stable across nodes, and always
//! deployed. `POST /api/castalia/memberships` verifies a Member Key signature,
//! creates that key's deterministic Active membership, and is retry-idempotent.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine as _;
use dregg_cell::FactoryRegistry;
use dregg_turn::{CallForest, Turn};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use starbridge_castalia_membership::{
    CASTALIA_PERMISSIONLESS_JOIN_DOMAIN, CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION,
    MembershipBirthError, PermissionlessMembershipError, castalia_membership_factory,
    permissionless_membership_cell_id, permissionless_membership_factory,
    permissionless_membership_token_id, validate_permissionless_membership_cell,
};

use crate::state::NodeState;

pub const GENESIS_AUTHORITY_FIELD: &str = "castalia_membership_authority";

/// Parse the optional Castalia authority from genesis.
///
/// Absence means the Castalia membership surface is not configured. A present
/// value must be exactly one non-zero 32-byte hex public key.
pub fn authority_from_genesis(genesis: &serde_json::Value) -> Result<Option<[u8; 32]>, String> {
    let Some(value) = genesis.get(GENESIS_AUTHORITY_FIELD) else {
        return Ok(None);
    };
    let encoded = value.as_str().ok_or_else(|| {
        format!("{GENESIS_AUTHORITY_FIELD} must be a 32-byte hexadecimal public key")
    })?;
    if encoded.len() != 64 {
        return Err(format!(
            "{GENESIS_AUTHORITY_FIELD} must be exactly 64 hexadecimal characters"
        ));
    }
    let mut authority = [0u8; 32];
    for (index, byte) in authority.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16).map_err(|_| {
            format!("{GENESIS_AUTHORITY_FIELD} must contain only hexadecimal characters")
        })?;
    }
    if authority == [0; 32] {
        return Err(format!(
            "{GENESIS_AUTHORITY_FIELD} must not be the zero key"
        ));
    }
    Ok(Some(authority))
}

/// Idempotently overlay the canonical descriptor, refusing a same-VK conflict.
pub fn deploy_checked(
    authority: [u8; 32],
    registry: &mut FactoryRegistry,
) -> Result<[u8; 32], MembershipBirthError> {
    castalia_membership_factory(authority)?.deploy_checked(registry)
}

/// Idempotently deploy the canonical permissionless v2 factory.
pub fn deploy_permissionless_checked(
    registry: &mut FactoryRegistry,
) -> Result<[u8; 32], PermissionlessMembershipError> {
    permissionless_membership_factory().deploy_checked(registry)
}

/// Public membership route, mounted outside the node's operator-auth layer.
pub fn routes() -> Router<NodeState> {
    Router::new().route("/api/castalia/memberships", post(post_membership))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JoinRequest {
    version: u64,
    owner_public_key: String,
    signature_suite: String,
    signature: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinResponse {
    version: u64,
    membership_cell_id: String,
    owner_public_key: String,
    state: &'static str,
    generation: u64,
    factory_id: String,
    program_id: String,
    state_commitment: String,
    receipt_hash: Option<String>,
    created: bool,
}

#[derive(Debug, Serialize)]
struct JoinErrorBody {
    error: &'static str,
    message: String,
}

#[derive(Debug)]
struct JoinError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl JoinError {
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn unavailable(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code,
            message: message.into(),
        }
    }

    fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            message: message.into(),
        }
    }
}

impl IntoResponse for JoinError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(JoinErrorBody {
                error: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}

fn decode_hex32(name: &'static str, encoded: &str) -> Result<[u8; 32], JoinError> {
    if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(JoinError::bad_request(
            "invalid_request",
            format!("{name} must be exactly 64 hexadecimal characters"),
        ));
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16).map_err(|_| {
            JoinError::bad_request("invalid_request", format!("{name} is not hexadecimal"))
        })?;
    }
    Ok(bytes)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn verify_join_request(request: &JoinRequest) -> Result<[u8; 32], JoinError> {
    if request.version != CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION {
        return Err(JoinError::bad_request(
            "unsupported_version",
            "permissionless Castalia membership requires version 2",
        ));
    }
    if request.signature_suite != "Ed25519" {
        return Err(JoinError::bad_request(
            "unsupported_signature_suite",
            "signatureSuite must be Ed25519",
        ));
    }
    let owner = decode_hex32("ownerPublicKey", &request.owner_public_key)?;
    if owner == [0; 32] {
        return Err(JoinError::bad_request(
            "invalid_owner",
            "ownerPublicKey must not be the zero key",
        ));
    }
    let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&request.signature)
        .map_err(|_| {
            JoinError::bad_request("invalid_signature", "signature must be unpadded base64url")
        })?;
    let signature_bytes: [u8; 64] = signature_bytes.try_into().map_err(|_| {
        JoinError::bad_request("invalid_signature", "Ed25519 signature must be 64 bytes")
    })?;
    let key = VerifyingKey::from_bytes(&owner).map_err(|_| {
        JoinError::bad_request("invalid_owner", "ownerPublicKey is not a valid Ed25519 key")
    })?;
    let mut message = Vec::with_capacity(CASTALIA_PERMISSIONLESS_JOIN_DOMAIN.len() + owner.len());
    message.extend_from_slice(CASTALIA_PERMISSIONLESS_JOIN_DOMAIN);
    message.extend_from_slice(&owner);
    key.verify_strict(&message, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| {
            JoinError::bad_request(
                "invalid_signature",
                "signature does not prove control of ownerPublicKey",
            )
        })?;
    Ok(owner)
}

fn membership_response(
    cell: &dregg_cell::Cell,
    owner: [u8; 32],
    receipt_hash: Option<[u8; 32]>,
    created: bool,
) -> Result<JoinResponse, JoinError> {
    validate_permissionless_membership_cell(cell, owner).map_err(|error| {
        JoinError::internal(
            "membership_integrity_error",
            format!("stored membership failed canonical v2 verification: {error}"),
        )
    })?;
    let factory = permissionless_membership_factory();
    Ok(JoinResponse {
        version: CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION,
        membership_cell_id: encode_hex(cell.id().as_bytes()),
        owner_public_key: encode_hex(&owner),
        state: "active",
        generation: 0,
        factory_id: encode_hex(&factory.factory_vk()),
        program_id: encode_hex(&factory.child_program_vk()),
        state_commitment: encode_hex(&cell.state_commitment()),
        receipt_hash: receipt_hash.map(|hash| encode_hex(&hash)),
        created,
    })
}

/// Build and admission-check the exact operator-signed turn that consensus
/// will finalize. The live ledger is never mutated here: finalization remains
/// the sole state and durable-commit writer, including in a one-node devnet.
fn prepare_membership_turn(
    s: &crate::state::NodeStateInner,
    operator: dregg_cell::CellId,
    owner: [u8; 32],
) -> Result<(Vec<u8>, [u8; 32]), JoinError> {
    let factory = permissionless_membership_factory();
    let params = factory.creation_params(owner).map_err(|error| {
        JoinError::bad_request(
            "invalid_membership_birth",
            format!("membership parameters were rejected: {error}"),
        )
    })?;
    let effect = dregg_turn::Effect::CreateCellFromFactory {
        factory_vk: factory.factory_vk(),
        owner_pubkey: owner,
        token_id: permissionless_membership_token_id(),
        params,
    };
    let federation_id = crate::executor_setup::federation_id_for_executor(s);
    let action = s
        .cclerk
        .make_action(operator, "castalia_join_v2", vec![effect], &federation_id);
    let mut call_forest = CallForest::new();
    call_forest.add_root(action);
    let previous_receipt_hash = s.cclerk.receipt_chain().last().map(|r| r.receipt_hash());
    let mut turn = Turn {
        agent: operator,
        nonce: s
            .ledger
            .get(&operator)
            .map(|cell| cell.state.nonce())
            .unwrap_or(0),
        fee: 0,
        memo: Some("castalia_join_v2".to_string()),
        valid_until: Some(i64::MAX / 2),
        call_forest,
        depends_on: vec![],
        previous_receipt_hash,
        conservation_proof: None,
        sovereign_witnesses: std::collections::HashMap::new(),
        execution_proof: None,
        execution_proof_cell: None,
        execution_proof_new_commitment: None,
        custom_program_proofs: None,
        effect_binding_proofs: Vec::new(),
        cross_effect_dependencies: Vec::new(),
        effect_witness_index_map: Vec::new(),
    };
    let executor = crate::executor_setup::new_submit_executor(s);
    turn.fee = executor.estimate_cost(&turn);
    crate::api::seed_executor_receipt_head(&executor, operator, previous_receipt_hash);

    // Admission is a dry run against an isolated ledger clone. The exact same
    // signed bytes are submitted below and re-executed by blocklace finality.
    let mut scratch = s.ledger.clone();
    match crate::executor_setup::execute_via_producer(
        &executor,
        &turn,
        &mut scratch,
        s.lean_producer_enabled,
    ) {
        dregg_turn::TurnResult::Committed { .. } => {}
        dregg_turn::TurnResult::Rejected { reason, .. } => {
            return Err(JoinError::unavailable(
                "membership_relay_rejected",
                format!("Dregg rejected the membership birth: {reason}"),
            ));
        }
        other => {
            return Err(JoinError::unavailable(
                "membership_relay_rejected",
                format!("membership turn did not commit during admission: {other:?}"),
            ));
        }
    }

    let turn_hash = turn.hash();
    let signed = s.cclerk.sign_turn(&turn);
    let bytes = postcard::to_stdvec(&signed).map_err(|error| {
        JoinError::internal(
            "membership_encoding_failed",
            format!("could not encode the signed membership turn: {error}"),
        )
    })?;
    Ok((bytes, turn_hash))
}

async fn post_membership(
    State(state): State<NodeState>,
    Json(request): Json<JoinRequest>,
) -> Result<Json<JoinResponse>, JoinError> {
    let owner = verify_join_request(&request)?;
    let membership_id = permissionless_membership_cell_id(owner);
    let s = state.write().await;

    if let Some(existing) = s.ledger.get(&membership_id) {
        let response = membership_response(existing, owner, None, false)?;
        return Ok(Json(response));
    }

    let blocklace = s.blocklace_handle.clone().ok_or_else(|| {
        JoinError::unavailable(
            "membership_consensus_unavailable",
            "membership issuance requires the node's finalization service",
        )
    })?;

    let operator = crate::executor_setup::local_agent_cell(&s);
    let operator_balance = s
        .ledger
        .get(&operator)
        .map(|cell| cell.state.balance())
        .unwrap_or(0);
    if operator_balance <= 0 {
        return Err(JoinError::unavailable(
            "membership_relay_unavailable",
            "this node's relay cell is not funded; initialize it from Castalia genesis",
        ));
    }

    let (turn_data, turn_hash) = prepare_membership_turn(&s, operator, owner)?;
    drop(s);

    // The blocklace is the one authoritative writer. In solo mode submission
    // produces an ordered block immediately; its finality worker then writes
    // the cell, attested root, and CommitRecord through one canonical path.
    blocklace.submit_turn(&state, turn_data).await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        {
            let s = state.read().await;
            if let Some(cell) = s.ledger.get(&membership_id) {
                return Ok(Json(membership_response(cell, owner, None, true)?));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(JoinError::unavailable(
                "membership_finalization_timeout",
                format!(
                    "membership turn {} was submitted but did not finalize before the response deadline",
                    encode_hex(&turn_hash)
                ),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dregg_cell::factory::FactoryDescriptor;
    use ed25519_dalek::{Signer, SigningKey};

    const AUTHORITY: [u8; 32] = [0x41; 32];

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn absent_authority_disables_membership_factory() {
        assert_eq!(authority_from_genesis(&serde_json::json!({})), Ok(None));
    }

    #[test]
    fn authority_is_strict_nonzero_hex32() {
        assert_eq!(
            authority_from_genesis(&serde_json::json!({
                GENESIS_AUTHORITY_FIELD: hex(&AUTHORITY)
            })),
            Ok(Some(AUTHORITY))
        );
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!(7),
            serde_json::json!("41"),
            serde_json::json!(hex(&[0; 32])),
            serde_json::json!(format!("{}00", hex(&AUTHORITY))),
        ] {
            let error = authority_from_genesis(&serde_json::json!({
                GENESIS_AUTHORITY_FIELD: invalid
            }))
            .expect_err("present malformed authority must fail closed");
            assert!(error.contains(GENESIS_AUTHORITY_FIELD), "{error}");
        }
    }

    #[test]
    fn canonical_overlay_is_idempotent_and_refuses_conflict() {
        let mut registry = FactoryRegistry::new();
        let factory_vk = deploy_checked(AUTHORITY, &mut registry).expect("first deploy");
        assert_eq!(
            deploy_checked(AUTHORITY, &mut registry),
            Ok(factory_vk),
            "exact descriptor overlay must be restart-idempotent"
        );

        let canonical = registry
            .get(&factory_vk)
            .expect("canonical descriptor")
            .clone();
        let conflicting = FactoryDescriptor {
            creation_budget: canonical
                .creation_budget
                .map(|value| value.saturating_add(1))
                .or(Some(1)),
            ..canonical
        };
        let mut poisoned = FactoryRegistry::new();
        poisoned.deploy(conflicting);
        assert_eq!(
            deploy_checked(AUTHORITY, &mut poisoned),
            Err(MembershipBirthError::FactoryDeploymentMismatch)
        );
    }

    #[test]
    fn permissionless_overlay_is_stable_without_genesis_authority() {
        let mut registry = FactoryRegistry::new();
        let first = deploy_permissionless_checked(&mut registry).expect("first deploy");
        assert_eq!(deploy_permissionless_checked(&mut registry), Ok(first));
    }

    fn signed_join(seed: [u8; 32]) -> JoinRequest {
        let key = SigningKey::from_bytes(&seed);
        let owner = key.verifying_key().to_bytes();
        let mut message = CASTALIA_PERMISSIONLESS_JOIN_DOMAIN.to_vec();
        message.extend_from_slice(&owner);
        JoinRequest {
            version: 2,
            owner_public_key: encode_hex(&owner),
            signature_suite: "Ed25519".to_string(),
            signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(key.sign(&message).to_bytes()),
        }
    }

    #[test]
    fn join_signature_is_bound_to_the_member_key() {
        let request = signed_join([0x52; 32]);
        let owner = verify_join_request(&request).expect("valid member proof");
        assert_eq!(request.owner_public_key, encode_hex(&owner));

        let mut substituted = request;
        substituted.owner_public_key = encode_hex(
            &SigningKey::from_bytes(&[0x53; 32])
                .verifying_key()
                .to_bytes(),
        );
        assert!(verify_join_request(&substituted).is_err());
    }

    #[tokio::test]
    async fn signed_join_prepares_the_exact_valid_member_birth_turn() {
        let directory = tempfile::tempdir().expect("temporary node directory");
        let member = SigningKey::from_bytes(&[0x61; 32]);
        let owner = member.verifying_key().to_bytes();
        let membership_id = permissionless_membership_cell_id(owner);

        let state = NodeState::new(directory.path(), vec![]).expect("node state");
        let mut s = state.write().await;
        s.lean_producer_enabled = false;
        let operator_key = s.cclerk.public_key().0;
        let operator = dregg_cell::Cell::with_balance(
            operator_key,
            *blake3::hash(b"default").as_bytes(),
            100_000_000,
        );
        let operator_id = operator.id();
        assert_eq!(operator_id, crate::executor_setup::local_agent_cell(&s));
        s.ledger.insert_cell(operator).expect("fund relay cell");

        let (encoded, turn_hash) =
            prepare_membership_turn(&s, operator_id, owner).expect("canonical membership turn");
        let signed: dregg_sdk::SignedTurn =
            postcard::from_bytes(&encoded).expect("signed turn wire bytes");
        assert_eq!(signed.turn.hash(), turn_hash);
        assert!(signed.signer.verify(&turn_hash, &signed.signature));

        let executor = crate::executor_setup::new_submit_executor(&s);
        crate::api::seed_executor_receipt_head(
            &executor,
            operator_id,
            signed.turn.previous_receipt_hash,
        );
        assert!(matches!(
            crate::executor_setup::execute_via_producer(
                &executor,
                &signed.turn,
                &mut s.ledger,
                false,
            ),
            dregg_turn::TurnResult::Committed { .. }
        ));
        let cell = s
            .ledger
            .get(&membership_id)
            .expect("membership materialized");
        validate_permissionless_membership_cell(cell, owner).expect("exact v2 cell");
    }
}
