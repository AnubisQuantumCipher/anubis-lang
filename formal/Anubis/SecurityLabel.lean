/-
  Anubis — mechanized SECURITY-LABEL FINITE ABSTRACTION (Completion Blueprint Phase 8, Slice 1)

  The production `SecurityLabel` type in `compiler/src/middle/security_label.rs` is a three-variant
  Rust enum:

    enum SecurityLabel {
      Clean,
      Labeled { source: Option<String> },
      Unknown { reason: Option<&'static str> },
    }

  This file mechanizes the FINITE ABSTRACTION over which the six mission-scoped operations
  (`from_legacy_taint`, `from_legacy_secret`, `join`, `declassified_by`, `to_legacy_taint`,
  `to_legacy_secret`) are proved to agree with the production Rust bytes.

  ## Completeness of the abstraction

  None of the six operations INSPECT the CONTENT of a `source: Option<String>` or a
  `reason: Option<&'static str>`. Concrete rationale, per operation:

  * `from_legacy_taint`: matches on `(tainted, source)` only, never on the string content.
    `Labeled { source: s }` echoes the input `Option<String>` unchanged; the `Unknown`
    branch emits a FIXED constant string that is NOT derived from the input.
  * `from_legacy_secret`: takes a `bool` only.
  * `join`: matches on variant tags and combines `Option`s with `Option::or`, a
    presence-preserving operation that never reads a character of the wrapped string.
  * `declassified_by`: matches on `Self::Labeled { source: Some(_) }` guarded by a
    boolean; only presence of the source is tested.
  * `to_legacy_taint`: emits `Some("unknown-label")` (a fixed constant) for `Unknown`,
    passes the `source` through untouched for `Labeled`, and emits `(false, None)` for `Clean`.
  * `to_legacy_secret`: reads only the variant tag via `is_clean`.

  Therefore a two-token representative set for the source (`"s1"`, `"s2"`) and a two-token
  representative set for the reason (`"r1"`, `"r2"`) — plus `None` and the two FIXED constants
  the production code emits (`legacyShapeReason`, `unknownLabelSource`) — is COMPLETE. Two
  tokens are the minimum needed to expose join's LEFT-BIASED provenance/reason fallback
  (`Option::or`), which a single-token abstraction would silently accept as full-record
  commutativity.

  ## What this file proves

  Lean 4 core only. No `axiom`, `sorry`, `admit`, or `native_decide` (all four are rejected by
  `scripts/run_formal_gate.sh`). Every theorem below is decidable by `rfl`, structural case
  split, or `decide` over the finite abstract alphabet.

    * `fromLegacyTaint_*` — the four-shape classification and the ambiguous-shape carve-out
      (`(false, Some(_)) → Unknown (some legacyShapeReason)`);
    * `fromLegacySecret_*` — two-shape classification;
    * `join_*` — dominance, class-level commutativity, class-level idempotence, and a
      FIRST-BIAS COUNTEREXAMPLE that explicitly rules out full-record commutativity;
    * `declassifiedBy_*` — clears a CONCRETE labeled value only when `policy_ok = true`;
      NEVER clears `Unknown` or a source-less `Labeled`;
    * `toLegacyTaint_*` / `toLegacySecret_*` — the two adapters fail closed on `Unknown`
      (integrity ≠ `(false, None)`, confidentiality ≠ `false`).

  ## Boundary of this slice

  This module proves the MODEL. `scripts/run_security_label_correspondence_gate.sh` closes
  the model↔production loop by independently observing the Rust and Lean tables and
  byte-comparing them. See `docs/PROOF_CORRESPONDENCE.md` § "Production-linked SecurityLabel
  slice" for the exact TCB items this slice moves and the ones that stay open.
-/

namespace Anubis.SecurityLabel

/-! ### The finite abstraction -/

/-- Concrete fail-closed constants the production Rust code emits verbatim. Cited as
    string LITERALS on both sides so the observer TSV lines are byte-identical without any
    shared canonicalization table. -/
def legacyShapeReason : String := "legacy-shape: taint_source without tainted"
def unknownLabelSource : String := "unknown-label"

/-- The finite abstract security label. Mirrors the production Rust enum shape:
    `Clean`, `Labeled { source: Option<String> }`, `Unknown { reason: Option<String> }`.
    The reason is `Option String` (not `Option (&'static str)`) because Lean has no
    static-lifetime distinction; the abstraction is over VALUE identity, which is the same. -/
inductive Label where
  | clean
  | labeled : Option String → Label
  | unknown : Option String → Label
  deriving DecidableEq, Repr

/-- The three security-verdict classes. Used to state class-level commutativity and
    idempotence, which the production `SecurityLabel::join` satisfies even though the
    FULL RECORD is left-biased in provenance and reason. -/
inductive Class where
  | cClean
  | cLabeled
  | cUnknown
  deriving DecidableEq, Repr

def classOf : Label → Class
  | .clean       => .cClean
  | .labeled _   => .cLabeled
  | .unknown _   => .cUnknown

/-- Rust `Option::or` — return the first argument if `some`, otherwise the second.
    Left-biased. Mirrors `Option<T>::or(self, optb: Option<T>) -> Option<T>`. -/
def optOr {α : Type} : Option α → Option α → Option α
  | some x, _ => some x
  | none,   y => y

/-! ### The six operations, faithful to the Rust semantics -/

/-- Legacy adapter for the pre-lattice integrity-lane `(tainted, source)` shape.
    Mirrors `SecurityLabel::from_legacy_taint` at `compiler/src/middle/security_label.rs:131`:
      `(false, None)   → Clean`
      `(true,  s)      → Labeled { source: s }`
      `(false, Some _) → Unknown { reason: Some legacyShapeReason }` (shape error). -/
def fromLegacyTaint : Bool → Option String → Label
  | false, none     => .clean
  | true,  s        => .labeled s
  | false, some _   => .unknown (some legacyShapeReason)

/-- Legacy adapter for the pre-lattice confidentiality-lane `secret: bool`.
    Mirrors `SecurityLabel::from_legacy_secret` at `security_label.rs:147`:
      `false → Clean`, `true → Labeled { source: None }`. -/
def fromLegacySecret : Bool → Label
  | false => .clean
  | true  => .labeled none

/-- Lattice join. Non-overlapping 3×3 exhaustive cover of `Label × Label`, in the same
    verdict order as the Rust match at `security_label.rs:189`. -/
def join : Label → Label → Label
  | .labeled a,  .labeled b  => .labeled (optOr a b)
  | .labeled s,  .clean      => .labeled s
  | .labeled s,  .unknown _  => .labeled s
  | .clean,      .labeled s  => .labeled s
  | .unknown _,  .labeled s  => .labeled s
  | .unknown a,  .unknown b  => .unknown (optOr a b)
  | .unknown r,  .clean      => .unknown r
  | .clean,      .unknown r  => .unknown r
  | .clean,      .clean      => .clean

/-- Declassification. Clears a CONCRETE labeled value only when `policy_ok = true`.
    Mirrors `SecurityLabel::declassified_by` at `security_label.rs:221`. -/
def declassifiedBy : Label → Bool → Label
  | .labeled (some _), true => .clean
  | other,             _    => other

/-- Integrity-lane legacy adapter. Fails closed on `Unknown` (never emits `(false, None)`).
    Mirrors `SecurityLabel::to_legacy_taint` at `security_label.rs:243`. -/
def toLegacyTaint : Label → Bool × Option String
  | .clean       => (false, none)
  | .labeled s   => (true,  s)
  | .unknown _   => (true,  some unknownLabelSource)

/-- Confidentiality-lane legacy adapter. Fails closed on `Unknown` (never emits `false`).
    Mirrors `SecurityLabel::to_legacy_secret` at `security_label.rs:252`. -/
def toLegacySecret : Label → Bool
  | .clean => false
  | _      => true

/-! ### Classification theorems for `fromLegacyTaint` (mission §A.1) -/

theorem fromLegacyTaint_false_none : fromLegacyTaint false none = .clean := rfl

theorem fromLegacyTaint_true_none : fromLegacyTaint true none = .labeled none := rfl

theorem fromLegacyTaint_true_some (s : String) :
    fromLegacyTaint true (some s) = .labeled (some s) := rfl

theorem fromLegacyTaint_false_some_is_unknown (s : String) :
    fromLegacyTaint false (some s) = .unknown (some legacyShapeReason) := rfl

/-- The shape-error carve-out is total and independent of the input string. -/
theorem fromLegacyTaint_shape_error_reason (s1 s2 : String) :
    fromLegacyTaint false (some s1) = fromLegacyTaint false (some s2) := rfl

/-! ### Classification theorems for `fromLegacySecret` -/

theorem fromLegacySecret_false : fromLegacySecret false = .clean := rfl

theorem fromLegacySecret_true : fromLegacySecret true = .labeled none := rfl

/-! ### Join dominance, class commutativity, class idempotence -/

/-- `Labeled` dominates every other class on either side of a join. -/
theorem join_labeled_left (s : Option String) (b : Label) :
    classOf (join (.labeled s) b) = .cLabeled := by
  cases b <;> rfl

theorem join_labeled_right (s : Option String) (a : Label) :
    classOf (join a (.labeled s)) = .cLabeled := by
  cases a <;> rfl

/-- Between `Unknown` and `Clean`, `Unknown` dominates. -/
theorem join_unknown_clean (r : Option String) :
    join (.unknown r) .clean = .unknown r := rfl

theorem join_clean_unknown (r : Option String) :
    join .clean (.unknown r) = .unknown r := rfl

/-- `Clean` is neutral only against itself. -/
theorem join_clean_clean : join .clean .clean = .clean := rfl

/-- Join is COMMUTATIVE at the verdict-class level. Class-level, NOT full-record: the
    provenance/reason side loses information when the two sides differ, so the WITNESS
    below rules out full commutativity as a theorem. -/
theorem join_class_comm (a b : Label) : classOf (join a b) = classOf (join b a) := by
  cases a <;> cases b <;> rfl

/-- Join is IDEMPOTENT at the verdict-class level. -/
theorem join_class_idempotent (a : Label) : classOf (join a a) = classOf a := by
  cases a <;> rfl

/-- **Full-record idempotence.** Stronger than `join_class_idempotent`: not only does
    the class of `join a a` equal the class of `a`, the entire label (including
    provenance/reason) is preserved. This works because `optOr x x = x` for every
    `x : Option α` — the left-bias rule collapses when both sides carry the same value.
    Together with `join_full_not_commutative(_unknown)`, this pins the FULL semantics
    of `join` without overclaiming commutativity on distinct-source pairs. -/
theorem join_full_idempotent (a : Label) : join a a = a := by
  cases a with
  | clean       => rfl
  | labeled s   => cases s <;> rfl
  | unknown r   => cases r <;> rfl

/-- Full-record commutativity FAILS: with two distinct `Labeled` sources, join's left
    bias picks the FIRST argument's source. This is the mission's guardrail against
    overclaiming: only class-level commutativity, never full-record. -/
theorem join_full_not_commutative :
    join (.labeled (some "a")) (.labeled (some "b"))
      ≠ join (.labeled (some "b")) (.labeled (some "a")) := by
  decide

/-- Same guardrail on the `Unknown` reason side. -/
theorem join_full_not_commutative_unknown :
    join (.unknown (some "a")) (.unknown (some "b"))
      ≠ join (.unknown (some "b")) (.unknown (some "a")) := by
  decide

/-- The left-bias EXACTLY: when the first side carries a source, the join keeps it. -/
theorem join_labeled_first_wins (s : String) (b : Option String) :
    join (.labeled (some s)) (.labeled b) = .labeled (some s) := rfl

/-- The fall-back rule: when the first side has NO source, the second side's source
    is preserved. Together with `join_labeled_first_wins` this pins the FULL provenance
    semantics — no source is ever fabricated, and no source is ever silently dropped
    when at least one side carries one. -/
theorem join_labeled_none_falls_back (b : Option String) :
    join (.labeled none) (.labeled b) = .labeled b := by
  cases b <;> rfl

/-- Same first-wins rule on the `Unknown` reason side. -/
theorem join_unknown_first_wins (r : String) (b : Option String) :
    join (.unknown (some r)) (.unknown b) = .unknown (some r) := rfl

theorem join_unknown_none_falls_back (b : Option String) :
    join (.unknown none) (.unknown b) = .unknown b := by
  cases b <;> rfl

/-! ### Declassification laws (mission §84-85) -/

theorem declassifiedBy_labeled_some_ok (s : String) :
    declassifiedBy (.labeled (some s)) true = .clean := rfl

theorem declassifiedBy_labeled_some_fail (s : String) :
    declassifiedBy (.labeled (some s)) false = .labeled (some s) := rfl

/-- Declassify NEVER clears a source-less `Labeled`. Both policy branches. -/
theorem declassifiedBy_labeled_none (b : Bool) :
    declassifiedBy (.labeled none) b = .labeled none := by
  cases b <;> rfl

/-- Declassify NEVER clears `Unknown`. Both policy branches, both reasons. -/
theorem declassifiedBy_unknown (r : Option String) (b : Bool) :
    declassifiedBy (.unknown r) b = .unknown r := by
  cases b <;> rfl

theorem declassifiedBy_clean (b : Bool) :
    declassifiedBy .clean b = .clean := by
  cases b <;> rfl

/-! ### Adapter fail-closed laws (mission §consumer promotion, Slice 5) -/

/-- Integrity adapter never maps `Unknown` to `(false, None)`. -/
theorem toLegacyTaint_unknown_is_tainted (r : Option String) :
    (toLegacyTaint (.unknown r)).1 = true := by
  cases r <;> rfl

theorem toLegacyTaint_unknown_source_is_unknown_label (r : Option String) :
    (toLegacyTaint (.unknown r)).2 = some unknownLabelSource := by
  cases r <;> rfl

/-- Integrity adapter reports every `Labeled` as tainted, preserving the source untouched. -/
theorem toLegacyTaint_labeled (s : Option String) :
    toLegacyTaint (.labeled s) = (true, s) := rfl

theorem toLegacyTaint_clean : toLegacyTaint .clean = (false, none) := rfl

/-- Confidentiality adapter never maps `Unknown` to `false`. -/
theorem toLegacySecret_unknown (r : Option String) :
    toLegacySecret (.unknown r) = true := by
  cases r <;> rfl

theorem toLegacySecret_labeled (s : Option String) :
    toLegacySecret (.labeled s) = true := by
  cases s <;> rfl

theorem toLegacySecret_clean : toLegacySecret .clean = false := rfl

/-! ### Composition sanity: the adapters are surjective onto the fail-closed rows

    A round-trip witness: the LEGACY output an integrity consumer would see for a
    concrete labeled input carries that same source unchanged, so no round-trip
    can silently fabricate provenance. -/

theorem toLegacyTaint_labeled_some_preserves (s : String) :
    toLegacyTaint (.labeled (some s)) = (true, some s) := rfl

theorem toLegacyTaint_labeled_none : toLegacyTaint (.labeled none) = (true, none) := rfl

/-! ### The finite corpus the correspondence gate observes -/

/-- The 7-element abstract-label corpus. Two source tokens `s1`, `s2` and two reason
    tokens `r1`, `r2` are the MINIMUM sufficient to force the first-bias / fall-back
    distinction on both `join` axes; a single-token corpus would silently accept
    full-record commutativity as a theorem. -/
def abstractLabels : List Label :=
  [ .clean,
    .labeled none,
    .labeled (some "s1"),
    .labeled (some "s2"),
    .unknown none,
    .unknown (some "r1"),
    .unknown (some "r2") ]

def bools : List Bool := [false, true]

/-- The 2-element `Option String` input corpus for `fromLegacyTaint`.
    One source-token representative is complete for that operation (its source path
    is pure echo), so we only need presence-vs-absence. -/
def sourceInputs : List (Option String) := [none, some "s1"]

/-! ### Encoders for the correspondence TSV

    Both the Lean observer (`Anubis/SecurityLabelObserver.lean`) and the Rust observer
    (`compiler::middle::observe_security_label_correspondence`) emit exactly these
    strings, byte-for-byte, so `cmp` between the two output files is the correspondence
    predicate.

    IMPORTANT: no shared canonicalization table is used. Both sides name the SAME string
    LITERALS (the `legacyShapeReason` and `unknownLabelSource` constants above, matched
    to the Rust source at `compiler/src/middle/security_label.rs:136,247`). Byte
    equality comes from those literals being computed independently, not from a shared
    remapping function. -/

def encodeBool : Bool → String
  | false => "false"
  | true  => "true"

def encodeSourceInput : Option String → String
  | none     => "none"
  | some s   => s!"some:{s}"

def encodeLabel : Label → String
  | .clean               => "Clean"
  | .labeled none        => "Labeled(none)"
  | .labeled (some s)    => s!"Labeled(some:{s})"
  | .unknown none        => "Unknown(none)"
  | .unknown (some r)    => s!"Unknown(some:{r})"

def encodeLegacyTaint : Bool × Option String → String
  | (t, none)     => s!"Legacy(tainted={encodeBool t},source=none)"
  | (t, some s)   => s!"Legacy(tainted={encodeBool t},source=some:{s})"

/-- One TSV row: `op\targ1\targ2\tout`. `-` is used for an absent second argument. -/
def rowTsv (op : String) (arg1 arg2 out : String) : String :=
  op ++ "\t" ++ arg1 ++ "\t" ++ arg2 ++ "\t" ++ out

/-- The complete row list, in the canonical deterministic order the correspondence
    gate expects. Total row count is `4 + 2 + 49 + 14 + 7 + 7 = 83`. -/
def observationRows : List String :=
  -- 1. from_legacy_taint over Bool × {none, some:s1}  (4 rows)
  (bools.flatMap (fun t =>
    sourceInputs.map (fun s =>
      rowTsv "from_legacy_taint"
        (encodeBool t)
        (encodeSourceInput s)
        (encodeLabel (fromLegacyTaint t s))))) ++
  -- 2. from_legacy_secret over Bool  (2 rows)
  (bools.map (fun b =>
    rowTsv "from_legacy_secret"
      (encodeBool b) "-"
      (encodeLabel (fromLegacySecret b)))) ++
  -- 3. join over abstractLabels × abstractLabels  (49 rows)
  (abstractLabels.flatMap (fun a =>
    abstractLabels.map (fun b =>
      rowTsv "join"
        (encodeLabel a) (encodeLabel b)
        (encodeLabel (join a b))))) ++
  -- 4. declassified_by over abstractLabels × Bool  (14 rows)
  (abstractLabels.flatMap (fun a =>
    bools.map (fun p =>
      rowTsv "declassified_by"
        (encodeLabel a) (encodeBool p)
        (encodeLabel (declassifiedBy a p))))) ++
  -- 5. to_legacy_taint over abstractLabels  (7 rows)
  (abstractLabels.map (fun a =>
    rowTsv "to_legacy_taint"
      (encodeLabel a) "-"
      (encodeLegacyTaint (toLegacyTaint a)))) ++
  -- 6. to_legacy_secret over abstractLabels  (7 rows)
  (abstractLabels.map (fun a =>
    rowTsv "to_legacy_secret"
      (encodeLabel a) "-"
      (encodeBool (toLegacySecret a))))

/-- The declared total row count. Locked by `observationRows_length`. -/
def declaredRowCount : Nat := 83

/-- Machine-checked: the corpus has exactly the declared row count. -/
theorem observationRows_length : observationRows.length = declaredRowCount := by
  decide

/-- Machine-checked: no duplicate row. The gate additionally checks uniqueness of the
    `(op, arg1, arg2)` KEY (a duplicate key with different outputs is a schema
    violation); this decidable fact locks the corpus against silent shrinkage. -/
theorem observationRows_nodup : observationRows.Nodup := by
  decide

end Anubis.SecurityLabel
