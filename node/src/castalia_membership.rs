//! Production composition for the authority-bound Castalia membership factory.
//!
//! Genesis pins one institutional authority. Every fresh node executor rebuilds
//! the exact descriptor from that authority and deploys it with conflict refusal.
//! This module never births a member cell and exposes no HTTP route.

use dregg_cell::FactoryRegistry;
use starbridge_castalia_membership::{MembershipBirthError, castalia_membership_factory};

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

#[cfg(test)]
mod tests {
    use super::*;
    use dregg_cell::factory::FactoryDescriptor;

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
}
