//! Anubis native SMT decision procedure — a from-scratch QF_BV solver that replaces the z3 third
//! party for the integer contract lane. ZERO external solver dependency (std only).
//!
//! **Unsat fail-closed:** every public path that returns a native Unsat does so only after
//! [`lrat::check_proof`] accepts a self-contained RUP certificate from CDCL. Missing or invalid
//! cert → `None` (defer). **Sat fail-closed:** models are re-checked by independent
//! [`bv::Formula::eval`] replay.
//!
//! **Authority (product default flip 2026-07-25):** the compiler uses
//! [`native_check_sat_model_authoritative`] by default (proven fragment + cert/replay). z3 remains
//! a fail-closed cross-check when present. Opt out with `ANUBIS_NATIVE_AUTHORITATIVE=0`.
//! Division stays deferred by design. Variable×variable multiply is IN the proven fragment
//! (`mulVar_correct`, admitted 2026-07-25) — the older "var×var mul stays deferred" note here was
//! stale — and since the structural-hashing rewrite of 2026-07-26 it actually DECIDES rather than
//! grinding; only an irreducible product miter now falls out to z3 on budget.
//!
//! Pipeline: SMT-LIB2 text → `bv::Formula` (parse) → pre-blast cost gate → CNF (bit-blast) →
//! `sat::Cnf::solve_limited` → RUP cert check on Unsat / model replay on Sat → verdict.
//!
//! **Termination (2026-07-26).** Every public entry point is bounded in four independent places, so no
//! obligation can make the checker grind:
//! 1. [`blast::gate_cost`] over the parsed formula, against [`MAX_BLAST_GATES`] — declines before a
//!    single CNF byte is allocated;
//! 2. [`MAX_CNF_CLAUSES`] enforced by the `Cnf` itself while blasting — a deterministic backstop in
//!    case the estimate ever under-counts;
//! 3. [`DEFAULT_BUDGET`] conflicts (deterministic, the bound that fires in practice) plus a wall-clock
//!    net ([`DEFAULT_TIME_BUDGET_MS`]) for the case a single conflict is itself expensive;
//! 4. [`MAX_CERT_WORK`] on RUP certificate checking — without which the grind simply MOVES from the
//!    search into [`lrat::check_proof`], whose cost grows ~quadratically in conflicts.
//!
//! All four can only turn `Some(verdict)` into `None`, so they defer to z3 exactly the way division
//! already does and cannot change a verdict native would otherwise have returned.

pub mod blast;
pub mod bv;
pub mod fp;
pub mod fragment;
pub mod lrat;
pub mod parse;
pub mod sat;

use lrat::check_proof;
use sat::{Cnf, SatResult, SolveLimits};
use std::time::{Duration, Instant};

/// Default decision budget (max CDCL conflicts). Sized so easy obligations decide fast and hard ones
/// return `None` (defer to z3) rather than grind. Tune with the differential gate; measure with
/// `ANUBIS_NATIVE_STATS_LOG`.
///
/// **Why 20_000 (was 2_000_000 until 2026-07-26):** the old value was not a working bound. On the
/// 168,643-clause multiplier miter that `ensures(result == a * b)` produced, 100,000 conflicts already
/// cost 15.3 s and still returned `Unknown`; 2,000,000 was minutes-to-hours of 100 %-CPU grind, which
/// is a hang for every practical purpose (`anubis check` on `import std.math` never returned).
///
/// The new value is measured, not guessed. A sweep of all 613 `.anb` files under `examples/` +
/// `tests/fixtures/` with the budget raised to 400,000 and `ANUBIS_NATIVE_STATS_LOG` on produced 2,543
/// native obligations, of which **every single one was decided** (1,325 unsat / 1,218 sat, zero
/// deferrals) consuming at most **999 conflicts** — median 0, p99 128. So 20,000 keeps a 20× margin
/// over the worst real obligation and loses no decision anywhere in the corpus, while bounding the
/// pathological case to ~1.5 s (measured on the commutativity miter `a*b == b*a`).
///
/// Re-measure with `ANUBIS_NATIVE_STATS_LOG` before changing this. Overridable at runtime with
/// `ANUBIS_NATIVE_CONFLICT_BUDGET`.
pub const DEFAULT_BUDGET: u64 = 20_000;

/// Ceiling on the gates a formula may blast to ([`blast::gate_cost`]), checked BEFORE blasting.
///
/// This bound is about MEMORY and construction work, not search: a probe measured at 344k estimated
/// gates built 559,431 clauses in 16 ms, so the ceiling sits at tens of MB and a few tens of ms of
/// construction. Search is bounded separately by [`DEFAULT_BUDGET`]. Nested wide multiplies
/// (`((a*b)*c)*d` at 64 bits costs 6·64² per product) are what make this necessary — without it a
/// single obligation can ask for a multi-hundred-megabyte CNF before any budget is consulted.
///
/// The corpus sweep's largest real obligation was 83,780 clauses (~25k gates), so this leaves ≳16×
/// headroom and declined nothing across 2,543 obligations. Overridable with
/// `ANUBIS_NATIVE_GATE_CEILING`.
pub const MAX_BLAST_GATES: u64 = 400_000;

/// Hard clause ceiling enforced by the `Cnf` while blasting — the deterministic backstop for
/// [`MAX_BLAST_GATES`]. Set above the clause count [`MAX_BLAST_GATES`] implies (~4 clauses/gate) so it
/// only ever fires if the pre-blast estimate under-counts, which would be a bug in
/// [`blast::gate_cost`], not a property of the input.
pub const MAX_CNF_CLAUSES: usize = 2_000_000;

/// Ceiling on RUP certificate-checking work, as `steps × (original_clauses + steps)` — the clause
/// visits one full propagation pass over the growing database costs, summed across steps.
///
/// **Why this exists as its own bound.** Bounding the SEARCH is not enough: on `Unsat` the CDCL engine
/// emits one RUP step per conflict, and [`lrat::check_proof`] re-derives every one of them with a
/// deliberately naive round-based propagation (O(rounds × database) per step — see that module's
/// "Residual limitations"). Its cost therefore grows ~quadratically in conflicts and *dominates* the
/// solve it verifies. Measured on pigeonhole n+1→n:
///
/// | steps  | solve  | check_proof |
/// |--------|--------|-------------|
/// | 162    | 0.25ms | 1.2ms       |
/// | 953    | 6.6ms  | 41.7ms      |
/// | 4,105  | 43ms   | 672ms       |
/// | 42,676 | 2.9s   | **78.1s**   |
///
/// So a conflict budget alone just relocates the grind from the search to the checker. The right
/// response is to keep the checker simple — its smallness IS its trust argument — and enforce the
/// "contract-scale CNF" precondition it documents. Declining to CHECK an oversized certificate is a
/// deferral, never an unverified `Unsat`: the fail-closed rule (no `Unsat` without an accepted cert)
/// is untouched.
///
/// 50M admits every obligation in the corpus sweep with ≥7× margin (worst real case: 920 steps over
/// 6,195 clauses ≈ 6.5M) while rejecting the 1.83G-work pigeonhole case above by ~36×. Overridable
/// with `ANUBIS_NATIVE_CERT_WORK`.
pub const MAX_CERT_WORK: u64 = 50_000_000;

/// Wall-clock net per obligation. NOT the primary bound — [`DEFAULT_BUDGET`] is, precisely because it
/// is deterministic (a verdict must not depend on machine load). This exists only for the shape the
/// conflict budget cannot bound: a very large CNF where each conflict is cheap but propagation is
/// slow. Overridable with `ANUBIS_NATIVE_TIME_BUDGET_MS`; `0` disables it (fully deterministic).
pub const DEFAULT_TIME_BUDGET_MS: u64 = 10_000;

/// Read a `u64` env override, falling back to `default` when unset or unparseable.
fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

/// The bounds one native decision runs under.
#[derive(Debug, Clone)]
struct NativeLimits {
    gate_ceiling: u64,
    clause_ceiling: usize,
    conflicts: u64,
    cert_work: u64,
    /// `None` ⇒ no wall-clock component (fully deterministic).
    time_budget: Option<Duration>,
}

impl NativeLimits {
    /// Product defaults, with env overrides applied.
    fn from_env() -> NativeLimits {
        let ms = env_u64("ANUBIS_NATIVE_TIME_BUDGET_MS", DEFAULT_TIME_BUDGET_MS);
        NativeLimits {
            time_budget: (ms > 0).then(|| Duration::from_millis(ms)),
            ..NativeLimits::deterministic(env_u64("ANUBIS_NATIVE_CONFLICT_BUDGET", DEFAULT_BUDGET))
        }
    }

    /// An explicit conflict budget with NO wall-clock component — used by the differential gate and
    /// the unit suites, which must be reproducible to the conflict. The size ceilings still apply:
    /// they are deterministic, so they cost nothing in reproducibility.
    fn deterministic(conflicts: u64) -> NativeLimits {
        NativeLimits {
            gate_ceiling: env_u64("ANUBIS_NATIVE_GATE_CEILING", MAX_BLAST_GATES),
            clause_ceiling: env_u64("ANUBIS_NATIVE_CLAUSE_CEILING", MAX_CNF_CLAUSES as u64)
                as usize,
            conflicts,
            cert_work: env_u64("ANUBIS_NATIVE_CERT_WORK", MAX_CERT_WORK),
            time_budget: None,
        }
    }
}

/// Native decision for one SMT-LIB2 obligation, as emitted by `compiler/src/middle/mod.rs`.
///
/// Returns:
/// * `Some(true)`  — SATISFIABLE. A model of `assumptions ∧ ¬property` exists, i.e. a counterexample;
///   the obligation's property is NOT proven. (Same meaning as z3 answering `sat`.)
/// * `Some(false)` — UNSATISFIABLE. No model — the property is PROVEN. (z3 `unsat`.)
/// * `None`        — the solver declines: out-of-fragment (non-BV theory, unsupported op, parse
///   failure) OR undecided within the budget. The caller MUST defer to z3.
///
/// SOUNDNESS: a definite verdict is returned ONLY when the formula parsed as pure QF_BV, every term
/// bit-blasted with a supported gate, and the SAT engine actually decided the resulting CNF. Any
/// uncertainty is `None`. So this function can be wired in front of z3 without ever changing a verdict
/// z3 would not also give — provided the bit-blaster is correct, which the differential gate + the Lean
/// proof establish.
pub fn native_check_sat(smt: &str) -> Option<bool> {
    native_check_sat_model(smt).map(|v| matches!(v, NativeVerdict::Sat(_)))
}

/// As [`native_check_sat`] but with an explicit conflict budget and no wall-clock component (used by
/// the differential gate, which must be reproducible).
pub fn native_check_sat_budget(smt: &str, budget: u64) -> Option<bool> {
    native_check_sat_model_budget(smt, budget).map(|v| matches!(v, NativeVerdict::Sat(_)))
}

/// AUTHORITATIVE boolean verdict (Phase-7): as [`native_check_sat`] but fragment-gated (see
/// [`native_check_sat_model_authoritative`]). Returns `None` (defer to z3) unless every op is
/// machine-checked. The compiler uses this wherever native may be the sole authority.
pub fn native_check_sat_authoritative(smt: &str) -> Option<bool> {
    native_check_sat_model_authoritative(smt).map(|v| matches!(v, NativeVerdict::Sat(_)))
}

/// A definite native verdict, carrying the reconstructed model on SAT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeVerdict {
    /// No model exists — the obligation's property is PROVEN.
    Unsat,
    /// A model exists — `(name, value, width)` for every declared bit-vector variable, LSB-verified.
    /// The model has ALREADY been re-checked by the independent `bv::Formula::eval` replay: this
    /// variant is returned only when the assignment concretely satisfies every assertion.
    Sat(Vec<(String, u128, u32)>),
}

/// As [`native_check_sat`], additionally reconstructing the SMT-level model on SAT.
///
/// The model is read out of the CDCL assignment via the bit-blaster's variable→literal map, then
/// REPLAYED through the independent concrete evaluator (`bv::Formula::eval` — a straight-line
/// interpreter sharing no code with the bit-blaster or the SAT engine). If the replay does not
/// re-satisfy the formula — which would mean a solver defect — this returns `None` (defer), so a
/// broken model can never be presented as a counterexample. UNSAT needs no model.
pub fn native_check_sat_model(smt: &str) -> Option<NativeVerdict> {
    decide(smt, &NativeLimits::from_env())
}

/// AUTHORITATIVE verdict (Phase-7 TCB minimization). Identical to [`native_check_sat_model`] EXCEPT it
/// first applies the proof-backed fragment gate ([`fragment::is_proven_authoritative`]): if the
/// obligation touches any op whose bit-blast is not machine-checked in `formal/Anubis/BitBlast.lean`
/// (`bvsdiv`/`bvudiv`/`bvsrem`/`bvurem`, `bvashr`, `sign_extend`), it returns `None` (defer) rather
/// than a native verdict. Note `Mul` — const AND var×var — IS proven and admitted; see
/// [`fragment::PROVEN_OP_TAGS`] for the live list rather than trusting any prose copy of it.
///
/// On the native-authoritative path (z3-absent window), a native `Unsat` is trusted only when:
/// 1. every op is in the proven fragment, and
/// 2. the CDCL engine produced a RUP/LRAT certificate that [`lrat::check_proof`] accepts.
///
/// SAT still requires independent `bv::Formula::eval` model replay. Division remains deferred by
/// design. The un-gated [`native_check_sat_model`] uses the same Unsat-cert check so no Unsat leaves
/// this crate without a verified certificate.
pub fn native_check_sat_model_authoritative(smt: &str) -> Option<NativeVerdict> {
    let limits = NativeLimits::from_env();
    let formula = parse::parse_smt2(smt)?;
    if !fragment::is_proven_authoritative(&formula) {
        return None;
    }
    decide_formula(smt, &formula, &limits)
}

/// As [`native_check_sat_model`] with an explicit conflict budget and no wall-clock component.
pub fn native_check_sat_model_budget(smt: &str, budget: u64) -> Option<NativeVerdict> {
    decide(smt, &NativeLimits::deterministic(budget))
}

/// The proof objects behind an `Unsat`, in formats an OUTSIDE checker can consume.
///
/// The solver already re-derives every RUP step in-process before returning `Unsat` — that is the
/// fail-closed rule. But "the compiler checked a certificate" is a claim about the compiler; an
/// evidence bundle that keeps only the verdict asks a reader to trust exactly the component under
/// review. These fields let a third party re-run the refutation with `drat-trim`, `cake_lpr`, or any
/// DIMACS/DRAT tool, with no Anubis binary in the loop.
#[derive(Debug, Clone)]
pub struct ProofArtifacts {
    /// The exact SMT-LIB query that was decided.
    pub smt: String,
    /// The blasted CNF as DIMACS text — the clauses the refutation is *against*.
    pub cnf_dimacs: String,
    /// The RUP refutation as DRAT text, one derived clause per line, terminating in the empty clause.
    pub proof_drat: String,
    pub num_vars: usize,
    pub num_clauses: usize,
    pub steps: usize,
    /// Which checker accepted it in-process, so a mismatch with an external tool is attributable.
    pub checker: &'static str,
    pub checker_version: &'static str,
}

fn cert_to_dimacs(cert: &lrat::UnsatCert) -> String {
    let mut out = format!("p cnf {} {}\n", cert.num_vars, cert.original.len());
    for c in &cert.original {
        for l in c {
            out.push_str(&l.to_string());
            out.push(' ');
        }
        out.push_str("0\n");
    }
    out
}

fn cert_to_drat(cert: &lrat::UnsatCert) -> String {
    let mut out = String::new();
    for step in &cert.steps {
        for l in &step.lits {
            out.push_str(&l.to_string());
            out.push(' ');
        }
        out.push_str("0\n");
    }
    out
}

/// Decide `smt`, and on a certificate-backed `Unsat` also hand back the CNF + refutation.
///
/// Runs the SAME `decide_formula` path as every other entry point — the bounds cannot be bypassed by
/// asking for artifacts, which is why this threads an out-parameter instead of adding a second
/// decision path. `None` artifacts mean the verdict was Sat, or the cert was declined/absent.
pub fn native_prove_with_artifacts(smt: &str) -> Option<(NativeVerdict, Option<ProofArtifacts>)> {
    let limits = NativeLimits::from_env();
    let formula = parse::parse_smt2(smt)?;
    if !fragment::is_proven_authoritative(&formula) {
        return None;
    }
    let mut artifacts = None;
    let v = decide_formula_inner(smt, &formula, &limits, Some(&mut artifacts))?;
    Some((v, artifacts))
}

/// Parse, then decide under `limits`.
fn decide(smt: &str, limits: &NativeLimits) -> Option<NativeVerdict> {
    let formula = parse::parse_smt2(smt)?;
    decide_formula(smt, &formula, limits)
}

/// The single decision path every public entry point funnels through, so all four bounds are applied
/// uniformly and cannot be bypassed by adding an entry point.
fn decide_formula(
    smt: &str,
    formula: &bv::Formula,
    limits: &NativeLimits,
) -> Option<NativeVerdict> {
    decide_formula_inner(smt, formula, limits, None)
}

/// The single decision path. `artifacts_out`, when supplied, receives the CNF + refutation on a
/// certificate-accepted `Unsat`; it never changes a verdict or relaxes a bound.
fn decide_formula_inner(
    smt: &str,
    formula: &bv::Formula,
    limits: &NativeLimits,
    artifacts_out: Option<&mut Option<ProofArtifacts>>,
) -> Option<NativeVerdict> {
    let started = Instant::now();
    // BOUND 1 — pre-blast: decline an obligation whose CNF would be oversized, before allocating it.
    let cost = blast::gate_cost(formula);
    if cost > limits.gate_ceiling {
        log_stats(smt, 0, 0, 0, "declined_gate_ceiling", started);
        return None;
    }
    // BOUND 2 — during blasting: the Cnf refuses to grow past the clause ceiling and `blast_with_map`
    // returns None if it ever had to (a truncated CNF is a different formula, so it decides nothing).
    let mut cnf = Cnf::with_clause_limit(limits.clause_ceiling);
    let map = blast::blast_with_map(formula, &mut cnf)?;
    // BOUND 3 — the search: conflict budget (deterministic, primary) + wall-clock net.
    let solve_limits = SolveLimits {
        conflicts: limits.conflicts,
        deadline: limits.time_budget.map(|d| Instant::now() + d),
    };
    let (vars, clauses) = (cnf.num_vars(), cnf.num_clauses());
    let outcome = cnf.solve_limited(&solve_limits);
    let verdict = match outcome.result {
        // Fail-closed: never trust Unsat without an independently verified RUP certificate.
        SatResult::Unsat(cert) => {
            // BOUND 4 — the certificate: `check_proof` re-derives every RUP step with a naive
            // round-based propagation, so its cost grows ~quadratically in conflicts and can far
            // exceed the search it verifies. Refuse to CHECK an oversized certificate rather than
            // grind on it. Declining to check ⇒ `None` ⇒ defer; the "no Unsat without an accepted
            // cert" rule is strengthened, never relaxed.
            //
            // Note this bound is DETERMINISTIC and the wall-clock deadline is deliberately NOT
            // consulted here: once the work is under the ceiling the check is bounded by
            // construction, so cutting it on elapsed time would trade a decision for load-dependence
            // and buy nothing.
            let steps = cert.steps.len() as u64;
            let work = steps.saturating_mul(steps.saturating_add(cert.original.len() as u64));
            if work > limits.cert_work {
                log_stats(
                    smt,
                    vars,
                    clauses,
                    outcome.conflicts,
                    "declined_cert_work",
                    started,
                );
                return None;
            }
            if check_proof(&cert) {
                if let Some(slot) = artifacts_out {
                    *slot = Some(ProofArtifacts {
                        smt: smt.to_string(),
                        cnf_dimacs: cert_to_dimacs(&cert),
                        proof_drat: cert_to_drat(&cert),
                        num_vars: cert.num_vars,
                        num_clauses: cert.original.len(),
                        steps: cert.steps.len(),
                        checker: "anubis-solver::lrat::check_proof",
                        checker_version: env!("CARGO_PKG_VERSION"),
                    });
                }
                Some(NativeVerdict::Unsat)
            } else {
                None
            }
        }
        SatResult::Unknown => None,
        SatResult::Sat(assign) => {
            // Read each declared bit-vector's value out of the assignment (LSB first). A variable
            // absent from the map or a literal beyond the assignment is unconstrained — 0 is a model.
            let read_lit = |l: &sat::Lit| -> bool {
                let v = assign.get(l.var()).copied().unwrap_or(false);
                if l.is_neg() {
                    !v
                } else {
                    v
                }
            };
            let mut model = Vec::new();
            let mut env = std::collections::HashMap::new();
            for (name, w) in &formula.bv_vars {
                let value = match map.bv.get(name) {
                    Some(bits) => bits
                        .iter()
                        .enumerate()
                        .fold(0u128, |acc, (i, l)| acc | ((read_lit(l) as u128) << i)),
                    None => 0,
                };
                env.insert(name.clone(), value);
                model.push((name.clone(), value, *w));
            }
            let mut bool_env = std::collections::HashMap::new();
            for name in &formula.bool_vars {
                bool_env.insert(
                    name.clone(),
                    map.bools.get(name).map(&read_lit).unwrap_or(false),
                );
            }
            // The native replay: the model must concretely satisfy every assertion under the
            // independent evaluator, or we refuse to certify the SAT (defer to z3).
            if formula.eval(&env, &bool_env) != Some(true) {
                None
            } else {
                Some(NativeVerdict::Sat(model))
            }
        }
    };
    let tag = match (&verdict, outcome.timed_out) {
        (Some(NativeVerdict::Unsat), _) => "unsat",
        (Some(NativeVerdict::Sat(_)), _) => "sat",
        (None, true) => "declined_deadline",
        (None, false) => "declined_budget_or_cert",
    };
    log_stats(smt, vars, clauses, outcome.conflicts, tag, started);
    verdict
}

/// Opt-in per-obligation telemetry (`ANUBIS_NATIVE_STATS_LOG=<path>`), appended as TSV:
/// `vars  clauses  conflicts  outcome  micros  first-line-of-smt`.
///
/// This is how [`DEFAULT_BUDGET`] is tuned against real obligations rather than guessed: sweep the
/// corpus with `ANUBIS_NATIVE_CONFLICT_BUDGET` set high, then read off the conflict distribution.
/// Zero cost when the variable is unset; never influences a verdict.
fn log_stats(
    smt: &str,
    vars: usize,
    clauses: usize,
    conflicts: u64,
    outcome: &str,
    started: Instant,
) {
    let Ok(path) = std::env::var("ANUBIS_NATIVE_STATS_LOG") else {
        return;
    };
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let head: String = smt
            .lines()
            .filter(|l| l.starts_with("(assert"))
            .count()
            .to_string();
        let _ = writeln!(
            f,
            "{vars}\t{clauses}\t{conflicts}\t{outcome}\t{}\t{head}",
            started.elapsed().as_micros()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsat_hands_back_a_checkable_cnf_and_refutation() {
        // An evidence bundle that keeps only "status: UNSAT" asks the reader to trust the component
        // under review. These are the objects that let someone re-run the refutation elsewhere, so
        // the test asserts they are STRUCTURALLY USABLE, not merely non-empty:
        //   - the DIMACS header agrees with the clause count actually emitted;
        //   - the DRAT proof terminates in the empty clause (the refutation's whole point);
        //   - the in-process checker re-accepts what was serialized.
        let smt = "(set-logic QF_BV)\n(assert (not (= (_ bv5 64) (_ bv5 64))))\n(check-sat)\n";
        let (verdict, art) = native_prove_with_artifacts(smt).expect("decided");
        assert!(matches!(verdict, NativeVerdict::Unsat));
        let art = art.expect("unsat must carry its refutation");

        let header = art.cnf_dimacs.lines().next().unwrap();
        assert!(header.starts_with("p cnf "), "DIMACS header: {header}");
        let declared: usize = header.split_whitespace().nth(3).unwrap().parse().unwrap();
        let body = art
            .cnf_dimacs
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty())
            .count();
        assert_eq!(
            declared, body,
            "DIMACS header count must match emitted clauses"
        );
        assert_eq!(declared, art.num_clauses);

        let last = art
            .proof_drat
            .lines()
            .filter(|l| !l.trim().is_empty())
            .next_back()
            .expect("proof has steps");
        assert_eq!(
            last.trim(),
            "0",
            "a refutation must end in the empty clause"
        );
        assert_eq!(art.checker, "anubis-solver::lrat::check_proof");
        assert!(!art.checker_version.is_empty());
    }

    #[test]
    fn sat_carries_no_refutation() {
        // Artifacts are proof objects. A SAT verdict has none, and must not present anything that
        // could be mistaken for one.
        let smt = "(set-logic QF_BV)\n(declare-fun x () (_ BitVec 8))\n                   (assert (= x (_ bv7 8)))\n(check-sat)\n";
        if let Some((verdict, art)) = native_prove_with_artifacts(smt) {
            assert!(matches!(verdict, NativeVerdict::Sat(_)));
            assert!(art.is_none(), "SAT must not carry a refutation");
        }
    }

    #[test]
    fn ground_true_equality_is_unsat_negated() {
        // ¬(5 == 5) is unsatisfiable ⇒ 5 == 5 is proven.
        let smt = "(set-logic QF_BV)\n(assert (not (= (_ bv5 64) (_ bv5 64))))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), Some(false));
    }

    #[test]
    fn ground_false_equality_is_sat() {
        // 5 == 6 is satisfiable-as-written? No: (= 5 6) is false, so the assert is unsatisfiable.
        let smt = "(set-logic QF_BV)\n(assert (= (_ bv5 64) (_ bv6 64)))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), Some(false));
    }

    #[test]
    fn simple_symbolic_add_is_sat() {
        // exists x. x + 1 == 3  (x = 2). SAT.
        let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
                   (assert (= (bvadd x (_ bv1 64)) (_ bv3 64)))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), Some(true));
    }

    #[test]
    fn out_of_fragment_defers() {
        // A string OPERATION (str.len, not just equality) is out of the supported fragment → decline.
        let smt = "(set-logic QF_S)\n(declare-const s String)\n\
                   (assert (= (str.len s) 3))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), None);
    }

    #[test]
    fn string_equality_is_decided() {
        // s == "a" is satisfiable; s == "a" AND s == "b" is not (distinct literals ⇒ distinct ids).
        let sat = "(set-logic QF_S)\n(declare-const s String)\n(assert (= s \"a\"))\n(check-sat)\n";
        assert_eq!(native_check_sat(sat), Some(true));
        let unsat = "(set-logic QF_S)\n(declare-const s String)\n\
                     (assert (= s \"a\"))\n(assert (= s \"b\"))\n(check-sat)\n";
        assert_eq!(native_check_sat(unsat), Some(false));
        // A literal tautology and contradiction.
        assert_eq!(
            native_check_sat("(set-logic QF_S)\n(assert (not (= \"a\" \"a\")))\n(check-sat)\n"),
            Some(false)
        );
        assert_eq!(
            native_check_sat("(set-logic QF_S)\n(assert (not (= \"a\" \"b\")))\n(check-sat)\n"),
            Some(true)
        );
    }

    #[test]
    fn sat_model_is_concrete_and_replayed() {
        // exists x. x + 1 == 3 — the ONLY model is x = 2; the extracted model must say so.
        let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
                   (assert (= (bvadd x (_ bv1 64)) (_ bv3 64)))\n(check-sat)\n(get-model)\n";
        match native_check_sat_model(smt) {
            Some(NativeVerdict::Sat(model)) => {
                assert_eq!(model, vec![("x".to_string(), 2u128, 64u32)]);
            }
            other => panic!("expected Sat with x=2, got {:?}", other),
        }
        // And a proven obligation yields Unsat with no model needed.
        let proven = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
                      (assert (not (bvule x x)))\n(check-sat)\n";
        assert_eq!(native_check_sat_model(proven), Some(NativeVerdict::Unsat));
    }

    #[test]
    fn authoritative_unsat_requires_valid_cert_path() {
        // ¬(x ≤ x) is unsat on the proven fragment; authoritative must return Unsat
        // (which only happens after lrat::check_proof accepts the CDCL certificate).
        let proven = "(set-logic QF_BV)\n(declare-const x (_ BitVec 32))\n\
                      (assert (not (bvule x x)))\n(check-sat)\n";
        assert_eq!(
            native_check_sat_model_authoritative(proven),
            Some(NativeVerdict::Unsat)
        );
        assert_eq!(native_check_sat_authoritative(proven), Some(false));
    }

    #[test]
    fn authoritative_defers_division() {
        // bvsdiv is deferred by design — not in the proven fragment.
        let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 32))\n\
                   (assert (not (= (bvsdiv x (_ bv1 32)) x)))\n(check-sat)\n";
        assert_eq!(native_check_sat_model_authoritative(smt), None);
    }

    // ---- Termination bounds (2026-07-26). The checker must never grind, on any input. ----

    /// The exact obligation `compiler/stdlib/std/math.anb::math_mul` emits, and the reason
    /// `import std.math;` used to hang `anubis check` forever: `ensures(result == a * b)` over a body
    /// that returns `a * b` lowers to `¬(a*b = a*b)` — TWO 64×64 schoolbook multipliers asked to be
    /// proven equal, i.e. a multiplier miter (50,688 vars / 168,643 clauses; still undecided after
    /// 100,000 conflicts / 15.3 s, against a 2,000,000-conflict budget).
    ///
    /// Structural hashing collapses the second multiplier onto the first and constant folding reduces
    /// the equality to `true`, so this must now be DECIDED (proven) — not merely deferred — and fast.
    #[test]
    fn stdlib_math_mul_miter_is_decided_not_hung() {
        let smt = "(set-logic QF_BV)\n\
                   (declare-const anb_a (_ BitVec 64))\n(declare-const anb_b (_ BitVec 64))\n\
                   (assert (bvsge anb_a (_ bv0 64)))\n(assert (bvsle anb_a (_ bv4294967295 64)))\n\
                   (assert (bvsge anb_b (_ bv0 64)))\n(assert (bvsle anb_b (_ bv4294967295 64)))\n\
                   (assert (not (= (bvmul anb_a anb_b) (bvmul anb_a anb_b))))\n(check-sat)\n";
        let t0 = Instant::now();
        let v = native_check_sat_model_authoritative(smt);
        let elapsed = t0.elapsed();
        assert_eq!(
            v,
            Some(NativeVerdict::Unsat),
            "the a*b==a*b miter must be PROVEN via structural sharing, not deferred"
        );
        // Generous (CI under load) but still orders of magnitude below the old grind.
        assert!(
            elapsed < Duration::from_secs(5),
            "miter took {elapsed:?} — sharing/folding regressed"
        );
    }

    /// Structural hashing must actually SHARE: blasting `a*b` twice may not cost twice the clauses.
    /// This is the mechanism the test above depends on, asserted directly so a regression names itself.
    #[test]
    fn identical_subterms_are_structurally_shared() {
        let one =
            "(set-logic QF_BV)\n(declare-const a (_ BitVec 32))\n(declare-const b (_ BitVec 32))\n\
                   (assert (bvult (bvmul a b) (_ bv7 32)))\n(check-sat)\n";
        let two =
            "(set-logic QF_BV)\n(declare-const a (_ BitVec 32))\n(declare-const b (_ BitVec 32))\n\
                   (assert (bvult (bvmul a b) (_ bv7 32)))\n\
                   (assert (bvugt (bvmul a b) (_ bv2 32)))\n(check-sat)\n";
        let clauses = |smt: &str| {
            let f = parse::parse_smt2(smt).expect("parse");
            let mut cnf = Cnf::new();
            blast::blast(&f, &mut cnf).expect("blast");
            cnf.num_clauses()
        };
        let (c1, c2) = (clauses(one), clauses(two));
        // The second `bvmul a b` must reuse the first's gates: the delta is the extra comparator only,
        // nowhere near a whole second multiplier (which is the dominant cost of `c1`).
        assert!(
            c2 < c1 + c1 / 4,
            "second identical product was not shared: {c1} → {c2} clauses"
        );
    }

    /// `blast::gate_cost` is the pre-blast bound, so it MUST over-approximate the real blast. If it
    /// ever under-counts, the gate ceiling stops bounding anything.
    #[test]
    fn gate_cost_over_approximates_the_real_blast() {
        for smt in [
            "(set-logic QF_BV)(declare-const a (_ BitVec 32))(declare-const b (_ BitVec 32))\
             (assert (bvult (bvmul a b) a))(check-sat)",
            "(set-logic QF_BV)(declare-const a (_ BitVec 16))(declare-const b (_ BitVec 16))\
             (assert (= (bvadd (bvmul a (_ bv23 16)) (bvshl a b)) (bvsub b (bvneg a))))(check-sat)",
            "(set-logic QF_BV)(declare-const a (_ BitVec 64))(declare-const b (_ BitVec 64))\
             (assert (and (bvsle a b) (not (bvsgt (bvxor a b) (bvor a (bvand a b))))))(check-sat)",
            "(set-logic QF_BV)(declare-const a (_ BitVec 8))\
             (assert (bvult (ite (bvslt a (_ bv0 8)) (bvneg a) a) (_ bv9 8)))(check-sat)",
        ] {
            let f = parse::parse_smt2(smt).expect("parse");
            let cost = blast::gate_cost(&f);
            let mut cnf = Cnf::new();
            blast::blast(&f, &mut cnf).expect("blast");
            // Every gate allocates exactly one variable (plus the one forced-true constant and one per
            // declared bit), so variables are a faithful lower bound on gates actually emitted.
            let declared: usize =
                f.bv_vars.iter().map(|(_, w)| *w as usize).sum::<usize>() + f.bool_vars.len() + 1;
            let real_gates = cnf.num_vars().saturating_sub(declared) as u64;
            assert!(
                cost >= real_gates,
                "gate_cost UNDER-counted ({cost} < {real_gates}) — the pre-blast ceiling is unsound as a bound:\n{smt}"
            );
        }
    }

    /// BOUND 1: an obligation whose estimated blast exceeds the gate ceiling is declined before any
    /// CNF is allocated. Driven through the env override so the test does not depend on the shipped
    /// constant, and asserts the DEFAULT still admits the same formula (the ceiling is not so tight
    /// that ordinary work trips it).
    #[test]
    fn gate_ceiling_declines_before_blasting() {
        let smt =
            "(set-logic QF_BV)\n(declare-const a (_ BitVec 64))\n(declare-const b (_ BitVec 64))\n\
                   (assert (bvult (bvmul a b) a))\n(check-sat)\n";
        let f = parse::parse_smt2(smt).expect("parse");
        let cost = blast::gate_cost(&f);
        assert!(cost > 1_000, "probe should be non-trivial, cost={cost}");
        let tight = NativeLimits {
            gate_ceiling: cost - 1,
            ..NativeLimits::deterministic(DEFAULT_BUDGET)
        };
        assert_eq!(
            decide(smt, &tight),
            None,
            "an over-ceiling obligation must defer"
        );
        assert!(
            cost <= MAX_BLAST_GATES,
            "a plain 64-bit var-mul must stay UNDER the shipped ceiling (cost={cost})"
        );
    }

    /// BOUND 2: the clause ceiling truncates the CNF, and a truncated CNF decides nothing — both at
    /// the blaster (returns `None`) and at the solver (returns `Unknown`), so neither layer alone is
    /// load-bearing.
    #[test]
    fn clause_ceiling_never_decides_a_truncated_formula() {
        let smt =
            "(set-logic QF_BV)\n(declare-const a (_ BitVec 32))\n(declare-const b (_ BitVec 32))\n\
                   (assert (bvult (bvmul a b) a))\n(check-sat)\n";
        let f = parse::parse_smt2(smt).expect("parse");
        let mut cnf = Cnf::with_clause_limit(64);
        assert!(
            blast::blast(&f, &mut cnf).is_none(),
            "blast must refuse a truncated CNF"
        );
        assert!(cnf.overflowed(), "overflow flag must latch");
        assert!(
            matches!(
                cnf.solve_limited(&SolveLimits::conflicts(DEFAULT_BUDGET))
                    .result,
                SatResult::Unknown
            ),
            "an overflowed Cnf must never be decided"
        );
        let tight = NativeLimits {
            clause_ceiling: 64,
            ..NativeLimits::deterministic(DEFAULT_BUDGET)
        };
        assert_eq!(decide(smt, &tight), None, "over-ceiling clauses must defer");
    }

    /// BOUND 3: a starved conflict budget defers instead of deciding, and never lies. `a*b == b*a` is
    /// a TRUE property whose two products share no gates, so it is a genuine multiplier miter: the
    /// native lane must terminate on it, with a deferral rather than a guess.
    #[test]
    fn hard_miter_defers_within_the_conflict_budget() {
        let smt =
            "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n(declare-const y (_ BitVec 64))\n\
                   (assert (not (= (bvmul x y) (bvmul y x))))\n(check-sat)\n";
        let t0 = Instant::now();
        let v = native_check_sat_model_authoritative(smt);
        let elapsed = t0.elapsed();
        assert_eq!(v, None, "a commutativity miter must defer, not decide");
        assert!(
            elapsed < Duration::from_secs(60),
            "commutativity miter took {elapsed:?} — the conflict budget is not bounding the search"
        );
    }

    /// BOUND 3b: the wall-clock net converts a would-be grind into a deferral. A deadline already in
    /// the past makes any search that reaches a conflict stop immediately.
    #[test]
    fn expired_deadline_defers() {
        let smt =
            "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n(declare-const y (_ BitVec 64))\n\
                   (assert (not (= (bvmul x y) (bvmul y x))))\n(check-sat)\n";
        let expired = NativeLimits {
            time_budget: Some(Duration::from_millis(0)),
            ..NativeLimits::deterministic(u64::MAX)
        };
        // A zero-length budget yields a deadline at `now`, so the first sampled check stops the search.
        assert_eq!(decide(smt, &expired), None);
    }

    /// BOUND 4: an oversized RUP certificate is DECLINED rather than checked. `check_proof` costs
    /// ~quadratically in conflicts (78 s at 42,676 steps), so without this the grind simply moves from
    /// the search to the checker. Declining to check yields `None` — never an unverified `Unsat`.
    #[test]
    fn oversized_certificate_defers_instead_of_grinding() {
        // A small, genuinely-unsat obligation that needs real conflicts to refute.
        let smt =
            "(set-logic QF_BV)\n(declare-const x (_ BitVec 16))\n(declare-const y (_ BitVec 16))\n\
                   (assert (not (bvule (bvand x y) (bvor x y))))\n(check-sat)\n";
        // With the shipped ceiling this is proven.
        assert_eq!(
            native_check_sat_model_authoritative(smt),
            Some(NativeVerdict::Unsat)
        );
        // With the cert-work ceiling at zero, the SAME obligation must defer rather than return an
        // Unsat whose certificate was never verified.
        let no_cert = NativeLimits {
            cert_work: 0,
            ..NativeLimits::deterministic(DEFAULT_BUDGET)
        };
        assert_eq!(
            decide(smt, &no_cert),
            None,
            "an un-checkable certificate must defer, never yield Unsat"
        );
    }

    /// The whole point of every bound: they may only ever turn a verdict into a deferral. Same formula,
    /// starved limits — the answer is `None` or the SAME verdict, never the opposite one.
    #[test]
    fn bounds_only_ever_weaken_to_defer() {
        let cases = [
            // proven
            "(set-logic QF_BV)(declare-const x (_ BitVec 32))(assert (not (bvule x x)))(check-sat)",
            // counterexample
            "(set-logic QF_BV)(declare-const x (_ BitVec 32))\
             (assert (= (bvadd x (_ bv1 32)) (_ bv3 32)))(check-sat)",
            // proven, with a product
            "(set-logic QF_BV)(declare-const x (_ BitVec 32))(declare-const y (_ BitVec 32))\
             (assert (not (= (bvmul x y) (bvmul x y))))(check-sat)",
        ];
        for smt in cases {
            let full = decide(smt, &NativeLimits::deterministic(DEFAULT_BUDGET));
            assert!(full.is_some(), "baseline must decide: {smt}");
            for starved in [
                NativeLimits {
                    gate_ceiling: 1,
                    ..NativeLimits::deterministic(DEFAULT_BUDGET)
                },
                NativeLimits {
                    clause_ceiling: 8,
                    ..NativeLimits::deterministic(DEFAULT_BUDGET)
                },
                NativeLimits {
                    cert_work: 0,
                    ..NativeLimits::deterministic(DEFAULT_BUDGET)
                },
                NativeLimits::deterministic(0),
            ] {
                match decide(smt, &starved) {
                    None => {}
                    Some(v) => assert_eq!(
                        Some(v),
                        full,
                        "a starved bound changed the VERDICT (not just deferred) on: {smt}"
                    ),
                }
            }
        }
    }
}
