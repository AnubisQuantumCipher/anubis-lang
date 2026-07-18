/-
  Anubis — mechanized EFFECT soundness (Phase 5, goal (b))

  Anubis is a Safe-mode language: a function must *declare* the side effects it may perform
  (`uses(FsWrite, Net, …)`), and the compiler REFUSES to build a program whose statically
  *inferred* effect set is not a subset of what it declared. The whole `inferred ⊆ declared`
  gate (the transitive effect-inference discipline of Phase 2) is only trustworthy if the
  inference is a genuine OVER-approximation: it must never miss an effect that some concrete
  run actually performs. If inference under-approximated, a program could pass the gate yet
  perform an undeclared effect at runtime — exactly the "green check certifies what run
  violates" failure the whole project forbids.

  This file mechanizes that guarantee over a minimal effect calculus, in Lean 4 core (no
  Mathlib):

    * a finite `Effect` type (`FsWrite | Net | Shell`);
    * an `Expr` whose leaves perform a primitive effect, with pure values, sequential
      composition `seq` (both sub-effects happen), and a runtime branch `cond c a b` (exactly
      ONE of `a`/`b` runs, chosen by a world/oracle `w : Nat → Bool` — the input the checker
      cannot see);
    * `infer : Expr → List Effect`, the transitive effect set (path-INsensitive: it unions
      both arms of every `cond`, exactly as Anubis's static inference does), represented as a
      duplicate-free list (a genuine finite set — `mem_union` characterizes membership and
      `infer_nodup` proves no duplicates);
    * `PerformsIn w : Expr → Effect → Prop`, the operational effect semantics of an actual run
      under world `w`, and `Performs e f := ∃ w, PerformsIn w e f` ("some evaluation triggers `f`").

  THEOREMS (the load-bearing soundness chain):
    (i)   `infer_sound_run` :  `PerformsIn w e f → f ∈ infer e`
            — static inference OVER-approximates every concrete run; nothing runs that
              inference missed. (Proved by induction; the `cond` cases DROP the world, which is
              precisely why inference is a safe over-approximation.)
    (i')  `infer_sound`     :  `Performs e f → f ∈ infer e`  (world-free corollary).
    (ii)  `gate_sound`      :  `Subseteq (infer e) declared → Performs e f → f ∈ declared`
            — THE checker gate is sound: a program that passes `inferred ⊆ declared` performs
              only declared effects, in every world.
          `gate_sound_run` : the same for a single concrete run `PerformsIn w e f`.
    (bonus, decidability) `subseteqb` + `subseteqb_iff` make the ⊆ gate executable, and
          `gate_sound_decide` runs soundness off the Bool gate the compiler would actually call.
    (bonus, honesty) `infer_strict` exhibits a concrete run where `f ∈ infer e` yet
          `¬ PerformsIn w e f` — inference is a STRICT (sound-but-incomplete) over-approximation,
          the honest characterization of a conservative effect gate; `infer_pureVal`/`infer_prim`
          witness that it is nonetheless PRECISE, not the degenerate "everything" set.
-/

namespace Anubis.EffectSoundness

/-! ### The effect lattice and the expression language -/

/-- The finite universe of side effects Anubis tracks (a minimal stand-in for the real
    `FsWrite | Net | Shell | …` capability set). `DecidableEq` makes membership and the ⊆ gate
    computable. -/
inductive Effect where
  | fsWrite
  | net
  | shell
  deriving DecidableEq, Repr

/-- A minimal effectful expression:
    * `pureVal` — a pure value, performs nothing;
    * `prim e`  — a primitive operation that performs exactly effect `e`;
    * `seq a b` — run `a` then `b`; BOTH sub-effect sets happen (effects union);
    * `cond c a b` — run exactly ONE arm, selected at runtime by condition variable `c`
      (the branch the checker cannot predict). Static inference unions both arms. -/
inductive Expr where
  | pureVal
  | prim (e : Effect)
  | seq (a b : Expr)
  | cond (c : Nat) (a b : Expr)
  | call (body : Expr)            -- invoke a callee whose body is `body` (transitive effects)
  deriving Repr

/-! ### The inferred effect SET

We represent the transitive effect set as a duplicate-free `List Effect`. `insertE`/`union`
are a self-contained dedup union (using only `Effect`'s `DecidableEq`), and `mem_union`
characterizes membership — the only property the ⊆ gate consumes. -/

/-- Insert `x` into `l` only if absent — keeps the list duplicate-free. -/
def insertE (x : Effect) (l : List Effect) : List Effect :=
  if x ∈ l then l else x :: l

/-- Duplicate-free union of two effect lists. -/
def union (a b : List Effect) : List Effect :=
  a.foldr insertE b

/-- Membership in `insertE` is membership in `x :: l` (dedup does not change the SET). -/
theorem mem_insertE (x y : Effect) (l : List Effect) :
    y ∈ insertE x l ↔ y = x ∨ y ∈ l := by
  unfold insertE
  by_cases hx : x ∈ l
  · simp only [if_pos hx]
    constructor
    · intro hy; exact Or.inr hy
    · intro hy; rcases hy with rfl | hy
      · exact hx
      · exact hy
  · simp only [if_neg hx, List.mem_cons]

/-- **`union` is set union.** `y ∈ union a b ↔ y ∈ a ∨ y ∈ b`. This is the whole reason the
    dedup representation is faithful: order and duplicates are invisible to membership. -/
theorem mem_union (a b : List Effect) (y : Effect) :
    y ∈ union a b ↔ y ∈ a ∨ y ∈ b := by
  induction a with
  | nil => simp [union]
  | cons h t ih =>
    have hstep : union (h :: t) b = insertE h (union t b) := rfl
    rw [hstep, mem_insertE, ih, List.mem_cons]
    constructor
    · rintro (rfl | ht | hb)
      · exact Or.inl (Or.inl rfl)
      · exact Or.inl (Or.inr ht)
      · exact Or.inr hb
    · rintro ((rfl | ht) | hb)
      · exact Or.inl rfl
      · exact Or.inr (Or.inl ht)
      · exact Or.inr (Or.inr hb)

/-- Static, path-INsensitive effect inference: the transitive set of effects an expression may
    perform on ANY run. Mirrors Anubis's transitive effect inference — a `cond` contributes the
    union of BOTH arms, because the checker cannot know which arm runs. -/
def infer : Expr → List Effect
  | .pureVal    => []
  | .prim e     => [e]
  | .seq a b    => union (infer a) (infer b)
  | .cond _ a b => union (infer a) (infer b)
  | .call body  => infer body      -- TRANSITIVE: the caller inherits all of the callee's effects

/-! ### The operational effect semantics

`PerformsIn w e f` holds when an actual run of `e` under world `w` (which resolves each `cond`)
triggers effect `f`. This is the DYNAMIC truth the static `infer` must over-approximate. -/

/-- Effects a concrete run of `e` under world `w` actually performs. A `seq` performs either
    side's effects; a `cond` performs only the effects of the arm `w` selects. -/
inductive PerformsIn (w : Nat → Bool) : Expr → Effect → Prop where
  | prim (e : Effect) : PerformsIn w (.prim e) e
  | seqL {a b : Expr} {f : Effect} : PerformsIn w a f → PerformsIn w (.seq a b) f
  | seqR {a b : Expr} {f : Effect} : PerformsIn w b f → PerformsIn w (.seq a b) f
  | condT {c : Nat} {a b : Expr} {f : Effect} :
      w c = true → PerformsIn w a f → PerformsIn w (.cond c a b) f
  | condF {c : Nat} {a b : Expr} {f : Effect} :
      w c = false → PerformsIn w b f → PerformsIn w (.cond c a b) f
  | call {body : Expr} {f : Effect} :
      PerformsIn w body f → PerformsIn w (.call body) f

/-- "Some evaluation of `e` triggers `f`" — the world-free effect judgment (the requested
    `performs : Expr → Effect → Prop`), obtained by existentially closing over the runtime world. -/
def Performs (e : Expr) (f : Effect) : Prop := ∃ w, PerformsIn w e f

/-! ### (i) Soundness of inference: infer over-approximates every concrete run -/

/-- **Effect-inference soundness (over-approximation).** Every effect a concrete run performs is
    in the statically inferred set: `PerformsIn w e f → f ∈ infer e`. Nothing runs that inference
    missed. Note the `cond` cases discard the world `w` — inference keeps BOTH arms, which is
    exactly what makes it a safe over-approximation of whichever arm actually runs. -/
theorem infer_sound_run {w : Nat → Bool} {e : Expr} {f : Effect} :
    PerformsIn w e f → f ∈ infer e := by
  intro h
  induction h with
  | prim e => simp [infer]
  | seqL _ ih => exact (mem_union _ _ _).mpr (Or.inl ih)
  | seqR _ ih => exact (mem_union _ _ _).mpr (Or.inr ih)
  | condT _ _ ih => exact (mem_union _ _ _).mpr (Or.inl ih)
  | condF _ _ ih => exact (mem_union _ _ _).mpr (Or.inr ih)
  | call _ ih => exact ih   -- infer (call body) = infer body, so the callee's effect carries up

/-- World-free corollary: `Performs e f → f ∈ infer e`. -/
theorem infer_sound {e : Expr} {f : Effect} : Performs e f → f ∈ infer e := by
  rintro ⟨w, h⟩; exact infer_sound_run h

/-! ### The `inferred ⊆ declared` gate, and its soundness -/

/-- `l ⊆ d` on effect lists: every inferred effect is declared. -/
def Subseteq (l d : List Effect) : Prop := ∀ f, f ∈ l → f ∈ d

/-- **(ii) The checker gate is sound.** If a program passes `inferred ⊆ declared`, then every
    effect any evaluation performs is declared. This is the mechanized justification of Anubis's
    Safe-mode effect discipline: a green build performs only declared effects. -/
theorem gate_sound {e : Expr} {declared : List Effect} {f : Effect} :
    Subseteq (infer e) declared → Performs e f → f ∈ declared := by
  intro hsub hperf
  exact hsub f (infer_sound hperf)

/-- Per-concrete-run form: the gate is sound for any single world `w`. -/
theorem gate_sound_run {w : Nat → Bool} {e : Expr} {declared : List Effect} {f : Effect} :
    Subseteq (infer e) declared → PerformsIn w e f → f ∈ declared := by
  intro hsub hperf
  exact hsub f (infer_sound_run hperf)

/-! ### (bonus) Decidability of the ⊆ gate — the check the compiler actually runs -/

/-- Executable ⊆ test: the Boolean the compiler evaluates. -/
def subseteqb (l d : List Effect) : Bool := l.all (fun x => decide (x ∈ d))

/-- The Boolean gate agrees with the propositional ⊆ relation. -/
theorem subseteqb_iff (l d : List Effect) : subseteqb l d = true ↔ Subseteq l d := by
  unfold subseteqb Subseteq
  rw [List.all_eq_true]
  constructor
  · intro h f hf; exact of_decide_eq_true (h f hf)
  · intro h x hx; exact decide_eq_true (h x hx)

/-- **Soundness off the executable gate.** If the Bool check the compiler runs returns `true`,
    every performed effect is declared — the fully-computational statement of the gate's soundness. -/
theorem gate_sound_decide {e : Expr} {declared : List Effect} {f : Effect} :
    subseteqb (infer e) declared = true → Performs e f → f ∈ declared := by
  intro hb hperf
  exact gate_sound ((subseteqb_iff _ _).mp hb) hperf

/-! ### (bonus) Honesty: inference is PRECISE, and STRICTLY over-approximating -/

/-- Pure code infers no effects — so a pure function passes an empty `uses()`. (Precision: infer
    is not the degenerate "all effects" set.) -/
theorem infer_pureVal : infer .pureVal = [] := rfl

/-- A single primitive infers exactly its own effect — nothing spurious. -/
theorem infer_prim (e : Effect) : infer (.prim e) = [e] := rfl

/-- **Inference is a STRICT over-approximation.** Concretely, for `cond 0 (prim net) (prim shell)`
    run in a world that takes the first arm (`w 0 = true`), inference reports `shell` as possible
    yet the run never performs it. This is the honest content of "sound but incomplete": the gate
    is conservative on purpose, and this witnesses that it genuinely loses precision at `cond`. -/
theorem infer_strict :
    Effect.shell ∈ infer (.cond 0 (.prim .net) (.prim .shell)) ∧
    ¬ PerformsIn (fun _ => true) (.cond 0 (.prim .net) (.prim .shell)) Effect.shell := by
  refine ⟨by decide, ?_⟩
  intro h
  cases h with
  | condT _ hp => cases hp
  | condF hc _ => exact absurd hc (by decide)

/-! ### (bonus) `infer` really is a set: the inferred list is duplicate-free -/

/-- `insertE` preserves duplicate-freeness. -/
theorem nodup_insertE (x : Effect) (l : List Effect) (h : l.Nodup) :
    (insertE x l).Nodup := by
  unfold insertE
  by_cases hx : x ∈ l
  · simp only [if_pos hx]; exact h
  · simp only [if_neg hx]; exact List.nodup_cons.mpr ⟨hx, h⟩

/-- `union` preserves duplicate-freeness (given a duplicate-free right operand). -/
theorem nodup_union (a b : List Effect) (hb : b.Nodup) : (union a b).Nodup := by
  induction a with
  | nil => simpa [union] using hb
  | cons h t ih =>
    have hstep : union (h :: t) b = insertE h (union t b) := rfl
    rw [hstep]; exact nodup_insertE h (union t b) ih

/-- **`infer e` is a genuine finite set:** it contains no duplicates. -/
theorem infer_nodup (e : Expr) : (infer e).Nodup := by
  induction e with
  | pureVal => exact List.nodup_nil
  | prim e => exact List.nodup_cons.mpr ⟨by simp, List.nodup_nil⟩
  | seq a b iha ihb => exact nodup_union (infer a) (infer b) ihb
  | cond c a b iha ihb => exact nodup_union (infer a) (infer b) ihb
  | call body ih => exact ih

/-! ### Transitive effect inference: the caller inherits the callee's effects -/

/-- **Transitive effect inference is sound.** A caller that invokes a callee `body` inherits ALL of
    the callee's effects: if the callee can perform `f`, then `f` is in the CALLER's inferred set.
    This mechanizes Anubis's transitive effect-inference discipline (Phase 2) — the effect check
    does not stop at a function boundary; a called function's effects flow into the caller's
    `inferred` set. -/
theorem infer_transitive {body : Expr} {f : Effect} :
    Performs body f → f ∈ infer (.call body) := by
  rintro ⟨w, h⟩
  exact infer_sound_run (PerformsIn.call h)

/-- **The gate is TRANSITIVELY sound.** If the CALLER passes `inferred ⊆ declared`, then every
    effect the callee performs THROUGH the call is declared by the caller — so a green build cannot
    hide an undeclared effect behind a function call. This is the transitive face of `gate_sound`. -/
theorem gate_sound_transitive {body : Expr} {declared : List Effect} {f : Effect} :
    Subseteq (infer (.call body)) declared → Performs body f → f ∈ declared := by
  intro hsub hperf
  exact hsub f (infer_transitive hperf)

end Anubis.EffectSoundness
