/-
  Anubis — mechanized SMT-encoding soundness (Phase 5, slice 1)

  The whole "a green `check` never certifies what `run` violates" guarantee rests on ONE
  correspondence: the QF_BV64 terms the checker emits must denote exactly the i64 semantics the
  runtime executes. This file mechanizes that correspondence for the integer lane in Lean 4 core
  (no Mathlib), modeling both a 64-bit runtime value AND its SMT encoding as `BitVec 64` — because
  a Rust `i64` and an SMT `(_ BitVec 64)` ARE the same object (two's-complement, mod 2^64).

  What is proved here:
    * the arithmetic the encoder emits (`bvadd`/`bvsub`/`bvmul`/`bvand`/`bvor`/`bvxor`) is
      DEFINITIONALLY the wrapping i64 arithmetic the runtime uses (`i64::wrapping_*`);
    * the A1 unsigned-param boundary mask (`x & 0xFFFFFFFF`, commit 4ce6537) lands the value in
      `[0, 2^32)` and is NON-NEGATIVE in the signed interpretation — exactly the range the solver
      soundly injects (`bvsge x 0`, `bvsle x (2^32-1)`) once the runtime enforces the mask.

  These are the load-bearing lemmas; the signed div/rem/shift correspondence is slice 2.
-/

namespace Anubis.Encoding

/-- A 64-bit machine word: a Rust `i64` and an SMT `(_ BitVec 64)` are the same two's-complement
    object, so both the runtime and the encoder are modeled by `Word`. -/
abbrev Word := BitVec 64

/-! ### Arithmetic: encoder = runtime, definitionally

The encoder maps `+ - * & | ^` to `bvadd bvsub bvmul bvand bvor bvxor`; the runtime uses
`i64::wrapping_{add,sub,mul}` and the bitwise ops. On `BitVec 64` these are the SAME functions
(wrapping is the definition of `BitVec` arithmetic), so each correspondence is `rfl`. Stating them
explicitly pins the claim and guards against a future encoder edit silently diverging. -/

/-- `bvadd` (encoder) = wrapping i64 `+` (runtime). -/
theorem enc_add (x y : Word) : x + y = BitVec.add x y := (BitVec.add_eq x y).symm

/-- `bvsub` (encoder) = wrapping i64 `-` (runtime). -/
theorem enc_sub (x y : Word) : x - y = BitVec.sub x y := (BitVec.sub_eq x y).symm

/-- `bvmul` (encoder) = wrapping i64 `*` (runtime). -/
theorem enc_mul (x y : Word) : x * y = BitVec.mul x y := (BitVec.mul_eq x y).symm

/-- The bitwise ops coincide by definition (`&&& ||| ^^^` are `bvand bvor bvxor`). -/
theorem enc_and (x y : Word) : x &&& y = BitVec.and x y := rfl
theorem enc_or  (x y : Word) : x ||| y = BitVec.or  x y := rfl
theorem enc_xor (x y : Word) : x ^^^ y = BitVec.xor x y := rfl

/-! ### Signed comparisons: encoder = runtime

The runtime's i64 `<` and `<=` are SIGNED comparisons — the two's-complement value order that
`BitVec.toInt` denotes. The encoder emits `bvslt`/`bvsle` (the checker uses SIGNED bit-vector
comparisons for `<`/`<=`, matching i64 — a 32-bit unsigned model was proven unsound). They coincide,
so an `ensures`/`requires` comparison the solver proves holds over the runtime's i64 ordering. -/

/-- `bvslt` (encoder) = signed i64 `<` (runtime), both the `toInt` order. -/
theorem enc_slt (x y : Word) : x.slt y = decide (x.toInt < y.toInt) := BitVec.slt_eq_decide

/-- `bvsle` (encoder) = signed i64 `≤` (runtime). -/
theorem enc_sle (x y : Word) : x.sle y = decide (x.toInt ≤ y.toInt) := BitVec.sle_eq_decide

/-! ### A1 unsigned-param boundary mask soundness (commit 4ce6537)

`anubis_coerce_uint_param` masks a `u32` parameter to its low 32 bits at the runtime boundary. The
solver then injects `0 <= x` and `x <= 2^32 - 1` for that parameter. Those two facts are sound
precisely because the masked value provably lies in `[0, 2^32)` AND is non-negative under the SIGNED
interpretation the solver's `bvsge`/`bvsle` use. Both are proved below. -/

/-- The u32 mask: low 32 bits (`0xFFFFFFFF = 2^32 - 1`). -/
def u32Mask : Word := 0xFFFFFFFF

/-- `0xFFFFFFFF` denotes exactly `2^32 - 1`. -/
theorem u32Mask_toNat : u32Mask.toNat = 2^32 - 1 := by decide

/-- **The mask IS reduction mod 2^32.** `anubis_coerce_uint_param` computes `n & 0xFFFFFFFF`, which
    this proves equals `n mod 2^32` — so the runtime boundary genuinely reduces a u32 parameter modulo
    `2^32`, the semantics the "u32 ∈ [0, 2^32)" contract and the SPEC's integer section both assert. -/
theorem u32_mask_eq_mod (x : Word) : (x &&& u32Mask).toNat = x.toNat % 2^32 := by
  rw [BitVec.toNat_and, u32Mask_toNat]
  exact Nat.and_two_pow_sub_one_eq_mod x.toNat 32

/-- **A1 upper bound.** A masked u32 param is `< 2^32` — the sound justification for the injected
    `bvsle x (2^32-1)`. Proof: `(x &&& mask).toNat = x.toNat &&& (2^32-1)`, and `&&&` with a value
    `< 2^n` is itself `< 2^n` (`Nat.and_lt_two_pow`). -/
theorem u32_mask_lt (x : Word) : (x &&& u32Mask).toNat < 2^32 := by
  rw [BitVec.toNat_and, u32Mask_toNat]
  exact Nat.and_lt_two_pow x.toNat (by decide)

/-- **A1 lower bound (signed).** A masked u32 param is NON-NEGATIVE in the signed i64 view — the
    sound justification for the injected `bvsge x 0`. Because the masked value is `< 2^32 <= 2^63`,
    its `toInt` equals its `toNat` (no sign bit set), which is `≥ 0`. -/
theorem u32_mask_nonneg (x : Word) : 0 ≤ (x &&& u32Mask).toInt := by
  have hlt : (x &&& u32Mask).toNat < 2^32 := u32_mask_lt x
  have h : 2 * (x &&& u32Mask).toNat < 2^64 := by omega
  rw [BitVec.toInt_eq_toNat_of_lt h]
  exact Int.natCast_nonneg _

end Anubis.Encoding
