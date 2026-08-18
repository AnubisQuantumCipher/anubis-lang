//! Completion Blueprint Phase 3 — explicit security-label lattice.
//!
//! Introduced by Slice 2 of the Phase 3 arc. The mission (see
//! `docs/language/ROADMAP.md` § "Completion Blueprint Phase 3 —
//! security-label lattice") requires that security-relevant root-binding
//! booleans and `Option`-means-clean ambiguity be replaced with an explicit
//! lane-independent state that distinguishes:
//!
//! - `Clean`    — proven unlabeled on the analyzed path(s);
//! - `Labeled`  — may carry the lane's label; retains stable provenance
//!   (`source`) when the analysis knows one;
//! - `Unknown`  — analysis lacks evidence to prove `Clean` or to identify a
//!   concrete labeled source.
//!
//! Slice 2 introduced the type, constructors, lattice `join`, and legacy
//! adapters without migrating callers. Slice 3 stores the lattice state on
//! `ScopeBinding` and makes the three root producers write it. Slice 4
//! migrates the five remaining path/carrier writers (control-flow-merge,
//! aggregate mutation, non-`Var` place-assign taint and secret duals, and
//! the `ReturnSummaryLane::taint_place` helper) to the lattice setters, so
//! every direct write to `info.tainted` / `taint_source` / `declassified` /
//! `secret` outside the two setter methods is gone.
//!
//! Slice 5 classifies and promotes every Unknown case:
//!
//! Producer sources of `Unknown`:
//! - `SecurityLabel::from_legacy_taint(false, Some(_))` — the historical
//!   "shape error" where a stale `taint_source` accompanied a false
//!   `tainted` bool (documented on the constructor). Now reachable only from
//!   `sync_labels_from_legacy`, which shadow-logs the site.
//! - Explicit `SecurityLabel::unknown(reason)` — reserved for producers
//!   that intentionally give up (currently the two Slice 3/5 regression
//!   tests).
//!
//! Terminal-consumer promotion to fail-closed:
//! - `set_taint_label` derives `info.tainted = true` and
//!   `taint_source = Some("unknown-label")` for any `Unknown` — every
//!   downstream consumer reading `info.tainted` therefore refuses to treat
//!   `Unknown` as `Clean`. The existing `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`
//!   / `ANUBIS_SECRET_EXFILTRATION` diagnostics fire without a new
//!   diagnostic code needing to be added.
//! - `set_secret_label` derives `secret = true` for `Unknown` on the
//!   confidentiality lane, satisfying the same fail-closed contract.
//! - The two sink emit sites in `analyze_expr_effect` additionally
//!   shadow-log `taint_sink_consumer` / `secret_egress_consumer` on any
//!   `Unknown` at a plain-`Var` argument, so a review can promote the
//!   generic rejection to a dedicated `ANUBIS_PHASE3_UNKNOWN_AT_SINK`
//!   diagnostic in a later slice if that precision is wanted; no verdict
//!   changes here.
//!
//! ## Lattice laws
//!
//! The mission (§78) requires exactly these `join` laws, and they are locked
//! by unit tests in this file:
//!
//! - `join(Clean, Clean) = Clean`
//! - `join(Labeled, _) = Labeled` and `join(_, Labeled) = Labeled`
//! - `join(Unknown, Clean) = Unknown` (and symmetrically)
//! - `join(Unknown, Unknown) = Unknown`
//! - declassification (§84-85) clears a *concrete* `Labeled { source: Some }`
//!   only when the caller has already established the well-formed
//!   policy/reason condition; `Unknown` and `Labeled { source: None }` MUST
//!   NOT silently collapse to `Clean` merely because a boolean defaulted to
//!   `false` or an `Option` was `None`.

/// Lane-independent security state for a `ScopeBinding` root or a value
/// under analysis. See the module docs for the lattice laws and the mission
/// citations that authorize them.
///
/// This remains `pub(crate)` while Phase 3 migrates the checker in bounded
/// slices. Slice 3 uses it for root binding transfer; Slices 4-5 extend the
/// same domain through carriers and terminal consumers.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[allow(dead_code)] // Phase 3 transition: later slices consume the remaining operations.
pub(crate) enum SecurityLabel {
    /// Proven unlabeled on every analyzed path.
    #[default]
    Clean,
    /// May carry the lane's label. `source` retains the stable provenance
    /// string when the producer knows one (matches the historical
    /// `taint_source: Option<String>` shape); `None` means "labeled but the
    /// concrete source is not recorded" — legal on the confidentiality lane
    /// which historically carried no source, and legal on the integrity lane
    /// when the analysis chose not to attribute a source.
    Labeled { source: Option<String> },
    /// Analysis lacks evidence to prove either `Clean` or to identify a
    /// concrete labeled source. `reason` is a short static hint for
    /// diagnostics (e.g. `Some("missing-scope-entry")`); `None` is legal.
    ///
    /// Slice 5 makes every security-sensitive terminal consumer explicit
    /// about how it handles this variant — the mission (§111) forbids
    /// silently equating `Unknown` with `Clean`.
    Unknown { reason: Option<&'static str> },
}

#[allow(dead_code)] // Phase 3 transition: later slices consume the remaining operations.
impl SecurityLabel {
    /// Construct a `Clean` label.
    pub(crate) fn clean() -> Self {
        Self::Clean
    }

    /// Construct a `Labeled` label with the given optional provenance.
    pub(crate) fn labeled(source: Option<String>) -> Self {
        Self::Labeled { source }
    }

    /// Construct a `Labeled` label with a concrete provenance string.
    pub(crate) fn labeled_from(source: impl Into<String>) -> Self {
        Self::Labeled {
            source: Some(source.into()),
        }
    }

    /// Construct an `Unknown` label with an optional short static reason.
    pub(crate) fn unknown(reason: Option<&'static str>) -> Self {
        Self::Unknown { reason }
    }

    /// Legacy adapter: interpret the pre-lattice integrity-lane
    /// `(tainted: bool, source: Option<String>)` shape.
    ///
    /// - `(false, None)` → `Clean`
    /// - `(true, s)`     → `Labeled { source: s }`
    /// - `(false, Some(_))` is a shape error and yields `Unknown` with a
    ///   documented reason — the historical code sometimes recorded a
    ///   `taint_source` while `tainted` was still false (see e.g. the
    ///   post-declassify write in `merge_taint_over`), and the mission
    ///   explicitly forbids silently collapsing that unresolved shape to
    ///   `Clean`.
    pub(crate) fn from_legacy_taint(tainted: bool, source: Option<String>) -> Self {
        match (tainted, source) {
            (false, None) => Self::Clean,
            (true, s) => Self::Labeled { source: s },
            (false, Some(_)) => Self::Unknown {
                reason: Some("legacy-shape: taint_source without tainted"),
            },
        }
    }

    /// Legacy adapter: interpret the pre-lattice confidentiality-lane
    /// `secret: bool` shape.
    ///
    /// - `false` → `Clean`
    /// - `true`  → `Labeled { source: None }` (the confidentiality lane
    ///   historically carried no attributable source string)
    pub(crate) fn from_legacy_secret(secret: bool) -> Self {
        if secret {
            Self::Labeled { source: None }
        } else {
            Self::Clean
        }
    }

    /// True iff `self` is `Clean`.
    pub(crate) fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }

    /// True iff `self` is `Labeled { .. }` (any source).
    pub(crate) fn is_labeled(&self) -> bool {
        matches!(self, Self::Labeled { .. })
    }

    /// True iff `self` is `Unknown { .. }` (any reason).
    pub(crate) fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown { .. })
    }

    /// Provenance string if `self` is `Labeled { source: Some(_) }`;
    /// `None` for `Clean`, `Unknown`, or `Labeled { source: None }`.
    pub(crate) fn provenance(&self) -> Option<&str> {
        match self {
            Self::Labeled { source: Some(s) } => Some(s.as_str()),
            _ => None,
        }
    }

    /// Reason string if `self` is `Unknown { reason: Some(_) }`.
    pub(crate) fn unknown_reason(&self) -> Option<&'static str> {
        match self {
            Self::Unknown { reason } => *reason,
            _ => None,
        }
    }

    /// Lattice `join` per mission §78. See the module docs for the exact
    /// laws; the unit tests below lock them.
    pub(crate) fn join(self, other: Self) -> Self {
        use SecurityLabel::*;
        match (self, other) {
            (Labeled { source: a }, Labeled { source: b }) => {
                // Retain the first known source; fall back to the second so
                // a joiner that lost provenance on one side still keeps the
                // other side's.
                Labeled { source: a.or(b) }
            }
            (Labeled { source }, _) | (_, Labeled { source }) => Labeled { source },
            (Unknown { reason: a }, Unknown { reason: b }) => Unknown { reason: a.or(b) },
            (Unknown { reason }, Clean) | (Clean, Unknown { reason }) => Unknown { reason },
            (Clean, Clean) => Clean,
        }
    }

    /// In-place join of `self` with `other`.
    pub(crate) fn join_assign(&mut self, other: Self) {
        let taken = core::mem::take(self);
        *self = taken.join(other);
    }

    /// Declassification per mission §84-85. Clears a *concrete* labeled
    /// value only when the caller has already established the well-formed
    /// policy/reason condition (`policy_ok = true`).
    ///
    /// - `Labeled { source: Some(_) }` + `policy_ok=true`  → `Clean`
    /// - `Labeled { source: Some(_) }` + `policy_ok=false` → unchanged
    /// - `Labeled { source: None }`  → unchanged (unresolved provenance;
    ///   mission §85 forbids silent collapse to `Clean`)
    /// - `Unknown { .. }`            → unchanged (analysis lacks evidence)
    /// - `Clean`                     → `Clean` (idempotent)
    pub(crate) fn declassified_by(self, policy_ok: bool) -> Self {
        match self {
            Self::Labeled { source: Some(_) } if policy_ok => Self::Clean,
            other => other,
        }
    }

    /// Emit a shadow-log line when `ANUBIS_PHASE3_SHADOW` is set. Slice 3
    /// records newly-visible `Unknown` without changing any check verdict.
    pub(crate) fn shadow_unknown(site: &'static str, reason: Option<&'static str>) {
        if std::env::var_os("ANUBIS_PHASE3_SHADOW").is_none() {
            return;
        }
        match reason {
            Some(r) => eprintln!("ANUBIS_PHASE3_UNKNOWN site={site} reason={r}"),
            None => eprintln!("ANUBIS_PHASE3_UNKNOWN site={site}"),
        }
    }

    /// Integrity-lane adapter: `Unknown` MUST NOT become `(false, None)`.
    /// Slice 5 is what promotes this to a user-facing diagnostic; Slice 3
    /// only guarantees the adapter cannot invent Clean.
    pub(crate) fn to_legacy_taint(&self) -> (bool, Option<String>) {
        match self {
            Self::Clean => (false, None),
            Self::Labeled { source } => (true, source.clone()),
            Self::Unknown { .. } => (true, Some("unknown-label".to_string())),
        }
    }

    /// Confidentiality-lane adapter: `Unknown` MUST NOT become `false`.
    pub(crate) fn to_legacy_secret(&self) -> bool {
        !self.is_clean()
    }
}

/// Completion Blueprint Phase 8 Slice 1 — production-linked correspondence observer.
///
/// Emit one canonical TSV row per (op, args) tuple over the DECLARED FINITE ABSTRACTION
/// for the six mission-scoped operations. Every row is produced by CALLING the actual
/// `SecurityLabel` method above — this function does NOT carry a shadow reimplementation.
/// The Lean model at `formal/Anubis/SecurityLabel.lean` computes the SAME rows over its
/// own kernel-checked semantics; the correspondence gate
/// (`scripts/run_security_label_correspondence_gate.sh`) byte-compares the two outputs.
///
/// Row format: `op\targ1\targ2\tout\n` (line terminator per row, `-` for an absent
/// second argument). Both encoders emit the string LITERALS `"legacy-shape: taint_source
/// without tainted"` (from `from_legacy_taint`) and `"unknown-label"` (from
/// `to_legacy_taint`) as-is, so the observation streams match byte-for-byte without any
/// shared canonicalization table.
///
/// The corpus is:
/// * `from_legacy_taint`: {false,true} × {None, Some("s1")} — 4 rows
/// * `from_legacy_secret`: {false,true} — 2 rows
/// * `join`: `ABSTRACT_LABELS × ABSTRACT_LABELS` — 49 rows
/// * `declassified_by`: `ABSTRACT_LABELS × {false,true}` — 14 rows
/// * `to_legacy_taint`: `ABSTRACT_LABELS` — 7 rows
/// * `to_legacy_secret`: `ABSTRACT_LABELS` — 7 rows
///
/// Total: 83 rows. `DECLARED_ROW_COUNT` locks that number for the harness.
pub(crate) const DECLARED_ROW_COUNT: usize = 83;

/// The finite abstract-label corpus. Two source tokens (`"s1"`, `"s2"`) and two reason
/// tokens (`"r1"`, `"r2"`) are the MINIMUM sufficient to expose join's left-biased
/// provenance and reason fallback; a single-token corpus would silently accept full-record
/// commutativity, which the mission explicitly forbids as a claim.
fn abstract_labels() -> Vec<SecurityLabel> {
    vec![
        SecurityLabel::Clean,
        SecurityLabel::Labeled { source: None },
        SecurityLabel::Labeled {
            source: Some("s1".to_string()),
        },
        SecurityLabel::Labeled {
            source: Some("s2".to_string()),
        },
        SecurityLabel::Unknown { reason: None },
        SecurityLabel::Unknown { reason: Some("r1") },
        SecurityLabel::Unknown { reason: Some("r2") },
    ]
}

fn encode_bool(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

fn encode_source_input(s: Option<&str>) -> String {
    match s {
        None => "none".to_string(),
        Some(s) => format!("some:{}", s),
    }
}

fn encode_label(l: &SecurityLabel) -> String {
    match l {
        SecurityLabel::Clean => "Clean".to_string(),
        SecurityLabel::Labeled { source: None } => "Labeled(none)".to_string(),
        SecurityLabel::Labeled { source: Some(s) } => format!("Labeled(some:{})", s),
        SecurityLabel::Unknown { reason: None } => "Unknown(none)".to_string(),
        SecurityLabel::Unknown { reason: Some(r) } => format!("Unknown(some:{})", r),
    }
}

fn encode_legacy_taint(t: bool, s: &Option<String>) -> String {
    match s {
        None => format!("Legacy(tainted={},source=none)", encode_bool(t)),
        Some(s) => format!("Legacy(tainted={},source=some:{})", encode_bool(t), s),
    }
}

pub(crate) fn observe_correspondence_rows<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    let bools: [bool; 2] = [false, true];
    let source_inputs: [Option<&str>; 2] = [None, Some("s1")];

    // 1. from_legacy_taint — 4 rows.
    for &t in bools.iter() {
        for &s in source_inputs.iter() {
            let out_label = SecurityLabel::from_legacy_taint(t, s.map(|s| s.to_string()));
            writeln!(
                out,
                "from_legacy_taint\t{}\t{}\t{}",
                encode_bool(t),
                encode_source_input(s),
                encode_label(&out_label),
            )?;
        }
    }

    // 2. from_legacy_secret — 2 rows.
    for &b in bools.iter() {
        let out_label = SecurityLabel::from_legacy_secret(b);
        writeln!(
            out,
            "from_legacy_secret\t{}\t-\t{}",
            encode_bool(b),
            encode_label(&out_label),
        )?;
    }

    let labels = abstract_labels();

    // 3. join — 49 rows.
    for a in labels.iter() {
        for b in labels.iter() {
            let out_label = a.clone().join(b.clone());
            writeln!(
                out,
                "join\t{}\t{}\t{}",
                encode_label(a),
                encode_label(b),
                encode_label(&out_label),
            )?;
        }
    }

    // 4. declassified_by — 14 rows.
    for a in labels.iter() {
        for &p in bools.iter() {
            let out_label = a.clone().declassified_by(p);
            writeln!(
                out,
                "declassified_by\t{}\t{}\t{}",
                encode_label(a),
                encode_bool(p),
                encode_label(&out_label),
            )?;
        }
    }

    // 5. to_legacy_taint — 7 rows.
    for a in labels.iter() {
        let (t, s) = a.to_legacy_taint();
        writeln!(
            out,
            "to_legacy_taint\t{}\t-\t{}",
            encode_label(a),
            encode_legacy_taint(t, &s),
        )?;
    }

    // 6. to_legacy_secret — 7 rows.
    for a in labels.iter() {
        let s = a.to_legacy_secret();
        writeln!(
            out,
            "to_legacy_secret\t{}\t-\t{}",
            encode_label(a),
            encode_bool(s),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SecurityLabel;

    // Helpers for readability.
    fn lbl(src: &str) -> SecurityLabel {
        SecurityLabel::labeled_from(src)
    }
    fn lbl_none() -> SecurityLabel {
        SecurityLabel::labeled(None)
    }
    fn unk() -> SecurityLabel {
        SecurityLabel::unknown(Some("test"))
    }

    // ── constructors and predicates ────────────────────────────────────────

    #[test]
    fn default_is_clean() {
        let s: SecurityLabel = Default::default();
        assert!(s.is_clean());
        assert!(!s.is_labeled());
        assert!(!s.is_unknown());
        assert_eq!(s.provenance(), None);
        assert_eq!(s.unknown_reason(), None);
    }

    #[test]
    fn labeled_with_source_reports_provenance() {
        let s = lbl("field `k`");
        assert!(!s.is_clean());
        assert!(s.is_labeled());
        assert!(!s.is_unknown());
        assert_eq!(s.provenance(), Some("field `k`"));
    }

    #[test]
    fn labeled_without_source_has_no_provenance_but_is_labeled() {
        let s = lbl_none();
        assert!(s.is_labeled());
        assert_eq!(s.provenance(), None);
    }

    #[test]
    fn unknown_with_reason_reports_it() {
        let s = SecurityLabel::unknown(Some("missing-scope-entry"));
        assert!(s.is_unknown());
        assert_eq!(s.unknown_reason(), Some("missing-scope-entry"));
        // An Unknown MUST NOT masquerade as Clean.
        assert!(!s.is_clean());
    }

    // ── legacy adapters ────────────────────────────────────────────────────

    #[test]
    fn from_legacy_taint_all_four_shapes() {
        assert_eq!(
            SecurityLabel::from_legacy_taint(false, None),
            SecurityLabel::Clean
        );
        assert_eq!(
            SecurityLabel::from_legacy_taint(true, Some("s".to_string())),
            SecurityLabel::Labeled {
                source: Some("s".to_string())
            }
        );
        assert_eq!(
            SecurityLabel::from_legacy_taint(true, None),
            SecurityLabel::Labeled { source: None }
        );
        // Shape error: not silently Clean.
        let ambiguous = SecurityLabel::from_legacy_taint(false, Some("s".to_string()));
        assert!(
            ambiguous.is_unknown(),
            "shape error must not collapse to Clean"
        );
    }

    #[test]
    fn from_legacy_secret_maps_both_bools() {
        assert_eq!(
            SecurityLabel::from_legacy_secret(false),
            SecurityLabel::Clean
        );
        assert_eq!(
            SecurityLabel::from_legacy_secret(true),
            SecurityLabel::Labeled { source: None }
        );
    }

    // ── join laws (mission §78) ────────────────────────────────────────────

    #[test]
    fn join_clean_clean_is_clean() {
        assert_eq!(
            SecurityLabel::Clean.join(SecurityLabel::Clean),
            SecurityLabel::Clean
        );
    }

    #[test]
    fn join_labeled_anything_is_labeled_both_sides() {
        let l = lbl("s");
        assert!(l.clone().join(SecurityLabel::Clean).is_labeled());
        assert!(SecurityLabel::Clean.join(l.clone()).is_labeled());
        assert!(l.clone().join(unk()).is_labeled());
        assert!(unk().join(l.clone()).is_labeled());
        assert!(l.clone().join(l).is_labeled());
    }

    #[test]
    fn join_preserves_provenance_from_first_side_and_falls_back() {
        // First side has a source: it wins.
        let a = lbl("first");
        let b = lbl("second");
        match a.join(b) {
            SecurityLabel::Labeled { source: Some(s) } => assert_eq!(s, "first"),
            other => panic!("expected Labeled with source `first`, got {other:?}"),
        }
        // First side is source-less; fall back to the second's source.
        let a = lbl_none();
        let b = lbl("second");
        match a.join(b) {
            SecurityLabel::Labeled { source: Some(s) } => assert_eq!(s, "second"),
            other => panic!("expected fallback provenance `second`, got {other:?}"),
        }
        // Both source-less: still Labeled, no source.
        let a = lbl_none();
        let b = lbl_none();
        assert_eq!(a.join(b), SecurityLabel::Labeled { source: None });
    }

    #[test]
    fn join_unknown_clean_is_unknown_both_sides() {
        let u = unk();
        assert!(u.clone().join(SecurityLabel::Clean).is_unknown());
        assert!(SecurityLabel::Clean.join(u).is_unknown());
    }

    #[test]
    fn join_unknown_unknown_is_unknown_and_prefers_first_reason() {
        let a = SecurityLabel::unknown(Some("a"));
        let b = SecurityLabel::unknown(Some("b"));
        let joined = a.join(b);
        assert_eq!(joined.unknown_reason(), Some("a"));
        // Reason-less first: fall back to second.
        let a = SecurityLabel::unknown(None);
        let b = SecurityLabel::unknown(Some("b"));
        assert_eq!(a.join(b).unknown_reason(), Some("b"));
    }

    #[test]
    fn join_is_commutative_on_verdict_and_idempotent() {
        for a in [SecurityLabel::Clean, lbl("s"), lbl_none(), unk()] {
            for b in [SecurityLabel::Clean, lbl("t"), lbl_none(), unk()] {
                let left = a.clone().join(b.clone());
                let right = b.clone().join(a.clone());
                assert_eq!(left.is_clean(), right.is_clean());
                assert_eq!(left.is_labeled(), right.is_labeled());
                assert_eq!(left.is_unknown(), right.is_unknown());
                // Idempotence: joining with self is self (verdict-preserving;
                // provenance may collapse to `a`'s side by the fall-back rule).
                let self_joined = a.clone().join(a.clone());
                assert_eq!(self_joined.is_clean(), a.is_clean());
                assert_eq!(self_joined.is_labeled(), a.is_labeled());
                assert_eq!(self_joined.is_unknown(), a.is_unknown());
            }
        }
    }

    #[test]
    fn join_assign_matches_join() {
        let mut a = lbl_none();
        a.join_assign(lbl("s"));
        assert_eq!(
            a,
            SecurityLabel::Labeled {
                source: Some("s".to_string())
            }
        );

        let mut b = SecurityLabel::Clean;
        b.join_assign(unk());
        assert!(b.is_unknown());

        let mut c = SecurityLabel::Clean;
        c.join_assign(SecurityLabel::Clean);
        assert!(c.is_clean());
    }

    // ── declassification (mission §84-85) ──────────────────────────────────

    #[test]
    fn declassify_ok_clears_concrete_labeled() {
        let s = lbl("field `k`");
        assert_eq!(s.declassified_by(true), SecurityLabel::Clean);
    }

    #[test]
    fn declassify_fail_leaves_concrete_labeled() {
        let s = lbl("field `k`");
        assert_eq!(s.clone().declassified_by(false), s);
    }

    #[test]
    fn declassify_never_clears_source_less_labeled() {
        // Mission §85: no silent Clean from unresolved provenance.
        let s = lbl_none();
        assert_eq!(s.clone().declassified_by(true), s);
    }

    #[test]
    fn declassify_never_clears_unknown() {
        // Mission §85: no silent Clean from unresolved provenance.
        let s = SecurityLabel::unknown(Some("no-scope-entry"));
        assert_eq!(s.clone().declassified_by(true), s);
    }

    #[test]
    fn declassify_of_clean_is_clean() {
        assert_eq!(
            SecurityLabel::Clean.declassified_by(true),
            SecurityLabel::Clean
        );
        assert_eq!(
            SecurityLabel::Clean.declassified_by(false),
            SecurityLabel::Clean
        );
    }

    #[test]
    fn to_legacy_taint_never_collapses_unknown_to_clean() {
        let (tainted, source) = unk().to_legacy_taint();
        assert!(tainted, "Unknown must not become tainted=false");
        assert!(
            source.is_some(),
            "Unknown must not become taint_source=None"
        );
    }

    #[test]
    fn to_legacy_secret_never_collapses_unknown_to_clean() {
        assert!(
            unk().to_legacy_secret(),
            "Unknown must not become secret=false"
        );
    }

    #[test]
    fn to_legacy_clean_and_labeled_round_trip() {
        assert_eq!(SecurityLabel::Clean.to_legacy_taint(), (false, None));
        assert!(!SecurityLabel::Clean.to_legacy_secret());
        let labeled = lbl("src");
        assert_eq!(labeled.to_legacy_taint(), (true, Some("src".to_string())));
        assert!(lbl_none().to_legacy_secret());
    }

    // ── Phase 8 Slice 1: correspondence observer smoke tests ───────────────
    //
    // These lock the ROW SHAPE and the per-op row counts of the canonical TSV
    // that `observe_correspondence_rows` emits. The mission's byte-for-byte
    // correspondence gate (`scripts/run_security_label_correspondence_gate.sh`)
    // compares the ENTIRE stream against a Lean-emitted stream; these tests
    // guard against a silent shrinkage or reordering of the Rust side alone.

    fn observe_to_string() -> String {
        let mut buf: Vec<u8> = Vec::new();
        super::observe_correspondence_rows(&mut buf)
            .expect("observe_correspondence_rows must not fail on Vec<u8>");
        String::from_utf8(buf).expect("observer output must be valid UTF-8")
    }

    #[test]
    fn observer_emits_declared_row_count() {
        let body = observe_to_string();
        let rows: Vec<&str> = body.lines().collect();
        assert_eq!(
            rows.len(),
            super::DECLARED_ROW_COUNT,
            "observer must emit exactly DECLARED_ROW_COUNT ({}) rows",
            super::DECLARED_ROW_COUNT
        );
    }

    #[test]
    fn observer_per_op_row_counts_match_abstraction() {
        let body = observe_to_string();
        let mut per_op = std::collections::BTreeMap::<&str, usize>::new();
        for row in body.lines() {
            let op = row
                .split('\t')
                .next()
                .expect("row must have a tab-separated op prefix");
            *per_op.entry(op).or_insert(0) += 1;
        }
        assert_eq!(per_op.get("from_legacy_taint").copied().unwrap_or(0), 4);
        assert_eq!(per_op.get("from_legacy_secret").copied().unwrap_or(0), 2);
        assert_eq!(per_op.get("join").copied().unwrap_or(0), 49);
        assert_eq!(per_op.get("declassified_by").copied().unwrap_or(0), 14);
        assert_eq!(per_op.get("to_legacy_taint").copied().unwrap_or(0), 7);
        assert_eq!(per_op.get("to_legacy_secret").copied().unwrap_or(0), 7);
        assert_eq!(
            per_op.len(),
            6,
            "observer must emit exactly six named operations"
        );
    }

    #[test]
    fn observer_row_keys_are_unique() {
        let body = observe_to_string();
        let mut keys: std::collections::BTreeSet<String> = Default::default();
        for row in body.lines() {
            let cols: Vec<&str> = row.splitn(4, '\t').collect();
            assert_eq!(
                cols.len(),
                4,
                "each row must have four tab-separated fields, got {:?}",
                row
            );
            let key = format!("{}|{}|{}", cols[0], cols[1], cols[2]);
            let inserted = keys.insert(key.clone());
            assert!(
                inserted,
                "duplicate (op,arg1,arg2) key `{}` in observer output — the abstract corpus must be a set, not a multiset",
                key
            );
        }
    }

    #[test]
    fn observer_witnesses_load_bearing_rows() {
        // Spot-check that the observed rows are computed from the ACTUAL
        // implementation above, not a shadow reimplementation. Each row here
        // exercises a distinct security-verdict branch and is stated with the
        // exact byte content the Lean model also emits.
        let body = observe_to_string();
        let want = [
            // shape error carve-out from `from_legacy_taint(false, Some(_))`.
            "from_legacy_taint\tfalse\tsome:s1\tUnknown(some:legacy-shape: taint_source without tainted)",
            // Left-bias witness on `join(Labeled{s1}, Labeled{s2})`.
            "join\tLabeled(some:s1)\tLabeled(some:s2)\tLabeled(some:s1)",
            // Fall-back witness on `join(Labeled{None}, Labeled{s2})`.
            "join\tLabeled(none)\tLabeled(some:s2)\tLabeled(some:s2)",
            // Unknown dominance over Clean on `join(Unknown{r1}, Clean)`.
            "join\tUnknown(some:r1)\tClean\tUnknown(some:r1)",
            // Declassify authorized release only from concrete Labeled + policy_ok.
            "declassified_by\tLabeled(some:s1)\ttrue\tClean",
            // Declassify NEVER clears Unknown, even with policy_ok=true.
            "declassified_by\tUnknown(some:r1)\ttrue\tUnknown(some:r1)",
            // Declassify NEVER clears source-less Labeled, even with policy_ok=true.
            "declassified_by\tLabeled(none)\ttrue\tLabeled(none)",
            // Integrity adapter fails closed on Unknown.
            "to_legacy_taint\tUnknown(some:r1)\t-\tLegacy(tainted=true,source=some:unknown-label)",
            // Confidentiality adapter fails closed on Unknown.
            "to_legacy_secret\tUnknown(some:r1)\t-\ttrue",
            // Confidentiality adapter is `false` only for Clean.
            "to_legacy_secret\tClean\t-\tfalse",
        ];
        for row in want.iter() {
            assert!(
                body.lines().any(|line| line == *row),
                "observer output is missing load-bearing row `{}` — either the abstraction shrank or the observer stopped calling the production impl",
                row
            );
        }
    }
}
