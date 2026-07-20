/-
  Anubis — mechanized PATH-CONDITION soundness (Phase 5, the path-sensitivity lane)

  Item 2 (path-sensitivity, 2026-07-20): the `ensures`-discharge threads BRANCH GUARDS — the path
  conditions of the enclosing `if`s, plus the negation of every preceding guard-clause `if cond
  { …return… }` — into an early/tail return obligation, so a postcondition that holds only on that
  control-flow path can be proven (`if x < 0 { return 0; } return x;` proves `result >= 0` because the
  tail is reached only when `x >= 0`). The Rust side realises this by pushing each guard as an extra
  premise onto the SMT obligation (via `push_branch_path_condition`), gated by modelability so a
  reassigned/de-modeled symbol's guard silently drops.

  This file machine-checks the soundness of that move: adding a path condition as a premise is sound.
  A checker obligation "`Γ` entails `P`" is exactly the SMT discharge `Γ ⊨ P` (UNSAT of `Γ ∧ ¬P`), so:

  THEOREMS
    * `path_premise_sound`         — if `P` is proven under `Γ` extended with the path condition `g`,
      and at an actual reachable state every premise in `Γ` holds AND `g` holds (control genuinely took
      this branch), then `P` holds there. Discharging a return obligation under its branch guard is sound.
    * `entails_weaken`             — adding a premise never LOSES a proof (monotonicity): threading a
      guard can only make more obligations provable, never fewer.
    * `path_premise_no_false_proof`— TIGHTNESS: if there is a reachable state where `Γ` and `g` all hold
      but `P` is FALSE, the checker does NOT prove `P` under `g :: Γ` — the counterexample survives. So
      path-sensitivity turns a REJECT into a PASS ONLY when the contract truly holds on the path; it can
      never certify a contract that is false on a reachable path.
    * `guard_clause_escape_sound`  — the concrete guard-clause instance: after `if cond { …return… }`
      the fall-through obligation may assume `¬cond`, and that is sound exactly because the fall-through
      state is one where `cond` is false.

  Lean 4 core (no Mathlib). Fully constructive — `#print axioms` is empty for every theorem here.
-/

namespace Anubis.PathCondition

variable {σ : Type}

/-- An assertion over program states. -/
abbrev Assertion (σ : Type) := σ → Prop

/-- The checker "entails `P` under premises `Γ`" — `P` holds in every state where all of `Γ` hold.
    This is precisely what the SMT discharge establishes: `Γ ⊨ P` (unsatisfiability of `Γ ∧ ¬P`). -/
def Entails (Γ : List (Assertion σ)) (P : Assertion σ) : Prop :=
  ∀ s, (∀ q ∈ Γ, q s) → P s

/-- **Soundness of threading a path condition.** If the checker proves `P` under the premises `Γ`
    extended with the path condition `g`, and at an actual reachable state `s` every premise in `Γ`
    holds and the path condition `g` holds (i.e. control genuinely took this branch), then `P` holds at
    `s`. So discharging a return obligation under its branch guard is sound. -/
theorem path_premise_sound
    (Γ : List (Assertion σ)) (g P : Assertion σ)
    (h : Entails (g :: Γ) P) (s : σ) (hg : g s) (hΓ : ∀ q ∈ Γ, q s) : P s := by
  apply h s
  intro q hq
  cases hq with
  | head => exact hg
  | tail _ hmem => exact hΓ _ hmem

/-- **Monotonicity — a TRUE premise only makes MORE provable.** If `P` is entailed by `Γ`, it is still
    entailed after adding any premise `g`, so threading guards never LOSES a proof. -/
theorem entails_weaken
    (Γ : List (Assertion σ)) (g P : Assertion σ) (h : Entails Γ P) : Entails (g :: Γ) P := by
  intro s hs
  apply h s
  intro q hq
  exact hs q (List.mem_cons_of_mem g hq)

/-- **Tightness — a path premise NEVER masks a false contract.** If there is a reachable state `s` at
    which the premises `Γ` and the path condition `g` all hold but `P` is FALSE, then the checker does
    NOT prove `P` under `g :: Γ` — the obligation is genuinely unprovable (the counterexample `s`
    survives). So path-sensitivity turns a REJECT into a PASS only when the contract truly holds on the
    path; it can never certify a contract that is false on a reachable path. -/
theorem path_premise_no_false_proof
    (Γ : List (Assertion σ)) (g P : Assertion σ)
    (s : σ) (hg : g s) (hΓ : ∀ q ∈ Γ, q s) (hbad : ¬ P s) : ¬ Entails (g :: Γ) P := by
  intro hent
  exact hbad (path_premise_sound Γ g P hent s hg hΓ)

/-- **Guard-clause escape, concretely.** A guard clause `if cond { …return… }` diverts every path on
    which `cond` holds, so the fall-through code runs only in states where `cond` is false. Modelling
    `cond` as a decidable predicate `c : σ → Bool` and the escape premise as `¬ (c s)`, an obligation
    `P` discharged under that escape premise is sound at a fall-through state — because the fall-through
    state is, by definition, one where the guard is false. This is the exact fact the Rust
    `guard_clause_escapes` adds (`(cond, negate = true)`), specialised to a single guard clause. -/
theorem guard_clause_escape_sound
    (c : σ → Bool) (Γ : List (Assertion σ)) (P : Assertion σ)
    (h : Entails ((fun s => c s = false) :: Γ) P)
    (s : σ) (hfall : c s = false) (hΓ : ∀ q ∈ Γ, q s) : P s :=
  path_premise_sound Γ (fun s => c s = false) P h s hfall hΓ

end Anubis.PathCondition
