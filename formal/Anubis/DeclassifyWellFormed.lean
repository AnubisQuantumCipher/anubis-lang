/-
  Anubis — mechanized DECLASSIFY WELL-FORMEDNESS non-interference (Phase 5, goal (a), refinement)

  `NonInterference.lean` proves Safe-mode non-interference over the DECLASSIFY-FREE fragment and
  models EVERY `declassify` as an authorized downgrade (label forced `Lo`). But the real checker
  (operator security fix, 2026-07-20) does NOT trust every declassify: `declassify(value, policy,
  reason)` RELEASES a label only when BOTH `policy` and `reason` are present and non-empty (after
  trimming). A malformed `declassify(secret, "", "")` is a NO-OP that KEEPS the label — otherwise an
  empty-policy declassify would be a silent bypass with no auditable justification. This file
  mechanizes that refinement: the label downgrade is GATED on well-formedness, and non-interference
  survives every malformed declassify.

  Faithfulness. Values are `Anubis.Encoding.Word = BitVec 64` (the exact i64 model `Encoding.lean`
  pins to the runtime), the binary ops are the same runtime/encoder operations, and `wf` mirrors the
  compiler's `declassify_wellformed(policy, reason)` — modelled as: BOTH the (post-trim) policy and
  reason are non-empty. A declassify node carries its policy and reason as `List Char`, and the
  evaluator downgrades to `Lo` ONLY when `wf policy reason = true`; otherwise it returns the operand
  unchanged, label intact.

  What is proved here:
    * `eval_declassify_wf` / `eval_declassify_malformed` — the gated evaluator: a well-formed
      declassify forces `Lo`; a malformed one is the identity (value AND label preserved);
    * `malformed_declassify_is_noop` / `malformed_declassify_preserves_label` — the operator fix as a
      theorem: `declassify(x, "", "")` changes nothing — in particular a secret stays secret;
    * `noninterference` — MAIN THEOREM. Over the `NoAuthRelease` fragment (every declassify malformed,
      OR none) low-equivalent stores give equal `Lo` results: a secret never reaches a public sink
      even in the PRESENCE of (malformed) declassify nodes — so an empty-policy declassify is not a
      hole in the guarantee;
    * `malformed_declassify_no_leak` — a malformed declassify of a secret is still `Hi` (does not
      launder), the direct contrast to the authorized case;
    * `wellformed_declassify_downgrades` — TIGHTNESS: a WELL-FORMED declassify genuinely downgrades a
      secret to a differing public value, so the `wf` gate is exactly what separates a silent bypass
      (rejected) from an authorized release (permitted). Non-vacuity of the `wf` premise.

  Reused from `Anubis.Encoding` (not re-proved): `Word` and its runtime arithmetic.
-/
import Anubis.Encoding

namespace Anubis.DeclassifyWellFormed

open Anubis.Encoding

/-! ### Security lattice (same two-point lattice as `NonInterference`) -/

/-- A two-point security label. `Lo` = public (bottom), `Hi` = secret (top). -/
inductive Label where
  | Lo
  | Hi
deriving DecidableEq

/-- Label join (least upper bound): `Hi` iff either operand is `Hi`. -/
def Label.join : Label → Label → Label
  | Lo, y => y
  | Hi, _ => Hi

@[simp] theorem Label.lo_join (b : Label) : Label.join Lo b = b := rfl
@[simp] theorem Label.hi_join (b : Label) : Label.join Hi b = Hi := rfl

/-- A join equals `Lo` only when BOTH operands are `Lo`. -/
theorem Label.join_eq_lo {a b : Label} (h : a.join b = Lo) : a = Lo ∧ b = Lo := by
  cases a <;> cases b <;> simp_all [Label.join]

/-! ### Well-formedness of a declassify policy -/

/-- Program variables. -/
abbrev Var := Nat

/-- Well-formedness of a declassify's `(policy, reason)`, mirroring the compiler's
    `declassify_wellformed`: BOTH must be non-empty. The `List Char` here is the EFFECTIVE
    (post-trim) string, so "non-empty" is exactly the compiler's `!p.trim().is_empty()`. -/
def wf (policy reason : List Char) : Bool := !policy.isEmpty && !reason.isEmpty

/-! ### Expressions with a POLICY-CARRYING declassify -/

/-- Expressions over `Var`. Unlike `NonInterference`, the `declassify` node carries its policy and
    reason, and its downgrade is gated on `wf`. -/
inductive Expr where
  | lit  : Word → Expr
  | var  : Var → Expr
  | add  : Expr → Expr → Expr
  | eqb  : Expr → Expr → Expr
  | band : Expr → Expr → Expr
  | declassify : List Char → List Char → Expr → Expr

/-- A store maps each variable to a runtime value AND its intrinsic security label. -/
abbrev Store := Var → Word × Label

/-- Big-step evaluator. Binary ops join operand labels (the taint rule). A `declassify` downgrades
    to `Lo` ONLY when its policy/reason are well-formed; a malformed declassify returns its operand
    UNCHANGED — value and label both preserved (the operator security fix). -/
def eval (s : Store) : Expr → Word × Label
  | .lit n    => (n, Label.Lo)
  | .var x    => s x
  | .add a b  => ((eval s a).1 + (eval s b).1,
                  (eval s a).2.join (eval s b).2)
  | .eqb a b  => ((if (eval s a).1 = (eval s b).1 then 1 else 0),
                  (eval s a).2.join (eval s b).2)
  | .band a b => ((eval s a).1 &&& (eval s b).1,
                  (eval s a).2.join (eval s b).2)
  | .declassify pol rea e =>
      cond (wf pol rea) ((eval s e).1, Label.Lo) (eval s e)

/-- A WELL-FORMED declassify forces the label to `Lo`, value preserved — the authorized release. -/
theorem eval_declassify_wf (s : Store) (pol rea : List Char) (e : Expr) (h : wf pol rea = true) :
    eval s (.declassify pol rea e) = ((eval s e).1, Label.Lo) := by
  show cond (wf pol rea) ((eval s e).1, Label.Lo) (eval s e) = ((eval s e).1, Label.Lo)
  rw [h]; rfl

/-- A MALFORMED declassify is the IDENTITY: the operand is returned unchanged (value AND label). -/
theorem eval_declassify_malformed (s : Store) (pol rea : List Char) (e : Expr)
    (h : wf pol rea = false) :
    eval s (.declassify pol rea e) = eval s e := by
  show cond (wf pol rea) ((eval s e).1, Label.Lo) (eval s e) = eval s e
  rw [h]; rfl

/-- **The operator security fix as a theorem: a malformed declassify changes nothing.** In
    particular the label is preserved, so `declassify(secret, "", "")` keeps the value secret. -/
theorem malformed_declassify_is_noop (s : Store) (pol rea : List Char) (e : Expr)
    (h : wf pol rea = false) :
    eval s (.declassify pol rea e) = eval s e :=
  eval_declassify_malformed s pol rea e h

/-- A malformed declassify preserves the label — no silent downgrade. -/
theorem malformed_declassify_preserves_label (s : Store) (pol rea : List Char) (e : Expr)
    (h : wf pol rea = false) :
    (eval s (.declassify pol rea e)).2 = (eval s e).2 := by
  rw [eval_declassify_malformed s pol rea e h]

/-! ### The fragment with no AUTHORIZED release, and the non-interference theorem -/

/-- Every declassify in the expression is MALFORMED (`wf = false`) — or there are none. This is the
    fragment Safe mode is left in when no WELL-FORMED release is present; strictly weaker than
    `NonInterference.DeclassifyFree` (it permits malformed declassify nodes). -/
def NoAuthRelease : Expr → Prop
  | .lit _        => True
  | .var _        => True
  | .add a b      => NoAuthRelease a ∧ NoAuthRelease b
  | .eqb a b      => NoAuthRelease a ∧ NoAuthRelease b
  | .band a b     => NoAuthRelease a ∧ NoAuthRelease b
  | .declassify pol rea e => wf pol rea = false ∧ NoAuthRelease e

/-- Two stores are low-equivalent when they agree on every `Lo` variable's value. -/
def LowEquiv (s1 s2 : Store) : Prop :=
  ∀ x : Var, (s1 x).2 = Label.Lo → (s1 x).1 = (s2 x).1

/-- **NON-INTERFERENCE THROUGH MALFORMED DECLASSIFY.** If `s1` and `s2` are low-equivalent, then for
    every expression in the `NoAuthRelease` fragment (every declassify malformed) whose result is
    `Lo`, the value is identical under both stores. So a secret never reaches a public sink even when
    the program is peppered with empty-policy declassify calls — the operator's fix means those are
    not release valves. Proof: structural induction; the declassify case rewrites via
    `eval_declassify_malformed` to its operand and recurses. -/
theorem noninterference {s1 s2 : Store} (h : LowEquiv s1 s2) :
    ∀ e : Expr, NoAuthRelease e → (eval s1 e).2 = Label.Lo → (eval s1 e).1 = (eval s2 e).1 := by
  intro e
  induction e with
  | lit n => intro _ _; rfl
  | var x => intro _ hlo; exact h x hlo
  | add a b iha ihb =>
      intro hnf hlo
      simp only [eval] at hlo ⊢
      obtain ⟨ha, hb⟩ := Label.join_eq_lo hlo
      rw [iha hnf.1 ha, ihb hnf.2 hb]
  | eqb a b iha ihb =>
      intro hnf hlo
      simp only [eval] at hlo ⊢
      obtain ⟨ha, hb⟩ := Label.join_eq_lo hlo
      rw [iha hnf.1 ha, ihb hnf.2 hb]
  | band a b iha ihb =>
      intro hnf hlo
      simp only [eval] at hlo ⊢
      obtain ⟨ha, hb⟩ := Label.join_eq_lo hlo
      rw [iha hnf.1 ha, ihb hnf.2 hb]
  | declassify pol rea e ihe =>
      intro hnf hlo
      obtain ⟨hmal, hne⟩ := hnf
      rw [eval_declassify_malformed s1 pol rea e hmal] at hlo ⊢
      rw [eval_declassify_malformed s2 pol rea e hmal]
      exact ihe hne hlo

/-! ### Tightness: the `wf` gate is exactly the bypass/release boundary -/

/-- Two stores agreeing on every public variable but disagreeing on the value of secret var `0`. -/
def sSecret0 : Store := fun x => if x = 0 then (0, Label.Hi) else (0, Label.Lo)
def sSecret1 : Store := fun x => if x = 0 then (1, Label.Hi) else (0, Label.Lo)

theorem lowEquiv_sSecret : LowEquiv sSecret0 sSecret1 := by
  intro x hx
  by_cases hx0 : x = 0
  · subst hx0; simp only [sSecret0] at hx; exact absurd hx (by decide)
  · simp only [sSecret0, sSecret1, if_neg hx0]

/-- **A malformed declassify does NOT launder a secret.** `declassify(secret, "", "")` keeps the
    result `Hi`, so it is caught by the ordinary sink check exactly as the raw secret would be. -/
theorem malformed_declassify_no_leak :
    (eval sSecret0 (.declassify [] [] (.var 0))).2 = Label.Hi := by
  decide

/-- **TIGHTNESS / non-vacuity of the `wf` gate.** A WELL-FORMED declassify (non-empty policy AND
    reason) genuinely downgrades the secret `var 0` to a `Lo` result whose VALUE differs across the
    two low-equivalent stores — the authorized release. Contrast `malformed_declassify_no_leak`: the
    ONLY difference is `wf`. So `wf` is precisely the line between a silent empty-policy bypass (which
    keeps the label, blocked) and an accountable release (permitted). -/
theorem wellformed_declassify_downgrades :
    ∃ (s1 s2 : Store) (pol rea : List Char) (e : Expr),
      LowEquiv s1 s2 ∧ wf pol rea = true
        ∧ (eval s1 (.declassify pol rea e)).2 = Label.Lo
        ∧ (eval s1 (.declassify pol rea e)).1 ≠ (eval s2 (.declassify pol rea e)).1 := by
  refine ⟨sSecret0, sSecret1, ['p'], ['r'], Expr.var 0, lowEquiv_sSecret, by decide, ?_, ?_⟩
  · decide
  · decide

end Anubis.DeclassifyWellFormed
