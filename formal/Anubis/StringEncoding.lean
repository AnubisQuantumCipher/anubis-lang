/-
  Anubis — mechanized SMT-encoding soundness (Phase 5): the QF_S string lane

  Companion to `Anubis.Encoding` (the integer QF_BV64 lane). The checker's string lanes
  (commits eb5b8d9 / f30e78d / 7990cf0 / 9ad2410 — `==`, `str.len`, `++`, and the len-sum lane)
  emit QF_S terms: a string literal/param becomes an SMT `String`, `==` becomes `(= s t)`,
  `len(s)` becomes `(str.len s)` interpreted as an SMT `Int`, and `a ++ b` becomes `(str.++ a b)`.
  The "a green `check` never certifies what `run` violates" guarantee needs those QF_S terms to
  denote exactly the runtime's `String` semantics.

  The runtime (transpiled Rust) executes a string as a sequence of characters; equality is
  structural, `len` is the character count, and `++` is sequence concatenation. We model an Anubis
  string as a Lean `List Char` — the faithful denotation of that sequence — and pin the three
  correspondences the string lanes rest on:

    * EQUALITY IS EXACT: the encoded equality (modeled as `decide (s = t)`, the boolean an SMT
      `(= s t)` denotes) is `true` iff `s = t` structurally. So a check-proved `ensures(x == "lit")`
      holds precisely when the runtime `String` equality it compiles to holds — no over- or
      under-approximation.
    * LENGTH IS FAITHFUL AND NON-NEGATIVE: `(s.length : Int)` models `str.len` and is always `≥ 0`
      (SMT `str.len` is a total non-negative function), and it distributes over concatenation:
      `str.len (a ++ b) = str.len a + str.len b`. This is exactly the identity the `len(a)+len(b)`
      SMT-Int len-sum lane (9ad2410) relies on.
    * CONCATENATION IS ASSOCIATIVE with the empty string as identity — the monoid laws SMT `str.++`
      obeys, so a checker rewrite that reassociates a `++` chain cannot change the modeled value.

  A NON-VACUITY witness (`encEq ['a'] ['b'] = false`, `['a'] ≠ ['b']`) shows the equality model is
  not trivially always-true.

  HONEST RESIDUAL: only equality / length / concatenation are modeled here. `str.indexof` and
  `str.substr` have runtime-divergence subtleties (byte- vs char-index conventions, out-of-range
  clamping vs the SMT `-1`/`""` conventions) and are handled as a sound SUBSET in the checker, not a
  total correspondence; they are deliberately NOT claimed sound in full here. Core Lean only,
  sorry-free.
-/

import Anubis.Encoding

namespace Anubis.StringEncoding

/-- An Anubis string value: the runtime executes a string as a sequence of characters, so both the
    runtime value and the SMT `String` the encoder emits are modeled by `List Char`. -/
abbrev Str := List Char

/-! ### Equality is exact

The encoder maps `==` on strings to the SMT `(= s t)`, whose denotation is the boolean "are these
two strings the same sequence". We model that boolean as `decide (s = t)` and prove it is `true`
exactly when the two sequences are structurally equal — the runtime's `String` equality. -/

/-- Model of the SMT `(= s t)` boolean: `decide` of structural sequence equality. -/
def encEq (s t : Str) : Bool := decide (s = t)

/-- **Equality is exact.** The encoded equality boolean is `true` iff the strings are structurally
    equal. So `ensures(x == "lit")` proved by the checker matches runtime `String` equality with no
    slack in either direction. -/
theorem encEq_iff (s t : Str) : encEq s t = true ↔ s = t := by
  unfold encEq
  exact decide_eq_true_iff

/-- Equality is reflexive: `s == s` always encodes to `true` (a self-comparison the checker may fold). -/
theorem encEq_refl (s : Str) : encEq s s = true := by
  rw [encEq_iff]

/-- Equality is symmetric, matching the SMT `(= s t) = (= t s)`. -/
theorem encEq_symm (s t : Str) : encEq s t = encEq t s := by
  unfold encEq
  by_cases h : s = t
  · simp [h]
  · have h' : t ≠ s := fun e => h e.symm
    simp [h, h']

/-! ### Length is faithful and non-negative

`str.len` is modeled by the character count cast into SMT `Int`. It is total and non-negative, and
it distributes over `str.++` — the identity the `len(a)+len(b)` len-sum lane depends on. -/

/-- Model of `str.len s`: the character count as an SMT `Int`. -/
def encLen (s : Str) : Int := (s.length : Int)

/-- **`str.len` is non-negative.** SMT `str.len` is a total function into the non-negative integers;
    the model honours that, so any `len(...)` the checker introduces is soundly `≥ 0`. -/
theorem encLen_nonneg (s : Str) : 0 ≤ encLen s := by
  unfold encLen
  exact Int.natCast_nonneg _

/-- **`str.len` of concatenation splits additively (over `Int`).** This is the exact fact the
    `len(a)+len(b)` SMT-Int len-sum lane (9ad2410) uses: `str.len (a ++ b) = str.len a + str.len b`,
    encoded in the same `Int` sort the lane sums in. -/
theorem encLen_append (a b : Str) : encLen (a ++ b) = encLen a + encLen b := by
  unfold encLen
  rw [List.length_append]
  omega

/-- The empty string has length `0` (SMT `(str.len "") = 0`). -/
theorem encLen_nil : encLen ([] : Str) = 0 := rfl

/-! ### Concatenation is a monoid: associativity + empty identity

SMT `str.++` is associative with `""` as the two-sided identity. Modeling `++` as `List.append`
inherits exactly those laws, so a checker rewrite that reassociates or drops an empty operand in a
`++` chain preserves the modeled value. -/

/-- Model of `str.++`: sequence concatenation. -/
def encCat (a b : Str) : Str := a ++ b

/-- **`str.++` is associative.** A reassociating rewrite of a `++` chain is value-preserving. -/
theorem encCat_assoc (a b c : Str) : encCat (encCat a b) c = encCat a (encCat b c) := by
  unfold encCat
  exact List.append_assoc a b c

/-- Left identity: `"" ++ s = s`. -/
theorem encCat_nil_left (s : Str) : encCat [] s = s := by
  unfold encCat
  exact List.nil_append s

/-- Right identity: `s ++ "" = s`. -/
theorem encCat_nil_right (s : Str) : encCat s [] = s := by
  unfold encCat
  exact List.append_nil s

/-- Length composes through the concatenation model, tying `encCat` back to the len-sum lane. -/
theorem encLen_encCat (a b : Str) : encLen (encCat a b) = encLen a + encLen b := by
  unfold encCat
  exact encLen_append a b

/-! ### Non-vacuity

The equality model is not trivially always-true: two distinct single-character strings are provably
unequal, and the encoded equality is `false` on them. -/

/-- A concrete distinct pair, so `encEq` is not vacuously `true`. -/
theorem encEq_distinct : encEq ['a'] ['b'] = false := by decide

/-- The underlying strings really are distinct (the witness `encEq` distinguishes). -/
theorem strings_distinct : (['a'] : Str) ≠ ['b'] := by decide

end Anubis.StringEncoding
