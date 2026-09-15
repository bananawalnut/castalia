//! Contract for the opt-in strict live `dga1_` authority profile.
//!
//! The generic credential language remains intentionally broader. These tests
//! verify the separate resource-bound path without changing `Verifier::admit`.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use dregg_auth::{
    credential::{CREDENTIAL_PREFIX, Caveat, Credential, GatewayKey, Pred, RootKey},
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

fn bounded_profile(root: &RootKey, extra: impl IntoIterator<Item = Caveat>) -> String {
    let mut caveats = vec![
        attr("subject", "alice"),
        attr("operation", OPERATION),
        attr("resource", RESOURCE),
        first_party(Pred::NotAfter { at: VALID_UNTIL }),
    ];
    caveats.extend(extra);
    root.mint(caveats).encode()
}

fn nested_all_of(depth: usize) -> Pred {
    let mut predicate = Pred::NotAfter { at: VALID_UNTIL };
    for _ in 1..depth {
        predicate = Pred::AllOf(vec![predicate]);
    }
    predicate
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
    assert_eq!(
        authority.issuer_key_digest(),
        blake3::hash(&root.public().0).as_bytes()
    );
    assert_eq!(authority.credential_tail(), &expected_tail);
    assert_eq!(authority.verified_at(), NOW);
    assert_eq!(authority.reason_code(), "verified_live_authority");
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
fn operation_resource_and_time_substitution_reject() {
    let root = RootKey::from_seed([47; 32]);
    let token = exact_profile(&root).encode();
    let gate = Verifier::new(root.public().to_hex());

    for call in [
        Call::tool("gallery.card.delete").resource(RESOURCE).at(NOW),
        Call::tool(OPERATION)
            .resource("dregg://gallery/cards/alice/settings")
            .at(NOW),
        Call::tool(OPERATION).resource(RESOURCE).at(VALID_FROM - 1),
        Call::tool(OPERATION).resource(RESOURCE).at(VALID_UNTIL + 1),
    ] {
        assert!(
            gate.admit_resource_bound(&token, &call).is_err(),
            "operation, resource, expired, and future substitutions must reject"
        );
    }
}

#[test]
fn tight_interval_and_tail_bind_to_the_verified_presentation() {
    let root = RootKey::from_seed([48; 32]);
    let credential = root
        .mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            first_party(Pred::AttrPrefix {
                key: "resource".into(),
                prefix: RESOURCE_PREFIX.into(),
            }),
            first_party(Pred::NotBefore { at: 900 }),
            first_party(Pred::NotAfter { at: 1_100 }),
        ])
        .attenuate([
            attr("tool", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::Within {
                not_before: 950,
                not_after: 1_075,
            }),
            first_party(Pred::NotAfter { at: 1_050 }),
        ]);
    let expected_tail = credential.tail();
    let gate = Verifier::new(root.public().to_hex());

    let authority = gate
        .admit_resource_bound(&credential.encode(), &exact_call())
        .expect("every bound atom admits this exact presentation");

    assert_eq!(authority.valid_from(), 950);
    assert_eq!(authority.valid_until(), 1_050);
    assert_eq!(authority.issuer_public_key(), &root.public().0);
    assert_eq!(authority.credential_tail(), &expected_tail);
}

#[test]
fn malformed_wrong_issuer_contradictory_and_overflow_profiles_reject() {
    let root = RootKey::from_seed([49; 32]);
    let other_root = RootKey::from_seed([50; 32]);
    let token = exact_profile(&root).encode();

    assert!(
        Verifier::new(other_root.public().to_hex())
            .admit_resource_bound(&token, &exact_call())
            .is_err(),
        "a presentation cannot be substituted under another issuer"
    );
    assert!(
        Verifier::new("not-an-ed25519-key")
            .admit_resource_bound(&token, &exact_call())
            .is_err(),
        "a malformed configured issuer key must reject"
    );
    assert!(
        Verifier::new(root.public().to_hex())
            .admit_resource_bound("dga1_not-a-credential", &exact_call())
            .is_err(),
        "a malformed presentation must reject"
    );

    let mut signed_bytes = URL_SAFE_NO_PAD
        .decode(token.strip_prefix(CREDENTIAL_PREFIX).expect("v1 token"))
        .expect("credential body is base64url");
    signed_bytes[0] ^= 1;
    let tampered = format!(
        "{CREDENTIAL_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(signed_bytes)
    );
    assert!(
        Verifier::new(root.public().to_hex())
            .admit_resource_bound(&tampered, &exact_call())
            .is_err(),
        "tampering with the signed credential body must reject"
    );

    let refusal = Verifier::new(other_root.public().to_hex())
        .admit_resource_bound(&token, &exact_call())
        .expect_err("wrong issuer refuses");
    assert_eq!(refusal.to_string(), "live_authority_refused");
    assert_eq!(format!("{refusal:?}"), "LiveAuthorityError");

    for credential in [
        root.mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::Within {
                not_before: VALID_UNTIL,
                not_after: VALID_FROM,
            }),
        ]),
        root.mint([
            attr("subject", "alice"),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: u64::MAX }),
        ]),
    ] {
        assert!(
            Verifier::new(root.public().to_hex())
                .admit_resource_bound(&credential.encode(), &exact_call())
                .is_err(),
            "contradictory or sentinel-overflow validity must reject"
        );
    }
}

#[test]
fn strict_decode_enforces_block_and_caveat_boundaries() {
    let root = RootKey::from_seed([51; 32]);
    let gate = Verifier::new(root.public().to_hex());

    let mut thirty_two_blocks = exact_profile(&root);
    for _ in 1..32 {
        thirty_two_blocks = thirty_two_blocks.attenuate([]);
    }
    assert!(
        gate.admit_resource_bound(&thirty_two_blocks.encode(), &exact_call())
            .is_ok(),
        "32 blocks is the inclusive strict boundary"
    );
    let thirty_three_blocks = thirty_two_blocks.attenuate([]);
    assert!(
        gate.admit_resource_bound(&thirty_three_blocks.encode(), &exact_call())
            .is_err(),
        "33 blocks must reject before authority analysis"
    );

    let required = [
        attr("subject", "alice"),
        attr("operation", OPERATION),
        attr("resource", RESOURCE),
        first_party(Pred::NotAfter { at: VALID_UNTIL }),
    ];
    let mut at_limit = required.to_vec();
    at_limit.extend(
        std::iter::repeat_with(|| first_party(Pred::NotAfter { at: VALID_UNTIL }))
            .take(256 - required.len()),
    );
    assert!(
        gate.admit_resource_bound(&root.mint(at_limit.clone()).encode(), &exact_call())
            .is_ok(),
        "256 caveats is the inclusive strict boundary"
    );
    at_limit.push(first_party(Pred::NotAfter { at: VALID_UNTIL }));
    assert!(
        gate.admit_resource_bound(&root.mint(at_limit).encode(), &exact_call())
            .is_err(),
        "257 caveats must reject"
    );
}

#[test]
fn strict_decode_enforces_predicate_depth_and_node_boundaries() {
    let root = RootKey::from_seed([52; 32]);
    let gate = Verifier::new(root.public().to_hex());

    let depth_sixteen = bounded_profile(&root, [first_party(nested_all_of(16))]);
    assert!(
        gate.admit_resource_bound(&depth_sixteen, &exact_call())
            .is_ok(),
        "predicate depth 16 is the inclusive strict boundary"
    );
    let depth_seventeen = bounded_profile(&root, [first_party(nested_all_of(17))]);
    assert!(
        gate.admit_resource_bound(&depth_seventeen, &exact_call())
            .is_err(),
        "predicate depth 17 must reject"
    );

    // Four required predicate roots plus one AllOf root and 507/508 leaves.
    let nodes_512 = bounded_profile(
        &root,
        [first_party(Pred::AllOf(
            std::iter::repeat_n(Pred::NotAfter { at: VALID_UNTIL }, 507).collect(),
        ))],
    );
    assert!(
        gate.admit_resource_bound(&nodes_512, &exact_call()).is_ok(),
        "512 predicate nodes is the inclusive strict boundary"
    );
    let nodes_513 = bounded_profile(
        &root,
        [first_party(Pred::AllOf(
            std::iter::repeat_n(Pred::NotAfter { at: VALID_UNTIL }, 508).collect(),
        ))],
    );
    assert!(
        gate.admit_resource_bound(&nodes_513, &exact_call())
            .is_err(),
        "513 predicate nodes must reject"
    );
}

#[test]
fn strict_decode_enforces_string_and_encoded_boundaries_without_trimming() {
    let root = RootKey::from_seed([53; 32]);
    let gate = Verifier::new(root.public().to_hex());

    let subject_4096 = "a".repeat(4_096);
    let token_4096 = root
        .mint([
            attr("subject", &subject_4096),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ])
        .encode();
    assert!(
        gate.admit_resource_bound(&token_4096, &exact_call())
            .is_ok(),
        "a 4,096-byte string is the inclusive strict boundary"
    );

    let subject_4097 = "a".repeat(4_097);
    let token_4097 = root
        .mint([
            attr("subject", &subject_4097),
            attr("operation", OPERATION),
            attr("resource", RESOURCE),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ])
        .encode();
    assert!(
        gate.admit_resource_bound(&token_4097, &exact_call())
            .is_err(),
        "a 4,097-byte string must reject"
    );

    let token = exact_profile(&root).encode();
    assert!(gate.admit_resource_bound(&token, &exact_call()).is_ok());
    assert!(
        gate.admit_resource_bound(&format!("{token}\n"), &exact_call())
            .is_err(),
        "the strict path must not trim into a duplicate accepted boundary form"
    );

    for encoded_len in [65_536, 65_537] {
        let hostile = format!("{CREDENTIAL_PREFIX}{}", "A".repeat(encoded_len - 5));
        assert_eq!(hostile.len(), encoded_len);
        assert!(gate.admit_resource_bound(&hostile, &exact_call()).is_err());
    }
}

#[test]
fn strict_decode_rejects_noncanonical_duplicate_wire_forms_without_changing_legacy_decode() {
    let root = RootKey::from_seed([55; 32]);
    let token = exact_profile(&root).encode();
    let mut raw = URL_SAFE_NO_PAD
        .decode(token.strip_prefix(CREDENTIAL_PREFIX).expect("v1 token"))
        .expect("canonical body");
    assert_eq!(raw[32], 1, "the fixture has one credential block");
    raw.splice(32..33, [0x81, 0x00]);
    let duplicate_varint = format!("{CREDENTIAL_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw));

    assert!(
        Credential::decode(&duplicate_varint).is_ok(),
        "legacy structural decode remains byte-compatible"
    );
    assert!(
        Verifier::new(root.public().to_hex())
            .admit_resource_bound(&duplicate_varint, &exact_call())
            .is_err(),
        "strict decode rejects a non-shortest duplicate representation"
    );
}

#[test]
fn strict_failures_are_fixed_and_hostile_values_never_render() {
    let root = RootKey::from_seed([54; 32]);
    let marker = "HOSTILE_BEARER_subject_resource_secret_proof_key";
    let hostile = format!("{CREDENTIAL_PREFIX}{marker}");
    let refusal =
        match Verifier::new(root.public().to_hex()).admit_resource_bound(&hostile, &exact_call()) {
            Ok(_) => panic!("hostile malformed input must refuse"),
            Err(refusal) => refusal,
        };

    let display = refusal.to_string();
    let debug = format!("{refusal:?}");
    let json = serde_json::to_string(&refusal).expect("fixed error JSON is total");
    assert_eq!(display, "live_authority_refused");
    assert_eq!(debug, "LiveAuthorityError");
    assert_eq!(json, "\"live_authority_refused\"");
    for rendered in [&display, &debug, &json] {
        assert!(!rendered.contains(marker));
        assert!(!rendered.contains("alice"));
        assert!(!rendered.contains(RESOURCE));
    }

    let panic = std::panic::catch_unwind(|| panic!("{refusal}"))
        .expect_err("the test intentionally exercises panic formatting");
    let panic_text = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("panic payload is fixed text");
    assert_eq!(panic_text, "live_authority_refused");
    assert!(!panic_text.contains(marker));
}

fn raw_token(raw: &[u8]) -> String {
    format!("{CREDENTIAL_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw))
}

fn assert_fixed_refusal(gate: &Verifier, token: &str) {
    let result = std::panic::catch_unwind(|| gate.admit_resource_bound(token, &exact_call()));
    assert!(result.is_ok(), "untrusted input must not panic");
    let error = result.unwrap().expect_err("malformed input must refuse");
    assert_eq!(error.to_string(), "live_authority_refused");
    assert_eq!(format!("{error:?}"), "LiveAuthorityError");
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        "\"live_authority_refused\""
    );
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn hostile_lengths_overflow_truncation_and_trailing_bytes_refuse_without_panics() {
    let root = RootKey::from_seed([56; 32]);
    let gate = Verifier::new(root.public().to_hex());
    let token = exact_profile(&root).encode();
    let raw = URL_SAFE_NO_PAD.decode(&token[5..]).unwrap();
    for end in 0..raw.len() {
        assert_fixed_refusal(&gate, &raw_token(&raw[..end]));
    }
    let mut trailing = raw.clone();
    trailing.push(0);
    assert_fixed_refusal(&gate, &raw_token(&trailing));

    // Block count, caveat count, predicate discriminant, then key length.
    // Huge claimed allocations, overflow and non-shortest varints must be
    // refused by the allocation-free scan, even when the input is tiny.
    for offset in [32, 33, 35, 36] {
        for length in [
            vec![0xff; 10],
            vec![0xff; 9].into_iter().chain([2]).collect(),
            vec![0x80, 0x80, 0x40],
            vec![0x81, 0],
        ] {
            let mut hostile = raw[..offset].to_vec();
            hostile.extend(length);
            hostile.extend_from_slice(&raw[offset + 1..]);
            assert_fixed_refusal(&gate, &raw_token(&hostile));
        }
    }
    let mut invalid_utf8 = raw.clone();
    invalid_utf8[37] = 0xff;
    assert_fixed_refusal(&gate, &raw_token(&invalid_utf8));
    let mut wrong_proof = raw.clone();
    *wrong_proof.last_mut().unwrap() ^= 1;
    assert_fixed_refusal(&gate, &raw_token(&wrong_proof));
    // Partial base64 output followed by invalid input takes the guarded error path.
    let mut invalid_base64 = token.clone();
    invalid_base64.push('!');
    assert_fixed_refusal(&gate, &invalid_base64);
    assert_fixed_refusal(&gate, &format!("{token}="));
    assert_fixed_refusal(&gate, &format!(" {token}"));
    assert_fixed_refusal(&gate, &format!("dgd1_{}", &token[5..]));
}

#[test]
fn encoded_limit_is_inclusive_for_a_valid_signed_presentation() {
    let root = RootKey::from_seed([57; 32]);
    let gate = Verifier::new(root.public().to_hex());
    let resource = format!(
        "dregg://gallery/cards/{}",
        vec!["a".repeat(120); 32].join("/")
    );
    let build = |subject_len| {
        let mut caveats = vec![
            attr("subject", &"s".repeat(subject_len)),
            attr("operation", OPERATION),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ];
        caveats.extend(std::iter::repeat_with(|| attr("resource", &resource)).take(12));
        root.mint(caveats).encode()
    };
    // Adjust only a legal subject string to make raw length exactly 49,148.
    let sample = build(2_000);
    let raw_len = URL_SAFE_NO_PAD.decode(&sample[5..]).unwrap().len();
    let subject_len = 2_000 + 49_148 - raw_len;
    assert!((128..=4_096).contains(&subject_len));
    let at_limit = build(subject_len);
    assert_eq!(at_limit.len(), 65_536);
    let call = Call::tool(OPERATION).resource(&resource).at(NOW);
    assert!(gate.admit_resource_bound(&at_limit, &call).is_ok());
    let over_limit = build(subject_len + 1);
    assert!(over_limit.len() > 65_536);
    assert!(gate.admit_resource_bound(&over_limit, &call).is_err());
}

#[test]
fn strict_success_and_semantic_failures_render_only_fixed_codes() {
    let root = RootKey::from_seed([58; 32]);
    let gate = Verifier::new(root.public().to_hex());
    let token = exact_profile(&root).encode();
    let authority = gate.admit_resource_bound(&token, &exact_call()).unwrap();
    assert_eq!(
        format!("{authority:?}"),
        "VerifiedLiveAuthority(verified_live_authority)"
    );
    assert_fixed_refusal(
        &Verifier::new(RootKey::from_seed([59; 32]).public().to_hex()),
        &token,
    );
    assert_fixed_refusal(&Verifier::new("invalid issuer"), &token);
    for extra in [
        attr("unknown", "PRIVATE_TEST_MARKER"),
        first_party(Pred::NotAfter { at: NOW - 1 }),
        attr("resource", "dregg://gallery/private/marker"),
    ] {
        assert_fixed_refusal(&gate, &bounded_profile(&root, [extra]));
    }
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
        "impl From<Credential> for VerifiedLiveAuthority",
        "pub fn credential(",
        "pub fn token(",
        "pub fn caveats(",
        "pub fn receipt(",
        "pub fn resource_prefix(",
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

#[test]
fn legacy_tool_grants_remain_compatible_and_strict_refusal_has_no_fallback() {
    use dregg_auth::policy::{Grant, Policy};
    let policy = Policy::generate();
    let token = policy
        .issue(
            Grant::to("legacy-holder")
                .tools(["read", "write"])
                .until(VALID_UNTIL),
        )
        .unwrap()
        .encode();
    let gate = Verifier::new(policy.public_key_hex());
    let call = Call::tool("read").resource(RESOURCE).at(NOW);
    assert!(gate.admit(&token, &call).admitted());
    assert!(gate.admit_resource_bound(&token, &call).is_err());
    assert!(gate.admit(&format!(" {token}\n"), &call).admitted());
    assert!(
        gate.admit_resource_bound(&format!(" {token}\n"), &call)
            .is_err()
    );
    assert!(!gate.admit(&token, &Call::tool("delete").at(NOW)).admitted());
}

#[test]
fn request_arguments_are_advisory_and_every_live_verification_rebinds_time() {
    let root = RootKey::from_seed([60; 32]);
    let token = exact_profile(&root).encode();
    let gate = Verifier::new(root.public().to_hex());
    let call = exact_call()
        .arg("subject", "caller-claim")
        .arg("operation", "delete")
        .arg("resource", "caller-resource")
        .arg("clock", "0");
    let authority = gate.admit_resource_bound(&token, &call).unwrap();
    assert_eq!(authority.subject(), "alice");
    assert_eq!(authority.operation(), OPERATION);
    assert_eq!(authority.resource(), RESOURCE);
    assert!(
        gate.admit_resource_bound(&token, &exact_call().at(VALID_FROM))
            .is_ok()
    );
    assert!(
        gate.admit_resource_bound(&token, &exact_call().at(VALID_UNTIL))
            .is_ok()
    );
    assert!(
        gate.admit_resource_bound(&token, &exact_call().at(VALID_UNTIL + 1))
            .is_err()
    );
    let missing_resource = Call::tool(OPERATION).at(NOW).arg("resource", RESOURCE);
    assert!(
        gate.admit_resource_bound(&token, &missing_resource)
            .is_err()
    );
}

#[test]
fn canonical_request_grammar_never_normalizes_aliases() {
    let root = RootKey::from_seed([61; 32]);
    let gate = Verifier::new(root.public().to_hex());
    for operation in [
        "Gallery.card.read",
        "gallery..read",
        ".read",
        "read.",
        "gallery/card/read",
        "1gallery.card.read",
        "gallery.card-read",
        "gallery.card.read ",
    ] {
        let token = root
            .mint([
                attr("subject", "alice"),
                attr("operation", operation),
                attr("resource", RESOURCE),
                first_party(Pred::NotAfter { at: VALID_UNTIL }),
            ])
            .encode();
        assert!(
            gate.admit_resource_bound(&token, &Call::tool(operation).resource(RESOURCE).at(NOW))
                .is_err()
        );
    }
    for resource in [
        "DREGG://gallery/cards/alice/profile",
        "dregg://Gallery/cards/alice/profile",
        "dregg://gallery/cards/./alice/profile",
        "dregg://gallery/cards/../alice/profile",
        "dregg://gallery//cards/alice/profile",
        "dregg://gallery/cards/alice/profile/",
        "dregg://gallery/cards/alice/%70rofile",
        "dregg://gallery/cards/alice/profile?q=1",
        "dregg://gallery/cards/alice/profile#part",
        "dregg://gallery/cards/alice/πrofile",
        "dregg://gallery/cards/alice/profile\n",
        "dregg://gallery/cards",
    ] {
        let token = root
            .mint([
                attr("subject", "alice"),
                attr("operation", OPERATION),
                attr("resource", resource),
                first_party(Pred::NotAfter { at: VALID_UNTIL }),
            ])
            .encode();
        assert!(
            gate.admit_resource_bound(&token, &exact_call().resource(resource))
                .is_err()
        );
    }
}

#[test]
fn canonical_operation_and_resource_byte_limits_are_inclusive() {
    let root = RootKey::from_seed([62; 32]);
    let gate = Verifier::new(root.public().to_hex());
    let operation = format!("{}.{}", vec!["a".repeat(32); 3].join("."), "a".repeat(29));
    let resource = format!(
        "dregg://w/n/{}/{}",
        vec!["a".repeat(127); 31].join("/"),
        "a".repeat(116)
    );
    assert_eq!(operation.len(), 128);
    assert_eq!(resource.len(), 4_096);
    let sign = |op: &str, res: &str| {
        root.mint([
            attr("subject", "alice"),
            attr("operation", op),
            attr("resource", res),
            first_party(Pred::NotAfter { at: VALID_UNTIL }),
        ])
        .encode()
    };
    assert!(
        gate.admit_resource_bound(
            &sign(&operation, &resource),
            &Call::tool(&operation).resource(&resource).at(NOW)
        )
        .is_ok()
    );
    let over_operation = format!("{operation}a");
    assert!(
        gate.admit_resource_bound(
            &sign(&over_operation, &resource),
            &Call::tool(&over_operation).resource(&resource).at(NOW)
        )
        .is_err()
    );
    let over_resource = format!("{resource}a");
    assert!(
        gate.admit_resource_bound(
            &sign(&operation, &over_resource),
            &Call::tool(&operation).resource(&over_resource).at(NOW)
        )
        .is_err()
    );
}
