/-
  Anubis — mechanized LINEAR CAPABILITY soundness (Phase 5, the capability pillar)

  Anubis's capability discipline (Phase 2, `9824807`) issues use-once tokens: a capability may be
  consumed AT MOST ONCE along any execution. The checker enforces this LINEARLY (a capability binding
  moves on use; a second use is a compile error). The guarantee is only trustworthy if "the checker
  accepts a program" really implies "no capability is ever spent twice at runtime".

  This file mechanizes exactly that, in Lean 4 core (no Mathlib). A program is a sequence of
  capability USES; the static discipline is `Linear` (the sequence has no repeats — each capability
  used at most once); the operational semantics `run` threads the set of already-consumed
  capabilities and FAILS (`none`) the instant a capability is reused (a double-spend / use-after-move).

  THEOREMS (the soundness chain):
    * `linear_never_double_spends`      — MAIN. A `Linear` program NEVER double-spends: `run` from the
      empty consumed-set succeeds. So a program the checker's linearity gate accepts cannot spend a
      capability twice at runtime — the exact use-once guarantee.
    * `run_isSome_iff`                  — CHARACTERIZATION. `run` succeeds IFF the program is
      internally repeat-free and disjoint from what's already consumed. Operational success is
      exactly the absence of a double-spend, so linearity is necessary as well as sufficient.
    * `run_none_of_not_linear`          — TIGHTNESS. EVERY non-linear program (some capability listed
      twice) genuinely aborts (`run` returns `none`), so `Linear` is not a vacuous requirement.
    * `nonlinear_double_spends`         — a concrete non-vacuity witness (`[0,0]` double-spends).
-/

namespace Anubis.Capability

/-- A capability token identity. -/
abbrev Cap := Nat

/-- A program is the sequence of capability USES it performs, in order. -/
abbrev Prog := List Cap

/-- The static linearity discipline the checker enforces: every capability is used AT MOST ONCE
    (the sequence of uses has no repeats). This is exactly "use-once / move-on-use". -/
def Linear (p : Prog) : Prop := p.Nodup

/-- Operational semantics: thread the set (here a list) of already-CONSUMED capabilities; a use of a
    capability already consumed is a double-spend and aborts with `none`. Success returns the final
    consumed set. -/
def run : Prog → List Cap → Option (List Cap)
  | [],        consumed => some consumed
  | c :: rest, consumed => if c ∈ consumed then none else run rest (c :: consumed)

/-! ### The core lemma: a no-repeat program disjoint from what's consumed always succeeds -/

/-- If `p` has no internal repeats AND none of its capabilities are already consumed, `run` succeeds.
    The two hypotheses are exactly the invariant that rules out every double-spend: no reuse WITHIN
    `p`, and no reuse of something spent BEFORE `p`. -/
theorem run_isSome_of_nodup_disjoint :
    ∀ (p : Prog) (consumed : List Cap),
      p.Nodup → (∀ c, c ∈ p → c ∉ consumed) → (run p consumed).isSome := by
  intro p
  induction p with
  | nil => intro consumed _ _; simp [run]
  | cons c rest ih =>
      intro consumed hnodup hdisj
      have hc_not : c ∉ consumed := hdisj c (by simp)
      have hnodup_rest : rest.Nodup := (List.nodup_cons.mp hnodup).2
      have hc_not_rest : c ∉ rest := (List.nodup_cons.mp hnodup).1
      -- After consuming `c`, the rest is still repeat-free and disjoint from `c :: consumed`.
      have hdisj' : ∀ x, x ∈ rest → x ∉ (c :: consumed) := by
        intro x hx hmem
        rcases List.mem_cons.mp hmem with rfl | hxc
        · exact hc_not_rest hx
        · exact hdisj x (by simp [hx]) hxc
      simp only [run, if_neg hc_not]
      exact ih (c :: consumed) hnodup_rest hdisj'

/-- **Linear programs never double-spend.** A `Linear` program run from nothing-consumed always
    succeeds — the checker's linearity gate accepting a program means no capability is spent twice at
    runtime. This is the use-once guarantee, mechanized. -/
theorem linear_never_double_spends (p : Prog) (h : Linear p) : (run p []).isSome := by
  exact run_isSome_of_nodup_disjoint p [] h (by intro c _ hmem; exact absurd hmem (by simp))

/-! ### The exact converse: success ⇒ no double-spend (so the guarantee is iff, not one-way) -/

/-- If `run` SUCCEEDS then the program had no internal repeat AND touched nothing already consumed.
    This is the converse of `run_isSome_of_nodup_disjoint`: operational success is EXACTLY the
    absence of any double-spend, so the checker's linearity condition is not merely sufficient — it
    is also necessary. -/
theorem nodup_disjoint_of_run_isSome :
    ∀ (p : Prog) (consumed : List Cap),
      (run p consumed).isSome → p.Nodup ∧ (∀ c, c ∈ p → c ∉ consumed) := by
  intro p
  induction p with
  | nil =>
      intro consumed _
      exact ⟨List.nodup_nil, by intro c hc; exact absurd hc (by simp)⟩
  | cons c rest ih =>
      intro consumed hsome
      by_cases hc : c ∈ consumed
      · simp [run, hc] at hsome
      · have hrun : run (c :: rest) consumed = run rest (c :: consumed) := by
          simp [run, hc]
        rw [hrun] at hsome
        obtain ⟨hnd, hdis⟩ := ih (c :: consumed) hsome
        have hc_not_rest : c ∉ rest := fun hmem => hdis c hmem (by simp)
        refine ⟨List.nodup_cons.mpr ⟨hc_not_rest, hnd⟩, ?_⟩
        intro x hx hxcons
        rcases List.mem_cons.mp hx with rfl | hxr
        · exact hc hxcons
        · exact hdis x hxr (List.mem_cons_of_mem c hxcons)

/-- **Success is EXACTLY the absence of a double-spend.** `run` succeeds from a given consumed-set
    iff the program is internally repeat-free and disjoint from what's already spent. -/
theorem run_isSome_iff (p : Prog) (consumed : List Cap) :
    (run p consumed).isSome ↔ (p.Nodup ∧ ∀ c, c ∈ p → c ∉ consumed) :=
  ⟨nodup_disjoint_of_run_isSome p consumed,
   fun h => run_isSome_of_nodup_disjoint p consumed h.1 h.2⟩

/-! ### Tightness: non-linear programs genuinely abort -/

/-- **Every non-linear program double-spends.** If a program is NOT linear (some capability listed
    twice), `run` from nothing-consumed aborts with `none`. Combined with `linear_never_double_spends`,
    this makes linearity the precise runtime-success criterion — not an over-approximation. -/
theorem run_none_of_not_linear (p : Prog) (h : ¬ Linear p) : run p [] = none := by
  cases hrun : run p [] with
  | none => rfl
  | some s =>
      have hsome : (run p []).isSome := by rw [hrun]; rfl
      exact absurd (nodup_disjoint_of_run_isSome p [] hsome).1 h

/-- **Concrete tightness witness.** Using capability `0` twice is not linear and genuinely aborts —
    so `Linear` is a real, non-vacuous requirement. -/
theorem nonlinear_double_spends :
    ∃ p : Prog, ¬ Linear p ∧ run p [] = none :=
  ⟨[0, 0], by simp [Linear], run_none_of_not_linear [0, 0] (by simp [Linear])⟩

end Anubis.Capability
