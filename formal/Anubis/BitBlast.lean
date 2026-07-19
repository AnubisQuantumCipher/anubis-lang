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
