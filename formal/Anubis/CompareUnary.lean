/-
  Anubis — mechanized SMT-encoding soundness: comparisons + unary ops (Phase 5, compare-unary slice)

  Companion to `Anubis.Encoding`. That file pinned the arithmetic lane (`+ - * & | ^`), the signed
  `<`/`<=` order (`bvslt`/`bvsle`), and the A1 u32 boundary mask. This file closes the REMAINING
  comparison and unary correspondences, so that the full binary-arithmetic + comparison + unary
  encoder surface the checker emits is machine-checked against the i64 runtime.

  The checker emits, and the runtime executes:
    * `>`  -> `bvsgt`   / signed i64 `>`          (there is no `BitVec.sgt`; `>` is a FLIPPED `slt`)
    * `>=` -> `bvsge`   / signed i64 `>=`         (likewise a flipped `sle`)
    * `==` -> `(= l r)` / structural i64 equality
    * `!=` -> `(not (= l r))` / structural i64 disequality
    * unary `-` -> `bvneg` / `i64::wrapping_neg`
    * unary `~` -> `bvnot` / bitwise NOT, which on two's complement is `-x - 1`

  What is proved here (all over `Word := BitVec 64`, the shared i64 / `(_ BitVec 64)` object):
    * `enc_sgt` / `enc_sge`: `>` and `>=` denote the signed `toInt` order — established by REDUCING
      them to the already-proven `slt`/`sle` with the operands swapped, which is exactly what the
      SMT `bvsgt`/`bvsge` (and the runtime's flipped `<`/`<=`) mean. No unsound unsigned model.
    * `enc_eq` / `enc_ne`: the encoder's `(= l r)` is EXACT structural equality (`↔ x = y`), and
      `(not (= l r))` is exact disequality (`↔ x ≠ y`) — the equality lane neither over- nor
      under-approximates.
    * `enc_neg` / `enc_neg_ofInt`: `bvneg` is the runtime `wrapping_neg`, and equals reinterpreting
      the negated mathematical integer `-x.toInt` back into 64 bits (`ofInt 64 (-x.toInt)`) — i.e.
      negation modulo 2^64, the wrapping the runtime performs.
    * `enc_not` / `enc_not_toInt` / `enc_not_add`: `bvnot x = -x - 1` as a `BitVec` identity, the same
      relation on the signed value (`(~~~x).toInt = -x.toInt - 1`, holds with NO wrapping because
      bitwise-not is a bijection on `[-2^63, 2^63)`), and the two's-complement law `x + ~~~x = -1`.

  Together with `Anubis.Encoding`, a green `check` over `< <= > >= == != - ~` and `+ - * & | ^`
  is now backed by a mechanized proof that each emitted term denotes exactly the i64 the runtime
  computes.

  Note on `!` (logical not on the boolean lane): the checker models it with SMT `Bool` `(not b)`,
  a different sort from `BitVec 64`. It is a boolean-sort fact, not a bit-vector correspondence, so
  it is out of scope for this word-level file and is not stated here to avoid a contrived encoding.
-/

import Anubis.Encoding

namespace Anubis.Encoding

/-! ### Signed `>` and `>=`: encoder = runtime, via flipped `slt`/`sle`

Lean core has no `BitVec.sgt`/`BitVec.sge`; the checker's `bvsgt l r` / `bvsge l r` are DEFINED as
the flipped strict/non-strict signed comparison `slt r l` / `sle r l`, which is also how the runtime
evaluates `l > r` (`r < l`) and `l >= r` (`r <= l`). So `>`/`>=` inherit the signed `toInt` order
directly from the already-proven `enc_slt`/`enc_sle` with the operands swapped. -/

/-- `bvsgt` (encoder) = signed i64 `>` (runtime): `x > y` is `y < x` over the `toInt` order. -/
theorem enc_sgt (x y : Word) : y.slt x = decide (x.toInt > y.toInt) := by
  rw [enc_slt]

/-- `bvsge` (encoder) = signed i64 `>=` (runtime): `x >= y` is `y <= x` over the `toInt` order. -/
theorem enc_sge (x y : Word) : y.sle x = decide (x.toInt ≥ y.toInt) := by
  rw [enc_sle]

/-! ### Equality `==` / disequality `!=`: encoder = exact structural (dis)equality

The encoder emits `(= l r)` for `==` and `(not (= l r))` for `!=`. On `BitVec 64`, SMT structural
equality IS Lean's `=`, so the encoded predicate is neither weaker nor stronger than the runtime's
i64 equality — it is exact. -/

/-- `(= l r)` (encoder) ⟺ i64 structural equality (runtime). Exact: no over/under-approximation. -/
theorem enc_eq (x y : Word) : (x == y) = true ↔ x = y := beq_iff_eq

/-- `(not (= l r))` (encoder) ⟺ i64 structural disequality (runtime). -/
theorem enc_ne (x y : Word) : (x == y) = false ↔ x ≠ y := beq_eq_false_iff_ne

/-! ### Unary `-`: `bvneg` = `i64::wrapping_neg`

`BitVec.neg` is the encoder's `bvneg`; `-x` is the `Neg` instance the runtime's `wrapping_neg`
denotes. They are the same map, and it equals reinterpreting `-x.toInt` back into 64 bits — negation
modulo 2^64. -/

/-- `bvneg` (encoder) = the `Neg` used to model `i64::wrapping_neg` (runtime). -/
theorem enc_neg (x : Word) : BitVec.neg x = -x := BitVec.neg_eq x

/-- **Wrapping negation is `ofInt (-toInt)`.** `bvneg x` equals reinterpreting the negated
    mathematical value `-x.toInt` as a 64-bit word — i.e. `-x mod 2^64`, exactly the wrapping the
    runtime's `wrapping_neg` performs. -/
theorem enc_neg_ofInt (x : Word) : -x = BitVec.ofInt 64 (-(x.toInt)) := by
  rw [BitVec.ofInt_neg, BitVec.ofInt_toInt]

/-! ### Unary `~`: `bvnot x = -x - 1`

The runtime's bitwise NOT on two's complement satisfies `~x = -x - 1`. We prove it three ways: as a
`BitVec` identity, on the signed value (`toInt`, where it holds with NO wrapping), and as the
two's-complement law `x + ~x = -1`. -/

/-- **`bvnot x = -x - 1` (BitVec identity).** The encoder's `bvnot` is the runtime two's-complement
    NOT, `-x - 1`. -/
theorem enc_not (x : Word) : ~~~x = -x - 1 := by
  rw [BitVec.not_eq_neg_add]; rfl

/-- **`bvnot` on the signed value.** `(~~~x).toInt = -x.toInt - 1`, with NO wrapping: bitwise NOT is
    a bijection carrying `[-2^63, 2^63)` onto itself, so the identity holds exactly over `ℤ`. This is
    the sharpest statement that `~` negates-and-decrements the runtime's signed i64 value. -/
theorem enc_not_toInt (x : Word) : (~~~x).toInt = -x.toInt - 1 := by
  rw [BitVec.toInt_eq_toNat_cond, BitVec.toInt_eq_toNat_cond, BitVec.toNat_not]
  have hlt : x.toNat < 2 ^ 64 := x.isLt
  split <;> split <;> omega

/-- **Two's-complement law `x + ~x = -1` (all-ones).** Adding a value to its bitwise NOT yields the
    all-ones word, `-1`. An independent check that the `bvnot` encoding is the genuine two's-complement
    complement. Discharged by `bv_omega` (omega with bit-vector preprocessing — sound, not `decide`). -/
theorem enc_not_add (x : Word) : x + ~~~x = -1 := by
  bv_omega

end Anubis.Encoding
