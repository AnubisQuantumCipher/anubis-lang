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
//! Slice 2 introduces the type, its constructors, lattice `join`, and legacy
//! adapters from the historical `(bool, Option<String>)` and `bool`-only
//! shapes. **No caller migrates in this slice.** Slices 3-5 do the migration
//! in the recommended dependency order (root transfer → path/carrier →
//! terminal enforcement) so that this slice cannot flip any fixture verdict.
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
/// This is `pub(crate)` because Slice 2 deliberately introduces the type
/// without wiring it into any caller. Slices 3-5 migrate the existing
/// boolean/`Option` sites into this domain; only after Phase 3 close is the
/// visibility revisited.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[allow(dead_code)] // Slice 2: introduced; Slice 3+ migrate callers.
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

#[allow(dead_code)] // Slice 2: introduced; Slice 3+ migrate callers.
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
}
