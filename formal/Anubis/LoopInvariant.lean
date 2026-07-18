/-
  Anubis — mechanized LOOP-INVARIANT (Hoare while-rule) soundness (Phase 5, the loop lane)

  Anubis verifies a `while c { body }` (and a desugared `for`) with B3 loop invariants: the checker
  proves each invariant (a) holds on ENTRY and (b) is PRESERVED by one iteration (given the guard),
  then ASSUMES it after the loop. The post-loop assumption is only sound if the standard partial-
  correctness while rule holds: a preserved invariant holds at loop EXIT, and the guard is FALSE there.
  This file machine-checks exactly that, so the checker's "assume the invariant (and ¬guard) after the
  loop" step is justified — a post-loop obligation discharged from the invariant is sound.

  Lean 4 core (no Mathlib). We model the loop with a big-step evaluation relation (so non-termination
  is simply the absence of a derivation — partial correctness, which is what the checker claims: it
  never asserts termination).

  THEOREMS:
    * `while_invariant`   — the core rule: if `I` holds initially and is preserved by the body under the
      guard, then any terminating run ends in a state where `I` holds AND the guard is false.
    * `while_establishes` — the way the checker USES it: if additionally `I ∧ ¬guard ⇒ Q`, the loop
      establishes the post-loop obligation `Q` (this is the discharge the checker performs).
    * `for_range_bounds`  — a concrete instance for the desugared `for i in start..n` with the auto
      `i ≥ start` invariant (tasks #27/#28): every terminating run exits with `start ≤ i` and `i ≥ n`
      (`¬ i < n`), i.e. the loop counter is exactly at the bound — the fact the checker assumes after a
      `for`.
-/

namespace Anubis.LoopInvariant

variable {σ : Type}

/-- Big-step evaluation of `while guard { body }`: `WhileBig guard body s s'` means starting in `s`
    the loop terminates in `s'`. `done` when the guard is false; `step` runs the body once and
    recurses. A non-terminating loop simply has no derivation — this is partial correctness, exactly
    the checker's claim (it verifies invariants, never termination). -/
inductive WhileBig (guard : σ → Bool) (body : σ → σ) : σ → σ → Prop
  | done {s : σ} : guard s = false → WhileBig guard body s s
  | step {s s' : σ} : guard s = true → WhileBig guard body (body s) s' → WhileBig guard body s s'

/-- **The while rule (partial correctness).** If the invariant `I` holds at entry and is preserved by
    one iteration whenever the guard holds, then at loop EXIT the invariant still holds AND the guard
    is false. This is precisely what licenses the checker to assume `I ∧ ¬guard` after the loop. -/
theorem while_invariant
    {guard : σ → Bool} {body : σ → σ} (I : σ → Prop)
    (hpres : ∀ s, I s → guard s = true → I (body s)) :
    ∀ {s s' : σ}, WhileBig guard body s s' → I s → I s' ∧ guard s' = false := by
  intro s s' h
  induction h with
  | done hnb => intro hI; exact ⟨hI, hnb⟩
  | step hb _ ih => intro hI; exact ih (hpres _ hI hb)

/-- **How the checker discharges a post-loop obligation.** If the invariant is preserved AND
    `I s ∧ ¬guard s` implies the desired post-loop predicate `Q`, then every terminating run of the
    loop establishes `Q`. This is the exact shape of a `for`/`while` post-condition discharge:
    the checker proves the invariant, then derives the obligation from `invariant ∧ loop-exited`. -/
theorem while_establishes
    {guard : σ → Bool} {body : σ → σ} (I Q : σ → Prop)
    (hpres : ∀ s, I s → guard s = true → I (body s))
    (hpost : ∀ s, I s → guard s = false → Q s) :
    ∀ {s s' : σ}, WhileBig guard body s s' → I s → Q s' := by
  intro s s' h hI
  have ⟨hI', hnb'⟩ := while_invariant I hpres h hI
  exact hpost s' hI' hnb'

/-! ### Concrete instance: the desugared `for i in start..n` with the auto `i ≥ start` invariant -/

/-- A `for i in start..n` desugars to `while i < n { …; i = i + 1 }` (tasks #27/#28). Model the state as
    the counter `i : Nat`, guard `i < n`, body `i ↦ i + 1`. The checker auto-adds the invariant
    `start ≤ i` (task #28). This instance shows every terminating run exits with `start ≤ i` AND
    `¬ i < n` (`i ≥ n`) — exactly the counter fact the checker assumes after a `for`. -/
theorem for_range_bounds (start n : Nat) :
    ∀ {i i' : Nat},
      WhileBig (fun i => decide (i < n)) (fun i => i + 1) i i' →
      start ≤ i → start ≤ i' ∧ ¬ i' < n := by
  intro i i' h hstart
  have hpres : ∀ j, start ≤ j → (decide (j < n)) = true → start ≤ (j + 1) := by
    intro j hj _; exact Nat.le_succ_of_le hj
  have ⟨hle, hguard⟩ := while_invariant (fun j => start ≤ j) hpres h hstart
  refine ⟨hle, ?_⟩
  -- guard is `decide (i' < n) = false`, i.e. `¬ i' < n`
  simpa using hguard

end Anubis.LoopInvariant
