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
// It deliberately reads the PUBLISHED TEXT FORMATS — DIMACS CNF and the
// DRAT-shaped refutation — rather than any solver struct, so it exercises the
// same artifacts and the same path a stranger with `drat-trim` would. It is
// still Anubis code, so it is evidence of internal consistency, not of
// independence; the bundle prints the external replay command for that, and the
// detail string below says so rather than implying more.

/// Parse DIMACS or DRAT clause text. Comment and problem lines are skipped;
/// every other line is a zero-terminated list of literals.
fn parse_clauses(text: &str) -> Vec<Vec<i32>> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('c') || line.starts_with('p') {
            continue;
        }
        // A deletion line ("d 1 2 0") removes a clause; skipping it is sound for
        // checking, it only makes propagation do more work.
        if line.starts_with('d') {
            continue;
        }
        let mut clause = Vec::new();
        let mut terminated = false;
        for tok in line.split_whitespace() {
            match tok.parse::<i32>() {
                Ok(0) => {
                    terminated = true;
                    break;
                }
                Ok(l) => clause.push(l),
                Err(_) => break,
            }
        }
        if terminated {
            out.push(clause);
        }
    }
    out
}

/// Unit-propagate to fixpoint. Returns true when a clause is falsified.
fn propagate(clauses: &[Vec<i32>], assign: &mut [i8]) -> bool {
    loop {
        let mut progressed = false;
        for clause in clauses {
            let mut unassigned = 0i32;
            let mut last = 0i32;
            let mut satisfied = false;
            for &lit in clause {
                let var = lit.unsigned_abs() as usize;
                if var >= assign.len() {
                    continue;
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
                return true; // every literal false: conflict
            }
            if unassigned == 1 {
                let var = last.unsigned_abs() as usize;
                assign[var] = if last > 0 { 1 } else { -1 };
                progressed = true;
            }
        }
        if !progressed {
            return false;
        }
    }
}

/// Re-derive a refutation by reverse unit propagation. Returns the number of
/// lemmas checked, or a named reason it was rejected.
fn check_rup(cnf_text: &str, proof_text: &str) -> std::result::Result<usize, String> {
    let mut formula = parse_clauses(cnf_text);
    let lemmas = parse_clauses(proof_text);
    if lemmas.is_empty() {
        return Err("refutation contains no lemmas".into());
    }
    let max_var = formula
        .iter()
        .chain(lemmas.iter())
        .flatten()
        .map(|l| l.unsigned_abs() as usize)
        .max()
        .unwrap_or(0);
    let mut derived_empty = false;
    for (i, lemma) in lemmas.iter().enumerate() {
        let mut assign = vec![0i8; max_var + 1];
        // RUP: assume the lemma is false, then propagate. A conflict means the
        // formula already implied it.
        for &lit in lemma {
            let var = lit.unsigned_abs() as usize;
            let want: i8 = if lit > 0 { -1 } else { 1 };
            if assign[var] != 0 && assign[var] != want {
                return Err(format!("lemma {i} is tautological or self-contradictory"));
            }
            assign[var] = want;
        }
        if !propagate(&formula, &mut assign) {
            return Err(format!(
                "lemma {i} is not implied by the formula (reverse unit propagation found no conflict)"
            ));
        }
        if lemma.is_empty() {
            derived_empty = true;
        }
        formula.push(lemma.clone());
    }
    if !derived_empty {
        return Err("refutation never derives the empty clause".into());
    }
    Ok(lemmas.len())
}

/// Replay every refutation the bundle publishes.
fn verify_published_proofs(dir: &Path, id_prefix: &str, report: &mut EvidenceVerifyReport) {
    let index = dir.join("analysis").join("proofs.json");
    if !index.is_file() {
        // Fail closed rather than return silently. A silent return pushes NO
        // check, so the report contains no `.proofs` row at all and reads as
        // though the proofs were examined and were fine. The documented
        // forgery is "stub the .drat and recompute MANIFEST.sha256"; DELETING
        // the proofs is the cheaper variant, and it used to reach
        // `overall: PASS` with the proof lane silently absent.
        //
        // Safe to fail here: `--evidence` always writes this file. A program
        // with no contracts at all still publishes an obligations array. So an
        // evidence bundle without it is either pre-v3 and archived, or altered
        // — and the message says both so an operator can tell which.
        report.push(
            &format!("{id_prefix}.proofs"),
            CheckStatus::Fail,
            "LAB_REAL",
            "analysis/proofs.json is absent: this bundle publishes no refutations, so nothing \
             in it was re-derived. Every `--evidence` bundle writes this file, including for a \
             program with no contracts, so its absence means the bundle predates the proofs \
             lane or has been altered."
                .to_string(),
        );
        return;
    }
    let raw = match std::fs::read_to_string(&index) {
        Ok(r) => r,
        Err(e) => {
            report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Fail,
                "LAB_REAL",
                format!("read analysis/proofs.json: {e}"),
            );
            return;
        }
    };
    let doc: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Fail,
                "LAB_REAL",
                format!("analysis/proofs.json parse: {e}"),
            );
            return;
        }
    };
    let obligations = doc
        .get("obligations")
        .and_then(|o| o.as_array())
        .cloned()
        .unwrap_or_default();
    if obligations.is_empty() {
        // Same reasoning as the absent file: silence here reads as "checked, fine".
        report.push(
            &format!("{id_prefix}.proofs"),
            CheckStatus::Fail,
            "LAB_REAL",
            "analysis/proofs.json publishes an empty obligations array: there is nothing to \
             re-derive, and a bundle that proves nothing must not verify as though it did."
                .to_string(),
        );
        return;
    }

    let mut replayed = 0usize;
    let mut uncertified = Vec::new();
    for ob in &obligations {
        let kind = ob.get("proof").and_then(|v| v.as_str()).unwrap_or("");
        let name = ob.get("obligation").and_then(|v| v.as_str()).unwrap_or("?");
        if kind != "rup_refutation" {
            // A row without a refutation is not a failure, but it IS a claim
            // resting on the solver's word, and it gets counted as such.
            uncertified.push(format!("{name} [{kind}]"));
            continue;
        }
        let cnf_rel = ob.get("cnf_dimacs").and_then(|v| v.as_str()).unwrap_or("");
        let drat_rel = ob.get("proof_drat").and_then(|v| v.as_str()).unwrap_or("");
        if cnf_rel.is_empty() || drat_rel.is_empty() {
            report.push(
                &format!("{id_prefix}.proofs"),
                CheckStatus::Fail,
                "LAB_REAL",
                format!("obligation {name} claims a refutation but names no cnf/drat file"),
            );
            return;
        }
        let cnf = match std::fs::read_to_string(dir.join(cnf_rel)) {
            Ok(t) => t,
            Err(e) => {
                report.push(
                    &format!("{id_prefix}.proofs"),
                    CheckStatus::Fail,
                    "LAB_REAL",
                    format!("{cnf_rel}: {e}"),
                );
                return;
            }
        };
        let drat = match std::fs::read_to_string(dir.join(drat_rel)) {
            Ok(t) => t,
            Err(e) => {
                report.push(
                    &format!("{id_prefix}.proofs"),
                    CheckStatus::Fail,
                    "LAB_REAL",
                    format!("{drat_rel}: {e}"),
                );
                return;
            }
        };
        match check_rup(&cnf, &drat) {
            Ok(_) => replayed += 1,
            Err(reason) => {
                report.push(
                    &format!("{id_prefix}.proofs"),
                    CheckStatus::Fail,
                    "LAB_REAL",
                    format!("REFUTATION REJECTED for {name}: {reason} ({drat_rel})"),
                );
                return;
            }
        }
    }

    let detail = if uncertified.is_empty() {
        format!(
            "{replayed}/{} refutation(s) re-derived from the published CNF/DRAT (Anubis-side replay; run validate.sh with drat-trim for an independent one)",
            obligations.len()
        )
    } else {
        format!(
            "{replayed}/{} refutation(s) re-derived; {} obligation(s) carry NO certificate and rest on the solver's word: {}",
            obligations.len(),
            uncertified.len(),
            uncertified.join(", ")
        )
    };
    report.push(
        &format!("{id_prefix}.proofs"),
        CheckStatus::Pass,
        "LAB_REAL",
        detail,
    );
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
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("run_cap") || name.contains("run-cap") {
        return true;
    }
    // Peek schema.
    if let Ok(raw) = std::fs::read_to_string(path) {
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
        match crate_verify_unverified(dir) {
            Ok(true) => report.push(
                &format!("{id_prefix}.unverified"),
                CheckStatus::Pass,
                "UNVERIFIED",
                "UNVERIFIED integrity envelope hashes match (no proof claim)",
            ),
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

    match verify_pca(dir) {
        Ok(true) => report.push(
            &format!("{id_prefix}.pca"),
            CheckStatus::Pass,
            "LAB_REAL",
            "PCA re-derived and matched sealed claim; hashes untampered",
        ),
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
                std::fs::read_to_string(&src_path),
                std::fs::read_to_string(&conf_path),
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
    // Mirror main's verify_unverified_build_evidence lightly via validate_bundle path.
    // UNVERIFIED envelopes use unverified.json — use hash file presence + manifest if any.
    let u = dir.join("unverified.json");
    if !u.is_file() {
        return Err(anyhow!("missing unverified.json"));
    }
    let _raw = std::fs::read_to_string(&u)?;
    // Structural: MANIFEST.sha256 if present must list existing files with matching hashes.
    let man = dir.join("MANIFEST.sha256");
    if man.is_file() {
        let text = std::fs::read_to_string(&man)?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let expect = parts.next().unwrap_or("");
            let file = parts.collect::<Vec<_>>().join(" ");
            if file.is_empty() {
                continue;
            }
            let p = dir.join(&file);
            if !p.is_file() {
                return Ok(false);
            }
            let actual = sha256_file_hex(&p)?;
            if actual != expect {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
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
        std::fs::read_to_string(&src_path),
        std::fs::read_to_string(&conf_path),
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

/// Human-readable summary lines for CLI.
pub fn format_human(report: &EvidenceVerifyReport) -> String {
    let mut lines = Vec::new();
    lines.push(format!("anubis evidence-verify: {}", report.path));
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
            c.id, c.classification, c.detail
        ));
    }
    if !report.classifications_seen.is_empty() {
        lines.push(format!(
            "classifications: {}",
            report.classifications_seen.join(", ")
        ));
    }
    for n in &report.notes {
        lines.push(format!("note: {n}"));
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
    use super::{check_rup, parse_clauses};

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
    fn comments_problem_and_deletion_lines_are_skipped() {
        let parsed = parse_clauses("c a comment\np cnf 2 1\nd 1 2 0\n1 -2 0\n");
        assert_eq!(parsed, vec![vec![1, -2]]);
    }

    #[test]
    fn an_unterminated_clause_line_is_not_accepted_as_a_clause() {
        // A line with no terminating 0 is malformed; treating it as a clause
        // would let a truncated file look like a shorter valid proof.
        assert!(parse_clauses("1 2\n").is_empty());
    }
}
