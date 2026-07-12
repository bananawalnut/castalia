# Castalia WirePresentation seam

Issue: I05-Castalia replacement / Castalia #1

Owning repository: `bananawalnut/castalia`

## Implemented boundary

Castalia exposes a wire-only credential presentation boundary:

- holder-private state: `dregg_credentials::Presentation`;
- conversion: `Presentation::to_wire()`;
- wire type: `dregg_credentials::WirePresentation`;
- typed verification entry: `dregg_credentials::verify_wire`;
- JSON verification entry: `dregg_credentials::verify_wire_json`;
- verifier expectations: `dregg_credentials::VerificationOptions`;
- checked projection: `dregg_credentials::VerifiedPresentation`;
- typed failures: `dregg_credentials::VerificationError`.

The JSON envelope denies unknown fields. Both verification entries now require
verifier-owned expectations for the federation root, app/action/audience tuple,
proof-time window, evidence tier/state, and a pinned fingerprint of the complete
circuit public-input tuple. The app identifier is the current circuit's resource
and audience binding. The action/app pair is recomputed against
`request_predicate`; the timestamp is checked against verifier time and maximum
age; federation root and the full public-input fingerprint are compared exactly.

The current wire proof model has no independent context identifier or
challenge/nonce public input. Supplying either expectation therefore returns
`UnsupportedWireBinding` rather than pretending replay/context semantics are
bound. Likewise, this seam does not independently run the cryptographic proof
verifier: it accepts only the existing `LocalOnly` constraint-check fixture and
rejects a wire-carried `Valid` verdict as untrusted. A consumer requiring a
cryptographically verified tier receives a fail-closed tier/state mismatch.
Callers receive only verifier-derived disclosure, federation context, and the
anonymous-mode flag.

## Consumer contract

A later secS adapter may supply only:

1. a typed or serialized `WirePresentation`; and
2. verifier-owned public expectations.

The public-input fingerprint must be pinned by the verifier's request/session;
deriving the expected value from the received artifact would not establish a
replay or challenge binding.

It must not request or persist `Presentation`, `AuthorizationTrace`, credentials,
wallet/holder identifiers, source tokens, or private witnesses. It must not trust
a caller-supplied verdict alongside the artifact.

Focused reproduction:

```bash
cargo test -p dregg-credentials --test wire_presentation_seam -- --nocapture
cargo test -p dregg-credentials
```

Fixture provenance and the negative matrix are recorded in
`credentials/fixtures/wire_presentation/README.md`.

## Non-claims

This narrow seam does not implement secS behavior, replay/nonce or arbitrary
context binding, live authority, revocation freshness, federation/finality, new
proof verification, privacy/unlinkability, light-client or recursive-proof state,
selective audit, or production readiness.
The current non-anonymous local fixture exercises the existing local
constraint-check posture; it is not evidence of remote cryptographic verification.
