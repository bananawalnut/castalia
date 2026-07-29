//! RED contract for the opt-in strict live `dga1_` authority profile.
//!
//! The generic credential language remains intentionally broader. These tests
//! define the separate resource-bound verifier path that later C02-C04 commits
//! must implement without changing `Verifier::admit`.

use dregg_auth::{
    credential::{Caveat, GatewayKey, Pred, RootKey},
    policy::{Call, Verifier},
};

const NOW: u64 = 1_000;
const VALID_FROM: u64 = 900;
const VALID_UNTIL: u64 = 1_100;
const OPERATION: &str = "gallery.card.read";
const RESOURCE: &str = "dregg://gallery/cards/alice/profile";
const RESOURCE_PREFIX: &str = "dregg://gallery/cards/alice/";

fn first_party(pred: Pred) -> Caveat {
    Caveat::FirstParty(pred)
}

fn attr(key: &str, value: &str) -> Caveat {
    first_party(Pred::AttrEq {
        key: key.into(),
        value: value.into(),
    })
}

fn exact_profile(root: &RootKey) -> dregg_auth::credential::Credential {
    root.mint([
        attr("subject", "alice"),
        attr("operation", OPERATION),
        attr("resource", RESOURCE),
        first_party(Pred::Within {
            not_before: VALID_FROM,
            not_after: VALID_UNTIL,
        }),
    ])
}

fn exact_call() -> Call {
    Call::tool(OPERATION).resource(RESOURCE).at(NOW)
}

#[test]
fn valid_exact_profile_returns_credential_bound_authority() {
    let root = RootKey::from_seed([41; 32]);
    let credential = exact_profile(&root);
    let expected_tail = credential.tail();
    let token = credential.encode();
    let gate = Verifier::new(root.public().to_hex());

    let authority = gate
        .admit_resource_bound(&token, &exact_call())
        .expect("the exact positive finite profile must verify");

    assert_eq!(authority.subject(), "alice");
    assert_eq!(authority.operation(), OPERATION);
    assert_eq!(authority.resource(), RESOURCE);
    assert_eq!(authority.valid_from(), VALID_FROM);
    assert_eq!(authority.valid_until(), VALID_UNTIL);
    assert_eq!(authority.issuer_public_key(), &root.public().0);
    assert_eq!(authority.credential_tail(), &expected_tail);
    assert_eq!(authority.verified_at(), NOW);
    assert!(!authority.reason_code().is_empty());
    assert!(
        authority
            .reason_code()
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
        "the success reason is a fixed redacted code"
    );
}

#[test]
fn issuer_block_requires_one_unique_nonempty_subject() {
    let root = RootKey::from_seed([42; 32]);
    let profiles = [
        root.mint([
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
        root.mint([
            attr("subject", ""),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
        root.mint([
            attr("subject", "alice"),
            attr("subject", "alice"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
        root.mint([
            attr("subject", "alice"),
            attr("subject", "mallory"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
    ];
    let gate = Verifier::new(root.public().to_hex());

    for credential in profiles {
        assert!(
            gate.admit_resource_bound(&credential.encode(), &exact_call())
                .is_err(),
            "missing, empty, duplicate, and conflicting root subjects must reject"
        );
    }
}

#[test]
fn broad_prefix_is_projected_only_to_an_attenuated_exact_resource() {
    let root = RootKey::from_seed([43; 32]);
    let credential = root
        .mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            first_party(Pred::AttrPrefix {
                key: "resource".into(),
                prefix: RESOURCE_PREFIX.into(),
            }),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ])
        .attenuate([attr("resource", RESOURCE)]);
    let token = credential.encode();
    let gate = Verifier::new(root.public().to_hex());

    let authority = gate
        .admit_resource_bound(&token, &exact_call())
        .expect("the exact child resource is inside both caveats");
    assert_eq!(authority.resource(), RESOURCE);
    assert_ne!(authority.resource(), RESOURCE_PREFIX);

    let sibling = Call::tool(OPERATION)
        .resource("dregg://gallery/cards/alice/settings")
        .at(NOW);
    assert!(gate.admit_resource_bound(&token, &sibling).is_err());
}

#[test]
fn boolean_third_party_and_unknown_authority_shapes_reject() {
    let root = RootKey::from_seed([44; 32]);
    let gateway = GatewayKey::from_seed([45; 32]);
    let forbidden = [
        root.mint([
            attr("subject", "alice"),
            first_party(Pred::AnyOf(vec![Pred::AttrEq {
                key: "operation".into(),
                value: OPERATION.into(),
            }])),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
        root.mint([
            attr("subject", "alice"),
            first_party(Pred::Not(Box::new(Pred::AttrEq {
                key: "operation".into(),
                value: "gallery.card.delete".into(),
            }))),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
        root.mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
            Caveat::ThirdParty {
                gateway: gateway.public().0,
                caveat_id: b"approval".to_vec(),
                hint: String::new(),
            },
        ]),
        root.mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            attr("tenant", "internal"),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ]),
    ];
    let gate = Verifier::new(root.public().to_hex());

    for credential in forbidden {
        assert!(
            gate.admit_resource_bound(&credential.encode(), &exact_call())
                .is_err(),
            "the strict profile must reject AnyOf, Not, third-party, and unknown-key shapes"
        );
    }
}

#[test]
fn missing_resource_or_finite_time_rejects() {
    let root = RootKey::from_seed([46; 32]);
    let token = exact_profile(&root).encode();
    let gate = Verifier::new(root.public().to_hex());

    assert!(
        gate.admit_resource_bound(&token, &Call::tool(OPERATION).at(NOW))
            .is_err(),
        "the request must bind one exact resource"
    );
    assert!(
        gate.admit_resource_bound(&token, &Call::tool(OPERATION).resource(RESOURCE))
            .is_err(),
        "the authority clock must be supplied"
    );

    let no_expiry = root
        .mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotBefore { at: VALID_FROM }),
        ])
        .encode();
    assert!(
        gate.admit_resource_bound(&no_expiry, &exact_call())
            .is_err(),
        "a finite upper validity bound is mandatory"
    );
}

#[test]
fn verified_authority_is_not_promoted_to_a_public_dto() {
    let policy = include_str!("../src/policy.rs");
    let declaration = policy
        .find("pub struct VerifiedLiveAuthority")
        .expect("the strict path must expose the sealed verified result type");
    let struct_body = braced_item(&policy[declaration..]);

    assert!(
        struct_body
            .lines()
            .skip(1)
            .all(|line| !line.trim_start().starts_with("pub ")),
        "VerifiedLiveAuthority fields must remain private"
    );

    let before = &policy[declaration.saturating_sub(256)..declaration];
    let derive = before.rsplit("#[derive(").next().unwrap_or_default();
    for forbidden in ["Default", "Deserialize", "Serialize"] {
        assert!(
            !derive.contains(forbidden),
            "VerifiedLiveAuthority must not derive {forbidden}"
        );
    }
    for forbidden in [
        "impl Default for VerifiedLiveAuthority",
        "impl<'de> Deserialize<'de> for VerifiedLiveAuthority",
        "impl Deserialize for VerifiedLiveAuthority",
        "impl From<Call> for VerifiedLiveAuthority",
        "impl From<Receipt> for VerifiedLiveAuthority",
        "pub fn credential(",
        "pub fn new(",
    ] {
        assert!(
            !policy[declaration..].contains(forbidden),
            "sealed result exposes forbidden DTO/construction surface: {forbidden}"
        );
    }
}

fn braced_item(source: &str) -> &str {
    let start = source.find('{').expect("item has an opening brace");
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[start..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[start..=start + offset];
                }
            }
            _ => {}
        }
    }
    panic!("item has a closing brace");
}
