use dregg_credentials::{
    AttrValue, CredentialAttributes, CredentialSchema, IssuerKeys, PresentationOptions,
    VerificationError, VerificationOptions, WireEvidenceTier, WirePresentation,
    WireVerificationState, issue, present, verify_wire, verify_wire_json,
};
use dregg_token::AuthRequest;
use serde_json::Value;

fn fixture_issuer() -> IssuerKeys {
    IssuerKeys::new(
        [11u8; 32],
        [
            33, 181, 62, 99, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ],
        b"wire-seam-test-kid",
        "wire-seam-test-issuer",
    )
}

fn fixture_schema() -> CredentialSchema {
    CredentialSchema::new(
        "wire-employee-v1",
        vec!["department".into(), "clearance_level".into()],
    )
}

fn fixture_request() -> AuthRequest {
    AuthRequest {
        action: Some("api:read".into()),
        app_id: Some("employee-portal".into()),
        now: Some(1_700_000_000),
        ..Default::default()
    }
}

fn make_wire_json() -> (String, VerificationOptions) {
    let issuer = fixture_issuer();
    let schema = fixture_schema();
    let credential = issue(
        &issuer,
        &schema,
        [77u8; 32],
        CredentialAttributes::new()
            .with("department", AttrValue::Text("Engineering".into()))
            .with("clearance_level", AttrValue::Integer(3)),
        1_700_000_000,
        None,
    )
    .expect("issue fixture credential");
    let presentation = present(
        &credential,
        &fixture_request(),
        &PresentationOptions::new().disclose("department"),
    )
    .expect("present fixture credential");
    let wire = presentation.to_wire();
    let public_inputs_fingerprint = wire.public_inputs_fingerprint();
    let json = serde_json::to_string_pretty(&wire).expect("serialize wire form");
    let options = VerificationOptions {
        expected_schema: Some(schema),
        expected_disclosure: vec!["department".into()],
        expected_federation_root: Some(issuer.federation_root),
        expected_app_id: Some("employee-portal".into()),
        expected_action: Some("api:read".into()),
        expected_audience: Some("employee-portal".into()),
        verification_time: Some(1_700_000_030),
        max_proof_age_secs: Some(60),
        expected_evidence_tier: Some(WireEvidenceTier::LocalConstraintCheck),
        expected_wire_state: Some(WireVerificationState::LocalConstraintChecked),
        expected_public_inputs_fingerprint: Some(public_inputs_fingerprint),
        ..Default::default()
    };
    (json, options)
}

#[test]
fn request_app_mutation_rejects() {
    let (json, mut options) = make_wire_json();
    options.expected_app_id = Some("other-app".into());
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::RequestBindingMismatch { field: "app_id" })
    ));
}

#[test]
fn request_action_mutation_rejects() {
    let (json, mut options) = make_wire_json();
    options.expected_action = Some("api:write".into());
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::RequestBindingMismatch { field: "action" })
    ));
}

#[test]
fn audience_mutation_rejects() {
    let (json, mut options) = make_wire_json();
    options.expected_audience = Some("other-audience".into());
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::RequestBindingMismatch { field: "audience" })
    ));
}

#[test]
fn unsupported_context_and_nonce_semantics_fail_closed() {
    let (json, mut options) = make_wire_json();
    options.expected_context = Some("tenant:finance".into());
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::UnsupportedWireBinding { field: "context" })
    ));

    options.expected_context = None;
    options.expected_nonce = Some([7u8; 32]);
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::UnsupportedWireBinding { field: "nonce" })
    ));
}

#[test]
fn expiry_time_mutation_rejects() {
    let (json, mut options) = make_wire_json();
    options.verification_time = Some(1_700_000_061);
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::ProofExpired)
    ));
}

#[test]
fn evidence_tier_and_verification_state_mutations_reject() {
    let (json, mut options) = make_wire_json();
    options.expected_evidence_tier = Some(WireEvidenceTier::CryptographicProof);
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::EvidenceTierMismatch)
    ));

    options.expected_evidence_tier = Some(WireEvidenceTier::LocalConstraintCheck);
    options.expected_wire_state = Some(WireVerificationState::CryptographicallyVerified);
    assert!(matches!(
        verify_wire_json(&json, &options),
        Err(VerificationError::VerificationStateMismatch)
    ));

    let mut fixture: Value = serde_json::from_str(&json).expect("parse fixture JSON");
    fixture["proof"]["verification"] = Value::String("Valid".into());
    options.expected_wire_state = Some(WireVerificationState::LocalConstraintChecked);
    assert!(matches!(
        verify_wire_json(&fixture.to_string(), &options),
        Err(VerificationError::UntrustedWireVerificationState)
    ));
}

#[test]
fn proof_public_inputs_mutation_rejects() {
    let (json, options) = make_wire_json();
    let mut fixture: Value = serde_json::from_str(&json).expect("parse fixture JSON");
    fixture["proof"]["circuit_proof"]["public_inputs"]["timestamp"] = Value::from(1);
    assert!(matches!(
        verify_wire_json(&fixture.to_string(), &options),
        Err(VerificationError::PublicInputsFingerprintMismatch)
    ));
}

#[test]
fn serialized_wire_form_excludes_holder_private_state() {
    let (json, _) = make_wire_json();
    let forbidden = [
        "AuthorizationTrace",
        "raw_credential",
        "raw_proof_bytes",
        "wallet_id",
        "holder_id",
        "private_witness",
        "source_token",
        "root_key",
        "\"trace\"",
    ];

    for sentinel in forbidden {
        assert!(!json.contains(sentinel), "wire JSON leaked `{sentinel}`");
    }
    assert!(json.contains("department"));
    assert!(json.contains("Engineering"));
}

#[test]
fn verifier_returns_only_checked_disclosure_and_public_context() {
    let (json, options) = make_wire_json();
    let verified = verify_wire_json(&json, &options).expect("wire verification");

    assert_eq!(verified.disclosed.len(), 1);
    assert_eq!(verified.disclosed[0].0, "department");
    assert_eq!(
        verified.federation_root,
        options.expected_federation_root.unwrap()
    );
    assert!(!verified.anonymous);
}

#[test]
fn verifier_requires_a_trusted_federation_root() {
    let (json, mut options) = make_wire_json();
    let wire: WirePresentation = serde_json::from_str(&json).expect("parse wire form");

    options.expected_federation_root = None;
    assert!(matches!(
        verify_wire(&wire, &options),
        Err(VerificationError::MissingExpectedFederationRoot)
    ));

    options.expected_federation_root = Some([42u8; 32]);
    assert!(matches!(
        verify_wire(&wire, &options),
        Err(VerificationError::FederationRootMismatch { .. })
    ));
}

#[test]
fn malformed_unknown_and_private_fields_fail_closed() {
    let (json, options) = make_wire_json();

    assert!(matches!(
        verify_wire_json("{not json", &options),
        Err(VerificationError::MalformedWire(_))
    ));

    for field in [
        "trace",
        "wire_version",
        "raw_credential",
        "raw_proof_bytes",
        "wallet_id",
        "holder_id",
        "private_witness",
        "source_token",
    ] {
        let mut fixture: Value = serde_json::from_str(&json).expect("parse fixture JSON");
        fixture[field] = Value::String("forbidden-or-unsupported".into());
        assert!(matches!(
            verify_wire_json(&fixture.to_string(), &options),
            Err(VerificationError::MalformedWire(_))
        ));
    }
}
