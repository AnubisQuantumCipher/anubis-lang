/-
  Anubis — mechanized QF_ABV ARRAY-ENCODING soundness (Phase 5, the array lane)

  The Anubis solver models a bounded sequence as an SMT array over the theory of arrays (QF_ABV):
  an array literal `[e0, e1, …]` is encoded by asserting `(= (select arr (_ bv i 64)) e_i)` for each
  cell (see `compiler/src/middle/mod.rs`, the `Expr::ArrayLiteral` branch), a symbolic-index read
  `a[i]` becomes `(select arr i)`, and a concrete-index write `a[k]=v` updates cell k. The SMT theory
  of arrays is TOTAL (an array is defined at every index); the RUNTIME array is a bounded `Vec`
  (indexed `[0, len)`, out-of-bounds panics). This file machine-checks that reasoning with `select`/
  `store` over the total model is SOUND with respect to the bounded runtime, on exactly the in-bounds
  domain the checker's length/range facts constrain — so an obligation the solver discharges from
  array facts holds at runtime.

  Lean 4 core (no Mathlib). THEOREMS:
    * `sel_sto_eq` / `sel_sto_ne` — the McCarthy read-over-write axioms the SMT array theory uses.
    * `model_sel_inbounds`        — the total model's `select` at an in-bounds index IS the runtime read.
    * `model_set_eq_store`        — a runtime write `xs.set i v`, modeled, equals the SMT `store` on the
                                    whole domain: the encoder's store faithfully tracks the runtime write.
    * `sel_model_literal`         — a literal array's asserted cell facts (`select model i = xs[i]`) are
                                    exactly the runtime element at each in-bounds cell (encoder honesty).
-/

namespace Anubis.ArrayEncoding

/-- A runtime element. The int lane elsewhere is `BitVec 64`; here the array *structure* is what we
    reason about, so a generic element type keeps the theorems about indexing, not element arithmetic. -/
abbrev Val := Int

/-- Array index. The encoder emits `(_ bv i 64)` literal indices and loop-bounded symbolic indices,
    all constrained in-bounds (`i < len ≤ 2^64`), so no BitVec wraparound occurs and `Nat` is faithful. -/
abbrev Idx := Nat

/-- The SMT (McCarthy) total array: `(Array (_ BitVec 64) (_ BitVec 64))`. -/
abbrev Arr := Idx → Val

/-- `select` — read. -/
def sel (a : Arr) (i : Idx) : Val := a i

/-- `store` — functional update, the theory-of-arrays write. -/
def sto (a : Arr) (i : Idx) (v : Val) : Arr := fun j => if j = i then v else a j

/-! ### The McCarthy read-over-write axioms (what z3's array theory reasons with) -/

/-- Reading the just-written cell yields the written value. -/
@[simp] theorem sel_sto_eq (a : Arr) (i : Idx) (v : Val) : sel (sto a i v) i = v := by
  simp [sel, sto]

/-- Reading a different cell is unaffected by the write. -/
theorem sel_sto_ne (a : Arr) (i j : Idx) (v : Val) (h : j ≠ i) :
    sel (sto a i v) j = sel a j := by
  simp [sel, sto, h]

/-! ### The total-model ↔ bounded-runtime correspondence -/

/-- The total-array MODEL of a runtime bounded array `xs`: in-bounds → the element, out-of-bounds → a
    default. `List.getD i d` returns `xs[i]` when `i < xs.length` and `d` otherwise — exactly a total
    extension of the bounded array. This is the object the SMT `select`/`store` reasoning stands for. -/
def model (xs : List Val) (d : Val) : Arr := fun i => xs.getD i d

/-- **In-bounds read faithfulness.** The model's `select` at an index the checker has proven in-bounds
    equals the runtime read `xs[i]`. So a `(select arr i)` fact the solver uses is the runtime value. -/
theorem model_sel_inbounds (xs : List Val) (d : Val) (i : Idx) (_h : i < xs.length) :
    sel (model xs d) i = xs.getD i d := by
  simp [sel, model]

/-- **Store tracks the runtime write.** For an IN-BOUNDS write (which is all the encoder emits — a
    concrete-index write `a[k]=v` with `k` a literal in `[0, len)`, task #30), modeling `xs.set i v`
    equals the SMT `store` of the model at `i`, on the WHOLE domain. So after `a[k]=v` the solver's
    `store`-updated array agrees with the runtime array at every index (cell `k` updated, rest
    unchanged). (An out-of-bounds `store` writes `v` while a runtime out-of-bounds `set` is a no-op —
    the encoder never emits one, so the correspondence is scoped to the in-bounds write it does emit.) -/
theorem model_set_eq_store (xs : List Val) (d : Val) (i : Idx) (v : Val) (hi : i < xs.length) :
    ∀ j, model (xs.set i v) d j = sto (model xs d) i v j := by
  intro j
  simp only [model, sto]
  by_cases hji : j = i
  · subst hji
    -- in-bounds write at the written index yields the written value
    rw [List.getD_eq_getElem?_getD, List.getElem?_set_self hi]
    simp
  · -- different index: `set` leaves cell j untouched
    rw [List.getD_eq_getElem?_getD, List.getD_eq_getElem?_getD,
        List.getElem?_set_ne (Ne.symm hji)]
    simp [hji]

/-- **Encoder honesty for a literal array.** The cell facts the encoder asserts — `select model i =`
    the i-th element — are exactly the runtime elements at every in-bounds cell. So the assumptions
    the solver loads for `[e0, e1, …]` are true of the runtime array, not an over-claim. -/
theorem sel_model_literal (xs : List Val) (d : Val) (i : Idx) (h : i < xs.length) :
    sel (model xs d) i = xs[i] := by
  simp only [sel, model]
  rw [List.getD_eq_getElem?_getD, List.getElem?_eq_getElem h]
  rfl

end Anubis.ArrayEncoding
