/-
  Anubis — mechanized SMT-encoding soundness (Phase 5, slice: signed div/rem/shift)

  The whole "a green `check` never certifies what `run` violates" guarantee rests on the checker's
  SMT terms denoting EXACTLY the i64 semantics the runtime executes. Slice 1 (`Anubis.Encoding`)
  pinned the wrapping arithmetic, bitwise ops, signed comparisons, and the u32 boundary mask. This
  file discharges the harder SIGNED integer operations, where a naive encoder is easy to get subtly
  wrong (Euclidean vs truncating division, `bvsmod` vs `bvsrem`, logical vs arithmetic shift, the
  `MIN / -1` overflow, and the shift-amount reduction).

  Runtime (compiler `run.rs`) executes, over `i64` modeled here as `BitVec 64`:
    * `/`  → `i64::wrapping_div`  — truncates TOWARD ZERO; `MIN / -1` WRAPS to `MIN`.
    * `%`  → `i64::wrapping_rem`  — remainder takes the sign of the DIVIDEND.
    * `<<` → `i64::wrapping_shl`  — shift amount reduced mod 64 (`rem_euclid(64)` = low 6 bits).
    * `>>` → `i64::wrapping_shr`  — ARITHMETIC (sign-extending) right shift, amount mod 64.
  Checker (`mod.rs`) emits, over `(_ BitVec 64)`:
    * `/`  → `bvsdiv`
    * `%`  → `bvsrem`                          (NOT `bvsmod` — that would follow the divisor's sign)
    * `<<` → `(bvshl  l (bvurem r 64))`
    * `>>` → `(bvashr l (bvurem r 64))`        (arithmetic, NOT `bvlshr`)

  What is proved here (each a MEANINGFUL correspondence, most at the true runtime width 64):
    * `enc_sdiv` / `enc_srem` : the `toInt` of the emitted `bvsdiv`/`bvsrem` equals the Int-level
      truncated division / truncated remainder — `bvsdiv` reduced back into range (`bmod 2^64`, which
      is what makes `MIN / -1` wrap) and `bvsrem` with no correction (a remainder never overflows).
    * `sdiv_truncates_not_euclid` / `srem_sign_of_dividend` : concrete witnesses that the encoding is
      TRUNCATING (`Int.tdiv`/`Int.tmod`), which DIFFERS from Euclidean (`Int.ediv`/`Int.emod`). This is
      the load-bearing distinction: `bvsrem` (chosen) vs `bvsmod` (rejected) diverge on a negative
      dividend, and `enc_srem_neg7_two` shows the emitted term genuinely computes the Rust value `-1`.
    * `enc_sdiv_min_wrap` / `enc_srem_min_no_wrap` : the `MIN / -1` overflow wraps to `MIN` (matching
      `i64::wrapping_div`) while `MIN % -1 = 0` needs no wrap — the exact `bmod`-only-on-div asymmetry.
    * `shift_amount_urem` / `shift_amount_mask_eq` / `shift_amount_lt` : `bvurem r 64` reduces the
      shift amount to `r.toNat % 64`, equals a low-6-bit mask `r &&& 63`, and is always a valid `0..63`
      shift — so both `bvshl`/`bvashr` receive exactly the runtime's `rem_euclid(64)` amount.
    * `enc_ashr` / `ashr_sign_extends` : the emitted `bvashr` is arithmetic (`toInt` = Int `>>>`,
      sign-extending), witnessed by `(-8) >> 1 = -4` where a LOGICAL shift would give a large positive.
    * `enc_shl_sign_bit` : left-shifting `1` to the top bit yields `i64::MIN`, tying `bvshl` to the
      signed two's-complement interpretation the runtime shares.

  If any Lean/SMT/Rust semantics had diverged it would be a genuine encoder bug; none was found — the
  Lean `BitVec.sdiv/srem/sshiftRight` core semantics match SMT-LIB `bvsdiv/bvsrem/bvashr` and Rust's
  `wrapping_div/rem/shr` on all cases checked, including the `MIN / -1` corner.
-/

import Anubis.Encoding

namespace Anubis.IntSigned

open Anubis.Encoding

/-! ### Signed division and remainder: encoder = runtime -/

/-- **`bvsdiv` correspondence.** The `toInt` of the emitted `bvsdiv` is the Int-level TRUNCATED
    (toward-zero) quotient reduced back into signed range. The `bmod 2^64` is not decoration: it is
    exactly the correction that makes `MIN / -1` wrap to `MIN` (see `enc_sdiv_min_wrap`); on every
    non-overflowing input the quotient is already in range and `bmod` is the identity. -/
theorem enc_sdiv (x y : Word) :
    (x.sdiv y).toInt = (x.toInt.tdiv y.toInt).bmod (2 ^ 64) :=
  BitVec.toInt_sdiv x y

/-- **`bvsrem` correspondence.** The `toInt` of the emitted `bvsrem` is the Int-level TRUNCATED
    remainder `Int.tmod`, which takes the sign of the DIVIDEND — matching `i64::wrapping_rem`. No
    `bmod` correction appears because a truncated remainder is always in range. -/
theorem enc_srem (x y : Word) :
    (x.srem y).toInt = x.toInt.tmod y.toInt :=
  BitVec.toInt_srem x y

/-- **Truncating, not Euclidean.** On a negative dividend, the truncating quotient the encoder uses
    (`Int.tdiv`, matching Rust `/`) differs from the Euclidean quotient (`Int.ediv`). If `mod.rs` had
    emitted a Euclidean-division encoding, a `check` could certify a quotient the `run` never produces.
    `(-7) / 2` is `-3` (truncated toward zero) but Euclidean division floors it to `-4`. -/
theorem sdiv_truncates_not_euclid :
    ((-7 : Int).tdiv 2 = -3) ∧ ((-7 : Int).ediv 2 = -4) := by decide

/-- **`bvsrem`, NOT `bvsmod`.** The load-bearing choice for `%`: a truncated remainder takes the sign
    of the DIVIDEND (`Int.tmod (-7) 2 = -1`, the Rust value), whereas `bvsmod`/Euclidean remainder
    would take the sign of the DIVISOR (`Int.emod (-7) 2 = 1`). These disagree, so encoding `%` as
    `bvsmod` would be unsound; `bvsrem` is correct. -/
theorem srem_sign_of_dividend :
    ((-7 : Int).tmod 2 = -1) ∧ ((-7 : Int).emod 2 = 1) := by decide

/-- The emitted `bvsrem` term at the true runtime width genuinely evaluates to the Rust `%` result
    `-1` on `(-7) % 2` — a concrete end-to-end witness of `enc_srem` disagreeing with `bvsmod` (`+1`). -/
theorem enc_srem_neg7_two : ((-7 : Word).srem 2).toInt = -1 := by
  rw [enc_srem]; decide

/-- **`MIN / -1` wraps to `MIN`.** The one signed-division overflow: mathematically `-2^63 / -1 = 2^63`
    is unrepresentable in `i64`, and BOTH `i64::wrapping_div` and `bvsdiv` wrap it back to `MIN`. This
    is precisely where the `bmod 2^64` in `enc_sdiv` does real work. -/
theorem enc_sdiv_min_wrap : (BitVec.intMin 64).sdiv (-1) = BitVec.intMin 64 := by decide

/-- **`MIN % -1 = 0`, no wrap.** The remainder counterpart never overflows, so `enc_srem` carries no
    `bmod` — `bvsrem MIN (-1)` is `0`, matching `i64::wrapping_rem(MIN, -1)`. This asymmetry (div wraps,
    rem does not) is exactly mirrored by the `bmod`-on-div / no-correction-on-rem shape of the encoder. -/
theorem enc_srem_min_no_wrap : (BitVec.intMin 64).srem (-1) = 0 := by decide

/-! ### Shift-amount reduction: `bvurem r 64` = `rem_euclid(64)` = low 6 bits -/

/-- **`bvurem r 64` = `r mod 64`.** The emitted shift amount `(bvurem r 64)` reduces to `r.toNat % 64`,
    exactly the `rem_euclid(64)` the runtime applies to a shift count. -/
theorem shift_amount_urem (r : Word) : (r % 64).toNat = r.toNat % 64 := by
  simp [BitVec.toNat_umod]

/-- **The reduction is a low-6-bit mask.** `bvurem r 64` equals `r &&& 63` — reducing a shift amount
    mod 64 keeps exactly the low 6 bits, the standard hardware shift-count semantics the runtime uses. -/
theorem shift_amount_mask_eq (r : Word) : r % 64 = r &&& 63 := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_umod, BitVec.toNat_and]
  show r.toNat % 64 = r.toNat &&& (2 ^ 6 - 1)
  rw [Nat.and_two_pow_sub_one_eq_mod r.toNat 6]

/-- **The reduced amount is a valid shift.** After `bvurem _ 64` the shift count is always `< 64`, so
    the `bvshl`/`bvashr` the encoder feeds it are well-defined (no over-shift), matching the runtime. -/
theorem shift_amount_lt (r : Word) : (r % 64).toNat < 64 := by
  rw [BitVec.toNat_umod]; exact Nat.mod_lt _ (by decide)

/-! ### Arithmetic right shift and left shift -/

/-- **`bvashr` is arithmetic.** The `toInt` of the emitted `bvashr` is the Int-level arithmetic right
    shift `x.toInt >>> n` (sign-extending, i.e. floor-division by `2^n`) — matching `i64::wrapping_shr`
    on signed `i64`. Had the encoder emitted `bvlshr` (logical), a negative operand would be certified
    with a large positive result the runtime never produces (see `ashr_sign_extends`). -/
theorem enc_ashr (x : Word) (n : Nat) : (x.sshiftRight n).toInt = x.toInt >>> n :=
  BitVec.toInt_sshiftRight

/-- **Sign extension, witnessed.** `(-8) >> 1` is `-4` under the emitted arithmetic `bvashr`, whereas a
    LOGICAL shift of the same bit pattern yields the large positive `2^63 - 4`. The two disagree, so the
    `bvashr`-not-`bvlshr` choice is load-bearing; the arithmetic result is the Rust one. -/
theorem ashr_sign_extends :
    ((-8 : Word).sshiftRight 1).toInt = -4 ∧ ((-8 : Word) >>> 1).toInt = 2 ^ 63 - 4 := by
  refine ⟨?_, by decide⟩
  rw [enc_ashr]; decide

/-- **`bvshl` respects the signed interpretation.** Left-shifting `1` into the top bit produces the
    two's-complement `i64::MIN` (`-2^63`), tying the emitted `bvshl` to the same signed reading of the
    64-bit word that `bvsdiv`/`bvsrem`/`bvashr` and the runtime all use. -/
theorem enc_shl_sign_bit : ((1 : Word) <<< 63).toInt = -(2 ^ 63) := by decide

end Anubis.IntSigned
