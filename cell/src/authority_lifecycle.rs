//! Pure, deterministic identities for Castalia authority lifecycle cells.
//!
//! This module reserves names and validates commitments only. It does not read
//! a ledger, create a cell, or apply lifecycle state.

use std::collections::BTreeSet;

/// Domain for world-scoped public value digests.
pub const AUTHORITY_VALUE_DOMAIN_V1: &str = "castalia-authority-value-v1";
/// Domain for commitments to canonical lifecycle-key bytes.
pub const AUTHORITY_LIFECYCLE_KEY_DOMAIN_V1: &str = "castalia-authority-lifecycle-key-v1";
/// Domain for lifecycle token IDs derived from canonical lifecycle-key bytes.
pub const AUTHORITY_LIFECYCLE_TOKEN_DOMAIN_V1: &str = "castalia-authority-lifecycle-token-v1";

const ISSUER_KEY_ID_PREFIX: &str = "dregg-issuer:blake3:";
const DIGEST_LEN: usize = 32;
const LOWER_HEX_DIGEST_LEN: usize = DIGEST_LEN * 2;

/// Rejections produced by the pure authority identity contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityIdentityError {
    MalformedRegistryId,
    MalformedReceiverAudience,
    UnknownValueKind,
    UnknownLifecycleKind,
    NonCanonicalLifecycleKey,
    LifecycleKeyCommitmentMismatch,
    LifecycleTokenIdMismatch,
    DuplicateLifecycleIdentity,
    LifecycleTokenCollision,
    MalformedIssuerPublicKey,
    NonCanonicalIssuerKeyId,
    IssuerKeyIdMismatch,
    DuplicateIssuerKeyId,
    DuplicateIssuerPublicKey,
    IssuerKeyIdCollision,
}

impl core::fmt::Display for AuthorityIdentityError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for AuthorityIdentityError {}

/// Stable registry and exact receiver audience for one authority world.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthorityScopeV1 {
    registry_id: [u8; DIGEST_LEN],
    receiver_audience: Vec<u8>,
}

impl AuthorityScopeV1 {
    /// Validate an exact 32-byte registry ID and canonical `dregg://<world>` audience.
    pub fn new(
        registry_id: &[u8],
        receiver_audience: &[u8],
    ) -> Result<Self, AuthorityIdentityError> {
        let registry_id: [u8; DIGEST_LEN] = registry_id
            .try_into()
            .map_err(|_| AuthorityIdentityError::MalformedRegistryId)?;
        if !is_canonical_receiver_audience(receiver_audience) {
            return Err(AuthorityIdentityError::MalformedReceiverAudience);
        }
        Ok(Self {
            registry_id,
            receiver_audience: receiver_audience.to_vec(),
        })
    }

    pub fn registry_id(&self) -> &[u8; DIGEST_LEN] {
        &self.registry_id
    }

    pub fn receiver_audience(&self) -> &[u8] {
        &self.receiver_audience
    }
}

fn is_canonical_receiver_audience(candidate: &[u8]) -> bool {
    let Some(world) = candidate.strip_prefix(b"dregg://") else {
        return false;
    };
    if world.is_empty() || world.len() > 128 {
        return false;
    }
    world.iter().enumerate().all(|(index, byte)| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
    })
}

/// Closed kinds accepted by the world-scoped value-digest contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityValueKindV1 {
    Subject,
    Operation,
    Resource,
    Issuer,
    Root,
    Namespace,
    CredentialTail,
    AuthorityRoot,
}

impl AuthorityValueKindV1 {
    pub const ALL: [Self; 8] = [
        Self::Subject,
        Self::Operation,
        Self::Resource,
        Self::Issuer,
        Self::Root,
        Self::Namespace,
        Self::CredentialTail,
        Self::AuthorityRoot,
    ];

    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Subject => b"subject",
            Self::Operation => b"operation",
            Self::Resource => b"resource",
            Self::Issuer => b"issuer",
            Self::Root => b"root",
            Self::Namespace => b"namespace",
            Self::CredentialTail => b"credential-tail",
            Self::AuthorityRoot => b"authority-root",
        }
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, AuthorityIdentityError> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_bytes() == bytes)
            .ok_or(AuthorityIdentityError::UnknownValueKind)
    }
}

/// Closed authority lifecycle cell kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityLifecycleKindV1 {
    Issuer,
    AuthorityRoot,
    Namespace,
    Resource,
    CredentialTail,
}

impl AuthorityLifecycleKindV1 {
    pub const ALL: [Self; 5] = [
        Self::Issuer,
        Self::AuthorityRoot,
        Self::Namespace,
        Self::Resource,
        Self::CredentialTail,
    ];

    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Issuer => b"issuer",
            Self::AuthorityRoot => b"authority-root",
            Self::Namespace => b"namespace",
            Self::Resource => b"resource",
            Self::CredentialTail => b"credential-tail",
        }
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, AuthorityIdentityError> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_bytes() == bytes)
            .ok_or(AuthorityIdentityError::UnknownLifecycleKind)
    }
}

/// Compute the issue-locked, world-scoped public digest.
pub fn world_scoped_value_digest_v1(
    scope: &AuthorityScopeV1,
    kind: AuthorityValueKindV1,
    value: &[u8],
) -> [u8; DIGEST_LEN] {
    let mut hasher = blake3::Hasher::new_derive_key(AUTHORITY_VALUE_DOMAIN_V1);
    update_len_prefixed(&mut hasher, scope.registry_id());
    update_len_prefixed(&mut hasher, scope.receiver_audience());
    update_len_prefixed(&mut hasher, kind.as_bytes());
    update_len_prefixed(&mut hasher, value);
    *hasher.finalize().as_bytes()
}

fn update_len_prefixed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// A reserved lifecycle identity derived without consulting current cell state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityLifecycleIdentityV1 {
    scope: AuthorityScopeV1,
    kind: AuthorityLifecycleKindV1,
    value_digest: [u8; DIGEST_LEN],
    canonical_key_bytes: Vec<u8>,
    key_commitment: [u8; DIGEST_LEN],
    token_id: [u8; DIGEST_LEN],
}

impl AuthorityLifecycleIdentityV1 {
    pub fn derive(
        scope: AuthorityScopeV1,
        kind: AuthorityLifecycleKindV1,
        value_digest: [u8; DIGEST_LEN],
    ) -> Self {
        let canonical_key_bytes = canonical_lifecycle_key_bytes(&scope, kind, &value_digest);
        let key_commitment =
            derive_from_canonical_key(AUTHORITY_LIFECYCLE_KEY_DOMAIN_V1, &canonical_key_bytes);
        let token_id =
            derive_from_canonical_key(AUTHORITY_LIFECYCLE_TOKEN_DOMAIN_V1, &canonical_key_bytes);
        Self {
            scope,
            kind,
            value_digest,
            canonical_key_bytes,
            key_commitment,
            token_id,
        }
    }

    pub fn scope(&self) -> &AuthorityScopeV1 {
        &self.scope
    }

    pub const fn kind(&self) -> AuthorityLifecycleKindV1 {
        self.kind
    }

    pub const fn value_digest(&self) -> &[u8; DIGEST_LEN] {
        &self.value_digest
    }

    pub fn canonical_key_bytes(&self) -> &[u8] {
        &self.canonical_key_bytes
    }

    pub const fn key_commitment(&self) -> [u8; DIGEST_LEN] {
        self.key_commitment
    }

    pub const fn token_id(&self) -> [u8; DIGEST_LEN] {
        self.token_id
    }
}

fn canonical_lifecycle_key_bytes(
    scope: &AuthorityScopeV1,
    kind: AuthorityLifecycleKindV1,
    value_digest: &[u8; DIGEST_LEN],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(
        DIGEST_LEN
            + 1
            + scope.receiver_audience().len()
            + 1
            + kind.as_bytes().len()
            + 1
            + DIGEST_LEN,
    );
    bytes.extend_from_slice(scope.registry_id());
    bytes.push(0);
    bytes.extend_from_slice(scope.receiver_audience());
    bytes.push(0);
    bytes.extend_from_slice(kind.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(value_digest);
    bytes
}

fn derive_from_canonical_key(domain: &str, canonical_key_bytes: &[u8]) -> [u8; DIGEST_LEN] {
    let mut hasher = blake3::Hasher::new_derive_key(domain);
    hasher.update(canonical_key_bytes);
    *hasher.finalize().as_bytes()
}

/// Untrusted identity components used to recognize a reserved identity without a cell lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityLifecycleIdentityClaimV1 {
    pub registry_id: Vec<u8>,
    pub receiver_audience: Vec<u8>,
    pub lifecycle_kind: Vec<u8>,
    pub value_digest: Vec<u8>,
    pub canonical_key_bytes: Vec<u8>,
    pub key_commitment: [u8; DIGEST_LEN],
    pub token_id: [u8; DIGEST_LEN],
}

impl From<&AuthorityLifecycleIdentityV1> for AuthorityLifecycleIdentityClaimV1 {
    fn from(identity: &AuthorityLifecycleIdentityV1) -> Self {
        Self {
            registry_id: identity.scope.registry_id().to_vec(),
            receiver_audience: identity.scope.receiver_audience().to_vec(),
            lifecycle_kind: identity.kind.as_bytes().to_vec(),
            value_digest: identity.value_digest.to_vec(),
            canonical_key_bytes: identity.canonical_key_bytes.clone(),
            key_commitment: identity.key_commitment,
            token_id: identity.token_id,
        }
    }
}

/// Validate canonical reservation claims without consulting ledger existence.
pub fn validate_reserved_authority_identity_claims_v1(
    claims: &[AuthorityLifecycleIdentityClaimV1],
) -> Result<Vec<AuthorityLifecycleIdentityV1>, AuthorityIdentityError> {
    for (index, claim) in claims.iter().enumerate() {
        for other in &claims[..index] {
            if claim.token_id == other.token_id {
                return if claim == other {
                    Err(AuthorityIdentityError::DuplicateLifecycleIdentity)
                } else {
                    Err(AuthorityIdentityError::LifecycleTokenCollision)
                };
            }
        }
    }

    claims.iter().map(validate_identity_claim).collect()
}

fn validate_identity_claim(
    claim: &AuthorityLifecycleIdentityClaimV1,
) -> Result<AuthorityLifecycleIdentityV1, AuthorityIdentityError> {
    let scope = AuthorityScopeV1::new(&claim.registry_id, &claim.receiver_audience)?;
    let kind = AuthorityLifecycleKindV1::try_from_bytes(&claim.lifecycle_kind)?;
    let value_digest: [u8; DIGEST_LEN] = claim
        .value_digest
        .as_slice()
        .try_into()
        .map_err(|_| AuthorityIdentityError::NonCanonicalLifecycleKey)?;
    let expected = AuthorityLifecycleIdentityV1::derive(scope, kind, value_digest);
    if claim.canonical_key_bytes != expected.canonical_key_bytes {
        return Err(AuthorityIdentityError::NonCanonicalLifecycleKey);
    }
    if claim.key_commitment != expected.key_commitment {
        return Err(AuthorityIdentityError::LifecycleKeyCommitmentMismatch);
    }
    if claim.token_id != expected.token_id {
        return Err(AuthorityIdentityError::LifecycleTokenIdMismatch);
    }
    Ok(expected)
}

/// Canonical issuer selector, exact public key, and reserved lifecycle identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuerLifecycleIdentityV1 {
    issuer_key_id: String,
    public_key: [u8; DIGEST_LEN],
    identity: AuthorityLifecycleIdentityV1,
}

impl IssuerLifecycleIdentityV1 {
    pub fn derive(
        scope: AuthorityScopeV1,
        public_key: &[u8],
    ) -> Result<Self, AuthorityIdentityError> {
        let public_key: [u8; DIGEST_LEN] = public_key
            .try_into()
            .map_err(|_| AuthorityIdentityError::MalformedIssuerPublicKey)?;
        let digest =
            world_scoped_value_digest_v1(&scope, AuthorityValueKindV1::Issuer, &public_key);
        let issuer_key_id = format!("{ISSUER_KEY_ID_PREFIX}{}", lowercase_hex(&digest));
        let identity =
            AuthorityLifecycleIdentityV1::derive(scope, AuthorityLifecycleKindV1::Issuer, digest);
        Ok(Self {
            issuer_key_id,
            public_key,
            identity,
        })
    }

    pub fn validate(
        scope: AuthorityScopeV1,
        issuer_key_id: &[u8],
        public_key: &[u8],
    ) -> Result<Self, AuthorityIdentityError> {
        if !is_canonical_issuer_key_id(issuer_key_id) {
            return Err(AuthorityIdentityError::NonCanonicalIssuerKeyId);
        }
        let expected = Self::derive(scope, public_key)?;
        if issuer_key_id != expected.issuer_key_id.as_bytes() {
            return Err(AuthorityIdentityError::IssuerKeyIdMismatch);
        }
        Ok(expected)
    }

    pub fn issuer_key_id(&self) -> &str {
        &self.issuer_key_id
    }

    pub const fn public_key(&self) -> &[u8; DIGEST_LEN] {
        &self.public_key
    }

    pub fn identity(&self) -> &AuthorityLifecycleIdentityV1 {
        &self.identity
    }
}

fn is_canonical_issuer_key_id(candidate: &[u8]) -> bool {
    candidate.len() == ISSUER_KEY_ID_PREFIX.len() + LOWER_HEX_DIGEST_LEN
        && candidate.starts_with(ISSUER_KEY_ID_PREFIX.as_bytes())
        && candidate[ISSUER_KEY_ID_PREFIX.len()..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Untrusted issuer record components for duplicate/collision validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuerLifecycleRecordInputV1 {
    pub issuer_key_id: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl From<&IssuerLifecycleIdentityV1> for IssuerLifecycleRecordInputV1 {
    fn from(identity: &IssuerLifecycleIdentityV1) -> Self {
        Self {
            issuer_key_id: identity.issuer_key_id.as_bytes().to_vec(),
            public_key: identity.public_key.to_vec(),
        }
    }
}

/// Validate a canonical issuer set and reject every duplicate or claimed collision.
pub fn validate_issuer_lifecycle_records_v1(
    scope: AuthorityScopeV1,
    records: &[IssuerLifecycleRecordInputV1],
) -> Result<Vec<IssuerLifecycleIdentityV1>, AuthorityIdentityError> {
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        for other in &records[..index] {
            if record.issuer_key_id == other.issuer_key_id && record.public_key != other.public_key
            {
                return Err(AuthorityIdentityError::IssuerKeyIdCollision);
            }
        }
        if !ids.insert(record.issuer_key_id.as_slice()) {
            return Err(AuthorityIdentityError::DuplicateIssuerKeyId);
        }
        if !keys.insert(record.public_key.as_slice()) {
            return Err(AuthorityIdentityError::DuplicateIssuerPublicKey);
        }
    }

    records
        .iter()
        .map(|record| {
            IssuerLifecycleIdentityV1::validate(
                scope.clone(),
                &record.issuer_key_id,
                &record.public_key,
            )
        })
        .collect()
}
