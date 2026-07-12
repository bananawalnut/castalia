# WirePresentation fixture provenance

Issue: Castalia #1 / I05-Castalia replacement

The focused integration test generates the fixture in process from deterministic,
public test inputs and immediately verifies the serialized wire form:

```bash
cargo test -p dregg-credentials --test wire_presentation_seam -- --nocapture
```

The fixture path is the checked generator and verifier test, not a committed opaque
JSON blob. This avoids pinning incidental proof metadata while preserving a
reviewable source for every input.

Allowed serialized content:

- the typed `WirePresentationProof`;
- disclosed attributes selected by the holder;
- typed predicate proofs;
- the anonymous-mode bit.

Verifier-owned expectations, including the trusted federation root, request
app/action/audience, time window, evidence tier/state, and pinned public-input
fingerprint, stay outside the artifact. The fixture must not contain
`Presentation`, `AuthorizationTrace`, raw credentials, detached raw proof bytes,
wallet or holder identifiers, source tokens, private witnesses, or operator
material.

This fixture demonstrates only the Castalia wire boundary. It is not secS adapter
evidence and does not establish proof verification, revocation freshness,
unlinkability, live authority, federation/finality, or production readiness.

## Negative fixture matrix

`credentials/tests/wire_presentation_seam.rs` keeps these rejection cases
executable:

| Input | Expected result |
|---|---|
| malformed JSON | `VerificationError::MalformedWire` |
| unknown envelope field/version | `VerificationError::MalformedWire` |
| trace/private holder field | `VerificationError::MalformedWire` |
| missing verifier-owned federation root | `VerificationError::MissingExpectedFederationRoot` |
| mismatched verifier-owned federation root | `VerificationError::FederationRootMismatch` |
| wrong app/action/audience | `VerificationError::RequestBindingMismatch` |
| context or nonce expectation | `VerificationError::UnsupportedWireBinding` |
| proof timestamp outside verifier window | `VerificationError::ProofExpired` / `ProofFromFuture` |
| stronger evidence tier/state required | `EvidenceTierMismatch` / `VerificationStateMismatch` |
| caller-mutated `verification = Valid` | `UntrustedWireVerificationState` |
| changed proof public inputs | `PublicInputsFingerprintMismatch` |

Unknown versions are not guessed or silently normalized. A future version needs
an explicit typed envelope and its own reviewed compatibility policy.

The current fixture has no context or nonce public input and carries only the
existing local constraint-check posture. Those semantics remain explicitly
unsupported rather than inferred from adjacent JSON. The verifier must pin the
expected public-input fingerprint independently; computing it from the artifact
being verified is not replay protection.
