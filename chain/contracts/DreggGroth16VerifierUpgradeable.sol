// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IGroth16Verifier25} from "./IGroth16Verifier25.sol";
import {IGroth16VerifierRegistry} from "./IGroth16VerifierRegistry.sol";
import {DreggSettlementVK} from "./DreggSettlementVK.sol";

/// UPGRADEABLE-VK Groth16(BN254) verifier for the dregg 25-lane settlement
/// statement, with a VK-EPOCH REGISTRY.
///
/// ── WHAT THIS IS ────────────────────────────────────────────────────────
/// The gnark-GENERATED verifier (`DreggGroth16Verifier25.sol`) hard-codes the
/// verifying key (α in G1; β, γ, δ in G2, stored NEGATED; the Pedersen
/// commitment key G, Gσ in G2; and the 27 IC points = the constant + 26
/// public-input bases) as Solidity CONSTANTS. Every VK epoch (a GAP-flip, the
/// nullifier flip, a re-genesis) therefore forces a redeploy of the verifier
/// AND — because `DreggSettlement` pins the verifier at construction — the
/// settlement contract too.
///
/// This contract moves the VK into STORAGE, keyed by a `uint256 epoch`:
///   * `mapping(uint256 => VerifyingKey) _vks`  — the VK per epoch,
///   * `currentEpoch`                            — the pointer a fresh proof
///                                                 targets,
///   * `advanceEpoch(newVk)`                     — write the NEXT epoch's VK
///                                                 and bump the pointer (ONE tx).
/// A proof is checked against the epoch it targets, so proofs minted under an
/// old VK stay verifiable at their epoch after a flip; already-settled roots
/// (recorded permanently by `DreggSettlement`) are unaffected either way.
///
/// The pairing MATH is byte-identical to the generated verifier — the same
/// commitment proof-of-knowledge gate, the same public-input MSM, and the
/// same final equation
///     e(A, B) · e(C, −δ) · e(α, −β) · e(L, −γ) == 1
/// — reproduced reading the VK from STORAGE instead of from code constants.
/// Epoch 0 is seeded (in the constructor) with the LIVE deployed VK, copied
/// byte-for-byte from `DreggGroth16Verifier25.sol`, so the current live proof
/// still verifies unchanged.
///
/// ── THE GATE (load-bearing security) ────────────────────────────────────
/// A MUTABLE VK is a security control: a setter that could install a VK which
/// accepts any proof (a forged one over any statement) is an accept-anything
/// backdoor. The registry closes that with TWO on-chain controls the code
/// itself ENFORCES — not merely names as a deploy requirement:
///
///   1. A TIMELOCK. A VK/epoch change is a two-phase `proposeEpoch` →
///      (wait `TIMELOCK_DELAY`) → `activateEpoch`. There is NO immediate-effect
///      path: a proposed VK is staged but INACTIVE until the delay elapses, so a
///      swap is observable and vetoable (`cancelProposal`) for at least the delay
///      before it can take effect. A compromised owner cannot flip the VK in one
///      block.
///   2. TWO-STEP OWNERSHIP. `transferOwnership` only NOMINATES; the nominee must
///      `acceptOwnership`. A one-way transfer to a wrong/dead/hostile address can
///      never strand or silently capture the trust root.
///
/// For a public/mainnet instance the owner SHOULD still be a governance multisig
/// (so the propose/veto keys are themselves distributed), but the timelock is now
/// a property of THIS contract, not a promise about who holds the key. See
/// `docs/deos/UPGRADEABLE-VK-REGISTRY.md`.
///
/// These gates are the only thing standing between the registry and a forged-VK
/// acceptance, so they are real, tested in both polarities and across the delay.
contract DreggGroth16VerifierUpgradeable is IGroth16VerifierRegistry {
    // ── BN254 field / scalar orders (same as the generated verifier) ──────
    /// Base field order P.
    uint256 internal constant P =
        0x30644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd47;
    /// Scalar field order R. Public inputs must be < R (canonical residue).
    uint256 internal constant R =
        0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001;

    // Precompile addresses.
    uint256 internal constant PRECOMPILE_ADD = 0x06;
    uint256 internal constant PRECOMPILE_MUL = 0x07;
    uint256 internal constant PRECOMPILE_VERIFY = 0x08;

    /// A G1 point (affine). Coordinates in Fp.
    struct G1Point {
        uint256 x;
        uint256 y;
    }

    /// A G2 point (affine) over Fp2 = Fp[i]/(i²+1), coordinates written as the
    /// pair (c0, c1) meaning c0 + c1·i — matching the generated verifier's
    /// `_X_0`/`_X_1` constant naming. β, γ, δ are stored NEGATED (exactly the
    /// values gnark's `ExportSolidity` bakes in); G, Gσ are stored as-is.
    struct G2Point {
        uint256 x0;
        uint256 x1;
        uint256 y0;
        uint256 y1;
    }

    /// A full verifying key. `ic` has 27 entries: `ic[0]` is the constant term
    /// and `ic[1..26]` are the 26 public-input bases (25 statement lanes + 1
    /// Pedersen-commitment public input).
    struct VerifyingKey {
        G1Point alpha;
        G2Point betaNeg;
        G2Point gammaNeg;
        G2Point deltaNeg;
        G2Point pedersenG;
        G2Point pedersenGSigma;
        G1Point[27] ic;
    }

    // ── registry state ────────────────────────────────────────────────────
    address public owner;
    /// Two-step ownership: a NOMINATED next owner that must itself call
    /// `acceptOwnership`. A one-way transfer to a wrong/dead address can therefore
    /// never strand or silently capture the VK trust root in a single tx.
    address public pendingOwner;
    uint256 public currentEpoch;
    mapping(uint256 => VerifyingKey) private _vks;
    mapping(uint256 => bool) private _vkSet;

    /// The MINIMUM on-chain delay between PROPOSING a VK/epoch and ACTIVATING it.
    /// This is the load-bearing timelock the audit required the code to ENFORCE
    /// (not merely name): a VK swap is observable and vetoable for at least this
    /// long before it can take effect, so a compromised owner cannot install an
    /// accept-anything VK in a single block. CONSERVATIVE DEFAULT: 2 days — the
    /// common governance-timelock floor; long enough for a swap to be seen and
    /// challenged, short enough not to cripple a legitimate epoch rotation.
    uint256 public constant TIMELOCK_DELAY = 2 days;

    /// A pending, timelocked VK proposal for the next epoch. The proposed VK is
    /// staged into `_vks[epoch]` at propose time (already `_validate`d) but stays
    /// INACTIVE (`_vkSet[epoch]` false, `currentEpoch` unmoved) until
    /// `activateEpoch` is called on/after `eta`. Cancellable before then (the veto).
    struct Proposal {
        bool pending;
        uint256 epoch; // == currentEpoch + 1 at propose time
        uint256 eta; // earliest activation timestamp (propose time + TIMELOCK_DELAY)
    }
    Proposal public proposal;

    error NotOwner(address caller);
    error NotPendingOwner(address caller);
    error ZeroOwner();
    error MalformedVerifyingKey(string reason);
    error EpochAlreadySet(uint256 epoch);
    error NoPendingProposal();
    error ProposalNotReady(uint256 eta);

    event OwnershipTransferStarted(address indexed from, address indexed to);
    event OwnershipTransferred(address indexed from, address indexed to);
    event EpochProposed(uint256 indexed epoch, uint256 eta, address indexed by);
    event EpochProposalCancelled(uint256 indexed epoch, address indexed by);
    event EpochAdvanced(uint256 indexed epoch, address indexed by);
    event VerifyingKeySet(uint256 indexed epoch, address indexed by);

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner(msg.sender);
        _;
    }

    /// Seeds epoch 0 with the LIVE deployed VK (byte-identical to
    /// `DreggGroth16Verifier25.sol`) and installs the deployer as owner. The
    /// seed is validated (in-field + G1 on-curve), so a transcription slip
    /// reverts the deploy rather than shipping a silently-wrong VK.
    constructor() {
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);

        VerifyingKey memory vk = _epoch0VK();
        _validate(vk);
        _store(0, vk);
        _vkSet[0] = true;
        currentEpoch = 0;
        emit VerifyingKeySet(0, msg.sender);
    }

    // ── ownership (TWO-STEP) ──────────────────────────────────────────────

    /// NOMINATE a next owner. The nominee is not the owner until it calls
    /// `acceptOwnership`; this transaction only records the pending nominee, so a
    /// transfer to a wrong/dead address cannot strand the trust root.
    function transferOwnership(address to) external onlyOwner {
        if (to == address(0)) revert ZeroOwner();
        pendingOwner = to;
        emit OwnershipTransferStarted(owner, to);
    }

    /// Accept a pending ownership nomination — only callable by the nominee.
    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert NotPendingOwner(msg.sender);
        emit OwnershipTransferred(owner, pendingOwner);
        owner = pendingOwner;
        pendingOwner = address(0);
    }

    // ── epoch administration (THE GATE: propose → timelock → activate) ─────

    /// PROPOSE the next epoch's VK. Validates and STAGES it into `_vks[epoch]`
    /// (epoch == currentEpoch + 1) but leaves it INACTIVE — `currentEpoch` does
    /// not move and `verifyProof*` do not see it — until `activateEpoch` is called
    /// on/after `eta = now + TIMELOCK_DELAY`. Re-proposing before activation simply
    /// re-stages the VK and resets the delay. `onlyOwner`.
    function proposeEpoch(VerifyingKey calldata newVk)
        external
        onlyOwner
        returns (uint256 epoch, uint256 eta)
    {
        _validate(newVk);
        epoch = currentEpoch + 1;
        if (_vkSet[epoch]) revert EpochAlreadySet(epoch);
        // Stage the VK (validated). It is NOT active: `_vkSet[epoch]` stays false
        // until `activateEpoch`, so nothing verifies against it during the delay.
        _store(epoch, newVk);
        eta = block.timestamp + TIMELOCK_DELAY;
        proposal = Proposal({pending: true, epoch: epoch, eta: eta});
        emit EpochProposed(epoch, eta, msg.sender);
    }

    /// ACTIVATE the pending proposal once its timelock has elapsed: flip the staged
    /// VK live and advance the current-epoch pointer. This is the whole VK-epoch
    /// flip — no redeploy — but it can only land at least `TIMELOCK_DELAY` after the
    /// proposal, never in the same block. `onlyOwner`.
    function activateEpoch() external onlyOwner returns (uint256 epoch) {
        Proposal memory p = proposal;
        if (!p.pending) revert NoPendingProposal();
        if (block.timestamp < p.eta) revert ProposalNotReady(p.eta);
        epoch = p.epoch;
        // The staged VK was written (and validated) at propose time; flip it live
        // and advance the pointer. Refuses a double-activate (proposal consumed).
        _vkSet[epoch] = true;
        currentEpoch = epoch;
        proposal.pending = false;
        emit EpochAdvanced(epoch, msg.sender);
        emit VerifyingKeySet(epoch, msg.sender);
    }

    /// VETO a pending proposal before it activates (e.g. a swap spotted during the
    /// timelock window). The staged-but-inactive VK is left dangling in `_vks` and
    /// harmlessly overwritten by the next `proposeEpoch`. `onlyOwner`.
    function cancelProposal() external onlyOwner {
        Proposal memory p = proposal;
        if (!p.pending) revert NoPendingProposal();
        proposal.pending = false;
        emit EpochProposalCancelled(p.epoch, msg.sender);
    }

    function isEpochSet(uint256 epoch) external view returns (bool) {
        return _vkSet[epoch];
    }

    /// Read back an epoch's stored VK (e.g. to copy, perturb, or re-install).
    function getVerifyingKey(uint256 epoch)
        external
        view
        returns (VerifyingKey memory)
    {
        return _vks[epoch];
    }

    // ── the VK commitment (a FUNCTION OF THE STORED KEY) ──────────────────

    /// Domain tag = the `schema` field of `chain/codegen/dregg_vk.json`. A
    /// schema bump moves every chain's pin.
    string internal constant DIGEST_DOMAIN = "dregg-groth16-vk/1";

    /// `IGroth16Verifier25.vkDigest`: the digest of the CURRENT epoch's VK —
    /// the key a fresh `verifyProof` actually runs the pairing against.
    ///
    /// ⚑ This is the self-evidencing half of the VK pin. The key lives in
    /// STORAGE, so the digest is computed from the very words `_verify` loads:
    /// there is no second copy to drift. An epoch flip that installs a
    /// different key MOVES this value in the same transaction that makes the
    /// new key live — which is the entire property the superseded pin,
    /// `keccak256("dregg-settlement-vk-dev-setup")`, could not have.
    function vkDigest() external view returns (bytes32) {
        return vkDigestAtEpoch(currentEpoch);
    }

    /// The digest of a TARGETED epoch's stored VK. Reverts `MalformedVerifyingKey`
    /// for an epoch that was never set, so an unset epoch can never be reported
    /// as committing to the all-zero key (the Nomad-law default).
    ///
    /// Serialization is byte-identical to `DreggSettlementVK.digest()`,
    /// `solana_settlement::vk_digest::digest_of` and the Cosmos emitted
    /// constant — all four from `chain/codegen/dregg_vk.json`. G2 words go in
    /// EIP-197 order (imaginary coordinate first), matching both the storage
    /// layout the pairing feeds the precompile and the generated verifier's
    /// `_X_1`/`_X_0` word order.
    function vkDigestAtEpoch(uint256 epoch) public view returns (bytes32) {
        if (!_vkSet[epoch]) revert MalformedVerifyingKey("epoch not set");
        VerifyingKey storage vk = _vks[epoch];

        uint256[] memory w = new uint256[](76);
        w[0] = vk.alpha.x;
        w[1] = vk.alpha.y;
        _g2Words(w, 2, vk.betaNeg);
        _g2Words(w, 6, vk.gammaNeg);
        _g2Words(w, 10, vk.deltaNeg);
        _g2Words(w, 14, vk.pedersenG);
        _g2Words(w, 18, vk.pedersenGSigma);
        // ic[0] is the constant term; ic[1..27] are the 26 IC bases.
        for (uint256 i = 0; i < 27; i++) {
            w[22 + 2 * i] = vk.ic[i].x;
            w[23 + 2 * i] = vk.ic[i].y;
        }

        return keccak256(
            abi.encodePacked(
                DIGEST_DOMAIN,
                uint32(25), // NUM_PUBLIC_INPUTS
                uint32(26), // NUM_IC_BASES
                abi.encodePacked(w)
            )
        );
    }

    /// Append a G2 point in the digest's pinned EIP-197 order: x1, x0, y1, y0.
    function _g2Words(uint256[] memory w, uint256 at, G2Point storage pt)
        private
        view
    {
        w[at] = pt.x1;
        w[at + 1] = pt.x0;
        w[at + 2] = pt.y1;
        w[at + 3] = pt.y0;
    }

    // ── verification ──────────────────────────────────────────────────────

    /// `IGroth16Verifier25` drop-in: verify against the CURRENT epoch's VK.
    function verifyProof(
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[2] calldata commitments,
        uint256[2] calldata commitmentPok,
        uint256[25] calldata publicInputs
    ) external view returns (bool) {
        return _verify(currentEpoch, a, b, c, commitments, commitmentPok, publicInputs);
    }

    /// Verify against a TARGETED epoch's VK (old epochs stay verifiable).
    function verifyProofAtEpoch(
        uint256 epoch,
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[2] calldata commitments,
        uint256[2] calldata commitmentPok,
        uint256[25] calldata publicInputs
    ) external view returns (bool) {
        return _verify(epoch, a, b, c, commitments, commitmentPok, publicInputs);
    }

    // ── the pairing (byte-identical math over the STORAGE VK) ──────────────

    function _verify(
        uint256 epoch,
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[2] calldata commitments,
        uint256[2] calldata commitmentPok,
        uint256[25] calldata input
    ) internal view returns (bool) {
        if (!_vkSet[epoch]) return false;
        VerifyingKey storage vk = _vks[epoch];

        // HashToField for the committed public input, exactly as the generated
        // verifier: keccak over the packed commitment point (the
        // `publicAndCommitmentCommitted` list is empty for this circuit, so it
        // contributes no bytes), reduced mod R.
        uint256 pubCommit = uint256(
            keccak256(abi.encodePacked(commitments[0], commitments[1]))
        ) % R;

        // Pedersen commitment proof-of-knowledge gate:
        //   e(commitment, Gσ) · e(pok, G) == 1.
        if (!_checkPedersen(vk, commitments, commitmentPok)) return false;

        // Public-input linear combination L (the MSM), then the final pairing.
        (uint256 lx, uint256 ly, bool okMsm) = _msm(vk, input, pubCommit, commitments);
        if (!okMsm) return false;

        return _checkPairing(vk, a, b, c, lx, ly);
    }

    /// e(commitment, Gσ) · e(pok, G) == 1. G2 words go in EIP-197 order
    /// (imaginary coordinate first), matching the generated verifier.
    function _checkPedersen(
        VerifyingKey storage vk,
        uint256[2] calldata commitments,
        uint256[2] calldata pok
    ) internal view returns (bool) {
        uint256[12] memory p;
        p[0] = commitments[0];
        p[1] = commitments[1];
        p[2] = vk.pedersenGSigma.x1;
        p[3] = vk.pedersenGSigma.x0;
        p[4] = vk.pedersenGSigma.y1;
        p[5] = vk.pedersenGSigma.y0;
        p[6] = pok[0];
        p[7] = pok[1];
        p[8] = vk.pedersenG.x1;
        p[9] = vk.pedersenG.x0;
        p[10] = vk.pedersenG.y1;
        p[11] = vk.pedersenG.y0;

        uint256[1] memory out;
        bool ok;
        assembly ("memory-safe") {
            ok := staticcall(gas(), PRECOMPILE_VERIFY, p, 0x180, out, 0x20)
        }
        return ok && out[0] == 1;
    }

    /// L = IC[0] + commitment + Σ_{i<25} input[i]·IC[i+1] + pubCommit·IC[26].
    /// (The gnark commitment adds the raw commitment point into the constant
    /// term, exactly as `publicInputMSM` in the generated verifier.)
    function _msm(
        VerifyingKey storage vk,
        uint256[25] calldata input,
        uint256 pubCommit,
        uint256[2] calldata commitments
    ) internal view returns (uint256 lx, uint256 ly, bool ok) {
        // IC[0] + commitment
        (lx, ly, ok) = _ecAdd(vk.ic[0].x, vk.ic[0].y, commitments[0], commitments[1]);
        if (!ok) return (0, 0, false);

        for (uint256 i = 0; i < 25; i++) {
            uint256 s = input[i];
            if (s >= R) return (0, 0, false); // non-canonical public input
            (uint256 mx, uint256 my, bool k1) = _ecMul(vk.ic[i + 1].x, vk.ic[i + 1].y, s);
            if (!k1) return (0, 0, false);
            (lx, ly, ok) = _ecAdd(lx, ly, mx, my);
            if (!ok) return (0, 0, false);
        }

        if (pubCommit >= R) return (0, 0, false);
        (uint256 cx, uint256 cy, bool k2) = _ecMul(vk.ic[26].x, vk.ic[26].y, pubCommit);
        if (!k2) return (0, 0, false);
        (lx, ly, ok) = _ecAdd(lx, ly, cx, cy);
    }

    /// e(A, B) · e(C, −δ) · e(α, −β) · e(L, −γ) == 1. Proof word order matches
    /// the generated verifier / adapter: A, B, C already in EIP-197 order.
    function _checkPairing(
        VerifyingKey storage vk,
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256 lx,
        uint256 ly
    ) internal view returns (bool) {
        uint256[24] memory p;
        // e(A, B)
        p[0] = a[0];
        p[1] = a[1];
        p[2] = b[0][0];
        p[3] = b[0][1];
        p[4] = b[1][0];
        p[5] = b[1][1];
        // e(C, −δ)
        p[6] = c[0];
        p[7] = c[1];
        p[8] = vk.deltaNeg.x1;
        p[9] = vk.deltaNeg.x0;
        p[10] = vk.deltaNeg.y1;
        p[11] = vk.deltaNeg.y0;
        // e(α, −β)
        p[12] = vk.alpha.x;
        p[13] = vk.alpha.y;
        p[14] = vk.betaNeg.x1;
        p[15] = vk.betaNeg.x0;
        p[16] = vk.betaNeg.y1;
        p[17] = vk.betaNeg.y0;
        // e(L, −γ)
        p[18] = lx;
        p[19] = ly;
        p[20] = vk.gammaNeg.x1;
        p[21] = vk.gammaNeg.x0;
        p[22] = vk.gammaNeg.y1;
        p[23] = vk.gammaNeg.y0;

        uint256[1] memory out;
        bool ok;
        assembly ("memory-safe") {
            ok := staticcall(gas(), PRECOMPILE_VERIFY, p, 0x300, out, 0x20)
        }
        return ok && out[0] == 1;
    }

    function _ecAdd(uint256 x1, uint256 y1, uint256 x2, uint256 y2)
        internal
        view
        returns (uint256 rx, uint256 ry, bool ok)
    {
        uint256[4] memory inp = [x1, y1, x2, y2];
        assembly ("memory-safe") {
            ok := staticcall(gas(), PRECOMPILE_ADD, inp, 0x80, inp, 0x40)
            rx := mload(inp)
            ry := mload(add(inp, 0x20))
        }
    }

    function _ecMul(uint256 x, uint256 y, uint256 s)
        internal
        view
        returns (uint256 rx, uint256 ry, bool ok)
    {
        uint256[3] memory inp = [x, y, s];
        assembly ("memory-safe") {
            ok := staticcall(gas(), PRECOMPILE_MUL, inp, 0x60, inp, 0x40)
            rx := mload(inp)
            ry := mload(add(inp, 0x20))
        }
    }

    // ── VK well-formedness gate (malformed VK reverts at set time) ─────────

    /// Reject an out-of-field or off-curve VK. Every coordinate must be a
    /// reduced Fp residue (< P); every G1 point (α and the 27 IC points) must
    /// satisfy y² = x³ + 3. (G2 on-curve is left to the pairing precompile,
    /// which rejects a bad β/γ/δ/G/Gσ at verify time — fail-closed.)
    function _validate(VerifyingKey memory vk) internal pure {
        _g1(vk.alpha, "alpha");
        _g2InField(vk.betaNeg, "betaNeg");
        _g2InField(vk.gammaNeg, "gammaNeg");
        _g2InField(vk.deltaNeg, "deltaNeg");
        _g2InField(vk.pedersenG, "pedersenG");
        _g2InField(vk.pedersenGSigma, "pedersenGSigma");
        for (uint256 i = 0; i < 27; i++) {
            _g1(vk.ic[i], "ic");
        }
    }

    function _g1(G1Point memory pt, string memory tag) internal pure {
        if (pt.x >= P || pt.y >= P) revert MalformedVerifyingKey(tag);
        // y² == x³ + 3 (BN254 G1). Rejects garbage/off-curve bases.
        uint256 lhs = mulmod(pt.y, pt.y, P);
        uint256 rhs = addmod(mulmod(mulmod(pt.x, pt.x, P), pt.x, P), 3, P);
        if (lhs != rhs) revert MalformedVerifyingKey(tag);
    }

    function _g2InField(G2Point memory pt, string memory tag) internal pure {
        if (pt.x0 >= P || pt.x1 >= P || pt.y0 >= P || pt.y1 >= P) {
            revert MalformedVerifyingKey(tag);
        }
    }

    function _store(uint256 epoch, VerifyingKey memory vk) internal {
        VerifyingKey storage s = _vks[epoch];
        s.alpha = vk.alpha;
        s.betaNeg = vk.betaNeg;
        s.gammaNeg = vk.gammaNeg;
        s.deltaNeg = vk.deltaNeg;
        s.pedersenG = vk.pedersenG;
        s.pedersenGSigma = vk.pedersenGSigma;
        for (uint256 i = 0; i < 27; i++) {
            s.ic[i] = vk.ic[i];
        }
    }

    // ── epoch-0 seed: the LIVE deployed VK, byte-identical to
    //    DreggGroth16Verifier25.sol (α, β̄, γ̄, δ̄, G, Gσ, and the 27 IC points).
    function _epoch0VK() internal pure returns (VerifyingKey memory vk) {
        uint256[] memory w = DreggSettlementVK.words();

        vk.alpha = G1Point(w[0], w[1]);
        vk.betaNeg = G2Point(w[3], w[2], w[5], w[4]);
        vk.gammaNeg = G2Point(w[7], w[6], w[9], w[8]);
        vk.deltaNeg = G2Point(w[11], w[10], w[13], w[12]);
        vk.pedersenG = G2Point(w[15], w[14], w[17], w[16]);
        vk.pedersenGSigma = G2Point(w[19], w[18], w[21], w[20]);

        // ic[0] is the constant term; ic[1..26] are the 26 public-input bases.
        for (uint256 i = 0; i < 27; i++) {
            vk.ic[i] = G1Point(w[22 + 2 * i], w[23 + 2 * i]);
        }
    }
}
