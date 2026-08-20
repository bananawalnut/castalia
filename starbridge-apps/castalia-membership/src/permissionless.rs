//! Permissionless Castalia membership.
//!
//! This is deliberately separate from the legacy institution-controlled v1
//! application lifecycle. A v2 membership is a public, deterministic,
//! member-owned cell that is Active at birth. The factory mechanically enforces
//! the complete initial state; no admission service or institutional signer is
//! involved.

use dregg_cell::{
    Cell, CellMode, CellProgram, ChildVkStrategy, FactoryCreationParams, FactoryDescriptor,
    FactoryError, FactoryRegistry, FieldConstraint, StateConstraint,
};
use dregg_types::CellId;

use crate::{CASTALIA_MEMBERSHIP_CREATION_BUDGET, canonical_program_vk, field_from_u64};

/// Magic value identifying the permissionless Castalia membership schema.
pub const MAGIC_CASTMEM2: u64 = u64::from_le_bytes(*b"CASTMEM2");
/// Version of the permissionless membership schema.
pub const CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION: u64 = 2;
/// Public self-issuance policy marker.
pub const CASTALIA_PERMISSIONLESS_POLICY: u64 = 1;
/// Active is the only base-membership status in v2.
pub const CASTALIA_PERMISSIONLESS_ACTIVE: u64 = 1;
/// Domain used by a Wallet to prove consent to its deterministic membership.
pub const CASTALIA_PERMISSIONLESS_JOIN_DOMAIN: &[u8] =
    b"castalia/permissionless-membership-join/v2\0";

const FIELD_COUNT: usize = 16;
const MAGIC_SLOT: usize = 0;
const SCHEMA_VERSION_SLOT: usize = 1;
const POLICY_SLOT: usize = 2;
const STATUS_SLOT: usize = 12;
const GENERATION_SLOT: usize = 13;

fn immutable_membership_constraints() -> Vec<StateConstraint> {
    (0..FIELD_COUNT)
        .map(|index| StateConstraint::Immutable { index: index as u8 })
        .collect()
}

/// The exact sixteen-field state every permissionless membership carries.
#[must_use]
pub fn permissionless_membership_initial_fields() -> [(u32, u64); FIELD_COUNT] {
    let mut values = [0u64; FIELD_COUNT];
    values[MAGIC_SLOT] = MAGIC_CASTMEM2;
    values[SCHEMA_VERSION_SLOT] = CASTALIA_PERMISSIONLESS_MEMBERSHIP_SCHEMA_VERSION;
    values[POLICY_SLOT] = CASTALIA_PERMISSIONLESS_POLICY;
    values[STATUS_SLOT] = CASTALIA_PERMISSIONLESS_ACTIVE;
    values[GENERATION_SLOT] = 0;
    std::array::from_fn(|index| (index as u32, values[index]))
}

/// Immutable base-membership program. Roles, sanctions, and service access live
/// in separate cells; the fact that a key joined Castalia is not revocable.
#[must_use]
pub fn permissionless_membership_program() -> CellProgram {
    CellProgram::Cases(vec![dregg_cell::program::TransitionCase {
        guard: dregg_cell::program::TransitionGuard::Always,
        constraints: immutable_membership_constraints(),
    }])
}

/// Canonical verifier key for the permissionless child program.
#[must_use]
pub fn permissionless_membership_child_program_vk() -> [u8; 32] {
    canonical_program_vk(&permissionless_membership_program())
}

/// Stable public factory identifier. It is intentionally not derived from an
/// institution key: every conforming Dregg node deploys the same factory.
#[must_use]
pub fn permissionless_membership_factory_vk() -> [u8; 32] {
    *blake3::hash(b"castalia/permissionless-membership-factory/v2\0").as_bytes()
}

/// Stable token domain. Combined with the Member Key by `CellId::derive_raw`,
/// this makes membership one-per-key and retry-idempotent.
#[must_use]
pub fn permissionless_membership_token_id() -> [u8; 32] {
    *blake3::hash(b"castalia/permissionless-membership-cell/v2\0").as_bytes()
}

/// Deterministic membership cell for one Member Key.
#[must_use]
pub fn permissionless_membership_cell_id(owner_public_key: [u8; 32]) -> CellId {
    CellId::derive_raw(&owner_public_key, &permissionless_membership_token_id())
}

fn descriptor() -> FactoryDescriptor {
    let child_vk = permissionless_membership_child_program_vk();
    FactoryDescriptor {
        factory_vk: permissionless_membership_factory_vk(),
        child_program_vk: Some(child_vk),
        child_vk_strategy: Some(ChildVkStrategy::Fixed(Some(child_vk))),
        allowed_cap_templates: vec![],
        field_constraints: permissionless_membership_initial_fields()
            .into_iter()
            .map(|(field_index, value)| FieldConstraint::Equality { field_index, value })
            .collect(),
        state_constraints: immutable_membership_constraints(),
        default_mode: CellMode::Sovereign,
        creation_budget: Some(CASTALIA_MEMBERSHIP_CREATION_BUDGET),
    }
}

/// Failure to construct or recognize an exact permissionless membership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionlessMembershipError {
    MissingOwner,
    Factory(FactoryError),
    OwnerMismatch,
    InitialFieldsMismatch,
    FactoryDeploymentMismatch,
    CellIdMismatch,
    TokenMismatch,
    ProgramMismatch,
    CapabilityMismatch,
}

impl std::fmt::Display for PermissionlessMembershipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingOwner => formatter.write_str("permissionless membership owner is missing"),
            Self::Factory(error) => write!(formatter, "factory creation rejected: {error:?}"),
            Self::OwnerMismatch => formatter.write_str("membership owner mismatch"),
            Self::InitialFieldsMismatch => {
                formatter.write_str("membership fields are not canonical")
            }
            Self::FactoryDeploymentMismatch => {
                formatter.write_str("durable public factory descriptor conflicts with v2")
            }
            Self::CellIdMismatch => formatter.write_str("membership cell id mismatch"),
            Self::TokenMismatch => formatter.write_str("membership token domain mismatch"),
            Self::ProgramMismatch => formatter.write_str("membership program mismatch"),
            Self::CapabilityMismatch => {
                formatter.write_str("base membership must not carry capabilities or delegation")
            }
        }
    }
}

impl std::error::Error for PermissionlessMembershipError {}

impl From<FactoryError> for PermissionlessMembershipError {
    fn from(value: FactoryError) -> Self {
        Self::Factory(value)
    }
}

/// Public v2 membership factory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionlessMembershipFactory {
    descriptor: FactoryDescriptor,
}

/// Construct the canonical public factory.
#[must_use]
pub fn permissionless_membership_factory() -> PermissionlessMembershipFactory {
    PermissionlessMembershipFactory {
        descriptor: descriptor(),
    }
}

impl PermissionlessMembershipFactory {
    #[must_use]
    pub fn descriptor(&self) -> &FactoryDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub fn factory_vk(&self) -> [u8; 32] {
        self.descriptor.factory_vk
    }

    #[must_use]
    pub fn child_program_vk(&self) -> [u8; 32] {
        permissionless_membership_child_program_vk()
    }

    #[must_use]
    pub fn program_vk_recipe(
        &self,
    ) -> (
        [u8; 32],
        dregg_cell::VerifierFingerprint,
        dregg_cell::ProvingSystemId,
    ) {
        (
            super::effect_vm_air_fingerprint(),
            super::effect_vm_verifier_fingerprint(),
            super::default_proving_system(),
        )
    }

    /// Idempotently deploy the public factory and its exact child program.
    pub fn deploy_checked(
        &self,
        registry: &mut FactoryRegistry,
    ) -> Result<[u8; 32], PermissionlessMembershipError> {
        if let Some(existing) = registry.get(&self.factory_vk())
            && existing != &self.descriptor
        {
            return Err(PermissionlessMembershipError::FactoryDeploymentMismatch);
        }
        let program = permissionless_membership_program();
        self.descriptor.validate_child_vk_canonical_v2(
            &program,
            super::effect_vm_air_fingerprint(),
            super::effect_vm_verifier_fingerprint(),
            super::default_proving_system(),
        )?;
        registry
            .deploy_with_full_child_program_v2(
                self.descriptor.clone(),
                program,
                super::effect_vm_air_fingerprint(),
                super::effect_vm_verifier_fingerprint(),
                super::default_proving_system(),
            )
            .map_err(PermissionlessMembershipError::Factory)
    }

    /// Exact member-owned factory parameters.
    pub fn creation_params(
        &self,
        owner_public_key: [u8; 32],
    ) -> Result<FactoryCreationParams, PermissionlessMembershipError> {
        if owner_public_key == [0; 32] {
            return Err(PermissionlessMembershipError::MissingOwner);
        }
        let params = FactoryCreationParams {
            mode: CellMode::Sovereign,
            program_vk: Some(self.child_program_vk()),
            initial_fields: permissionless_membership_initial_fields().into(),
            initial_caps: vec![],
            owner_pubkey: owner_public_key,
        };
        self.validate_birth(owner_public_key, &params)?;
        Ok(params)
    }

    pub fn validate_birth(
        &self,
        owner_public_key: [u8; 32],
        params: &FactoryCreationParams,
    ) -> Result<(), PermissionlessMembershipError> {
        if owner_public_key == [0; 32] {
            return Err(PermissionlessMembershipError::MissingOwner);
        }
        self.descriptor.validate_creation(params)?;
        if params.owner_pubkey != owner_public_key {
            return Err(PermissionlessMembershipError::OwnerMismatch);
        }
        let expected_fields = permissionless_membership_initial_fields();
        if params.initial_fields.as_slice() != expected_fields.as_slice() {
            return Err(PermissionlessMembershipError::InitialFieldsMismatch);
        }
        Ok(())
    }
}

/// Validate a node-returned cell without trusting advisory response fields.
pub fn validate_permissionless_membership_cell(
    cell: &Cell,
    owner_public_key: [u8; 32],
) -> Result<(), PermissionlessMembershipError> {
    if cell.id() != permissionless_membership_cell_id(owner_public_key) {
        return Err(PermissionlessMembershipError::CellIdMismatch);
    }
    if cell.public_key() != &owner_public_key {
        return Err(PermissionlessMembershipError::OwnerMismatch);
    }
    if cell.token_id() != &permissionless_membership_token_id() {
        return Err(PermissionlessMembershipError::TokenMismatch);
    }
    if cell.program != permissionless_membership_program() || cell.mode != CellMode::Sovereign {
        return Err(PermissionlessMembershipError::ProgramMismatch);
    }
    for (index, (_, value)) in permissionless_membership_initial_fields()
        .iter()
        .enumerate()
    {
        if cell.state.fields[index] != field_from_u64(*value) {
            return Err(PermissionlessMembershipError::InitialFieldsMismatch);
        }
    }
    if !cell.capabilities.is_empty() || cell.delegate.is_some() || cell.delegation.is_some() {
        return Err(PermissionlessMembershipError::CapabilityMismatch);
    }
    Ok(())
}
