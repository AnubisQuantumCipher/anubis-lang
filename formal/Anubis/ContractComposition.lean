/-
  Anubis — mechanized CONTRACT-COMPOSITION / substitution soundness (Phase 5, the call-site lane)

  When the checker discharges a callee's `requires` at a call site and assumes its `ensures`, it
  SUBSTITUTES the call arguments for the callee's parameters into the contract expression
  (`substitute_vars` in `compiler/src/middle/mod.rs`) and then evaluates the result in the caller's
  scope. The SOUNDNESS-CRITICAL comment on `substitute_vars` warns: if a MODELABLE subterm is cloned
  WITHOUT substituting, the callee-parameter name survives and re-binds to the caller's scope — a
  precondition bypass + a certified-false postcondition (the real bug `g(150)` vs `requires(abs(x)<100)`
  checked as `abs(5)<100`). This file machine-checks the substitution lemma that makes the discharge
  sound — and, as a tightness witness, that a substitution which DROPS a variable is genuinely unsound.

  Lean 4 core (no Mathlib). The expression language is the arithmetic core `substitute_vars` recurses
  into (Var/Lit/add/mul stand for the Var/Literal/Binary forms; the lemma's shape is identical for the
  rest). THEOREMS:
    * `subst_sound`      — evaluating a substituted contract in the caller env equals evaluating the
      original contract on the ARG VALUES. This is exactly the discharge's meaning.
    * `discharge_sound`  — the call-site discharge: the caller-scope obligation `subst σ Q` holds iff the
      callee's guarantee `Q` holds of the arg values, so discharging reduces the obligation to the
      callee contract FAITHFULLY (no strengthening, no bypass).
    * `dropping_a_var_is_unsound` — TIGHTNESS: a substitution that clones a `Var` unsubstituted can
      evaluate DIFFERENTLY, so `substitute_vars` recursing into every modelable form is load-bearing.
-/

namespace Anubis.ContractComposition

/-- The modelable contract-expression core (Var/Literal/Binary in the real IR). -/
inductive Exp where
  | var : String → Exp
  | lit : Int → Exp
  | add : Exp → Exp → Exp
  | mul : Exp → Exp → Exp
  deriving DecidableEq

/-- A variable environment (a scope: param/local name → value). -/
abbrev Env := String → Int

/-- Evaluate a contract expression in an environment. -/
def eval (env : Env) : Exp → Int
  | .var x => env x
  | .lit n => n
  | .add a b => eval env a + eval env b
  | .mul a b => eval env a * eval env b

/-- Substitution: replace each variable by an expression (the call arguments as caller-scope
    expressions). Recurses into EVERY form — this totality is what the lemma below rests on. -/
def subst (σ : String → Exp) : Exp → Exp
  | .var x => σ x
  | .lit n => .lit n
  | .add a b => .add (subst σ a) (subst σ b)
  | .mul a b => .mul (subst σ a) (subst σ b)

/-- **The substitution lemma (discharge soundness core).** Evaluating a substituted contract in the
    caller environment equals evaluating the ORIGINAL contract in the environment that maps each callee
    parameter to the value of its substituted argument. So `substitute_vars(contract, args)` evaluated
    in the caller's scope means exactly the callee's contract on the argument VALUES — no param name
    leaks back to the caller. Holds precisely because `subst` recurses into every constructor. -/
theorem subst_sound (env : Env) (σ : String → Exp) :
    ∀ e, eval env (subst σ e) = eval (fun x => eval env (σ x)) e := by
  intro e
  induction e with
  | var x => rfl
  | lit n => rfl
  | add a b iha ihb => simp [eval, subst, iha, ihb]
  | mul a b iha ihb => simp [eval, subst, iha, ihb]

/-- **Call-site discharge is faithful.** Read the callee's contract `Q` as a predicate on its result
    value (here: the contract expression is nonzero = "holds"). The caller-scope obligation
    `subst σ Q` holds iff `Q` holds of the argument values `argVal x = eval env (σ x)`. So proving the
    substituted obligation in caller scope is EXACTLY proving the callee's contract on the actual
    arguments — the discharge neither strengthens nor bypasses the contract. -/
theorem discharge_sound (env : Env) (σ : String → Exp) (Q : Exp) :
    (eval env (subst σ Q) ≠ 0) ↔ (eval (fun x => eval env (σ x)) Q ≠ 0) := by
  rw [subst_sound env σ Q]

/-- **Tightness — dropping a substitution is unsound.** A "buggy" substitution that clones a `Var`
    unsubstituted (leaving the callee param name to re-bind in the caller env) can evaluate DIFFERENTLY
    from the faithful substitution: here the callee contract `var "x"` under `σ x = lit 150` should
    evaluate to 150, but a subst that drops the var evaluates it as `env "x" = 5`. This is the exact
    `g(150)`-checked-as-`abs(5)` precondition-bypass class — so `substitute_vars` recursing into every
    modelable form is load-bearing, not incidental. -/
theorem dropping_a_var_is_unsound :
    ∃ (env : Env) (σ : String → Exp) (e : Exp),
      eval env e ≠ eval env (subst σ e) ∧ eval env (subst σ e) = eval (fun x => eval env (σ x)) e := by
  refine ⟨(fun _ => 5), (fun _ => .lit 150), .var "x", ?_, ?_⟩
  · decide           -- eval env (var x) = 5  ≠  eval env (subst σ (var x)) = 150
  · rfl              -- the FAITHFUL subst still agrees with the arg-value evaluation (subst_sound)

end Anubis.ContractComposition
