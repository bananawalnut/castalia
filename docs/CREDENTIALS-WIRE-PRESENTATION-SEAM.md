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

The JSON envelope denies unknown fields. Both verification entries require an
externally trusted federation root and compare it to the proof's bound public
input. Callers receive only verifier-derived disclosure, federation context, and
the anonymous-mode flag.

## Consumer contract

A later secS adapter may supply only:

1. a typed or serialized `WirePresentation`; and
2. verifier-owned public expectations.

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

This narrow seam does not implement secS behavior, live authority, revocation
freshness, federation/finality, new proof verification, privacy/unlinkability,
light-client or recursive-proof state, selective audit, or production readiness.
The current non-anonymous local fixture exercises the existing local
constraint-check posture; it is not evidence of remote cryptographic verification.
