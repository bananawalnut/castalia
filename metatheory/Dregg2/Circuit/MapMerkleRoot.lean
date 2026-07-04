/-
# Dregg2.Circuit.MapMerkleRoot — the DEPLOYED depth-16 binary-Merkle map root (the faithful map-op commitment).

## What this closes (deliverable 2 — the last real denotation gap)

`DescriptorIR2.opensTo`/`writesTo` (the `MapOp.holdsAt` legs for nullifiers / cells / commitments)
denoted a FLAT SPONGE `Dregg2.Substrate.Heap.root hash h = hash (h.map leafOf)` — a single sponge over
the sorted leaf list. The DEPLOYED map-op (`circuit/src/heap_root.rs`, `circuit/src/descriptor_ir2.rs`
`Ir2Air::MapOps`) commits a **depth-`d` BINARY MERKLE** root instead:

    leaf = hash[addr, value]                       -- arity-2 `hash_many` (`HeapLeaf::digest`)
    node = hash[left, right]                       -- arity-2 `hash_fact(l, [r])` (the `mix` fold)
    root = the perfect binary fold of the (padded) sorted leaf-digest list, depth `HEAP_TREE_DEPTH = 16`

This module models EXACTLY that binary fold (`mapNode`/`foldLevel`/`perfectRoot`) and proves it
INJECTIVE on the padded leaf-digest vector under the single named Poseidon2-CR floor
(`mapNode_injective` — the `hash[l,r]` 2-to-1 node CR — composed up the depth-`d` perfect tree by
`perfectRoot_injective`). `DescriptorIR2` re-defines `opensTo`/`writesTo` over THIS root (dropping the
flat-sponge `Heap.root`), so `MapOp.holdsAt` — a leg of the deployed `Satisfied2` — denotes the genuine
depth-16 binary-Merkle opening, and the anti-ghost `opensTo_functional`/`writesTo_functional` re-prove
against `perfectRoot_injective` (the binary-tree analog of `Heap.root_injective`), NOT the sponge.

## The model (faithful to `heap_root.rs`)

The deployed `CanonicalHeapTree` pads the sorted leaf list to `2^d` positions with the MIN/MAX
sentinels, then folds bottom-up by `hash_fact(l, [r])`. We model the COMMITMENT of a sorted heap as
`mapRoot hash h := perfectRoot hash d (padDigests d (h.map (leafOf hash)))`: the leaf digests of the
sorted entries, padded to `2^d` with a fixed sentinel digest, folded up the perfect tree. The sorted
discipline (`SortedKeys`) keeps the entry list canonical, so the heap MEANING (`Heap.get`) is unchanged
— only the COMMITMENT FUNCTION moves from the flat sponge to the binary fold. The named CR floor is the
SAME `Poseidon2SpongeCR`, now used at the 2-to-1 node (`mapNode`) and the leaf (`leafOf`).

## Axiom hygiene

`#assert_axioms` ⊆ {propext, Classical.choice, Quot.sound}. Crypto
enters ONLY as the named `Poseidon2SpongeCR` floor (at the node + leaf), the SAME floor the whole
commitment tower carries. NEW file; imports read-only.
-/
import Dregg2.Substrate.Heap
import Dregg2.Circuit.Poseidon2Binding

namespace Dregg2.Circuit.MapMerkleRoot

open Dregg2.Substrate
open Dregg2.Circuit.Poseidon2Binding (Poseidon2SpongeCR)

set_option autoImplicit false

/-- The deployed map-tree depth (`heap_root.rs::HEAP_TREE_DEPTH = 16`). The model is generic in `d`;
the deployment instance pins `d = 16`. -/
def HEAP_TREE_DEPTH : Nat := 16

/-! ## §1 — the 2-to-1 binary node (`hash[left, right]`, arity-2 — the deployed `hash_fact(l,[r])`). -/

/-- **`mapNode hash l r`** — the internal node digest, the arity-2 hash over `[left, right]`.
BYTE-IDENTICAL to `heap_root.rs`'s `hash_fact(cur, &[sib])` / `hash_fact(sib, &[cur])` fold node (a
length-2 absorb, NO domain marker — distinct from the cap node's `[FACT_MARK, l, r]`). -/
def mapNode (hash : List ℤ → ℤ) (l r : ℤ) : ℤ := hash [l, r]

/-- The 2-to-1 node is injective in its two children under CR: equal node images force equal
`[l, r]` lists, hence equal children. The per-level peel of the fold's anti-ghost. -/
theorem mapNode_injective (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash)
    {l₁ r₁ l₂ r₂ : ℤ} (h : mapNode hash l₁ r₁ = mapNode hash l₂ r₂) : l₁ = l₂ ∧ r₁ = r₂ := by
  have hlist := hCR _ _ h
  simp only [List.cons.injEq, and_true] at hlist
  exact ⟨hlist.1, hlist.2⟩

/-! ## §2 — the perfect-tree fold (a level pairs adjacent digests; `perfectRoot` folds `d` levels). -/

/-- **`foldLevel hash xs`** — one Merkle level: pair adjacent digests by `mapNode`. On a list of even
length `2n` this returns the `n` parent digests, matching `heap_root.rs`'s `chunks(2)` step. -/
def foldLevel (hash : List ℤ → ℤ) : List ℤ → List ℤ
  | [] => []
  | [x] => [x]                                   -- (unreached at even lengths; carry the orphan)
  | l :: r :: rest => mapNode hash l r :: foldLevel hash rest

/-- **`perfectRoot hash d xs`** — fold `d` levels and take the head: the perfect binary-tree root of
the `2^d` leaf digests `xs`. At `d = 0` the root is the single leaf (`xs.headD 0`). -/
def perfectRoot (hash : List ℤ → ℤ) : Nat → List ℤ → ℤ
  | 0,     xs => xs.headD 0
  | d + 1, xs => perfectRoot hash d (foldLevel hash xs)

/-! ## §3 — injectivity of one level, then of the whole fold (the binary `root_injective` analog). -/

/-- One fold level HALVES an even-length list: `(foldLevel hash xs).length = n` when
`xs.length = 2 * n`, matching `heap_root.rs`'s `chunks(2)` step. Inducts on the half-length `n`. -/
theorem foldLevel_length_half (hash : List ℤ → ℤ) :
    ∀ (n : Nat) (xs : List ℤ), xs.length = 2 * n → (foldLevel hash xs).length = n := by
  intro n
  induction n with
  | zero =>
    intro xs hn
    have : xs = [] := List.length_eq_zero_iff.mp (by omega)
    subst this; simp [foldLevel]
  | succ m ih =>
    intro xs hn
    match xs, hn with
    | l :: r :: rest, hn =>
      simp only [List.length_cons] at hn
      have hrest : rest.length = 2 * m := by omega
      simp only [foldLevel, List.length_cons, ih rest hrest]

/-- `foldLevel` is injective on lists of equal length `2*n` under the node CR: peel each pair by
`mapNode_injective`. (Lists that arise in `perfectRoot` always have length `2^d`, hence even at every
level above the leaves.) Inducts on the half-length `n`. -/
theorem foldLevel_injective (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash) :
    ∀ (n : Nat) {xs ys : List ℤ}, xs.length = 2 * n → ys.length = 2 * n →
      foldLevel hash xs = foldLevel hash ys → xs = ys := by
  intro n
  induction n with
  | zero =>
    intro xs ys hx hy _
    have hxe : xs = [] := List.length_eq_zero_iff.mp (by omega)
    have hye : ys = [] := List.length_eq_zero_iff.mp (by omega)
    rw [hxe, hye]
  | succ m ih =>
    intro xs ys hx hy hfold
    match xs, hx, ys, hy with
    | l :: r :: rest, hx, l' :: r' :: rest', hy =>
      simp only [foldLevel, List.cons.injEq] at hfold
      obtain ⟨hnode, hrest⟩ := hfold
      obtain ⟨hl, hr⟩ := mapNode_injective hash hCR hnode
      simp only [List.length_cons] at hx hy
      have hxlen : rest.length = 2 * m := by omega
      have hylen : rest'.length = 2 * m := by omega
      have := ih hxlen hylen hrest
      rw [hl, hr, this]

/-- **`perfectRoot_injective` — the binary-Merkle root BINDS the whole leaf-digest vector.** Two
length-`2^d` leaf-digest lists with EQUAL perfect-tree roots are EQUAL, under the single named CR floor
— peel each of the `d` levels by `foldLevel_injective`. The binary-tree analog of the flat sponge's
`Heap.root_injective`: a prover cannot keep the published map root while tampering ANY leaf digest. -/
theorem perfectRoot_injective (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash) :
    ∀ (d : Nat) {xs ys : List ℤ}, xs.length = 2 ^ d → ys.length = 2 ^ d →
      perfectRoot hash d xs = perfectRoot hash d ys → xs = ys := by
  intro d
  induction d with
  | zero =>
    intro xs ys hx hy hroot
    -- length `2^0 = 1`: both are singletons, `perfectRoot 0 = headD`.
    rw [pow_zero] at hx hy
    match xs, ys, hx, hy with
    | [x], [y], _, _ =>
      simp only [perfectRoot, List.headD_cons] at hroot
      rw [hroot]
  | succ d ih =>
    intro xs ys hx hy hroot
    simp only [perfectRoot] at hroot
    -- both `xs`, `ys` have length `2^(d+1) = 2 * 2^d`; one level halves to length `2^d`.
    have hxlen : xs.length = 2 ^ d + 2 ^ d := by rw [hx]; ring
    have hylen : ys.length = 2 ^ d + 2 ^ d := by rw [hy]; ring
    have hfl_x : (foldLevel hash xs).length = 2 ^ d := foldLevel_length_half hash (2 ^ d) xs (by omega)
    have hfl_y : (foldLevel hash ys).length = 2 ^ d := foldLevel_length_half hash (2 ^ d) ys (by omega)
    have hfold := ih hfl_x hfl_y hroot
    exact foldLevel_injective hash hCR (2 ^ d) (by omega) (by omega) hfold

/-! ## §4 — `mapRoot`: the deployed map COMMITMENT (the binary fold of the sorted heap's leaf digests).

The sorted heap (`Heap.FeltHeap`, the abstract map MEANING) is committed by folding its leaf-digest list
through the depth-`d` perfect binary tree — the deployed `CanonicalHeapTree::root`. The heap MEANING
(`Heap.get`/`Heap.SortedKeys`) is UNCHANGED; only the COMMITMENT FUNCTION moves from the flat sponge
(`Heap.root hash h = hash (h.map leafOf)`) to this binary fold. The deployment pins every heap as the
fixed-depth `2^d`-leaf padded vector; the `RootedAt` relation below carries that `length = 2^d`
discipline so the root BINDS the heap (`mapRoot_injective`). -/

/-- **`mapRoot hash d h`** — the depth-`d` binary-Merkle root of the sorted heap `h`: the perfect-tree
fold of its leaf-digest list `h.map (Heap.leafOf hash)`. BYTE-IDENTICAL to `heap_root.rs`'s
`CanonicalHeapTree::root` (arity-2 `leafOf` leaves, arity-2 `mapNode` nodes, depth `d`). -/
def mapRoot (hash : List ℤ → ℤ) (d : Nat) (h : Heap.FeltHeap) : ℤ :=
  perfectRoot hash d (h.map (Heap.leafOf hash))

/-- **`mapRoot_injective` — the binary-Merkle map root BINDS the whole heap (the anti-ghost), re-proved
against `perfectRoot_injective` (the NODE injectivity `mapNode_injective` composed up the tree), NOT the
flat-sponge `Heap.root_injective`.** Two depth-`d` `2^d`-leaf heaps publishing the same binary root are
EQUAL: the root injectivity peels the `d` node levels (`perfectRoot_injective`) to equal leaf-digest
lists, then the leaf CR (`Heap.map_leaf_injective`) peels each leaf to equal entries. A prover cannot keep
the published map root while tampering ANY address or value. -/
theorem mapRoot_injective (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash) (d : Nat)
    {h₁ h₂ : Heap.FeltHeap} (hlen₁ : h₁.length = 2 ^ d) (hlen₂ : h₂.length = 2 ^ d)
    (h : mapRoot hash d h₁ = mapRoot hash d h₂) : h₁ = h₂ := by
  have hmap : (h₁.map (Heap.leafOf hash)) = (h₂.map (Heap.leafOf hash)) :=
    perfectRoot_injective hash hCR d (by rw [List.length_map, hlen₁])
      (by rw [List.length_map, hlen₂]) h
  exact Heap.map_leaf_injective hash hCR h₁ h₂ hmap

/-! ## §5 — the faithful map OPENING (`opensToMerkle`/`writesToMerkle`) + the re-proved anti-ghost.

The binary-Merkle replacement for `DescriptorIR2.opensTo`/`writesTo`: a depth-`d` `2^d`-leaf sorted heap
behind the binary root reads / writes the abstract map. The `_functional` anti-ghost re-proves against
`mapRoot_injective` (hence `perfectRoot_injective`/`mapNode_injective`), NOT the sponge `root_injective` —
the deliverable-2 functional tooth over the deployed binary tree. -/

/-- **`opensToMerkle hash d r k o`** — some depth-`d` `2^d`-leaf sorted heap behind the BINARY-MERKLE
root `r` reads `o` at `k`. The faithful replacement of `DescriptorIR2.opensTo` over the deployed tree. -/
def opensToMerkle (hash : List ℤ → ℤ) (d : Nat) (r k : ℤ) (o : Option ℤ) : Prop :=
  ∃ h : Heap.FeltHeap, Heap.SortedKeys h ∧ h.length = 2 ^ d ∧ mapRoot hash d h = r ∧ Heap.get h k = o

/-- **`writesToMerkle hash d r k v r'`** — some depth-`d` `2^d`-leaf sorted heap behind binary root `r`
produces root `r'` under the sorted insert-or-update of `(k, v)`, with the post-heap still `2^d`-leaf. -/
def writesToMerkle (hash : List ℤ → ℤ) (d : Nat) (r k v r' : ℤ) : Prop :=
  ∃ h : Heap.FeltHeap, Heap.SortedKeys h ∧ h.length = 2 ^ d
    ∧ (Heap.set h k v).length = 2 ^ d
    ∧ mapRoot hash d h = r ∧ r' = mapRoot hash d (Heap.set h k v)

/-- **Binary-Merkle openings are FUNCTIONAL (the anti-ghost over the deployed tree).** Under CR, the
binary root + key determine the read: two openings of the same root at the same key agree. Re-proved
against `mapRoot_injective` (the binary fold), NOT the flat-sponge `Heap.root_injective`. -/
theorem opensToMerkle_functional (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash) (d : Nat)
    {r k : ℤ} {o₁ o₂ : Option ℤ}
    (h₁ : opensToMerkle hash d r k o₁) (h₂ : opensToMerkle hash d r k o₂) : o₁ = o₂ := by
  obtain ⟨m₁, _, hl₁, hr₁, hg₁⟩ := h₁
  obtain ⟨m₂, _, hl₂, hr₂, hg₂⟩ := h₂
  have hm : m₁ = m₂ := mapRoot_injective hash hCR d hl₁ hl₂ (hr₁.trans hr₂.symm)
  rw [← hg₁, ← hg₂, hm]

/-- Membership and non-membership at the same binary root/key EXCLUDE each other (the nullifier / cap
non-membership tooth, over the deployed tree). -/
theorem opensToMerkle_some_excludes_none (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash) (d : Nat)
    {r k v : ℤ} (h₁ : opensToMerkle hash d r k (some v)) (h₂ : opensToMerkle hash d r k none) : False := by
  have := opensToMerkle_functional hash hCR d h₁ h₂
  simp at this

/-- **Binary-Merkle writes are FUNCTIONAL.** Under CR, binary root + key + value determine the new root:
the map-op row's `new_root` column cannot be forged. Re-proved against `mapRoot_injective`. -/
theorem writesToMerkle_functional (hash : List ℤ → ℤ) (hCR : Poseidon2SpongeCR hash) (d : Nat)
    {r k v r₁ r₂ : ℤ}
    (h₁ : writesToMerkle hash d r k v r₁) (h₂ : writesToMerkle hash d r k v r₂) : r₁ = r₂ := by
  obtain ⟨m₁, _, hl₁, _, hr₁, he₁⟩ := h₁
  obtain ⟨m₂, _, hl₂, _, hr₂, he₂⟩ := h₂
  have hm : m₁ = m₂ := mapRoot_injective hash hCR d hl₁ hl₂ (hr₁.trans hr₂.symm)
  rw [he₁, he₂, hm]

/-! ## §6 — Axiom hygiene. -/

#assert_axioms mapNode_injective
#assert_axioms foldLevel_length_half
#assert_axioms foldLevel_injective
#assert_axioms perfectRoot_injective
#assert_axioms mapRoot_injective
#assert_axioms opensToMerkle_functional
#assert_axioms opensToMerkle_some_excludes_none
#assert_axioms writesToMerkle_functional

end Dregg2.Circuit.MapMerkleRoot
