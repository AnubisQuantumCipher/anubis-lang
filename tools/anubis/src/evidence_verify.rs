//! Independent portable evidence verifier.
//!
//! Host-side, offline, **no Tart / VZ required**. Walks a path and runs every
//! applicable check against sealed artifacts:
//!
//! | Artifact | Check | Classification |
//! |----------|-------|----------------|
//! | Evidence bundle (`evidence.json`) | `verify_pca` + optional Ed25519 | LAB_REAL / PARTIAL |
//! | Engagement (`engagement.json`) | content_hash | LAB_REAL |
//! | Receipt chain | hash + optional HMAC | LAB_REAL_HMAC |
//! | Run capability JSON | schema + optional MAC | LAB_REAL_HMAC |
//! | Confinement + source | re-derive grants | LAB_REAL |
//!
//! Does **not** claim Ed25519 for HMAC receipts/caps, and does not claim
//! production-PKI attestation. Fail-closed: any FAIL makes overall `ok=false`.

use anubis_compiler::evidence::{pca_signature_status, verify_pca};
use anubis_compiler::package::confinement::{self, ConfinementManifest, CONFINEMENT_FILENAME};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EVIDENCE_VERIFY_SCHEMA: &str = "anubis-evidence-verify-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckResult {
    pub id: String,
    pub status: CheckStatus,
    /// Honest trust label for what this check actually proves.
    pub classification: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceVerifyReport {
    pub schema: String,
    pub path: String,
    pub ok: bool,
    pub checks: Vec<CheckResult>,
    pub classifications_seen: Vec<String>,
    pub notes: Vec<String>,
}

/// Options for the portable verifier.
#[derive(Debug, Clone, Default)]
pub struct EvidenceVerifyOpts {
    /// Require PCA Ed25519 signature by this public key (hex).
    pub pubkey: Option<String>,
    /// Key for run-capability MAC verification (hex or any secret string).
    pub run_cap_key: Option<String>,
    /// When true, SKIP without key becomes FAIL for run-cap MAC if a cap file is present.
    pub strict: bool,
}

impl EvidenceVerifyReport {
    fn push(
        &mut self,
        id: &str,
        status: CheckStatus,
        classification: &str,
        detail: impl Into<String>,
    ) {
        if status == CheckStatus::Fail {
            self.ok = false;
        }
        if !self
            .classifications_seen
            .iter()
            .any(|c| c == classification)
        {
            self.classifications_seen.push(classification.to_string());
        }
        self.checks.push(CheckResult {
            id: id.into(),
            status,
            classification: classification.into(),
            detail: detail.into(),
        });
    }
}

// ---------------------------------------------------------------- proof replay
//
// A bundle that verifies only its own hashes answers "was this edited?", never
// "do the proofs hold?". Recomputing MANIFEST.sha256 over a forged refutation
// therefore used to produce `overall: PASS`. The check below closes that by
// re-deriving every published refutation.
//
// It reads the PUBLISHED DIMACS CNF and DRAT-shaped refutation, binds their
// canonical paths and bytes to the manifest, and compares that CNF with native
// regeneration of the row's SMT before RUP replay. Regeneration is Anubis code,
// so it establishes same-implementation consistency. An external DRAT checker
// can independently check the CNF refutation; it does not establish the missing
// independent SMT-to-CNF or source-to-SMT correspondence.

/// Fixed verifier ceilings, never supplied by the artifact or environment.
/// Byte/line caps include comments and blanks; literal storage is shared by
/// CNF and proof. Work counts initialization, assumptions, and every clause and
/// literal visit, including repeated propagation passes. Exceeding any ceiling
/// refuses verification rather than truncating or weakening the proof check.
#[derive(Clone, Copy)]
struct RupLimits {
    file_bytes: usize,
    line_bytes: usize,
    lines: usize,
    variables: usize,
    cnf_clauses: usize,
    proof_steps: usize,
    literals: usize,
    work: u64,
}

const RUP_LIMITS: RupLimits = RupLimits {
    file_bytes: 64_000_000,
    line_bytes: 1_000_000,
    lines: 4_000_000,
    variables: 2_000_000,
    cnf_clauses: 2_000_000,
    proof_steps: 100_000,
    literals: 8_000_000,
    work: 50_000_000,
};

fn proof_limit(kind: &str) -> String {
    format!("ANUBIS_PROOF_LIMIT: {kind}")
}

fn proof_invalid(reason: &str) -> String {
    format!("ANUBIS_PROOF_INVALID: {reason}")
}

fn check_proof_text_limits(text: &str, limits: RupLimits) -> std::result::Result<(), String> {
    if text.len() > limits.file_bytes {
        return Err(proof_limit("file_bytes"));
    }
    for (index, line) in text.lines().enumerate() {
        if index >= limits.lines {
            return Err(proof_limit("lines"));
        }
        if line.len() > limits.line_bytes {
            return Err(proof_limit("line_bytes"));
        }
    }
    Ok(())
}

fn parse_proof_clause(
    line: &str,
    variables: usize,
    remaining_literals: &mut usize,
) -> std::result::Result<Vec<i32>, String> {
    let mut clause = Vec::new();
    let mut tokens = line.split_ascii_whitespace();
    while let Some(token) = tokens.next() {
        if token == "0" {
            if tokens.next().is_some() {
                return Err(proof_invalid("tokens after clause terminator"));
            }
            return Ok(clause);
        }
        let digits = token.strip_prefix('-').unwrap_or(token);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(proof_invalid("nonnumeric clause literal"));
        }
        let literal: i32 = token
            .parse()
            .map_err(|_| proof_invalid("literal outside supported integer range"))?;
        let variable = usize::try_from(literal.unsigned_abs())
            .map_err(|_| proof_invalid("literal outside supported variable range"))?;
        if variable == 0 || variable > variables {
            return Err(proof_invalid("literal outside declared CNF variable bound"));
        }
        *remaining_literals = remaining_literals
            .checked_sub(1)
            .ok_or_else(|| proof_limit("literals"))?;
        clause
            .try_reserve(1)
            .map_err(|_| proof_limit("allocation"))?;
        clause.push(literal);
    }
    Err(proof_invalid("unterminated clause"))
}

struct ParsedCnf {
    variables: usize,
    clauses: Vec<Vec<i32>>,
}

fn parse_cnf(
    text: &str,
    limits: RupLimits,
    remaining_literals: &mut usize,
) -> std::result::Result<ParsedCnf, String> {
    check_proof_text_limits(text, limits)?;
    let mut header = None;
    let mut clauses = Vec::new();
    for line in text.lines() {
        let mut tokens = line.split_ascii_whitespace();
        let Some(first) = tokens.next() else { continue };
        if first == "c" {
            continue;
        }
        if first == "p" {
            if header.is_some() || tokens.next() != Some("cnf") {
                return Err(proof_invalid("duplicate or malformed CNF header"));
            }
            let mut count = || -> std::result::Result<usize, String> {
                let token = tokens
                    .next()
                    .ok_or_else(|| proof_invalid("incomplete CNF header"))?;
                if !token.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(proof_invalid("invalid CNF header count"));
                }
                token
                    .parse()
                    .map_err(|_| proof_invalid("CNF header count overflow"))
            };
            let variables = count()?;
            let declared_clauses = count()?;
            if tokens.next().is_some() {
                return Err(proof_invalid("trailing CNF header tokens"));
            }
            if variables > limits.variables {
                return Err(proof_limit("variables"));
            }
            if declared_clauses > limits.cnf_clauses {
                return Err(proof_limit("cnf_clauses"));
            }
            header = Some((variables, declared_clauses));
            continue;
        }
        let (variables, declared_clauses) =
            header.ok_or_else(|| proof_invalid("CNF clause before required header"))?;
        if clauses.len() >= declared_clauses {
            return Err(proof_invalid("CNF clause count exceeds header"));
        }
        let clause = parse_proof_clause(line, variables, remaining_literals)?;
        clauses
            .try_reserve(1)
            .map_err(|_| proof_limit("allocation"))?;
        clauses.push(clause);
    }
    let (variables, declared_clauses) =
        header.ok_or_else(|| proof_invalid("missing CNF header"))?;
    if clauses.len() != declared_clauses {
        return Err(proof_invalid("CNF clause count differs from header"));
    }
    Ok(ParsedCnf { variables, clauses })
}

fn parse_rup_additions(
    text: &str,
    variables: usize,
    limits: RupLimits,
    remaining_literals: &mut usize,
) -> std::result::Result<Vec<Vec<i32>>, String> {
    check_proof_text_limits(text, limits)?;
    let mut lemmas = Vec::new();
    for line in text.lines() {
        match line.split_ascii_whitespace().next() {
            None | Some("c") => continue,
            Some("d") => return Err("ANUBIS_PROOF_UNSUPPORTED: DRAT deletion steps".into()),
            Some("p") => return Err(proof_invalid("header is not a DRAT addition")),
            _ => {}
        }
        if lemmas.len() >= limits.proof_steps {
            return Err(proof_limit("proof_steps"));
        }
        let lemma = parse_proof_clause(line, variables, remaining_literals)?;
        lemmas
            .try_reserve(1)
            .map_err(|_| proof_limit("allocation"))?;
        lemmas.push(lemma);
    }
    if lemmas.is_empty() {
        return Err(proof_invalid("refutation contains no lemmas"));
    }
    if !lemmas.last().is_some_and(Vec::is_empty) {
        return Err(proof_invalid("refutation must end with the empty clause"));
    }
    Ok(lemmas)
}

struct RupWork {
    remaining: u64,
}

impl RupWork {
    fn charge(&mut self, amount: usize) -> std::result::Result<(), String> {
        let amount = u64::try_from(amount).map_err(|_| proof_limit("work"))?;
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| proof_limit("work"))?;
        Ok(())
    }
}

/// Unit-propagate to fixpoint, charging every visit. Returns true on conflict.
fn propagate(
    clauses: &[Vec<i32>],
    assign: &mut [i8],
    work: &mut RupWork,
) -> std::result::Result<bool, String> {
    loop {
        let mut progressed = false;
        for clause in clauses {
            work.charge(1)?;
            let mut unassigned = 0usize;
            let mut last = 0i32;
            let mut satisfied = false;
            for &lit in clause {
                work.charge(1)?;
                let var = lit.unsigned_abs() as usize;
                if var == 0 || var >= assign.len() {
                    return Err(proof_invalid("literal outside bounded assignment"));
                }
                let want: i8 = if lit > 0 { 1 } else { -1 };
                match assign[var] {
                    0 => {
                        unassigned += 1;
                        last = lit;
                    }
                    v if v == want => {
                        satisfied = true;
                        break;
                    }
                    _ => {}
                }
            }
            if satisfied {
                continue;
            }
            if unassigned == 0 {
                return Ok(true); // every literal false: conflict
            }
            if unassigned == 1 {
                let var = last.unsigned_abs() as usize;
                assign[var] = if last > 0 { 1 } else { -1 };
                progressed = true;
            }
        }
        if !progressed {
            return Ok(false);
        }
    }
}

/// Re-derive a refutation by reverse unit propagation. Returns the number of
/// lemmas checked, or a named reason it was rejected.
fn check_rup(cnf_text: &str, proof_text: &str) -> std::result::Result<usize, String> {
    check_rup_with_limits(cnf_text, proof_text, RUP_LIMITS)
}

fn check_rup_with_limits(
    cnf_text: &str,
    proof_text: &str,
    limits: RupLimits,
) -> std::result::Result<usize, String> {
    let mut remaining_literals = limits.literals;
    let ParsedCnf {
        variables,
        clauses: mut formula,
    } = parse_cnf(cnf_text, limits, &mut remaining_literals)?;
    let lemmas = parse_rup_additions(proof_text, variables, limits, &mut remaining_literals)?;
    let steps = lemmas.len();
    let assignment_size = variables
        .checked_add(1)
        .ok_or_else(|| proof_limit("variables"))?;
    let mut work = RupWork {
        remaining: limits.work,
    };
    work.charge(assignment_size)?;
    let mut assign = Vec::new();
    assign
        .try_reserve_exact(assignment_size)
        .map_err(|_| proof_limit("allocation"))?;
    assign.resize(assignment_size, 0i8);
    for (i, lemma) in lemmas.into_iter().enumerate() {
        work.charge(assignment_size)?;
        assign.fill(0);
        // RUP: assume the lemma is false, then propagate. A conflict means the
        // formula already implied it.
        for &lit in &lemma {
            work.charge(1)?;
            let var = lit.unsigned_abs() as usize;
            let want: i8 = if lit > 0 { -1 } else { 1 };
            if assign[var] != 0 && assign[var] != want {
                return Err(format!("lemma {i} is tautological or self-contradictory"));
            }
            assign[var] = want;
        }
        if !propagate(&formula, &mut assign, &mut work)? {
            return Err(format!(
                "lemma {i} is not implied by the formula (reverse unit propagation found no conflict)"
            ));
        }
        formula
            .try_reserve(1)
            .map_err(|_| proof_limit("allocation"))?;
        formula.push(lemma);
    }
    Ok(steps)
}

/// Replay every refutation the bundle publishes.
fn verify_published_proofs(dir: &Path, id_prefix: &str, report: &mut EvidenceVerifyReport) {
    let inputs = (|| -> Result<_> {
        let manifest = replay_manifest(dir)?;
        let proofs: ReplayProofIndex = serde_json::from_slice(&sealed_replay_bytes(
            dir,
            "analysis/proofs.json",
            &manifest,
        )?)?;
        let checks: Vec<anubis_compiler::middle::SolverCheck> =
            serde_json::from_slice(&sealed_replay_bytes(dir, "solver.json", &manifest)?)?;
        if proofs.obligations.is_empty() || proofs.obligations.len() != checks.len() {
            return Err(anyhow!("empty or mismatched solver/proof inventories"));
        }
        Ok((manifest, checks, proofs))
    })();
    let (manifest, checks, proofs) = match inputs {
        Ok(inputs) => inputs,
        Err(error) => {
            report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Fail,
                "LAB_REAL",
                format!("Published proof inventory rejected: {error}"),
            );
            return;
        }
    };
    let obligations = &proofs.obligations;
    if obligations.iter().any(|obligation| {
        obligation.obligation == "solver:no-obligations"
            || obligation.proof == "not_applicable_no_obligations"
    }) || checks.iter().any(|check| {
        check.name == "solver:no-obligations"
            || check.detail == anubis_compiler::middle::NO_OBLIGATIONS_DETAIL
    }) {
        // Do not award a proof or inflate the denominator for a reporting
        // sentinel. Reuse the strict cross-artifact validation, including the
        // version-specific PCA count and proof-kind compatibility boundary.
        match check_solver_replay(dir, |_, _| false) {
            Ok(SolverReplayVerification::Current {
                rows: 1,
                replayed: 0,
                status: "not_applicable",
                complete: true,
                synthetic_compatibility,
            }) => report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Skip,
                if synthetic_compatibility { "LEGACY_COMPATIBILITY" } else { "NOT_APPLICABLE" },
                if synthetic_compatibility {
                    "Zero proof obligations: validated PCA v2 no-obligations sentinel. The historical \
                     counterexample_no_refutation tag is compatibility metadata, not a counterexample, \
                     earned proof, or solver discharge."
                } else {
                    "Zero proof obligations: validated PCA v3 no-obligations sentinel; \
                     no refutation, earned proof, or solver discharge."
                },
            ),
            Ok(_) => report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Fail,
                "LAB_REAL",
                "Synthetic proof inventory does not establish a validated no-obligations sentinel",
            ),
            Err(error) => report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Fail,
                "LAB_REAL",
                format!("Synthetic proof inventory rejected: {error}"),
            ),
        }
        return;
    }

    let mut replayed = 0usize;
    let mut uncertified = Vec::new();
    for (index, (check, proof)) in checks.iter().zip(obligations).enumerate() {
        match check_published_refutation(dir, &manifest, index, check, proof) {
            Ok(true) => replayed += 1,
            Ok(false) => uncertified.push(format!(
                "{} [status={}, proof={}]",
                proof.obligation, proof.status, proof.proof
            )),
            Err(error) => {
                report.push(
                    &format!("{id_prefix}.proofs"),
                    CheckStatus::Fail,
                    "LAB_REAL",
                    format!("REFUTATION REJECTED at obligation {index}: {error}"),
                );
                return;
            }
        }
    }

    let coverage = if uncertified.is_empty() {
        format!(
            "{replayed}/{} published RUP refutation(s) checked against CNF regenerated from each bound SMT query.",
            obligations.len()
        )
    } else {
        format!(
            "{replayed}/{} published RUP refutation(s) checked; {} row(s) have no published RUP certificate: {}. No proof assurance or solver discharge is earned by those rows.",
            obligations.len(),
            uncertified.len(),
            uncertified.join(", ")
        )
    };
    report.push(
        &format!("{id_prefix}.proofs"),
        if uncertified.is_empty() {
            CheckStatus::Pass
        } else {
            CheckStatus::Skip
        },
        if uncertified.is_empty() {
            "LAB_REAL"
        } else if replayed == 0 {
            "NO_PUBLISHED_REFUTATIONS"
        } else {
            "PARTIAL"
        },
        format!(
            "{coverage} SMT-to-CNF regeneration is same-implementation consistency; independent \
             SMT-to-CNF and source-to-SMT correspondence are not established. RUP checking \
             establishes refutation of the bound CNF, not an independent proof of the program."
        ),
    );
}

// The replay sidecar is a claim about the published solver inventory, not a new
// proof of source-to-SMT correspondence. Check its joins and recompute its model
// replays rather than trusting a rehashed `replay_valid` boolean.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SolverReplayRecord {
    schema: String,
    scope: String,
    status: String,
    replay_valid: bool,
    obligations: Vec<SolverReplayRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SolverReplayRow {
    index: usize,
    obligation: String,
    solver_status: String,
    detail: String,
    smt: String,
    status: String,
    replay_attempted: bool,
    replay_valid: bool,
}

#[derive(Deserialize)]
struct ReplayProofIndex {
    obligations: Vec<ReplayProofRow>,
}

#[derive(Deserialize)]
struct ReplayProofRow {
    obligation: String,
    status: String,
    smt: String,
    proof: String,
    cnf_dimacs: Option<String>,
    proof_drat: Option<String>,
}

#[derive(Deserialize)]
struct ReplayPcaSummary {
    pca_version: u32,
    solver_obligations: usize,
    solver_all_discharged: bool,
}

fn pca_claim_disclosure(dir: &Path) -> Result<String> {
    #[derive(Deserialize)]
    struct Disclosure {
        pca_version: u32,
        verdict: String,
    }
    let manifest = replay_manifest(dir)?;
    let claim: Disclosure =
        serde_json::from_slice(&sealed_replay_bytes(dir, "pca.json", &manifest)?)?;
    let semantics = if claim.pca_version == 2 {
        "compatibility count semantics; historical origin is not established"
    } else {
        "current count semantics"
    };
    Ok(format!(
        "declared PCA version={} ({semantics}); recorded verdict={}",
        claim.pca_version, claim.verdict
    ))
}

enum SolverReplayVerification {
    Absent,
    LegacyUnverified,
    Current {
        rows: usize,
        replayed: usize,
        status: &'static str,
        complete: bool,
        synthetic_compatibility: bool,
    },
}

fn replay_manifest(dir: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    let text = String::from_utf8(read_bounded_proof_file(
        &dir.join("MANIFEST.sha256"),
        RUP_LIMITS.file_bytes,
    )?)?;
    check_proof_text_limits(&text, RUP_LIMITS).map_err(|error| anyhow!(error))?;
    if !text.ends_with('\n') {
        return Err(anyhow!("unterminated manifest"));
    }
    let mut entries = std::collections::BTreeMap::new();
    for line in text.split_terminator('\n') {
        // The producer separates the digest from the complete relative path
        // with two spaces. A manifest-covered filename may itself contain spaces.
        let (digest, path) = line
            .split_once("  ")
            .ok_or_else(|| anyhow!("malformed manifest entry"))?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || path.is_empty()
            || path.len() > 4096
            || path
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == b'\\' || byte == b':')
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || Path::new(path).is_absolute()
            || entries.insert(path.into(), digest.into()).is_some()
        {
            return Err(anyhow!("ambiguous or duplicate manifest entry for {path}"));
        }
    }
    Ok(entries)
}

fn read_bounded_proof_file(path: &Path, file_bytes: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(anyhow!("proof artifact is not a regular file"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let limit = u64::try_from(file_bytes).map_err(|_| anyhow!(proof_limit("file_bytes")))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(anyhow!("proof artifact is not a regular file"));
    }
    if metadata.len() > limit {
        return Err(anyhow!(proof_limit("file_bytes")));
    }
    // The cap applies during the read as well as to metadata, so growth after
    // metadata inspection cannot trigger an unbounded read_to_end allocation.
    let read_limit = limit
        .checked_add(1)
        .ok_or_else(|| anyhow!(proof_limit("file_bytes")))?;
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > file_bytes {
        return Err(anyhow!(proof_limit("file_bytes")));
    }
    Ok(bytes)
}

fn read_bounded_evidence_text(path: &Path, file_bytes: usize) -> Result<String> {
    Ok(String::from_utf8(read_bounded_proof_file(
        path, file_bytes,
    )?)?)
}

fn sealed_replay_bytes(
    dir: &Path,
    relative: &str,
    manifest: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let expected = manifest
        .get(relative)
        .ok_or_else(|| anyhow!("replay artifact is not manifest-covered: {relative}"))?;
    let path = dir.join(relative);
    if !std::fs::symlink_metadata(&path)?.file_type().is_file()
        || !path.canonicalize()?.starts_with(dir.canonicalize()?)
    {
        return Err(anyhow!(
            "replay artifact is not a contained regular file: {relative}"
        ));
    }
    let bytes = read_bounded_proof_file(&path, RUP_LIMITS.file_bytes)?;
    if hex::encode(Sha256::digest(&bytes)) != *expected {
        return Err(anyhow!("replay artifact hash mismatch: {relative}"));
    }
    Ok(bytes)
}

/// Bind each certificate to its inventory position and published SMT before
/// accepting RUP replay. Regeneration uses the same implementation as emission:
/// this checks consistency, not independent SMT-to-CNF correspondence.
fn check_published_refutation(
    dir: &Path,
    manifest: &std::collections::BTreeMap<String, String>,
    index: usize,
    check: &anubis_compiler::middle::SolverCheck,
    proof: &ReplayProofRow,
) -> Result<bool> {
    let stem = format!("analysis/proofs/obligation_{index:04}");
    let smt_path = format!("{stem}.smt2");
    if proof.obligation != check.name || proof.status != check.status || proof.smt != smt_path {
        return Err(anyhow!("proof identity mismatch at obligation {index}"));
    }
    if sealed_replay_bytes(dir, &smt_path, manifest)? != check.smt.as_bytes() {
        return Err(anyhow!("proof SMT binding mismatch at obligation {index}"));
    }
    if proof.proof != "rup_refutation" {
        if proof.cnf_dimacs.is_some() || proof.proof_drat.is_some() {
            return Err(anyhow!(
                "non-RUP row carries certificate paths at obligation {index}"
            ));
        }
        return Ok(false);
    }
    if check.status != "PASS"
        || check.name == "solver:no-obligations"
        || check.detail == anubis_compiler::middle::NO_OBLIGATIONS_DETAIL
        || unreplayed_proof_kind(&check.detail).is_some()
    {
        return Err(anyhow!(
            "RUP proof provenance mismatch at obligation {index}"
        ));
    }
    let cnf_path = format!("{stem}.cnf");
    let drat_path = format!("{stem}.drat");
    if proof.cnf_dimacs.as_deref() != Some(cnf_path.as_str())
        || proof.proof_drat.as_deref() != Some(drat_path.as_str())
    {
        return Err(anyhow!(
            "noncanonical or missing certificate paths at obligation {index}"
        ));
    }
    let cnf = sealed_replay_bytes(dir, &cnf_path, manifest)?;
    let drat = sealed_replay_bytes(dir, &drat_path, manifest)?;
    let Some((anubis_solver::NativeVerdict::Unsat, Some(regenerated))) =
        anubis_solver::native_prove_with_artifacts(&check.smt)
    else {
        return Err(anyhow!(
            "SMT-to-CNF consistency could not be re-established at obligation {index}: \
             bounded native regeneration returned no UNSAT certificate"
        ));
    };
    if regenerated.smt != check.smt || regenerated.cnf_dimacs.as_bytes() != cnf {
        return Err(anyhow!(
            "published CNF differs from native regeneration of bound SMT at obligation {index}"
        ));
    }
    // Different valid refutations of the same CNF remain acceptable. Do not
    // require the current solver to emit byte-identical DRAT search history.
    check_rup(std::str::from_utf8(&cnf)?, std::str::from_utf8(&drat)?)
        .map_err(|reason| anyhow!("RUP refutation rejected at obligation {index}: {reason}"))?;
    Ok(true)
}

fn unreplayed_proof_kind(detail: &str) -> Option<&'static str> {
    use anubis_compiler::middle;
    match detail {
        middle::UNRESOLVED_PRECONDITION_DETAIL => Some("unresolved_not_encoded"),
        middle::OVERAPPROX_UNDECIDED_DETAIL => Some("undecided_overapproximated"),
        middle::BRANCH_REACHABILITY_UNDECIDED_DETAIL => Some("undecided_branch_reachability"),
        middle::OVERAPPROX_COMBINED_UNDECIDED_DETAIL => {
            Some("undecided_value_and_branch_reachability")
        }
        _ => None,
    }
}

fn check_solver_replay<F>(dir: &Path, mut replay: F) -> Result<SolverReplayVerification>
where
    F: FnMut(&str, &str) -> bool,
{
    use anubis_compiler::middle;
    if !dir.join("analysis/solver_replay.json").try_exists()? {
        return Ok(SolverReplayVerification::Absent);
    }
    let manifest = replay_manifest(dir)?;
    let bytes = sealed_replay_bytes(dir, "analysis/solver_replay.json", &manifest)?;
    let header: serde_json::Value = serde_json::from_slice(&bytes)?;
    if header.get("schema").is_none() {
        return Ok(SolverReplayVerification::LegacyUnverified);
    }
    // Parse the original bytes, not the Value: duplicate typed fields must fail.
    let record: SolverReplayRecord = serde_json::from_slice(&bytes)?;
    if record.schema != "anubis-solver-replay-v1" || record.scope != "all_solver_checks" {
        return Err(anyhow!("unsupported solver replay schema or scope"));
    }
    let checks: Vec<middle::SolverCheck> =
        serde_json::from_slice(&sealed_replay_bytes(dir, "solver.json", &manifest)?)?;
    let proofs: ReplayProofIndex = serde_json::from_slice(&sealed_replay_bytes(
        dir,
        "analysis/proofs.json",
        &manifest,
    )?)?;
    let pca: ReplayPcaSummary =
        serde_json::from_slice(&sealed_replay_bytes(dir, "pca.json", &manifest)?)?;
    if checks.is_empty() {
        return Err(anyhow!(
            "versioned solver replay inventory is empty; no emitted solver check"
        ));
    }
    // The synthetic PASS is a reporting sentinel, not an obligation. Validate
    // both identifying fields before excluding it from the PCA's real count;
    // a forged name/detail must not hide an undecided or modeled check.
    for check in &checks {
        if (check.name == "solver:no-obligations" || check.detail == middle::NO_OBLIGATIONS_DETAIL)
            && (checks.len() != 1
                || check.name != "solver:no-obligations"
                || check.detail != middle::NO_OBLIGATIONS_DETAIL
                || check.status != "PASS"
                || check.model.is_some()
                || check.smt != "(check-sat)")
        {
            return Err(anyhow!("invalid synthetic no-obligations row"));
        }
    }
    if checks.len() != record.obligations.len() || checks.len() != proofs.obligations.len() {
        return Err(anyhow!(
            "solver replay, solver, and proof inventory counts differ"
        ));
    }
    let real_checks = || {
        checks
            .iter()
            .filter(|check| check.detail != middle::NO_OBLIGATIONS_DETAIL)
    };
    // PCA v2 counted the reporting sentinel; v3 counts real obligations only.
    // Preserve the declared version's exact convention, never accept either
    // count opportunistically. The sentinel was validated above in both cases.
    let expected_count = match pca.pca_version {
        2 => checks.len(),
        3 => real_checks().count(),
        version => {
            return Err(anyhow!(
                "unsupported PCA version for solver replay: {version}"
            ))
        }
    };
    if expected_count != pca.solver_obligations {
        return Err(anyhow!(
            "solver obligation count differs from declared PCA version"
        ));
    }
    // UNKNOWN is not discharge, even if an older producer's PCA counted it that way.
    if pca.solver_all_discharged != real_checks().all(|check| check.status == "PASS") {
        return Err(anyhow!(
            "solver inventory disagrees with PCA discharge status"
        ));
    }
    if let Some(first) = checks.first() {
        if sealed_replay_bytes(dir, "analysis/solver.smt2", &manifest)? != first.smt.as_bytes() {
            return Err(anyhow!(
                "legacy first-query SMT alias disagrees with solver inventory"
            ));
        }
    }

    let mut failed = false;
    let mut incomplete = false;
    let mut replayed = 0usize;
    for (index, ((check, row), proof)) in checks
        .iter()
        .zip(&record.obligations)
        .zip(&proofs.obligations)
        .enumerate()
    {
        let path = format!("analysis/proofs/obligation_{index:04}.smt2");
        if row.index != index
            || row.obligation != check.name
            || row.solver_status != check.status
            || row.detail != check.detail
            || row.smt != path
            || proof.obligation != check.name
            || proof.status != check.status
            || proof.smt != path
        {
            return Err(anyhow!(
                "solver replay/proof identity mismatch at obligation {index}"
            ));
        }
        if sealed_replay_bytes(dir, &path, &manifest)? != check.smt.as_bytes() {
            return Err(anyhow!(
                "published SMT differs from solver inventory at obligation {index}"
            ));
        }
        let synthetic = check.detail == middle::NO_OBLIGATIONS_DETAIL;
        let proof_kind_matches = if synthetic {
            match pca.pca_version {
                2 => proof.proof == "counterexample_no_refutation",
                3 => proof.proof == "not_applicable_no_obligations",
                _ => false,
            }
        } else if let Some(expected) = unreplayed_proof_kind(&check.detail) {
            proof.proof == expected
        } else {
            match proof.proof.as_str() {
                "rup_refutation" | "unsat_without_published_certificate" => check.status == "PASS",
                "refutation_not_accepted_by_check" => {
                    check.status != "PASS" && !middle::counterexample_was_replayed(check)
                }
                "counterexample_no_refutation" => check.status != "PASS",
                "declined_by_native_solver_deferred" => true,
                // Includes contradictory uncertainty provenance and unrecognized proof kinds.
                _ => false,
            }
        };
        if !proof_kind_matches {
            return Err(anyhow!("proof provenance mismatch at obligation {index}"));
        }
        check_published_refutation(dir, &manifest, index, check, proof)?;

        let mut attempted = false;
        let mut valid = false;
        let status = if check.detail == middle::UNRESOLVED_PRECONDITION_DETAIL {
            incomplete = true;
            "not_encoded"
        } else if check.detail == middle::OVERAPPROX_UNDECIDED_DETAIL {
            incomplete = true;
            "overapproximated_not_replayed"
        } else if check.detail == middle::BRANCH_REACHABILITY_UNDECIDED_DETAIL {
            incomplete = true;
            "branch_reachability_not_replayed"
        } else if check.detail == middle::OVERAPPROX_COMBINED_UNDECIDED_DETAIL {
            incomplete = true;
            "value_and_branch_reachability_not_replayed"
        } else if check.detail == middle::NO_OBLIGATIONS_DETAIL {
            "not_applicable_no_obligations"
        } else if check.status == "PASS" {
            "not_applicable"
        } else if check.status == "FAIL" {
            if let Some(model) = check.model.as_deref() {
                if middle::counterexample_was_replayed(check) {
                    attempted = true;
                    valid = replay(&check.smt, model);
                    if valid {
                        replayed += 1;
                        "counterexample_replayed"
                    } else {
                        failed = true;
                        "replay_failed"
                    }
                } else {
                    incomplete = true;
                    "not_replayed_untrusted_model"
                }
            } else {
                incomplete = true;
                "not_replayed_no_model"
            }
        } else if check.status == "UNKNOWN" {
            incomplete = true;
            "not_replayed_undecided"
        } else {
            incomplete = true;
            "not_replayed_unknown_status"
        };
        if row.status != status || row.replay_attempted != attempted || row.replay_valid != valid {
            return Err(anyhow!(
                "solver replay claim was not re-established at obligation {index}"
            ));
        }
    }
    let (status, valid) = if failed {
        ("replay_failed", false)
    } else if incomplete {
        ("incomplete", false)
    } else if replayed != 0 {
        ("counterexample_replayed", true)
    } else {
        ("not_applicable", false)
    };
    if record.status != status || record.replay_valid != valid {
        return Err(anyhow!(
            "solver replay aggregate disagrees with recomputed rows"
        ));
    }
    Ok(SolverReplayVerification::Current {
        rows: checks.len(),
        replayed,
        status,
        complete: !failed && !incomplete,
        synthetic_compatibility: pca.pca_version == 2
            && checks[0].detail == middle::NO_OBLIGATIONS_DETAIL,
    })
}

fn verify_solver_replay(dir: &Path, id_prefix: &str, report: &mut EvidenceVerifyReport) {
    let id = format!("{id_prefix}.solver_replay");
    match check_solver_replay(dir, anubis_compiler::middle::replay_counterexample) {
        Ok(SolverReplayVerification::Absent) => report.push(
            &id,
            CheckStatus::Fail,
            "NO_REPLAY_EVIDENCE",
            "No solver replay sidecar was emitted. A prior parse/typecheck rejection may \
             explain its absence; absence is not verified replay evidence.",
        ),
        Ok(SolverReplayVerification::LegacyUnverified) => report.push(
            &id,
            CheckStatus::Fail,
            "LEGACY_UNVERIFIED",
            "Unversioned solver replay data establishes no replay assurance. Historical \
             artifacts remain intact; removing a v1 schema cannot confer success.",
        ),
        Ok(SolverReplayVerification::Current {
            rows,
            replayed,
            status,
            complete,
            synthetic_compatibility,
        }) => report.push(
            &id,
            if complete {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            if synthetic_compatibility { "LEGACY_COMPATIBILITY" } else { "LAB_REAL" },
            format!(
                "solver-replay-v1: {rows} ordered rows bound to solver/proof/PCA artifacts; \
                 {replayed} model replay(s) re-established; status={status}. \
                 Published-artifact consistency using Anubis replay, not source-to-SMT \
                 correspondence or a program-discharge claim.{}",
                if synthetic_compatibility {
                    " PCA v2 synthetic counterexample tag accepted only as historical compatibility; no counterexample or proof credit."
                } else { "" }
            ),
        ),
        Err(error) => report.push(&id, CheckStatus::Fail, "LAB_REAL", error.to_string()),
    }
}

/// Verify all recognizable evidence artifacts under `path` (file or directory).
pub fn verify_path(path: &Path, opts: &EvidenceVerifyOpts) -> Result<EvidenceVerifyReport> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut report = EvidenceVerifyReport {
        schema: EVIDENCE_VERIFY_SCHEMA.into(),
        path: path.display().to_string(),
        ok: true,
        checks: Vec::new(),
        classifications_seen: Vec::new(),
        notes: vec![
            "Independent portable verifier: host-side, no VZ required.".into(),
            "HMAC checks are LAB_REAL_HMAC (not Ed25519 PKI).".into(),
            "PCA re-derive is LAB_REAL; signature is separate when present.".into(),
        ],
    };

    if !path.exists() {
        report.push(
            "path.exists",
            CheckStatus::Fail,
            "FAIL_CLOSED",
            format!("path does not exist: {}", path.display()),
        );
        return Ok(report);
    }

    // Single-file run capability.
    if path.is_file() {
        if looks_like_run_cap(&path) {
            verify_run_cap_file(&path, opts, &mut report);
            return Ok(report);
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "engagement.json")
            .unwrap_or(false)
        {
            if let Some(parent) = path.parent() {
                return verify_path(parent, opts);
            }
        }
        report.push(
            "path.kind",
            CheckStatus::Fail,
            "FAIL_CLOSED",
            "unrecognized evidence file (expected evidence bundle dir, engagement dir, or run_capability.json)",
        );
        return Ok(report);
    }

    // Directory: dispatch on contents.
    let mut found_any = false;

    if path.join("evidence.json").is_file() || path.join("unverified.json").is_file() {
        found_any = true;
        verify_evidence_bundle(&path, opts, &mut report);
    }

    if path.join("engagement.json").is_file() {
        found_any = true;
        verify_engagement_dir(&path, opts, &mut report);
    }

    // Standalone confinement (not inside a full engagement).
    if path.join(CONFINEMENT_FILENAME).is_file()
        && (path.join("source.anubis").is_file() || path.join("source.anb").is_file())
        && !path.join("evidence.json").is_file()
    {
        found_any = true;
        verify_confinement_pair(&path, &mut report);
    }

    // Nested common locations.
    for candidate in [
        path.join("evidence/run_capability.json"),
        path.join("run_capability.json"),
        path.join("evidence").join(CONFINEMENT_FILENAME),
    ] {
        if candidate.is_file() && looks_like_run_cap(&candidate) {
            found_any = true;
            verify_run_cap_file(&candidate, opts, &mut report);
        }
    }

    // Nested evidence bundles under evidence-*
    if let Ok(rd) = std::fs::read_dir(&path) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("evidence-") || n == "evidence")
                    .unwrap_or(false)
                && (p.join("evidence.json").is_file() || p.join("unverified.json").is_file())
            {
                found_any = true;
                verify_evidence_bundle(&p, opts, &mut report);
            }
        }
    }

    if !found_any {
        report.push(
            "discovery",
            CheckStatus::Fail,
            "FAIL_CLOSED",
            "no verifiable artifacts found (looked for evidence.json, engagement.json, run_capability.json, confinement_manifest.json)",
        );
    }

    Ok(report)
}

fn looks_like_run_cap(path: &Path) -> bool {
    const MAX_RUN_CAP_SNIFF_BYTES: usize = 1024 * 1024;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("run_cap") || name.contains("run-cap") {
        return true;
    }
    // Peek schema.
    if let Ok(raw) = read_bounded_evidence_text(path, MAX_RUN_CAP_SNIFF_BYTES) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if v.get("schema")
                .and_then(|s| s.as_str())
                .map(|s| s.starts_with("anubis-run-cap"))
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

fn verify_evidence_bundle(
    dir: &Path,
    opts: &EvidenceVerifyOpts,
    report: &mut EvidenceVerifyReport,
) {
    let id_prefix = format!("bundle:{}", short_path(dir));

    if dir.join("unverified.json").is_file() {
        if opts.pubkey.is_some() {
            report.push(
                &format!("{id_prefix}.signature"),
                CheckStatus::Fail,
                "UNVERIFIED",
                "--pubkey cannot be satisfied by an unsigned UNVERIFIED envelope",
            );
        }
        match crate_verify_unverified(dir) {
            Ok(true) => {
                report.push(
                    &format!("{id_prefix}.unverified_integrity"),
                    CheckStatus::Pass,
                    "UNVERIFIED",
                    "UNVERIFIED integrity envelope hashes match",
                );
                report.push(
                    &format!("{id_prefix}.assurance"),
                    CheckStatus::Fail,
                    "UNVERIFIED",
                    "no contract, proof, or verified artifact claim is present",
                );
            }
            Ok(false) => report.push(
                &format!("{id_prefix}.unverified"),
                CheckStatus::Fail,
                "UNVERIFIED",
                "UNVERIFIED integrity envelope failed",
            ),
            Err(e) => report.push(
                &format!("{id_prefix}.unverified"),
                CheckStatus::Fail,
                "UNVERIFIED",
                e.to_string(),
            ),
        }
        return;
    }

    let pca_result = verify_pca(dir);
    match &pca_result {
        Ok(true) => match pca_claim_disclosure(dir) {
            Ok(disclosure) => report.push(
                &format!("{id_prefix}.pca"),
                CheckStatus::Pass,
                "LAB_REAL",
                format!("PCA re-derived and matched recorded claim; manifest hashes match the bundle; {disclosure}. Verification confirms the recorded claim, including a recorded FAIL; signature status is separate."),
            ),
            Err(error) => report.push(
                &format!("{id_prefix}.pca"),
                CheckStatus::Fail,
                "LAB_REAL",
                format!("PCA metadata disclosure failed: {error}"),
            ),
        },
        Ok(false) => report.push(
            &format!("{id_prefix}.pca"),
            CheckStatus::Fail,
            "LAB_REAL",
            "PCA re-derive or hash validation failed (tamper or dishonest claim)",
        ),
        Err(e) => report.push(
            &format!("{id_prefix}.pca"),
            CheckStatus::Fail,
            "LAB_REAL",
            format!("PCA verify error: {e}"),
        ),
    }

    if matches!(pca_result, Ok(true)) {
        match crate::pca_claims_zk(dir) {
            Ok(true) => match crate::verify_zk_claim_if_present(dir) {
                Ok(()) => report.push(
                    &format!("{id_prefix}.zk"),
                    CheckStatus::Pass,
                    "LAB_REAL",
                    "claimed ZK receipt cryptographically verified against the bundled guest image",
                ),
                Err(error) => report.push(
                    &format!("{id_prefix}.zk"),
                    CheckStatus::Fail,
                    "UNVERIFIED_ZK",
                    format!("claimed ZK receipt was not verified: {error}"),
                ),
            },
            Ok(false) => {}
            Err(error) => report.push(
                &format!("{id_prefix}.zk"),
                CheckStatus::Fail,
                "UNVERIFIED_ZK",
                format!("cannot read ZK claim: {error}"),
            ),
        }
    }

    match pca_signature_status(dir) {
        Ok(Some((true, signer))) => {
            let mut ok = true;
            if let Some(expected) = &opts.pubkey {
                if signer.trim() != expected.trim() {
                    ok = false;
                    report.push(
                        &format!("{id_prefix}.sig"),
                        CheckStatus::Fail,
                        "LAB_REAL",
                        format!("Ed25519 signature present but signer mismatch (got {signer})"),
                    );
                }
            }
            if ok {
                report.push(
                    &format!("{id_prefix}.sig"),
                    CheckStatus::Pass,
                    "LAB_REAL",
                    format!("Ed25519 PCA signature valid (signer {signer})"),
                );
            }
        }
        Ok(Some((false, signer))) => report.push(
            &format!("{id_prefix}.sig"),
            CheckStatus::Fail,
            "LAB_REAL",
            format!("Ed25519 signature invalid (claimed signer {signer})"),
        ),
        Ok(None) => {
            if opts.pubkey.is_some() || opts.strict {
                report.push(
                    &format!("{id_prefix}.sig"),
                    CheckStatus::Fail,
                    "UNSIGNED",
                    "PCA unsigned but --pubkey/--strict requires a signature",
                );
            } else {
                report.push(
                    &format!("{id_prefix}.sig"),
                    CheckStatus::Skip,
                    "UNSIGNED",
                    "PCA unsigned (hash/PCA re-derive still apply)",
                );
            }
        }
        Err(e) => report.push(
            &format!("{id_prefix}.sig"),
            CheckStatus::Fail,
            "LAB_REAL",
            format!("signature status error: {e}"),
        ),
    }

    verify_published_proofs(dir, &id_prefix, report);
    verify_solver_replay(dir, &id_prefix, report);

    // Confinement re-derive when sealed alongside source.
    let conf_path = dir.join(CONFINEMENT_FILENAME);
    let source = if dir.join("source.anubis").is_file() {
        Some(dir.join("source.anubis"))
    } else if dir.join("source.anb").is_file() {
        Some(dir.join("source.anb"))
    } else {
        None
    };
    if conf_path.is_file() {
        if let Some(src_path) = source {
            match (
                read_bounded_evidence_text(&src_path, RUP_LIMITS.file_bytes),
                read_bounded_evidence_text(&conf_path, RUP_LIMITS.file_bytes),
            ) {
                (Ok(src), Ok(raw)) => match serde_json::from_str::<ConfinementManifest>(&raw) {
                    Ok(sealed) => {
                        match confinement::verify_confinement_matches_source(&src, &sealed) {
                            Ok(()) => report.push(
                                &format!("{id_prefix}.confinement"),
                                CheckStatus::Pass,
                                "LAB_REAL",
                                "confinement_manifest re-derives from source (no grant drift)",
                            ),
                            Err(e) => report.push(
                                &format!("{id_prefix}.confinement"),
                                CheckStatus::Fail,
                                "LAB_REAL",
                                e,
                            ),
                        }
                    }
                    Err(e) => report.push(
                        &format!("{id_prefix}.confinement"),
                        CheckStatus::Fail,
                        "LAB_REAL",
                        format!("confinement JSON parse: {e}"),
                    ),
                },
                (Err(e), _) | (_, Err(e)) => report.push(
                    &format!("{id_prefix}.confinement"),
                    CheckStatus::Fail,
                    "LAB_REAL",
                    format!("read source/confinement: {e}"),
                ),
            }
        } else {
            report.push(
                &format!("{id_prefix}.confinement"),
                CheckStatus::Skip,
                "PARTIAL",
                "confinement_manifest present but no source.anubis/source.anb to re-derive against",
            );
        }
    }
}

fn crate_verify_unverified(dir: &Path) -> Result<bool> {
    // Keep the public evidence-verify command on the same typed gate as
    // `verify` and `validate`; no weaker manifest-only verifier remains.
    crate::verify_unverified_build_evidence(dir)
}

fn verify_engagement_dir(dir: &Path, opts: &EvidenceVerifyOpts, report: &mut EvidenceVerifyReport) {
    let id_prefix = format!("engagement:{}", short_path(dir));

    match crate::offensive::engagement::load_engagement(dir) {
        Ok(eng) => match eng.verify_content_hash() {
            Ok(()) => report.push(
                &format!("{id_prefix}.content_hash"),
                CheckStatus::Pass,
                "LAB_REAL",
                format!(
                    "engagement content_hash matches body (id={})",
                    eng.engagement_id
                ),
            ),
            Err(e) => report.push(
                &format!("{id_prefix}.content_hash"),
                CheckStatus::Fail,
                "LAB_REAL",
                e.to_string(),
            ),
        },
        Err(e) => report.push(
            &format!("{id_prefix}.load"),
            CheckStatus::Fail,
            "LAB_REAL",
            format!("load engagement: {e}"),
        ),
    }

    match crate::offensive::receipts::verify_chain(dir) {
        Ok(v) => {
            let ok = v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false);
            let count = v.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
            let empty = v.get("empty").and_then(|b| b.as_bool()).unwrap_or(false);
            if ok {
                report.push(
                    &format!("{id_prefix}.receipts"),
                    CheckStatus::Pass,
                    "LAB_REAL_HMAC",
                    if empty {
                        "receipt chain empty (ok); MAC required when key present".into()
                    } else {
                        format!("receipt chain ok count={count} (hash+HMAC when keyed)")
                    },
                );
            } else {
                report.push(
                    &format!("{id_prefix}.receipts"),
                    CheckStatus::Fail,
                    "LAB_REAL_HMAC",
                    format!("receipt chain reported ok=false: {v}"),
                );
            }
        }
        Err(e) => report.push(
            &format!("{id_prefix}.receipts"),
            CheckStatus::Fail,
            "LAB_REAL_HMAC",
            e.to_string(),
        ),
    }

    // Optional nested evidence / run cap under engagement.
    let loot = dir.join("evidence");
    if loot.is_dir() {
        if loot.join("evidence.json").is_file() {
            verify_evidence_bundle(&loot, opts, report);
        }
        // Child evidence-* dirs
        if let Ok(rd) = std::fs::read_dir(&loot) {
            for ent in rd.flatten() {
                let p = ent.path();
                if p.is_dir() && p.join("evidence.json").is_file() {
                    verify_evidence_bundle(&p, opts, report);
                }
            }
        }
        let cap = loot.join("run_capability.json");
        if cap.is_file() {
            verify_run_cap_file(&cap, opts, report);
        }
    }
}

fn verify_confinement_pair(dir: &Path, report: &mut EvidenceVerifyReport) {
    let conf_path = dir.join(CONFINEMENT_FILENAME);
    let src_path = if dir.join("source.anubis").is_file() {
        dir.join("source.anubis")
    } else {
        dir.join("source.anb")
    };
    let id = format!("confinement:{}", short_path(dir));
    match (
        read_bounded_evidence_text(&src_path, RUP_LIMITS.file_bytes),
        read_bounded_evidence_text(&conf_path, RUP_LIMITS.file_bytes),
    ) {
        (Ok(src), Ok(raw)) => match serde_json::from_str::<ConfinementManifest>(&raw) {
            Ok(sealed) => match confinement::verify_confinement_matches_source(&src, &sealed) {
                Ok(()) => report.push(
                    &id,
                    CheckStatus::Pass,
                    "LAB_REAL",
                    "confinement re-derives from source",
                ),
                Err(e) => report.push(&id, CheckStatus::Fail, "LAB_REAL", e),
            },
            Err(e) => report.push(&id, CheckStatus::Fail, "LAB_REAL", format!("parse: {e}")),
        },
        (Err(e), _) | (_, Err(e)) => {
            report.push(&id, CheckStatus::Fail, "LAB_REAL", format!("read: {e}"))
        }
    }
}

fn verify_run_cap_file(path: &Path, opts: &EvidenceVerifyOpts, report: &mut EvidenceVerifyReport) {
    let id = format!("run_cap:{}", short_path(path));
    match crate::offensive::run_capability::read_cap(path) {
        Ok(cap) => {
            match crate::offensive::run_capability::verify_offline_structural(&cap) {
                Ok(()) => report.push(
                    &format!("{id}.structure"),
                    CheckStatus::Pass,
                    "LAB_REAL_HMAC",
                    format!(
                        "schema ok guest={} effects={} expires_unix={}",
                        cap.guest_id,
                        cap.allowed_effects.len(),
                        cap.expires_unix
                    ),
                ),
                Err(e) => report.push(
                    &format!("{id}.structure"),
                    CheckStatus::Fail,
                    "LAB_REAL_HMAC",
                    e.to_string(),
                ),
            }
            match &opts.run_cap_key {
                Some(key) => {
                    match crate::offensive::run_capability::verify_offline_mac(&cap, key) {
                        Ok(()) => report.push(
                            &format!("{id}.mac"),
                            CheckStatus::Pass,
                            "LAB_REAL_HMAC",
                            "run-capability MAC valid (HMAC over sealed fields; not Ed25519)",
                        ),
                        Err(e) => report.push(
                            &format!("{id}.mac"),
                            CheckStatus::Fail,
                            "LAB_REAL_HMAC",
                            e.to_string(),
                        ),
                    }
                }
                None => {
                    let env_key = std::env::var("ANUBIS_RUN_CAP_KEY").ok();
                    if let Some(key) = env_key {
                        match crate::offensive::run_capability::verify_offline_mac(&cap, &key) {
                            Ok(()) => report.push(
                                &format!("{id}.mac"),
                                CheckStatus::Pass,
                                "LAB_REAL_HMAC",
                                "run-capability MAC valid via ANUBIS_RUN_CAP_KEY",
                            ),
                            Err(e) => report.push(
                                &format!("{id}.mac"),
                                CheckStatus::Fail,
                                "LAB_REAL_HMAC",
                                e.to_string(),
                            ),
                        }
                    } else if opts.strict {
                        report.push(
                            &format!("{id}.mac"),
                            CheckStatus::Fail,
                            "LAB_REAL_HMAC",
                            "strict: run capability present but no --run-cap-key / ANUBIS_RUN_CAP_KEY",
                        );
                    } else {
                        report.push(
                            &format!("{id}.mac"),
                            CheckStatus::Skip,
                            "LAB_REAL_HMAC",
                            "MAC not checked (pass --run-cap-key or ANUBIS_RUN_CAP_KEY); structure only",
                        );
                    }
                }
            }
        }
        Err(e) => report.push(
            &format!("{id}.load"),
            CheckStatus::Fail,
            "LAB_REAL_HMAC",
            e.to_string(),
        ),
    }
}

fn short_path(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("?"))
        .to_string()
}

/// Human-readable summary lines for CLI. Every part that can quote a bundle's own text (a path,
/// a check's detail, a classification, a note) is shown through `printable`: a stranger's bundle
/// must not drive the terminal (eighth review of the checker limits, E4).
pub fn format_human(report: &EvidenceVerifyReport) -> String {
    use anubis_compiler::diagnostics::printable;
    let mut lines = Vec::new();
    lines.push(format!(
        "anubis evidence-verify: {}",
        printable(&report.path)
    ));
    lines.push(format!(
        "overall: {}  checks={}",
        if report.ok { "PASS" } else { "FAIL" },
        report.checks.len()
    ));
    for c in &report.checks {
        let mark = match c.status {
            CheckStatus::Pass => "PASS",
            CheckStatus::Fail => "FAIL",
            CheckStatus::Skip => "SKIP",
        };
        lines.push(format!(
            "  [{mark}] {} ({}) — {}",
            printable(&c.id),
            printable(&c.classification.to_string()),
            printable(&c.detail)
        ));
    }
    if !report.classifications_seen.is_empty() {
        lines.push(format!(
            "classifications: {}",
            printable(&report.classifications_seen.join(", "))
        ));
    }
    for n in &report.notes {
        lines.push(format!("note: {}", printable(n)));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::run_capability;
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[test]
    fn missing_path_fails_closed() {
        let r = verify_path(
            Path::new("/tmp/anubis-no-such-evidence-xyz"),
            &EvidenceVerifyOpts::default(),
        )
        .unwrap();
        assert!(!r.ok);
        assert!(r.checks.iter().any(|c| c.status == CheckStatus::Fail));
    }

    #[test]
    fn run_cap_structure_and_mac() {
        let dir = tempfile::tempdir().unwrap();
        let key = "unit-test-run-cap-key-32b!!!!!";
        let cap = run_capability::mint(run_capability::MintParams {
            key,
            engagement_id: "eng-1",
            engagement_hash: "eh-1",
            authorization_digest: "auth-1",
            source_digest: "src-1",
            compiler_digest: "comp-1",
            program_digest: "prog-1",
            guest_id: "guest-1",
            base_digest: "base-1",
            confinement_digest: "conf-1",
            allowed_effects: vec!["process.spawn".into(), "vm.execute".into()],
            allowed_targets: vec![],
            operator: "op",
            ttl_secs: 600,
        });
        let p = dir.path().join("run_capability.json");
        run_capability::write_cap(&p, &cap).unwrap();

        let opts = EvidenceVerifyOpts {
            run_cap_key: Some(key.into()),
            ..Default::default()
        };
        let r = verify_path(&p, &opts).unwrap();
        assert!(r.ok, "{:?}", r.checks);
        assert!(r
            .checks
            .iter()
            .any(|c| c.id.contains("mac") && c.status == CheckStatus::Pass));

        let bad = EvidenceVerifyOpts {
            run_cap_key: Some("wrong-key".into()),
            ..Default::default()
        };
        let r2 = verify_path(&p, &bad).unwrap();
        assert!(!r2.ok);
    }

    #[test]
    fn engagement_content_hash_and_empty_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let eng_dir = dir.path().join("eng");
        crate::offensive::engagement::engage_init(
            &eng_dir,
            "lab-verify",
            "auth charter for unit test",
        )
        .expect("engage_init");
        let eng = crate::offensive::engagement::load_engagement(&eng_dir).unwrap();
        eng.verify_content_hash().unwrap();

        let r = verify_path(&eng_dir, &EvidenceVerifyOpts::default()).unwrap();
        assert!(r.ok, "{:?}", r.checks);
        assert!(r
            .checks
            .iter()
            .any(|c| c.id.contains("content_hash") && c.status == CheckStatus::Pass));
        assert!(r
            .checks
            .iter()
            .any(|c| c.id.contains("receipts") && c.status == CheckStatus::Pass));
    }

    #[test]
    fn confinement_pair_detects_forge() {
        let dir = tempfile::tempdir().unwrap();
        let src = "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\n\
                   fn main() uses(net.send) { beacon(); }\n";
        std::fs::write(dir.path().join("source.anubis"), src).unwrap();
        let m = confinement::derive_confinement("pkg", "0.0.0", src).unwrap();
        std::fs::write(
            dir.path().join(CONFINEMENT_FILENAME),
            serde_json::to_string_pretty(&m).unwrap(),
        )
        .unwrap();
        let r = verify_path(dir.path(), &EvidenceVerifyOpts::default()).unwrap();
        assert!(r.ok, "{:?}", r.checks);

        // Forge grant
        let mut forged = m;
        for g in &mut forged.grants {
            if g.capability == "net.send" {
                g.hypervisor_grant = "network:host-only".into();
            }
        }
        std::fs::write(
            dir.path().join(CONFINEMENT_FILENAME),
            serde_json::to_string_pretty(&forged).unwrap(),
        )
        .unwrap();
        let r2 = verify_path(dir.path(), &EvidenceVerifyOpts::default()).unwrap();
        assert!(!r2.ok);
    }

    #[test]
    fn empty_dir_fails_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let r = verify_path(dir.path(), &EvidenceVerifyOpts::default()).unwrap();
        assert!(!r.ok);
        assert!(r.checks.iter().any(|c| c.id == "discovery"));
    }

    #[allow(dead_code)]
    fn _silence_mutex() {
        let _ = Mutex::new(HashSet::<String>::new());
    }
}

#[cfg(test)]
mod proof_replay_tests {
    use super::{
        check_rup, check_rup_with_limits, propagate, read_bounded_proof_file, RupLimits, RupWork,
        RUP_LIMITS,
    };

    // (x | y) & (x | !y) & (!x | y) & (!x | !y) is unsatisfiable.
    const UNSAT_CNF: &str = "p cnf 2 4\n1 2 0\n1 -2 0\n-1 2 0\n-1 -2 0\n";
    // Resolve to the two units, then the empty clause.
    const GOOD_DRAT: &str = "1 0\n-1 0\n0\n";

    #[test]
    fn an_honest_refutation_replays() {
        assert_eq!(check_rup(UNSAT_CNF, GOOD_DRAT).unwrap(), 3);
    }

    #[test]
    fn the_documented_forgery_is_rejected() {
        // The exact attack from the audit: swap the refutation for a plausible
        // two-line file. Recomputing MANIFEST.sha256 hides it from a hash check,
        // so this is the only thing standing between a forgery and a PASS.
        let err = check_rup(UNSAT_CNF, "1 2 0\n0\n").unwrap_err();
        assert!(err.contains("not implied"), "unexpected reason: {err}");
    }

    #[test]
    fn a_truncated_refutation_is_rejected() {
        let err = check_rup(UNSAT_CNF, "1 0\n-1 0\n").unwrap_err();
        assert!(err.contains("empty clause"), "unexpected reason: {err}");
    }

    #[test]
    fn an_empty_refutation_is_rejected() {
        assert!(check_rup(UNSAT_CNF, "").is_err());
    }

    #[test]
    fn a_satisfiable_formula_cannot_be_refuted() {
        // Nothing may derive the empty clause from a formula with a model.
        assert!(check_rup("p cnf 2 1\n1 2 0\n", "0\n").is_err());
    }

    #[test]
    fn comments_are_supported_but_deletions_are_explicitly_unsupported() {
        let cnf = format!("c CNF comment\n{UNSAT_CNF}");
        let proof = format!("c proof comment\n{GOOD_DRAT}c final comment\n");
        assert_eq!(check_rup(&cnf, &proof).unwrap(), 3);
        let deletion = format!("{GOOD_DRAT}d 1 2 0\n");
        assert!(check_rup(UNSAT_CNF, &deletion)
            .unwrap_err()
            .contains("ANUBIS_PROOF_UNSUPPORTED: DRAT deletion steps"));
    }

    #[test]
    fn malformed_cnf_headers_counts_and_clauses_are_rejected() {
        for cnf in [
            "",
            "1 0\n",
            "p cnf 1\n",
            "p cnf -1 1\n0\n",
            "p cnf 1 1 extra\n0\n",
            "p cnf 1 1\np cnf 1 1\n0\n",
            "p cnf 1 1\n",
            "p cnf 1 0\n0\n",
            "p cnf 1 1\n1\n",
            "p cnf 1 1\n1 junk 0\n",
            "p cnf 1 1\n0 junk\n",
            "p cnf 1 1\n0 0\n",
            "p cnf 1 1\n2 0\n",
            "p cnf 0 1\n1 0\n",
            "p cnf 1 1\nd 1 0\n",
            "p cnf 1 1\n-0\n",
        ] {
            let error = check_rup(cnf, "0\n").unwrap_err();
            assert!(
                error.starts_with("ANUBIS_PROOF_INVALID:"),
                "{cnf:?}: {error}"
            );
        }
    }

    #[test]
    fn malformed_proof_suffix_cannot_hide_behind_an_accepted_empty_clause() {
        for suffix in [
            "1 2\n",
            "nonsense\n",
            "1 junk 0\n",
            "0 trailing\n",
            "0 0\n",
            "p cnf 2 0\n",
            "1 0\n",
        ] {
            let proof = format!("{GOOD_DRAT}{suffix}");
            let error = check_rup(UNSAT_CNF, &proof).unwrap_err();
            assert!(
                error.starts_with("ANUBIS_PROOF_INVALID:"),
                "{suffix:?}: {error}"
            );
        }
    }

    #[test]
    fn proof_literals_are_bounded_by_the_cnf_declaration_before_allocation() {
        for literal in [i32::MIN, i32::MAX, 3, -3] {
            let proof = format!("{literal} 0\n0\n");
            let error = check_rup(UNSAT_CNF, &proof).unwrap_err();
            assert!(
                error.contains("literal outside declared CNF variable bound"),
                "{error}"
            );
        }
        let proof = "999999999999999999999999999999999999 0\n0\n";
        assert!(check_rup(UNSAT_CNF, proof)
            .unwrap_err()
            .contains("literal outside supported integer range"));
        assert!(check_rup("p cnf 0 1\n0\n", "0\n").is_ok());
        assert!(check_rup("p cnf 0 1\n0\n", "1 0\n0\n").is_err());
        let oversized = format!("p cnf {} 0\n", usize::MAX);
        assert_eq!(
            check_rup(&oversized, "0\n").unwrap_err(),
            "ANUBIS_PROOF_LIMIT: variables"
        );
        // Even an internal test override cannot turn header overflow into an allocation.
        let limits = RupLimits {
            variables: usize::MAX,
            ..RUP_LIMITS
        };
        assert_eq!(
            check_rup_with_limits(&oversized, "0\n", limits).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: variables"
        );
    }

    #[test]
    fn fixed_resource_ceilings_refuse_with_named_limits() {
        for (limits, name) in [
            (
                RupLimits {
                    file_bytes: 1,
                    ..RUP_LIMITS
                },
                "file_bytes",
            ),
            (
                RupLimits {
                    line_bytes: 1,
                    ..RUP_LIMITS
                },
                "line_bytes",
            ),
            (
                RupLimits {
                    lines: 1,
                    ..RUP_LIMITS
                },
                "lines",
            ),
            (
                RupLimits {
                    variables: 1,
                    ..RUP_LIMITS
                },
                "variables",
            ),
            (
                RupLimits {
                    cnf_clauses: 1,
                    ..RUP_LIMITS
                },
                "cnf_clauses",
            ),
            (
                RupLimits {
                    proof_steps: 1,
                    ..RUP_LIMITS
                },
                "proof_steps",
            ),
            (
                RupLimits {
                    literals: 1,
                    ..RUP_LIMITS
                },
                "literals",
            ),
            (
                RupLimits {
                    work: 0,
                    ..RUP_LIMITS
                },
                "work",
            ),
        ] {
            assert_eq!(
                check_rup_with_limits(UNSAT_CNF, GOOD_DRAT, limits).unwrap_err(),
                format!("ANUBIS_PROOF_LIMIT: {name}")
            );
        }
        let cnf = "p cnf 0 1\n0\n";
        let limits = RupLimits {
            line_bytes: 16,
            ..RUP_LIMITS
        };
        let proof = format!("c {}\n0\n", "x".repeat(limits.line_bytes));
        assert_eq!(
            check_rup_with_limits(cnf, &proof, limits).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: line_bytes"
        );
        let limits = RupLimits {
            lines: 2,
            ..RUP_LIMITS
        };
        assert_eq!(
            check_rup_with_limits(cnf, "\n\n0\n", limits).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: lines"
        );
        let limits = RupLimits {
            file_bytes: cnf.len(),
            ..RUP_LIMITS
        };
        let proof = format!("c {}\n0\n", "x".repeat(limits.file_bytes));
        assert_eq!(
            check_rup_with_limits(cnf, &proof, limits).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: file_bytes"
        );
    }

    #[test]
    fn work_meter_counts_assignment_initialization_and_propagation_visits() {
        let cnf = "p cnf 1 1\n0\n";
        let limits = RupLimits {
            work: 2,
            ..RUP_LIMITS
        };
        assert_eq!(
            check_rup_with_limits(cnf, "0\n", limits).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: work"
        );
        let mut assign = [0, 0];
        let mut work = RupWork { remaining: 0 };
        assert_eq!(
            propagate(&[vec![]], &mut assign, &mut work).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: work"
        );
        let mut work = RupWork { remaining: 1 };
        assert_eq!(
            propagate(&[vec![1]], &mut assign, &mut work).unwrap_err(),
            "ANUBIS_PROOF_LIMIT: work"
        );
    }

    #[test]
    fn artifact_reader_refuses_oversized_files_before_loading_them() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("proof.drat");
        std::fs::write(&path, GOOD_DRAT).unwrap();
        assert_eq!(
            read_bounded_proof_file(&path, GOOD_DRAT.len()).unwrap(),
            GOOD_DRAT.as_bytes()
        );
        assert!(read_bounded_proof_file(&path, 1)
            .unwrap_err()
            .to_string()
            .contains("ANUBIS_PROOF_LIMIT: file_bytes"));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(
            u64::try_from(RUP_LIMITS.file_bytes)
                .unwrap()
                .checked_add(1)
                .unwrap(),
        )
        .unwrap();
        assert!(read_bounded_proof_file(&path, RUP_LIMITS.file_bytes)
            .unwrap_err()
            .to_string()
            .contains("ANUBIS_PROOF_LIMIT: file_bytes"));
    }
}

#[cfg(test)]
mod solver_replay_tests {
    use super::*;
    use anubis_compiler::evidence::{build_evidence_bundle, refresh_manifest_hashes};
    use anubis_compiler::middle;
    use serde_json::{json, Value};

    fn bundle(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(source, "safe", None, vec![], root.path(), None, None)
            .expect("finite Safe evidence fixture");
        (root, bundle.dir)
    }

    fn read(dir: &Path, relative: &str) -> Value {
        serde_json::from_slice(&std::fs::read(dir.join(relative)).unwrap()).unwrap()
    }

    fn write(dir: &Path, relative: &str, value: &Value) {
        std::fs::write(
            dir.join(relative),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
        refresh_manifest_hashes(dir).unwrap();
    }

    #[test]
    fn replay_manifest_accepts_a_sealed_relative_filename_with_spaces() {
        let (_root, dir) = bundle("fn main() { let x = 1; assert(x == 1); }");
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        std::fs::write(dir.join("notes/read me.txt"), b"sealed note").unwrap();
        refresh_manifest_hashes(&dir).unwrap();
        let entries = replay_manifest(&dir).expect("producer manifest grammar");
        assert!(entries.contains_key("notes/read me.txt"));
        assert!(report(&dir).ok);
    }

    fn empty_report(dir: &Path) -> EvidenceVerifyReport {
        EvidenceVerifyReport {
            schema: EVIDENCE_VERIFY_SCHEMA.into(),
            path: dir.display().to_string(),
            ok: true,
            checks: vec![],
            classifications_seen: vec![],
            notes: vec![],
        }
    }

    fn report(dir: &Path) -> EvidenceVerifyReport {
        let mut report = empty_report(dir);
        verify_solver_replay(dir, "fixture", &mut report);
        report
    }

    fn proof_report(dir: &Path) -> EvidenceVerifyReport {
        let mut report = empty_report(dir);
        verify_published_proofs(dir, "fixture", &mut report);
        report
    }

    fn rup_bundle() -> (tempfile::TempDir, std::path::PathBuf) {
        let (root, dir) = bundle("fn f(x: i64) { assert(x == x); }");
        let proofs = read(&dir, "analysis/proofs.json");
        assert_eq!(proofs["obligations"].as_array().unwrap().len(), 1);
        assert_eq!(proofs["obligations"][0]["proof"], "rup_refutation");
        let honest = proof_report(&dir);
        assert!(honest.ok, "{:?}", honest.checks);
        assert_eq!(honest.checks[0].status, CheckStatus::Pass);
        assert!(honest.checks[0]
            .detail
            .contains("same-implementation consistency"));
        assert!(report(&dir).ok);
        (root, dir)
    }

    fn assert_both_lanes_reject(dir: &Path, reason: &str) {
        for result in [proof_report(dir), report(dir)] {
            assert!(!result.ok, "{:?}", result.checks);
            assert!(
                result.checks[0].detail.contains(reason),
                "expected {reason}: {:?}",
                result.checks
            );
        }
    }

    #[test]
    fn rehashed_unrelated_refutation_is_rejected_even_when_rup_accepts_it() {
        let (_root, dir) = rup_bundle();
        let unrelated_cnf = "p cnf 2 4\n1 2 0\n1 -2 0\n-1 2 0\n-1 -2 0\n";
        let unrelated_drat = "1 0\n-1 0\n0\n";
        assert!(check_rup(unrelated_cnf, unrelated_drat).is_ok());
        let cnf_path = dir.join("analysis/proofs/obligation_0000.cnf");
        assert_ne!(std::fs::read_to_string(&cnf_path).unwrap(), unrelated_cnf);
        std::fs::write(cnf_path, unrelated_cnf).unwrap();
        std::fs::write(
            dir.join("analysis/proofs/obligation_0000.drat"),
            unrelated_drat,
        )
        .unwrap();
        refresh_manifest_hashes(&dir).unwrap();
        assert_both_lanes_reject(&dir, "published CNF differs from native regeneration");
        let full = verify_path(&dir, &EvidenceVerifyOpts::default()).unwrap();
        assert!(!full.ok);
        for suffix in [".proofs", ".solver_replay"] {
            assert!(full
                .checks
                .iter()
                .any(|check| { check.id.ends_with(suffix) && check.status == CheckStatus::Fail }));
        }
    }

    #[test]
    fn consistently_rehashed_sat_query_cannot_borrow_a_pass_refutation() {
        let (_root, dir) = rup_bundle();
        let mut checks = read(&dir, "solver.json");
        checks[0]["smt"] = json!("(check-sat)");
        write(&dir, "solver.json", &checks);
        for relative in [
            "analysis/solver.smt2",
            "analysis/proofs/obligation_0000.smt2",
        ] {
            std::fs::write(dir.join(relative), "(check-sat)").unwrap();
        }
        refresh_manifest_hashes(&dir).unwrap();
        assert_both_lanes_reject(
            &dir,
            "bounded native regeneration returned no UNSAT certificate",
        );
    }

    #[test]
    fn certificate_paths_must_be_present_typed_and_index_canonical() {
        let (_root, dir) = rup_bundle();
        let original = read(&dir, "analysis/proofs.json");
        for (field, suffix) in [("cnf_dimacs", "cnf"), ("proof_drat", "drat")] {
            for path in [
                format!("/tmp/unrelated.{suffix}"),
                format!("../unrelated.{suffix}"),
                format!("analysis/proofs/../proofs/obligation_0000.{suffix}"),
                format!("analysis/proofs/obligation_0001.{suffix}"),
            ] {
                let mut proofs = original.clone();
                proofs["obligations"][0][field] = json!(path);
                write(&dir, "analysis/proofs.json", &proofs);
                assert_both_lanes_reject(&dir, "noncanonical or missing certificate paths");
            }
            for value in [Value::Null, json!(false)] {
                let mut proofs = original.clone();
                proofs["obligations"][0][field] = value;
                write(&dir, "analysis/proofs.json", &proofs);
                assert!(!proof_report(&dir).ok);
                assert!(!report(&dir).ok);
            }
            let mut proofs = original.clone();
            proofs["obligations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            write(&dir, "analysis/proofs.json", &proofs);
            assert_both_lanes_reject(&dir, "noncanonical or missing certificate paths");
        }
    }

    #[test]
    fn certificate_files_require_manifest_coverage_hashes_and_valid_rup() {
        let (_root, dir) = rup_bundle();
        let manifest_path = dir.join("MANIFEST.sha256");
        let manifest = std::fs::read_to_string(&manifest_path).unwrap();
        for relative in [
            "analysis/proofs/obligation_0000.cnf",
            "analysis/proofs/obligation_0000.drat",
        ] {
            let uncovered = manifest
                .lines()
                .filter(|line| line.split_whitespace().last() != Some(relative))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            std::fs::write(&manifest_path, uncovered).unwrap();
            assert_both_lanes_reject(&dir, "not manifest-covered");
            std::fs::write(&manifest_path, &manifest).unwrap();
            let path = dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            std::fs::write(&path, "altered without refreshing manifest").unwrap();
            assert_both_lanes_reject(&dir, "artifact hash mismatch");
            std::fs::write(&path, original).unwrap();
        }
        let drat_path = dir.join("analysis/proofs/obligation_0000.drat");
        let original_drat = std::fs::read_to_string(&drat_path).unwrap();
        for invalid in [String::new(), format!("{original_drat}malformed suffix\n")] {
            std::fs::write(&drat_path, invalid).unwrap();
            refresh_manifest_hashes(&dir).unwrap();
            assert_both_lanes_reject(&dir, "RUP refutation rejected");
        }
    }

    #[cfg(unix)]
    #[test]
    fn certificate_symlinks_and_escaping_parent_directories_are_rejected() {
        use std::os::unix::fs::symlink;

        let (root, dir) = rup_bundle();
        for relative in [
            "analysis/proofs/obligation_0000.cnf",
            "analysis/proofs/obligation_0000.drat",
        ] {
            let path = dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            let outside = root.path().join("outside-certificate");
            std::fs::write(&outside, &original).unwrap();
            std::fs::remove_file(&path).unwrap();
            symlink(&outside, &path).unwrap();
            // The target bytes still match the sealed digest; reject the file kind.
            assert_both_lanes_reject(&dir, "not a contained regular file");
            std::fs::remove_file(&path).unwrap();
            std::fs::write(&path, original).unwrap();
        }
        let outside = root.path().join("outside-proofs");
        std::fs::rename(dir.join("analysis/proofs"), &outside).unwrap();
        symlink(&outside, dir.join("analysis/proofs")).unwrap();
        assert_both_lanes_reject(&dir, "not a contained regular file");
    }

    #[test]
    fn non_rup_rows_do_not_earn_proof_coverage_or_carry_certificate_paths() {
        let (_root, dir) = rup_bundle();
        let mut proofs = read(&dir, "analysis/proofs.json");
        proofs["obligations"][0]["proof"] = json!("declined_by_native_solver_deferred");
        write(&dir, "analysis/proofs.json", &proofs);
        assert_both_lanes_reject(&dir, "non-RUP row carries certificate paths");
        for field in ["cnf_dimacs", "proof_drat"] {
            proofs["obligations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
        }
        write(&dir, "analysis/proofs.json", &proofs);
        let partial = proof_report(&dir);
        assert!(partial.ok, "{:?}", partial.checks);
        assert_eq!(partial.checks[0].status, CheckStatus::Skip);
        assert_eq!(partial.checks[0].classification, "NO_PUBLISHED_REFUTATIONS");
        assert!(partial.checks[0]
            .detail
            .contains("No proof assurance or solver discharge"));
        assert!(report(&dir).ok, "honest deferred provenance remains valid");

        let (_root, mixed) = bundle("fn f(x: i64) { assert(x == x); assert(x == 0); }");
        let partial = proof_report(&mixed);
        assert!(partial.ok, "{:?}", partial.checks);
        assert_eq!(partial.checks[0].status, CheckStatus::Skip);
        assert_eq!(partial.checks[0].classification, "PARTIAL");
        assert!(partial.checks[0].detail.contains("status=FAIL"));
        assert!(report(&mixed).ok);
    }

    #[test]
    fn safe_pass_and_synthetic_pass_have_no_counterexample_replay_claim() {
        for (source, synthetic) in [
            ("fn main() { let x = 1; assert(x == 1); }", false),
            ("fn main() { let x = 1; }", true),
        ] {
            let (_root, dir) = bundle(source);
            let checks = read(&dir, "solver.json");
            if synthetic {
                assert_eq!(checks.as_array().unwrap().len(), 1);
                assert_eq!(checks[0]["name"], "solver:no-obligations");
                assert_eq!(checks[0]["detail"], middle::NO_OBLIGATIONS_DETAIL);
                assert_eq!(read(&dir, "pca.json")["pca_version"], 3);
                assert_eq!(read(&dir, "pca.json")["solver_obligations"], 0);
                assert_eq!(
                    read(&dir, "analysis/proofs.json")["obligations"][0]["proof"],
                    "not_applicable_no_obligations"
                );
            }
            let outcome = check_solver_replay(&dir, |_, _| panic!("PASS must never replay"))
                .expect("honest no-replay record");
            assert!(matches!(
                outcome,
                SolverReplayVerification::Current {
                    status: "not_applicable",
                    replayed: 0,
                    complete: true,
                    synthetic_compatibility: false,
                    ..
                }
            ));
            assert!(report(&dir).ok);
            let full = verify_path(&dir, &EvidenceVerifyOpts::default()).unwrap();
            let pca = full
                .checks
                .iter()
                .find(|check| check.id.ends_with(".pca"))
                .unwrap();
            assert_eq!(pca.status, CheckStatus::Pass);
            assert!(pca.detail.contains("declared PCA version=3"));
            assert!(pca.detail.contains("recorded verdict=PASS"));
            assert!(pca.detail.contains("manifest hashes match the bundle"));
            assert!(full.checks.iter().any(|check| {
                check.id.ends_with(".solver_replay") && check.status == CheckStatus::Pass
            }));
            if synthetic {
                let proof_check = full
                    .checks
                    .iter()
                    .find(|check| check.id.ends_with(".proofs"))
                    .expect("proof lane must report that no obligations apply");
                assert_eq!(proof_check.status, CheckStatus::Skip);
                assert_eq!(proof_check.classification, "NOT_APPLICABLE");
                assert!(proof_check.detail.contains("Zero proof obligations"));
                assert!(proof_check.detail.contains("no refutation, earned proof"));
            }
        }
    }

    #[test]
    fn synthetic_pca_counts_follow_the_declared_version_without_rewriting_history() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/zk_prove_bundle");
        let historical_bytes = std::fs::read(fixture.join("pca.json")).unwrap();
        let historical: ReplayPcaSummary = serde_json::from_slice(&historical_bytes).unwrap();
        assert_eq!(historical.pca_version, 2);
        assert_eq!(historical.solver_obligations, 1);

        let (_root, dir) = bundle("fn main() { let x = 1; }");
        let generated = read(&dir, "pca.json");
        assert_eq!(generated["pca_version"], 3);
        assert_eq!(generated["solver_obligations"], 0);
        assert!(report(&dir).ok);
        let generated_proofs = read(&dir, "analysis/proofs.json");
        assert_eq!(
            generated_proofs["obligations"][0]["proof"],
            "not_applicable_no_obligations"
        );
        let mut old_proofs = generated_proofs.clone();
        old_proofs["obligations"][0]["proof"] = json!("counterexample_no_refutation");
        write(&dir, "analysis/proofs.json", &old_proofs);
        let wrong_v3_tag = report(&dir);
        assert!(!wrong_v3_tag.ok, "v3 must reject the historical proof tag");
        assert!(wrong_v3_tag.checks[0]
            .detail
            .contains("proof provenance mismatch"));
        assert!(!proof_report(&dir).ok);
        // Exercise old count semantics in a temporary, otherwise current v1
        // replay bundle. The archived fixture's unversioned sidecar stays
        // explicitly unverified in its separate compatibility test.
        let mut old_count = generated.clone();
        old_count["pca_version"] = json!(historical.pca_version);
        old_count["solver_obligations"] = json!(historical.solver_obligations);
        write(&dir, "pca.json", &old_count);
        let old_replay = report(&dir);
        assert!(old_replay.ok, "{:?}", old_replay.checks);
        assert_eq!(old_replay.checks[0].classification, "LEGACY_COMPATIBILITY");
        let old_proof = proof_report(&dir);
        assert!(old_proof.ok, "{:?}", old_proof.checks);
        assert_eq!(old_proof.checks[0].status, CheckStatus::Skip);
        assert_eq!(old_proof.checks[0].classification, "LEGACY_COMPATIBILITY");
        assert!(old_proof.checks[0]
            .detail
            .contains("Zero proof obligations"));
        assert!(old_proof.checks[0]
            .detail
            .contains("compatibility metadata"));
        let full = verify_path(&dir, &EvidenceVerifyOpts::default()).unwrap();
        assert!(full.checks.iter().any(|check| {
            check.id.ends_with(".proofs")
                && check.status == CheckStatus::Skip
                && check.classification == "LEGACY_COMPATIBILITY"
        }));

        write(&dir, "analysis/proofs.json", &generated_proofs);
        let wrong_v2_tag = report(&dir);
        assert!(!wrong_v2_tag.ok, "v2 must require the historical proof tag");
        assert!(wrong_v2_tag.checks[0]
            .detail
            .contains("proof provenance mismatch"));
        assert!(!proof_report(&dir).ok);

        for (version, count) in [(2, 0), (3, 1), (4, 0)] {
            let mut invalid = generated.clone();
            invalid["pca_version"] = json!(version);
            invalid["solver_obligations"] = json!(count);
            write(&dir, "pca.json", &invalid);
            assert!(
                !report(&dir).ok,
                "accepted PCA version={version} count={count}"
            );
            assert!(!proof_report(&dir).ok);
        }
        let mut missing_version = generated;
        missing_version
            .as_object_mut()
            .unwrap()
            .remove("pca_version");
        write(&dir, "pca.json", &missing_version);
        assert!(
            !report(&dir).ok,
            "an absent PCA version must not select legacy semantics"
        );
        assert!(!proof_report(&dir).ok);
        assert_eq!(
            historical_bytes,
            std::fs::read(fixture.join("pca.json")).unwrap()
        );
    }

    #[test]
    fn malformed_synthetic_rows_cannot_hide_real_or_failed_obligations() {
        let (_root, dir) = bundle("fn main() { let x = 1; }");
        let original = read(&dir, "solver.json");
        assert_eq!(original[0]["name"], "solver:no-obligations");
        for (field, value) in [
            ("name", json!("assert:real-obligation")),
            ("detail", json!(middle::PROVED_DETAIL_CERTIFIED)),
            ("status", json!("UNKNOWN")),
            ("model", json!("forged model")),
            ("smt", json!("(assert false)\n(check-sat)")),
        ] {
            let mut checks = original.clone();
            checks[0][field] = value;
            write(&dir, "solver.json", &checks);
            let result = check_solver_replay(&dir, |_, _| panic!("synthetic row must not replay"));
            assert!(
                matches!(result, Err(error) if error.to_string().contains("invalid synthetic"))
            );
            assert!(!proof_report(&dir).ok);
        }
        let duplicated = json!([original[0].clone(), original[0].clone()]);
        write(&dir, "solver.json", &duplicated);
        let result = check_solver_replay(&dir, |_, _| panic!("synthetic row must not replay"));
        assert!(matches!(result, Err(error) if error.to_string().contains("invalid synthetic")));
        assert!(!proof_report(&dir).ok);

        write(&dir, "solver.json", &original);
        let mut proofs = read(&dir, "analysis/proofs.json");
        let duplicate = proofs["obligations"][0].clone();
        proofs["obligations"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        write(&dir, "analysis/proofs.json", &proofs);
        assert!(
            !proof_report(&dir).ok,
            "mixed synthetic proof rows must fail"
        );
    }

    #[test]
    fn real_obligation_cannot_claim_the_no_obligations_proof_kind() {
        let (_root, dir) = bundle("fn main() { let x = 1; assert(x == 1); }");
        let mut proofs = read(&dir, "analysis/proofs.json");
        proofs["obligations"][0]["proof"] = json!("not_applicable_no_obligations");
        write(&dir, "analysis/proofs.json", &proofs);
        let forged = report(&dir);
        assert!(!forged.ok);
        assert!(forged.checks[0]
            .detail
            .contains("proof provenance mismatch"));
        assert!(
            !proof_report(&dir).ok,
            "a real obligation must not earn synthetic SKIP"
        );
    }

    #[test]
    fn safe_model_replay_rejects_a_rehashed_forged_witness() {
        let (_root, dir) = bundle("fn f(x: i64) { assert(x == x); assert(x == 0); }");
        let mut checks = read(&dir, "solver.json");
        let failed = checks
            .as_array()
            .unwrap()
            .iter()
            .position(|check| check["status"] == "FAIL" && check["model"].is_string())
            .expect("fixture must contain an actual counterexample");
        let honest = report(&dir);
        assert!(honest.ok, "{:?}", honest.checks);
        let full = verify_path(&dir, &EvidenceVerifyOpts::default()).unwrap();
        let pca = full
            .checks
            .iter()
            .find(|check| check.id.ends_with(".pca"))
            .unwrap();
        assert_eq!(pca.status, CheckStatus::Pass, "{}", pca.detail);
        assert!(pca.detail.contains("recorded verdict=FAIL"));
        assert_eq!(
            read(&dir, "analysis/solver_replay.json")["replay_valid"],
            true
        );

        checks[failed]["model"] = json!("not a solver model");
        write(&dir, "solver.json", &checks);
        let forged = report(&dir);
        assert!(!forged.ok);
        assert!(forged.checks[0].detail.contains("not re-established"));
    }

    #[test]
    fn pass_cannot_be_rehashed_into_a_counterexample_replay() {
        let (_root, dir) = bundle("fn main() { let x = 1; assert(x == 1); }");
        let mut record = read(&dir, "analysis/solver_replay.json");
        record["status"] = json!("counterexample_replayed");
        record["replay_valid"] = json!(true);
        write(&dir, "analysis/solver_replay.json", &record);
        let error = check_solver_replay(&dir, |_, _| panic!("PASS must never replay"));
        assert!(matches!(error, Err(error) if error.to_string().contains("aggregate")));
        record["obligations"][0]["status"] = json!("counterexample_replayed");
        record["obligations"][0]["replay_attempted"] = json!(true);
        record["obligations"][0]["replay_valid"] = json!(true);
        write(&dir, "analysis/solver_replay.json", &record);
        assert!(check_solver_replay(&dir, |_, _| panic!("PASS must never replay")).is_err());
    }

    #[test]
    fn uncertain_and_untrusted_provenance_never_invokes_model_replay() {
        let (_root, dir) = bundle("fn f(x: i64) { assert(x == 0); }");
        let original_checks = read(&dir, "solver.json");
        let original_record = read(&dir, "analysis/solver_replay.json");
        let original_proofs = read(&dir, "analysis/proofs.json");
        let failed = original_checks
            .as_array()
            .unwrap()
            .iter()
            .position(|check| check["status"] == "FAIL" && check["model"].is_string())
            .expect("fixture counterexample");
        for (detail, status, proof_kind) in [
            (
                middle::UNRESOLVED_PRECONDITION_DETAIL,
                "not_encoded",
                Some("unresolved_not_encoded"),
            ),
            (
                middle::OVERAPPROX_UNDECIDED_DETAIL,
                "overapproximated_not_replayed",
                Some("undecided_overapproximated"),
            ),
            (
                middle::BRANCH_REACHABILITY_UNDECIDED_DETAIL,
                "branch_reachability_not_replayed",
                Some("undecided_branch_reachability"),
            ),
            (
                middle::OVERAPPROX_COMBINED_UNDECIDED_DETAIL,
                "value_and_branch_reachability_not_replayed",
                Some("undecided_value_and_branch_reachability"),
            ),
            (
                "ANUBIS_REPLAY_MISMATCH: forged model",
                "not_replayed_untrusted_model",
                None,
            ),
        ] {
            let mut checks = original_checks.clone();
            let mut record = original_record.clone();
            let mut proofs = original_proofs.clone();
            checks[failed]["detail"] = json!(detail);
            checks[failed]["model"] = json!("forged model");
            record["obligations"][failed]["detail"] = json!(detail);
            record["obligations"][failed]["status"] = json!(status);
            record["obligations"][failed]["replay_attempted"] = json!(false);
            record["obligations"][failed]["replay_valid"] = json!(false);
            record["status"] = json!("incomplete");
            record["replay_valid"] = json!(false);
            if let Some(kind) = proof_kind {
                proofs["obligations"][failed]["proof"] = json!(kind);
            }
            write(&dir, "solver.json", &checks);
            write(&dir, "analysis/solver_replay.json", &record);
            write(&dir, "analysis/proofs.json", &proofs);
            let outcome = check_solver_replay(&dir, |_, _| panic!("untrusted provenance replayed"))
                .expect("correctly reported incompleteness");
            assert!(matches!(
                outcome,
                SolverReplayVerification::Current {
                    complete: false,
                    status: "incomplete",
                    ..
                }
            ));
        }
    }

    #[test]
    fn rehashed_proof_kind_cannot_contradict_a_replayed_model() {
        let (_root, dir) = bundle("fn f(x: i64) { assert(x == 0); }");
        let checks = read(&dir, "solver.json");
        let failed = checks
            .as_array()
            .unwrap()
            .iter()
            .position(|check| check["status"] == "FAIL" && check["model"].is_string())
            .expect("fixture counterexample");
        assert!(report(&dir).ok);
        let original = read(&dir, "analysis/proofs.json");
        for kind in [
            "unresolved_not_encoded",
            "undecided_overapproximated",
            "invented_proof_kind",
            "rup_refutation",
            "refutation_not_accepted_by_check",
            "unsat_without_published_certificate",
        ] {
            let mut proofs = original.clone();
            proofs["obligations"][failed]["proof"] = json!(kind);
            write(&dir, "analysis/proofs.json", &proofs);
            let result = report(&dir);
            assert!(!result.ok);
            assert!(result.checks[0]
                .detail
                .contains("proof provenance mismatch"));
        }
    }

    #[test]
    fn ordered_rows_proofs_and_smt_cannot_be_rehashed_apart() {
        let (_root, dir) = bundle("fn f(x: i64) { assert(x == x); assert(x == x); }");
        let original_record = read(&dir, "analysis/solver_replay.json");
        assert!(original_record["obligations"].as_array().unwrap().len() > 1);
        assert_eq!(
            original_record["obligations"][0]["obligation"],
            original_record["obligations"][1]["obligation"]
        );
        assert!(report(&dir).ok);
        let mut altered = original_record.clone();
        altered["obligations"].as_array_mut().unwrap().swap(0, 1);
        write(&dir, "analysis/solver_replay.json", &altered);
        assert!(!report(&dir).ok);
        altered = original_record.clone();
        altered["obligations"].as_array_mut().unwrap().pop();
        write(&dir, "analysis/solver_replay.json", &altered);
        assert!(!report(&dir).ok);
        write(&dir, "analysis/solver_replay.json", &original_record);

        let original_proofs = read(&dir, "analysis/proofs.json");
        let mut proofs = original_proofs.clone();
        proofs["obligations"][0]["status"] = json!("FAIL");
        write(&dir, "analysis/proofs.json", &proofs);
        assert!(!report(&dir).ok);
        write(&dir, "analysis/proofs.json", &original_proofs);
        let mut proofs = original_proofs.clone();
        proofs["obligations"][0]["proof"] = json!("counterexample_no_refutation");
        write(&dir, "analysis/proofs.json", &proofs);
        let forged_counterexample = report(&dir);
        assert!(!forged_counterexample.ok);
        assert!(forged_counterexample.checks[0]
            .detail
            .contains("proof provenance mismatch"));
        write(&dir, "analysis/proofs.json", &original_proofs);
        std::fs::write(
            dir.join("analysis/proofs/obligation_0000.smt2"),
            b"(check-sat)",
        )
        .unwrap();
        refresh_manifest_hashes(&dir).unwrap();
        assert!(!report(&dir).ok);
    }

    #[test]
    fn unknown_cannot_count_as_discharged_or_complete_replay_coverage() {
        let (_root, dir) = bundle("fn main() { let x = 1; assert(x == 1); }");
        let mut checks = read(&dir, "solver.json");
        let mut proofs = read(&dir, "analysis/proofs.json");
        let mut record = read(&dir, "analysis/solver_replay.json");
        checks[0]["status"] = json!("UNKNOWN");
        checks[0]["detail"] = json!("solver did not decide");
        proofs["obligations"][0]["status"] = json!("UNKNOWN");
        proofs["obligations"][0]["proof"] = json!("declined_by_native_solver_deferred");
        for field in ["cnf_dimacs", "proof_drat"] {
            proofs["obligations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
        }
        record["obligations"][0]["solver_status"] = json!("UNKNOWN");
        record["obligations"][0]["detail"] = checks[0]["detail"].clone();
        record["obligations"][0]["status"] = json!("not_replayed_undecided");
        record["status"] = json!("incomplete");
        write(&dir, "solver.json", &checks);
        write(&dir, "analysis/proofs.json", &proofs);
        write(&dir, "analysis/solver_replay.json", &record);
        let mut pca = read(&dir, "pca.json");
        for version in [2, 3] {
            pca["pca_version"] = json!(version);
            pca["solver_all_discharged"] = json!(true);
            write(&dir, "pca.json", &pca);
            assert!(
                !report(&dir).ok,
                "UNKNOWN cannot agree with PCA discharged=true"
            );
            pca["solver_all_discharged"] = json!(false);
            write(&dir, "pca.json", &pca);
            let outcome = check_solver_replay(&dir, |_, _| panic!("UNKNOWN must never replay"))
                .expect("honestly incomplete artifact");
            assert!(matches!(
                outcome,
                SolverReplayVerification::Current {
                    status: "incomplete",
                    complete: false,
                    ..
                }
            ));
            assert!(
                !report(&dir).ok,
                "honest incompleteness is not replay assurance"
            );
        }
    }

    #[test]
    fn schema_stripping_unknown_versions_and_missing_manifest_coverage_fail() {
        let (_root, dir) = bundle("fn main() { let x = 1; assert(x == 1); }");
        let original = read(&dir, "analysis/solver_replay.json");
        let mut stripped = original.clone();
        stripped.as_object_mut().unwrap().remove("schema");
        write(&dir, "analysis/solver_replay.json", &stripped);
        let legacy = report(&dir);
        assert!(!legacy.ok);
        assert_eq!(legacy.checks[0].classification, "LEGACY_UNVERIFIED");
        let full = verify_path(&dir, &EvidenceVerifyOpts::default()).unwrap();
        assert!(!full.ok);
        assert!(full.checks.iter().any(|check| {
            check.id.ends_with(".solver_replay")
                && check.status == CheckStatus::Fail
                && check.classification == "LEGACY_UNVERIFIED"
        }));
        let mut future = original.clone();
        future["schema"] = json!("anubis-solver-replay-unrecognized");
        write(&dir, "analysis/solver_replay.json", &future);
        assert!(!report(&dir).ok);
        write(&dir, "analysis/solver_replay.json", &original);
        let path = dir.join("MANIFEST.sha256");
        let manifest = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            manifest
                .lines()
                .filter(|line| !line.ends_with("  solver.json"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        assert!(!report(&dir).ok);
    }

    #[test]
    fn archived_unversioned_fixture_is_preserved_but_not_replay_verified() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/zk_prove_bundle");
        let before = std::fs::read(fixture.join("analysis/solver_replay.json")).unwrap();
        let result = report(&fixture);
        assert!(!result.ok);
        assert_eq!(result.checks[0].status, CheckStatus::Fail);
        assert_eq!(result.checks[0].classification, "LEGACY_UNVERIFIED");
        assert_eq!(
            before,
            std::fs::read(fixture.join("analysis/solver_replay.json")).unwrap()
        );
    }

    #[test]
    #[cfg(feature = "prove")]
    fn advertised_zk_receipt_gets_a_separate_cryptographic_check() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/zk_prove_bundle");
        let report = verify_path(&fixture, &EvidenceVerifyOpts::default()).unwrap();
        let zk = report
            .checks
            .iter()
            .find(|check| check.id.ends_with(".zk"))
            .expect("advertised receipt requires an explicit ZK result");
        assert_eq!(zk.status, CheckStatus::Pass, "{}", zk.detail);
    }

    #[test]
    fn absent_replay_evidence_does_not_verify_a_rejected_bundle() {
        let root = tempfile::tempdir().unwrap();
        let bundle = anubis_compiler::evidence::build_rejected_evidence_bundle(
            "fn main( {",
            "safe",
            vec![],
            root.path(),
            None,
            "fixture parse rejection",
        )
        .expect("rejection evidence");
        assert!(!bundle.dir.join("analysis/solver_replay.json").exists());
        let result = report(&bundle.dir);
        assert!(!result.ok);
        assert_eq!(result.checks[0].classification, "NO_REPLAY_EVIDENCE");
    }
}
