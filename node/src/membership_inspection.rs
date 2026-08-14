//! Authenticated inspection of a canonical cell under a persisted hybrid finalization quorum.
//!
//! This module verifies finalized state evidence only. It does not prove current key possession,
//! admit a member, or establish a holder-bound session.

use dregg_cell::Cell;
use dregg_federation::frost::MlDsaPublicKey;
use dregg_persist::federation::StoredAttestedRoot;
use dregg_types::{FederationId, PublicKey};
use starbridge_castalia_membership::{
    CHANGED_AT_SLOT, CREATED_AT_SLOT, CastaliaMemberApplicationV1, GENERATION_SLOT,
    MembershipStatus, STATUS_SLOT, castalia_membership_program, field_from_u64, membership_cell_id,
    membership_creation_params, membership_initial_fields,
};

/// Maximum full-ledger leaves accepted by the bounded first-cut flat-root verifier.
pub const MAX_INSPECTION_LEAVES: usize = 65_536;
/// Maximum canonical serialized cell bytes accepted by the verifier.
pub const MAX_INSPECTION_CELL_BYTES: usize = 1_048_576;
/// Maximum exact serialized finalization artifact accepted before decoding.
pub const MAX_INSPECTION_ATTESTED_ROOT_BYTES: usize = 1_048_576;

#[derive(Clone, Debug)]
pub struct AuthenticatedCellInspection {
    /// Exact cell identifier requested by the caller.
    pub cell_id: [u8; 32],
    /// Exact canonical `postcard(Cell)` bytes committed by the leaf hash.
    pub cell_bytes: Vec<u8>,
    /// Full canonical sorted ledger leaf set used by the current flat-root scheme.
    pub leaves: Vec<([u8; 32], [u8; 32])>,
    /// Exact canonical `postcard(StoredAttestedRoot)` bytes, including hybrid quorum.
    pub attested_root_bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct MembershipInspectionPolicy {
    /// Caller-pinned BLAKE3 digest of the exact canonical `postcard(StoredAttestedRoot)` bytes.
    /// This binds envelope metadata not covered by the finalization-vote preimage (federation,
    /// height, timestamp, and threshold) and provides the caller's continuity trust root.
    pub trusted_attested_root_digest: [u8; 32],
    /// Independently pinned federation identity.
    pub federation_id: FederationId,
    /// Independently pinned Ed25519 committee roster.
    pub committee: Vec<PublicKey>,
    /// Independently pinned, index-aligned ML-DSA-65 committee roster.
    pub ml_dsa_committee: Vec<MlDsaPublicKey>,
    /// No-rollback floor retained by the caller.
    pub minimum_height: u64,
    /// Authority clock supplied by the caller.
    pub now_unix_seconds: i64,
    /// Maximum accepted age of the finalized root.
    pub maximum_age_seconds: u64,
    /// Maximum accepted future clock skew of the finalized root.
    pub maximum_future_skew_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipInspectionError {
    CellTooLarge,
    MalformedCell,
    AttestedRootTooLarge,
    MalformedAttestedRoot,
    AttestedRootPinMismatch,
    CellIdMismatch,
    InvalidLeaves,
    CellLeafMissing,
    LedgerRootMismatch,
    FederationMismatch,
    HeightRollback,
    StaleRoot,
    FutureRoot,
    InvalidFinalizationQuorum,
    MembershipAuthorityMismatch,
    MembershipProgramMismatch,
    MembershipFieldMismatch,
    MembershipStatusMismatch,
    MembershipTimestampInvalid,
}

impl std::fmt::Display for MembershipInspectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CellTooLarge => "authenticated cell bytes exceed the verifier limit",
            Self::MalformedCell => "authenticated cell bytes are malformed or noncanonical",
            Self::AttestedRootTooLarge => "attested-root bytes exceed the verifier limit",
            Self::MalformedAttestedRoot => "attested-root bytes are malformed or noncanonical",
            Self::AttestedRootPinMismatch => {
                "attested-root bytes do not match the caller-pinned continuity digest"
            }
            Self::CellIdMismatch => "authenticated cell identity does not match the request",
            Self::InvalidLeaves => "authenticated ledger leaves are malformed or noncanonical",
            Self::CellLeafMissing => "authenticated cell leaf is absent or mismatched",
            Self::LedgerRootMismatch => "authenticated ledger root does not match the attestation",
            Self::FederationMismatch => "attested root belongs to a different federation",
            Self::HeightRollback => "attested root is below the caller's minimum height",
            Self::StaleRoot => "attested root is stale",
            Self::FutureRoot => "attested root timestamp is too far in the future",
            Self::InvalidFinalizationQuorum => "attested root lacks a valid pinned hybrid quorum",
            Self::MembershipAuthorityMismatch => {
                "membership cell authority does not match the pinned authority"
            }
            Self::MembershipProgramMismatch => {
                "membership cell does not carry the canonical authority-bound program"
            }
            Self::MembershipFieldMismatch => {
                "membership cell fields do not match the canonical application"
            }
            Self::MembershipStatusMismatch => "membership lifecycle status does not satisfy policy",
            Self::MembershipTimestampInvalid => "membership lifecycle timestamps are invalid",
        })
    }
}

impl std::error::Error for MembershipInspectionError {}

/// Verify and return the exact decoded cell authenticated by `inspection`.
///
/// Server-provided Boolean/count summaries are intentionally absent from this API and cannot
/// contribute to acceptance.
pub fn verify_authenticated_cell(
    inspection: &AuthenticatedCellInspection,
    policy: &MembershipInspectionPolicy,
) -> Result<Cell, MembershipInspectionError> {
    if inspection.cell_bytes.len() > MAX_INSPECTION_CELL_BYTES {
        return Err(MembershipInspectionError::CellTooLarge);
    }
    let (cell, trailing): (Cell, &[u8]) = postcard::take_from_bytes(&inspection.cell_bytes)
        .map_err(|_| MembershipInspectionError::MalformedCell)?;
    if !trailing.is_empty()
        || postcard::to_stdvec(&cell).map_err(|_| MembershipInspectionError::MalformedCell)?
            != inspection.cell_bytes
    {
        return Err(MembershipInspectionError::MalformedCell);
    }
    if !cell.verify_id_integrity() || cell.id().as_bytes() != &inspection.cell_id {
        return Err(MembershipInspectionError::CellIdMismatch);
    }

    if inspection.attested_root_bytes.len() > MAX_INSPECTION_ATTESTED_ROOT_BYTES {
        return Err(MembershipInspectionError::AttestedRootTooLarge);
    }
    let (attested_root, trailing): (StoredAttestedRoot, &[u8]) =
        postcard::take_from_bytes(&inspection.attested_root_bytes)
            .map_err(|_| MembershipInspectionError::MalformedAttestedRoot)?;
    if !trailing.is_empty()
        || postcard::to_stdvec(&attested_root)
            .map_err(|_| MembershipInspectionError::MalformedAttestedRoot)?
            != inspection.attested_root_bytes
    {
        return Err(MembershipInspectionError::MalformedAttestedRoot);
    }
    if blake3::hash(&inspection.attested_root_bytes).as_bytes()
        != &policy.trusted_attested_root_digest
    {
        return Err(MembershipInspectionError::AttestedRootPinMismatch);
    }

    if inspection.leaves.is_empty() || inspection.leaves.len() > MAX_INSPECTION_LEAVES {
        return Err(MembershipInspectionError::InvalidLeaves);
    }
    if inspection
        .leaves
        .windows(2)
        .any(|pair| pair[0].0 >= pair[1].0)
    {
        return Err(MembershipInspectionError::InvalidLeaves);
    }
    let cell_hash = *blake3::hash(&inspection.cell_bytes).as_bytes();
    match inspection
        .leaves
        .binary_search_by_key(&inspection.cell_id, |(id, _)| *id)
    {
        Ok(index) if inspection.leaves[index].1 == cell_hash => {}
        _ => return Err(MembershipInspectionError::CellLeafMissing),
    }

    let root = dregg_persist::canonical_ledger_root_from_leaves(&inspection.leaves);
    if root != attested_root.merkle_root {
        return Err(MembershipInspectionError::LedgerRootMismatch);
    }
    if attested_root.federation_id != policy.federation_id {
        return Err(MembershipInspectionError::FederationMismatch);
    }
    if attested_root.height < policy.minimum_height {
        return Err(MembershipInspectionError::HeightRollback);
    }

    let maximum_future_skew = i64::try_from(policy.maximum_future_skew_seconds)
        .map_err(|_| MembershipInspectionError::FutureRoot)?;
    let latest_allowed = policy
        .now_unix_seconds
        .checked_add(maximum_future_skew)
        .ok_or(MembershipInspectionError::FutureRoot)?;
    if attested_root.timestamp > latest_allowed {
        return Err(MembershipInspectionError::FutureRoot);
    }
    let maximum_age = i64::try_from(policy.maximum_age_seconds)
        .map_err(|_| MembershipInspectionError::StaleRoot)?;
    let earliest_allowed = policy
        .now_unix_seconds
        .checked_sub(maximum_age)
        .ok_or(MembershipInspectionError::StaleRoot)?;
    if attested_root.timestamp < earliest_allowed {
        return Err(MembershipInspectionError::StaleRoot);
    }

    if attested_root.threshold == 0
        || !attested_root.verify_finalization_quorum(&policy.committee, &policy.ml_dsa_committee)
    {
        return Err(MembershipInspectionError::InvalidFinalizationQuorum);
    }

    Ok(cell)
}

#[derive(Clone, Debug)]
pub struct CastaliaMembershipExpectation {
    pub authority_public_key: [u8; 32],
    pub application: CastaliaMemberApplicationV1,
    pub birth_nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCastaliaMembership {
    pub cell_id: [u8; 32],
    pub member_public_key: [u8; 32],
    pub application_commitment: [u8; 32],
    pub status: MembershipStatus,
    pub generation: u64,
    pub created_at: u64,
    pub changed_at: u64,
}

fn canonical_field_u64(field: &[u8; 32]) -> Result<u64, MembershipInspectionError> {
    if field[..24] != [0u8; 24] {
        return Err(MembershipInspectionError::MembershipFieldMismatch);
    }
    let mut value = [0u8; 8];
    value.copy_from_slice(&field[24..]);
    Ok(u64::from_be_bytes(value))
}

/// Interpret an already-authenticated cell as the exact expected Castalia membership.
///
/// The Member Key is bound through the canonical application commitment. A caller requiring
/// current possession must additionally verify a fresh W2 presentation; this function proves
/// finalized institutional recognition and lifecycle state only.
pub fn inspect_castalia_membership(
    cell: &Cell,
    expectation: &CastaliaMembershipExpectation,
) -> Result<VerifiedCastaliaMembership, MembershipInspectionError> {
    if membership_creation_params(&expectation.application, expectation.authority_public_key)
        .is_err()
    {
        return Err(MembershipInspectionError::MembershipFieldMismatch);
    }
    if cell.public_key() != &expectation.authority_public_key {
        return Err(MembershipInspectionError::MembershipAuthorityMismatch);
    }
    let expected_cell_id = membership_cell_id(
        expectation.authority_public_key,
        expectation.application.factory_id,
        expectation.application.commitment(),
        expectation.birth_nonce,
    );
    if cell.id() != expected_cell_id {
        return Err(MembershipInspectionError::CellIdMismatch);
    }
    if cell.program != castalia_membership_program(expectation.authority_public_key) {
        return Err(MembershipInspectionError::MembershipProgramMismatch);
    }

    for (index, expected) in membership_initial_fields(&expectation.application)
        .into_iter()
        .take(12)
    {
        let observed = cell
            .state
            .get_field(index as usize)
            .ok_or(MembershipInspectionError::MembershipFieldMismatch)?;
        if observed != &field_from_u64(expected) {
            return Err(MembershipInspectionError::MembershipFieldMismatch);
        }
    }

    let status = match canonical_field_u64(
        cell.state
            .get_field(STATUS_SLOT as usize)
            .ok_or(MembershipInspectionError::MembershipFieldMismatch)?,
    )? {
        0 => MembershipStatus::Pending,
        1 => MembershipStatus::Active,
        2 => MembershipStatus::Suspended,
        3 => MembershipStatus::Revoked,
        4 => MembershipStatus::Expired,
        _ => return Err(MembershipInspectionError::MembershipStatusMismatch),
    };
    if status != MembershipStatus::Active {
        return Err(MembershipInspectionError::MembershipStatusMismatch);
    }
    let generation = canonical_field_u64(
        cell.state
            .get_field(GENERATION_SLOT as usize)
            .ok_or(MembershipInspectionError::MembershipFieldMismatch)?,
    )?;
    let created_at = canonical_field_u64(
        cell.state
            .get_field(CREATED_AT_SLOT as usize)
            .ok_or(MembershipInspectionError::MembershipFieldMismatch)?,
    )?;
    let changed_at = canonical_field_u64(
        cell.state
            .get_field(CHANGED_AT_SLOT as usize)
            .ok_or(MembershipInspectionError::MembershipFieldMismatch)?,
    )?;
    if generation == 0 || created_at == 0 || changed_at <= created_at {
        return Err(MembershipInspectionError::MembershipTimestampInvalid);
    }

    Ok(VerifiedCastaliaMembership {
        cell_id: *cell.id().as_bytes(),
        member_public_key: expectation.application.owner_pubkey,
        application_commitment: expectation.application.commitment(),
        status,
        generation,
        created_at,
        changed_at,
    })
}
