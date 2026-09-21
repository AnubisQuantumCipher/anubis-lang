//! `anubis-diagnostics/1` — the machine-readable refusal format.
//!
//! A refusal that only a human can read is a refusal an agent must guess at,
//! and an agent that guesses reaches for the cheapest edit rather than the
//! correct one. This module is the other half of the language's fail-closed
//! discipline: the compiler already refuses honestly, and this is how it says
//! so to a machine without saying anything it cannot stand behind.
//!
//! Four properties are load-bearing, and each is enforced by construction here
//! rather than by review:
//!
//! 1. **The epistemic class is a field, never a severity.** `disproved` (a
//!    concrete counterexample exists) and `undecided` (no proof *and* no
//!    counterexample) are different facts about the world. Every format that
//!    collapses them into one "error" level — SARIF's `level`, an exit code —
//!    invites an agent to "fix" an undecided obligation by weakening the
//!    contract, which is exactly the laundering the language exists to refuse.
//!
//! 2. **A counterexample carries its own trust.** A model the compiler did not
//!    replay is shipped for audit but marked untrustworthy, and a model that
//!    does not assign every variable of its obligation says so in
//!    `model_completeness` and `missing_vars` — because a consumer must never
//!    read an omitted variable as zero.
//!
//! 3. **No suggestions, yet.** The compiler can already print a "possible fix"
//!    that re-fails when applied verbatim, because it never re-proves its own
//!    suggestion. Until a re-proof gate exists, `suggestions` is empty. An
//!    unvalidated suggestion in a machine-readable field is worse than no
//!    field: it converts a heuristic a human would eye into an edit an agent
//!    will apply.
//!
//! 4. **Nothing here names a way to silence a refusal.** No bypass flag, no
//!    confidence score, no suppression key. `suggestions` being empty is a
//!    deliberate absence, not an oversight.
//!
//! The document is emitted as JSON Lines by `anubis check --message-format=json`:
//! one `Diagnostic` per line, then one summary line. That is the shape rustc
//! and Dafny both use, and it lets a consumer act on the first refusal without
//! waiting for the run to finish.

use crate::middle::{classify_assertion_fail, AssertionFailKind, SolverCheck};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "anubis-diagnostics/1";

/// What kind of obligation failed. Distinct from [`Status`], which is what the
/// compiler managed to establish about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// A `requires` / `ensures` / `invariant` obligation.
    Contract,
    /// An arithmetic overflow obligation the compiler synthesised.
    WrapSafety,
    /// The solver disagreed with itself or declined to be trusted. Not a
    /// program defect.
    SolverTrust,
    /// A refusal from before the solver ran: a parse error, a type error, an
    /// undeclared effect, a taint violation. Structured only to the depth the
    /// compiler currently carries, which for now is the code and the message.
    Frontend,
}

/// What the compiler established. This is the field that must never be folded
/// into a severity: the repair for `disproved` and the repair for `undecided`
/// are different, and confusing them silences real refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// A concrete counterexample exists. The obligation is FALSE.
    Disproved,
    /// Neither proved nor disproved within the declared budget. The obligation
    /// may still be true.
    Undecided,
    /// The solver's answer could not be replayed. A soundness alarm.
    ReplayMismatch,
    /// Refused for a reason that is neither of the above.
    Refused,
}

/// Where the defect is. An agent that repairs source in response to a compiler
/// bug makes things worse, so this is explicit rather than implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectLocus {
    /// The program is wrong.
    Program,
    /// The compiler or solver is wrong. Do not edit the program.
    Compiler,
    /// The obligation is beyond what this compiler can decide. Neither is
    /// "wrong"; the obligation needs restating or the budget raising.
    Capability,
}

/// The single next step, named so an agent does not have to infer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAction {
    /// The counterexample shows a real violation: fix the body, or fix the
    /// contract if the contract is what is wrong. Requires judgement about
    /// which — the compiler cannot know the author's intent.
    RepairProgram,
    /// Restate the obligation so it falls in a decidable fragment, or raise the
    /// declared budget. Weakening the contract to make this pass is laundering.
    RestateOrRaiseBudget,
    /// Report this. Do not edit the program.
    InvestigateCompiler,
}

/// One variable assignment from a counterexample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    /// The variable as it appears in the source, with the compiler's `anb_`
    /// prefix removed.
    pub var: String,
    /// The raw SMT term, verbatim.
    pub smt_value: String,
    /// Signed 64-bit interpretation, when the term is a bitvector literal.
    /// Absent for sorts this does not apply to, never guessed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimal: Option<String>,
}

/// A concrete assignment that falsifies an obligation.
///
/// This is the asset no comparable toolchain ships: rustc has no counterexample
/// to give, and SARIF has no field to put one in. It is also the field most
/// capable of misleading a consumer, which is why it carries its own trust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counterexample {
    /// True only when the compiler re-checked this model against the
    /// obligation. A model that was not replayed is still shipped, for audit,
    /// but must not be repaired against.
    pub trustworthy: bool,
    /// `complete` when every variable the obligation declares is assigned.
    pub model_completeness: Completeness,
    /// Variables the obligation declares that the model did not assign. A
    /// consumer must not read these as zero: the solver left them free because
    /// any value works, which is a different statement.
    pub missing_vars: Vec<String>,
    pub assignments: Vec<Assignment>,
    /// The solver's output verbatim, so a reader can check this parse.
    pub raw_model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Complete,
    Partial,
}

/// Where in the source a refusal points.
///
/// Only ever the compiler's own structured span, converted to 1-based line and
/// column. A refusal whose origin the compiler does not track leaves this
/// absent, because an approximate location sends a reader — or an agent — to
/// edit the wrong line with full confidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// Byte offsets of the span, for a consumer that wants the exact extent.
    pub span_start: usize,
    pub span_end: usize,
}

/// What the obligation was, in both the solver's language and (where available)
/// the author's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    /// The compiler's name for it, e.g. `ensures:(bvsge ...)`.
    pub name: String,
    /// The full SMT-LIB2 query, verbatim.
    pub smt: String,
    /// Variables the query declares.
    pub declared_vars: Vec<String>,
}

/// The resource budget the obligation was decided under.
///
/// `measured` is false until the compiler issues `(get-info :all-statistics)`,
/// and while it is false `consumed` is null rather than estimated. An invented
/// figure would let an agent conclude "raise the budget" about an obligation no
/// budget can decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Always the deterministic resource counter, never a clock. Diagnostic
    /// text must not call this a timeout: that vocabulary tells an agent to
    /// retry when the correct move may be to restate the obligation.
    pub metric: String,
    pub limit: u64,
    pub measured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed: Option<u64>,
}

impl Budget {
    /// The declared z3 bound, unmeasured. Mirrors `Z3_ARGS` in `middle`.
    pub fn declared() -> Budget {
        Budget {
            metric: "z3-rlimit".into(),
            limit: 200_000_000,
            measured: false,
            consumed: None,
        }
    }
}

/// One refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// JSONL discriminator, so a consumer can tell a diagnostic from a summary
    /// without positional assumptions.
    #[serde(rename = "$type")]
    pub type_tag: String,
    pub schema: String,
    /// The stable `ANUBIS_*` code. Per finding, never a batch headline: a batch
    /// of two disproofs and one undecided has three codes, not one.
    pub code: String,
    pub family: Family,
    pub status: Status,
    pub defect_locus: DefectLocus,
    pub agent_action: AgentAction,
    /// Always `error` for a refusal. There is no lesser severity, because there
    /// is no way to proceed past one.
    pub severity: String,
    pub build_blocking: bool,
    /// Human-readable, and never the carrier of a fact no structured field
    /// holds.
    pub message: String,
    /// Absent for a refusal raised before any obligation was formed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation: Option<Obligation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterexample: Option<Counterexample>,
    /// Absent when the compiler does not track where this refusal came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Absent when no solver ran. Reporting a budget against a parse error
    /// would imply work was spent deciding something, and invite the reader to
    /// raise a bound that had nothing to do with the refusal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    /// Empty until the compiler re-proves its own suggestions. See the module
    /// documentation: this absence is deliberate.
    pub suggestions: Vec<String>,
}

/// The trailing line of the stream: the verdict and the counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    #[serde(rename = "$type")]
    pub type_tag: String,
    pub schema: String,
    /// `fail` iff any diagnostic is build-blocking. Anubis fails closed: the
    /// only pass is an empty refusal set.
    pub verdict: String,
    pub counts: Counts,
    /// Absent when nothing was discharged — a parse failure proves nothing, and
    /// a zero here would read as "nothing was witnessed" rather than "nothing
    /// was attempted".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
}

/// How much of a passing verdict rests on a witness rather than the solver's word.
///
/// Present on every summary where anything was discharged, including a `pass`.
/// This is the point: without it a `pass` says the same thing whether every
/// obligation carried a machine-checkable refutation or none did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub certified: usize,
    pub trusted_to_solver: usize,
    /// `certified + trusted_to_solver`.
    pub discharged: usize,
    /// The obligations resting on the solver's word, named rather than counted.
    pub uncertified: Vec<String>,
}

impl From<&crate::middle::CertificateCoverage> for Coverage {
    fn from(c: &crate::middle::CertificateCoverage) -> Coverage {
        Coverage {
            certified: c.certified,
            trusted_to_solver: c.trusted_to_solver,
            discharged: c.discharged,
            uncertified: c.uncertified.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub total: usize,
    pub disproved: usize,
    pub undecided: usize,
    pub replay_mismatch: usize,
    pub refused: usize,
}

/// The leading `ANUBIS_*` code of a refusal message, if it carries one.
///
/// Extraction rather than invention: the compiler already prefixes its refusals
/// with a stable code, and lifting it into a field is the whole point of this
/// format. A message with no code gets the generic one instead of a guess.
fn leading_code(message: &str) -> String {
    let head = message.split(':').next().unwrap_or("");
    let head = head.trim();
    if head.starts_with("ANUBIS_")
        && head.len() > "ANUBIS_".len()
        && head
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        head.to_string()
    } else {
        "ANUBIS_CHECK_FAILED".to_string()
    }
}

/// A refusal the solver lane did not produce: parse, type, effect, taint.
///
/// This exists so the stream cannot report `pass` on a run that failed. A
/// consumer reading only solver diagnostics would see an empty finding list for
/// a program that never reached the solver, and an empty finding list is the
/// one thing a verdict format must never say about a failure.
pub fn diagnostic_of_refusal(message: &str) -> Diagnostic {
    Diagnostic {
        type_tag: "anubis.diagnostic".into(),
        schema: SCHEMA.into(),
        code: leading_code(message),
        family: Family::Frontend,
        status: Status::Refused,
        defect_locus: DefectLocus::Program,
        agent_action: AgentAction::RepairProgram,
        severity: "error".into(),
        build_blocking: true,
        message: message.to_string(),
        obligation: None,
        counterexample: None,
        location: None,
        budget: None,
        suggestions: Vec::new(),
    }
}

/// One structured diagnostic per parse error, each carrying a real location.
///
/// Built from `parse_source_detailed`, the same structured diagnostics the
/// human renderer draws its carets from — not by reading back the rendered
/// text. A format that re-parsed its own prose would drift the first time the
/// caret rendering changed.
pub fn diagnostics_of_parse_errors(source: &str, path: &str) -> Vec<Diagnostic> {
    crate::frontend::parse_source_detailed(source)
        .diagnostics
        .iter()
        .map(|d| {
            let (line, column) = crate::frontend::line_col(source, d.span.start);
            Diagnostic {
                type_tag: "anubis.diagnostic".into(),
                schema: SCHEMA.into(),
                code: "ANUBIS_PARSE_ERROR".into(),
                family: Family::Frontend,
                status: Status::Refused,
                defect_locus: DefectLocus::Program,
                agent_action: AgentAction::RepairProgram,
                severity: "error".into(),
                build_blocking: true,
                message: d.message.clone(),
                obligation: None,
                counterexample: None,
                location: Some(Location {
                    file: path.to_string(),
                    line,
                    column,
                    span_start: d.span.start,
                    span_end: d.span.end,
                }),
                budget: None,
                suggestions: Vec::new(),
            }
        })
        .collect()
}

/// Strip the compiler's variable prefix so a name matches the source.
fn source_name(smt_name: &str) -> String {
    smt_name.strip_prefix("anb_").unwrap_or(smt_name).to_string()
}

/// Every `(declare-const NAME ...)` in a query.
fn declared_vars(smt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for rest in smt.split("(declare-const ").skip(1) {
        if let Some(name) = rest.split_whitespace().next() {
            let n = source_name(name);
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// Signed 64-bit value of a bitvector literal, in either `#x…` or `(_ bvN 64)`
/// form. `None` for anything else — never a guess.
fn decimal_of(smt_value: &str) -> Option<String> {
    let v = smt_value.trim();
    if let Some(hex) = v.strip_prefix("#x") {
        let raw = u64::from_str_radix(hex, 16).ok()?;
        return Some((raw as i64).to_string());
    }
    if let Some(inner) = v.strip_prefix("(_ bv").and_then(|r| r.split_whitespace().next()) {
        let raw: u64 = inner.parse().ok()?;
        return Some((raw as i64).to_string());
    }
    None
}

/// Build the counterexample record, including the parts that limit it.
fn counterexample_of(
    model: &str,
    declared: &[String],
    bindings: &BTreeMap<String, String>,
    trustworthy: bool,
) -> Counterexample {
    let mut assignments: Vec<Assignment> = bindings
        .iter()
        .map(|(name, value)| Assignment {
            var: source_name(name),
            smt_value: value.clone(),
            decimal: decimal_of(value),
        })
        .collect();
    assignments.sort_by(|a, b| a.var.cmp(&b.var));

    let assigned: Vec<String> = assignments.iter().map(|a| a.var.clone()).collect();
    let missing_vars: Vec<String> = declared
        .iter()
        .filter(|v| !assigned.contains(v))
        .cloned()
        .collect();

    Counterexample {
        trustworthy,
        model_completeness: if missing_vars.is_empty() {
            Completeness::Complete
        } else {
            Completeness::Partial
        },
        missing_vars,
        assignments,
        raw_model: model.to_string(),
    }
}

/// Classify one failed check into the structured fields.
///
/// The classification is taken from [`classify_assertion_fail`], which reads
/// the check's own shape. Where that function still infers from prose, the
/// inference is inherited rather than repeated: one classifier is a bug that
/// can be fixed in one place, three are a drift surface.
fn classify(check: &SolverCheck) -> (String, Family, Status, DefectLocus, AgentAction) {
    match classify_assertion_fail(check) {
        AssertionFailKind::Disproved => (
            "ANUBIS_ASSERTION_DISPROVED".into(),
            Family::Contract,
            Status::Disproved,
            DefectLocus::Program,
            AgentAction::RepairProgram,
        ),
        AssertionFailKind::WrapRisk => (
            "ANUBIS_WRAP_RISK".into(),
            Family::WrapSafety,
            Status::Disproved,
            DefectLocus::Program,
            AgentAction::RepairProgram,
        ),
        AssertionFailKind::Undecided => (
            "ANUBIS_ASSERTION_UNDECIDED".into(),
            Family::Contract,
            Status::Undecided,
            DefectLocus::Capability,
            AgentAction::RestateOrRaiseBudget,
        ),
        AssertionFailKind::ReplayMismatch => (
            "ANUBIS_REPLAY_MISMATCH".into(),
            Family::SolverTrust,
            Status::ReplayMismatch,
            DefectLocus::Compiler,
            AgentAction::InvestigateCompiler,
        ),
        AssertionFailKind::Other => (
            "ANUBIS_ASSERTION_UNPROVEN".into(),
            Family::Contract,
            Status::Refused,
            DefectLocus::Capability,
            AgentAction::RestateOrRaiseBudget,
        ),
    }
}

/// Convert one failed solver check into a diagnostic.
pub fn diagnostic_of(check: &SolverCheck) -> Diagnostic {
    let (code, family, status, defect_locus, agent_action) = classify(check);
    let declared = declared_vars(&check.smt);
    let counterexample = check.model.as_ref().map(|m| {
        // Asked of `middle`, not inferred here: it is the single authority on
        // whether a model was independently verified, and both the human
        // printer and this format must give the same answer.
        let replayed = crate::middle::counterexample_was_replayed(check);
        counterexample_of(m, &declared, &crate::middle::counterexample_bindings(m), replayed)
    });

    Diagnostic {
        type_tag: "anubis.diagnostic".into(),
        schema: SCHEMA.into(),
        code,
        family,
        status,
        defect_locus,
        agent_action,
        severity: "error".into(),
        build_blocking: true,
        message: check.detail.clone(),
        obligation: Some(Obligation {
            name: check.name.clone(),
            smt: check.smt.clone(),
            declared_vars: declared,
        }),
        counterexample,
        location: None,
        budget: Some(Budget::declared()),
        suggestions: Vec::new(),
    }
}

/// The whole refusal set, as JSON Lines: one diagnostic per line, then a
/// summary. A consumer can act on the first line without waiting for the last.
///
/// `other_refusal` carries a failure that did not come from the solver lane —
/// a parse error, a type error, an undeclared effect. It is emitted only when
/// the solver produced no failures of its own, because when it did, that string
/// is just those same failures already rendered for a human, and reporting both
/// would double-count one defect.
///
/// The invariant this function exists to hold: **the verdict is `fail` whenever
/// the check failed.** Every other field can be incomplete; this one cannot be
/// wrong, because a consumer that trusts a false `pass` ships the program.
pub fn render_jsonl_with(checks: &[SolverCheck], other_refusal: Option<&str>) -> String {
    let fails: Vec<&SolverCheck> = checks.iter().filter(|c| c.status == "FAIL").collect();
    let mut diagnostics: Vec<Diagnostic> = fails.iter().map(|c| diagnostic_of(c)).collect();
    if diagnostics.is_empty() {
        if let Some(message) = other_refusal {
            diagnostics.push(diagnostic_of_refusal(message));
        }
    }
    render(&diagnostics)
}

/// Serialize an assembled refusal set as JSON Lines.
///
/// The counts are derived here rather than passed in, so a caller cannot report
/// a total that disagrees with the diagnostics it shipped.
pub fn render(diagnostics: &[Diagnostic]) -> String {
    render_with_coverage(diagnostics, None)
}

/// As [`render`], and states how much of the verdict carries a witness.
pub fn render_with_coverage(diagnostics: &[Diagnostic], coverage: Option<Coverage>) -> String {
    let mut counts = Counts {
        total: diagnostics.len(),
        disproved: 0,
        undecided: 0,
        replay_mismatch: 0,
        refused: 0,
    };
    for d in diagnostics {
        match d.status {
            Status::Disproved => counts.disproved += 1,
            Status::Undecided => counts.undecided += 1,
            Status::ReplayMismatch => counts.replay_mismatch += 1,
            Status::Refused => counts.refused += 1,
        }
    }

    let mut out = String::new();
    for d in diagnostics {
        out.push_str(&serde_json::to_string(d).expect("diagnostic serializes"));
        out.push('\n');
    }
    let summary = Summary {
        type_tag: "anubis.summary".into(),
        schema: SCHEMA.into(),
        verdict: if diagnostics.is_empty() { "pass" } else { "fail" }.into(),
        counts,
        coverage: coverage.filter(|c| c.discharged > 0),
    };
    out.push_str(&serde_json::to_string(&summary).expect("summary serializes"));
    out.push('\n');
    out
}

/// Solver-lane only. Kept for callers that have no other failure to report.
pub fn render_jsonl(checks: &[SolverCheck]) -> String {
    render_jsonl_with(checks, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disproved_check() -> SolverCheck {
        SolverCheck {
            name: "ensures:(bvsge (bvsub anb_a anb_b) (_ bv0 64))".into(),
            status: "FAIL".into(),
            detail: crate::middle::DISPROVED_DETAIL_Z3.into(),
            model: Some(
                "sat\n(\n  (define-fun anb_a () (_ BitVec 64)\n    #x0000000000000001)\n  \
                 (define-fun anb_b () (_ BitVec 64)\n    #x0000000000000003)\n)\n"
                    .into(),
            ),
            smt: "(set-logic QF_BV)\n(declare-const anb_a (_ BitVec 64))\n\
                  (declare-const anb_b (_ BitVec 64))\n(check-sat)\n"
                .into(),
        }
    }

    #[test]
    fn a_disproof_carries_its_counterexample_in_source_names() {
        let d = diagnostic_of(&disproved_check());
        assert_eq!(d.status, Status::Disproved);
        assert_eq!(d.defect_locus, DefectLocus::Program);
        let ce = d.counterexample.expect("a disproof has a model");
        assert_eq!(ce.model_completeness, Completeness::Complete);
        let vars: Vec<&str> = ce.assignments.iter().map(|a| a.var.as_str()).collect();
        assert_eq!(vars, vec!["a", "b"], "the anb_ prefix must not reach a consumer");
        assert_eq!(ce.assignments[0].decimal.as_deref(), Some("1"));
        assert_eq!(ce.assignments[1].decimal.as_deref(), Some("3"));
    }

    #[test]
    fn a_partial_model_says_so_and_names_what_is_missing() {
        // Observed in the wild on wrap-safety obligations: the solver assigns
        // one variable and leaves the other free. A consumer must not read the
        // omission as zero.
        let mut c = disproved_check();
        c.model = Some(
            "sat\n(\n  (define-fun anb_b () (_ BitVec 64)\n    #x0000000000000000)\n)\n".into(),
        );
        let ce = diagnostic_of(&c).counterexample.expect("model present");
        assert_eq!(ce.model_completeness, Completeness::Partial);
        assert_eq!(ce.missing_vars, vec!["a"]);
    }

    #[test]
    fn a_native_counterexample_is_trusted_too() {
        // Regression: `trustworthy` was once decided by searching the detail for
        // the word "replayed". The native lane independently re-evaluates its
        // model but says so in different words, so a sound counterexample came
        // back untrustworthy — the safe direction, but it made the native lane
        // look weaker than the z3 lane purely because of its prose.
        let mut c = disproved_check();
        c.detail = crate::middle::DISPROVED_DETAIL_NATIVE.into();
        let ce = diagnostic_of(&c).counterexample.expect("model present");
        assert!(ce.trustworthy, "an independently re-evaluated model is trustworthy");
    }

    #[test]
    fn an_unreplayed_model_is_shipped_but_not_trusted() {
        let mut c = disproved_check();
        c.detail = "counterexample found".into(); // no "replayed"
        let ce = diagnostic_of(&c).counterexample.expect("model present");
        assert!(!ce.trustworthy, "a model the compiler did not replay is not repairable against");
        assert!(!ce.raw_model.is_empty(), "but it is still shipped for audit");
    }

    #[test]
    fn undecided_is_not_disproved_and_points_somewhere_different() {
        let c = SolverCheck {
            name: "ensures:(bvsge anb_x (_ bv0 64))".into(),
            status: "FAIL".into(),
            detail: "solver returned `unknown` within the declared budget".into(),
            model: None,
            smt: "(declare-const anb_x (_ BitVec 64))".into(),
        };
        let d = diagnostic_of(&c);
        assert_eq!(d.status, Status::Undecided);
        assert_eq!(d.defect_locus, DefectLocus::Capability);
        assert_eq!(d.agent_action, AgentAction::RestateOrRaiseBudget);
        assert!(d.counterexample.is_none());
        // The whole point: an agent must not treat this like a disproof and
        // weaken the contract to make it pass.
        assert_ne!(d.status, Status::Disproved);
    }

    #[test]
    fn a_solver_disagreement_never_asks_for_a_program_edit() {
        let c = SolverCheck {
            name: "ensures:(bvsge anb_x (_ bv0 64))".into(),
            status: "FAIL".into(),
            detail: "ANUBIS_REPLAY_MISMATCH: native and z3 disagree".into(),
            model: None,
            smt: "(declare-const anb_x (_ BitVec 64))".into(),
        };
        let d = diagnostic_of(&c);
        assert_eq!(d.defect_locus, DefectLocus::Compiler);
        assert_eq!(d.agent_action, AgentAction::InvestigateCompiler);
    }

    #[test]
    fn every_diagnostic_is_blocking_and_carries_no_lesser_severity() {
        for c in [disproved_check()] {
            let d = diagnostic_of(&c);
            assert_eq!(d.severity, "error");
            assert!(d.build_blocking);
        }
    }

    #[test]
    fn no_diagnostic_names_a_way_to_silence_itself() {
        let json = render_jsonl(&[disproved_check()]).to_lowercase();
        for forbidden in ["no-verify", "no_verify", "suppress", "confidence", "can_ignore"] {
            assert!(!json.contains(forbidden), "diagnostic JSON named {forbidden:?}");
        }
    }

    #[test]
    fn suggestions_are_empty_until_the_compiler_reproves_them() {
        // The compiler's current "possible fix" re-fails when applied verbatim,
        // because it never re-proves its own suggestion. Shipping that in a
        // machine-readable field would make agent loops worse, not better.
        assert!(diagnostic_of(&disproved_check()).suggestions.is_empty());
    }

    #[test]
    fn the_stream_is_one_object_per_line_then_a_summary() {
        let out = render_jsonl(&[disproved_check()]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        let d: serde_json::Value = serde_json::from_str(lines[0]).expect("line 0 is json");
        assert_eq!(d["$type"], "anubis.diagnostic");
        assert_eq!(d["schema"], SCHEMA);
        let s: serde_json::Value = serde_json::from_str(lines[1]).expect("line 1 is json");
        assert_eq!(s["$type"], "anubis.summary");
        assert_eq!(s["verdict"], "fail");
        assert_eq!(s["counts"]["disproved"], 1);
    }

    #[test]
    fn a_clean_run_is_a_pass_with_no_diagnostics() {
        let ok = SolverCheck {
            name: "ensures:x".into(),
            status: "PASS".into(),
            detail: "proved".into(),
            model: None,
            smt: String::new(),
        };
        let out = render_jsonl(&[ok]);
        assert_eq!(out.lines().count(), 1);
        let s: serde_json::Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
        assert_eq!(s["verdict"], "pass");
    }

    #[test]
    fn a_heterogeneous_batch_keeps_one_code_per_finding() {
        // The failure this prevents: a batch of a disproof and an undecided
        // printing under one headline code, which is strictly less informative
        // than the findings it introduces.
        let undecided = SolverCheck {
            name: "ensures:u".into(),
            status: "FAIL".into(),
            detail: "solver returned `unknown`".into(),
            model: None,
            smt: String::new(),
        };
        let out = render_jsonl(&[disproved_check(), undecided]);
        assert!(out.contains("ANUBIS_ASSERTION_DISPROVED"));
        assert!(out.contains("ANUBIS_ASSERTION_UNDECIDED"));
    }

    #[test]
    fn decimal_reads_both_literal_forms_and_refuses_anything_else() {
        assert_eq!(decimal_of("#x0000000000000003").as_deref(), Some("3"));
        assert_eq!(decimal_of("(_ bv3 64)").as_deref(), Some("3"));
        assert_eq!(
            decimal_of("#xffffffffffffffff").as_deref(),
            Some("-1"),
            "bitvectors are read as signed, matching the source types"
        );
        assert_eq!(decimal_of("(fp #b0 #b100 #b00)"), None, "never guess a float");
    }

    #[test]
    fn a_refusal_from_before_the_solver_still_reports_fail() {
        // The failure this prevents is the worst one this format can have: a
        // program that never reached the solver has no solver findings, and a
        // stream that therefore said `pass` would tell an agent the program
        // was accepted when the compiler refused it.
        let out = render_jsonl_with(
            &[],
            Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE: safe mode file_write forbidden without `uses(fs.write)`"),
        );
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        let d: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(d["code"], "ANUBIS_EFFECT_FORBIDDEN_IN_MODE");
        assert_eq!(d["family"], "frontend");
        assert_eq!(d["build_blocking"], true);
        assert!(d.get("obligation").is_none(), "a frontend refusal has no obligation");
        let s: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(s["verdict"], "fail");
    }

    #[test]
    fn one_defect_is_not_counted_twice() {
        // `check_error` for a solver failure is just those same checks rendered
        // for a human. Emitting both would report two defects where there is one.
        let human = "ANUBIS_ASSERTION_DISPROVED: ensures does not hold";
        let out = render_jsonl_with(&[disproved_check()], Some(human));
        assert_eq!(out.lines().count(), 2, "one diagnostic, one summary");
        let s: serde_json::Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
        assert_eq!(s["counts"]["total"], 1);
    }

    #[test]
    fn a_code_is_extracted_never_invented() {
        assert_eq!(leading_code("ANUBIS_PARSE_ERROR: bad token"), "ANUBIS_PARSE_ERROR");
        assert_eq!(
            leading_code("parse failed"),
            "ANUBIS_CHECK_FAILED",
            "a message with no code gets the generic one rather than a guess"
        );
        assert_eq!(
            leading_code("anubis_lowercase: x"),
            "ANUBIS_CHECK_FAILED",
            "only the compiler's own uppercase code shape is lifted"
        );
    }

    #[test]
    fn a_parse_error_points_at_a_real_line_and_column() {
        let src = "fn main() {\n    let x = ;\n";
        let ds = diagnostics_of_parse_errors(src, "t.anb");
        assert!(!ds.is_empty(), "a broken parse must produce at least one diagnostic");
        let loc = ds[0].location.as_ref().expect("a parse error knows where it is");
        assert_eq!(loc.file, "t.anb");
        assert!(loc.line >= 1 && loc.column >= 1, "1-based, not 0-based: {loc:?}");
        assert!(loc.span_end >= loc.span_start);
        assert_eq!(ds[0].code, "ANUBIS_PARSE_ERROR");
    }

    #[test]
    fn a_clean_parse_produces_no_parse_diagnostics() {
        assert!(diagnostics_of_parse_errors("fn main() { }\n", "t.anb").is_empty());
    }

    #[test]
    fn no_budget_is_reported_where_no_solver_ran() {
        // Reporting the z3 bound against a parse error would imply work was
        // spent deciding something, and invite a reader to raise a bound that
        // had nothing to do with the refusal.
        for d in diagnostics_of_parse_errors("fn main() {\n let x = ;\n", "t.anb") {
            assert!(d.budget.is_none(), "parse errors carry no budget");
        }
        assert!(diagnostic_of_refusal("ANUBIS_X: y").budget.is_none());
        assert!(
            diagnostic_of(&disproved_check()).budget.is_some(),
            "a solver verdict does carry the bound it was decided under"
        );
    }

    #[test]
    fn counts_cannot_disagree_with_the_diagnostics_shipped() {
        let ds = vec![diagnostic_of(&disproved_check()), diagnostic_of_refusal("ANUBIS_X: y")];
        let out = render(&ds);
        let summary: serde_json::Value =
            serde_json::from_str(out.lines().last().unwrap()).unwrap();
        assert_eq!(summary["counts"]["total"], 2);
        assert_eq!(summary["counts"]["disproved"], 1);
        assert_eq!(summary["counts"]["refused"], 1);
        assert_eq!(out.lines().count(), 3, "two diagnostics plus one summary");
    }

    #[test]
    fn a_pass_is_only_ever_an_empty_refusal_set() {
        // Anubis fails closed. There is no verdict between pass and fail, and
        // no diagnostic that a consumer may proceed past.
        assert!(render_jsonl_with(&[], None).contains("\"verdict\":\"pass\""));
        for refusal in ["ANUBIS_X: y", "parse failed", ""] {
            let out = render_jsonl_with(&[], Some(refusal));
            assert!(out.contains("\"verdict\":\"fail\""), "refusal {refusal:?} reported pass");
        }
    }

    #[test]
    fn budget_is_a_resource_counter_and_never_called_a_timeout() {
        let b = Budget::declared();
        assert_eq!(b.metric, "z3-rlimit");
        assert!(!b.measured, "the compiler does not yet ask z3 what it spent");
        assert!(b.consumed.is_none(), "and so must not report a figure");
    }
}
