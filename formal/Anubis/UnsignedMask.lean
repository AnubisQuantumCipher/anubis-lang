/-
  Anubis — mechanized UNSIGNED fixed-width boundary-mask soundness (Phase 5)

  Anubis's runtime reduces an unsigned value at a fixed-width boundary: a `u8`/`u16`/`u32`
  parameter (or a truncating cast `x as u8`) is `x & (2^w - 1)` — the low `w` bits — because a Rust
  narrowing cast keeps exactly those bits. The CHECKER relies on this: at that boundary it injects
  `0 <= x` and `x <= 2^w - 1` into the SMT context (the A1 result, commit 4ce6537, was the `u32`
  special case). This file generalizes A1 to ALL widths `w ≤ 32` and to the truncating-cast model,
  mechanizing three facts the solver depends on for EVERY unsigned width:

    * `maskW_lt`      — a masked value is `< 2^w`         (justifies the injected `bvsle x (2^w-1)`);
    * `maskW_eq_mod`  — the mask IS reduction mod `2^w`   (the runtime boundary genuinely reduces);
    * `maskW_nonneg`  — a masked value is signed-non-negative for `w ≤ 32` (justifies `bvsge x 0`,
                        since `2^w ≤ 2^63` leaves the i64 sign bit clear).

  It then mechanizes why the checker MUST fail closed on a truncating cast: modeling `x as u8` as the
  IDENTITY (`(x as u8) = x`) is UNSOUND. `u8_cast_eq_mod` proves the correct denotation is
  `x.toNat % 256`, and `u8_cast_not_identity` / `mask_identity_model_false` EXHIBIT a concrete `x`
  (256) where `x & 0xFF = 0 ≠ 256` — a green check under the identity model would certify a value the
  runtime never produces. The generalization is faithful: `maskW 32` is exactly `Encoding.u32Mask`
  (`maskW_32_eq_u32Mask`), so A1 is the `w = 32` instance of these theorems.

  Core Lean 4 only (no Mathlib); reuses `Anubis.Encoding` (`Word`, `u32Mask`). Sorry/axiom-free.
-/
import Anubis.Encoding

namespace Anubis.UnsignedMask

open Anubis.Encoding

/-- The general unsigned width-`w` boundary mask `2^w - 1`, as a 64-bit word. For `w ≤ 64` this is
    exactly the low `w` bits set; `anubis_coerce_uint_param` and a truncating cast `x as u<w>` both
    compute `x &&& maskW w`. -/
def maskW (w : Nat) : Word := BitVec.ofNat 64 (2^w - 1)

/-- For `w ≤ 64` the mask denotes exactly `2^w - 1` (no wrap in the 64-bit literal). -/
theorem maskW_toNat {w : Nat} (hw : w ≤ 64) : (maskW w).toNat = 2^w - 1 := by
  unfold maskW
  rw [BitVec.toNat_ofNat]
  apply Nat.mod_eq_of_lt
  have hle : 2^w ≤ 2^64 := Nat.pow_le_pow_right (by decide) hw
  have hpos : 0 < 2^64 := Nat.two_pow_pos 64
  omega

/-- **The mask IS reduction mod `2^w`.** The runtime boundary computes `x &&& (2^w - 1)`, which this
    proves equals `x mod 2^w` — so an unsigned `u<w>` parameter (or truncating cast) genuinely lands
    in `[0, 2^w)` by modular reduction, the semantics the checker's injected range facts assert. -/
theorem maskW_eq_mod {w : Nat} (hw : w ≤ 64) (x : Word) :
    (x &&& maskW w).toNat = x.toNat % 2^w := by
  rw [BitVec.toNat_and, maskW_toNat hw]
  exact Nat.and_two_pow_sub_one_eq_mod x.toNat w

/-- **Upper bound.** A masked `u<w>` value is `< 2^w` — the sound justification for the injected
    `bvsle x (2^w - 1)`. Direct consequence of `maskW_eq_mod` and `Nat.mod_lt`. -/
theorem maskW_lt {w : Nat} (hw : w ≤ 64) (x : Word) : (x &&& maskW w).toNat < 2^w := by
  rw [maskW_eq_mod hw]
  exact Nat.mod_lt _ (Nat.two_pow_pos w)

/-- **Lower bound (signed).** For `w ≤ 32` a masked value is NON-NEGATIVE in the signed i64 view —
    the sound justification for the injected `bvsge x 0`. Because the value is `< 2^w ≤ 2^32 ≤ 2^63`,
    the i64 sign bit is clear so `toInt = toNat ≥ 0`. -/
theorem maskW_nonneg {w : Nat} (hw : w ≤ 32) (x : Word) : 0 ≤ (x &&& maskW w).toInt := by
  have hle : w ≤ 64 := by omega
  have hlt : (x &&& maskW w).toNat < 2^w := maskW_lt hle x
  have hbound : 2^w ≤ 2^32 := Nat.pow_le_pow_right (by decide) hw
  have hlt32 : (x &&& maskW w).toNat < 2^32 := by omega
  have h : 2 * (x &&& maskW w).toNat < 2^64 := by
    have h33 : (2:Nat)^33 ≤ 2^64 := Nat.pow_le_pow_right (by decide) (by omega)
    have he : (2:Nat)^33 = 2 * 2^32 := by decide
    omega
  rw [BitVec.toInt_eq_toNat_of_lt h]
  exact Int.natCast_nonneg _

/-! ### Concrete unsigned widths the checker actually emits (`u8`, `u16`, `u32`) -/

/-- `u8` mask lands in `[0, 256)`. -/
theorem u8_mask_lt (x : Word) : (x &&& maskW 8).toNat < 256 := by
  have := maskW_lt (by decide : (8:Nat) ≤ 64) x
  simpa using this

/-- `u16` mask lands in `[0, 65536)`. -/
theorem u16_mask_lt (x : Word) : (x &&& maskW 16).toNat < 65536 := by
  have := maskW_lt (by decide : (16:Nat) ≤ 64) x
  simpa using this

/-- `u32` mask lands in `[0, 2^32)` and is signed-non-negative — the exact A1 facts, now as instances
    of the generic width-`w` theorems. -/
theorem u32_mask_lt (x : Word) : (x &&& maskW 32).toNat < 4294967296 := by
  have := maskW_lt (by decide : (32:Nat) ≤ 64) x
  simpa using this

theorem u32_mask_nonneg (x : Word) : 0 ≤ (x &&& maskW 32).toInt :=
  maskW_nonneg (by decide) x

/-- The generalization is faithful: the generic `maskW 32` is exactly `Encoding.u32Mask` (A1's
    `0xFFFFFFFF`). So A1 is precisely the `w = 32` instance of the theorems above — no divergence. -/
theorem maskW_32_eq_u32Mask : maskW 32 = Encoding.u32Mask := by decide

/-! ### Truncating cast `x as u8`: the identity model is UNSOUND -/

/-- **Correct denotation of a truncating cast.** `x as u8` denotes `x.toNat % 256` (the low byte),
    NOT `x`. Instance of `maskW_eq_mod` at `w = 8`. -/
theorem u8_cast_eq_mod (x : Word) : (x &&& maskW 8).toNat = x.toNat % 256 := by
  have := maskW_eq_mod (by decide : (8:Nat) ≤ 64) x
  simpa using this

/-- Concrete truncation: `256 as u8 = 0`. The high bit is dropped. -/
theorem u8_cast_256 : (256 : Word) &&& maskW 8 = 0 := by decide

/-- **The identity model `(x as u8) = x` is FALSE.** Witness `x = 256`: `256 & 0xFF = 0 ≠ 256`. A
    checker that modeled a truncating cast as the identity would emit an SMT term denoting `256` while
    the runtime produces `0` — a green check certifying a value `run` never yields. This is exactly
    why the checker must fail closed (mod.rs) on a truncating cast rather than treat it as identity. -/
theorem u8_cast_not_identity : ∃ x : Word, x &&& maskW 8 ≠ x :=
  ⟨256, by decide⟩

/-- The same unsoundness at the `toNat` level, side by side: the mod-model value (`0`) and the
    identity-model value (`256`) disagree, so the two models are NOT interchangeable. -/
theorem mask_identity_model_false :
    (256 : Word).toNat % 256 = 0 ∧ (256 : Word).toNat = 256 ∧ (0 : Nat) ≠ 256 := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

end Anubis.UnsignedMask
