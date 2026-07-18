/-
  Anubis — mechanized Safe-mode NON-INTERFERENCE (Phase 5, goal (a))

  Safe mode's central promise is a taint / information-flow guarantee: a value the checker
  marks `Lo` (public, sinkable) can NEVER be influenced by a value marked `Hi` (secret). The
  compiler enforces this by JOINING labels at every operation — an op's result is `Hi` if EITHER
  operand is `Hi` — and by forbidding declassification. This file mechanizes, in Lean 4 core
  (no Mathlib), that this join-propagation rule actually *delivers* non-interference: over a
  small-but-faithful taint calculus, a `Lo` result is a function of the `Lo` inputs alone.

  Faithfulness to the runtime. Values are `Anubis.Encoding.Word = BitVec 64` — the exact i64
  model `Encoding.lean` proves the SMT encoder matches — and the binary operations are the SAME
  runtime/encoder operations (`+ = bvadd`, `&&& = bvand`; see `enc_add`/`enc_and`). So the
  theorem is about label flow over the actual runtime value domain, not a toy `Int`.

  Declassification. Safe mode's ONE sanctioned escape hatch is `declassify`, which downgrades a
  value's label to `Lo` on purpose (an authorized release). This file models it faithfully — a
  `declassify` node forces the result label to `Lo` while leaving the VALUE unchanged — and proves
  BOTH halves of the real guarantee: (i) over the DECLASSIFY-FREE fragment non-interference is
  absolute (no secret can reach a public sink), and (ii) `declassify` genuinely CAN move a secret
  into a public value — so declassification is exactly the only breach, precisely what the checker
  permits under authorization and forbids otherwise.

  What is proved here:
    * `Label.join_eq_lo`   — a join is `Lo` only when BOTH operands are `Lo`; this is the crux of
                             taint propagation (a `Lo` result had NO `Hi` operand in its subtree);
    * `noninterference`    — MAIN THEOREM. Over a DECLASSIFY-FREE expression, if two stores are
                             low-equivalent (agree on every `Lo` variable) then a `Lo` result has
                             the SAME value in both: a secret never flows to a public sink;
    * `secret_write_invisible` — the operational reading: overwriting a `Hi` variable with ANY
                             value leaves every public (declassify-free) result unchanged;
    * `hi_can_leak_into_hi` — TIGHTNESS / non-vacuity: drop the `Lo`-result premise and the
                             conclusion genuinely FAILS (a secret DOES influence a *secret* sink);
    * `declassify_downgrades` — declassification IS a real (authorized) downgrade: a `declassify`d
                             secret is a `Lo` result whose value DIFFERS across low-equivalent
                             stores. Together with `noninterference` this pins declassify as the
                             SOLE way a secret becomes public — the exact Safe-mode boundary.

  Reused from `Anubis.Encoding` (not re-proved): `Word`, and the encoder=runtime identities
  `enc_add`, `enc_and` that pin the operation semantics.
-/
import Anubis.Encoding

namespace Anubis.NonInterference

open Anubis.Encoding

/-! ### Security lattice: `Lo ⊑ Hi`, with `Lo` the public bottom -/

/-- A two-point security label. `Lo` = public (bottom), `Hi` = secret (top). -/
inductive Label where
  | Lo
  | Hi
deriving DecidableEq

/-- Label join (least upper bound). Taint propagation: an operation's result carries `Hi` iff
    EITHER operand is `Hi`; it stays `Lo` only when both operands are `Lo`. `Lo` is the identity. -/
def Label.join : Label → Label → Label
  | Lo, y => y
  | Hi, _ => Hi

/-- `Lo` is the identity of the join (a public value combined with a public value stays public). -/
@[simp] theorem Label.lo_join (b : Label) : Label.join Lo b = b := rfl

/-- `Hi` absorbs (any operation touching a secret yields a secret). -/
@[simp] theorem Label.hi_join (b : Label) : Label.join Hi b = Hi := rfl

/-- **Crux of taint propagation.** A join equals `Lo` only when BOTH operands are `Lo`. Hence a
    `Lo`-labelled value can have had NO `Hi` operand anywhere in the operation tree that built it —
    this is exactly why a `Lo` result cannot depend on a secret. -/
theorem Label.join_eq_lo {a b : Label} (h : a.join b = Lo) : a = Lo ∧ b = Lo := by
  cases a <;> cases b <;> simp_all [Label.join]

/-! ### Expressions, stores, and the tainting evaluator -/

/-- Program variables. -/
abbrev Var := Nat

/-- A minimal expression language over `Var`: literals (always public), variable reads (label
    taken from the store — a taint source), and three binary ops that exercise arithmetic,
    comparison, and bitwise flavors. Every binary op propagates taint by joining operand labels. -/
inductive Expr where
  | lit  : Word → Expr
  | var  : Var → Expr
  | add  : Expr → Expr → Expr      -- runtime `+`  (= encoder `bvadd`)
  | eqb  : Expr → Expr → Expr      -- runtime `==` (0/1-valued)
  | band : Expr → Expr → Expr      -- runtime `&`  (= encoder `bvand`)
  | declassify : Expr → Expr       -- Safe mode's sanctioned downgrade: force the label to `Lo`

/-- A store maps each variable to a runtime value AND its security label; the label is intrinsic
    to the variable (the taint source / security policy), exactly as Safe mode assigns it. -/
abbrev Store := Var → Word × Label

/-- Big-step evaluator. A variable read returns the store's tagged value; a literal is public;
    every binary op computes its value with the SAME runtime/encoder operation and JOINS the
    operand labels (the Anubis taint rule). No declassification exists — this is Safe mode. -/
def eval (s : Store) : Expr → Word × Label
  | .lit n    => (n, Label.Lo)
  | .var x    => s x
  | .add a b  => ((eval s a).1 + (eval s b).1,
                  (eval s a).2.join (eval s b).2)
  | .eqb a b  => ((if (eval s a).1 = (eval s b).1 then 1 else 0),
                  (eval s a).2.join (eval s b).2)
  | .band a b => ((eval s a).1 &&& (eval s b).1,
                  (eval s a).2.join (eval s b).2)
  | .declassify e => ((eval s e).1, Label.Lo)   -- value preserved; label forced public

/-- An expression uses NO declassification anywhere — the fragment Safe mode enforces when no
    authorized release is present. Non-interference is absolute exactly on this fragment. -/
def DeclassifyFree : Expr → Prop
  | .lit _        => True
  | .var _        => True
  | .add a b      => DeclassifyFree a ∧ DeclassifyFree b
  | .eqb a b      => DeclassifyFree a ∧ DeclassifyFree b
  | .band a b     => DeclassifyFree a ∧ DeclassifyFree b
  | .declassify _ => False

/-- The `add` result value is exactly the encoder's `bvadd` on the operand values — the operation
    non-interference reasons about IS the one `Encoding.enc_add` matches to the runtime. -/
theorem add_value_is_encoder (s : Store) (a b : Expr) :
    (eval s (.add a b)).1 = BitVec.add (eval s a).1 (eval s b).1 := by
  simp only [eval]; exact enc_add _ _

/-- Likewise `band` is the encoder's `bvand`. -/
theorem band_value_is_encoder (s : Store) (a b : Expr) :
    (eval s (.band a b)).1 = BitVec.and (eval s a).1 (eval s b).1 := by
  simp only [eval]; exact enc_and _ _

/-! ### Low-equivalence and the non-interference theorem -/

/-- Two stores are low-equivalent when they agree on the VALUE of every variable the policy marks
    `Lo`. They may differ arbitrarily on `Hi` (secret) variables — those are precisely the inputs
    a public result must be shown independent of. -/
def LowEquiv (s1 s2 : Store) : Prop :=
  ∀ x : Var, (s1 x).2 = Label.Lo → (s1 x).1 = (s2 x).1

/-- **NON-INTERFERENCE (Safe mode's formal heart).** If `s1` and `s2` are low-equivalent, then for
    every expression whose evaluated result is `Lo` (public), the value is identical under both
    stores. Since the two stores may differ freely on secrets, this says a public value is
    determined by public inputs ALONE — a secret can never influence a public sink.

    Proof: structural induction on `Expr`. Literals are constant; a `Lo` variable is pinned by
    low-equivalence; and for every binary op a `Lo` result forces (via `join_eq_lo`) BOTH operand
    subtrees to be `Lo`, so both recurse to equal values and the op — identical on both sides —
    yields equal results. -/
theorem noninterference {s1 s2 : Store} (h : LowEquiv s1 s2) :
    ∀ e : Expr, DeclassifyFree e → (eval s1 e).2 = Label.Lo → (eval s1 e).1 = (eval s2 e).1 := by
  intro e
  induction e with
  | lit n => intro _ _; rfl
  | var x => intro _ hlo; exact h x hlo
  | add a b iha ihb =>
      intro hdf hlo
      simp only [eval] at hlo ⊢
      obtain ⟨ha, hb⟩ := Label.join_eq_lo hlo
      rw [iha hdf.1 ha, ihb hdf.2 hb]
  | eqb a b iha ihb =>
      intro hdf hlo
      simp only [eval] at hlo ⊢
      obtain ⟨ha, hb⟩ := Label.join_eq_lo hlo
      rw [iha hdf.1 ha, ihb hdf.2 hb]
  | band a b iha ihb =>
      intro hdf hlo
      simp only [eval] at hlo ⊢
      obtain ⟨ha, hb⟩ := Label.join_eq_lo hlo
      rw [iha hdf.1 ha, ihb hdf.2 hb]
  | declassify e _ =>
      -- Vacuous: a `declassify` expression is not declassify-free.
      intro hdf _
      exact absurd hdf (by simp [DeclassifyFree])

/-! ### Operational corollary: secret writes are invisible to public outputs -/

/-- Update one variable's value; its label is intrinsic to the policy, so it is left unchanged. -/
def Store.set (s : Store) (x : Var) (v : Word) : Store :=
  fun y => if y = x then (v, (s x).2) else s y

/-- **Secret writes are invisible.** Overwriting a `Hi` (secret) variable with ANY value leaves
    every public (`Lo`) result unchanged — the operational face of non-interference: you cannot
    leak a secret into a public output by choosing the secret's value. Follows from the main
    theorem, because updating only a secret keeps the two stores low-equivalent. -/
theorem secret_write_invisible (s : Store) (x : Var) (v : Word)
    (hx : (s x).2 = Label.Hi) (e : Expr) (hdf : DeclassifyFree e)
    (hpub : (eval s e).2 = Label.Lo) :
    (eval s e).1 = (eval (s.set x v) e).1 := by
  refine noninterference (s1 := s) (s2 := s.set x v) ?_ e hdf hpub
  intro y hy
  by_cases hyx : y = x
  · subst hyx; rw [hx] at hy; exact absurd hy (by decide)
  · simp only [Store.set, if_neg hyx]

/-! ### Tightness: the `Lo`-result premise is essential (non-vacuity) -/

/-- Two stores agreeing on every public variable but disagreeing on the value of secret var `0`. -/
def sSecret0 : Store := fun x => if x = 0 then (0, Label.Hi) else (0, Label.Lo)
def sSecret1 : Store := fun x => if x = 0 then (1, Label.Hi) else (0, Label.Lo)

/-- **Tightness / non-vacuity.** There exist low-equivalent stores and an expression whose result
    is `Hi` and whose VALUE differs between them. So the main theorem's `Lo`-result hypothesis is
    load-bearing: non-interference constrains public results only, and correctly permits a secret
    to influence a *secret* sink. This rules out a vacuous reading of `noninterference`. -/
theorem hi_can_leak_into_hi :
    ∃ (s1 s2 : Store) (e : Expr),
      LowEquiv s1 s2 ∧ (eval s1 e).2 = Label.Hi ∧ (eval s1 e).1 ≠ (eval s2 e).1 := by
  refine ⟨sSecret0, sSecret1, Expr.var 0, ?_, ?_, ?_⟩
  · intro x hx
    by_cases hx0 : x = 0
    · subst hx0; simp only [sSecret0] at hx; exact absurd hx (by decide)
    · simp only [sSecret0, sSecret1, if_neg hx0]
  · decide
  · decide

/-- **Declassification is a real, authorized downgrade.** `declassify (var 0)` over the two
    low-equivalent stores (which differ only on the SECRET `var 0`) produces a `Lo` result whose
    VALUE differs (`0` vs `1`). So a `declassify`d secret genuinely reaches a public output — the
    single breach Safe mode permits under authorization. Together with `noninterference` (which
    holds on exactly the `DeclassifyFree` fragment), this pins declassification as the SOLE way a
    secret becomes public: no declassify ⟹ no leak; a declassify ⟹ a controlled release. -/
theorem declassify_downgrades :
    ∃ (s1 s2 : Store) (e : Expr),
      LowEquiv s1 s2 ∧ ¬ DeclassifyFree e ∧ (eval s1 e).2 = Label.Lo
        ∧ (eval s1 e).1 ≠ (eval s2 e).1 := by
  refine ⟨sSecret0, sSecret1, Expr.declassify (Expr.var 0), ?_, ?_, ?_, ?_⟩
  · intro x hx
    by_cases hx0 : x = 0
    · subst hx0; simp only [sSecret0] at hx; exact absurd hx (by decide)
    · simp only [sSecret0, sSecret1, if_neg hx0]
  · simp [DeclassifyFree]
  · decide
  · decide

end Anubis.NonInterference
