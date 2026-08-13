//! Castalia-owned institutional membership cells.

use dregg_cell::program::{SimpleStateConstraint, TransitionCase, TransitionGuard};
use dregg_cell::{
    CellMode, CellProgram, ChildVkStrategy, FactoryCreationParams, FactoryDescriptor, FactoryError,
    FactoryRegistry, FieldConstraint, ProvingSystemId, StateConstraint, VerifierFingerprint,
    VkComponents, canonical_vk_v2,
};
use dregg_circuit::air_descriptor::fingerprint as air_fingerprint_of;
use dregg_circuit::effect_vm::AIR_DESCRIPTOR as EFFECT_VM_AIR_DESCRIPTOR;
use dregg_types::CellId;

pub use dregg_cell::field_from_u64;

/// Plonky3 revision committed by the canonical cell-program VK recipe.
const PLONKY3_PINNED_REV: &str = "82cfad73";

/// Compute a method symbol exactly as `dregg-turn` does without linking the full turn crate.
pub fn symbol(name: &str) -> [u8; 32] {
    *blake3::hash(name.as_bytes()).as_bytes()
}

fn canonical_program_vk(program: &CellProgram) -> [u8; 32] {
    let program_bytes = dregg_cell::factory::canonical_program_bytes(program);
    canonical_vk_v2(&VkComponents {
        program_bytes: &program_bytes,
        air_fingerprint: effect_vm_air_fingerprint(),
        verifier_fingerprint: effect_vm_verifier_fingerprint(),
        proving_system_id: default_proving_system(),
    })
}

fn effect_vm_air_fingerprint() -> [u8; 32] {
    air_fingerprint_of(&EFFECT_VM_AIR_DESCRIPTOR)
}

fn effect_vm_verifier_fingerprint() -> VerifierFingerprint {
    let mut verifier = blake3::Hasher::new_derive_key("dregg-effect-vm-verifier-v1");
    verifier.update(EFFECT_VM_AIR_DESCRIPTOR.air_id.as_bytes());
    VerifierFingerprint::SourceHash(*verifier.finalize().as_bytes())
}

fn default_proving_system() -> ProvingSystemId {
    ProvingSystemId::Plonky3BabyBearFri {
        p3_rev: PLONKY3_PINNED_REV,
    }
}

/// Magic value identifying the Castalia membership schema.
pub const MAGIC_CASTMEM1: u64 = u64::from_le_bytes(*b"CASTMEM1");
/// Version of the Castalia membership field schema.
pub const CASTALIA_MEMBERSHIP_SCHEMA_VERSION: u64 = 1;

/// Schema magic slot.
pub const MAGIC_SLOT: u8 = 0;
/// Schema version slot.
pub const SCHEMA_VERSION_SLOT: u8 = 1;
/// Application kind slot.
pub const APPLICATION_KIND_SLOT: u8 = 2;
/// Application version slot.
pub const APPLICATION_VERSION_SLOT: u8 = 3;
/// Application nonce slot.
pub const APPLICATION_NONCE_SLOT: u8 = 4;
/// Membership class slot.
pub const MEMBERSHIP_CLASS_SLOT: u8 = 5;
/// Jurisdiction code slot.
pub const JURISDICTION_CODE_SLOT: u8 = 6;
/// Application flags slot.
pub const APPLICATION_FLAGS_SLOT: u8 = 7;
/// First of four application-commitment limb slots.
pub const COMMITMENT_SLOT_START: u8 = 8;
/// Membership status slot.
pub const STATUS_SLOT: u8 = 12;
/// Transition generation slot.
pub const GENERATION_SLOT: u8 = 13;
/// Creation timestamp slot.
pub const CREATED_AT_SLOT: u8 = 14;
/// Last-change timestamp slot.
pub const CHANGED_AT_SLOT: u8 = 15;

/// Maximum membership cells this factory may create in one epoch.
pub const CASTALIA_MEMBERSHIP_CREATION_BUDGET: u64 = 10_000;

/// Membership lifecycle status encoded in [`STATUS_SLOT`].
#[repr(u64)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MembershipStatus {
    /// Application awaits authority review.
    Pending = 0,
    /// Membership is active.
    Active = 1,
    /// Membership is temporarily suspended.
    Suspended = 2,
    /// Membership has been permanently revoked.
    Revoked = 3,
    /// Membership has expired.
    Expired = 4,
}

/// Canonical data committed by a Castalia membership application.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CastaliaMemberApplicationV1 {
    /// Membership factory identifier.
    pub factory_id: [u8; 32],
    /// Child membership program identifier.
    pub program_id: [u8; 32],
    /// Applicant's official Dregg cell.
    pub official_dregg_cell_id: CellId,
    /// Applicant owner key.
    pub owner_pubkey: [u8; 32],
    /// Application kind code.
    pub application_kind: u64,
    /// Application format version.
    pub application_version: u64,
    /// Application nonce.
    pub application_nonce: u64,
    /// Requested membership class.
    pub membership_class: u64,
    /// Jurisdiction code.
    pub jurisdiction_code: u64,
    /// Application flags.
    pub application_flags: u64,
    /// Creation timestamp.
    pub created_at: u64,
}

impl CastaliaMemberApplicationV1 {
    /// Encode the application in the fixed v1 commitment order.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(28 + 32 * 4 + 8 * 7);
        bytes.extend_from_slice(b"castalia/member-application/v1\0");
        bytes.extend_from_slice(&self.factory_id);
        bytes.extend_from_slice(&self.program_id);
        bytes.extend_from_slice(self.official_dregg_cell_id.as_bytes());
        bytes.extend_from_slice(&self.owner_pubkey);
        for value in [
            self.application_kind,
            self.application_version,
            self.application_nonce,
            self.membership_class,
            self.jurisdiction_code,
            self.application_flags,
            self.created_at,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Hash the canonical v1 application encoding.
    pub fn commitment(&self) -> [u8; 32] {
        *blake3::hash(&self.canonical_bytes()).as_bytes()
    }
}

/// Derive the canonical token for one durable Castalia membership-cell birth.
///
/// The durable `birth_nonce` is reserved by Control. It is distinct from the
/// application nonce and is encoded as little-endian bytes under the D0 domain.
pub fn membership_birth_token_id(
    factory_id: [u8; 32],
    application_commitment: [u8; 32],
    birth_nonce: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"castalia/membership-birth-token/v1\0");
    hasher.update(&factory_id);
    hasher.update(&application_commitment);
    hasher.update(&birth_nonce.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Derive the authority-owned membership cell created by the canonical factory effect.
pub fn membership_cell_id(
    authority: [u8; 32],
    factory_id: [u8; 32],
    application_commitment: [u8; 32],
    birth_nonce: u64,
) -> CellId {
    CellId::derive_raw(
        &authority,
        &membership_birth_token_id(factory_id, application_commitment, birth_nonce),
    )
}

/// Produce all sixteen indexed initial membership fields.
pub fn membership_initial_fields(app: &CastaliaMemberApplicationV1) -> [(u32, u64); 16] {
    let commitment = app.commitment();
    let mut limbs = [0u64; 4];
    for (limb, chunk) in commitment.chunks_exact(8).enumerate() {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(chunk);
        limbs[limb] = u64::from_le_bytes(bytes);
    }

    [
        (MAGIC_SLOT as u32, MAGIC_CASTMEM1),
        (
            SCHEMA_VERSION_SLOT as u32,
            CASTALIA_MEMBERSHIP_SCHEMA_VERSION,
        ),
        (APPLICATION_KIND_SLOT as u32, app.application_kind),
        (APPLICATION_VERSION_SLOT as u32, app.application_version),
        (APPLICATION_NONCE_SLOT as u32, app.application_nonce),
        (MEMBERSHIP_CLASS_SLOT as u32, app.membership_class),
        (JURISDICTION_CODE_SLOT as u32, app.jurisdiction_code),
        (APPLICATION_FLAGS_SLOT as u32, app.application_flags),
        (COMMITMENT_SLOT_START as u32, limbs[0]),
        (COMMITMENT_SLOT_START as u32 + 1, limbs[1]),
        (COMMITMENT_SLOT_START as u32 + 2, limbs[2]),
        (COMMITMENT_SLOT_START as u32 + 3, limbs[3]),
        (STATUS_SLOT as u32, MembershipStatus::Pending as u64),
        (GENERATION_SLOT as u32, 0),
        (CREATED_AT_SLOT as u32, app.created_at),
        (CHANGED_AT_SLOT as u32, app.created_at),
    ]
}

fn lifecycle_case(
    method: &str,
    allowed: &[(MembershipStatus, MembershipStatus)],
) -> TransitionCase {
    TransitionCase {
        guard: TransitionGuard::MethodIs {
            method: symbol(method),
        },
        constraints: vec![StateConstraint::AllowedTransitions {
            slot_index: STATUS_SLOT,
            allowed: allowed
                .iter()
                .map(|(old, new)| (field_from_u64(*old as u64), field_from_u64(*new as u64)))
                .collect(),
        }],
    }
}

fn membership_invariants(authority: [u8; 32]) -> Vec<StateConstraint> {
    let mut invariants = Vec::with_capacity(16);
    invariants.push(StateConstraint::AnyOf {
        variants: vec![SimpleStateConstraint::SenderIs { pk: authority }],
    });
    for index in 0..=11 {
        invariants.push(StateConstraint::Immutable { index });
    }
    invariants.extend([
        StateConstraint::MonotonicSequence {
            seq_index: GENERATION_SLOT,
        },
        StateConstraint::Immutable {
            index: CREATED_AT_SLOT,
        },
        StateConstraint::StrictMonotonic {
            index: CHANGED_AT_SLOT,
        },
    ]);
    invariants
}

/// Build the authority-controlled Castalia membership lifecycle program.
pub fn castalia_membership_program(authority: [u8; 32]) -> CellProgram {
    CellProgram::Cases(vec![
        TransitionCase {
            guard: TransitionGuard::Always,
            constraints: membership_invariants(authority),
        },
        lifecycle_case(
            "activate",
            &[(MembershipStatus::Pending, MembershipStatus::Active)],
        ),
        lifecycle_case(
            "suspend",
            &[(MembershipStatus::Active, MembershipStatus::Suspended)],
        ),
        lifecycle_case(
            "resume",
            &[(MembershipStatus::Suspended, MembershipStatus::Active)],
        ),
        lifecycle_case(
            "revoke",
            &[
                (MembershipStatus::Pending, MembershipStatus::Revoked),
                (MembershipStatus::Active, MembershipStatus::Revoked),
                (MembershipStatus::Suspended, MembershipStatus::Revoked),
            ],
        ),
        lifecycle_case(
            "expire",
            &[
                (MembershipStatus::Pending, MembershipStatus::Expired),
                (MembershipStatus::Active, MembershipStatus::Expired),
                (MembershipStatus::Suspended, MembershipStatus::Expired),
            ],
        ),
    ])
}

/// Compute the canonical verifier key for the authority-bound child program.
pub fn castalia_membership_child_program_vk(authority: [u8; 32]) -> [u8; 32] {
    canonical_program_vk(&castalia_membership_program(authority))
}

/// Return the stable factory identifier bound to one Castalia authority.
pub fn castalia_membership_factory_vk(authority: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("castalia-membership-factory-v1");
    hasher.update(&authority);
    *hasher.finalize().as_bytes()
}

fn castalia_membership_factory_descriptor(authority: [u8; 32]) -> FactoryDescriptor {
    let child_vk = castalia_membership_child_program_vk(authority);
    FactoryDescriptor {
        factory_vk: castalia_membership_factory_vk(authority),
        child_program_vk: Some(child_vk),
        child_vk_strategy: Some(ChildVkStrategy::Fixed(Some(child_vk))),
        allowed_cap_templates: vec![],
        field_constraints: vec![
            FieldConstraint::Equality {
                field_index: MAGIC_SLOT as u32,
                value: MAGIC_CASTMEM1,
            },
            FieldConstraint::Equality {
                field_index: SCHEMA_VERSION_SLOT as u32,
                value: CASTALIA_MEMBERSHIP_SCHEMA_VERSION,
            },
            FieldConstraint::Equality {
                field_index: STATUS_SLOT as u32,
                value: MembershipStatus::Pending as u64,
            },
            FieldConstraint::Equality {
                field_index: GENERATION_SLOT as u32,
                value: 0,
            },
            FieldConstraint::NonZero {
                field_index: CREATED_AT_SLOT as u32,
            },
            FieldConstraint::NonZero {
                field_index: CHANGED_AT_SLOT as u32,
            },
        ],
        state_constraints: membership_invariants(authority),
        default_mode: CellMode::Sovereign,
        creation_budget: Some(CASTALIA_MEMBERSHIP_CREATION_BUDGET),
    }
}

fn membership_creation_params_unchecked(
    app: &CastaliaMemberApplicationV1,
    authority: [u8; 32],
) -> FactoryCreationParams {
    FactoryCreationParams {
        mode: CellMode::Sovereign,
        program_vk: Some(castalia_membership_child_program_vk(authority)),
        initial_fields: membership_initial_fields(app).into(),
        initial_caps: vec![],
        owner_pubkey: authority,
    }
}

/// Fail-closed reasons a proposed membership birth is not canonical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MembershipBirthError {
    /// No zero key may control a Castalia factory.
    MissingAuthority,
    /// The generic Dregg factory contract rejected the proposal.
    Factory(FactoryError),
    /// The application names a factory other than Castalia's fixed factory.
    ApplicationFactoryMismatch,
    /// The application names a child program other than the authority-bound program.
    ApplicationProgramMismatch,
    /// The application does not identify an official Dregg cell.
    MissingOfficialDreggCell,
    /// The application does not identify an owner key.
    MissingOwner,
    /// The constructor owner differs from the bound Castalia authority.
    OwnerMismatch,
    /// Constructor fields are not the exact canonical 16-field application state.
    InitialFieldsMismatch,
    /// Durable state already binds this factory identity to a different descriptor.
    FactoryDeploymentMismatch,
}

impl std::fmt::Display for MembershipBirthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAuthority => f.write_str("Castalia authority is missing"),
            Self::Factory(error) => write!(f, "factory creation rejected: {error:?}"),
            Self::ApplicationFactoryMismatch => f.write_str("application factory mismatch"),
            Self::ApplicationProgramMismatch => f.write_str("application program mismatch"),
            Self::MissingOfficialDreggCell => f.write_str("official Dregg cell is missing"),
            Self::MissingOwner => f.write_str("membership owner is missing"),
            Self::OwnerMismatch => {
                f.write_str("constructor owner does not match Castalia authority")
            }
            Self::InitialFieldsMismatch => {
                f.write_str("constructor fields do not match canonical application state")
            }
            Self::FactoryDeploymentMismatch => {
                f.write_str("durable factory descriptor conflicts with canonical Castalia factory")
            }
        }
    }
}

impl std::error::Error for MembershipBirthError {}

impl From<FactoryError> for MembershipBirthError {
    fn from(value: FactoryError) -> Self {
        Self::Factory(value)
    }
}

/// A Castalia membership factory bound once to its trusted authority and private descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastaliaMembershipFactory {
    authority: [u8; 32],
    descriptor: FactoryDescriptor,
}

/// Construct a bound factory, rejecting an absent authority before any issuance API is exposed.
pub fn castalia_membership_factory(
    authority: [u8; 32],
) -> Result<CastaliaMembershipFactory, MembershipBirthError> {
    if authority == [0; 32] {
        return Err(MembershipBirthError::MissingAuthority);
    }
    Ok(CastaliaMembershipFactory {
        authority,
        descriptor: castalia_membership_factory_descriptor(authority),
    })
}

impl CastaliaMembershipFactory {
    /// Return this authority-bound factory identity.
    pub fn factory_vk(&self) -> [u8; 32] {
        self.descriptor.factory_vk
    }

    /// Return the only child program this factory can issue.
    pub fn child_program_vk(&self) -> [u8; 32] {
        self.descriptor
            .child_program_vk
            .expect("bound membership factories always install a child program")
    }

    /// Deploy the private descriptor without exposing a bypassable raw constructor contract.
    pub fn deploy(&self, registry: &mut FactoryRegistry) -> [u8; 32] {
        registry.deploy(self.descriptor.clone())
    }

    /// Idempotently deploy this factory while refusing a conflicting durable descriptor.
    pub fn deploy_checked(
        &self,
        registry: &mut FactoryRegistry,
    ) -> Result<[u8; 32], MembershipBirthError> {
        if let Some(existing) = registry.get(&self.factory_vk())
            && existing != &self.descriptor
        {
            return Err(MembershipBirthError::FactoryDeploymentMismatch);
        }
        Ok(registry.deploy(self.descriptor.clone()))
    }

    fn validate_application(
        &self,
        app: &CastaliaMemberApplicationV1,
    ) -> Result<(), MembershipBirthError> {
        if app.factory_id != self.factory_vk() {
            return Err(MembershipBirthError::ApplicationFactoryMismatch);
        }
        if app.program_id != self.child_program_vk() {
            return Err(MembershipBirthError::ApplicationProgramMismatch);
        }
        if app.official_dregg_cell_id.as_bytes() == &[0; 32] {
            return Err(MembershipBirthError::MissingOfficialDreggCell);
        }
        if app.owner_pubkey == [0; 32] {
            return Err(MembershipBirthError::MissingOwner);
        }
        Ok(())
    }

    /// Build canonical parameters only after validating the committed application identity.
    pub fn creation_params(
        &self,
        app: &CastaliaMemberApplicationV1,
    ) -> Result<FactoryCreationParams, MembershipBirthError> {
        self.validate_application(app)?;
        let params = membership_creation_params_unchecked(app, self.authority);
        self.validate_birth(app, &params)?;
        Ok(params)
    }

    /// Validate a birth through the private descriptor and exact application binding.
    pub fn validate_birth(
        &self,
        app: &CastaliaMemberApplicationV1,
        params: &FactoryCreationParams,
    ) -> Result<(), MembershipBirthError> {
        self.validate_application(app)?;
        self.descriptor.validate_creation(params)?;
        if params.owner_pubkey != self.authority {
            return Err(MembershipBirthError::OwnerMismatch);
        }
        if params.initial_fields.as_slice() != membership_initial_fields(app) {
            return Err(MembershipBirthError::InitialFieldsMismatch);
        }
        Ok(())
    }
}

/// Safely construct canonical parameters through an authority-bound factory.
pub fn membership_creation_params(
    app: &CastaliaMemberApplicationV1,
    authority: [u8; 32],
) -> Result<FactoryCreationParams, MembershipBirthError> {
    castalia_membership_factory(authority)?.creation_params(app)
}

/// Safely validate a birth through an authority-bound factory.
pub fn validate_membership_birth(
    app: &CastaliaMemberApplicationV1,
    authority: [u8; 32],
    params: &FactoryCreationParams,
) -> Result<(), MembershipBirthError> {
    castalia_membership_factory(authority)?.validate_birth(app, params)
}
