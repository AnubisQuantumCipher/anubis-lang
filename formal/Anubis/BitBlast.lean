/-
  Anubis — mechanized NATIVE BIT-BLASTER soundness (Phase 7, TCB minimization)

  The native QF_BV solver (`solver/`, replacing the z3 third party for the integer lane) bit-blasts each
  bit-vector operation to boolean gates. The load-bearing gate is the ripple-carry ADDER
  (`solver/src/blast.rs::full_adder`/`add_carry`): every `bvadd`, and via two's complement every
  `bvsub`/`bvneg`, is built from it, and the comparators are built from the subtractor's borrow. If the
  adder is wrong, the native solver's UNSAT (a "proof") could be false — a false accept.

  This file machine-checks the adder in Lean 4 core (no Mathlib): the exact gates the blaster emits
  (`sum = a⊕b⊕cin`, `cout = maj(a,b,cin)`) compute the true integer sum. Chained with
  `Anubis/Encoding.lean` (which proves the SMT/runtime `bvadd` = `i64::wrapping_add` over `BitVec 64`),
  this closes the loop: native bit-blast adder = integer addition = SMT bvadd = runtime wrapping add.

  THEOREMS:
    * `fullAdder_spec`   — the 1-bit full adder's defining invariant: `sum + 2·cout = a + b + cin`.
    * `rippleCarry_spec` — the w-bit ripple-carry adder computes the exact integer sum:
      `⟦sum⟧ + 2^w·cout = ⟦a⟧ + ⟦b⟧ + cin`. (So mod 2^w it is bit-vector addition, and the carry-out is
      the unsigned overflow bit the comparators read.)
-/

namespace Anubis.BitBlast

/-- The 1-bit full adder exactly as `solver/src/blast.rs::full_adder` emits it:
    `sum = a ⊕ b ⊕ cin`, `cout = (a ∧ b) ∨ (cin ∧ (a ⊕ b))`. -/
def fullAdder (a b cin : Bool) : Bool × Bool :=
  let axb := xor a b
  (xor axb cin, (a && b) || (cin && axb))

/-- **1-bit adder invariant.** The output value equals the input value: `sum + 2·cout = a + b + cin`
    (as naturals, via `Bool.toNat`). Decided by exhausting the 8 boolean cases. -/
theorem fullAdder_spec (a b cin : Bool) :
    (fullAdder a b cin).1.toNat + 2 * (fullAdder a b cin).2.toNat
      = a.toNat + b.toNat + cin.toNat := by
  cases a <;> cases b <;> cases cin <;> decide

/-- Value of an LSB-first bit list as a natural number. -/
def bitsToNat : List Bool → Nat
  | [] => 0
  | b :: bs => b.toNat + 2 * bitsToNat bs

/-- Ripple-carry adder of two equal-length LSB-first bit lists with a carry-in, returning the sum bits
    and the final carry-out — the `add_carry` loop of the bit-blaster. -/
def rippleCarry : List Bool → List Bool → Bool → List Bool × Bool
  | [], _, cin => ([], cin)
  | _ :: _, [], cin => ([], cin)
  | a :: as, b :: bs, cin =>
      let (s, c) := fullAdder a b cin
      let (rest, cout) := rippleCarry as bs c
      (s :: rest, cout)

/-- **Ripple-carry adder correctness.** For equal-length operands, the adder computes the exact integer
    sum: the value of the sum bits plus `2^w` times the carry-out equals `⟦a⟧ + ⟦b⟧ + cin`. Hence the
    low `w` bits are `(a + b + cin) mod 2^w` — bit-vector (wrapping) addition — and the carry-out is the
    unsigned-overflow flag the ≤/< comparators are built from. -/
theorem rippleCarry_spec :
    ∀ (as bs : List Bool) (cin : Bool), as.length = bs.length →
      bitsToNat (rippleCarry as bs cin).1
          + 2 ^ as.length * (rippleCarry as bs cin).2.toNat
        = bitsToNat as + bitsToNat bs + cin.toNat := by
  intro as
  induction as with
  | nil =>
      intro bs cin h
      cases bs with
      | nil => simp [rippleCarry, bitsToNat]
      | cons _ _ => simp at h
  | cons a as ih =>
      intro bs cin h
      cases bs with
      | nil => simp at h
      | cons b bs =>
          have hlen : as.length = bs.length := by
            simpa [List.length_cons] using h
          -- The full adder on the low bit; `c` its carry into the recursive call.
          have hfa := fullAdder_spec a b cin
          have hih := ih bs (fullAdder a b cin).2 hlen
          -- Unfold one ripple step and relate 2^(w+1) to 2·2^w so the two powers share an atom.
          simp only [rippleCarry, bitsToNat, List.length_cons, Nat.pow_succ]
          -- The remaining nonlinearity is `2^w · cout`. Case on the recursive carry-out (a Bool) so it
          -- becomes 0 or 2^w, then the goal is linear in the atoms {2^as.length, ⟦as⟧, ⟦bs⟧, the toNats}:
          --   hfa : S + 2·C = a + b + cin ;  hih : R + 2^w·O = ⟦as⟧ + ⟦bs⟧ + C.  omega closes it.
          cases hO : (rippleCarry as bs (fullAdder a b cin).2).2 <;>
            simp only [hO, Bool.toNat_true, Bool.toNat_false, Nat.mul_one, Nat.mul_zero,
              Nat.add_zero] at hih ⊢ <;>
            omega

/-- The value of any bit list is below `2^w`. -/
theorem bitsToNat_lt : ∀ bs : List Bool, bitsToNat bs < 2 ^ bs.length := by
  intro bs
  induction bs with
  | nil => simp [bitsToNat]
  | cons b bs ih =>
      simp only [bitsToNat, List.length_cons, Nat.pow_succ]
      have : b.toNat ≤ 1 := by cases b <;> simp
      omega

/-- Bitwise complement negates the value within the word: `⟦~bs⟧ = 2^w − 1 − ⟦bs⟧`. -/
theorem bitsToNat_not : ∀ bs : List Bool,
    bitsToNat (bs.map (fun x => !x)) = 2 ^ bs.length - 1 - bitsToNat bs := by
  intro bs
  induction bs with
  | nil => simp [bitsToNat]
  | cons b bs ih =>
      have hlt := bitsToNat_lt bs
      simp only [List.map_cons, bitsToNat, List.length_cons, Nat.pow_succ]
      cases b <;> simp_all <;> omega

/-- Complement identity in ADDITION form (no truncated subtraction — omega-friendly):
    `⟦~bs⟧ + ⟦bs⟧ + 1 = 2^w`. This is what the two's-complement subtractor rests on. -/
theorem bitsToNat_not_add : ∀ bs : List Bool,
    bitsToNat (bs.map (fun x => !x)) + bitsToNat bs + 1 = 2 ^ bs.length := by
  intro bs
  induction bs with
  | nil => simp [bitsToNat]
  | cons b bs ih =>
      simp only [List.map_cons, bitsToNat, List.length_cons, Nat.pow_succ]
      cases b <;> simp_all <;> omega

/-- The ripple-carry sum has the width of its (equal-length) operands. -/
theorem rippleCarry_length : ∀ (a b : List Bool) (c : Bool), a.length = b.length →
    (rippleCarry a b c).1.length = a.length := by
  intro a
  induction a with
  | nil => intro b c _; simp [rippleCarry]
  | cons x xs ih =>
      intro b c h
      cases b with
      | nil => simp at h
      | cons y ys =>
          have hlen : xs.length = ys.length := by simpa using h
          simp only [rippleCarry, List.length_cons]
          rw [ih ys (fullAdder x y c).2 hlen]

/-- The unsigned comparator the bit-blaster emits: `ult a b = ¬(carry-out of a + ~b + 1)`
    (`solver/src/blast.rs::ult`). The two's-complement subtractor `a - b = a + ~b + 1` overflows
    (carry-out = 1) exactly when `a ≥ b`, so `¬carry` is `a < b`. -/
def ult (a b : List Bool) : Bool :=
  !(rippleCarry a (b.map (fun x => !x)) true).2

/-- **Unsigned comparator correctness (fully mechanized).** For equal-length operands,
    `ult a b = true ↔ ⟦a⟧ < ⟦b⟧`. Proof: the subtractor spec (`rippleCarry_spec` on `a`, `~b`,
    carry-in 1) gives `⟦sum⟧ + 2^w·cout = ⟦a⟧ + ⟦~b⟧ + 1`, and the ADDITION-form complement identity
    (`bitsToNat_not_add`) gives `⟦~b⟧ + ⟦b⟧ + 1 = 2^w`. Casing on the carry-out `cout` and feeding both
    equations (no truncated subtraction anywhere) to omega, together with the range bounds, decides it.
    Chains to the runtime: the native `ult` gate = unsigned `<` = `run.rs` comparison. -/
theorem ult_correct (a b : List Bool) (h : a.length = b.length) :
    (ult a b = true) ↔ (bitsToNat a < bitsToNat b) := by
  have hlen : a.length = (b.map (fun x => !x)).length := by
    simpa [List.length_map] using h
  have hspec := rippleCarry_spec a (b.map (fun x => !x)) true hlen
  have hnadd := bitsToNat_not_add b
  have hla := bitsToNat_lt a
  have hlb := bitsToNat_lt b
  have hsumlt := bitsToNat_lt (rippleCarry a (b.map (fun x => !x)) true).1
  rw [rippleCarry_length a (b.map (fun x => !x)) true hlen] at hsumlt
  -- Normalise every power/length to `2 ^ b.length` so omega sees ONE opaque nonneg atom (not two).
  rw [h] at hla hsumlt hspec
  simp only [Bool.toNat_true] at hspec
  unfold ult
  -- Case on the subtractor's carry-out. Each branch feeds omega the ADDITION-form facts
  --   hspec (sum), hnadd (⟦~b⟧+⟦b⟧+1 = 2^w), and the range bounds — all linear, no truncated sub.
  cases hco : (rippleCarry a (b.map (fun x => !x)) true).2
  · simp only [hco, Bool.toNat_false, Nat.mul_zero, Nat.add_zero] at hspec
    have hlt : bitsToNat a < bitsToNat b := by omega
    simp only [Bool.not_false]
    simpa using hlt
  · simp only [hco, Bool.toNat_true, Nat.mul_one] at hspec
    have hge : ¬ bitsToNat a < bitsToNat b := by omega
    simp only [Bool.not_true]
    simpa using hge

/-! ### Signed comparator

`blast.rs::slt(a,b)` flips the sign bit (the last/MSB entry of the LSB-first list) of BOTH operands
then does an unsigned compare — the textbook "offset binary" trick. We mechanize that this exactly
computes the two's-complement signed `<`, chaining off `ult_correct`. -/

/-- Toggle the sign bit (the LAST/MSB entry of the LSB-first list) — the flip `blast.rs::slt` does. -/
def flipMsb : List Bool → List Bool
  | [] => []
  | [b] => [!b]
  | b :: bs => b :: flipMsb bs

/-- Two's-complement (signed) value of an LSB-first bit list: the top bit carries the sign
    (`[b]` = a lone sign bit worth `-b`; a longer list adds `2 ·` the signed value of the rest). -/
def toIntW : List Bool → Int
  | [] => 0
  | [b] => -(b.toNat : Int)
  | b :: bs => (b.toNat : Int) + 2 * toIntW bs

/-- Flipping the sign bit preserves width. -/
theorem flipMsb_length : ∀ a : List Bool, (flipMsb a).length = a.length := by
  intro a
  induction a with
  | nil => rfl
  | cons b bs ih =>
      cases bs with
      | nil => rfl
      | cons c cs => simp only [flipMsb, List.length_cons, ih]

/-- **Offset-binary identity.** For a nonempty list, the UNSIGNED value after flipping the sign bit is
    the SIGNED value shifted up by `2^(w-1)`: `⟦flipMsb a⟧ = toIntW a + 2^(w-1)`. This is the whole
    reason "flip sign bit, then unsigned-compare" computes signed `<`. -/
theorem flipMsb_val : ∀ a : List Bool, a ≠ [] →
    (bitsToNat (flipMsb a) : Int) = toIntW a + ((2 ^ (a.length - 1) : Nat) : Int) := by
  intro a
  induction a with
  | nil => intro h; exact absurd rfl h
  | cons b bs ih =>
      intro _
      cases bs with
      | nil =>
          simp only [flipMsb, toIntW, bitsToNat, List.length_cons, List.length_nil,
            Nat.add_sub_cancel]
          cases b <;> decide
      | cons c cs =>
          have ihb := ih (by simp)
          have hnat : 2 ^ ((b :: c :: cs).length - 1) = 2 * 2 ^ ((c :: cs).length - 1) := by
            simp only [List.length_cons, Nat.add_sub_cancel]
            rw [Nat.pow_succ]; omega
          simp only [flipMsb, toIntW, bitsToNat]
          rw [hnat]
          -- All atoms now linear: omega handles the Nat→Int casts of `+`/`·2` and treats `2^…` as an
          -- opaque atom; combined with ihb (`↑⟦flipMsb (c::cs)⟧ = toIntW (c::cs) + ↑2^…`) it closes.
          omega

/-- Signed less-than exactly as `blast.rs::slt`: flip both sign bits, then `ult`. -/
def slt (a b : List Bool) : Bool := ult (flipMsb a) (flipMsb b)

/-- **Signed comparator correctness (fully mechanized).** For equal-length nonempty operands,
    `slt a b = true ↔ toIntW a < toIntW b`. Proof: `slt = ult ∘ flipMsb`, so `ult_correct` (widths
    preserved by `flipMsb_length`) reduces it to `⟦flipMsb a⟧ < ⟦flipMsb b⟧` (unsigned); `flipMsb_val`
    rewrites each side to `toIntW · + 2^(w-1)` with the SAME offset (equal lengths), which cancels.
    The signed `≤`/`>`/`≥` the blaster emits are `slt` negated/swapped, so they inherit this. -/
theorem slt_correct (a b : List Bool) (hlen : a.length = b.length) (hne : a ≠ []) :
    (slt a b = true) ↔ (toIntW a < toIntW b) := by
  have hbne : b ≠ [] := by
    intro h; subst h; simp only [List.length_nil] at hlen
    cases a with
    | nil => exact hne rfl
    | cons _ _ => simp at hlen
  have hfl : (flipMsb a).length = (flipMsb b).length := by
    rw [flipMsb_length, flipMsb_length, hlen]
  have hcast : ∀ m n : Nat, (m < n) ↔ ((m : Int) < (n : Int)) := fun m n => by omega
  rw [slt, ult_correct (flipMsb a) (flipMsb b) hfl,
      hcast (bitsToNat (flipMsb a)) (bitsToNat (flipMsb b)),
      flipMsb_val a hne, flipMsb_val b hbne, hlen]
  omega

/-! ### Derived comparators (`≤` both signedness, and by operand-swap `>`/`≥`)

`blast.rs::cmp` computes the two remaining primitives as the NEGATION of the strict compare on SWAPPED
operands: `ule a b = !(ult b a)` and `sle a b = !(slt b a)` — the textbook `a ≤ b ≡ ¬(b < a)`. The four
`>`/`≥` predicates the blaster emits are then just `ult`/`ule`/`slt`/`sle` applied to swapped operands
(`blast_pred`: `Ugt a b → cmp b a Ult`, `Uge a b → cmp b a Ule`, `Sgt/Sge` likewise). So proving these
two theorems, on top of `ult_correct`/`slt_correct`, mechanizes ALL EIGHT comparators the native lane
decides — closing the gap the earlier prose ("inherit this") merely asserted. The proof-backed FRAGMENT
GATE (`solver/src/fragment.rs`) admits a comparator as native-authoritative only because of these. -/

/-- Unsigned `≤` exactly as `blast.rs::cmp Ule`: negate the strict compare on swapped operands. -/
def ule (a b : List Bool) : Bool := !(ult b a)

/-- Signed `≤` exactly as `blast.rs::cmp Sle`: negate the signed strict compare on swapped operands. -/
def sle (a b : List Bool) : Bool := !(slt b a)

/-- **Unsigned `≤` correctness.** `ule a b = true ↔ ⟦a⟧ ≤ ⟦b⟧`. Direct corollary of `ult_correct`
    (on `b, a`) and `Nat`-order totality (`¬(⟦b⟧ < ⟦a⟧) ↔ ⟦a⟧ ≤ ⟦b⟧`). -/
theorem ule_correct (a b : List Bool) (h : a.length = b.length) :
    (ule a b = true) ↔ (bitsToNat a ≤ bitsToNat b) := by
  have key := ult_correct b a h.symm
  have hb : (ule a b = true) ↔ ¬ (ult b a = true) := by
    unfold ule; cases ult b a <;> simp
  rw [hb, key]
  omega

/-- **Signed `≤` correctness.** `sle a b = true ↔ toIntW a ≤ toIntW b`. Direct corollary of
    `slt_correct` (on `b, a`) and `Int`-order totality. Needs the same width/nonempty side conditions
    as `slt_correct`; `b ≠ []` follows from `a ≠ []` and equal lengths. -/
theorem sle_correct (a b : List Bool) (hlen : a.length = b.length) (hne : a ≠ []) :
    (sle a b = true) ↔ (toIntW a ≤ toIntW b) := by
  have hbne : b ≠ [] := by
    intro h; subst h; simp only [List.length_nil] at hlen
    cases a with
    | nil => exact hne rfl
    | cons _ _ => simp at hlen
  have key := slt_correct b a hlen.symm hbne
  have hb : (sle a b = true) ↔ ¬ (slt b a = true) := by
    unfold sle; cases slt b a <;> simp
  rw [hb, key]
  omega

/-! ### Equality (`bveq`)

`blast.rs::eq_bits` ANDs the per-bit XNORs, so its literal is true iff every corresponding bit matches
— which for equal-length lists is exactly `a = b`. Correctness (bitwise equality decides VALUE equality)
is the INJECTIVITY of `bitsToNat` on a fixed width: two equal-length bit lists with the same value ARE
the same list (the forward direction is trivial congruence). This is the proof backing that lets the
fragment gate (`solver/src/fragment.rs`) admit `Pred::Eq` as native-authoritative rather than defer it. -/

/-- **`bitsToNat` is injective on equal-length bit lists.** Same width + same value ⇒ same bits. The
    low bit is the value's parity (so it is forced by the value), and halving recurses on the rest. -/
theorem bitsToNat_inj : ∀ (a b : List Bool), a.length = b.length →
    bitsToNat a = bitsToNat b → a = b := by
  intro a
  induction a with
  | nil =>
      intro b hlen _
      cases b with
      | nil => rfl
      | cons _ _ => simp at hlen
  | cons x xs ih =>
      intro b hlen hval
      cases b with
      | nil => simp at hlen
      | cons y ys =>
          have hlen' : xs.length = ys.length := by
            simp only [List.length_cons] at hlen; omega
          simp only [bitsToNat] at hval
          have hx : x.toNat ≤ 1 := by cases x <;> simp
          have hy : y.toNat ≤ 1 := by cases y <;> simp
          have hxy : x.toNat = y.toNat := by omega
          have hrest : bitsToNat xs = bitsToNat ys := by omega
          have hxe : x = y := by cases x <;> cases y <;> simp_all [Bool.toNat]
          rw [hxe, ih ys hlen' hrest]

/-- Abstract model of `blast.rs::eq_bits` for equal-width operands: all bits match ⇔ the lists match. -/
def eqBits (a b : List Bool) : Bool := decide (a = b)

/-- **Equality comparator correctness.** For equal-length operands, `eqBits a b = true ↔ ⟦a⟧ = ⟦b⟧`.
    Forward is congruence on `bitsToNat`; backward is `bitsToNat_inj`. -/
theorem eqBits_correct (a b : List Bool) (h : a.length = b.length) :
    (eqBits a b = true) ↔ (bitsToNat a = bitsToNat b) := by
  unfold eqBits
  rw [decide_eq_true_eq]
  constructor
  · intro hab; rw [hab]
  · intro hval; exact bitsToNat_inj a b h hval

/-! ### Bitwise AND / OR / XOR (`bvand`/`bvor`/`bvxor`)

`blast.rs::bitwise` wires one gate per bit position — `out[i] = gate(a[i], b[i])` for equal-length
operands (a width mismatch returns `None`). That is exactly `List.zipWith gate a b`. Correctness — the
pointwise gates compute the arithmetic `Nat.land`/`lor`/`xor` of the values — goes through the testBit
bridge: bit `i` of `⟦l⟧` IS `l.getD i false` (`bitsToNat_testBit`), and core's `Nat.testBit_and/or/xor`
say the Nat ops are pointwise. These theorems admit `Term::And/Or/Xor` into the native-authoritative
fragment (`solver/src/fragment.rs`) — the highest-value widening, since the u32 literal-arg call-site
coercion lowers to `bvand`. -/

/-- **The testBit bridge.** Bit `i` of the value of an LSB-first bit list is the list's `i`-th entry:
    `(⟦l⟧).testBit i = l.getD i false`. The low bit is the value's parity; halving shifts the list. -/
theorem bitsToNat_testBit : ∀ (l : List Bool) (i : Nat),
    (bitsToNat l).testBit i = l.getD i false := by
  intro l
  induction l with
  | nil =>
      intro i
      simp [bitsToNat, Nat.zero_testBit]
  | cons b bs ih =>
      intro i
      cases i with
      | zero =>
          have hb : b.toNat ≤ 1 := by cases b <;> simp
          rw [Nat.testBit_zero]
          simp only [bitsToNat, List.getD_cons_zero]
          cases b <;> simp <;> omega
      | succ j =>
          rw [Nat.testBit_succ]
          have hdiv : (b.toNat + 2 * bitsToNat bs) / 2 = bitsToNat bs := by
            have hb : b.toNat ≤ 1 := by cases b <;> simp
            omega
          simp only [bitsToNat, hdiv, List.getD_cons_succ]
          exact ih j

/-- Generic pointwise-gate value theorem: if the Bool gate `f` matches the Nat op `g` bit-by-bit
    (`hg`) and maps double-false to false (`hf` — the out-of-range case), then the zipWith of `f`
    computes `g` of the values, for equal-length operands. -/
private theorem bitsToNat_zipWith (f : Bool → Bool → Bool) (g : Nat → Nat → Nat)
    (hf : f false false = false)
    (hg : ∀ m n i, (g m n).testBit i = f (m.testBit i) (n.testBit i)) :
    ∀ (a b : List Bool), a.length = b.length →
      bitsToNat (List.zipWith f a b) = g (bitsToNat a) (bitsToNat b) := by
  intro a b h
  apply Nat.eq_of_testBit_eq
  intro i
  rw [bitsToNat_testBit, hg, bitsToNat_testBit, bitsToNat_testBit]
  -- Pointwise: (zipWith f a b).getD i false = f (a.getD i false) (b.getD i false).
  induction a generalizing b i with
  | nil =>
      cases b with
      | nil => simpa using hf.symm
      | cons _ _ => simp at h
  | cons x xs ih =>
      cases b with
      | nil => simp at h
      | cons y ys =>
          have h' : xs.length = ys.length := by
            simp only [List.length_cons] at h; omega
          cases i with
          | zero => simp
          | succ j => simpa using ih ys h' j

/-- **Bitwise AND correctness.** `⟦zipWith (·&&·) a b⟧ = ⟦a⟧ &&& ⟦b⟧` (equal lengths). -/
theorem andBits_correct (a b : List Bool) (h : a.length = b.length) :
    bitsToNat (List.zipWith (· && ·) a b) = bitsToNat a &&& bitsToNat b :=
  bitsToNat_zipWith (· && ·) (· &&& ·) rfl Nat.testBit_and a b h

/-- **Bitwise OR correctness.** `⟦zipWith (·||·) a b⟧ = ⟦a⟧ ||| ⟦b⟧` (equal lengths). -/
theorem orBits_correct (a b : List Bool) (h : a.length = b.length) :
    bitsToNat (List.zipWith (· || ·) a b) = bitsToNat a ||| bitsToNat b :=
  bitsToNat_zipWith (· || ·) (· ||| ·) rfl Nat.testBit_or a b h

/-- **Bitwise XOR correctness.** `⟦zipWith xor a b⟧ = ⟦a⟧ ^^^ ⟦b⟧` (equal lengths). -/
theorem xorBits_correct (a b : List Bool) (h : a.length = b.length) :
    bitsToNat (List.zipWith (· ^^ ·) a b) = bitsToNat a ^^^ bitsToNat b :=
  bitsToNat_zipWith (· ^^ ·) (· ^^^ ·) rfl Nat.testBit_xor a b h

/-! ### Subtraction and negation (`bvsub`/`bvneg`)

`blast.rs::sub_carry(a,b)` = `add_carry(a, ¬b, 1)` — the textbook two's-complement subtractor, and the
SAME circuit `ult` is already built on. `Term::Neg(a)` is `sub_carry(zeros, a)`. So both correctness
results are corollaries of `rippleCarry_spec` + the complement identity `bitsToNat_not_add`, in the
wrapping (mod `2^w`) form matching `run.rs`'s `wrapping_sub`/`wrapping_neg`. These admit
`Term::Sub`/`Term::Neg` into the native-authoritative fragment (`solver/src/fragment.rs`). -/

/-- Zero bits have value zero. -/
theorem bitsToNat_replicate_false (k : Nat) : bitsToNat (List.replicate k false) = 0 := by
  induction k with
  | zero => simp [bitsToNat]
  | succ k ih => simp [List.replicate_succ, bitsToNat, ih]

/-- Subtractor exactly as `blast.rs::sub_carry`: add the complement with carry-in 1, keep the sum. -/
def subBits (a b : List Bool) : List Bool := (rippleCarry a (b.map (fun x => !x)) true).1

/-- **Subtractor computes wrapping subtraction.** For equal-length operands,
    `⟦subBits a b⟧ = (⟦a⟧ + 2^w − ⟦b⟧) mod 2^w` — Nat-safe wrapping form (`⟦b⟧ < 2^w`, so the borrow is
    absorbed by the added `2^w`). Proof: `rippleCarry_spec` on `(a, ¬b, 1)` gives
    `⟦sum⟧ + 2^w·cout = ⟦a⟧ + ⟦¬b⟧ + 1`; `bitsToNat_not_add` turns `⟦¬b⟧ + 1` into `2^w − ⟦b⟧`;
    dropping the `2^w·cout` term mod `2^w` leaves the (in-range) sum. -/
theorem subBits_correct (a b : List Bool) (h : a.length = b.length) :
    bitsToNat (subBits a b) = (bitsToNat a + 2 ^ a.length - bitsToNat b) % 2 ^ a.length := by
  have hlen : a.length = (b.map (fun x => !x)).length := by
    simpa [List.length_map] using h
  have hspec := rippleCarry_spec a (b.map (fun x => !x)) true hlen
  have hnadd := bitsToNat_not_add b
  have hlb := bitsToNat_lt b
  have hlt := bitsToNat_lt (rippleCarry a (b.map (fun x => !x)) true).1
  rw [rippleCarry_length a (b.map (fun x => !x)) true hlen] at hlt
  simp only [Bool.toNat_true] at hspec
  -- Normalise every power to `2 ^ b.length` so omega sees one opaque atom.
  rw [h] at hlt hspec ⊢
  unfold subBits
  have key : bitsToNat a + 2 ^ b.length - bitsToNat b
      = bitsToNat (rippleCarry a (b.map (fun x => !x)) true).1
        + 2 ^ b.length * (rippleCarry a (b.map (fun x => !x)) true).2.toNat := by
    omega
  rw [key, Nat.add_mul_mod_self_left, Nat.mod_eq_of_lt hlt]

/-! ### Conditional select (`ite` — the mux)

`blast.rs::Term::Ite(p,a,b)` blasts the selector predicate to one literal `sel` and wires
`out[i] = mux(sel, a[i], b[i])` — a per-bit 2:1 mux with the COMMON selector. With one shared selector,
the elementwise select IS the list-level if-then-else, so the value law is `⟦ite s a b⟧ =
if s then ⟦a⟧ else ⟦b⟧` — definitional, proved by cases. The per-bit `mux` gate itself
(`sel → (c ↔ a); ¬sel → (c ↔ b)`, four clauses) is the same trusted Tseitin-core family as
`and2`/`or2` (TIER-0 — shared with every proven blast, e.g. the adder's own gates). This admits
`Term::Ite` into the native-authoritative fragment — un-deferring the `abs`/`min`/`max` contracts the
encoder lowers to `(ite …)`. -/

/-- Conditional select exactly as the blaster's common-selector per-bit mux row. -/
def iteBits (s : Bool) (a b : List Bool) : List Bool := if s then a else b

/-- **Mux/select correctness.** `⟦iteBits s a b⟧ = if s then ⟦a⟧ else ⟦b⟧`. -/
theorem iteBits_correct (s : Bool) (a b : List Bool) :
    bitsToNat (iteBits s a b) = if s then bitsToNat a else bitsToNat b := by
  cases s <;> rfl

/-- Negation exactly as `blast.rs::Term::Neg`: subtract from the all-zero word. -/
def negBits (a : List Bool) : List Bool := subBits (List.replicate a.length false) a

/-- **Negation computes the wrapping two's complement:** `⟦negBits a⟧ = (2^w − ⟦a⟧) mod 2^w`
    (so `⟦−0⟧ = 0` and otherwise `2^w − ⟦a⟧` — exactly `wrapping_neg`). -/
theorem negBits_correct (a : List Bool) :
    bitsToNat (negBits a) = (2 ^ a.length - bitsToNat a) % 2 ^ a.length := by
  have hlen : (List.replicate a.length false).length = a.length := by
    simp
  have hsub := subBits_correct (List.replicate a.length false) a hlen
  rw [negBits, hsub]
  simp [bitsToNat_replicate_false, hlen]

/-! ### Constant left shift

`blast.rs::const_shift(_, Const k, Left)` wires `out[i] = if i ≥ k then a[i−k] else 0`, capped to the
operand width `w` — equivalently, it PREPENDS `k` low zero bits and keeps the low `w` bits. We mechanize
that this computes `(⟦a⟧ · 2^k) mod 2^w` (SMT `bvshl` by a constant literal). The truncation lemma
`bitsToNat_take` (keeping the low `m` bits = value mod `2^m`) is the reusable core the constant multiply
and the barrel shifter also rest on. -/

/-- Prepending `k` false (low) bits multiplies the value by `2^k`. -/
theorem bitsToNat_replicate_false_append (k : Nat) (xs : List Bool) :
    bitsToNat (List.replicate k false ++ xs) = 2 ^ k * bitsToNat xs := by
  induction k with
  | zero => simp
  | succ k ih =>
      simp only [List.replicate_succ, List.cons_append, bitsToNat, Bool.toNat_false,
        Nat.zero_add]
      rw [ih, Nat.pow_succ, Nat.mul_comm (2 ^ k) 2, Nat.mul_assoc]

/-- **Truncation is modulus.** Keeping the low `m` bits of a bit list computes its value mod `2^m`. -/
theorem bitsToNat_take : ∀ (l : List Bool) (m : Nat),
    bitsToNat (l.take m) = bitsToNat l % 2 ^ m := by
  intro l
  induction l with
  | nil => intro m; simp [bitsToNat]
  | cons b bs ih =>
      intro m
      cases m with
      | zero => simp [bitsToNat, Nat.pow_zero, Nat.mod_one]
      | succ m =>
          have hpos : 0 < 2 ^ m := Nat.two_pow_pos m
          have hb : b.toNat < 2 := by cases b <;> decide
          have hrlt : bitsToNat bs % 2 ^ m < 2 ^ m := Nat.mod_lt _ hpos
          simp only [List.take_succ_cons, bitsToNat, Nat.pow_succ]
          rw [ih m, Nat.mul_comm (2 ^ m) 2, Nat.add_mod,
            Nat.mul_mod_mul_left 2 (bitsToNat bs) (2 ^ m),
            Nat.mod_eq_of_lt (show b.toNat < 2 * 2 ^ m by omega),
            Nat.mod_eq_of_lt (show b.toNat + 2 * (bitsToNat bs % 2 ^ m) < 2 * 2 ^ m by omega)]

/-- Constant left shift by `k`, exactly as `const_shift … Left`: prepend `k` low zeros, keep width `w`. -/
def shlConst (a : List Bool) (k : Nat) : List Bool :=
  (List.replicate k false ++ a).take a.length

/-- **Constant left-shift correctness (fully mechanized).** `⟦a << k⟧ = (⟦a⟧ · 2^k) mod 2^w` — the SMT
    `bvshl` semantics by a constant, matching the runtime `wrapping_shl`. Follows immediately from the
    truncation lemma (`bitsToNat_take`) and the zero-prepend lemma (`bitsToNat_replicate_false_append`). -/
theorem shlConst_correct (a : List Bool) (k : Nat) :
    bitsToNat (shlConst a k) = (bitsToNat a * 2 ^ k) % 2 ^ a.length := by
  unfold shlConst
  rw [bitsToNat_take, bitsToNat_replicate_false_append, Nat.mul_comm]

/-! ### Constant multiply

`blast.rs::const_mul` computes `x * c` for a CONSTANT `c` as `Σ_{i : bit i of c set} (x << i)`, accumulated
LSB-first with the ripple adder (so mod `2^w`). We mechanize `⟦mulConst x c⟧ = (⟦x⟧ · c) mod 2^w`, reusing
`shlConst_correct` (the partial products) and `rippleCarry_spec` (the adder). Variable × variable is
deferred to z3, so it needs no proof. -/

/-- The bit-blaster's `add` (LSB-first, carry-in 0) as a value function: `(rippleCarry a b false).1`. -/
def addBits (a b : List Bool) : List Bool := (rippleCarry a b false).1

/-- **Adder computes wrapping addition.** Corollary of `rippleCarry_spec`: the low `w` bits are
    `(⟦a⟧ + ⟦b⟧) mod 2^w` (the discarded carry-out is the overflow). -/
theorem addBits_correct (a b : List Bool) (h : a.length = b.length) :
    bitsToNat (addBits a b) = (bitsToNat a + bitsToNat b) % 2 ^ a.length := by
  have hspec := rippleCarry_spec a b false h
  have hlt := bitsToNat_lt (rippleCarry a b false).1
  rw [rippleCarry_length a b false h] at hlt
  simp only [Bool.toNat_false, Nat.add_zero] at hspec
  unfold addBits
  rw [← hspec, Nat.add_mul_mod_self_left, Nat.mod_eq_of_lt hlt]

/-- **Low-bit peel for `mod 2^(k+1)`.** `c mod 2^(k+1) = c mod 2^k + 2^k · (bit k of c)`, where bit k is
    `(c / 2^k) mod 2` — the `(c >> i) & 1` the multiplier tests. -/
theorem mod_two_pow_succ (c k : Nat) :
    c % 2 ^ (k + 1) = c % 2 ^ k + 2 ^ k * (c / 2 ^ k % 2) := by
  rw [Nat.pow_succ, Nat.mod_mul]

/-- The constant left shift preserves the operand width. -/
theorem shlConst_length (x : List Bool) (k : Nat) : (shlConst x k).length = x.length := by
  unfold shlConst
  rw [List.length_take, List.length_append, List.length_replicate]
  omega

/-- One fold step of the constant multiplier: add the partial product `x << i` iff bit `i` of `c` is set
    (`(c / 2^i) mod 2 = 1`, i.e. the `(c >> i) & 1` the blaster tests). -/
def mulStep (x : List Bool) (c : Nat) (acc : List Bool) (i : Nat) : List Bool :=
  if c / 2 ^ i % 2 = 1 then addBits acc (shlConst x i) else acc

/-- Constant multiply, exactly as `blast.rs::const_mul`: fold the partial products over the bit indices
    `0 .. w-1`, starting from the `w`-bit zero. -/
def mulConst (x : List Bool) (c : Nat) : List Bool :=
  (List.range x.length).foldl (mulStep x c) (List.replicate x.length false)

/-- Accumulation invariant: after folding bits `0 .. n-1`, the accumulator has width `w` and value
    `(⟦x⟧ · (c mod 2^n)) mod 2^w`. -/
theorem mulConst_aux (x : List Bool) (c : Nat) : ∀ n,
    ((List.range n).foldl (mulStep x c) (List.replicate x.length false)).length = x.length
    ∧ bitsToNat ((List.range n).foldl (mulStep x c) (List.replicate x.length false))
        = bitsToNat x * (c % 2 ^ n) % 2 ^ x.length := by
  intro n
  induction n with
  | zero =>
      refine ⟨?_, ?_⟩
      · simp [List.range_zero, List.foldl_nil, List.length_replicate]
      · simp [List.range_zero, List.foldl_nil, bitsToNat_replicate_false, Nat.mod_one,
          Nat.mul_zero, Nat.zero_mod]
  | succ n ih =>
      obtain ⟨hlen, hval⟩ := ih
      rw [List.range_succ, List.foldl_append, List.foldl_cons, List.foldl_nil]
      have hsl : (shlConst x n).length = x.length := shlConst_length x n
      revert hlen hval
      generalize (List.range n).foldl (mulStep x c) (List.replicate x.length false) = acc
      intro hlen hval
      unfold mulStep
      by_cases hc : c / 2 ^ n % 2 = 1
      · rw [if_pos hc]
        refine ⟨?_, ?_⟩
        · rw [addBits, rippleCarry_length acc (shlConst x n) false (by rw [hlen, hsl])]
          exact hlen
        · rw [addBits_correct acc (shlConst x n) (by rw [hlen, hsl]), hlen, hval, shlConst_correct,
            ← Nat.add_mod, ← Nat.mul_add, mod_two_pow_succ, hc, Nat.mul_one]
      · rw [if_neg hc]
        refine ⟨hlen, ?_⟩
        have h0 : c / 2 ^ n % 2 = 0 := by omega
        rw [hval, mod_two_pow_succ, h0, Nat.mul_zero, Nat.add_zero]

/-- **Constant-multiply correctness (fully mechanized).** `⟦mulConst x c⟧ = (⟦x⟧ · c) mod 2^w` — SMT
    `bvmul` by a constant, matching the runtime `wrapping_mul`. From the accumulation invariant at
    `n = w` (`c mod 2^w ≡ c` under the outer mod). -/
theorem mulConst_correct (x : List Bool) (c : Nat) :
    bitsToNat (mulConst x c) = bitsToNat x * c % 2 ^ x.length := by
  unfold mulConst
  rw [(mulConst_aux x c x.length).2, Nat.mul_mod, Nat.mod_mod, ← Nat.mul_mod]

/-! ### Variable × variable (schoolbook) multiply

`blast.rs::var_mul` computes `x * y` as `Σ_i (y[i] ? (x << i) : 0)` mod `2^w` — the same
shift-and-add family as `const_mul`, with the constant's bits replaced by the list bits of `y`.
Mechanized as `mulVar_correct`. -/

/-- `testBit` ↔ low bit of the shifted value. -/
private theorem testBit_eq_div_mod (n i : Nat) :
    (n.testBit i = true) ↔ (n / 2 ^ i % 2 = 1) := by
  unfold Nat.testBit
  rw [Nat.shiftRight_eq_div_pow, Nat.and_comm, Nat.and_one_is_mod]
  constructor
  · intro h
    cases hmod : n / 2 ^ i % 2 with
    | zero => simp [hmod] at h
    | succ m => omega
  · intro h; simp [h]

/-- One fold step of schoolbook mul: add `x << i` iff bit `i` of the *value* of `y` is set.
    Equivalent to `y.getD i false` by `bitsToNat_testBit` (the blaster's bit wire). -/
def mulVarStep (x : List Bool) (yVal : Nat) (acc : List Bool) (i : Nat) : List Bool :=
  if yVal.testBit i then addBits acc (shlConst x i) else acc

/-- Variable multiply, matching `blast.rs::var_mul` (fold over bit indices `0 .. w-1`). -/
def mulVar (x y : List Bool) : List Bool :=
  (List.range x.length).foldl (mulVarStep x (bitsToNat y)) (List.replicate x.length false)

/-- Accumulation invariant for schoolbook mul (parallel to `mulConst_aux`). -/
theorem mulVar_aux (x : List Bool) (yVal : Nat) : ∀ n,
    ((List.range n).foldl (mulVarStep x yVal) (List.replicate x.length false)).length = x.length
    ∧ bitsToNat ((List.range n).foldl (mulVarStep x yVal) (List.replicate x.length false))
        = bitsToNat x * (yVal % 2 ^ n) % 2 ^ x.length := by
  intro n
  induction n with
  | zero =>
      refine ⟨?_, ?_⟩
      · simp [List.range_zero, List.foldl_nil, List.length_replicate]
      · simp [List.range_zero, List.foldl_nil, bitsToNat_replicate_false, Nat.mod_one,
          Nat.mul_zero, Nat.zero_mod]
  | succ n ih =>
      obtain ⟨hlen, hval⟩ := ih
      rw [List.range_succ, List.foldl_append, List.foldl_cons, List.foldl_nil]
      have hsl : (shlConst x n).length = x.length := shlConst_length x n
      revert hlen hval
      generalize (List.range n).foldl (mulVarStep x yVal) (List.replicate x.length false) = acc
      intro hlen hval
      unfold mulVarStep
      by_cases hc : yVal / 2 ^ n % 2 = 1
      · have hbit : yVal.testBit n = true := (testBit_eq_div_mod yVal n).2 hc
        rw [if_pos hbit]
        refine ⟨?_, ?_⟩
        · rw [addBits, rippleCarry_length acc (shlConst x n) false (by rw [hlen, hsl])]
          exact hlen
        · rw [addBits_correct acc (shlConst x n) (by rw [hlen, hsl]), hlen, hval, shlConst_correct,
            ← Nat.add_mod, ← Nat.mul_add, mod_two_pow_succ, hc, Nat.mul_one]
      · have hbit : yVal.testBit n = false := by
          have : yVal.testBit n ≠ true := fun ht => hc ((testBit_eq_div_mod yVal n).1 ht)
          exact Bool.eq_false_iff.2 this
        have h0 : yVal / 2 ^ n % 2 = 0 := by omega
        rw [if_neg (by simp [hbit])]
        refine ⟨hlen, ?_⟩
        rw [hval, mod_two_pow_succ, h0, Nat.mul_zero, Nat.add_zero]

/-- **Variable-multiply correctness (fully mechanized).** `⟦mulVar x y⟧ = (⟦x⟧ · ⟦y⟧) mod 2^w` —
    SMT `bvmul` for free operands, matching `wrapping_mul`. Requires equal widths (the blaster's
    equal-length gate). At `n = w`, `⟦y⟧ mod 2^w = ⟦y⟧` by `bitsToNat_lt`. -/
theorem mulVar_correct (x y : List Bool) (hlen : x.length = y.length) :
    bitsToNat (mulVar x y) = bitsToNat x * bitsToNat y % 2 ^ x.length := by
  unfold mulVar
  have h := mulVar_aux x (bitsToNat y) x.length
  rw [h.2]
  have hlt : bitsToNat y < 2 ^ x.length := by
    rw [hlen]; exact bitsToNat_lt y
  rw [Nat.mod_eq_of_lt hlt]

/-! ### Variable (barrel) shift — left

`blast.rs::var_shift(_, _, Left)` is a log-depth barrel shifter: for each bit `k` of the amount, it
conditionally shifts the running value left by `2^k` positions (`cur := mux(bit k, cur << 2^k, cur)`).
Since the per-bit mux uses the SAME selector `bit k` for every lane, the layer is exactly
`if bit k then (cur << 2^k) else cur`, and `cur << 2^k` is the already-proven `shlConst`. We mechanize
`⟦barrelShl a b⟧ = (⟦a⟧ · 2^⟦b⟧) mod 2^w` (SMT `bvshl`, incl. "amount ≥ width ⇒ 0" since ⟦b⟧ can reach
that and the layered shifts flush every bit). -/

/-- Appending one high bit `x`: `⟦l ++ [x]⟧ = ⟦l⟧ + 2^|l|·x`. -/
theorem bitsToNat_append (l : List Bool) (x : Bool) :
    bitsToNat (l ++ [x]) = bitsToNat l + 2 ^ l.length * x.toNat := by
  induction l with
  | nil => simp [bitsToNat, Nat.pow_zero, Nat.one_mul]
  | cons b bs ih =>
      have hp : (2 : Nat) ^ (bs.length + 1) = 2 * 2 ^ bs.length := by rw [Nat.pow_succ]; omega
      simp only [List.cons_append, bitsToNat, List.length_cons, ih, hp]
      rw [Nat.mul_add, ← Nat.mul_assoc]
      omega

/-- Low-bit peel for `take`: `⟦b.take (m+1)⟧ = ⟦b.take m⟧ + 2^m · (bit m of b)`. Holds unconditionally:
    past the end `getD` is `false` (adds 0) and `take` saturates (`b.take (m+1) = b.take m = b`). -/
theorem bitsToNat_take_succ (b : List Bool) (m : Nat) :
    bitsToNat (b.take (m + 1)) = bitsToNat (b.take m) + 2 ^ m * (b.getD m false).toNat := by
  induction b generalizing m with
  | nil => simp [bitsToNat, List.getD]
  | cons c cs ih =>
      cases m with
      | zero => simp [bitsToNat, List.getD]
      | succ m =>
          have hp : (2 : Nat) ^ (m + 1) = 2 * 2 ^ m := by rw [Nat.pow_succ]; omega
          simp only [List.take_succ_cons, bitsToNat, List.getD_cons_succ, ih m, hp]
          rw [Nat.mul_add, ← Nat.mul_assoc]
          omega

/-- Left barrel shift, as `blast.rs::var_shift(_, _, Left)`: fold over amount bits `k`, conditionally
    shifting the running value left by `2^k` (the constant shift `shlConst`) iff bit `k` of `b` is set. -/
def barrelShl (a b : List Bool) : List Bool :=
  (List.range b.length).foldl
    (fun cur k => if b.getD k false = true then shlConst cur (2 ^ k) else cur) a

/-- `(A % n) * B % n = A * B % n` — the running truncation may be pushed through a multiply. -/
private theorem mul_mod_left_eq (A B n : Nat) : A % n * B % n = A * B % n := by
  rw [Nat.mul_mod (A % n) B n, Nat.mod_mod, ← Nat.mul_mod]

/-- Accumulation invariant: after folding amount bits `0 .. m-1`, the running value is
    `(⟦a⟧ · 2^⟦b.take m⟧) mod 2^w` — `a` left-shifted by the low `m` bits of the amount. -/
theorem barrelShl_aux (a b : List Bool) : ∀ m,
    ((List.range m).foldl
        (fun cur k => if b.getD k false = true then shlConst cur (2 ^ k) else cur) a).length
      = a.length
    ∧ bitsToNat ((List.range m).foldl
        (fun cur k => if b.getD k false = true then shlConst cur (2 ^ k) else cur) a)
      = bitsToNat a * 2 ^ bitsToNat (b.take m) % 2 ^ a.length := by
  intro m
  induction m with
  | zero =>
      refine ⟨by simp, ?_⟩
      have hlt := bitsToNat_lt a
      simp only [List.range_zero, List.foldl_nil, List.take_zero, bitsToNat, Nat.pow_zero,
        Nat.mul_one]
      exact (Nat.mod_eq_of_lt hlt).symm
  | succ m ih =>
      obtain ⟨hlen, hval⟩ := ih
      rw [List.range_succ, List.foldl_append, List.foldl_cons, List.foldl_nil]
      revert hlen hval
      generalize (List.range m).foldl
        (fun cur k => if b.getD k false = true then shlConst cur (2 ^ k) else cur) a = cur
      intro hlen hval
      by_cases hb : b.getD m false = true
      · rw [if_pos hb]
        refine ⟨by rw [shlConst_length]; exact hlen, ?_⟩
        rw [shlConst_correct, hlen, hval, bitsToNat_take_succ, hb, Bool.toNat_true, Nat.mul_one,
          Nat.pow_add, ← Nat.mul_assoc, mul_mod_left_eq]
      · rw [if_neg hb]
        refine ⟨hlen, ?_⟩
        simp only [Bool.not_eq_true] at hb
        rw [hval, bitsToNat_take_succ, hb, Bool.toNat_false, Nat.mul_zero, Nat.add_zero]

/-- **Left barrel-shift correctness (fully mechanized).** `⟦barrelShl a b⟧ = (⟦a⟧ · 2^⟦b⟧) mod 2^w` —
    SMT `bvshl` by a variable amount, matching the runtime `wrapping_shl` (incl. "amount ≥ width ⇒ 0",
    since `⟦b⟧` can reach `w` and the layered shifts flush every bit). From the invariant at `m = |b|`. -/
theorem barrelShl_correct (a b : List Bool) :
    bitsToNat (barrelShl a b) = bitsToNat a * 2 ^ bitsToNat b % 2 ^ a.length := by
  unfold barrelShl
  rw [(barrelShl_aux a b b.length).2, List.take_length]

/-! ### Constant logical right shift

`blast.rs::const_shift(_, Const k, LogicalRight)` wires `out[i] = if i+k < w then a[i+k] else 0` — drop the
low `k` bits and zero-fill the high, keeping width `w`. We mechanize `⟦a >> k⟧ = ⟦a⟧ / 2^k` (SMT `bvlshr`
by a constant / runtime `wrapping_shr`), the value dual of the left shift. -/

/-- Appending HIGH zeros does not change the value. -/
theorem bitsToNat_append_replicate_false (l : List Bool) (k : Nat) :
    bitsToNat (l ++ List.replicate k false) = bitsToNat l := by
  induction l with
  | nil => simp [bitsToNat, bitsToNat_replicate_false]
  | cons b bs ih => simp only [List.cons_append, bitsToNat, ih]

/-- **Dropping low bits is integer division.** `⟦a.drop k⟧ = ⟦a⟧ / 2^k` — the value dual of
    `bitsToNat_take` (which is mod). -/
theorem bitsToNat_drop : ∀ (a : List Bool) (k : Nat), bitsToNat (a.drop k) = bitsToNat a / 2 ^ k := by
  intro a
  induction a with
  | nil => intro k; simp [bitsToNat]
  | cons c cs ih =>
      intro k
      cases k with
      | zero => simp [bitsToNat]
      | succ k =>
          have hc : c.toNat < 2 := by cases c <;> decide
          simp only [List.drop_succ_cons, bitsToNat, Nat.pow_succ, ih k,
            Nat.mul_comm (2 ^ k) 2, ← Nat.div_div_eq_div_mul]
          congr 1
          omega

/-- Constant logical right shift by `k`, as `const_shift … LogicalRight`: drop `k` low bits, high-fill 0. -/
def shrConstL (a : List Bool) (k : Nat) : List Bool :=
  a.drop k ++ List.replicate (min k a.length) false

/-- **Constant logical-right-shift correctness (fully mechanized).** `⟦a >> k⟧ = ⟦a⟧ / 2^k`. -/
theorem shrConstL_correct (a : List Bool) (k : Nat) :
    bitsToNat (shrConstL a k) = bitsToNat a / 2 ^ k := by
  unfold shrConstL
  rw [bitsToNat_append_replicate_false, bitsToNat_drop]

/-! ### Variable (barrel) logical right shift

`blast.rs::var_shift(_, _, LogicalRight)`: the barrel dual of the left shift, conditionally shifting the
running value RIGHT by `2^k` (`shrConstL`) for each set amount bit. We mechanize
`⟦a >> b⟧ = ⟦a⟧ / 2^⟦b⟧` (SMT `bvlshr` by a variable amount). No truncation is needed — a right shift
only removes bits, so the running value stays in range. -/

/-- The constant logical right shift preserves the operand width. -/
theorem shrConstL_length (a : List Bool) (k : Nat) : (shrConstL a k).length = a.length := by
  unfold shrConstL
  rw [List.length_append, List.length_drop, List.length_replicate]
  omega

/-- Barrel logical right shift, as `var_shift(_, _, LogicalRight)`. -/
def barrelLshr (a b : List Bool) : List Bool :=
  (List.range b.length).foldl
    (fun cur k => if b.getD k false = true then shrConstL cur (2 ^ k) else cur) a

/-- Accumulation invariant: after folding amount bits `0 .. m-1`, the running value is
    `⟦a⟧ / 2^⟦b.take m⟧` — `a` right-shifted by the low `m` bits of the amount. -/
theorem barrelLshr_aux (a b : List Bool) : ∀ m,
    ((List.range m).foldl
        (fun cur k => if b.getD k false = true then shrConstL cur (2 ^ k) else cur) a).length
      = a.length
    ∧ bitsToNat ((List.range m).foldl
        (fun cur k => if b.getD k false = true then shrConstL cur (2 ^ k) else cur) a)
      = bitsToNat a / 2 ^ bitsToNat (b.take m) := by
  intro m
  induction m with
  | zero =>
      refine ⟨by simp, ?_⟩
      simp [List.range_zero, List.foldl_nil, List.take_zero, bitsToNat, Nat.pow_zero, Nat.div_one]
  | succ m ih =>
      obtain ⟨hlen, hval⟩ := ih
      rw [List.range_succ, List.foldl_append, List.foldl_cons, List.foldl_nil]
      revert hlen hval
      generalize (List.range m).foldl
        (fun cur k => if b.getD k false = true then shrConstL cur (2 ^ k) else cur) a = cur
      intro hlen hval
      by_cases hb : b.getD m false = true
      · rw [if_pos hb]
        refine ⟨by rw [shrConstL_length]; exact hlen, ?_⟩
        rw [shrConstL_correct, hval, bitsToNat_take_succ, hb, Bool.toNat_true, Nat.mul_one,
          Nat.pow_add, Nat.div_div_eq_div_mul]
      · rw [if_neg hb]
        refine ⟨hlen, ?_⟩
        simp only [Bool.not_eq_true] at hb
        rw [hval, bitsToNat_take_succ, hb, Bool.toNat_false, Nat.mul_zero, Nat.add_zero]

/-- **Barrel logical-right-shift correctness (fully mechanized).** `⟦a >> b⟧ = ⟦a⟧ / 2^⟦b⟧` — SMT
    `bvlshr` by a variable amount / runtime `wrapping_shr`. From the invariant at `m = |b|`. -/
theorem barrelLshr_correct (a b : List Bool) :
    bitsToNat (barrelLshr a b) = bitsToNat a / 2 ^ bitsToNat b := by
  unfold barrelLshr
  rw [(barrelLshr_aux a b b.length).2, List.take_length]

/-! ### Structural ops — concat and extract

`blast.rs::blast_term` wires `Concat`, `Extract`, and the extends as pure list operations (append,
slice) — no arithmetic gates. We mechanize their value semantics so the whole native-authoritative
fragment (bar `ashr` and the CDCL engine) is value-checked. `ZeroExtend` is already covered by
`bitsToNat_append_replicate_false` (high zeros preserve value). -/

/-- **General concatenation value.** `⟦l₁ ++ l₂⟧ = ⟦l₁⟧ + 2^|l₁|·⟦l₂⟧` — appending a HIGH chunk `l₂`
    shifts it up by `|l₁|`. This is `blast.rs::Concat` (result LSB-first = low-part ++ high-part). -/
theorem bitsToNat_append_list (l₁ l₂ : List Bool) :
    bitsToNat (l₁ ++ l₂) = bitsToNat l₁ + 2 ^ l₁.length * bitsToNat l₂ := by
  induction l₁ with
  | nil => simp [bitsToNat]
  | cons b bs ih =>
      simp only [List.cons_append, bitsToNat, List.length_cons, ih, Nat.pow_succ]
      rw [Nat.mul_add, ← Nat.mul_assoc, Nat.mul_comm (2 ^ bs.length) 2, Nat.mul_assoc]
      omega

/-- **Extract value.** `blast.rs::Extract(hi, lo, a)` slices bits `[lo, hi]` (LSB-first
    `a[lo..=hi]` = `(a.drop lo).take (hi+1-lo)`); its value is `(⟦a⟧ / 2^lo) mod 2^(hi+1-lo)` — the SMT
    `(_ extract hi lo)` semantics. From `bitsToNat_take` (mod) ∘ `bitsToNat_drop` (div). -/
theorem bitsToNat_extract (a : List Bool) (lo len : Nat) :
    bitsToNat ((a.drop lo).take len) = bitsToNat a / 2 ^ lo % 2 ^ len := by
  rw [bitsToNat_take, bitsToNat_drop]
