//! Wire format: **postcard, versioned by prefix, base64url for headers**.
//!
//! The byte form is [postcard](https://docs.rs/postcard) (compact, canonical
//! for a fixed schema — the property the signed digests rely on). The string
//! form is the version prefix plus base64url (no padding) of the postcard
//! bytes, safe for HTTP headers / CLI args / env vars:
//!
//! * credential: `dga1_<base64url>`
//! * discharge:  `dgd1_<base64url>`
//!
//! The prefix IS the version: a breaking schema change bumps it (`dga2_`), and
//! a decoder never guesses — an unknown prefix is an error, not a fallback.
//! The golden-vector discipline applies to any binding (sdk-py/sdk-ts wrap
//! this exact byte schema; vectors in `tests/`).
//!
//! A decoded credential is structurally validated (signature lengths, a
//! non-empty chain, the carried proof key matching the tail block) before it
//! is handed back; cryptographic validity is still — always — decided by
//! [`Credential::verify`].

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use super::caveat::Caveat;
use super::chain::{Block, Credential, Discharge};
use super::pred::Pred;

/// Version prefix of an encoded credential.
pub const CREDENTIAL_PREFIX: &str = "dga1_";
/// Version prefix of an encoded discharge.
pub const DISCHARGE_PREFIX: &str = "dgd1_";

/// A credential or discharge failed to decode.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    /// The string does not carry a known version prefix.
    #[error("unknown wire prefix (expected `{expected}`)")]
    Prefix {
        /// The prefix this decoder accepts.
        expected: &'static str,
    },
    /// The payload is not valid base64url.
    #[error("payload is not base64url: {0}")]
    Base64(String),
    /// The bytes do not parse as the versioned postcard schema.
    #[error("payload does not match the v1 schema: {0}")]
    Schema(String),
    /// The structure is schema-valid but internally inconsistent.
    #[error("malformed credential: {0}")]
    Malformed(&'static str),
}

#[derive(Serialize, Deserialize)]
struct BlockWire {
    caveats: Vec<Caveat>,
    next_pub: [u8; 32],
    /// 64 signature bytes (postcard byte-seq; length checked on decode).
    sig: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct CredentialWire {
    nonce: [u8; 32],
    blocks: Vec<BlockWire>,
    /// The tail (proof-of-possession / attenuation) key seed — what makes the
    /// encoded form a BEARER credential.
    proof_seed: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct DischargeWire {
    caveat_id: Vec<u8>,
    caveats: Vec<Pred>,
    binding: Option<[u8; 32]>,
    /// 64 signature bytes.
    sig: Vec<u8>,
}

const MAX_LIVE_ENCODED_BYTES: usize = 65_536;
const MAX_LIVE_RAW_BYTES: usize = 49_148;
const MAX_LIVE_BLOCKS: usize = 32;
const MAX_LIVE_CAVEATS: usize = 256;
const MAX_LIVE_PRED_DEPTH: usize = 16;
const MAX_LIVE_PRED_NODES: usize = 512;
const MAX_LIVE_STRING_BYTES: usize = 4_096;
const MAX_LIVE_DECODED_ALLOCATION: usize = 1024 * 1024;

/// Strict decoding has zero third-party discharges: a credential wire carries
/// none, and the strict verification context constructs none. Third-party
/// caveats are rejected by this preflight before postcard allocation.
///
/// Copy/erasure inventory: the caller's borrowed encoded input cannot be
/// overwritten here. Base64 writes into one guarded raw allocation, including
/// on invalid-input errors. Postcard's bounded strings/vectors move into the
/// guarded credential; its temporary proof seed is wiped on every exit.
/// Safe overwrite uses fill plus black_box (best effort, not a cryptographic
/// erasure guarantee). Serde temporaries, compiler/stack copies, dalek's internal
/// key copy, and chain.rs's bounded postcard digest scratch are dependency/API
/// copies without an erasure hook here. No raw-value renderer is called.
struct LiveRawBytes(Vec<u8>);

fn overwrite(bytes: &mut [u8]) {
    bytes.fill(0);
    std::hint::black_box(bytes);
}

impl Drop for LiveRawBytes {
    fn drop(&mut self) {
        overwrite(&mut self.0);
    }
}

fn erase_pred(pred: &mut Pred) {
    fn erase_string(value: &mut String) {
        let mut bytes = std::mem::take(value).into_bytes();
        overwrite(&mut bytes);
    }
    match pred {
        Pred::AttrEq { key, value } => {
            erase_string(key);
            erase_string(value);
        }
        Pred::AttrPrefix { key, prefix } => {
            erase_string(key);
            erase_string(prefix);
        }
        Pred::AllOf(children) | Pred::AnyOf(children) => {
            for child in children {
                erase_pred(child);
            }
        }
        Pred::Not(child) => erase_pred(child),
        _ => {}
    }
}

fn erase_caveats(caveats: &mut [Caveat]) {
    for caveat in caveats {
        // Strict preflight permits only first-party predicates, depth <= 16.
        if let Caveat::FirstParty(pred) = caveat {
            erase_pred(pred);
        }
    }
}

struct LiveDecodedWire(CredentialWire);

impl Drop for LiveDecodedWire {
    fn drop(&mut self) {
        overwrite(&mut self.0.proof_seed);
        for block in &mut self.0.blocks {
            erase_caveats(&mut block.caveats);
        }
    }
}

/// Internal borrow-only guard: strict verification cannot escape with a raw
/// credential, and all decoded predicate strings are overwritten on refusal
/// and success. The dalek SigningKey retains its own dependency-owned Drop.
pub(crate) struct LiveCredential(Credential);

impl std::ops::Deref for LiveCredential {
    type Target = Credential;

    fn deref(&self) -> &Credential {
        &self.0
    }
}

impl Drop for LiveCredential {
    fn drop(&mut self) {
        for block in &mut self.0.blocks {
            erase_caveats(&mut block.caveats);
        }
    }
}

struct LiveWirePreflight<'a> {
    bytes: &'a [u8],
    cursor: usize,
    caveats: usize,
    pred_nodes: usize,
    allocation: usize,
}

impl<'a> LiveWirePreflight<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            cursor: 0,
            caveats: 0,
            pred_nodes: 0,
            allocation: 0,
        }
    }

    fn credential(mut self) -> Result<(), ()> {
        // Reserve four raw-buffer equivalents for base64 rounding, the raw
        // bytes, and bounded postcard digest scratch used by chain verification.
        self.charge(self.bytes.len())?;
        self.take(32)?;
        let blocks = self.length()?;
        if !(1..=MAX_LIVE_BLOCKS).contains(&blocks) {
            return Err(());
        }
        self.charge(
            blocks
                .checked_mul(std::mem::size_of::<BlockWire>() + std::mem::size_of::<Block>())
                .ok_or(())?,
        )?;
        for _ in 0..blocks {
            self.block()?;
        }
        self.take(32)?;
        if self.cursor != self.bytes.len() {
            return Err(());
        }
        Ok(())
    }

    fn block(&mut self) -> Result<(), ()> {
        let caveats = self.length()?;
        self.caveats = self.caveats.checked_add(caveats).ok_or(())?;
        if self.caveats > MAX_LIVE_CAVEATS {
            return Err(());
        }
        self.charge(
            caveats
                .checked_mul(std::mem::size_of::<Caveat>())
                .ok_or(())?,
        )?;
        for _ in 0..caveats {
            self.caveat()?;
        }
        self.take(32)?;
        let signature_len = self.length()?;
        if signature_len != 64 {
            return Err(());
        }
        self.charge(signature_len)?;
        self.take(signature_len)?;
        Ok(())
    }

    fn caveat(&mut self) -> Result<(), ()> {
        match self.varint()? {
            0 => self.pred(1),
            // The strict profile accepts no third-party caveat and constructs
            // no discharge lookup path.
            1 => Err(()),
            _ => Err(()),
        }
    }

    fn pred(&mut self, depth: usize) -> Result<(), ()> {
        if depth > MAX_LIVE_PRED_DEPTH {
            return Err(());
        }
        self.pred_nodes = self.pred_nodes.checked_add(1).ok_or(())?;
        if self.pred_nodes > MAX_LIVE_PRED_NODES {
            return Err(());
        }
        self.charge(std::mem::size_of::<Pred>())?;

        match self.varint()? {
            // Reject non-positive expressions before allocating their trees.
            0 | 1 | 8 | 9 => Err(()),
            2 | 3 => {
                self.string()?;
                self.string()
            }
            4 | 5 => {
                self.varint()?;
                Ok(())
            }
            6 => {
                self.varint()?;
                self.varint()?;
                Ok(())
            }
            7 => {
                let predicates = self.length()?;
                if predicates == 0 || predicates > MAX_LIVE_PRED_NODES - self.pred_nodes {
                    return Err(());
                }
                for _ in 0..predicates {
                    self.pred(depth.checked_add(1).ok_or(())?)?;
                }
                Ok(())
            }
            _ => Err(()),
        }
    }

    fn string(&mut self) -> Result<(), ()> {
        let len = self.length()?;
        if len > MAX_LIVE_STRING_BYTES {
            return Err(());
        }
        self.charge(len)?;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes).map(|_| ()).map_err(|_| ())
    }

    fn length(&mut self) -> Result<usize, ()> {
        usize::try_from(self.varint()?).map_err(|_| ())
    }

    fn varint(&mut self) -> Result<u64, ()> {
        let mut value = 0u64;
        for index in 0..10u32 {
            let byte = *self.take(1)?.first().ok_or(())?;
            let payload = u64::from(byte & 0x7f);
            if index == 9 && payload > 1 {
                return Err(());
            }
            value |= payload.checked_shl(7 * index).ok_or(())?;
            if byte & 0x80 == 0 {
                // Reject non-shortest duplicate encodings of the same value.
                if index > 0 && payload == 0 {
                    return Err(());
                }
                return Ok(value);
            }
        }
        Err(())
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], ()> {
        let end = self.cursor.checked_add(len).ok_or(())?;
        let slice = self.bytes.get(self.cursor..end).ok_or(())?;
        self.cursor = end;
        Ok(slice)
    }

    fn charge(&mut self, bytes: usize) -> Result<(), ()> {
        // A 4x allowance covers Vec capacity growth/minimum reservations and
        // overlapping wire/credential storage. This bounds requested decode
        // storage, not allocator metadata or process-wide memory usage.
        self.allocation = self
            .allocation
            .checked_add(bytes.checked_mul(4).ok_or(())?)
            .ok_or(())?;
        if self.allocation > MAX_LIVE_DECODED_ALLOCATION {
            return Err(());
        }
        Ok(())
    }
}

fn sig64(v: &[u8]) -> Result<[u8; 64], WireError> {
    v.try_into()
        .map_err(|_| WireError::Malformed("signature is not 64 bytes"))
}

impl Credential {
    /// Encode to the `dga1_…` string form. **Bearer**: the string carries the
    /// tail key, i.e. both the right to present and the right to attenuate
    /// further — transmit it like the capability it is.
    pub fn encode(&self) -> String {
        let wire = CredentialWire {
            nonce: self.nonce,
            blocks: self
                .blocks
                .iter()
                .map(|b| BlockWire {
                    caveats: b.caveats.clone(),
                    next_pub: b.next_pub,
                    sig: b.sig.to_vec(),
                })
                .collect(),
            proof_seed: self.proof.to_bytes(),
        };
        let bytes = postcard::to_stdvec(&wire).expect("credential encoding is total");
        format!("{CREDENTIAL_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Decode from the `dga1_…` string form. Structural validation only —
    /// authorization is decided by [`Credential::verify`].
    pub fn decode(s: &str) -> Result<Credential, WireError> {
        let body = s
            .trim()
            .strip_prefix(CREDENTIAL_PREFIX)
            .ok_or(WireError::Prefix {
                expected: CREDENTIAL_PREFIX,
            })?;
        let bytes = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|e| WireError::Base64(e.to_string()))?;
        let wire: CredentialWire =
            postcard::from_bytes(&bytes).map_err(|e| WireError::Schema(e.to_string()))?;
        if wire.blocks.is_empty() {
            return Err(WireError::Malformed("a credential has at least one block"));
        }
        let blocks = wire
            .blocks
            .iter()
            .map(|b| {
                Ok(Block {
                    caveats: b.caveats.clone(),
                    next_pub: b.next_pub,
                    sig: sig64(&b.sig)?,
                })
            })
            .collect::<Result<Vec<_>, WireError>>()?;
        let proof = SigningKey::from_bytes(&wire.proof_seed);
        let tail_pub = blocks.last().expect("non-empty checked above").next_pub;
        if proof.verifying_key().to_bytes() != tail_pub {
            return Err(WireError::Malformed(
                "carried proof key does not match the tail block (stripped or reassembled chain)",
            ));
        }
        Ok(Credential {
            nonce: wire.nonce,
            blocks,
            proof,
        })
    }

    /// Strict live-authority decoder. The legacy [`Credential::decode`] wire
    /// behavior remains unchanged; this opt-in path bounds encoded and raw
    /// input before postcard, scans all lengths/counters without allocating,
    /// then performs one bounded owned decode.
    pub(crate) fn decode_live_bounded(s: &str) -> Result<LiveCredential, ()> {
        if s.len() > MAX_LIVE_ENCODED_BYTES || s.trim() != s {
            return Err(());
        }
        let body = s.strip_prefix(CREDENTIAL_PREFIX).ok_or(())?;
        // The encoded cap permits at most 49,148 actual raw bytes. The
        // decoder's rounded output estimate may need one spare byte.
        let capacity = base64::decoded_len_estimate(body.len());
        if capacity > MAX_LIVE_RAW_BYTES + 1 {
            return Err(());
        }
        let mut raw = LiveRawBytes(vec![0; capacity]);
        let len = URL_SAFE_NO_PAD
            .decode_slice(body, &mut raw.0)
            .map_err(|_| ())?;
        if len > MAX_LIVE_RAW_BYTES {
            return Err(());
        }
        let bytes = &raw.0[..len];
        LiveWirePreflight::new(bytes).credential()?;

        let (wire, remainder): (CredentialWire, _) =
            postcard::take_from_bytes(bytes).map_err(|_| ())?;
        let mut wire = LiveDecodedWire(wire);
        if !remainder.is_empty() {
            return Err(());
        }

        let proof = SigningKey::from_bytes(&wire.0.proof_seed);
        overwrite(&mut wire.0.proof_seed);
        let mut credential = LiveCredential(Credential {
            nonce: wire.0.nonce,
            blocks: Vec::with_capacity(wire.0.blocks.len()),
            proof,
        });
        for block in &mut wire.0.blocks {
            let sig = sig64(&block.sig).map_err(|_| ())?;
            credential.0.blocks.push(Block {
                caveats: std::mem::take(&mut block.caveats),
                next_pub: block.next_pub,
                sig,
            });
        }
        let tail_pub = credential.0.blocks.last().ok_or(())?.next_pub;
        if credential.0.proof.verifying_key().to_bytes() != tail_pub {
            return Err(());
        }
        Ok(credential)
    }
}

impl Discharge {
    /// Encode to the `dgd1_…` string form.
    pub fn encode(&self) -> String {
        let wire = DischargeWire {
            caveat_id: self.caveat_id.clone(),
            caveats: self.caveats.clone(),
            binding: self.binding,
            sig: self.sig.to_vec(),
        };
        let bytes = postcard::to_stdvec(&wire).expect("discharge encoding is total");
        format!("{DISCHARGE_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Decode from the `dgd1_…` string form.
    pub fn decode(s: &str) -> Result<Discharge, WireError> {
        let body = s
            .trim()
            .strip_prefix(DISCHARGE_PREFIX)
            .ok_or(WireError::Prefix {
                expected: DISCHARGE_PREFIX,
            })?;
        let bytes = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|e| WireError::Base64(e.to_string()))?;
        let wire: DischargeWire =
            postcard::from_bytes(&bytes).map_err(|e| WireError::Schema(e.to_string()))?;
        Ok(Discharge::from_parts(
            wire.caveat_id,
            wire.caveats,
            wire.binding,
            sig64(&wire.sig)?,
        ))
    }
}
