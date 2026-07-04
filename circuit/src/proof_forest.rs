//! Cutover-ready selector set for the proof-carrying effect-VM lane.
//!
//! The original bespoke-`EffectVmAir` proof FOREST (`ForestNode` / `ProofForest` /
//! `verify_forest` — the v1 Proof-Carrying-Data-minus-recursion realization of
//! `docs/rebuild/PHASE-PROOF-CARRYING.md`) is RETIRED with the v1 hand-AIR. The
//! recursion tower folds the rotated leaf in `crate::ivc_turn_chain` /
//! `crate::joint_turn_recursive` instead. What remains here is the shared
//! [`CUTOVER_READY_SELECTORS`] set the joint-turn aggregation reads.

/// The cutover-ready selectors whose descriptors the differential harness has
/// GRADUATED (descriptor ⟺ hand-AIR proven IDENTICAL on the real witness +
/// anti-ghost, AND each carries the Lean `selectorGate s` binding tooth). Mirror
/// of `sdk::full_turn_proof::CUTOVER_READY_SELECTORS` and the harness's
/// `descriptor_proof_binds_to_its_selector` cutover set.
pub const CUTOVER_READY_SELECTORS: &[usize] = &[
    crate::effect_vm::columns::sel::TRANSFER,
    crate::effect_vm::columns::sel::NOTE_SPEND,
    crate::effect_vm::columns::sel::NOTE_CREATE,
    crate::effect_vm::columns::sel::EMIT_EVENT,
    crate::effect_vm::columns::sel::BRIDGE_MINT,
    crate::effect_vm::columns::sel::BURN,
    crate::effect_vm::columns::sel::CELL_SEAL,
    crate::effect_vm::columns::sel::CELL_DESTROY,
    crate::effect_vm::columns::sel::REFUSAL,
    crate::effect_vm::columns::sel::SET_VERIFICATION_KEY,
    crate::effect_vm::columns::sel::SET_PERMISSIONS,
    crate::effect_vm::columns::sel::EXERCISE_VIA_CAPABILITY,
    crate::effect_vm::columns::sel::PIPELINED_SEND,
    crate::effect_vm::columns::sel::INCREMENT_NONCE,
    crate::effect_vm::columns::sel::REFRESH_DELEGATION,
    crate::effect_vm::columns::sel::REVOKE_DELEGATION,
    crate::effect_vm::columns::sel::INTRODUCE,
];
