/-!
# Program-wide mode aggregation

This file models the Safe < Research < Exploit classification used at Anubis command boundaries.
It proves the lattice property itself: no later or nested privileged item can be hidden by an
earlier Safe item. The theorem is deliberately scoped; the Rust traversal is covered separately by
CLI regression tests.

Lean 4 core only. No axioms, `sorry`, `admit`, or `native_decide`.
-/

namespace Anubis.ModeAggregation

inductive Mode where
  | safe
  | research
  | exploit
  deriving DecidableEq, Repr

/-- The privilege order Safe ≤ Research ≤ Exploit. -/
def Mode.atMost : Mode → Mode → Prop
  | .safe, _ => True
  | .research, .research => True
  | .research, .exploit => True
  | .exploit, .exploit => True
  | _, _ => False

/-- Least upper bound in the privilege order. -/
def Mode.join : Mode → Mode → Mode
  | .exploit, _ => .exploit
  | _, .exploit => .exploit
  | .research, _ => .research
  | _, .research => .research
  | .safe, .safe => .safe

def aggregate : List Mode → Mode
  | [] => .safe
  | mode :: modes => mode.join (aggregate modes)

theorem atMost_trans {a b c : Mode} (hab : a.atMost b) (hbc : b.atMost c) :
    a.atMost c := by
  cases a <;> cases b <;> cases c <;> simp_all [Mode.atMost]

theorem left_atMost_join (a b : Mode) : a.atMost (a.join b) := by
  cases a <;> cases b <;> simp [Mode.atMost, Mode.join]

theorem right_atMost_join (a b : Mode) : b.atMost (a.join b) := by
  cases a <;> cases b <;> simp [Mode.atMost, Mode.join]

theorem member_atMost_aggregate {mode : Mode} {modes : List Mode}
    (member : mode ∈ modes) : mode.atMost (aggregate modes) := by
  induction modes with
  | nil => simp at member
  | cons head tail ih =>
      simp only [aggregate]
      simp only [List.mem_cons] at member
      cases member with
      | inl equal =>
          subst mode
          exact left_atMost_join head (aggregate tail)
      | inr inTail =>
          exact atMost_trans (ih inTail) (right_atMost_join head (aggregate tail))

@[simp] theorem safe_prefix_does_not_lower (modes : List Mode) :
    aggregate (.safe :: modes) = aggregate modes := by
  cases result : aggregate modes <;> simp [aggregate, Mode.join, result]

theorem aggregate_safe_iff_every_member_safe (modes : List Mode) :
    aggregate modes = .safe ↔ ∀ mode, mode ∈ modes → mode = .safe := by
  constructor
  · intro aggregateSafe mode member
    have bounded := member_atMost_aggregate member
    rw [aggregateSafe] at bounded
    cases mode <;> simp_all [Mode.atMost]
  · intro everySafe
    induction modes with
    | nil => rfl
    | cons head tail ih =>
        have headSafe : head = .safe := everySafe head (by simp)
        have tailSafe : ∀ mode, mode ∈ tail → mode = .safe := by
          intro mode member
          exact everySafe mode (by simp [member])
        rw [aggregate, headSafe, ih tailSafe]
        rfl

theorem research_cannot_hide_behind_safe {modes : List Mode}
    (member : .research ∈ modes) : aggregate (.safe :: modes) ≠ .safe := by
  intro hidden
  have bounded := member_atMost_aggregate member
  have hiddenTail : aggregate modes = .safe := by
    simpa using hidden
  rw [hiddenTail] at bounded
  exact bounded

theorem exploit_cannot_hide_behind_safe {modes : List Mode}
    (member : .exploit ∈ modes) : aggregate (.safe :: modes) = .exploit := by
  have bounded := member_atMost_aggregate member
  cases result : aggregate modes <;> simp_all [Mode.atMost, Mode.join, aggregate]

end Anubis.ModeAggregation
