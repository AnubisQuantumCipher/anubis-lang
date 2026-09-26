//! Sovereign evidence / reproducibility system.
//! Produces timestamped tamper-evident bundles modeled on risc0-metal-hybrid evidence.

use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EvidenceManifest {
    pub timestamp: String,
    pub tool: String,
    pub mode: String,
    pub source_hash: String,
    #[serde(default)]
    pub build_log_hash: String,
    #[serde(default)]
    pub artifact_hash: Option<String>,
    #[serde(default)]
    pub lane: Option<String>,
    #[serde(default)]
    pub environment_hash: String,
    #[serde(default)]
    pub source_tree_hash: String,
    #[serde(default)]
    pub sarif_hash: String,
    #[serde(default)]
    pub bounty_report_hash: String,
    /// SHA-256 digest binding the core manifest fields (source/build/tree hashes + verdict). This
    /// is a *digest*, not a cryptographic signature — the real Ed25519 signature lives in `pca.sig`.
    /// Renamed from the misleading `manifest_signature`; the alias keeps older bundles readable.
    #[serde(default, alias = "manifest_signature")]
    pub manifest_sha256: String,
    pub checks: Vec<Check>,
    pub verdict: String,
    // Optional security-mode context recorded with the evidence.
    #[serde(default)]
    pub security: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentCapture {
    pub os: String,
    pub arch: String,
    pub rustc: String,
    pub cargo: String,
    pub z3: String,
    pub anubis: String,
    /// These host-local strings are recorded provenance, not an attested platform witness.
    #[serde(default)]
    pub machine_fields_status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SourceTreeEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// The source Merkle listing is not itself source closure evidence: every leaf
/// must also name the regular, manifest-covered bytes from which its digest and
/// the Merkle root can be recomputed. Historic descriptor-only listings remain
/// readable as JSON, but cannot satisfy source-closure validation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedSourceLeaves {
    schema: String,
    source_merkle_root: String,
    leaves: Vec<SealedSourceLeaf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedSourceLeaf {
    path: String,
    sha256: String,
    bytes: u64,
    sealed_path: String,
}

#[derive(Debug)]
pub struct EvidenceBundle {
    pub dir: PathBuf,
    pub manifest: EvidenceManifest,
    /// The analysis limit the bundle's own analysis stopped at (its re-check, or the derivation of
    /// its claim), with no finding before it: the bundle records no verdict about the program, and
    /// the command reports this limit rather than a program verdict.
    pub limit: Option<String>,
}

fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn sha256_file(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|data| sha256_bytes(&data))
}

fn tool_identity() -> String {
    format!("anubis {}", env!("CARGO_PKG_VERSION"))
}

/// Machine labels for a refuted encoding whose model is not a checked program counterexample.
/// The proof index and replay record use the same classification so neither can turn a branch-
/// reachability uncertainty into a native SAT/disproof claim.
fn undecided_provenance(detail: &str) -> Option<(&'static str, &'static str)> {
    if detail == crate::middle::OVERAPPROX_UNDECIDED_DETAIL {
        Some((
            "undecided_overapproximated",
            "overapproximated_not_replayed",
        ))
    } else if detail == crate::middle::BRANCH_REACHABILITY_UNDECIDED_DETAIL {
        Some((
            "undecided_branch_reachability",
            "branch_reachability_not_replayed",
        ))
    } else if detail == crate::middle::OVERAPPROX_COMBINED_UNDECIDED_DETAIL {
        Some((
            "undecided_value_and_branch_reachability",
            "value_and_branch_reachability_not_replayed",
        ))
    } else {
        None
    }
}

/// Record what this bundle actually replayed. A solver PASS is a proof claim, never a
/// counterexample replay; a FAIL with an untrusted model is not one either. Rows use the
/// solver-check index because display names can repeat at distinct call sites.
fn solver_replay_record<F>(
    solver_checks: &[crate::middle::SolverCheck],
    mut replay: F,
) -> serde_json::Value
where
    F: FnMut(&str, &str) -> bool,
{
    let mut rows = Vec::with_capacity(solver_checks.len());
    let no_obligations = crate::middle::is_no_obligations_sentinel(solver_checks);
    let mut failed_replay = false;
    let mut incomplete = solver_checks.is_empty();
    let mut replayed = false;

    for (index, check) in solver_checks.iter().enumerate() {
        let mut attempted = false;
        let mut valid = false;
        let status = if check.detail == crate::middle::UNRESOLVED_PRECONDITION_DETAIL {
            incomplete = true;
            "not_encoded"
        } else if let Some((_, replay_status)) = undecided_provenance(&check.detail) {
            incomplete = true;
            replay_status
        } else if no_obligations {
            "not_applicable_no_obligations"
        } else if crate::middle::reserves_no_obligations_identity(check) {
            incomplete = true;
            "not_replayed_invalid_sentinel"
        } else if check.status == "PASS" {
            "not_applicable"
        } else if check.status == "FAIL" {
            if let Some(model) = check.model.as_deref() {
                if crate::middle::counterexample_was_replayed(check) {
                    attempted = true;
                    valid = replay(&check.smt, model);
                    if valid {
                        replayed = true;
                        "counterexample_replayed"
                    } else {
                        failed_replay = true;
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
        rows.push(serde_json::json!({
            "index": index,
            "obligation": check.name,
            "solver_status": check.status,
            "detail": check.detail,
            "smt": format!("analysis/proofs/obligation_{index:04}.smt2"),
            "status": status,
            "replay_attempted": attempted,
            "replay_valid": valid,
        }));
    }

    let (status, replay_valid) = if failed_replay {
        ("replay_failed", false)
    } else if incomplete {
        ("incomplete", false)
    } else if replayed {
        ("counterexample_replayed", true)
    } else {
        ("not_applicable", false)
    };
    serde_json::json!({
        "schema": "anubis-solver-replay-v1",
        "scope": "all_solver_checks",
        "status": status,
        "replay_valid": replay_valid,
        "obligations": rows,
    })
}

pub fn build_evidence_bundle(
    source: &str,
    mode: &str,
    artifact: Option<&str>,
    logs: Vec<String>,
    out_base: &Path,
    lane: Option<&str>,
    security: Option<serde_json::Value>,
) -> Result<EvidenceBundle, String> {
    // Phase-6: single-file path uses Merkle one-leaf identity (= sha256(source)).
    build_evidence_bundle_tree(
        &[("source.anubis".to_string(), source.as_bytes().to_vec())],
        mode,
        artifact,
        logs,
        out_base,
        lane,
        security,
        None,
    )
}

/// Multi-file / multi-package evidence: `source_hash` is the Merkle root over sorted leaves.
/// Optional `dep_closure` is written and included in MANIFEST (top-level signature binds it).
#[allow(clippy::too_many_arguments)] // cohesive evidence-bundle inputs; a struct would not clarify
pub fn build_evidence_bundle_tree(
    files: &[(String, Vec<u8>)],
    mode: &str,
    artifact: Option<&str>,
    logs: Vec<String>,
    out_base: &Path,
    lane: Option<&str>,
    security: Option<serde_json::Value>,
    dep_closure: Option<&serde_json::Value>,
) -> Result<EvidenceBundle, String> {
    build_evidence_bundle_tree_inner(
        files,
        mode,
        artifact,
        logs,
        out_base,
        lane,
        security,
        dep_closure,
        None,
    )
}

/// Emit an honest, tamper-evident rejection bundle for a command that failed before artifact
/// production. The rejection is a first-class failing check and the PCA is explicitly marked
/// `rejected`/`FAIL`; it can never be mistaken for a successful proof claim.
pub fn build_rejected_evidence_bundle(
    source: &str,
    mode: &str,
    logs: Vec<String>,
    out_base: &Path,
    lane: Option<&str>,
    rejection: &str,
) -> Result<EvidenceBundle, String> {
    build_evidence_bundle_tree_inner(
        &[("source.anubis".to_string(), source.as_bytes().to_vec())],
        mode,
        None,
        logs,
        out_base,
        lane,
        None,
        None,
        Some(rejection),
    )
}

/// Tree-aware form of [`build_rejected_evidence_bundle`]. This is used when the command checked a
/// resolved multi-file/import program: `source.anubis` is the deterministic resolved snapshot and
/// the other leaves preserve the original entry/dependency material under the same Merkle root.
#[allow(clippy::too_many_arguments)]
pub fn build_rejected_evidence_bundle_tree(
    files: &[(String, Vec<u8>)],
    mode: &str,
    logs: Vec<String>,
    out_base: &Path,
    lane: Option<&str>,
    rejection: &str,
    dep_closure: Option<&serde_json::Value>,
) -> Result<EvidenceBundle, String> {
    build_evidence_bundle_tree_inner(
        files,
        mode,
        None,
        logs,
        out_base,
        lane,
        None,
        dep_closure,
        Some(rejection),
    )
}

/// The bundle's own validator, copied into every evidence directory.
///
/// It answers two different questions and never conflates them:
///
///   1. Has anything been edited?  Every file in MANIFEST.sha256 is re-hashed.
///   2. Do the proofs actually check?  Every exported DRAT refutation is
///      replayed against its DIMACS CNF with an external checker.
///
/// The second question is the one that matters, and a validator that only
/// answered the first was reporting "OK" for a bundle whose proofs had never
/// been re-derived. When no DRAT checker is installed the script says so in
/// plain words and exits non-zero rather than implying the proofs passed:
/// "unchecked" is not "verified".
///
/// `sha256sum` is tried before `shasum` because Linux is the primary target and
/// a stock Linux box has coreutils but not necessarily perl's shasum.
const VALIDATE_SH: &str = r#"#!/usr/bin/env sh
# Self-contained evidence validation. No 'anubis' binary is used, by design:
# a bundle you can only check with the tool that produced it is not evidence.
set -eu
DIR=$(dirname "$0")

# ---- pick a SHA-256 tool (Linux first, then macOS) --------------------------
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  echo 'validate.sh: no sha256sum or shasum on PATH' >&2
  exit 2
fi

# ---- 1. integrity: nothing in the bundle was edited --------------------------
if [ ! -f "$DIR/MANIFEST.sha256" ]; then
  echo 'MISSING MANIFEST.sha256' >&2
  exit 1
fi
while read -r line; do
  [ -z "$line" ] && continue
  hash=$(echo "$line" | cut -d' ' -f1)
  file=$(echo "$line" | cut -d' ' -f2- | xargs)
  if [ -f "$DIR/$file" ]; then
    actual=$(sha256 "$DIR/$file")
    if [ "$actual" != "$hash" ]; then
      echo "TAMPER: $file hash mismatch" >&2
      exit 1
    fi
  else
    echo "MISSING: $file" >&2
    exit 1
  fi
done < "$DIR/MANIFEST.sha256"
echo 'integrity: OK (every file matches MANIFEST.sha256)'

# ---- 2. proofs: replay every refutation with an external checker -------------
PROOF_DIR="$DIR/analysis/proofs"
if [ ! -d "$PROOF_DIR" ]; then
  echo 'proofs: none exported in this bundle'
  echo 'validate.sh: OK (integrity only)'
  exit 0
fi

total=0
for cnf in "$PROOF_DIR"/obligation_*.cnf; do
  [ -e "$cnf" ] || break
  total=$((total + 1))
done
if [ "$total" -eq 0 ]; then
  echo 'proofs: none exported in this bundle'
  echo 'validate.sh: OK (integrity only)'
  exit 0
fi

if command -v drat-trim >/dev/null 2>&1; then
  CHECKER=drat-trim
elif command -v cake_lpr >/dev/null 2>&1; then
  CHECKER=cake_lpr
else
  echo "proofs: $total refutation(s) present but NOT REPLAYED: no drat-trim or cake_lpr on PATH" >&2
  echo 'validate.sh: INCOMPLETE - integrity checked, proofs unchecked' >&2
  echo '  install a DRAT checker and re-run; unchecked is not verified' >&2
  exit 3
fi

checked=0
for cnf in "$PROOF_DIR"/obligation_*.cnf; do
  [ -e "$cnf" ] || break
  drat="${cnf%.cnf}.drat"
  name=$(basename "$cnf" .cnf)
  if [ ! -f "$drat" ]; then
    echo "PROOF MISSING: $name has a formula but no refutation" >&2
    exit 1
  fi
  # The checker's EXIT CODE is the verdict, not its stdout: drat-trim prefixes
  # its "s VERIFIED" line with a carriage return, so matching on text silently
  # fails. 0 means the refutation re-derived the empty clause; nonzero means it
  # did not, which is exactly what a forged, empty, or satisfiable input gives.
  if "$CHECKER" "$cnf" "$drat" >/dev/null 2>&1; then
    checked=$((checked + 1))
  else
    echo "PROOF FAILED: $name did not replay under $CHECKER" >&2
    exit 1
  fi
done
echo "proofs: $checked/$total refutation(s) replayed and VERIFIED by $CHECKER"

# What this does and does not establish, stated in the artifact itself.
cat <<'NOTE'
validate.sh: OK
  established: every bundled file is unedited, and every exported refutation
               re-derives the empty clause under an external checker.
  NOT established: that each CNF is the faithful encoding of its .smt2, or that
               each .smt2 is the faithful obligation for the source. That link
               is still the compiler's word. See analysis/proofs.json.
NOTE
"#;

#[allow(clippy::too_many_arguments)]
fn build_evidence_bundle_tree_inner(
    files: &[(String, Vec<u8>)],
    mode: &str,
    artifact: Option<&str>,
    logs: Vec<String>,
    out_base: &Path,
    lane: Option<&str>,
    security: Option<serde_json::Value>,
    dep_closure: Option<&serde_json::Value>,
    rejection: Option<&str>,
) -> Result<EvidenceBundle, String> {
    if files.is_empty() {
        return Err("evidence source closure has no leaves".into());
    }
    if files.len() > MAX_EVIDENCE_TREE_ENTRIES {
        return Err("source closure leaf count exceeds verifier limit".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut total_source_bytes = 0_u64;
    for (path, bytes) in files {
        total_source_bytes = total_source_bytes
            .checked_add(bytes.len() as u64)
            .ok_or("source closure byte count overflow")?;
        if !evidence_manifest_path_ok(path)
            || !seen.insert(path)
            || bytes.len() as u64 > MAX_EVIDENCE_SOURCE_LEAF_BYTES
            || total_source_bytes > MAX_EVIDENCE_HASH_BYTES
        {
            return Err("invalid, duplicate, or oversized source closure leaf".into());
        }
    }
    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    // The bundle is written under a name ending in `.partial` and renamed when it is complete: a
    // process the allocator ends at the hard budget or the reserve (it cannot unwind) left a bundle
    // whose evidence and manifest said PASS but had no claim or hashes (eighth review of the checker
    // limits, B8-4).
    // The name is this build's own (process and sequence number): checks started in one directory
    // in the same second must not write into, or clear, each other's unfinished bundle.
    let final_name = format!("evidence-{}-{}", ts, mode);
    static STAGED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = out_base.join(format!(
        ".{final_name}.{}.{}.partial",
        std::process::id(),
        STAGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    if dir.exists() {
        // Left by an earlier process of the same number, in the same second: unfinished.
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let source_hash = crate::package::merkle::merkle_root(files.to_vec());
    // Primary source body for re-derive: exact source.anubis leaf first, then
    // a canonical suffix match, else concatenated text leaves.
    // leaves. The concat fallback filters to leaves that are valid UTF-8 text and free of NUL bytes —
    // a build artifact (Mach-O / ELF) is neither, so it can never be appended into the `source.anubis`
    // snapshot. Defense-in-depth: without this, a caller that passes a binary leaf (e.g. a native
    // artifact that slipped into the collected file tree) would inflate `source.anubis` with the
    // artifact's bytes, making `anubis report` fail to parse it. The merkle `source_hash` above is
    // still taken over ALL leaves, so bundle integrity is unchanged — only the human/parser-facing
    // snapshot is kept clean.
    let source = source_snapshot_from_leaves(files, MAX_EVIDENCE_SOURCE_BYTES)?;
    if total_source_bytes
        .checked_add(source.len() as u64)
        .is_none_or(|n| n > MAX_EVIDENCE_HASH_BYTES)
    {
        return Err("source closure exceeds verifier byte budget".into());
    }
    let build_log = logs.join("\n");
    let build_log_hash = sha256_bytes(build_log.as_bytes());
    let artifact_data = artifact
        .map(std::fs::read)
        .transpose()
        .map_err(|e| format!("artifact read failed: {}", e))?;
    let artifact_hash = artifact_data.as_deref().map(sha256_bytes);
    let hybrid_sidecars = copy_hybrid_sidecars(artifact, &dir)?;

    std::fs::write(dir.join("source.anubis"), &source).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("build.log"), &build_log).map_err(|e| e.to_string())?;
    if let Some(data) = &artifact_data {
        std::fs::write(dir.join("artifact"), data).map_err(|e| e.to_string())?;
    }
    if let Some(closure) = dep_closure {
        write_json(&dir.join("dep_closure.json"), closure)?;
    }
    // Phase-6 crown: seal function summaries from the sealed source text.
    // Package publish overwrites with extract_from_package (correct name/version/merkle) before sign.
    if let Ok(sum) = crate::package::summary::extract_from_source_text("package", "0.0.0", &source)
    {
        let _ = crate::package::summary::write_to_evidence_dir(&dir, &sum);
    }
    // Declassification audit trail — every `declassify(value, policy, reason)` call, per function, with
    // its well-formedness (operator directive 2026-07-20). A compliance-ready log of deliberate private-
    // data releases: an auditor greps `declassify_audit.json` for every disclosure + cited policy + reason.
    if let Ok(audit) =
        crate::package::summary::extract_declassify_audit("package", "0.0.0", &source)
    {
        let _ = crate::package::summary::write_audit_to_evidence_dir(&dir, &audit);
    }
    // VZ confinement manifest — the hypervisor confinement policy DERIVED from the program's proven
    // capability set (the six canonical effects). A SECOND, independent boundary consistent-by-
    // construction with what `anubis check` proved, sealed here (covered by MANIFEST.sha256 + pca.sig)
    // and re-derived + byte-compared on verify (fail-closed on a forged/source-swapped grant). It maps
    // each capability to a concrete Apple-Virtualization (tart) grant with honest tart_enforced /
    // needs_human flags — never claiming an isolation tart cannot deliver.
    if let Ok(cm) = crate::package::confinement::derive_confinement("package", "0.0.0", &source) {
        let _ = crate::package::confinement::write_confinement_to_evidence_dir(&dir, &cm);
    }
    // Effect-derived macOS entitlement / App Sandbox profile — OS-facing policy from the same proven
    // capability set as confinement. Sealed + re-derived on verify (fail-closed on forge). Honesty:
    // apple_enforced_claim is false; OS enforcement requires codesign (needs_human residual).
    if let Ok(ep) =
        crate::package::entitlements::derive_entitlement_profile("package", "0.0.0", &source)
    {
        let _ = crate::package::entitlements::write_entitlement_profile_to_evidence_dir(&dir, &ep);
    }
    // Seal the actual bytes behind every multi-leaf source root. A list of
    // digests alone cannot re-derive sha256(path || NUL || content), and a
    // manifest-covered but self-declared root is not source-closure evidence.
    let mut source_leaf_files = Vec::new();
    if files.len() > 1 {
        std::fs::create_dir_all(dir.join("source-leaves")).map_err(|e| e.to_string())?;
        let mut canonical_files: Vec<_> = files.iter().collect();
        canonical_files.sort_by(|a, b| a.0.cmp(&b.0));
        let leaves: Vec<SealedSourceLeaf> = canonical_files
            .into_iter()
            .enumerate()
            .map(|(index, (path, bytes))| {
                let sealed_path = format!("source-leaves/{index:08}.bin");
                std::fs::write(dir.join(&sealed_path), bytes).map_err(|e| e.to_string())?;
                source_leaf_files.push(sealed_path.clone());
                Ok(SealedSourceLeaf {
                    path: path.clone(),
                    sha256: sha256_bytes(bytes),
                    bytes: bytes.len() as u64,
                    sealed_path,
                })
            })
            .collect::<Result<_, String>>()?;
        write_json(
            &dir.join("source-merkle-leaves.json"),
            &SealedSourceLeaves {
                schema: "anubis-source-merkle-leaves-v2".into(),
                source_merkle_root: source_hash.clone(),
                leaves,
            },
        )?;
    }
    std::fs::create_dir_all(dir.join("analysis")).map_err(|e| e.to_string())?;
    if lane == Some(format!("{mode}-check").as_str())
        || (mode == "safe" && lane == Some(PACKAGE_PUBLISH_LANE))
    {
        // The command's semantic policy and native proof budgets are part of the check lane.
        // A verifier replays only the reproducible product defaults; nondefault or wall-clock
        // runs are recorded but cannot be upgraded to a source-derived PCA claim.
        write_json(
            &dir.join("analysis/check-config.json"),
            &current_check_config(),
        )?;
    }

    let mut checks = vec![];
    let mut hir_json = serde_json::json!({"functions": []});
    let mut mir_json = serde_json::json!([]);
    let mut taint_json = serde_json::json!([]);
    let mut solver_json = serde_json::json!([]);
    // Static monomorphization inventory (generic call sites with concrete type args).
    // Empty array when no generics specialize; always written so tools can rely on the path.
    let mut mono_json = serde_json::json!([]);

    let parse_res = crate::frontend::parse_source(&source);
    // Multi-leaf bundles seal the analyzed snapshot and the entry bytes, but there is no
    // independently checked import-to-snapshot translation yet. They cannot claim that this
    // analysis establishes the command's original multi-file program.
    let unresolved_source_snapshot = files.len() > 1;
    let unresolved_mode_elevator = parse_res
        .as_ref()
        .ok()
        .is_some_and(|ast| items_have_unresolved_mode_elevator(&ast.items));
    checks.push(match &parse_res {
        Ok(_) => Check {
            name: "parse".into(),
            status: "PASS".into(),
            detail: "ok".into(),
        },
        Err(e) => Check {
            name: "parse".into(),
            status: "FAIL".into(),
            detail: e.clone(),
        },
    });

    // A check refused at the analysis limit ran out of stack or memory; running the same analysis
    // again for the bundle would only repeat that (and double the time to the refusal).
    // A limit refusal is a refusal of the analysis of a program that parsed (a parse error's
    // rendering begins with the input's path, which can be anything).
    let limit_refusal =
        parse_res.is_ok() && rejection.is_some_and(crate::diagnostics::is_analysis_limit);
    if limit_refusal {
        checks.push(Check {
            name: "typecheck".into(),
            status: "FAIL".into(),
            detail: "not run again: the command's check reached the analysis limit \
                     (command_rejection)"
                .into(),
        });
    }
    // The analysis limit this bundle's own analysis stopped at, with no finding before it (see
    // [`EvidenceBundle::limit`]).
    let mut lane_limit: Option<String> = None;
    if let (Ok(ast), false) = (parse_res, limit_refusal) {
        let tc_mode = match mode {
            "research" => crate::frontend::Mode::Research,
            "exploit" => crate::frontend::Mode::Exploit,
            _ => crate::frontend::Mode::Safe,
        };
        match crate::middle::typecheck(ast, tc_mode) {
            Ok(ir) => {
                let tainted = crate::middle::TaintPass::apply(ir);
                hir_json = serde_json::to_value(&tainted.hir).map_err(|e| e.to_string())?;
                mir_json = serde_json::to_value(&tainted.mir).map_err(|e| e.to_string())?;
                taint_json =
                    serde_json::to_value(&tainted.taint_traces).map_err(|e| e.to_string())?;
                let solver_checks = crate::middle::SymbolicEngine::check_obligations(&tainted);
                solver_json = serde_json::to_value(&solver_checks).map_err(|e| e.to_string())?;
                mono_json = serde_json::to_value(&tainted.mono_specializations)
                    .map_err(|e| e.to_string())?;
                // save smt and replay for gate7
                // PROOF ARTIFACTS, not just verdicts.
                //
                // The bundle records solver status, every SMT query, and what replay actually ran.
                // That is still the compiler's own account of its work, so an auditor checking whether
                // proofs hold has to trust the component under audit. These files are the objects a
                // THIRD PARTY can re-check with `drat-trim` / `cake_lpr` and no Anubis binary:
                // the exact query, the exact blasted CNF, and the refutation over it.
                //
                // Produced by re-deciding each recorded query through the ordinary bounded path —
                // not by tapping the hot checking path — so emitting evidence cannot change a
                // verdict. An obligation that declines (out of fragment, over budget, or z3-decided)
                // simply has no refutation to publish, and is recorded as such rather than omitted.
                {
                    let pdir = dir.join("analysis").join("proofs");
                    let _ = std::fs::create_dir_all(&pdir);
                    let mut index = Vec::new();
                    let no_obligations = crate::middle::is_no_obligations_sentinel(&solver_checks);
                    for (i, c) in solver_checks.iter().enumerate() {
                        let stem = format!("obligation_{i:04}");
                        let _ = std::fs::write(pdir.join(format!("{stem}.smt2")), &c.smt);
                        // The synthetic empty-check marker is not a solver obligation. Its
                        // `(check-sat)` placeholder is satisfiable, but that is not a program
                        // counterexample and must not enter the native proof lane.
                        if no_obligations || crate::middle::reserves_no_obligations_identity(c) {
                            index.push(serde_json::json!({
                                "obligation": c.name,
                                "status": c.status,
                                "proof": if no_obligations {
                                    "not_applicable_no_obligations"
                                } else {
                                    "invalid_synthetic_no_obligations"
                                },
                                "smt": format!("analysis/proofs/{stem}.smt2"),
                            }));
                            continue;
                        }
                        // A model under over-approximated value or branch reachability is not a
                        // checked program counterexample. Keep the reason typed in the index;
                        // a native SAT label would incorrectly present it as a disproof.
                        if let Some((proof, _)) = undecided_provenance(&c.detail) {
                            index.push(serde_json::json!({
                                "obligation": c.name,
                                "status": c.status,
                                "proof": proof,
                                "smt": format!("analysis/proofs/{stem}.smt2"),
                            }));
                            continue;
                        }
                        // Never encoded, so there is no query to prove or refute. Handing its
                        // comment-only `.smt2` to the native solver would label the row as a
                        // counterexample or a deferral — both false.
                        if c.detail == crate::middle::UNRESOLVED_PRECONDITION_DETAIL {
                            index.push(serde_json::json!({
                                "obligation": c.name,
                                "status": c.status,
                                "proof": "unresolved_not_encoded",
                                "smt": format!("analysis/proofs/{stem}.smt2"),
                            }));
                            continue;
                        }
                        match anubis_solver::native_prove_with_artifacts(&c.smt) {
                            // Only an obligation the CHECK accepted gets a published refutation. A
                            // native refutation of a query the check refused — for any reason: z3
                            // rejected the query or disagreed, the premises were vacuous, the verdict
                            // was withdrawn — is not a proof of anything, and a `rup_refutation` row
                            // would present it as one. The row says only what is true of every such
                            // case: the check did not accept it.
                            Some((anubis_solver::NativeVerdict::Unsat, Some(a)))
                                if c.status == "PASS" =>
                            {
                                let _ =
                                    std::fs::write(pdir.join(format!("{stem}.cnf")), &a.cnf_dimacs);
                                let _ = std::fs::write(
                                    pdir.join(format!("{stem}.drat")),
                                    &a.proof_drat,
                                );
                                index.push(serde_json::json!({
                                    "obligation": c.name,
                                    "status": c.status,
                                    "proof": "rup_refutation",
                                    "smt": format!("analysis/proofs/{stem}.smt2"),
                                    "cnf_dimacs": format!("analysis/proofs/{stem}.cnf"),
                                    "proof_drat": format!("analysis/proofs/{stem}.drat"),
                                    "num_vars": a.num_vars,
                                    "num_clauses": a.num_clauses,
                                    "steps": a.steps,
                                    "checker": a.checker,
                                    "checker_version": a.checker_version,
                                    "replay": format!(
                                        "drat-trim analysis/proofs/{stem}.cnf analysis/proofs/{stem}.drat"),
                                }));
                            }
                            other => {
                                // Named, not omitted: "no refutation here" is itself evidence, and a
                                // silently missing row reads as if the obligation never existed.
                                index.push(serde_json::json!({
                                    "obligation": c.name,
                                    "status": c.status,
                                    "proof": match other {
                                        Some((anubis_solver::NativeVerdict::Sat(_), _)) =>
                                            "counterexample_no_refutation",
                                        Some((anubis_solver::NativeVerdict::Unsat, _))
                                            if c.status != "PASS" =>
                                            "refutation_not_accepted_by_check",
                                        Some(_) => "unsat_without_published_certificate",
                                        None => "declined_by_native_solver_deferred",
                                    },
                                    "smt": format!("analysis/proofs/{stem}.smt2"),
                                }));
                            }
                        }
                    }
                    let _ = std::fs::write(
                        dir.join("analysis").join("proofs.json"),
                        serde_json::to_string_pretty(&serde_json::json!({
                            "note": "Re-checkable proof objects. DIMACS CNF + DRAT refutation per \
                                     proven obligation; verify with drat-trim or any DRAT checker. \
                                     No Anubis binary required.",
                            "obligations": index,
                        }))
                        .unwrap_or_default(),
                    );
                }
                if let Some(first) = solver_checks.first() {
                    std::fs::write(dir.join("analysis").join("solver.smt2"), &first.smt)
                        .map_err(|e| format!("write analysis/solver.smt2: {e}"))?;
                }
                let replay_json =
                    solver_replay_record(&solver_checks, crate::middle::replay_counterexample);
                std::fs::write(
                    dir.join("analysis").join("solver_replay.json"),
                    serde_json::to_string_pretty(&replay_json).unwrap(),
                )
                .map_err(|e| format!("write analysis/solver_replay.json: {e}"))?;

                checks.push(Check {
                    name: "typecheck".into(),
                    status: "PASS".into(),
                    detail: format!(
                        "mode={} symbols={} functions={} mono={}",
                        mode,
                        tainted.symbols.len(),
                        tainted.hir.functions.len(),
                        tainted.mono_specializations.len()
                    ),
                });
                // Monomorphization inventory check: always PASS when typecheck passed.
                // Detail reports how many concrete specializations the checker proved.
                checks.push(Check {
                    name: "monomorphization".into(),
                    status: "PASS".into(),
                    detail: format!(
                        "static_specializations={} (codegen remains AnubisValue-erased)",
                        tainted.mono_specializations.len()
                    ),
                });
                if !tainted.taint_labels.is_empty() || !tainted.taint_traces.is_empty() {
                    checks.push(Check {
                        name: "taint".into(),
                        status: "PASS".into(),
                        detail: format!(
                            "labels={} traces={}",
                            tainted.taint_labels.len(),
                            tainted.taint_traces.len()
                        ),
                    });
                }
                checks.push(Check {
                    name: "symbolic".into(),
                    status: if tainted.constraints.is_empty() {
                        "FAIL"
                    } else {
                        "PASS"
                    }
                    .into(),
                    detail: format!("constraints={}", tainted.constraints.len()),
                });
                let solver_status = if crate::middle::solver_checks_discharged(&solver_checks) {
                    "PASS"
                } else {
                    "FAIL"
                };
                checks.push(Check {
                    name: "solver".into(),
                    status: solver_status.into(),
                    detail: solver_checks
                        .iter()
                        .map(|check| format!("{}={}", check.name, check.status))
                        .collect::<Vec<_>>()
                        .join(","),
                });
            }
            Err(err) => {
                // The re-check is a request of its own: stopped at a limit (another process took
                // the memory, or two checks share a scope) with nothing found, it says nothing
                // about the program the command already checked (B8-2).
                if crate::diagnostics::is_analysis_limit(&err)
                    && !crate::middle::last_analysis_kept_findings()
                {
                    lane_limit = Some(err.clone());
                }
                checks.push(Check {
                    name: "typecheck".into(),
                    status: "FAIL".into(),
                    detail: err,
                })
            }
        }
    }

    if let Some(rejection) = rejection {
        checks.push(Check {
            name: "command_rejection".into(),
            status: "FAIL".into(),
            detail: rejection.to_string(),
        });
    }

    checks.push(Check {
        name: "source_hash".into(),
        status: "PASS".into(),
        detail: source_hash.clone(),
    });
    checks.push(Check {
        name: "build_log_hash".into(),
        status: "PASS".into(),
        detail: build_log_hash.clone(),
    });
    if let Some(hash) = &artifact_hash {
        checks.push(Check {
            name: "artifact".into(),
            status: "PASS".into(),
            detail: "native emitted".into(),
        });
        checks.push(Check {
            name: "artifact_hash".into(),
            status: "PASS".into(),
            detail: hash.clone(),
        });
    }
    if !hybrid_sidecars.is_empty() {
        checks.push(Check {
            name: "hybrid_receipt_artifacts".into(),
            status: "PASS".into(),
            detail: hybrid_sidecars
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(","),
        });
        for (name, hash) in &hybrid_sidecars {
            checks.push(Check {
                name: hybrid_hash_check_name(name),
                status: "PASS".into(),
                detail: hash.clone(),
            });
        }
        if let Some(check) = risc0_metadata_check(&dir) {
            checks.push(check);
        }
    }

    // Proof-Carrying Artifact claim block — a deterministic verdict `verify` re-derives from the
    // source (plus a ZK receipt binding when the bundle carries a genuine receipt). Derived before
    // the manifest is written, so a limit it stops at is in the bundle's verdict: the claim's own
    // analysis is a third request, and a limit there was written as `typecheck_ok: false, verdict:
    // FAIL` under a command that had passed, rc 0 (B8-3). A limit refusal's claim is not
    // re-derived (its tier and verdict are set below), nor one whose re-check already stopped.
    let derived = if limit_refusal || lane_limit.is_some() {
        derive_claim(&source, mode, false)
    } else {
        derive_claim_bound(&dir, &source, mode)
    };
    if lane_limit.is_none() && derived.limit.is_some() && !derived.kept_finding {
        lane_limit = Some(derived.limit_text.clone().unwrap_or_else(|| {
            "ANUBIS_ANALYSIS_LIMIT: the analysis of the bundle's claim stopped at the analysis \
             limit"
                .into()
        }));
    }
    let mut claim = derived.claim;
    if rejection.is_none() {
        if let Some(limit) = &lane_limit {
            checks.push(Check {
                name: "evidence_analysis_limit".into(),
                status: "FAIL".into(),
                detail: format!(
                    "this bundle's own analysis stopped at the analysis limit, so it records no \
                     verdict about the program: {limit}"
                ),
            });
            claim = derive_claim(&source, mode, false).claim;
            claim.tier = "rejected".into();
            claim.rejection = Some(limit.clone());
            claim.verdict = "FAIL".into();
        }
    }

    write_json(&dir.join("hir.json"), &hir_json)?;
    write_json(&dir.join("mir.json"), &mir_json)?;
    write_json(&dir.join("taint-traces.json"), &taint_json)?;
    write_json(&dir.join("solver.json"), &solver_json)?;
    write_json(&dir.join("mono_specializations.json"), &mono_json)?;

    let environment = capture_environment();
    write_json(&dir.join("environment.json"), &environment)?;

    let sarif = build_sarif(&checks);
    write_json(&dir.join("checks.sarif"), &sarif)?;

    let report = build_bounty_report(mode, lane, &checks);
    std::fs::write(dir.join("bounty-report.md"), &report).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("validate.sh"), VALIDATE_SH).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let validate = dir.join("validate.sh");
        if let Ok(meta) = std::fs::metadata(&validate) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(validate, perms);
        }
    }

    let source_tree = build_source_tree(
        &dir,
        tracked_bundle_files(
            artifact_hash.is_some(),
            &hybrid_sidecars,
            &source_leaf_files,
        ),
    )?;
    write_json(&dir.join("source-tree.json"), &source_tree)?;
    let source_tree_text =
        std::fs::read_to_string(dir.join("source-tree.json")).map_err(|e| e.to_string())?;
    let environment_hash =
        sha256_file(&dir.join("environment.json")).ok_or("environment hash failed")?;
    let source_tree_hash = sha256_bytes(source_tree_text.as_bytes());
    let sarif_hash = sha256_file(&dir.join("checks.sarif")).ok_or("sarif hash failed")?;
    let bounty_report_hash =
        sha256_file(&dir.join("bounty-report.md")).ok_or("report hash failed")?;

    let all_pass = checks.iter().all(|c| c.status == "PASS");
    let verdict = if all_pass { "PASS" } else { "FAIL" }.to_string();
    let manifest_sha256 = sha256_bytes(
        format!(
            "{}:{}:{}:{}",
            source_hash, build_log_hash, source_tree_hash, verdict
        )
        .as_bytes(),
    );

    let manifest = EvidenceManifest {
        timestamp: ts,
        tool: tool_identity(),
        mode: mode.into(),
        source_hash,
        build_log_hash,
        artifact_hash,
        lane: lane.map(str::to_string),
        environment_hash,
        source_tree_hash,
        sarif_hash,
        bounty_report_hash,
        manifest_sha256,
        checks,
        verdict,
        security: security.or_else(|| {
            Some(serde_json::json!({
                "mode": mode,
                "note": "language attributes and effects recorded in checks and logs"
            }))
        }),
    };

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("evidence.json"), &json).map_err(|e| e.to_string())?;
    // v1 schema prefers manifest.json as well
    std::fs::write(dir.join("manifest.json"), &json).map_err(|e| e.to_string())?;
    // The claim block (derived above) is written before the manifest hashing so it is covered by
    // MANIFEST.sha256.
    if let Some(rejection) = rejection {
        claim.tier = "rejected".into();
        claim.rejection = Some(rejection.to_string());
        claim.verdict = "FAIL".into();
    }
    claim.evidence_lane = lane.map(str::to_string);
    claim.claim_kind = Some(
        if unresolved_source_snapshot {
            "source_snapshot_unverified_v1"
        } else if unresolved_mode_elevator {
            "source_mode_unverified_v1"
        } else if lane == Some(format!("{mode}-check").as_str()) {
            "source_check_v1"
        } else if mode == "safe" && lane == Some(PACKAGE_PUBLISH_LANE) {
            "package_publish_v1"
        } else if lane.is_some_and(|lane| lane.ends_with("-invalid-input-check")) {
            "invalid_input_unverified_v1"
        } else if lane.is_some_and(|lane| lane.ends_with("-analysis-limit-check")) {
            "analysis_limit_undecided_v1"
        } else if lane.is_some_and(|lane| lane.ends_with("-typecheck-refusal-check")) {
            "typecheck_refusal_unverified_v1"
        } else {
            "source_only_v1"
        }
        .into(),
    );
    write_json(&dir.join("pca.json"), &claim)?;
    // anubis.program-evidence.v3: assembled from the sealed bundle files and covered by the
    // manifest below. Best-effort — a program that does not fully discharge simply omits it and
    // the downstream verifier fail-closes on the missing v3 document.
    if let Err(err) = emit_program_evidence_v3(&dir) {
        eprintln!("program-evidence.v3 skipped: {err}");
    }
    write_manifest_hashes(&dir)?;
    // The producer must not publish a PASS-shaped bundle beyond the verifier's
    // actual file-count, byte, source, or check-roster limits. Keep an invalid
    // staged directory private instead of renaming it into a complete bundle.
    if !validate_bundle_recorded_files(&dir, false)? {
        return Err("produced evidence fails recorded-file validation".into());
    }

    // Complete: under its own name (a new one, if a bundle of this second already holds it, or
    // another process takes it first: a directory is never renamed over one that has files).
    let mut n = 1;
    let done = loop {
        let name = if n == 1 {
            final_name.clone()
        } else {
            format!("{final_name}-{n}")
        };
        let done = out_base.join(name);
        if !done.exists() {
            match std::fs::rename(&dir, &done) {
                Ok(()) => break done,
                Err(e) if !done.exists() => return Err(e.to_string()),
                Err(_) => {}
            }
        }
        n += 1;
    };
    Ok(EvidenceBundle {
        dir: done,
        manifest,
        limit: if rejection.is_none() {
            lane_limit
        } else {
            None
        },
    })
}

pub fn validate_bundle(dir: &Path) -> Result<bool, String> {
    validate_bundle_recorded_files(dir, true)
}

#[cfg(test)]
mod build_check_integrity_tests {
    use super::*;

    #[test]
    fn rehashed_empty_or_truncated_check_list_is_not_a_pass() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; }",
            "safe",
            None,
            vec![],
            root.path(),
            None,
            None,
        )
        .unwrap();
        assert!(validate_bundle(&bundle.dir).unwrap());
        let path = bundle.dir.join("evidence.json");
        let original = std::fs::read(&path).unwrap();
        for missing in ["all", "solver", "source_hash", "typecheck"] {
            let mut evidence: EvidenceManifest = serde_json::from_slice(&original).unwrap();
            if missing == "all" {
                evidence.checks.clear();
            } else {
                evidence.checks.retain(|check| check.name != missing);
            }
            std::fs::write(&path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(!validate_bundle(&bundle.dir).unwrap(), "missing {missing}");
        }
        std::fs::write(&path, original).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle(&bundle.dir).unwrap());
    }
}

/// Check the bundle's recorded files without requiring the program or build to
/// have passed. A checked counterexample is valid evidence of a FAIL verdict;
/// callers that require an acceptable artifact must use `validate_bundle` as
/// well as semantic PCA verification.
fn validate_bundle_recorded_files(dir: &Path, require_pass: bool) -> Result<bool, String> {
    let manifest_path = dir.join("evidence.json");
    if !manifest_path.exists() {
        return Err("no evidence.json".into());
    }
    // Validate the bounded complete file inventory before parsing or re-reading
    // any attacker-controlled evidence. The former order could read an unbounded
    // evidence.json even when MANIFEST itself was empty or invalid.
    if !validate_manifest_hashes(dir)? {
        return Ok(false);
    }
    let Some(manifest_text) = read_regular_evidence_text(&manifest_path, MAX_EVIDENCE_JSON_BYTES)?
    else {
        return Ok(false);
    };
    let manifest: EvidenceManifest =
        serde_json::from_str(&manifest_text).map_err(|e| e.to_string())?;
    let mut hashed_bytes = 0;
    let source_ok = source_closure_matches(dir, &manifest.source_hash, &mut hashed_bytes)?;
    let build_log_ok = manifest.build_log_hash.is_empty()
        || hash_evidence_file(
            &dir.join("build.log"),
            &mut hashed_bytes,
            MAX_EVIDENCE_HASH_BYTES,
        )?
        .is_some_and(|hash| hash == manifest.build_log_hash);
    let artifact_ok = match &manifest.artifact_hash {
        Some(expected) => hash_evidence_file(
            &dir.join("artifact"),
            &mut hashed_bytes,
            MAX_EVIDENCE_HASH_BYTES,
        )?
        .is_some_and(|hash| hash == *expected),
        None => true,
    };
    let env_ok = manifest.environment_hash.is_empty()
        || hash_evidence_file(
            &dir.join("environment.json"),
            &mut hashed_bytes,
            MAX_EVIDENCE_HASH_BYTES,
        )?
        .is_some_and(|hash| hash == manifest.environment_hash);
    let source_tree_ok = manifest.source_tree_hash.is_empty()
        || hash_evidence_file(
            &dir.join("source-tree.json"),
            &mut hashed_bytes,
            MAX_EVIDENCE_HASH_BYTES,
        )?
        .is_some_and(|hash| hash == manifest.source_tree_hash);
    let sarif_ok = manifest.sarif_hash.is_empty()
        || hash_evidence_file(
            &dir.join("checks.sarif"),
            &mut hashed_bytes,
            MAX_EVIDENCE_HASH_BYTES,
        )?
        .is_some_and(|hash| hash == manifest.sarif_hash);
    let report_ok = manifest.bounty_report_hash.is_empty()
        || hash_evidence_file(
            &dir.join("bounty-report.md"),
            &mut hashed_bytes,
            MAX_EVIDENCE_HASH_BYTES,
        )?
        .is_some_and(|hash| hash == manifest.bounty_report_hash);
    // A vacuous or truncated check list is not a successful build record.
    // These rows are still producer-reported outcomes; source/PCA re-derivation
    // and signer policy provide separate evidence, not the row labels alone.
    let mut seen_checks = std::collections::BTreeSet::new();
    let checks_well_formed = !manifest.checks.is_empty()
        && manifest.checks.iter().all(|check| {
            !check.name.is_empty()
                && seen_checks.insert(check.name.as_str())
                && matches!(check.status.as_str(), "PASS" | "FAIL")
        });
    let recorded = |name: &str| manifest.checks.iter().find(|check| check.name == name);
    let core_rows_present = ["parse", "source_hash", "build_log_hash"]
        .iter()
        .all(|name| recorded(name).is_some());
    let pass_rows_present = ["typecheck", "symbolic", "solver"]
        .iter()
        .all(|name| recorded(name).is_some());
    let hash_rows_consistent = recorded("source_hash")
        .is_some_and(|check| check.detail == manifest.source_hash)
        && recorded("build_log_hash").is_some_and(|check| check.detail == manifest.build_log_hash)
        && match &manifest.artifact_hash {
            Some(expected) => {
                recorded("artifact").is_some_and(|check| check.detail == "native emitted")
                    && recorded("artifact_hash").is_some_and(|check| check.detail == *expected)
            }
            None => recorded("artifact").is_none() && recorded("artifact_hash").is_none(),
        };
    let checks_ok = manifest.checks.iter().all(|c| c.status == "PASS");
    let recorded_verdict_consistent = manifest.verdict == if checks_ok { "PASS" } else { "FAIL" };

    Ok(source_ok
        && build_log_ok
        && artifact_ok
        && env_ok
        && source_tree_ok
        && sarif_ok
        && report_ok
        && checks_well_formed
        && core_rows_present
        && hash_rows_consistent
        && (!checks_ok || pass_rows_present)
        && recorded_verdict_consistent
        && (!require_pass || checks_ok))
}

/// The Proof-Carrying Artifact claim block: a deterministic, independently-checkable summary of what
/// a program is claimed to be. Unlike the hash-based manifest, it records the *semantic* verdict
/// (parse / typecheck / bounded solver), so `anubis verify` can RE-DERIVE it from the source and
/// confirm the recorded claim is honest — not merely untampered. It carries no timestamp: the same
/// source + mode always yields the same block, which is what makes re-derivation a real cross-check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimBlock {
    pub pca_version: u32,
    pub source_sha256: String,
    pub mode: String,
    /// v4 binds the claim to the recorded producer lane. Historic claims omit this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_lane: Option<String>,
    /// v4 names the extent of independent re-derivation. A source-only claim cannot
    /// authorize check sidecars or a bounty-ready report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_kind: Option<String>,
    /// Assurance tier actually reached. v2/v3/v4: `"checked"` — parse + typecheck + the bounded solver
    /// obligation pass ran. This deliberately makes no separate total-flow/taint-clean claim.
    pub tier: String,
    /// Present only for a fail-closed command rejection. A rejected PCA is evidence of refusal,
    /// never a proof claim, and therefore carries an explicit reason alongside verdict `FAIL`.
    #[serde(default)]
    pub rejection: Option<String>,
    pub parse_ok: bool,
    /// The compiler's current typecheck policy accepted the program. This is a bounded implementation
    /// result, not a total-flow theorem; known-open carriers remain named in `docs/CLAIMS.md`.
    pub typecheck_ok: bool,
    pub solver_obligations: usize,
    pub solver_all_discharged: bool,
    /// v4 distinguishes an unrun solver from one that discharged an empty obligation set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver_execution: Option<SolverExecution>,
    /// v4 source-derived typecheck refusal class. A security finding is a reproduced static
    /// policy finding, never a solver counterexample or proof of executable source behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typecheck_refusal_kind: Option<TypecheckRefusalKind>,
    /// Per-obligation backend provenance is not collected yet. Current v4 claims say
    /// `unobserved`, rather than claiming that z3 decided an obligation that may have used the
    /// native authoritative fragment. Historic v2/v3 claims retain their legacy `z3` value for
    /// byte-compatible source-only verification; it must not be treated as observed backend
    /// authority. `solver_execution` separately states whether any obligation pass ran.
    #[serde(default = "default_solver_backend")]
    pub solver_backend: String,
    /// Whether a zero-knowledge receipt is bound to this claim. `false` when the bundle carries no
    /// genuine receipt — stated explicitly so the block never silently implies a ZK proof it does
    /// not carry.
    #[serde(default)]
    pub zk_present: bool,
    /// When `zk_present`, the RISC Zero ImageID (guest-bound) the receipt attests to. `None`
    /// otherwise. Naming it makes the claim specific: `verify` re-derives it from the bundle and
    /// cryptographically re-checks the receipt against it (a wrong ImageID fails closed).
    #[serde(default)]
    pub zk_image_id: Option<String>,
    /// When `zk_present`, the SHA-256 of the receipt artifact — re-derived from the bundle's own
    /// `receipt.bin`, so a tampered receipt makes the re-derived claim mismatch.
    #[serde(default)]
    pub zk_receipt_sha256: Option<String>,
    /// When `zk_present`, the SHA-256 of the receipt's committed journal (the public output).
    #[serde(default)]
    pub zk_journal_sha256: Option<String>,
    pub verdict: String,
    pub tool: String,
}

/// Legacy deserialization value for source-only PCA v2/v3 claims. It was a producer policy
/// label, not a per-obligation observation of whether native or z3 decided a query.
fn default_solver_backend() -> String {
    "z3".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolverExecution {
    /// No real solver obligation was submitted, either because analysis stopped earlier or
    /// because the well-formed no-obligations sentinel represents an empty inventory.
    NotRun,
    /// At least one real obligation was submitted to the solver path. This does not identify
    /// which backend decided it; see `solver_backend`.
    Ran,
}

/// A PASS with no obligations needs no solver query; a PASS with real obligations must have
/// submitted them. Exact source re-derivation separately checks the obligation inventory and
/// sentinel shape, so this consistency rule does not grant authority to a producer's count.
pub fn solver_execution_matches_obligations(claim: &ClaimBlock) -> bool {
    match (claim.solver_execution, claim.solver_obligations) {
        (Some(SolverExecution::NotRun), 0) => true,
        (Some(SolverExecution::Ran), count) if count > 0 => true,
        _ => false,
    }
}

/// An unencoded precondition is a real unresolved obligation but never enters either solver.
/// This aggregate records whether any row entered the solver path, including a mixed stream;
/// exact row/source re-derivation separately establishes why each other row exists.
fn solver_path_was_invoked(checks: &[crate::middle::SolverCheck]) -> bool {
    checks.iter().any(|check| {
        !crate::middle::reserves_no_obligations_identity(check)
            && check.detail != crate::middle::UNRESOLVED_PRECONDITION_DETAIL
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypecheckRefusalKind {
    SecurityPolicyFinding,
    InvalidOrUnsupported,
    UndecidedLimit,
}

/// Re-derive the claim block from source. Deterministic and side-effect free — the single source of
/// truth used both when emitting a PCA and when verifying one, so the two agree exactly.
pub fn derive_claim_block(source: &str, mode: &str) -> ClaimBlock {
    derive_claim(source, mode, true).claim
}

const PCA_VERSION_CURRENT: u32 = 4;
pub const PACKAGE_PUBLISH_LANE: &str = "safe-package-publish";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PcaScope {
    /// A v4 plain check with its core rows, policy, analysis, and presentation re-derived.
    SourceCheckV1,
    /// Source/check sidecars were re-derived, but package summary identity is pending
    /// comparison with the mounted package and cannot authorize a dependency alone.
    PackagePublishV1,
    /// A v4 build/proof claim whose source verdict, not its platform sidecars, was re-derived.
    SourceOnly,
    /// A v2 or v3 source claim retained for compatibility, without check-sidecar authority.
    LegacySourceOnly,
}

/// The claim block, and the analysis limit its analysis stopped at, if any (then its verdict is not
/// a fact about the program). `analyze: false` records the parse only.
/// A claim block derived from a source, and how its analysis ended.
struct Derived {
    claim: ClaimBlock,
    source_mode: Option<crate::frontend::Mode>,
    unresolved_mode_elevator: bool,
    /// Exact semantic rows independently re-derived from the sealed source. A rejected command's
    /// FAIL marker must not coexist with forged PASS/FAIL parse, typecheck, or solver rows.
    check_rows: Vec<Check>,
    /// The independently reproduced analysis outputs. Bundled views of these results must not
    /// tell a stronger story than the source-derived PCA merely because their hashes were renewed.
    solver_checks: Vec<crate::middle::SolverCheck>,
    hir_json: Option<serde_json::Value>,
    mir_json: Option<serde_json::Value>,
    taint_json: Option<serde_json::Value>,
    mono_json: Option<serde_json::Value>,
    /// Every solver query has an SMT leaf. Only an ordinary accepted obligation may additionally
    /// have a CNF/refutation leaf; the no-obligations marker is not a program proof.
    proof_file_rows: Vec<bool>,
    /// A refusal independently reproduced from the sealed source by the same check lane.
    /// A command-level rejection cannot be authenticated by copying its own manifest text.
    check_refusal: Option<String>,
    /// Captured from the same typed typecheck request that produced `check_refusal`.
    replayable_security_refusal: bool,
    /// The analysis limit the analysis stopped at, if any.
    limit: Option<crate::middle::AnalysisLimit>,
    /// Stopped at a limit: the refusal it stopped with.
    limit_text: Option<String>,
    /// Stopped at the memory limit because the memory left ran low (the reserve), not the budget.
    by_reserve: bool,
    /// Whether, stopped at a limit, it still reported findings made before it: those hold on any
    /// machine, so the program does not type-check whatever memory a re-derivation had.
    kept_finding: bool,
}

/// The parsed function mode can understate nested `@research`/`@exploit` blocks and an earlier
/// Research-mode alias or Exploit attribute overwritten by a later lower-mode attribute.
/// Until the frontend records a source-wide intrinsic maximum, such programs may
/// run through ordinary checking but cannot receive a source-derived PCA mode claim.
pub fn items_have_unresolved_mode_elevator(items: &[crate::frontend::Item]) -> bool {
    use crate::frontend::{is_research_mode_attribute, Item, Mode};
    fn rank(mode: Mode) -> u8 {
        match mode {
            Mode::Safe => 0,
            Mode::Research => 1,
            Mode::Exploit => 2,
        }
    }
    items.iter().any(|item| match item {
        Item::Fn {
            mode,
            attributes,
            body,
            requires,
            ensures,
            ..
        } => {
            // Contracts are expression fields outside `body`. Feed them to
            // the same field-total walker without copying the expressions.
            crate::middle::body_and_expressions_have_mode_elevator(
                body,
                requires.iter().chain(ensures),
            ) || attributes.iter().any(|attr| {
                let declared = if is_research_mode_attribute(&attr.name) {
                    Some(Mode::Research)
                } else if attr.name == "exploit" {
                    Some(Mode::Exploit)
                } else {
                    None
                };
                declared.is_some_and(|candidate| rank(candidate) > rank(*mode))
            })
        }
        Item::Module { items, .. } => items_have_unresolved_mode_elevator(items),
        Item::Impl { methods, .. } | Item::Trait { methods, .. } => {
            items_have_unresolved_mode_elevator(methods)
        }
        Item::Import { .. } | Item::Struct { .. } | Item::Enum { .. } => false,
    })
}

/// A narrow, source-replayable information-flow finding. Every diagnostic must describe a
/// concrete policy flow; mixed unknown names/types, proof-search failures, IFC2 budget limits,
/// and resource limits remain invalid or undecided. This classification does not claim a runtime
/// witness or a solver counterexample.
pub fn replayable_security_refusal(failure: &crate::middle::TypecheckFailure) -> bool {
    failure.limit.is_none()
        && !failure.diagnostics.is_empty()
        && failure.diagnostics.iter().all(|finding| {
            matches!(
                finding.code.as_deref(),
                Some(
                    "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"
                        | "ANUBIS_INTERPROC_SINK"
                        | "ANUBIS_SECRET_EXFILTRATION"
                        | "ANUBIS_INTERPROC_EXFILTRATION"
                        | "ANUBIS_SECRET_TO_PUBLIC"
                        | "ANUBIS_IMPLICIT_FLOW"
                )
            )
        })
}

pub fn source_analysis_limit_refusal(failure: &crate::middle::TypecheckFailure) -> bool {
    failure.limit.is_some()
        || failure
            .diagnostics
            .iter()
            .any(|finding| finding.code.as_deref() == Some("ANUBIS_IFC2_LIMIT"))
}

/// Summarize the exact emitted checks for PCA. The no-obligations PASS sentinel is an
/// admission of a checked program with nothing to prove, not one discharged obligation.
fn solver_claim_summary(checks: &[crate::middle::SolverCheck]) -> (usize, bool) {
    (
        if crate::middle::is_no_obligations_sentinel(checks) {
            0
        } else {
            checks.len()
        },
        crate::middle::solver_checks_discharged(checks),
    )
}

fn derive_claim(source: &str, mode: &str, analyze: bool) -> Derived {
    derive_claim_for_version(source, mode, analyze, PCA_VERSION_CURRENT)
}

fn derive_claim_for_version(source: &str, mode: &str, analyze: bool, pca_version: u32) -> Derived {
    let source_sha256 = sha256_bytes(source.as_bytes());
    let tc_mode = match mode {
        "research" => crate::frontend::Mode::Research,
        "exploit" => crate::frontend::Mode::Exploit,
        _ => crate::frontend::Mode::Safe,
    };
    let parse_res = crate::frontend::parse_source(source);
    let parse_ok = parse_res.is_ok();
    // Keep the frontend's intrinsic-mode result intact. A successfully parsed program with no
    // functions has `None`; the CLI treats that as Safe at command admission, and the verifier
    // applies the same default below while still checking parse success separately.
    let source_mode = parse_res
        .as_ref()
        .ok()
        .and_then(|ast| crate::frontend::program_mode(&ast.items));
    let unresolved_mode_elevator = parse_res
        .as_ref()
        .ok()
        .is_some_and(|ast| items_have_unresolved_mode_elevator(&ast.items));
    let mut check_rows = vec![match &parse_res {
        Ok(_) => Check {
            name: "parse".into(),
            status: "PASS".into(),
            detail: "ok".into(),
        },
        Err(reason) => Check {
            name: "parse".into(),
            status: "FAIL".into(),
            detail: reason.clone(),
        },
    }];
    let mut typecheck_ok = false;
    let mut limit = None;
    let mut limit_text = None;
    let mut by_reserve = false;
    let mut kept_finding = false;
    let mut solver_obligations = 0usize;
    // v4 distinguishes an unrun solver from vacuous discharge. Historical v2/v3 receipts
    // retain their serialized value for source-only compatibility.
    let mut solver_all_discharged = pca_version != PCA_VERSION_CURRENT;
    let mut solver_executed = false;
    let mut typecheck_refusal_kind = None;
    let mut replayable_security_refusal_found = false;
    let mut check_refusal = None;
    let mut proof_file_rows = Vec::new();
    let mut derived_solver_checks = Vec::new();
    let mut hir_json = Some(serde_json::json!({"functions": []}));
    let mut mir_json = Some(serde_json::json!([]));
    let mut taint_json = Some(serde_json::json!([]));
    let mut mono_json = Some(serde_json::json!([]));
    if let (Ok(ast), true) = (parse_res, analyze) {
        let typed = if pca_version == PCA_VERSION_CURRENT {
            match crate::middle::typecheck_ex_detailed(ast, tc_mode, false) {
                Ok(ir) => Ok(ir),
                Err(failure) => {
                    replayable_security_refusal_found = replayable_security_refusal(&failure);
                    typecheck_refusal_kind = Some(if source_analysis_limit_refusal(&failure) {
                        TypecheckRefusalKind::UndecidedLimit
                    } else if replayable_security_refusal_found {
                        TypecheckRefusalKind::SecurityPolicyFinding
                    } else {
                        TypecheckRefusalKind::InvalidOrUnsupported
                    });
                    Err(failure.message)
                }
            }
        } else {
            crate::middle::typecheck(ast, tc_mode)
        };
        limit = crate::middle::last_analysis_limit();
        kept_finding = crate::middle::last_analysis_kept_findings();
        by_reserve = crate::middle::last_analysis_by_reserve();
        if let (Err(e), Some(_)) = (&typed, limit) {
            limit_text = Some(e.clone());
        }
        match typed {
            Ok(ir) => {
                typecheck_ok = true;
                let tainted = crate::middle::TaintPass::apply(ir);
                hir_json = serde_json::to_value(&tainted.hir).ok();
                mir_json = serde_json::to_value(&tainted.mir).ok();
                taint_json = serde_json::to_value(&tainted.taint_traces).ok();
                mono_json = serde_json::to_value(&tainted.mono_specializations).ok();
                // The earlier schema translated `typecheck` returning Ok into `taint_clean: true`.
                // That was a stronger guarantee than this lane derived: item 21 in `docs/CLAIMS.md`
                // contains accepted programs with runtime secret/taint witnesses. PCA v2 therefore
                // records only the bounded typecheck result above and carries no independent taint field.
                let solver_checks = crate::middle::SymbolicEngine::check_obligations(&tainted);
                let no_obligations = crate::middle::is_no_obligations_sentinel(&solver_checks);
                // Calling the inventory method is not a solver run: its synthetic PASS sentinel
                // means no query was sent. The independently re-derived stream still establishes
                // that the empty inventory is complete under this compiler's analysis.
                solver_executed = solver_path_was_invoked(&solver_checks);
                proof_file_rows = solver_checks
                    .iter()
                    .map(|check| {
                        !no_obligations
                            && !crate::middle::reserves_no_obligations_identity(check)
                            && check.status == "PASS"
                    })
                    .collect();
                check_rows.push(Check {
                    name: "typecheck".into(),
                    status: "PASS".into(),
                    detail: format!(
                        "mode={} symbols={} functions={} mono={}",
                        mode,
                        tainted.symbols.len(),
                        tainted.hir.functions.len(),
                        tainted.mono_specializations.len()
                    ),
                });
                check_rows.push(Check {
                    name: "monomorphization".into(),
                    status: "PASS".into(),
                    detail: format!(
                        "static_specializations={} (codegen remains AnubisValue-erased)",
                        tainted.mono_specializations.len()
                    ),
                });
                if !tainted.taint_labels.is_empty() || !tainted.taint_traces.is_empty() {
                    check_rows.push(Check {
                        name: "taint".into(),
                        status: "PASS".into(),
                        detail: format!(
                            "labels={} traces={}",
                            tainted.taint_labels.len(),
                            tainted.taint_traces.len()
                        ),
                    });
                }
                check_rows.push(Check {
                    name: "symbolic".into(),
                    status: if tainted.constraints.is_empty() {
                        "FAIL"
                    } else {
                        "PASS"
                    }
                    .into(),
                    detail: format!("constraints={}", tainted.constraints.len()),
                });
                check_rows.push(Check {
                    name: "solver".into(),
                    status: if crate::middle::solver_checks_discharged(&solver_checks) {
                        "PASS"
                    } else {
                        "FAIL"
                    }
                    .into(),
                    detail: solver_checks
                        .iter()
                        .map(|check| format!("{}={}", check.name, check.status))
                        .collect::<Vec<_>>()
                        .join(","),
                });
                let refusals = crate::middle::solver_stream_refusals(&solver_checks);
                if !refusals.is_empty() {
                    check_refusal = Some(crate::middle::format_check_failures(&refusals));
                }
                // Use the same typed discharge rule as check/build/run and the evidence manifest.
                // A no-obligations sentinel admits development checking without adding proof coverage;
                // UNKNOWN or an unrecognized status can never produce a PASS claim.
                (solver_obligations, solver_all_discharged) = solver_claim_summary(&solver_checks);
                // PCA v2 counted the synthetic check as an obligation. Retain that historical
                // count only while re-deriving a v2 bundle; v3/v4 count real obligations.
                // Both versions now require typed PASS for every real emitted check.
                if pca_version == 2 {
                    solver_obligations = solver_checks.len();
                }
                derived_solver_checks = solver_checks;
            }
            Err(reason) => {
                check_rows.push(Check {
                    name: "typecheck".into(),
                    status: "FAIL".into(),
                    detail: reason.clone(),
                });
                check_refusal = Some(reason);
            }
        }
    }
    let verdict = if parse_ok && typecheck_ok && solver_all_discharged {
        "PASS"
    } else {
        "FAIL"
    };
    Derived {
        claim: ClaimBlock {
            pca_version,
            source_sha256,
            mode: mode.to_string(),
            evidence_lane: None,
            claim_kind: (pca_version == PCA_VERSION_CURRENT).then(|| "source_only_v1".into()),
            tier: "checked".into(),
            rejection: None,
            parse_ok,
            typecheck_ok,
            solver_obligations,
            solver_all_discharged,
            solver_execution: (pca_version == PCA_VERSION_CURRENT).then_some(if solver_executed {
                SolverExecution::Ran
            } else {
                SolverExecution::NotRun
            }),
            typecheck_refusal_kind: (pca_version == PCA_VERSION_CURRENT)
                .then_some(typecheck_refusal_kind)
                .flatten(),
            solver_backend: if pca_version == PCA_VERSION_CURRENT {
                "unobserved".into()
            } else {
                default_solver_backend()
            },
            zk_present: false,
            zk_image_id: None,
            zk_receipt_sha256: None,
            zk_journal_sha256: None,
            verdict: verdict.into(),
            tool: tool_identity(),
        },
        source_mode,
        unresolved_mode_elevator,
        check_rows,
        solver_checks: derived_solver_checks,
        hir_json,
        mir_json,
        taint_json,
        mono_json,
        proof_file_rows,
        check_refusal,
        replayable_security_refusal: replayable_security_refusal_found,
        limit,
        limit_text,
        by_reserve,
        kept_finding,
    }
}

/// A ZK receipt binding derived STRUCTURALLY from a bundle's risc0 sidecars: the guest-bound
/// ImageID, the receipt digest (recomputed from the bundle's own `receipt.bin`), and the committed
/// journal digest. `Some` only when the bundle carries a genuine receipt — non-placeholder,
/// non-dev, non-mock, `verify_status=passed`. The CRYPTOGRAPHIC re-verification of the receipt
/// against the ImageID happens in the CLI (which links risc0); this derives the deterministic facts
/// the claim records so `verify` can re-derive and cross-check them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkBinding {
    pub image_id: String,
    pub receipt_sha256: String,
    pub journal_sha256: String,
}

pub fn derive_zk_binding(dir: &Path) -> Option<ZkBinding> {
    use std::io::Read;

    let r = dir.join("backend").join("risc0");
    for ancestor in [dir.to_path_buf(), dir.join("backend"), r.clone()] {
        if !std::fs::symlink_metadata(ancestor)
            .ok()?
            .file_type()
            .is_dir()
        {
            return None;
        }
    }
    let receipt_path = r.join("receipt.bin");
    let image_id_path = r.join("image_id.txt");
    let meta_path = r.join("risc0_metadata.json");
    if !receipt_path.exists() || !image_id_path.exists() || !meta_path.exists() {
        return None;
    }
    let (mut receipt_file, receipt_len) = open_regular_evidence_file(&receipt_path).ok()??;
    if receipt_len > MAX_EVIDENCE_HASH_BYTES {
        return None;
    }
    // A placeholder receipt is written when proving failed — it is never a binding.
    let marker = b"RISC0_RECEIPT_NOT_GENERATED";
    if receipt_len >= marker.len() as u64 {
        let mut prefix = [0_u8; b"RISC0_RECEIPT_NOT_GENERATED".len()];
        receipt_file.read_exact(&mut prefix).ok()?;
        if prefix == *marker {
            return None;
        }
    }
    let image_id = read_regular_evidence_text(&image_id_path, 1024)
        .ok()??
        .trim()
        .to_string();
    // A real ImageID is eight whitespace-separated u32 words (not the failure sentinel).
    let words: Vec<&str> = image_id.split_whitespace().collect();
    if words.len() != 8 || words.iter().any(|w| w.parse::<u32>().is_err()) {
        return None;
    }
    let meta: serde_json::Value = serde_json::from_str(
        &read_regular_evidence_text(&meta_path, MAX_EVIDENCE_JSON_BYTES).ok()??,
    )
    .ok()?;
    let is_real = meta.get("verify_status").and_then(|v| v.as_str()) == Some("passed")
        && meta
            .get("fresh_receipt_generated")
            .and_then(|v| v.as_bool())
            == Some(true)
        && meta.get("dev_mode").and_then(|v| v.as_bool()) == Some(false)
        && meta.get("mock_prover").and_then(|v| v.as_bool()) == Some(false)
        && meta
            .get("image_id_is_placeholder")
            .and_then(|v| v.as_bool())
            == Some(false);
    if !is_real {
        return None;
    }
    // Recompute the receipt digest from the bundle's own bytes (so a tampered receipt mismatches),
    // and the ImageID recorded in metadata must agree with image_id.txt.
    if meta.get("image_id").and_then(|v| v.as_str()) != Some(image_id.as_str()) {
        return None;
    }
    let journal_sha256 = meta
        .get("committed_journal_sha256")
        .and_then(|v| v.as_str())?
        .to_string();
    if !evidence_digest_ok(&journal_sha256) {
        return None;
    }
    let mut hashed_bytes = 0;
    let receipt_sha256 =
        hash_evidence_file(&receipt_path, &mut hashed_bytes, MAX_EVIDENCE_HASH_BYTES).ok()??;
    Some(ZkBinding {
        image_id,
        receipt_sha256,
        journal_sha256,
    })
}

/// The full claim block for a bundle: the source-derived analysis claim, plus a ZK receipt binding
/// when the bundle carries a genuine receipt. Used both when emitting a PCA and when verifying one,
/// so the two agree exactly (including the ZK fields).
pub fn derive_claim_block_bound(dir: &Path, source: &str, mode: &str) -> ClaimBlock {
    derive_claim_bound(dir, source, mode).claim
}

/// [`derive_claim_block_bound`], and how its analysis ended.
fn derive_claim_bound(dir: &Path, source: &str, mode: &str) -> Derived {
    derive_claim_bound_for_version(dir, source, mode, PCA_VERSION_CURRENT)
}

fn derive_claim_bound_for_version(
    dir: &Path,
    source: &str,
    mode: &str,
    pca_version: u32,
) -> Derived {
    let mut d = derive_claim_for_version(source, mode, true, pca_version);
    if let Some(zk) = derive_zk_binding(dir) {
        d.claim.zk_present = true;
        d.claim.zk_image_id = Some(zk.image_id);
        d.claim.zk_receipt_sha256 = Some(zk.receipt_sha256);
        d.claim.zk_journal_sha256 = Some(zk.journal_sha256);
    }
    d
}

#[cfg(test)]
pub(crate) fn derive_version_three_claim_for_test(
    dir: &Path,
    source: &str,
    mode: &str,
) -> ClaimBlock {
    derive_claim_bound_for_version(dir, source, mode, 3).claim
}

/// Verify a Proof-Carrying Artifact: first the hash / tamper validation, then — the PCA hardening —
/// RE-DERIVE the claim block from the bundle's own source and confirm it matches the recorded
/// `pca.json` exactly. A bundle whose recorded verdict does not match what the source actually
/// proves fails closed, even if every hash was recomputed to look internally consistent.
/// Compare a freshly re-derived claim against the recorded one over every field that is a function
/// of the source and the bundle's own artifacts, IGNORING the `tool` provenance string.
///
/// `tool` records which build produced the bundle (`anubis <version>`). It is recorded in `pca.json`
/// and tamper-protected by `MANIFEST.sha256` (and the signature, when signed), but it is NOT
/// re-derivable from the source — `verify` can only ever recompute the *verifying* tool's own
/// version. Requiring the two to match would make a valid bundle fail cold-verification under any
/// other tool version, breaking the "a stranger can cold-verify" guarantee (a false negative). Every
/// field that IS re-derivable must still match exactly, so a tampered claim — wrong verdict, flipped
/// typecheck, swapped ZK binding, altered obligation count — still fails closed here.
fn claim_semantically_matches(fresh: &ClaimBlock, recorded: &ClaimBlock) -> bool {
    let mut neutralized = fresh.clone();
    neutralized.tool = recorded.tool.clone();
    neutralized == *recorded
}

fn finish_pca_rederivation(
    matches: bool,
    derived: &Derived,
    recorded: &ClaimBlock,
) -> Result<bool, String> {
    // A limit can produce the same FAIL-shaped booleans on both sides without
    // deciding the claim. Matching serialized fields are not a completed check.
    if derived.limit.is_some() && !derived.kept_finding {
        return Err(match derived.limit {
            Some(crate::middle::AnalysisLimit::Memory) if derived.by_reserve => {
                "ANUBIS_ANALYSIS_LIMIT: PCA re-derivation reached the memory reserve; the intact claim is undecided"
            }
            Some(crate::middle::AnalysisLimit::Memory) => {
                "ANUBIS_ANALYSIS_LIMIT: PCA re-derivation reached its memory budget; the intact claim is undecided"
            }
            _ => "ANUBIS_ANALYSIS_LIMIT: PCA re-derivation reached a stack or closure-depth limit; the intact claim is undecided",
        }
        .into());
    }
    if !matches && derived.limit == Some(crate::middle::AnalysisLimit::Memory) {
        if derived.kept_finding && (recorded.typecheck_ok || recorded.verdict == "PASS") {
            return Ok(false);
        }
        return Err(if derived.by_reserve {
            "ANUBIS_ANALYSIS_LIMIT: the claim could not be re-derived: the memory left to the \
             check (on the machine, or in the memory-capped cgroup it runs in) fell below the \
             reserve the checker keeps free, so the intact bundle is neither confirmed nor \
             refuted (other processes are using the memory it needs, and raising \
             ANUBIS_ANALYSIS_MEMORY_MIB does not change that: run it with more memory free)"
        } else {
            "ANUBIS_ANALYSIS_LIMIT: the claim could not be re-derived: the checker's \
             analysis reached its memory budget on this machine, so the intact bundle is \
             neither confirmed nor refuted (give the check more memory: \
             ANUBIS_ANALYSIS_MEMORY_MIB, in MiB)"
        }
        .into());
    }
    Ok(matches)
}

/// A source check (accepted or rejected) is analysis, not execution. Its hash manifest may
/// cover extra files after an attacker rehashes it, so allow only producer-emitted files.
/// This excludes an unattested native artifact, RISC0 receipt/journal/guest, execution report,
/// or `program-evidence.json` claiming a completed program while the command was refused.
fn source_check_tree_matches(dir: &Path, derived: &Derived) -> Result<bool, String> {
    use std::collections::BTreeSet;

    let mut files: BTreeSet<String> = [
        "MANIFEST.sha256",
        "pca.sig",
        "source.anubis",
        "build.log",
        "hir.json",
        "mir.json",
        "taint-traces.json",
        "solver.json",
        "mono_specializations.json",
        "environment.json",
        "checks.sarif",
        "bounty-report.md",
        "validate.sh",
        "source-tree.json",
        "analysis/check-config.json",
        "evidence.json",
        "manifest.json",
        "pca.json",
        "summaries.json",
        "declassify_audit.json",
        "confinement_manifest.json",
        "entitlement_profile.json",
        "program.entitlements",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let mut directories: BTreeSet<String> = ["analysis"].into_iter().map(str::to_string).collect();
    if derived.claim.typecheck_ok {
        directories.insert("analysis/proofs".into());
        files.insert("analysis/proofs.json".into());
        files.insert("analysis/solver.smt2".into());
        files.insert("analysis/solver_replay.json".into());
        for (index, accepts_refutation) in derived.proof_file_rows.iter().enumerate() {
            let stem = format!("analysis/proofs/obligation_{index:04}");
            files.insert(format!("{stem}.smt2"));
            if *accepts_refutation {
                files.insert(format!("{stem}.cnf"));
                files.insert(format!("{stem}.drat"));
            }
        }
    }
    let source_listing = dir.join("source-merkle-leaves.json");
    if source_listing.exists() {
        let Some(bytes) = read_regular_evidence_bytes(&source_listing, MAX_EVIDENCE_JSON_BYTES)?
        else {
            return Ok(false);
        };
        let Ok(listing) = serde_json::from_slice::<SealedSourceLeaves>(&bytes) else {
            return Ok(false);
        };
        files.insert("source-merkle-leaves.json".into());
        directories.insert("source-leaves".into());
        for leaf in listing.leaves {
            files.insert(leaf.sealed_path);
        }
    }

    fn walk(
        current: &Path,
        prefix: &str,
        files: &BTreeSet<String>,
        directories: &BTreeSet<String>,
        visited: &mut usize,
    ) -> Result<bool, String> {
        for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
            *visited = visited
                .checked_add(1)
                .ok_or("source-check tree entry count overflow")?;
            if *visited > MAX_EVIDENCE_TREE_ENTRIES {
                return Err("source-check tree entry budget exceeded".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                return Ok(false);
            };
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let kind = std::fs::symlink_metadata(entry.path())
                .map_err(|e| e.to_string())?
                .file_type();
            if kind.is_dir() {
                if !directories.contains(&relative)
                    || !walk(&entry.path(), &relative, files, directories, visited)?
                {
                    return Ok(false);
                }
            } else if !kind.is_file() || !files.contains(&relative) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    let mut visited = 0;
    walk(dir, "", &files, &directories, &mut visited)
}

fn evidence_bytes_match(dir: &Path, name: &str, expected: &[u8]) -> Result<bool, String> {
    use std::io::Read;

    let Some((mut file, declared_len)) = open_regular_evidence_file(&dir.join(name))? else {
        return Ok(false);
    };
    if declared_len > MAX_EVIDENCE_HASH_BYTES || declared_len != expected.len() as u64 {
        return Ok(false);
    }
    let mut buf = [0u8; 64 * 1024];
    let mut offset = 0;
    while offset < expected.len() {
        let count = buf.len().min(expected.len() - offset);
        if file.read_exact(&mut buf[..count]).is_err()
            || buf[..count] != expected[offset..offset + count]
        {
            return Ok(false);
        }
        offset += count;
    }
    // Detect growth after the opened length was checked, without allocating its contents.
    let mut extra = [0u8; 1];
    Ok(file.read(&mut extra).map_err(|e| e.to_string())? == 0)
}

fn evidence_json_matches<T: Serialize>(
    dir: &Path,
    name: &str,
    expected: &T,
) -> Result<bool, String> {
    let bytes = serde_json::to_vec_pretty(expected).map_err(|e| e.to_string())?;
    evidence_bytes_match(dir, name, &bytes)
}

/// Package summaries and the declassification audit are authority-bearing source claims. The
/// check producer writes each only when extraction succeeds; its verifier must make the same
/// decision and compare the exact canonical bytes, including the absence case.
fn source_claim_sidecars_match(
    dir: &Path,
    source: &str,
    package_publish: bool,
) -> Result<bool, String> {
    let summary_ok = if package_publish {
        // The publisher replaces the provisional single-source summary with package name,
        // version, and module merkle. The package identity is NOT established by this PCA:
        // the resolver must compare this exact canonical record to the mounted package.
        let path = dir.join(crate::package::summary::SUMMARIES_FILENAME);
        match read_regular_evidence_text(&path, MAX_EVIDENCE_JSON_BYTES)? {
            Some(text) => {
                match serde_json::from_str::<crate::package::summary::PackageSummaries>(&text) {
                    Ok(summary) if summary.schema == crate::package::summary::SUMMARIES_SCHEMA => {
                        evidence_json_matches(
                            dir,
                            crate::package::summary::SUMMARIES_FILENAME,
                            &summary,
                        )?
                    }
                    _ => false,
                }
            }
            None => false,
        }
    } else {
        let summaries =
            crate::package::summary::extract_from_source_text("package", "0.0.0", source);
        match summaries {
            Ok(expected) => {
                evidence_json_matches(dir, crate::package::summary::SUMMARIES_FILENAME, &expected)?
            }
            Err(_) => !dir
                .join(crate::package::summary::SUMMARIES_FILENAME)
                .exists(),
        }
    };
    let audit = crate::package::summary::extract_declassify_audit("package", "0.0.0", source);
    let audit_ok = match audit {
        Ok(expected) => evidence_json_matches(
            dir,
            crate::package::summary::DECLASSIFY_AUDIT_FILENAME,
            &expected,
        )?,
        Err(_) => !dir
            .join(crate::package::summary::DECLASSIFY_AUDIT_FILENAME)
            .exists(),
    };
    Ok(summary_ok && audit_ok)
}

/// New source-derived check bundles must contain the producer's exact policy projections. A
/// serde round-trip is insufficient: ignored fields or duplicate JSON keys can hide extra grants.
fn source_policy_sidecars_match(dir: &Path, source: &str) -> Result<bool, String> {
    let confinement = crate::package::confinement::derive_confinement("package", "0.0.0", source);
    let confinement_ok = match confinement {
        Ok(expected) => evidence_json_matches(
            dir,
            crate::package::confinement::CONFINEMENT_FILENAME,
            &expected,
        )?,
        Err(_) => !dir
            .join(crate::package::confinement::CONFINEMENT_FILENAME)
            .exists(),
    };
    let entitlement =
        crate::package::entitlements::derive_entitlement_profile("package", "0.0.0", source);
    let entitlement_ok = match entitlement {
        Ok(expected) => {
            evidence_json_matches(
                dir,
                crate::package::entitlements::ENTITLEMENT_PROFILE_FILENAME,
                &expected,
            )? && evidence_bytes_match(
                dir,
                crate::package::entitlements::ENTITLEMENT_PLIST_FILENAME,
                crate::package::entitlements::entitlement_plist_xml(&expected).as_bytes(),
            )?
        }
        Err(_) => {
            !dir.join(crate::package::entitlements::ENTITLEMENT_PROFILE_FILENAME)
                .exists()
                && !dir
                    .join(crate::package::entitlements::ENTITLEMENT_PLIST_FILENAME)
                    .exists()
        }
    };
    Ok(confinement_ok && entitlement_ok)
}

/// Source-derived analysis views, including the proof inventory's exact typed statuses and
/// certificate leaves. A renewed hash list cannot turn a failed obligation into a proof row.
fn analysis_sidecars_match(dir: &Path, derived: &Derived) -> Result<bool, String> {
    for (name, value) in [
        ("hir.json", &derived.hir_json),
        ("mir.json", &derived.mir_json),
        ("taint-traces.json", &derived.taint_json),
        ("mono_specializations.json", &derived.mono_json),
    ] {
        let Some(value) = value else {
            return Ok(false);
        };
        if !evidence_json_matches(dir, name, value)? {
            return Ok(false);
        }
    }
    // The producer first converts SolverCheck structs into serde_json::Value, whose map keys
    // serialize in canonical order. Reproduce that step before comparing bytes.
    let Ok(solver_json) = serde_json::to_value(&derived.solver_checks) else {
        return Ok(false);
    };
    if !evidence_json_matches(dir, "solver.json", &solver_json)? {
        return Ok(false);
    }
    if !derived.claim.typecheck_ok {
        return Ok(!dir.join("analysis/proofs.json").exists()
            && !dir.join("analysis/solver.smt2").exists()
            && !dir.join("analysis/solver_replay.json").exists());
    }

    let Some(first) = derived.solver_checks.first() else {
        return Ok(false);
    };
    if !evidence_bytes_match(dir, "analysis/solver.smt2", first.smt.as_bytes())? {
        return Ok(false);
    }
    let replay = solver_replay_record(&derived.solver_checks, crate::middle::replay_counterexample);
    if !evidence_json_matches(dir, "analysis/solver_replay.json", &replay)? {
        return Ok(false);
    }

    let no_obligations = crate::middle::is_no_obligations_sentinel(&derived.solver_checks);
    let mut index = Vec::new();
    for (i, check) in derived.solver_checks.iter().enumerate() {
        let stem = format!("obligation_{i:04}");
        let smt_path = format!("analysis/proofs/{stem}.smt2");
        let cnf_path = format!("analysis/proofs/{stem}.cnf");
        let drat_path = format!("analysis/proofs/{stem}.drat");
        if !evidence_bytes_match(dir, &smt_path, check.smt.as_bytes())? {
            return Ok(false);
        }
        let simple_proof =
            if no_obligations || crate::middle::reserves_no_obligations_identity(check) {
                Some(if no_obligations {
                    "not_applicable_no_obligations"
                } else {
                    "invalid_synthetic_no_obligations"
                })
            } else if let Some((proof, _)) = undecided_provenance(&check.detail) {
                Some(proof)
            } else if check.detail == crate::middle::UNRESOLVED_PRECONDITION_DETAIL {
                Some("unresolved_not_encoded")
            } else {
                None
            };
        if let Some(proof) = simple_proof {
            if dir.join(&cnf_path).exists() || dir.join(&drat_path).exists() {
                return Ok(false);
            }
            index.push(serde_json::json!({
                "obligation": check.name,
                "status": check.status,
                "proof": proof,
                "smt": smt_path,
            }));
            continue;
        }
        match anubis_solver::native_prove_with_artifacts(&check.smt) {
            Some((anubis_solver::NativeVerdict::Unsat, Some(artifacts)))
                if check.status == "PASS" =>
            {
                if !evidence_bytes_match(dir, &cnf_path, artifacts.cnf_dimacs.as_bytes())?
                    || !evidence_bytes_match(dir, &drat_path, artifacts.proof_drat.as_bytes())?
                {
                    return Ok(false);
                }
                index.push(serde_json::json!({
                    "obligation": check.name,
                    "status": check.status,
                    "proof": "rup_refutation",
                    "smt": smt_path,
                    "cnf_dimacs": cnf_path,
                    "proof_drat": drat_path,
                    "num_vars": artifacts.num_vars,
                    "num_clauses": artifacts.num_clauses,
                    "steps": artifacts.steps,
                    "checker": artifacts.checker,
                    "checker_version": artifacts.checker_version,
                    "replay": format!("drat-trim analysis/proofs/{stem}.cnf analysis/proofs/{stem}.drat"),
                }));
            }
            other => {
                if dir.join(&cnf_path).exists() || dir.join(&drat_path).exists() {
                    return Ok(false);
                }
                let proof = match other {
                    Some((anubis_solver::NativeVerdict::Sat(_), _)) => {
                        "counterexample_no_refutation"
                    }
                    Some((anubis_solver::NativeVerdict::Unsat, _)) if check.status != "PASS" => {
                        "refutation_not_accepted_by_check"
                    }
                    Some(_) => "unsat_without_published_certificate",
                    None => "declined_by_native_solver_deferred",
                };
                index.push(serde_json::json!({
                    "obligation": check.name,
                    "status": check.status,
                    "proof": proof,
                    "smt": smt_path,
                }));
            }
        }
    }
    evidence_json_matches(
        dir,
        "analysis/proofs.json",
        &serde_json::json!({
            "note": "Re-checkable proof objects. DIMACS CNF + DRAT refutation per \
                     proven obligation; verify with drat-trim or any DRAT checker. \
                     No Anubis binary required.",
            "obligations": index,
        }),
    )
}

fn presentation_sidecars_match(dir: &Path, manifest: &EvidenceManifest) -> Result<bool, String> {
    Ok(
        evidence_json_matches(dir, "checks.sarif", &build_sarif(&manifest.checks))?
            && evidence_bytes_match(
                dir,
                "bounty-report.md",
                build_bounty_report(&manifest.mode, manifest.lane.as_deref(), &manifest.checks)
                    .as_bytes(),
            )?
            && evidence_bytes_match(dir, "validate.sh", VALIDATE_SH.as_bytes())?,
    )
}

/// A typed source comparison must not silently discard unknown JSON keys, including nested
/// grants or sandbox fields. Permit only the documented historical default field when absent.
fn sealed_json_shape_matches<T: Serialize>(
    text: &str,
    sealed: &T,
    historical_default: Option<&str>,
) -> bool {
    let (Ok(raw), Ok(mut typed)) = (
        serde_json::from_str::<serde_json::Value>(text),
        serde_json::to_value(sealed),
    ) else {
        return false;
    };
    if let Some(field) = historical_default {
        if raw
            .as_object()
            .is_some_and(|object| !object.contains_key(field))
        {
            if let Some(object) = typed.as_object_mut() {
                object.remove(field);
            }
        }
    }
    raw == typed
}

/// Return the independently established scope, rather than letting callers treat a
/// historic source verdict as verification of newer check sidecars.
pub fn verify_pca_scope(dir: &Path) -> Result<Option<PcaScope>, String> {
    verify_pca_inner(dir)
}

/// Compatibility predicate. Security and presentation consumers must use the typed scope.
pub fn verify_pca(dir: &Path) -> Result<bool, String> {
    Ok(verify_pca_scope(dir)?.is_some())
}

fn verify_pca_inner(dir: &Path) -> Result<Option<PcaScope>, String> {
    let hashes_ok = validate_bundle_recorded_files(dir, false)?;
    if !hashes_ok {
        return Ok(None);
    }
    let pca_path = dir.join("pca.json");
    if !pca_path.exists() {
        // Semantic verification cannot degrade to hash-only success. Callers that intentionally
        // need legacy integrity checking must name and use `validate_bundle` instead of presenting
        // that weaker result as PCA verification.
        return Ok(None);
    }
    let Some(pca_text) = read_regular_evidence_text(&pca_path, MAX_EVIDENCE_JSON_BYTES)? else {
        return Ok(None);
    };
    let recorded: ClaimBlock = serde_json::from_str(&pca_text).map_err(|e| e.to_string())?;
    if !matches!(recorded.pca_version, 2 | 3 | PCA_VERSION_CURRENT)
        || !matches!(recorded.mode.as_str(), "safe" | "research" | "exploit")
    {
        return Ok(None);
    }
    let Some(manifest_text) =
        read_regular_evidence_text(&dir.join("evidence.json"), MAX_EVIDENCE_JSON_BYTES)?
    else {
        return Ok(None);
    };
    let manifest: EvidenceManifest =
        serde_json::from_str(&manifest_text).map_err(|e| e.to_string())?;
    // Current source-check authority requires the source analyzed by PCA to be the source the
    // command checked. A Merkle root authenticates each leaf's bytes, but does not establish that
    // `source.anubis` is the correct resolved program for an `entry/...` leaf. Until resolution
    // emits a checked correspondence witness, current multi-leaf claims have no PCA scope.
    if recorded.pca_version == PCA_VERSION_CURRENT && dir.join("source-merkle-leaves.json").exists()
    {
        return Ok(None);
    }
    // Archived PCA v2 receipts and build/proof bundles use different sidecar schemas. The
    // source-derived check contract applies only to the current plain check lane; other lanes
    // retain their existing source claim and signature checks without being presented as if
    // these newer sidecars had been independently re-derived.
    let plain_check_lane =
        manifest.lane.as_deref() == Some(format!("{}-check", recorded.mode).as_str());
    let package_publish_lane =
        recorded.mode == "safe" && manifest.lane.as_deref() == Some(PACKAGE_PUBLISH_LANE);
    let scope_fields_match = match recorded.pca_version {
        2 | 3 => recorded.claim_kind.is_none() && recorded.evidence_lane.is_none(),
        PCA_VERSION_CURRENT => {
            recorded.evidence_lane == manifest.lane
                && recorded.claim_kind.as_deref()
                    == Some(if plain_check_lane {
                        "source_check_v1"
                    } else if package_publish_lane {
                        "package_publish_v1"
                    } else if manifest
                        .lane
                        .as_deref()
                        .is_some_and(|lane| lane.ends_with("-invalid-input-check"))
                    {
                        "invalid_input_unverified_v1"
                    } else if manifest
                        .lane
                        .as_deref()
                        .is_some_and(|lane| lane.ends_with("-analysis-limit-check"))
                    {
                        "analysis_limit_undecided_v1"
                    } else if manifest
                        .lane
                        .as_deref()
                        .is_some_and(|lane| lane.ends_with("-typecheck-refusal-check"))
                    {
                        "typecheck_refusal_unverified_v1"
                    } else {
                        "source_only_v1"
                    })
        }
        _ => false,
    };
    let source_derived_check_lane =
        recorded.pca_version == PCA_VERSION_CURRENT && (plain_check_lane || package_publish_lane);
    // The producer writes identical bytes to both manifest names. MANIFEST.sha256 binds each
    // file separately, but an attacker who can regenerate unsigned hashes can otherwise make
    // the public manifest.json tell a different story from the checked evidence.json.
    let manifest_mirror_matches = matches!(
        read_regular_evidence_bytes(&dir.join("manifest.json"), MAX_EVIDENCE_JSON_BYTES)?,
        Some(bytes) if bytes.as_slice() == manifest_text.as_bytes()
    );
    // Reject unrecognized top-level fields as well as a divergent mirror. In particular, a
    // rehashed free-form `authorization` key must not acquire authority by being ignored here.
    let manifest_shape_matches = !source_derived_check_lane
        || serde_json::to_string_pretty(&manifest).is_ok_and(|text| text == manifest_text);
    // No engagement authorization or scope is derivable from source in this PCA. The producer's
    // default context is descriptive only; any supplied security block needs a separate verifier.
    let security_context_matches = !source_derived_check_lane
        || manifest.security.as_ref()
            == Some(&serde_json::json!({
                "mode": recorded.mode.as_str(),
                "note": "language attributes and effects recorded in checks and logs"
            }));
    let presentation_ok =
        !source_derived_check_lane || presentation_sidecars_match(dir, &manifest)?;
    // Exact solver/refusal bytes can be replayed only under the recorded solver and
    // compiler versions. A different installation is an unsupported cold verifier,
    // never a reason to weaken the sidecar comparison to matching status strings.
    let strict_toolchain_matches = if source_derived_check_lane {
        let environment =
            read_regular_evidence_text(&dir.join("environment.json"), MAX_EVIDENCE_JSON_BYTES)?;
        environment
            .and_then(|text| {
                serde_json::from_str::<EnvironmentCapture>(&text)
                    .ok()
                    .map(|captured| (text, captured))
            })
            .is_some_and(|(text, captured)| {
                serde_json::to_string_pretty(&captured).ok().as_deref() == Some(text.as_str())
                    && captured.z3 == command_output("z3", &["--version"])
                    && captured.anubis == env!("CARGO_PKG_VERSION")
                    && recorded.tool == tool_identity()
                    && captured.machine_fields_status == "producer_reported_unverified"
            })
    } else {
        true
    };
    // A program-level disproof cannot coexist with an artifact manifest that
    // reports every recorded check and the whole build as PASS. The converse
    // is possible: source analysis may pass while a build or platform check
    // fails, so require only this one-way consistency relation.
    let verdicts_consistent = recorded.verdict != "FAIL" || manifest.verdict == "FAIL";
    let modes_consistent = recorded.mode == manifest.mode;
    let Some(source) =
        read_regular_evidence_text(&dir.join("source.anubis"), MAX_EVIDENCE_SOURCE_BYTES)?
    else {
        return Ok(None);
    };
    // Explicit source binding: the claim's recorded hash must be the hash of the bundle's own
    // source. (Also implied by `fresh == recorded`, but asserted directly so the source↔claim tie
    // can never drift.)
    let source_bound = recorded.source_sha256 == sha256_bytes(source.as_bytes());
    // If the bundle is signed, the signature must verify over the current claim + manifest. An
    // unsigned bundle is still a valid (unsigned) PCA. A forged/invalid signature fails closed.
    let sig_ok = match pca_signature_status(dir)? {
        Some((ok, _signer)) => ok,
        None => true,
    };
    // VZ confinement cross-check (consistent-BY-CONSTRUCTION, not merely tamper-evident): if the
    // bundle carries a confinement manifest, RE-DERIVE it from the bundle's own source and fail
    // closed on any drift. A grant that contradicts the proven effect set — e.g. a hand-forged
    // `network:host-only` over a source that provably uses `net.send` — cannot survive the re-derive
    // (which is a pure function of the source), so a forged or source-swapped grant is rejected here.
    let check_config_ok = !source_derived_check_lane
        || evidence_json_matches(dir, "analysis/check-config.json", &default_check_config())?;
    let strict_policy_ok = !source_derived_check_lane
        || with_default_check_analysis(|| source_policy_sidecars_match(dir, &source))?;
    let source_claim_sidecars_ok = !source_derived_check_lane
        || source_claim_sidecars_match(dir, &source, package_publish_lane)?;
    let confine_ok = if source_derived_check_lane {
        true // strict_policy_ok covers both policy files and their canonical bytes
    } else {
        let cm_path = dir.join(crate::package::confinement::CONFINEMENT_FILENAME);
        if cm_path.exists() {
            match read_regular_evidence_text(&cm_path, MAX_EVIDENCE_JSON_BYTES) {
                Ok(Some(text)) => {
                    match serde_json::from_str::<crate::package::confinement::ConfinementManifest>(
                        &text,
                    ) {
                        Ok(sealed) => {
                            sealed_json_shape_matches(&text, &sealed, Some("research_effects"))
                                && crate::package::confinement::verify_confinement_matches_source(
                                    &source, &sealed,
                                )
                                .is_ok()
                        }
                        Err(_) => false,
                    }
                }
                _ => false, // a malformed sealed confinement manifest fails closed
            }
        } else {
            true // legacy bundle without a confinement manifest
        }
    };
    // The JSON profile is source-derived, and the codesign plist must be its exact canonical
    // rendering. Otherwise a rehashed plist could grant authority absent from the checked
    // profile. The producer emits both files together; a lone sidecar is not a valid pair.
    let entitlement_ok = if source_derived_check_lane {
        true // strict_policy_ok covers the profile and the exact derived plist
    } else {
        let ep_path = dir.join(crate::package::entitlements::ENTITLEMENT_PROFILE_FILENAME);
        let plist_path = dir.join(crate::package::entitlements::ENTITLEMENT_PLIST_FILENAME);
        match (ep_path.exists(), plist_path.exists()) {
            (false, false) => true, // legacy bundle without entitlement sidecars
            (true, true) => match read_regular_evidence_text(&ep_path, MAX_EVIDENCE_JSON_BYTES) {
                Ok(Some(text)) => {
                    match serde_json::from_str::<crate::package::entitlements::EntitlementProfile>(
                            &text,
                        ) {
                            Ok(sealed) => {
                                sealed_json_shape_matches(&text, &sealed, None)
                                    && crate::package::entitlements::verify_entitlement_profile_matches_source(
                                        &source, &sealed,
                                    )
                                    .is_ok()
                                    && matches!(
                                        read_regular_evidence_bytes(&plist_path, MAX_EVIDENCE_JSON_BYTES),
                                        Ok(Some(bytes)) if bytes.as_slice() == crate::package::entitlements::entitlement_plist_xml(&sealed).as_bytes()
                                    )
                            }
                            Err(_) => false,
                        }
                }
                _ => false,
            },
            _ => false,
        }
    };
    // A claim no derivation produces is refuted as it stands: a PASS needs the parse, the type
    // check and every obligation discharged, and no rejection (a forged PASS with `typecheck_ok:
    // false` was answered "could not be re-derived" when the re-derivation stopped at a limit;
    // eighth review of the checker limits, E3).
    let consistent = recorded.verdict != "PASS"
        || (recorded.parse_ok
            && recorded.typecheck_ok
            && recorded.solver_all_discharged
            && (recorded.pca_version != PCA_VERSION_CURRENT
                || solver_execution_matches_obligations(&recorded))
            && recorded.rejection.is_none());
    // Integrity decides first, before any re-derivation (which can stop at a limit, or end the
    // process at the hard memory budget): a bundle whose files, source binding, signature,
    // confinement or entitlements do not check is invalid, whatever the analysis could re-derive.
    if !(hashes_ok
        && scope_fields_match
        && manifest_mirror_matches
        && manifest_shape_matches
        && security_context_matches
        && presentation_ok
        && strict_toolchain_matches
        && verdicts_consistent
        && modes_consistent
        && source_bound
        && sig_ok
        && confine_ok
        && entitlement_ok
        && check_config_ok
        && strict_policy_ok
        && source_claim_sidecars_ok
        && consistent)
    {
        return Ok(None);
    }
    // Re-derive the full claim — including the ZK binding — from the bundle's own artifacts. A
    // tampered receipt, a swapped ImageID, or a claim that lies about carrying a receipt makes the
    // re-derived block differ from the recorded one and fails closed here (the CLI additionally
    // re-verifies the receipt cryptographically against the ImageID).
    let derived = if source_derived_check_lane {
        with_default_check_analysis(|| {
            derive_claim_bound_for_version(dir, &source, &recorded.mode, recorded.pca_version)
        })
    } else {
        derive_claim_bound_for_version(dir, &source, &recorded.mode, recorded.pca_version)
    };
    let source_mode_matches = (recorded.pca_version != PCA_VERSION_CURRENT
        || !derived.unresolved_mode_elevator)
        && match derived.source_mode {
            Some(crate::frontend::Mode::Safe) => recorded.mode == "safe",
            Some(crate::frontend::Mode::Research) => recorded.mode == "research",
            Some(crate::frontend::Mode::Exploit) => recorded.mode == "exploit",
            // `None` covers a parsed source with no function mode as well as a parse refusal.
            // Only the former may take the CLI's Safe default; a malformed source must never
            // acquire current check authority by rehashing an exact FAIL-shaped parse row.
            None => derived.claim.parse_ok && recorded.mode == "safe",
        };
    // The CLI records --verified in a distinct lane. Until PCA derivation itself runs that
    // stronger typecheck and binds the option into the claim, this verifier cannot confirm it.
    let unsupported_verified_lane = manifest
        .lane
        .as_deref()
        .is_some_and(|lane| lane.ends_with("-verified-check"));
    let core_names: std::collections::BTreeSet<&str> = [
        "parse",
        "typecheck",
        "monomorphization",
        "taint",
        "symbolic",
        "solver",
    ]
    .into_iter()
    .collect();
    let recorded_core: Vec<Check> = manifest
        .checks
        .iter()
        .filter(|row| core_names.contains(row.name.as_str()))
        .cloned()
        .collect();
    let core_rows_match = !source_derived_check_lane || recorded_core == derived.check_rows;
    // A pure check emits only source-derived rows plus the sealed source/log hashes. Build and
    // platform lanes have additional producer-reported rows with distinct trust boundaries.
    let plain_check_rows_match = if source_derived_check_lane && recorded.rejection.is_none() {
        let mut expected_rows = derived.check_rows.clone();
        expected_rows.push(Check {
            name: "source_hash".into(),
            status: "PASS".into(),
            detail: manifest.source_hash.clone(),
        });
        expected_rows.push(Check {
            name: "build_log_hash".into(),
            status: "PASS".into(),
            detail: manifest.build_log_hash.clone(),
        });
        manifest.checks == expected_rows && manifest.artifact_hash.is_none()
    } else {
        true
    };
    // A rejected check has an additional command-level refusal. Verify that refusal against the
    // sealed source and the manifest row, then compare every other PCA field as usual. Do not
    // accept arbitrary rejected build/platform/tool errors merely because their source also has a
    // failing obligation; those need their own typed and independently reproducible provenance.
    let mut expected = derived.claim.clone();
    if recorded.pca_version == PCA_VERSION_CURRENT {
        expected.evidence_lane = manifest.lane.clone();
        expected.claim_kind = Some(
            if plain_check_lane {
                "source_check_v1"
            } else if package_publish_lane {
                "package_publish_v1"
            } else if manifest
                .lane
                .as_deref()
                .is_some_and(|lane| lane.ends_with("-invalid-input-check"))
            {
                "invalid_input_unverified_v1"
            } else if manifest
                .lane
                .as_deref()
                .is_some_and(|lane| lane.ends_with("-analysis-limit-check"))
            {
                "analysis_limit_undecided_v1"
            } else if manifest
                .lane
                .as_deref()
                .is_some_and(|lane| lane.ends_with("-typecheck-refusal-check"))
            {
                "typecheck_refusal_unverified_v1"
            } else {
                "source_only_v1"
            }
            .into(),
        );
    }
    let rejection_matches = match recorded.rejection.as_deref() {
        None => {
            recorded.tier == "checked"
                // A plain check with no command refusal is an accepted check. A rehashed FAIL
                // whose command_rejection row and PCA rejection were both stripped cannot
                // become current check authority, even when its source-derived rows still match.
                && (!source_derived_check_lane || derived.claim.verdict == "PASS")
                && !manifest
                    .checks
                    .iter()
                    .any(|c| c.name == "command_rejection")
        }
        Some(reason) => {
            let mut expected_rows = derived.check_rows.clone();
            expected_rows.push(Check {
                name: "command_rejection".into(),
                status: "FAIL".into(),
                detail: reason.into(),
            });
            expected_rows.push(Check {
                name: "source_hash".into(),
                status: "PASS".into(),
                detail: manifest.source_hash.clone(),
            });
            expected_rows.push(Check {
                name: "build_log_hash".into(),
                status: "PASS".into(),
                detail: manifest.build_log_hash.clone(),
            });
            recorded.tier == "rejected"
                && source_derived_check_lane
                && plain_check_lane
                // A --verified check now has its own lane. Its stronger typecheck is not
                // represented by the default claim re-derivation and remains unverified here.
                && manifest.lane.as_deref() == Some(format!("{}-check", recorded.mode).as_str())
                && manifest.artifact_hash.is_none()
                && derived.claim.verdict == "FAIL"
                && derived.claim.parse_ok
                && (derived.claim.typecheck_ok || derived.replayable_security_refusal)
                && derived.check_refusal.as_deref() == Some(reason)
                && manifest.checks == expected_rows
        }
    };
    if rejection_matches && recorded.rejection.is_some() {
        expected.tier = "rejected".into();
        expected.rejection = recorded.rejection.clone();
    }
    let matches = !unsupported_verified_lane
        && source_mode_matches
        && core_rows_match
        && plain_check_rows_match
        && rejection_matches
        && (!source_derived_check_lane || source_check_tree_matches(dir, &derived)?)
        && (!source_derived_check_lane
            || with_default_check_analysis(|| analysis_sidecars_match(dir, &derived))?)
        && claim_semantically_matches(&expected, &recorded);
    if !finish_pca_rederivation(matches, &derived, &recorded)? {
        return Ok(None);
    }
    Ok(Some(if source_derived_check_lane && package_publish_lane {
        PcaScope::PackagePublishV1
    } else if source_derived_check_lane {
        PcaScope::SourceCheckV1
    } else if recorded.pca_version == PCA_VERSION_CURRENT {
        PcaScope::SourceOnly
    } else {
        PcaScope::LegacySourceOnly
    }))
}

/// The `pca.sig` sidecar: an Ed25519 signature over the PCA, written OUTSIDE `MANIFEST.sha256` (it
/// signs the manifest, so it cannot be part of it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcaSignature {
    pub algorithm: String,
    pub public_key: String,
    pub signature: String,
    pub signed: String,
}

/// Generate a fresh Ed25519 keypair as `(signing_key_hex, verifying_key_hex)` — 32 bytes each.
pub fn generate_keypair() -> Result<(String, String), String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| e.to_string())?;
    let sk = SigningKey::from_bytes(&seed);
    let vk = sk.verifying_key();
    Ok((hex::encode(sk.to_bytes()), hex::encode(vk.to_bytes())))
}

/// The bytes a PCA signature covers: `sha256(pca.json) || sha256(MANIFEST.sha256)`. Signing this
/// binds the signer to both the semantic claim and the whole hashed file tree.
fn pca_signed_message(dir: &Path) -> Result<Vec<u8>, String> {
    let pca = read_regular_evidence_bytes(&dir.join("pca.json"), MAX_EVIDENCE_JSON_BYTES)?
        .ok_or("missing or nonregular pca.json")?;
    let manifest =
        read_regular_evidence_bytes(&dir.join("MANIFEST.sha256"), MAX_EVIDENCE_MANIFEST_BYTES)?
            .ok_or("missing or nonregular MANIFEST.sha256")?;
    let mut msg = Vec::with_capacity(64);
    msg.extend_from_slice(&Sha256::digest(&pca));
    msg.extend_from_slice(&Sha256::digest(&manifest));
    Ok(msg)
}

/// Sign a PCA with an Ed25519 signing key (hex). Writes `pca.sig` and returns the signer's public
/// key (hex). The signature covers the claim block and the manifest root, so any later tamper to
/// either invalidates it.
pub fn sign_pca(dir: &Path, signing_key_hex: &str) -> Result<String, String> {
    let sk_bytes: [u8; 32] = hex::decode(signing_key_hex.trim())
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "signing key must be 32 bytes".to_string())?;
    let sk = SigningKey::from_bytes(&sk_bytes);
    let sig = sk.sign(&pca_signed_message(dir)?);
    let vk_hex = hex::encode(sk.verifying_key().to_bytes());
    write_json(
        &dir.join("pca.sig"),
        &PcaSignature {
            algorithm: "ed25519".into(),
            public_key: vk_hex.clone(),
            signature: hex::encode(sig.to_bytes()),
            signed: "sha256(pca.json)||sha256(MANIFEST.sha256)".into(),
        },
    )?;
    Ok(vk_hex)
}

/// Signature status of a bundle: `None` when unsigned, `Some((verified, signer_public_key))` when a
/// `pca.sig` is present — `verified` is whether the signature checks out over the current PCA.
pub fn pca_signature_status(dir: &Path) -> Result<Option<(bool, String)>, String> {
    let sig_path = dir.join("pca.sig");
    let Some(sig_text) = read_regular_evidence_text(&sig_path, MAX_EVIDENCE_SIGNATURE_BYTES)?
    else {
        if std::fs::symlink_metadata(&sig_path).is_ok() {
            return Err("nonregular pca.sig".into());
        }
        return Ok(None);
    };
    let rec: PcaSignature = serde_json::from_str(&sig_text).map_err(|e| e.to_string())?;
    if rec.algorithm != "ed25519" || rec.signed != "sha256(pca.json)||sha256(MANIFEST.sha256)" {
        return Ok(Some((false, rec.public_key)));
    }
    let vk_bytes: [u8; 32] = match hex::decode(&rec.public_key)
        .ok()
        .and_then(|b| b.try_into().ok())
    {
        Some(b) => b,
        None => return Ok(Some((false, rec.public_key))),
    };
    let sig_bytes: [u8; 64] = match hex::decode(&rec.signature)
        .ok()
        .and_then(|b| b.try_into().ok())
    {
        Some(b) => b,
        None => return Ok(Some((false, rec.public_key))),
    };
    let msg = pca_signed_message(dir)?;
    let verified = match VerifyingKey::from_bytes(&vk_bytes) {
        Ok(vk) => vk.verify(&msg, &Signature::from_bytes(&sig_bytes)).is_ok(),
        Err(_) => false,
    };
    Ok(Some((verified, rec.public_key)))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Canonical JSON matching Python `json.dumps(sort_keys=True, separators=(",", ":"),
/// ensure_ascii=False)`: recursively key-sorted, compact, non-ASCII emitted raw. Object keys
/// are ASCII throughout this schema, so byte-wise sort equals Python's codepoint sort. Scalars
/// reuse serde_json's escaping/number formatting, which matches Python for this integer/ASCII
/// data. Used to reproduce, in the compiler, the exact obligation/function digests the frozen
/// `anubis_program_verify` recomputes from the sealed bundle files.
fn canonical_json(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical_json(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_default());
                out.push(':');
                canonical_json(map.get(*key).unwrap(), out);
            }
            out.push('}');
        }
        scalar => out.push_str(&serde_json::to_string(scalar).unwrap_or_default()),
    }
}

fn canonical_sha256(value: &serde_json::Value) -> String {
    let mut buf = String::new();
    canonical_json(value, &mut buf);
    sha256_bytes(buf.as_bytes())
}

fn stage_authority(stage: &str) -> &'static str {
    match stage {
        "parse" => "anubis-frontend-parser",
        "typecheck" => "anubis-typecheck",
        "monomorphization" => "anubis-monomorphization-inventory",
        "policy-effects" => "anubis-typecheck-effect-inventory",
        "policy-capability" => "anubis-confinement-derivation",
        "policy-information-flow" => "anubis-taint-pass",
        "policy-declassification" => "anubis-source-walker",
        "symbolic" => "anubis-symbolic-engine",
        "solver" => "anubis-solver-native-rup",
        "source-binding" => "anubis-source-merkle",
        "artifact-binding" => "anubis-artifact-sha256",
        "evidence-closure" => "anubis-manifest-sha256",
        _ => "anubis",
    }
}

const PROGRAM_EVIDENCE_STAGES: [&str; 12] = [
    "parse",
    "typecheck",
    "monomorphization",
    "policy-effects",
    "policy-capability",
    "policy-information-flow",
    "policy-declassification",
    "symbolic",
    "solver",
    "source-binding",
    "artifact-binding",
    "evidence-closure",
];

/// Assemble `program-evidence.json` (`anubis.program-evidence.v3`) from the already-written
/// bundle files, matching the frozen `inventory-safe-v1` verifier contract. Emitted only for a
/// fully-discharged safe program — every obligation an rup_refutation PASS and a native artifact
/// present — so a partial build fail-closes at the verifier rather than presenting an incomplete
/// v3 document. Best-effort: any shape it cannot map returns Err and the file is simply skipped.
fn emit_program_evidence_v3(dir: &Path) -> Result<(), String> {
    let read_json = |rel: &str| -> Result<serde_json::Value, String> {
        let bytes = std::fs::read(dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{rel}: {e}"))
    };
    let artifact_row = |rel: &str| -> Result<serde_json::Value, String> {
        let path = dir.join(rel);
        let sha = sha256_file(&path).ok_or_else(|| format!("{rel}: sha"))?;
        let bytes = std::fs::metadata(&path)
            .map_err(|e| format!("{rel}: {e}"))?
            .len();
        Ok(serde_json::json!({"path": rel, "sha256": sha, "bytes": bytes}))
    };

    // Native artifact is mandatory for inventory-safe-v1.
    let artifact_path = dir.join("artifact");
    if !artifact_path.is_file() {
        return Err("no native artifact".into());
    }
    let artifact_sha = sha256_file(&artifact_path).ok_or("artifact sha")?;

    let source_bytes = std::fs::read(dir.join("source.anubis")).map_err(|e| e.to_string())?;
    let source_sha = sha256_bytes(&source_bytes);

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let compiler_sha = sha256_file(&exe).ok_or("compiler self-hash")?;
    let compiler_basename = exe
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("anubis")
        .to_string();

    // Function inventory: recompute ids exactly as the verifier does from hir.json.
    let hir = read_json("hir.json")?;
    let functions_val = hir
        .get("functions")
        .and_then(|v| v.as_array())
        .ok_or("hir.functions")?;
    let mut functions = Vec::new();
    let mut fids: Vec<serde_json::Value> = Vec::new();
    for f in functions_val {
        if f.get("mode").and_then(|m| m.as_str()) != Some("safe") {
            return Err("non-safe function".into());
        }
        let fid = canonical_sha256(f);
        fids.push(serde_json::Value::String(fid.clone()));
        functions.push(serde_json::json!({
            "id": fid,
            "name": f.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "module": f.get("module").cloned().unwrap_or(serde_json::Value::Null),
            "mode": f.get("mode").and_then(|v| v.as_str()).unwrap_or(""),
            "effects": f.get("effects").cloned().unwrap_or_else(|| serde_json::json!([])),
            "param_count": f.get("params").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
            "symbol_count": f.get("symbols").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        }));
    }
    let fids_val = serde_json::Value::Array(fids);

    // Solver inventory from analysis/proofs.json (order preserved to match solver.json).
    let proofs = read_json("analysis/proofs.json")?;
    let proof_rows = proofs
        .get("obligations")
        .and_then(|v| v.as_array())
        .ok_or("proofs.obligations")?;
    let mut obligations = Vec::new();
    for row in proof_rows {
        if row.get("proof").and_then(|v| v.as_str()) != Some("rup_refutation")
            || row.get("status").and_then(|v| v.as_str()) != Some("PASS")
        {
            return Err("obligation without published rup refutation".into());
        }
        let name = row
            .get("obligation")
            .and_then(|v| v.as_str())
            .ok_or("obligation name")?;
        let smt_p = row.get("smt").and_then(|v| v.as_str()).ok_or("smt path")?;
        let cnf_p = row
            .get("cnf_dimacs")
            .and_then(|v| v.as_str())
            .ok_or("cnf path")?;
        let drat_p = row
            .get("proof_drat")
            .and_then(|v| v.as_str())
            .ok_or("drat path")?;
        let smt_sha = sha256_file(&dir.join(smt_p)).ok_or("smt sha")?;
        let cnf_sha = sha256_file(&dir.join(cnf_p)).ok_or("cnf sha")?;
        let drat_sha = sha256_file(&dir.join(drat_p)).ok_or("drat sha")?;
        let stable = serde_json::json!({
            "name": name,
            "smt_sha256": smt_sha,
            "cnf_sha256": cnf_sha,
            "proof_sha256": drat_sha,
        });
        obligations.push(serde_json::json!({
            "id": canonical_sha256(&stable),
            "name": name,
            "status": "PASS",
            "proof_kind": "rup_refutation",
            "smt_path": smt_p,
            "smt_sha256": smt_sha,
            "cnf_path": cnf_p,
            "cnf_sha256": cnf_sha,
            "proof_path": drat_p,
            "proof_sha256": drat_sha,
            "num_vars": row.get("num_vars").cloned().unwrap_or_else(|| serde_json::json!(0)),
            "num_clauses": row.get("num_clauses").cloned().unwrap_or_else(|| serde_json::json!(0)),
            "steps": row.get("steps").cloned().unwrap_or_else(|| serde_json::json!(0)),
            "checker": row.get("checker").and_then(|v| v.as_str()).unwrap_or(""),
            "checker_version": row.get("checker_version").and_then(|v| v.as_str()).unwrap_or(""),
        }));
    }
    if obligations.is_empty() {
        return Err("zero obligations".into());
    }
    let verified = obligations.len();

    let declassifications = read_json("declassify_audit.json")
        .ok()
        .and_then(|v| {
            v.get("declassifications")
                .and_then(|d| d.as_array())
                .map(|a| a.len())
        })
        .unwrap_or(0);
    let capabilities = read_json("confinement_manifest.json")
        .ok()
        .and_then(|v| {
            v.get("capabilities_present")
                .and_then(|d| d.as_array())
                .map(|a| a.len())
        })
        .unwrap_or(0);
    let taint_count = read_json("taint-traces.json")?
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let mono_count = read_json("mono_specializations.json")?
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let mir_count = read_json("mir.json")?
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    let mut consumers = Vec::new();
    for cid in ["effects", "capability", "information-flow"] {
        consumers.push(serde_json::json!({
            "id": cid,
            "status": "PASS",
            "authority": "anubis-typecheck-producer-attested",
            "subjects": fids_val.clone(),
        }));
    }
    consumers.push(serde_json::json!({
        "id": "declassification",
        "status": "PASS",
        "authority": "anubis-source-walker-producer-attested",
        "subjects": {"count": declassifications},
    }));
    consumers.push(serde_json::json!({
        "id": "mode",
        "status": "PASS",
        "authority": "anubis-typecheck-producer-attested",
        "subjects": fids_val.clone(),
    }));
    consumers.push(serde_json::json!({
        "id": "contracts",
        "status": "PASS",
        "authority": "anubis-typecheck-producer-attested",
        "subjects": {"solver_obligation_count": verified},
    }));

    let stages: Vec<serde_json::Value> = PROGRAM_EVIDENCE_STAGES
        .iter()
        .map(|s| serde_json::json!({"id": s, "status": "PASS", "authority": stage_authority(s)}))
        .collect();

    let program = serde_json::json!({
        "schema": "anubis.program-evidence.v3",
        "version": 3,
        "mode": "safe",
        "source": {
            "path": "source.anubis",
            "sha256": source_sha,
            "merkle": source_sha,
            "bytes": source_bytes.len(),
        },
        "compiler": {
            "tool": tool_identity(),
            "path_basename": compiler_basename,
            "sha256": compiler_sha,
        },
        "artifacts": {
            "hir": artifact_row("hir.json")?,
            "mir": artifact_row("mir.json")?,
            "taint": artifact_row("taint-traces.json")?,
            "solver": artifact_row("solver.json")?,
            "monomorphization": artifact_row("mono_specializations.json")?,
            "native": {"path": "artifact", "sha256": artifact_sha},
        },
        "stages": stages,
        "solver_inventory": {"count": verified, "obligations": obligations},
        "policy_inventory": {
            "functions": functions,
            "consumers": consumers,
            "capabilities_present_count": capabilities,
            "taint_trace_count": taint_count,
            "monomorphization_count": mono_count,
            "mir_function_count": mir_count,
        },
        "residual_non_claims": [
            "no-source-to-vc-proof",
            "no-smt-to-cnf-proof",
            "no-source-native-refinement",
            "no-universal-language-soundness",
            "policy-semantics-producer-attested",
            "runtime-not-observed",
            "derived-confinement-is-not-os-enforcement",
        ],
    });

    let text = serde_json::to_string_pretty(&program).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("program-evidence.json"), text).map_err(|e| e.to_string())
}

fn check_config_value(
    policy: crate::middle::CheckPolicy,
    native: anubis_solver::NativeProofParameters,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "anubis-check-config/1",
        "wrap_safety": policy.wrap_safety,
        "native_authoritative": policy.native_authoritative,
        "require_native_proofs": policy.require_native_proofs,
        "native": {
            "gate_ceiling": native.gate_ceiling,
            "clause_ceiling": native.clause_ceiling,
            "conflicts": native.conflicts,
            "cert_work": native.cert_work,
            "time_budget_ms": native.time_budget_ms,
        }
    })
}

fn current_check_config() -> serde_json::Value {
    check_config_value(
        crate::middle::current_check_policy(),
        anubis_solver::current_native_proof_parameters(),
    )
}

fn default_check_config() -> serde_json::Value {
    check_config_value(
        crate::middle::CheckPolicy::product_default(),
        anubis_solver::NativeProofParameters::product_default(),
    )
}

/// The current check can be independently replayed under a deterministic policy. A command
/// using a different analysis/proof policy remains a command result, not a PCA-verified result.
pub fn check_environment_is_default() -> bool {
    current_check_config() == default_check_config()
}

fn with_default_check_analysis<T>(f: impl FnOnce() -> T) -> T {
    crate::middle::with_pca_default_check_policy(|| {
        anubis_solver::with_pca_default_native_limits(f)
    })
}

fn capture_environment() -> EnvironmentCapture {
    EnvironmentCapture {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        rustc: command_output("rustc", &["--version"]),
        cargo: command_output("cargo", &["--version"]),
        z3: command_output("z3", &["--version"]),
        anubis: env!("CARGO_PKG_VERSION").into(),
        machine_fields_status: "producer_reported_unverified".into(),
    }
}

fn command_output(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unavailable".into())
}

fn build_sarif(checks: &[Check]) -> serde_json::Value {
    let results = checks
        .iter()
        .filter(|check| check.status != "PASS")
        .map(|check| {
            let rule_id = if check.detail.contains("tainted flow")
                || check.detail.contains("tainted") && check.detail.contains("sink")
            {
                "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY".to_string()
            } else if check.detail.contains("declassify") && check.detail.contains("policy") {
                "ANUBIS_DECLASSIFY_MISSING_POLICY".to_string()
            } else if check.detail.contains("declassify") && check.detail.contains("reason") {
                "ANUBIS_DECLASSIFY_MISSING_REASON".to_string()
            } else if check.detail.contains("assert") && check.status == "FAIL" {
                "ANUBIS_ASSERTION_COUNTEREXAMPLE".to_string()
            } else if check.detail.contains("ANUBIS_REPLAY_MISMATCH")
                || check.detail.contains("replay")
                || check.detail.contains("REPLAY_FAILED")
            {
                // Prefer the Phase-4 B1 code when present; keep legacy alias for older bundles.
                if check.detail.contains("ANUBIS_REPLAY_MISMATCH") {
                    "ANUBIS_REPLAY_MISMATCH".to_string()
                } else {
                    "ANUBIS_SOLVER_MODEL_REPLAY_FAILED".to_string()
                }
            } else if check.detail.contains("unsupported") {
                "ANUBIS_SOLVER_UNSUPPORTED_EXPRESSION".to_string()
            } else if check.detail.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE")
                || check.detail.contains("forbidden in mode")
                || check.detail.contains("safe mode shell")
            {
                "ANUBIS_EFFECT_FORBIDDEN_IN_MODE".to_string()
            } else if check
                .detail
                .contains("ANUBIS_RESEARCH_MISSING_AUTHORIZATION")
                || check.detail.contains("requires authorization")
            {
                "ANUBIS_RESEARCH_MISSING_AUTHORIZATION".to_string()
            } else if check.detail.contains("ANUBIS_POC_MISSING_SCOPE")
                || check.detail.contains("missing scope")
            {
                "ANUBIS_POC_MISSING_SCOPE".to_string()
            } else if check.detail.contains("ANUBIS_FUZZ_SANDBOX_REQUIRED")
                || (check.detail.contains("fuzz") && check.detail.contains("sandbox"))
            {
                "ANUBIS_FUZZ_SANDBOX_REQUIRED".to_string()
            } else if check.detail.contains("ANUBIS_FUZZ_CRASH")
                || check.detail.contains("fuzz crash")
            {
                "ANUBIS_FUZZ_CRASH".to_string()
            } else if check.detail.contains("ANUBIS_EFFECT_NOT_DECLARED") {
                "ANUBIS_EFFECT_NOT_DECLARED".to_string()
            } else {
                check.name.clone()
            };
            serde_json::json!({
                "ruleId": rule_id,
                "level": "error",
                "message": { "text": check.detail },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": "source.anubis" }
                    }
                }]
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "anubis",
                    "rules": checks.iter().map(|check| serde_json::json!({
                        "id": check.name,
                        "shortDescription": { "text": check.name }
                    })).collect::<Vec<_>>()
                }
            },
            "results": results
        }]
    })
}

fn build_bounty_report(mode: &str, lane: Option<&str>, checks: &[Check]) -> String {
    let mut report = String::new();
    report.push_str("# Anubis Bounty Evidence Report\n\n");
    report.push_str(&format!("- mode: {}\n", mode));
    report.push_str(&format!("- lane: {}\n", lane.unwrap_or("unspecified")));
    report.push_str("\n## Checks\n\n");
    for check in checks {
        report.push_str(&format!(
            "- `{}`: {} - {}\n",
            check.name, check.status, check.detail
        ));
    }
    report
}

fn copy_hybrid_sidecars(
    artifact: Option<&str>,
    bundle_dir: &Path,
) -> Result<Vec<(String, String)>, String> {
    let Some(artifact) = artifact else {
        return Ok(vec![]);
    };
    let artifact_path = Path::new(artifact);
    let Some(parent) = artifact_path.parent() else {
        return Ok(vec![]);
    };
    let expected = ["guest.elf", "image_id.txt", "generated-methods.rs"];
    let existing = expected
        .iter()
        .filter(|name| parent.join(name).exists())
        .copied()
        .collect::<Vec<_>>();
    let mut copied: Vec<(String, String)> = vec![];
    if !existing.is_empty() {
        if existing.len() != expected.len() {
            return Err(format!(
                "incomplete hybrid proof sidecars beside artifact: found {:?}, expected {:?}",
                existing, expected
            ));
        }
        for name in expected {
            let data = std::fs::read(parent.join(name))
                .map_err(|e| format!("read hybrid sidecar {}: {}", name, e))?;
            std::fs::write(bundle_dir.join(name), &data)
                .map_err(|e| format!("write hybrid sidecar {}: {}", name, e))?;
            copied.push((name.to_string(), sha256_bytes(&data)));
        }
    }

    // RISC0 sidecars (for Gate 10 strict tamper + MANIFEST inclusion)
    // Copy from parent/backend/risc0 if present (and also flat risc0_* if forced)
    let risc0_dir = parent.join("backend").join("risc0");
    let risc0_patterns = [
        "guest.elf",
        "image_id.txt",
        "receipt.bin",
        "risc0_metadata.json",
        "receipt.verify.log",
        "prove.log",
        "guest/src/main.rs",
    ];
    if risc0_dir.exists() {
        let bundle_risc0 = bundle_dir.join("backend").join("risc0");
        let _ = std::fs::create_dir_all(&bundle_risc0);
        for pat in &risc0_patterns {
            // support nested guest/src too
            let src = risc0_dir.join(pat);
            if src.exists() {
                if let Ok(data) = std::fs::read(&src) {
                    let flat_name = if pat.contains('/') {
                        format!("risc0_{}", pat.replace('/', "_"))
                    } else {
                        format!("risc0_{}", pat)
                    };
                    // flat for MANIFEST walk
                    let _ = std::fs::write(bundle_dir.join(&flat_name), &data);
                    copied.push((flat_name, sha256_bytes(&data)));
                    // tree for script backend/risc0/ checks and A15
                    let dst = bundle_risc0.join(pat);
                    if let Some(p) = dst.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    let _ = std::fs::write(&dst, &data);
                }
            }
        }
    }
    // also pick up any risc0_* flat beside artifact
    for e in std::fs::read_dir(parent)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with("risc0_") && e.path().is_file() {
            if let Ok(data) = std::fs::read(e.path()) {
                let _ = std::fs::write(bundle_dir.join(&name), &data);
                if !copied.iter().any(|(n, _)| n == &name) {
                    copied.push((name, sha256_bytes(&data)));
                }
            }
        }
    }

    Ok(copied)
}

fn hybrid_hash_check_name(name: &str) -> String {
    format!("hybrid_{}_hash", name.replace(['.', '-'], "_"))
}

fn risc0_metadata_check(bundle_dir: &Path) -> Option<Check> {
    let metadata_path = bundle_dir
        .join("risc0_risc0_metadata.json")
        .exists()
        .then(|| bundle_dir.join("risc0_risc0_metadata.json"))
        .or_else(|| {
            bundle_dir
                .join("backend/risc0/risc0_metadata.json")
                .exists()
                .then(|| bundle_dir.join("backend/risc0/risc0_metadata.json"))
        })?;
    let text = std::fs::read_to_string(&metadata_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let verify_status = value
        .get("verify_status")
        .and_then(|v| v.as_str())
        .unwrap_or("missing");
    let fresh = value
        .get("fresh_receipt_generated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let dev_mode = value
        .get("dev_mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mock_prover = value
        .get("mock_prover")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let cache_used = value
        .get("cache_used")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let placeholder = value
        .get("placeholder_image_id")
        .or_else(|| value.get("image_id_is_placeholder"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let metal_hybrid = value
        .get("metal_hybrid")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let patch_active = metal_hybrid
        .get("patch_crates_io_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let methods_patch_active = metal_hybrid
        .get("methods_patch_crates_io_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(patch_active);
    let prover_patch_active = metal_hybrid
        .get("prover_patch_crates_io_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(patch_active);
    let patch_provenance_ok = patch_active || methods_patch_active || prover_patch_active;
    // Validate the metal-hybrid reference by existence + structure, not by matching a
    // specific ANUBIS_RISC0_METAL_REFERENCE value. prove() resolves an in-repo default
    // when the env var is unset, so the previous env-var string match (with a
    // "/tmp/test-metal-prover" fallback) spuriously FAILed otherwise-valid in-repo proofs.
    // The real "did this use the vendored patched circuit" guarantee is carried by the
    // patch_active flags above (from cargo metadata); these two are structural sanity checks.
    let reference_path = metal_hybrid.get("reference_path").and_then(|v| v.as_str());
    let vendored_patch_path = metal_hybrid
        .get("vendored_patch_path")
        .and_then(|v| v.as_str());
    let reference_ok = reference_path
        .map(|p| !p.is_empty() && std::path::Path::new(p).is_dir())
        .unwrap_or(false);
    let vendor_ok = match (reference_path, vendored_patch_path) {
        (Some(base), Some(vp)) => {
            vp == format!("{}/vendor/risc0-circuit-rv32im", base)
                && std::path::Path::new(vp).join("Cargo.toml").is_file()
        }
        _ => false,
    };
    let passed = verify_status == "passed"
        && fresh
        && !dev_mode
        && !mock_prover
        && !cache_used
        && !placeholder
        && patch_provenance_ok
        && reference_ok
        && vendor_ok;
    Some(Check {
        name: "risc0_receipt_verify".into(),
        status: if passed { "PASS" } else { "FAIL" }.into(),
        detail: format!(
            "verify_status={} fresh_receipt_generated={} dev_mode={} mock_prover={} cache_used={} placeholder_image_id={} patch_crates_io_active={} methods_patch_crates_io_active={} prover_patch_crates_io_active={} patch_provenance_ok={} reference_ok={} vendor_ok={}",
            verify_status,
            fresh,
            dev_mode,
            mock_prover,
            cache_used,
            placeholder,
            patch_active,
            methods_patch_active,
            prover_patch_active,
            patch_provenance_ok,
            reference_ok,
            vendor_ok
        ),
    })
}

fn tracked_bundle_files(
    has_artifact: bool,
    hybrid_sidecars: &[(String, String)],
    source_leaf_files: &[String],
) -> Vec<String> {
    let mut files = vec![
        "source.anubis",
        "build.log",
        "hir.json",
        "mir.json",
        "taint-traces.json",
        "solver.json",
        "mono_specializations.json",
        "environment.json",
        "checks.sarif",
        "bounty-report.md",
        "validate.sh",
        "source-tree.json",
        "evidence.json",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if has_artifact {
        files.push("artifact".into());
    }
    files.extend(hybrid_sidecars.iter().map(|(name, _)| name.clone()));
    if !source_leaf_files.is_empty() {
        files.push("source-merkle-leaves.json".into());
        files.extend(source_leaf_files.iter().cloned());
    }
    files
}

fn build_source_tree(dir: &Path, files: Vec<String>) -> Result<Vec<SourceTreeEntry>, String> {
    files
        .into_iter()
        .filter(|file| file != "source-tree.json" && file != "evidence.json")
        .filter_map(|file| {
            let path = dir.join(&file);
            path.exists().then_some((file, path))
        })
        .map(|(file, path)| {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            Ok(SourceTreeEntry {
                path: file,
                sha256: sha256_bytes(&data),
                bytes: data.len() as u64,
            })
        })
        .collect()
}

/// Recompute `MANIFEST.sha256` after adding files (e.g. package summaries before sign).
pub fn refresh_manifest_hashes(dir: &Path) -> Result<(), String> {
    write_manifest_hashes(dir)
}

fn write_manifest_hashes(dir: &Path) -> Result<(), String> {
    let mut entries = vec![];
    collect_manifest_hashes(dir, dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let text = entries
        .into_iter()
        .map(|(name, hash)| format!("{}  {}\n", hash, name))
        .collect::<String>();
    std::fs::write(dir.join("MANIFEST.sha256"), text).map_err(|e| e.to_string())
}

fn collect_manifest_hashes(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<(String, String)>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_manifest_hashes(root, &path, entries)?;
            continue;
        }
        if !path.is_file() || entry.file_name() == "MANIFEST.sha256" {
            continue;
        }
        let name = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let hash = sha256_file(&path).ok_or_else(|| format!("hash failed for {}", name))?;
        entries.push((name, hash));
    }
    Ok(())
}

// These are verifier work ceilings, not limits on the language or native artifact format.
// A bundle exceeding one needs an explicitly reviewed larger-budget verifier; it must not
// fall back to unbounded reads. There is deliberately no small per-artifact size limit.
const MAX_EVIDENCE_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EVIDENCE_TREE_ENTRIES: usize = 200_000;
const MAX_EVIDENCE_HASH_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_EVIDENCE_PATH_BYTES: usize = 4096;
const MAX_EVIDENCE_TREE_DEPTH: usize = 64;
const MAX_EVIDENCE_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_EVIDENCE_SOURCE_LEAF_BYTES: u64 = MAX_EVIDENCE_SOURCE_BYTES;
const MAX_EVIDENCE_JSON_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EVIDENCE_SIGNATURE_BYTES: u64 = 64 * 1024;
// Both the current writer and the archived PCA v2 fixture carry these semantic
// inputs. A creator cannot erase one and rehash a smaller MANIFEST into validity.
// PCA itself is checked separately by verify_pca, which also handles old bundles
// whose hash layer was intentionally checked without a PCA document.
const REQUIRED_EVIDENCE_LEAVES: &[&str] = &[
    "evidence.json",
    "source.anubis",
    "solver.json",
    "build.log",
    "environment.json",
    "source-tree.json",
    "checks.sarif",
    "bounty-report.md",
    "hir.json",
    "mir.json",
    "taint-traces.json",
    "validate.sh",
];

/// Open only an existing regular file. A nonblocking, no-follow open on Unix also
/// prevents a concurrently substituted FIFO or symlink from becoming a blocking or
/// out-of-bundle read. Metadata identity is checked after opening.
fn open_regular_evidence_file(path: &Path) -> Result<Option<(std::fs::File, u64)>, String> {
    let before = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.to_string()),
    };
    if !before.file_type().is_file() {
        return Ok(None);
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.file_type().is_file() {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Ok(None);
        }
    }
    Ok(Some((file, opened.len())))
}

fn read_regular_evidence_bytes(path: &Path, max_bytes: u64) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;

    let Some((file, declared_len)) = open_regular_evidence_file(path)? else {
        return Ok(None);
    };
    if declared_len > max_bytes {
        return Err("evidence input byte budget exceeded".into());
    }
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != declared_len || bytes.len() as u64 > max_bytes {
        return Ok(None);
    }
    Ok(Some(bytes))
}

fn read_regular_evidence_text(path: &Path, max_bytes: u64) -> Result<Option<String>, String> {
    read_regular_evidence_bytes(path, max_bytes)?
        .map(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
        .transpose()
}

/// Construct the analyzed source snapshot in the same canonical path order as
/// the Merkle root. The limit is checked before concatenation, so the producer
/// never emits a bundle that the verifier must refuse for snapshot size.
fn source_snapshot_from_leaves(
    files: &[(String, Vec<u8>)],
    max_bytes: u64,
) -> Result<String, String> {
    let mut canonical_files: Vec<_> = files.iter().collect();
    canonical_files.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, bytes)) = canonical_files
        .iter()
        .copied()
        .find(|(path, _)| path == "source.anubis")
        .or_else(|| {
            canonical_files
                .iter()
                .copied()
                .find(|(path, _)| path.ends_with("/source.anubis"))
        })
    {
        if bytes.len() as u64 > max_bytes {
            return Err("analyzed source snapshot exceeds verifier limit".into());
        }
        return String::from_utf8(bytes.clone())
            .map_err(|_| "analyzed source.anubis leaf is not UTF-8".to_string());
    }
    let mut snapshot = String::new();
    let mut first_text_leaf = true;
    for (_, bytes) in canonical_files {
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let next_len = snapshot
            .len()
            .checked_add(usize::from(!first_text_leaf))
            .and_then(|n| n.checked_add(text.len()))
            .ok_or("analyzed source snapshot size overflow")?;
        if next_len as u64 > max_bytes {
            return Err("analyzed source snapshot exceeds verifier limit".into());
        }
        if !first_text_leaf {
            snapshot.push('\n');
        }
        snapshot.push_str(text);
        first_text_leaf = false;
    }
    Ok(snapshot)
}

/// Reconstruct the actual Merkle root and the analyzed source snapshot from
/// manifest-covered leaf bytes. The old descriptor-only multi-file format
/// cannot establish either relationship and is intentionally limited to
/// historical hash-inventory inspection; PCA and accepted-artifact validation
/// require the source closure to be independently reconstructible.
fn source_closure_matches(
    dir: &Path,
    expected_root: &str,
    inspected_bytes: &mut u64,
) -> Result<bool, String> {
    let Some(snapshot) =
        read_regular_evidence_bytes(&dir.join("source.anubis"), MAX_EVIDENCE_SOURCE_BYTES)?
    else {
        return Ok(false);
    };
    *inspected_bytes = inspected_bytes
        .checked_add(snapshot.len() as u64)
        .ok_or("evidence source byte count overflow")?;
    if *inspected_bytes > MAX_EVIDENCE_HASH_BYTES {
        return Err("evidence source byte budget exceeded".into());
    }
    let snapshot_hash = sha256_bytes(&snapshot);
    let listing_path = dir.join("source-merkle-leaves.json");
    if !listing_path.exists() {
        // The historic one-leaf identity remains compatible with PCA v2.
        return Ok(snapshot_hash == expected_root);
    }
    let Some(listing_bytes) = read_regular_evidence_bytes(&listing_path, MAX_EVIDENCE_JSON_BYTES)?
    else {
        return Ok(false);
    };
    let Ok(listing) = serde_json::from_slice::<SealedSourceLeaves>(&listing_bytes) else {
        return Ok(false);
    };
    if listing.schema != "anubis-source-merkle-leaves-v2"
        || listing.leaves.len() < 2
        || listing.leaves.len() > MAX_EVIDENCE_TREE_ENTRIES
        || !evidence_digest_ok(&listing.source_merkle_root)
        || listing.source_merkle_root != expected_root
    {
        return Ok(false);
    }

    let mut previous_path: Option<&str> = None;
    let mut leaf_hashes = Vec::with_capacity(listing.leaves.len());
    let mut exact_snapshot_hash = None;
    let mut first_suffix_snapshot_hash = None;
    let mut concatenated = Sha256::new();
    let mut first_text_leaf = true;
    for (index, leaf) in listing.leaves.iter().enumerate() {
        let sealed_path = format!("source-leaves/{index:08}.bin");
        if !evidence_manifest_path_ok(&leaf.path)
            || previous_path.is_some_and(|path| leaf.path.as_str() <= path)
            || !evidence_digest_ok(&leaf.sha256)
            || leaf.sealed_path != sealed_path
            || leaf.bytes > MAX_EVIDENCE_SOURCE_LEAF_BYTES
            || inspected_bytes
                .checked_add(leaf.bytes)
                .is_none_or(|n| n > MAX_EVIDENCE_HASH_BYTES)
        {
            return Ok(false);
        }
        previous_path = Some(&leaf.path);
        let Some(content) =
            read_regular_evidence_bytes(&dir.join(&sealed_path), MAX_EVIDENCE_SOURCE_LEAF_BYTES)?
        else {
            return Ok(false);
        };
        if content.len() as u64 != leaf.bytes || sha256_bytes(&content) != leaf.sha256 {
            return Ok(false);
        }
        *inspected_bytes += leaf.bytes;

        if leaf.path == "source.anubis" || leaf.path.ends_with("/source.anubis") {
            let named = (
                sha256_bytes(&content),
                std::str::from_utf8(&content).is_ok(),
            );
            if leaf.path == "source.anubis" {
                exact_snapshot_hash = Some(named);
            } else if first_suffix_snapshot_hash.is_none() {
                first_suffix_snapshot_hash = Some(named);
            }
        }
        if std::str::from_utf8(&content).is_ok() && !content.contains(&0) {
            if !first_text_leaf {
                concatenated.update(b"\n");
            }
            concatenated.update(&content);
            first_text_leaf = false;
        }

        let mut leaf_hasher = Sha256::new();
        leaf_hasher.update(leaf.path.as_bytes());
        leaf_hasher.update([0]);
        leaf_hasher.update(&content);
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&leaf_hasher.finalize());
        leaf_hashes.push((leaf.path.as_str(), digest));
    }
    let named_snapshot = exact_snapshot_hash.or(first_suffix_snapshot_hash);
    if named_snapshot.as_ref().is_some_and(|(_, utf8)| !utf8) {
        return Ok(false);
    }
    let derived_snapshot_hash = named_snapshot
        .map(|(hash, _)| hash)
        .unwrap_or_else(|| hex::encode(concatenated.finalize()));
    if derived_snapshot_hash != snapshot_hash {
        return Ok(false);
    }
    let actual_root = crate::package::merkle::merkle_root_from_leaf_hashes(
        leaf_hashes.into_iter().map(|(_, digest)| digest).collect(),
    );
    Ok(actual_root == expected_root)
}

fn evidence_manifest_path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_EVIDENCE_PATH_BYTES
        && !path
            .bytes()
            .any(|b| b.is_ascii_control() || b == b'\\' || b == b':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !Path::new(path).is_absolute()
}

fn evidence_digest_ok(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An archived PCA v2 fixture wrote its journal after MANIFEST, but its
/// manifest-covered claim names the exact journal digest. This exception is
/// available only when the PCA bytes match their own listed manifest digest.
fn legacy_v2_journal_digest(
    dir: &Path,
    expected: &std::collections::BTreeMap<String, String>,
) -> Result<Option<String>, String> {
    let Some(pca_digest) = expected.get("pca.json") else {
        return Ok(None);
    };
    let Some(bytes) = read_regular_evidence_bytes(&dir.join("pca.json"), MAX_EVIDENCE_JSON_BYTES)?
    else {
        return Ok(None);
    };
    if sha256_bytes(&bytes) != *pca_digest {
        return Ok(None);
    }
    let Ok(claim) = serde_json::from_slice::<ClaimBlock>(&bytes) else {
        return Ok(None);
    };
    if claim.pca_version != 2 || !claim.zk_present {
        return Ok(None);
    }
    Ok(claim
        .zk_journal_sha256
        .filter(|hash| evidence_digest_ok(hash)))
}

fn hash_evidence_file(
    path: &Path,
    hashed_bytes: &mut u64,
    max_bytes: u64,
) -> Result<Option<String>, String> {
    use std::io::Read;

    let Some((mut file, declared_len)) = open_regular_evidence_file(path)? else {
        return Ok(None);
    };
    if hashed_bytes
        .checked_add(declared_len)
        .is_none_or(|n| n > max_bytes)
    {
        return Err("evidence hash byte budget exceeded".into());
    }
    let mut hasher = Sha256::new();
    let mut actual_len = 0_u64;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        actual_len = actual_len
            .checked_add(n as u64)
            .ok_or("evidence hash byte count overflow")?;
        if actual_len > declared_len
            || hashed_bytes
                .checked_add(actual_len)
                .is_none_or(|total| total > max_bytes)
        {
            return Err("evidence hash byte budget exceeded or file changed".into());
        }
        hasher.update(&chunk[..n]);
    }
    if actual_len != declared_len
        || file.metadata().map_err(|e| e.to_string())?.len() != declared_len
    {
        return Ok(None);
    }
    *hashed_bytes += actual_len;
    Ok(Some(hex::encode(hasher.finalize())))
}

fn validate_evidence_tree(
    current: &Path,
    prefix: &str,
    depth: usize,
    expected: &mut std::collections::BTreeMap<String, String>,
    legacy_journal_digest: Option<&str>,
    visited: &mut usize,
    hashed_bytes: &mut u64,
) -> Result<bool, String> {
    if depth > MAX_EVIDENCE_TREE_DEPTH {
        return Err("evidence tree depth budget exceeded".into());
    }
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        *visited = visited
            .checked_add(1)
            .ok_or("evidence tree entry count overflow")?;
        if *visited > MAX_EVIDENCE_TREE_ENTRIES {
            return Err("evidence tree entry budget exceeded".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            return Ok(false);
        };
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if !evidence_manifest_path_ok(&relative) {
            return Ok(false);
        }
        let path = entry.path();
        let kind = std::fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type();
        if kind.is_dir() {
            if !validate_evidence_tree(
                &path,
                &relative,
                depth + 1,
                expected,
                legacy_journal_digest,
                visited,
                hashed_bytes,
            )? {
                return Ok(false);
            }
        } else if kind.is_file() {
            // The signature is created only after hashing the sealed bundle. Only
            // the exact archived-v2 journal case below has another exception.
            if relative == "MANIFEST.sha256" || relative == "pca.sig" {
                continue;
            }
            let recorded = if let Some(hash) = expected.remove(&relative) {
                hash
            } else if relative == "backend/risc0/journal.bin" {
                let Some(hash) = legacy_journal_digest else {
                    return Ok(false);
                };
                hash.to_owned()
            } else {
                return Ok(false);
            };
            if hash_evidence_file(&path, hashed_bytes, MAX_EVIDENCE_HASH_BYTES)?.as_deref()
                != Some(recorded.as_str())
            {
                return Ok(false);
            }
        } else {
            // Symlinks, devices, sockets, and FIFOs cannot supply evidence bytes.
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_manifest_hashes(dir: &Path) -> Result<bool, String> {
    use std::io::Read;

    if !std::fs::symlink_metadata(dir)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_dir()
    {
        return Ok(false);
    }
    let manifest = dir.join("MANIFEST.sha256");
    let Some((file, manifest_len)) = open_regular_evidence_file(&manifest)? else {
        return Ok(false);
    };
    if manifest_len == 0 || manifest_len > MAX_EVIDENCE_MANIFEST_BYTES {
        return Err("evidence manifest byte budget exceeded or manifest empty".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_EVIDENCE_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != manifest_len || bytes.len() as u64 > MAX_EVIDENCE_MANIFEST_BYTES {
        return Ok(false);
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(false);
    };
    if !text.ends_with('\n') {
        return Ok(false);
    }
    let mut expected = std::collections::BTreeMap::new();
    for line in text.split_terminator('\n') {
        let Some((digest, path)) = line.split_once("  ") else {
            return Ok(false);
        };
        if !evidence_digest_ok(digest)
            || !evidence_manifest_path_ok(path)
            || path == "MANIFEST.sha256"
            || path == "pca.sig"
            || expected
                .insert(path.to_owned(), digest.to_owned())
                .is_some()
        {
            return Ok(false);
        }
        if expected.len() > MAX_EVIDENCE_TREE_ENTRIES {
            return Err("evidence manifest entry budget exceeded".into());
        }
    }
    if REQUIRED_EVIDENCE_LEAVES
        .iter()
        .any(|path| !expected.contains_key(*path))
    {
        return Ok(false);
    }
    let legacy_journal_digest = legacy_v2_journal_digest(dir, &expected)?;
    let mut visited = 0;
    let mut hashed_bytes = 0;
    Ok(validate_evidence_tree(
        dir,
        "",
        0,
        &mut expected,
        legacy_journal_digest.as_deref(),
        &mut visited,
        &mut hashed_bytes,
    )? && expected.is_empty())
}

#[cfg(test)]
mod source_closure_tests {
    use super::*;

    #[test]
    fn mismatched_resolved_snapshot_has_integrity_but_no_source_check_authority() {
        let root = tempfile::tempdir().unwrap();
        let snapshot = b"fn main() { let x = 1; assert(x == 1); }".to_vec();
        let entry = b"fn main() { let x = 1; assert(x == 2); }".to_vec();
        let bundle = build_evidence_bundle_tree(
            &[
                ("entry/main.anb".into(), entry),
                ("source.anubis".into(), snapshot.clone()),
            ],
            "safe",
            None,
            vec![],
            root.path(),
            Some("safe-check"),
            None,
            None,
        )
        .unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, true).unwrap());
        let claim_path = bundle.dir.join("pca.json");
        let mut claim: ClaimBlock =
            serde_json::from_slice(&std::fs::read(&claim_path).unwrap()).unwrap();
        assert_eq!(claim.verdict, "PASS");
        assert_eq!(
            claim.claim_kind.as_deref(),
            Some("source_snapshot_unverified_v1")
        );
        assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None);

        // Even a producer able to refresh all unsigned hashes cannot turn a detached snapshot
        // into the command's source-check evidence by relabeling the claim.
        claim.claim_kind = Some("source_check_v1".into());
        write_json(&claim_path, &claim).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None);

        // Exact leaf agreement alone still lacks a checked import/resolution correspondence.
        let matching = build_evidence_bundle_tree(
            &[
                ("entry/main.anb".into(), snapshot.clone()),
                ("source.anubis".into(), snapshot),
            ],
            "safe",
            None,
            vec![],
            &root.path().join("matching"),
            Some("safe-check"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(verify_pca_scope(&matching.dir).unwrap(), None);
    }

    fn multi_source_bundle() -> (tempfile::TempDir, PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let source = b"fn main() { let x = 1; }".to_vec();
        let files = vec![
            ("entry/main.anb".into(), source.clone()),
            ("source.anubis".into(), source),
        ];
        let bundle =
            build_evidence_bundle_tree(&files, "safe", None, vec![], base.path(), None, None, None)
                .unwrap();
        (base, bundle.dir)
    }

    #[test]
    fn source_closure_rederives_root_from_sealed_bytes() {
        let (_base, dir) = multi_source_bundle();
        assert!(validate_bundle_recorded_files(&dir, false).unwrap());
        assert!(verify_pca(&dir).unwrap());
        let listing: SealedSourceLeaves =
            serde_json::from_slice(&std::fs::read(dir.join("source-merkle-leaves.json")).unwrap())
                .unwrap();
        assert_eq!(listing.schema, "anubis-source-merkle-leaves-v2");
        assert!(dir.join(&listing.leaves[0].sealed_path).is_file());

        // Rehashing the mutable MANIFEST cannot conceal a changed source leaf.
        std::fs::write(dir.join(&listing.leaves[0].sealed_path), b"different entry").unwrap();
        write_manifest_hashes(&dir).unwrap();
        assert!(!validate_bundle_recorded_files(&dir, false).unwrap());
    }

    #[test]
    fn source_closure_refuses_a_self_declared_root_and_legacy_descriptors() {
        let (_base, dir) = multi_source_bundle();
        let listing_path = dir.join("source-merkle-leaves.json");
        let original = std::fs::read(&listing_path).unwrap();

        // Both advertised roots agree, and every altered file is rehashed, but
        // the root does not follow from the manifest-covered source leaf bytes.
        let mut listing: serde_json::Value = serde_json::from_slice(&original).unwrap();
        let forged_root = "0".repeat(64);
        listing["source_merkle_root"] = serde_json::json!(forged_root);
        write_json(&listing_path, &listing).unwrap();
        let evidence_path = dir.join("evidence.json");
        let original_evidence = std::fs::read(&evidence_path).unwrap();
        let mut evidence: EvidenceManifest = serde_json::from_slice(&original_evidence).unwrap();
        evidence.source_hash = forged_root;
        write_json(&evidence_path, &evidence).unwrap();
        write_manifest_hashes(&dir).unwrap();
        assert!(!validate_bundle_recorded_files(&dir, false).unwrap());

        // Old listings recorded only paths/digests/sizes. They remain inspectable
        // as historical artifacts, but cannot assert a checked source closure.
        std::fs::write(&evidence_path, original_evidence).unwrap();
        let mut legacy: serde_json::Value = serde_json::from_slice(&original).unwrap();
        legacy.as_object_mut().unwrap().remove("schema");
        for leaf in legacy["leaves"].as_array_mut().unwrap() {
            leaf.as_object_mut().unwrap().remove("sealed_path");
        }
        write_json(&listing_path, &legacy).unwrap();
        write_manifest_hashes(&dir).unwrap();
        assert!(!validate_bundle_recorded_files(&dir, false).unwrap());
    }

    #[test]
    fn source_closure_refuses_a_snapshot_unrelated_to_its_leaves() {
        let (_base, dir) = multi_source_bundle();
        std::fs::write(dir.join("source.anubis"), b"fn main() {} ").unwrap();
        write_manifest_hashes(&dir).unwrap();
        assert!(!validate_bundle_recorded_files(&dir, false).unwrap());
    }

    #[test]
    fn source_closure_refuses_a_non_utf8_analyzed_leaf() {
        let base = tempfile::tempdir().unwrap();
        let files = vec![
            ("entry/main.anb".into(), b"fn main() {}".to_vec()),
            ("source.anubis".into(), b"fn main() {\xff }".to_vec()),
        ];
        let result =
            build_evidence_bundle_tree(&files, "safe", None, vec![], base.path(), None, None, None);
        assert!(result.unwrap_err().contains("not UTF-8"));
    }

    #[test]
    fn source_closure_identity_and_snapshot_are_order_independent() {
        let files = vec![
            ("b.anb".into(), b"fn main() {}".to_vec()),
            ("a.anb".into(), Vec::new()),
        ];
        let mut reversed = files.clone();
        reversed.reverse();
        assert_eq!(
            crate::package::merkle::merkle_root(files.clone()),
            crate::package::merkle::merkle_root(reversed.clone())
        );
        assert_eq!(
            source_snapshot_from_leaves(&files, MAX_EVIDENCE_SOURCE_BYTES).unwrap(),
            source_snapshot_from_leaves(&reversed, MAX_EVIDENCE_SOURCE_BYTES).unwrap()
        );
        assert!(source_snapshot_from_leaves(&files, 5).is_err());

        let base = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle_tree(
            &reversed,
            "safe",
            None,
            vec![],
            base.path(),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        let listing_path = bundle.dir.join("source-merkle-leaves.json");
        let mut listing: SealedSourceLeaves =
            serde_json::from_slice(&std::fs::read(&listing_path).unwrap()).unwrap();
        listing.leaves.reverse();
        write_json(&listing_path, &listing).unwrap();
        let mut inspected_bytes = 0;
        assert!(!source_closure_matches(
            &bundle.dir,
            &bundle.manifest.source_hash,
            &mut inspected_bytes
        )
        .unwrap());
    }

    #[test]
    fn exact_snapshot_leaf_precedes_an_entry_named_source_anubis() {
        let files = vec![
            (
                "entry/source.anubis".into(),
                b"not the checked AST".to_vec(),
            ),
            ("source.anubis".into(), b"fn main() {}".to_vec()),
        ];
        assert_eq!(
            source_snapshot_from_leaves(&files, MAX_EVIDENCE_SOURCE_BYTES).unwrap(),
            "fn main() {}"
        );
        let base = tempfile::tempdir().unwrap();
        let bundle =
            build_evidence_bundle_tree(&files, "safe", None, vec![], base.path(), None, None, None)
                .unwrap();
        assert_eq!(
            std::fs::read_to_string(bundle.dir.join("source.anubis")).unwrap(),
            "fn main() {}"
        );
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
    }
}

#[cfg(test)]
mod manifest_validation_tests {
    use super::*;

    fn core_bundle() -> tempfile::TempDir {
        let bundle = tempfile::tempdir().unwrap();
        for path in REQUIRED_EVIDENCE_LEAVES {
            std::fs::write(bundle.path().join(path), path.as_bytes()).unwrap();
        }
        std::fs::write(bundle.path().join("pca.json"), b"claim").unwrap();
        write_manifest_hashes(bundle.path()).unwrap();
        bundle
    }

    fn refused(dir: &Path) {
        assert!(
            !matches!(validate_manifest_hashes(dir), Ok(true)),
            "an unsafe or incomplete manifest must not validate"
        );
    }

    fn omit_journal_row(dir: &Path) {
        let manifest = dir.join("MANIFEST.sha256");
        let text = std::fs::read_to_string(&manifest).unwrap();
        let without_journal: String = text
            .lines()
            .filter(|line| !line.ends_with("  backend/risc0/journal.bin"))
            .map(|line| format!("{line}\n"))
            .collect();
        std::fs::write(manifest, without_journal).unwrap();
    }

    #[test]
    fn archived_v2_unlisted_journal_is_bound_to_its_manifest_covered_claim() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/zk_prove_bundle");
        assert!(validate_manifest_hashes(&fixture).unwrap());

        let bundle = core_bundle();
        let journal = bundle.path().join("backend/risc0/journal.bin");
        std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
        std::fs::copy(fixture.join("backend/risc0/journal.bin"), &journal).unwrap();
        std::fs::copy(fixture.join("pca.json"), bundle.path().join("pca.json")).unwrap();
        write_manifest_hashes(bundle.path()).unwrap();
        omit_journal_row(bundle.path());
        assert!(validate_manifest_hashes(bundle.path()).unwrap());

        std::fs::write(&journal, b"changed").unwrap();
        refused(bundle.path());
        std::fs::copy(fixture.join("backend/risc0/journal.bin"), &journal).unwrap();

        let pca_path = bundle.path().join("pca.json");
        let mut claim: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&pca_path).unwrap()).unwrap();
        claim["pca_version"] = serde_json::json!(PCA_VERSION_CURRENT);
        write_json(&pca_path, &claim).unwrap();
        write_manifest_hashes(bundle.path()).unwrap();
        omit_journal_row(bundle.path());
        refused(bundle.path());
    }

    #[test]
    fn manifest_requires_all_present_leaves_and_the_archived_core() {
        let bundle = core_bundle();
        assert!(validate_manifest_hashes(bundle.path()).unwrap());

        std::fs::write(bundle.path().join("unlisted.json"), b"added after sealing").unwrap();
        refused(bundle.path());
        std::fs::remove_file(bundle.path().join("unlisted.json")).unwrap();

        std::fs::remove_file(bundle.path().join("solver.json")).unwrap();
        write_manifest_hashes(bundle.path()).unwrap();
        refused(bundle.path());
    }

    #[test]
    fn manifest_refuses_empty_duplicate_malformed_and_traversing_rows() {
        let bundle = core_bundle();
        let path = bundle.path().join("MANIFEST.sha256");
        let valid = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, "").unwrap();
        refused(bundle.path());

        let first = valid.lines().next().unwrap();
        std::fs::write(&path, format!("{valid}{first}\n")).unwrap();
        refused(bundle.path());

        let digest = sha256_bytes(b"outside");
        for bad in [
            "../outside",
            "analysis/../outside",
            "/dev/null",
            "C:/outside",
        ] {
            std::fs::write(&path, format!("{valid}{digest}  {bad}\n")).unwrap();
            refused(bundle.path());
        }
        std::fs::write(&path, format!("g{}", &valid[1..])).unwrap();
        refused(bundle.path());
        std::fs::write(&path, valid.trim_end_matches('\n')).unwrap();
        refused(bundle.path());
    }

    #[test]
    fn only_the_root_signature_sidecar_may_be_unlisted() {
        let bundle = core_bundle();
        std::fs::write(bundle.path().join("pca.sig"), b"later signature").unwrap();
        assert!(validate_manifest_hashes(bundle.path()).unwrap());

        std::fs::create_dir(bundle.path().join("analysis")).unwrap();
        std::fs::write(bundle.path().join("analysis/pca.sig"), b"unlisted").unwrap();
        refused(bundle.path());
    }

    #[cfg(unix)]
    #[test]
    fn manifest_refuses_symlink_and_special_file_without_opening_them() {
        use std::os::unix::{fs::symlink, net::UnixListener};

        let bundle = core_bundle();
        symlink("/dev/zero", bundle.path().join("device_link")).unwrap();
        refused(bundle.path());
        std::fs::remove_file(bundle.path().join("device_link")).unwrap();

        let _socket = UnixListener::bind(bundle.path().join("socket")).unwrap();
        refused(bundle.path());
    }

    #[test]
    fn manifest_read_and_file_hashing_have_work_ceilings() {
        let bundle = core_bundle();
        let manifest = bundle.path().join("MANIFEST.sha256");
        std::fs::File::create(&manifest)
            .unwrap()
            .set_len(MAX_EVIDENCE_MANIFEST_BYTES + 1)
            .unwrap();
        refused(bundle.path());

        let tiny = bundle.path().join("tiny");
        std::fs::write(&tiny, b"bounded read").unwrap();
        let mut consumed = 0;
        assert!(hash_evidence_file(&tiny, &mut consumed, 1).is_err());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn sparse_source_and_artifact_over_budget_refuse_before_parsing() {
        let source = core_bundle();
        std::fs::OpenOptions::new()
            .write(true)
            .open(source.path().join("source.anubis"))
            .unwrap()
            .set_len(MAX_EVIDENCE_HASH_BYTES + 1)
            .unwrap();
        assert!(matches!(
            validate_bundle_recorded_files(source.path(), false),
            Err(ref reason) if reason.contains("budget")
        ));

        let artifact = core_bundle();
        std::fs::File::create(artifact.path().join("artifact"))
            .unwrap()
            .set_len(MAX_EVIDENCE_HASH_BYTES + 1)
            .unwrap();
        let manifest = artifact.path().join("MANIFEST.sha256");
        let mut text = std::fs::read_to_string(&manifest).unwrap();
        text.push_str(&format!("{}  artifact\n", sha256_bytes(b"placeholder")));
        std::fs::write(manifest, text).unwrap();
        assert!(matches!(
            validate_bundle_recorded_files(artifact.path(), false),
            Err(ref reason) if reason.contains("budget")
        ));
    }

    #[test]
    fn signature_sidecar_is_bounded_and_regular() {
        let bundle = core_bundle();
        std::fs::File::create(bundle.path().join("pca.sig"))
            .unwrap()
            .set_len(MAX_EVIDENCE_SIGNATURE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            pca_signature_status(bundle.path()),
            Err(ref reason) if reason.contains("budget")
        ));
    }

    #[test]
    fn source_and_pca_json_reads_refuse_sparse_oversized_files() {
        let bundle = core_bundle();
        let source = bundle.path().join("source.anubis");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .set_len(MAX_EVIDENCE_SOURCE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            read_regular_evidence_text(&source, MAX_EVIDENCE_SOURCE_BYTES),
            Err(ref reason) if reason.contains("budget")
        ));

        let pca = bundle.path().join("pca.json");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&pca)
            .unwrap()
            .set_len(MAX_EVIDENCE_JSON_BYTES + 1)
            .unwrap();
        assert!(matches!(
            pca_signed_message(bundle.path()),
            Err(ref reason) if reason.contains("budget")
        ));
    }

    #[test]
    fn zk_binding_streams_receipt_and_bounds_its_text_sidecars() {
        let bundle = tempfile::tempdir().unwrap();
        let r = bundle.path().join("backend/risc0");
        std::fs::create_dir_all(&r).unwrap();
        let receipt = r.join("receipt.bin");
        let image_id = r.join("image_id.txt");
        let metadata = r.join("risc0_metadata.json");
        std::fs::write(&receipt, b"structural receipt only").unwrap();
        std::fs::write(&image_id, b"1 2 3 4 5 6 7 8").unwrap();
        write_json(
            &metadata,
            &serde_json::json!({
                "verify_status": "passed",
                "fresh_receipt_generated": true,
                "dev_mode": false,
                "mock_prover": false,
                "image_id_is_placeholder": false,
                "image_id": "1 2 3 4 5 6 7 8",
                "committed_journal_sha256": sha256_bytes(b"journal"),
            }),
        )
        .unwrap();
        assert!(derive_zk_binding(bundle.path()).is_some());

        std::fs::OpenOptions::new()
            .write(true)
            .open(&receipt)
            .unwrap()
            .set_len(MAX_EVIDENCE_HASH_BYTES + 1)
            .unwrap();
        assert!(derive_zk_binding(bundle.path()).is_none());
        std::fs::write(&receipt, b"structural receipt only").unwrap();

        std::fs::OpenOptions::new()
            .write(true)
            .open(&image_id)
            .unwrap()
            .set_len(1025)
            .unwrap();
        assert!(derive_zk_binding(bundle.path()).is_none());
        std::fs::write(&image_id, b"1 2 3 4 5 6 7 8").unwrap();

        std::fs::OpenOptions::new()
            .write(true)
            .open(&metadata)
            .unwrap()
            .set_len(MAX_EVIDENCE_JSON_BYTES + 1)
            .unwrap();
        assert!(derive_zk_binding(bundle.path()).is_none());
    }
}

#[cfg(test)]
mod pca_tests {
    use super::*;

    #[test]
    fn matching_fail_fields_from_an_unfinished_analysis_are_undecided() {
        let mut derived =
            derive_claim_for_version("fn main() {}", "safe", false, PCA_VERSION_CURRENT);
        let recorded = derived.claim.clone();
        assert_eq!(recorded.verdict, "FAIL");
        derived.limit = Some(crate::middle::AnalysisLimit::Memory);
        assert!(matches!(
            finish_pca_rederivation(true, &derived, &recorded),
            Err(ref reason) if reason.starts_with("ANUBIS_ANALYSIS_LIMIT")
        ));
        derived.limit = Some(crate::middle::AnalysisLimit::Depth);
        assert!(matches!(
            finish_pca_rederivation(true, &derived, &recorded),
            Err(ref reason) if reason.starts_with("ANUBIS_ANALYSIS_LIMIT")
        ));
    }

    fn replay_check(
        name: &str,
        status: &str,
        detail: &str,
        model: Option<&str>,
    ) -> crate::middle::SolverCheck {
        crate::middle::SolverCheck {
            name: name.into(),
            status: status.into(),
            detail: detail.into(),
            model: model.map(str::to_owned),
            smt: format!("; {name}\n(check-sat)\n"),
        }
    }

    #[test]
    fn replay_record_never_promotes_a_pass_or_untrusted_failure_to_replay() {
        let checks = vec![
            replay_check("same", "PASS", crate::middle::PROVED_DETAIL_CERTIFIED, None),
            replay_check(
                "same",
                "FAIL",
                crate::middle::DISPROVED_DETAIL_Z3,
                Some("good"),
            ),
            replay_check("unknown", "UNKNOWN", "solver unknown", None),
            replay_check("no-model", "FAIL", "solver declined", None),
            replay_check(
                "untrusted",
                "FAIL",
                "ANUBIS_REPLAY_MISMATCH",
                Some("forged"),
            ),
            replay_check(
                "unencoded",
                "FAIL",
                crate::middle::UNRESOLVED_PRECONDITION_DETAIL,
                Some("forged"),
            ),
            replay_check(
                "value",
                "FAIL",
                crate::middle::OVERAPPROX_UNDECIDED_DETAIL,
                Some("forged"),
            ),
            replay_check(
                "branch",
                "FAIL",
                crate::middle::BRANCH_REACHABILITY_UNDECIDED_DETAIL,
                Some("forged"),
            ),
            replay_check(
                "combined",
                "FAIL",
                crate::middle::OVERAPPROX_COMBINED_UNDECIDED_DETAIL,
                Some("forged"),
            ),
            replay_check("empty", "PASS", crate::middle::NO_OBLIGATIONS_DETAIL, None),
            replay_check("future", "DEFERRED", "unrecognized status", None),
        ];
        let mut attempted = Vec::new();
        let record = solver_replay_record(&checks, |smt, model| {
            attempted.push((smt.to_owned(), model.to_owned()));
            model == "good"
        });
        assert_eq!(record["schema"], "anubis-solver-replay-v1");
        assert_eq!(record["scope"], "all_solver_checks");
        assert_eq!(record["status"], "incomplete");
        assert_eq!(record["replay_valid"], false);
        assert_eq!(attempted, vec![(checks[1].smt.clone(), "good".into())]);
        let rows = record["obligations"].as_array().unwrap();
        let statuses: Vec<&str> = rows
            .iter()
            .map(|row| row["status"].as_str().unwrap())
            .collect();
        assert_eq!(
            statuses,
            [
                "not_applicable",
                "counterexample_replayed",
                "not_replayed_undecided",
                "not_replayed_no_model",
                "not_replayed_untrusted_model",
                "not_encoded",
                "overapproximated_not_replayed",
                "branch_reachability_not_replayed",
                "value_and_branch_reachability_not_replayed",
                "not_replayed_invalid_sentinel",
                "not_replayed_unknown_status",
            ]
        );
        assert_eq!(rows[0]["obligation"], "same");
        assert_eq!(rows[1]["obligation"], "same");
        assert_eq!(rows[1]["index"], 1);
        assert_eq!(rows[1]["smt"], "analysis/proofs/obligation_0001.smt2");
        assert_eq!(rows[0]["replay_attempted"], false);
        assert_eq!(rows[1]["replay_attempted"], true);
    }

    #[test]
    fn replay_record_failed_replay_dominates_later_success() {
        let checks = vec![
            replay_check(
                "bad",
                "FAIL",
                crate::middle::DISPROVED_DETAIL_Z3,
                Some("bad"),
            ),
            replay_check(
                "good",
                "FAIL",
                crate::middle::DISPROVED_DETAIL_NATIVE,
                Some("good"),
            ),
        ];
        let record = solver_replay_record(&checks, |_, model| model == "good");
        assert_eq!(record["status"], "replay_failed");
        assert_eq!(record["replay_valid"], false);
        assert_eq!(record["obligations"][0]["replay_attempted"], true);
        assert_eq!(record["obligations"][0]["replay_valid"], false);
        assert_eq!(record["obligations"][1]["replay_valid"], true);
    }

    #[test]
    fn replay_record_success_needs_an_actual_trusted_model_replay() {
        let checks = vec![
            replay_check(
                "proved",
                "PASS",
                crate::middle::PROVED_DETAIL_CERTIFIED,
                None,
            ),
            replay_check(
                "disproved",
                "FAIL",
                crate::middle::DISPROVED_DETAIL_Z3,
                Some("rechecked model"),
            ),
        ];
        let record = solver_replay_record(&checks, |_, model| model == "rechecked model");
        assert_eq!(record["status"], "counterexample_replayed");
        assert_eq!(record["replay_valid"], true);
        assert_eq!(record["obligations"][0]["replay_attempted"], false);
        assert_eq!(record["obligations"][1]["replay_attempted"], true);
    }

    #[test]
    fn replay_record_empty_stream_is_incomplete() {
        let record = solver_replay_record(&[], |_, _| panic!("no model exists to replay"));
        assert_eq!(record["status"], "incomplete");
        assert_eq!(record["replay_valid"], false);
        assert_eq!(record["obligations"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn safe_bundle_replay_rows_match_every_solver_check_and_are_manifest_bound() {
        let root = tempfile::tempdir().unwrap();
        let source = "fn main() { let x = 1; assert(x == 1); assert(x == 2); }";
        let bundle = build_evidence_bundle(source, "safe", None, vec![], root.path(), None, None)
            .expect("safe evidence bundle");
        let checks: Vec<crate::middle::SolverCheck> =
            serde_json::from_slice(&std::fs::read(bundle.dir.join("solver.json")).unwrap())
                .unwrap();
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(bundle.dir.join("analysis/solver_replay.json")).unwrap(),
        )
        .unwrap();
        let rows = record["obligations"].as_array().unwrap();
        assert_eq!(rows.len(), checks.len());
        assert!(checks.iter().any(|check| check.status == "PASS"));
        assert!(checks.iter().any(|check| check.status == "FAIL"));
        assert_eq!(record["status"], "counterexample_replayed");
        assert_eq!(record["replay_valid"], true);
        for (index, (check, row)) in checks.iter().zip(rows).enumerate() {
            assert_eq!(row["index"], index);
            assert_eq!(row["obligation"], check.name);
            assert_eq!(row["solver_status"], check.status);
            assert_eq!(row["detail"], check.detail);
            let smt = row["smt"].as_str().unwrap();
            assert_eq!(
                std::fs::read_to_string(bundle.dir.join(smt)).unwrap(),
                check.smt
            );
            if check.status == "PASS" {
                assert_eq!(row["status"], "not_applicable");
                assert_eq!(row["replay_attempted"], false);
                assert_eq!(row["replay_valid"], false);
            } else if check.status == "FAIL" {
                assert_eq!(row["status"], "counterexample_replayed");
                assert_eq!(row["replay_attempted"], true);
                assert_eq!(row["replay_valid"], true);
            }
        }
        assert!(validate_manifest_hashes(&bundle.dir).unwrap());
        std::fs::write(bundle.dir.join("analysis/solver_replay.json"), b"{}").unwrap();
        assert!(!validate_manifest_hashes(&bundle.dir).unwrap());
    }

    #[test]
    fn safe_bundle_without_obligations_never_claims_a_replay() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; }",
            "safe",
            None,
            vec![],
            root.path(),
            None,
            None,
        )
        .expect("safe evidence bundle");
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(bundle.dir.join("analysis/solver_replay.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(record["status"], "not_applicable");
        assert_eq!(record["replay_valid"], false);
        let rows = record["obligations"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["status"], "not_applicable_no_obligations");
        assert_eq!(rows[0]["replay_attempted"], false);
        let claim: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.dir.join("pca.json")).unwrap()).unwrap();
        assert_eq!(claim["pca_version"], PCA_VERSION_CURRENT);
        assert_eq!(claim["solver_obligations"], 0);
        let proofs: serde_json::Value = serde_json::from_slice(
            &std::fs::read(bundle.dir.join("analysis/proofs.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            proofs["obligations"][0]["proof"],
            "not_applicable_no_obligations"
        );
        assert!(validate_manifest_hashes(&bundle.dir).unwrap());
    }

    #[test]
    fn undecided_evidence_preserves_value_and_branch_provenance() {
        assert_eq!(
            undecided_provenance(crate::middle::OVERAPPROX_UNDECIDED_DETAIL),
            Some((
                "undecided_overapproximated",
                "overapproximated_not_replayed"
            ))
        );
        assert_eq!(
            undecided_provenance(crate::middle::BRANCH_REACHABILITY_UNDECIDED_DETAIL),
            Some((
                "undecided_branch_reachability",
                "branch_reachability_not_replayed"
            ))
        );
        assert_eq!(
            undecided_provenance(crate::middle::OVERAPPROX_COMBINED_UNDECIDED_DETAIL),
            Some((
                "undecided_value_and_branch_reachability",
                "value_and_branch_reachability_not_replayed"
            ))
        );
        assert_eq!(
            undecided_provenance(crate::middle::DISPROVED_DETAIL_NATIVE),
            None
        );
    }

    /// An ACCEPTED program that handles untrusted input without letting it reach a sink. The three
    /// tests below use it to build a legitimate bundle.
    ///
    /// These tests used to require a LIVE known false accept (a program `check` accepted while it
    /// leaked at runtime), repointed to the next open specimen each time one was fixed: first
    /// `b.f = key; let g = b.f; print(g());` (closed in Completion Phase 4), then
    /// `b.f = key; print(b.f());` (closed by the item-21 Family-2 slice), then the taint
    /// sink-argument carrier `b.f = |x| shell(x); b.f(input())` (closed by the item-21
    /// sink-argument slice). A test suite that needs a vulnerability to stay open works against
    /// fixing it.
    ///
    /// The properties do not depend on a leak. The PCA pipeline never derives a `taint_clean`
    /// guarantee (typecheck success is not a noninterference proof), so a v2 claim must not contain
    /// one, and `verify_pca` must reject one that was injected and re-hashed — whether or not the
    /// injected claim happens to be true of the program. That is a stronger statement than "the
    /// verifier rejects one particular false claim". The fixture reads `input()` so that a taint
    /// claim is meaningful for it; `pca_fixture_is_accepted_and_nearby_leak_is_not` pins that it is
    /// accepted and that the same program printing the input is rejected, so it is not vacuously
    /// clean.
    fn accepted_untrusted_input_program() -> &'static str {
        r#"
fn main() uses(io.read) {
    let raw = input();
    let copy = raw;
    let n = 2 + 3;
    print(n);
}
"#
    }

    #[test]
    fn pca_fixture_is_accepted_and_nearby_leak_is_not() {
        assert!(derive_claim_block(accepted_untrusted_input_program(), "safe").typecheck_ok);
        let leaky = "fn main() uses(io.read) {\n    let raw = input();\n    print(raw);\n}\n";
        assert!(
            !derive_claim_block(leaky, "safe").typecheck_ok,
            "the control must be rejected, or the fixture proves nothing about taint"
        );
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("anubis-pca-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn derive_claim_block_is_deterministic() {
        let src = "fn main() { let x = 2 + 3; print(x); }";
        assert_eq!(
            derive_claim_block(src, "safe"),
            derive_claim_block(src, "safe")
        );
        assert_eq!(derive_claim_block(src, "safe").verdict, "PASS");
    }

    #[test]
    fn injected_solver_checks_cannot_launder_unknown_or_invalid_status_into_a_pca_pass() {
        assert_eq!(solver_claim_summary(&[]), (0, false));
        let absent = solver_replay_record(&[], |_, _| panic!("empty stream must not replay"));
        assert_eq!(absent["status"], "incomplete");
        assert_eq!(absent["replay_valid"], false);
        let check = |status: &str, detail: &str| crate::middle::SolverCheck {
            name: "wrap-safety:add".into(),
            status: status.into(),
            detail: detail.into(),
            model: None,
            smt: "(check-sat)".into(),
        };
        let sentinel = crate::middle::SolverCheck {
            name: "solver:no-obligations".into(),
            status: "PASS".into(),
            detail: crate::middle::NO_OBLIGATIONS_DETAIL.into(),
            model: None,
            smt: "(check-sat)".into(),
        };
        assert_eq!(
            solver_claim_summary(std::slice::from_ref(&sentinel)),
            (0, true)
        );
        assert_eq!(
            solver_claim_summary(&[check("PASS", crate::middle::NO_OBLIGATIONS_DETAIL)]),
            (1, false),
            "reserved prose on a real obligation must not erase it"
        );
        assert_eq!(
            solver_claim_summary(&[
                sentinel,
                check("PASS", crate::middle::PROVED_DETAIL_SOLVER_ONLY)
            ]),
            (2, false),
            "a synthetic sentinel mixed with a real check is invalid"
        );
        assert_eq!(
            solver_claim_summary(&[check("PASS", crate::middle::PROVED_DETAIL_SOLVER_ONLY)]),
            (1, true)
        );
        for status in ["FAIL", "UNKNOWN", "PASS ", ""] {
            assert_eq!(
                solver_claim_summary(&[check(status, "no discharge")]),
                (1, false),
                "a {status:?} check must block the PCA claim"
            );
        }
    }

    #[test]
    fn source_with_no_solver_obligations_keeps_a_checked_not_proved_claim() {
        let claim = derive_claim_block("fn main() { let x = 1; }", "safe");
        assert_eq!(claim.pca_version, PCA_VERSION_CURRENT);
        assert_eq!(claim.verdict, "PASS");
        assert_eq!(claim.tier, "checked");
        assert_eq!(claim.solver_obligations, 0);
        assert!(claim.solver_all_discharged);
        assert!(!claim.zk_present);
    }

    #[test]
    fn current_pca_does_not_assert_unearned_taint_clean_for_an_accepted_program() {
        // Until a separate taint theorem is derived, the PCA must report the bounded typecheck
        // result without translating `typecheck returned Ok` into the stronger `taint_clean: true`
        // guarantee — for every accepted program, not only a leaking one.
        let claim = derive_claim_block(accepted_untrusted_input_program(), "safe");
        assert!(
            claim.typecheck_ok,
            "the fixture must be an accepted program"
        );
        let json = serde_json::to_value(&claim).unwrap();
        assert_eq!(json["pca_version"], PCA_VERSION_CURRENT);
        assert!(
            json.get("taint_clean").is_none(),
            "PCA must not serialize the unearned taint-clean guarantee: {json}"
        );
    }

    #[test]
    fn verify_pca_rejects_legacy_v1_unearned_taint_clean_claim() {
        let base = unique_dir("legacy-taint-claim");
        let bundle = build_evidence_bundle(
            accepted_untrusted_input_program(),
            "safe",
            None,
            vec![],
            &base,
            None,
            None,
        )
        .unwrap();
        let pca_path = bundle.dir.join("pca.json");
        let mut legacy: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&pca_path).unwrap()).unwrap();
        legacy["pca_version"] = serde_json::json!(1);
        legacy["taint_clean"] = serde_json::json!(true);
        write_json(&pca_path, &legacy).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();

        assert!(
            validate_bundle(&bundle.dir).unwrap(),
            "the forged bundle must pass the hash-only layer before semantic verification"
        );
        assert!(
            !matches!(verify_pca(&bundle.dir), Ok(true)),
            "verify must fail closed on a schema-v1 bundle that asserts unearned taint cleanliness"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_pca_rejects_rehashed_current_claim_with_a_retired_taint_claim() {
        let base = unique_dir("v2-retired-taint-claim");
        let bundle = build_evidence_bundle(
            accepted_untrusted_input_program(),
            "safe",
            None,
            vec![],
            &base,
            None,
            None,
        )
        .unwrap();
        let pca_path = bundle.dir.join("pca.json");
        let mut poisoned: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&pca_path).unwrap()).unwrap();
        assert_eq!(poisoned["pca_version"], PCA_VERSION_CURRENT);
        poisoned
            .as_object_mut()
            .unwrap()
            .insert("taint_clean".into(), serde_json::Value::Bool(true));
        write_json(&pca_path, &poisoned).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();

        assert!(
            validate_bundle(&bundle.dir).unwrap(),
            "the forged bundle must pass the hash layer before semantic verification"
        );
        assert!(
            !matches!(verify_pca(&bundle.dir), Ok(true)),
            "PCA v2 must reject, not silently discard, the retired taint-clean claim"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_pca_rejects_rehashed_bundle_with_no_semantic_claim() {
        let base = unique_dir("missing-pca");
        let bundle = build_evidence_bundle(
            "fn main() { assert(true); }",
            "safe",
            None,
            vec![],
            &base,
            None,
            None,
        )
        .unwrap();
        std::fs::remove_file(bundle.dir.join("pca.json")).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();

        assert!(
            validate_bundle(&bundle.dir).unwrap(),
            "the no-PCA poison must pass the hash layer"
        );
        assert!(
            !verify_pca(&bundle.dir).unwrap(),
            "missing semantic evidence must not downgrade verify to integrity-only success"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_refutes_a_pass_claim_no_derivation_produces() {
        // A PASS verdict beside `typecheck_ok: false` (eighth review of the checker limits, E3):
        // refuted as it stands, not left to a re-derivation that may stop at a limit.
        let base = unique_dir("inconsistent");
        let good = "fn main() { let x = 1; print(x); }";
        let bundle = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        let mut lie = derive_claim_block(good, "safe");
        assert_eq!(lie.verdict, "PASS");
        lie.typecheck_ok = false;
        write_json(&bundle.dir.join("pca.json"), &lie).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle(&bundle.dir).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_bundle_is_complete_under_its_name_and_never_overwritten() {
        // Built under a `.partial` name and renamed when complete (B8-4); a second bundle of the
        // same second takes a name of its own.
        let base = unique_dir("staged");
        let good = "fn main() { let x = 1; print(x); }";
        let a = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        let b = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        assert_ne!(a.dir, b.dir);
        for bundle in [&a, &b] {
            let name = bundle
                .dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert!(name.starts_with("evidence-"), "{name}");
            assert!(bundle.dir.join("MANIFEST.sha256").exists());
            assert!(bundle.dir.join("pca.json").exists());
            assert!(bundle.limit.is_none());
            assert!(verify_pca(&bundle.dir).unwrap());
        }
        let partial = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().ends_with(".partial"));
        assert!(!partial);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn bundles_built_side_by_side_in_one_directory_are_all_complete() {
        // Checks started together in one directory stage apart and take distinct names.
        let base = unique_dir("side-by-side");
        let good = "fn main() { let x = 1; print(x); }";
        let dirs: Vec<std::path::PathBuf> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    s.spawn(|| {
                        build_evidence_bundle(good, "safe", None, vec![], &base, None, None)
                            .unwrap()
                            .dir
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let distinct: std::collections::BTreeSet<&std::path::PathBuf> = dirs.iter().collect();
        assert_eq!(distinct.len(), 4);
        for dir in &dirs {
            assert!(verify_pca(dir).unwrap(), "{}", dir.display());
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_pca_rederives_claim_and_catches_a_consistent_lie() {
        let base = unique_dir("rederive");
        let good = "fn main() { let x = 1; print(x); }";
        let bundle = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        // A freshly built PCA verifies.
        assert!(verify_pca(&bundle.dir).unwrap());

        // Forge the claim block so it disagrees with the source, then regenerate the manifest so
        // every hash is internally consistent — a hash-only check would now be satisfied.
        let mut lie = derive_claim_block(good, "safe");
        lie.typecheck_ok = !lie.typecheck_ok;
        lie.solver_obligations += 1;
        write_json(&bundle.dir.join("pca.json"), &lie).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();

        // The hash / tamper layer alone is satisfied (recorded checks PASS, hashes consistent)...
        assert!(validate_bundle(&bundle.dir).unwrap());
        // ...but re-deriving the claim from the source catches the lie and fails closed.
        assert!(!verify_pca(&bundle.dir).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn checked_counterexample_is_valid_evidence_but_not_an_accepted_artifact() {
        let base = unique_dir("honest-disproof");
        let bundle = build_evidence_bundle(
            "fn f(x: i64) { assert(x == 0); }",
            "safe",
            None,
            vec![],
            &base,
            None,
            None,
        )
        .unwrap();
        let claim: ClaimBlock =
            serde_json::from_slice(&std::fs::read(bundle.dir.join("pca.json")).unwrap()).unwrap();
        assert_eq!(claim.verdict, "FAIL");
        assert!(!validate_bundle(&bundle.dir).unwrap());
        assert!(verify_pca(&bundle.dir).unwrap());

        // An internally contradictory manifest must not become valid by
        // regenerating the unsigned hash list.
        let manifest_path = bundle.dir.join("evidence.json");
        let mut manifest: EvidenceManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest.verdict = "PASS".into();
        write_json(&manifest_path, &manifest).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        assert!(!verify_pca(&bundle.dir).unwrap());

        for check in &mut manifest.checks {
            check.status = "PASS".into();
        }
        write_json(&manifest_path, &manifest).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle(&bundle.dir).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejected_check_is_valid_only_with_a_source_derived_refusal() {
        let base = tempfile::tempdir().unwrap();
        let source = "fn main() { let x = 1; assert(x == 1); assert(x == 2); }";
        // Reproduce the CLI check lane independently of the evidence producer. A proved
        // obligation precedes the disproof, so replay/check ordering also matters.
        let ast = crate::frontend::parse_source(source).unwrap();
        let typed = crate::middle::typecheck(ast, crate::frontend::Mode::Safe).unwrap();
        let tainted = crate::middle::TaintPass::apply(typed);
        let solver_checks = crate::middle::SymbolicEngine::check_obligations(&tainted);
        let refusals = crate::middle::solver_stream_refusals(&solver_checks);
        assert_eq!(refusals.len(), 1);
        let reason = crate::middle::format_check_failures(&refusals);
        let bundle = build_rejected_evidence_bundle(
            source,
            "safe",
            vec!["check rejected".into()],
            base.path(),
            Some("safe-check"),
            &reason,
        )
        .unwrap();
        assert!(!validate_bundle(&bundle.dir).unwrap());
        assert!(verify_pca(&bundle.dir).unwrap());

        // Rehashing both recorded copies of a fabricated refusal is insufficient. The verifier
        // must obtain the same reason from the sealed source, not from the producer's text.
        let pca_path = bundle.dir.join("pca.json");
        let evidence_path = bundle.dir.join("evidence.json");
        let manifest_path = bundle.dir.join("manifest.json");
        let mut claim: ClaimBlock =
            serde_json::from_slice(&std::fs::read(&pca_path).unwrap()).unwrap();
        let mut manifest: EvidenceManifest =
            serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
        claim.rejection = Some("ANUBIS_ASSERTION_DISPROVED: forged reason".into());
        manifest
            .checks
            .iter_mut()
            .find(|check| check.name == "command_rejection")
            .unwrap()
            .detail = claim.rejection.clone().unwrap();
        write_json(&pca_path, &claim).unwrap();
        write_json(&evidence_path, &manifest).unwrap();
        write_json(&manifest_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap());

        claim.rejection = Some(reason.clone());
        write_json(&pca_path, &claim).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(
            !verify_pca(&bundle.dir).unwrap(),
            "manifest refusal differs"
        );

        manifest
            .checks
            .iter_mut()
            .find(|check| check.name == "command_rejection")
            .unwrap()
            .detail = reason.clone();
        manifest.lane = Some("safe-build-rejected".into());
        write_json(&evidence_path, &manifest).unwrap();
        write_json(&manifest_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(
            !verify_pca(&bundle.dir).unwrap(),
            "build lane is not a check"
        );

        manifest.lane = Some("safe-check".into());
        write_json(&evidence_path, &manifest).unwrap();
        write_json(&manifest_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        // The v1-facing mirror is a claim, not just another hash-bound file. Rehashing a
        // PASS-shaped mirror while evidence.json remains the honest FAIL must not verify.
        let original_mirror = std::fs::read(&manifest_path).unwrap();
        let mut forged_mirror = manifest.clone();
        forged_mirror.verdict = "PASS".into();
        for row in &mut forged_mirror.checks {
            row.status = "PASS".into();
        }
        write_json(&manifest_path, &forged_mirror).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap(), "rehashed PASS mirror");
        std::fs::write(&manifest_path, original_mirror).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        // Likewise a rehashed codesign plist must not gain an entitlement absent from the
        // source-derived JSON profile. The verifier compares canonical plist bytes.
        let plist_path = bundle
            .dir
            .join(crate::package::entitlements::ENTITLEMENT_PLIST_FILENAME);
        let original_plist = std::fs::read(&plist_path).unwrap();
        let forged_plist = String::from_utf8(original_plist.clone()).unwrap().replacen(
            "</dict>",
            "\t<key>com.apple.security.network.client</key>\n\t<true/>\n</dict>",
            1,
        );
        assert_ne!(forged_plist.as_bytes(), original_plist.as_slice());
        std::fs::write(&plist_path, forged_plist).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap(), "rehashed plist grant");
        std::fs::write(&plist_path, original_plist).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        // Each changed analysis/presentation file receives a fresh complete hash manifest.
        // Source re-derivation, not the hash layer, must reject the contradictory content.
        for relative in ["solver.json", "analysis/proofs.json", "validate.sh"] {
            let path = bundle.dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            match relative {
                "solver.json" => {
                    let mut solver: serde_json::Value = serde_json::from_slice(&original).unwrap();
                    let failed = solver
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|row| row["status"] == "FAIL")
                        .unwrap();
                    failed["status"] = serde_json::json!("PASS");
                    write_json(&path, &solver).unwrap();
                }
                "analysis/proofs.json" => {
                    let mut proofs: serde_json::Value = serde_json::from_slice(&original).unwrap();
                    let failed = proofs["obligations"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|row| row["status"] == "FAIL")
                        .unwrap();
                    failed["status"] = serde_json::json!("PASS");
                    failed["proof"] = serde_json::json!("rup_refutation");
                    write_json(&path, &proofs).unwrap();
                }
                "validate.sh" => {
                    let forged = String::from_utf8(original.clone()).unwrap().replacen(
                        "#!/usr/bin/env sh",
                        "#!/bin/true",
                        1,
                    );
                    assert_ne!(forged.as_bytes(), original.as_slice());
                    std::fs::write(&path, forged).unwrap();
                }
                _ => unreachable!(),
            }
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(!verify_pca(&bundle.dir).unwrap(), "rehashed {relative}");
            std::fs::write(&path, original).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());
        }

        // SARIF and the human report have hashes inside both manifest copies. Update those too:
        // matching all recorded digests still cannot make their forged claims canonical.
        for (relative, hash_field) in [("checks.sarif", "sarif"), ("bounty-report.md", "report")] {
            let path = bundle.dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            if relative == "checks.sarif" {
                let mut sarif: serde_json::Value = serde_json::from_slice(&original).unwrap();
                sarif["runs"][0]["results"].as_array_mut().unwrap().clear();
                write_json(&path, &sarif).unwrap();
            } else {
                std::fs::write(&path, b"# All checks PASS\n").unwrap();
            }
            let mut forged_manifest = manifest.clone();
            let changed_hash = sha256_bytes(&std::fs::read(&path).unwrap());
            if hash_field == "sarif" {
                forged_manifest.sarif_hash = changed_hash;
            } else {
                forged_manifest.bounty_report_hash = changed_hash;
            }
            write_json(&evidence_path, &forged_manifest).unwrap();
            write_json(&manifest_path, &forged_manifest).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(!verify_pca(&bundle.dir).unwrap(), "rehashed {relative}");
            std::fs::write(&path, original).unwrap();
            write_json(&evidence_path, &manifest).unwrap();
            write_json(&manifest_path, &manifest).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());
        }

        let mut forged_security = manifest.clone();
        forged_security.security = Some(serde_json::json!({
            "mode": "safe", "authorization": "approved", "scope": "all targets"
        }));
        write_json(&evidence_path, &forged_security).unwrap();
        write_json(&manifest_path, &forged_security).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        assert!(
            !verify_pca(&bundle.dir).unwrap(),
            "forged authorization and scope"
        );
        write_json(&evidence_path, &manifest).unwrap();
        write_json(&manifest_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        for relative in [
            crate::package::confinement::CONFINEMENT_FILENAME,
            crate::package::entitlements::ENTITLEMENT_PROFILE_FILENAME,
        ] {
            let path = bundle.dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            let mut forged: serde_json::Value = serde_json::from_slice(&original).unwrap();
            if relative == crate::package::confinement::CONFINEMENT_FILENAME {
                forged["grants"][0]["authorization"] = serde_json::json!("approved");
            } else {
                forged["sandbox"]["authorization"] = serde_json::json!("approved");
            }
            write_json(&path, &forged).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(
                !verify_pca(&bundle.dir).unwrap(),
                "unknown field in {relative}"
            );
            std::fs::write(&path, original).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());
        }

        // Rehashing a different proof policy cannot turn a nondefault (or wall-clock)
        // check into a PCA-verifiable product-default check.
        let config_path = bundle.dir.join("analysis/check-config.json");
        let original_config = std::fs::read(&config_path).unwrap();
        let mut forged_config: serde_json::Value =
            serde_json::from_slice(&original_config).unwrap();
        forged_config["native"]["conflicts"] = serde_json::json!(0);
        write_json(&config_path, &forged_config).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap(), "rehashed proof policy");
        std::fs::write(&config_path, &original_config).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        std::fs::remove_file(&config_path).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap(), "missing proof policy");
        std::fs::write(&config_path, original_config).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        // These source-derived records affect package contracts and declassification
        // authority. Rehashing a forged row or omitting the record must not verify.
        for relative in [
            crate::package::summary::SUMMARIES_FILENAME,
            crate::package::summary::DECLASSIFY_AUDIT_FILENAME,
        ] {
            let path = bundle.dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            let mut forged: serde_json::Value = serde_json::from_slice(&original).unwrap();
            if relative == crate::package::summary::SUMMARIES_FILENAME {
                forged["functions"] = serde_json::json!([{"name": "forged"}]);
            } else {
                forged["declassifications"] = serde_json::json!([{
                    "function": "main",
                    "policy": "forged",
                    "reason": "forged",
                    "well_formed": true
                }]);
            }
            write_json(&path, &forged).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(!verify_pca(&bundle.dir).unwrap(), "rehashed {relative}");
            std::fs::write(&path, &original).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());

            std::fs::remove_file(&path).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(!verify_pca(&bundle.dir).unwrap(), "omitted {relative}");
            std::fs::write(&path, original).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());
        }

        // A JSON Value parse discards a duplicate key and keeps the last value. An extra
        // first authority field must not disappear during verification of a new check.
        for relative in [
            crate::package::confinement::CONFINEMENT_FILENAME,
            crate::package::entitlements::ENTITLEMENT_PROFILE_FILENAME,
        ] {
            let path = bundle.dir.join(relative);
            let original = std::fs::read(&path).unwrap();
            let text = String::from_utf8(original.clone()).unwrap();
            let forged = text.replacen(r#""schema":"#, "\"schema\": \"forged\",\n  \"schema\":", 1);
            assert_ne!(forged, text);
            std::fs::write(&path, forged).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(
                !verify_pca(&bundle.dir).unwrap(),
                "duplicate JSON key in {relative}"
            );
            std::fs::write(&path, original).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());
        }

        // Hashes alone do not prevent someone from attaching an executable or execution/proof
        // sidecars to a rejected check. Every attachment is included in a freshly hashed tree.
        for attachment in [
            "artifact",
            "guest.elf",
            "risc0_receipt.bin",
            "backend/risc0/receipt.bin",
            "program-evidence.json",
            "run-evidence.json",
        ] {
            let path = bundle.dir.join(attachment);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, b"attached after rejection").unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(!verify_pca(&bundle.dir).unwrap(), "attached {attachment}");
            std::fs::remove_file(&path).unwrap();
            if attachment.starts_with("backend/") {
                std::fs::remove_dir_all(bundle.dir.join("backend")).unwrap();
            }
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(verify_pca(&bundle.dir).unwrap());
        }

        for row_name in ["parse", "typecheck", "solver"] {
            let mut forged_rows = manifest.clone();
            let row = forged_rows
                .checks
                .iter_mut()
                .find(|check| check.name == row_name)
                .unwrap();
            row.status = if row.status == "PASS" { "FAIL" } else { "PASS" }.into();
            write_json(&evidence_path, &forged_rows).unwrap();
            write_json(&manifest_path, &forged_rows).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            assert!(!verify_pca(&bundle.dir).unwrap(), "forged {row_name} row");
        }
        write_json(&evidence_path, &manifest).unwrap();
        write_json(&manifest_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_pca(&bundle.dir).unwrap());

        let verified_lane = build_rejected_evidence_bundle(
            source,
            "safe",
            vec!["check --verified rejected".into()],
            base.path(),
            Some("safe-verified-check"),
            &reason,
        )
        .unwrap();
        assert!(!verify_pca(&verified_lane.dir).unwrap());

        // A PASS program cannot be turned into an apparently valid rejected claim by a producer
        // that chooses a FAIL reason and recomputes all unsigned hashes.
        let forged = build_rejected_evidence_bundle(
            "fn main() { let x = 1; }",
            "safe",
            vec!["check rejected".into()],
            base.path(),
            Some("safe-check"),
            &reason,
        )
        .unwrap();
        assert!(validate_bundle_recorded_files(&forged.dir, false).unwrap());
        assert!(!verify_pca(&forged.dir).unwrap());
    }

    #[test]
    #[ignore = "Research source checks require the repository's disposable Tart/VZ guest lane"]
    fn research_refusal_cannot_be_relabelled_as_a_safe_check() {
        let base = tempfile::tempdir().unwrap();
        let research_source = "@research(authorization: \"test\") fn hidden() {}\n\
            fn main() { let x = 1; assert(x == 2); }";
        let relabeled = derive_claim(research_source, "safe", true);
        assert_eq!(relabeled.source_mode, Some(crate::frontend::Mode::Research));
        assert_eq!(relabeled.claim.verdict, "FAIL");
        let research_refusal = relabeled.check_refusal.unwrap();
        let wrong_mode = build_rejected_evidence_bundle(
            research_source,
            "safe",
            vec!["mode relabeled".into()],
            base.path(),
            Some("safe-check"),
            &research_refusal,
        )
        .unwrap();
        assert!(validate_bundle_recorded_files(&wrong_mode.dir, false).unwrap());
        assert!(!verify_pca(&wrong_mode.dir).unwrap());
    }

    #[test]
    fn current_check_scope_downgrades_to_source_only_after_rehashed_lane_change() {
        let base = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; assert(x == 1); }",
            "safe",
            None,
            vec![],
            base.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceCheckV1)
        );
        let claim_path = bundle.dir.join("pca.json");
        let mut claim: ClaimBlock =
            serde_json::from_slice(&std::fs::read(&claim_path).unwrap()).unwrap();
        let mut manifest: EvidenceManifest =
            serde_json::from_slice(&std::fs::read(bundle.dir.join("evidence.json")).unwrap())
                .unwrap();
        manifest.lane = Some("safe-build".into());
        write_json(&bundle.dir.join("evidence.json"), &manifest).unwrap();
        write_json(&bundle.dir.join("manifest.json"), &manifest).unwrap();
        claim.evidence_lane = manifest.lane.clone();
        claim.claim_kind = Some("source_only_v1".into());
        write_json(&claim_path, &claim).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceOnly),
            "a rehashed lane downgrade must lose current check authority"
        );
    }

    #[test]
    fn empty_obligation_inventory_records_no_solver_query_and_refuses_relabeling() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; }",
            "safe",
            None,
            vec![],
            root.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        let claim_path = bundle.dir.join("pca.json");
        let mut claim: ClaimBlock =
            serde_json::from_slice(&std::fs::read(&claim_path).unwrap()).unwrap();
        assert_eq!(claim.verdict, "PASS");
        assert_eq!(claim.solver_obligations, 0);
        assert_eq!(claim.solver_execution, Some(SolverExecution::NotRun));
        assert!(claim.solver_all_discharged);
        assert!(solver_execution_matches_obligations(&claim));
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceCheckV1)
        );

        claim.solver_execution = Some(SolverExecution::Ran);
        write_json(&claim_path, &claim).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(!solver_execution_matches_obligations(&claim));
        assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None);
    }

    #[test]
    fn unencoded_obligation_rows_do_not_claim_a_solver_run() {
        let unencoded = crate::middle::SolverCheck {
            name: "requires-unresolved@fixture".into(),
            status: "FAIL".into(),
            detail: crate::middle::UNRESOLVED_PRECONDITION_DETAIL.into(),
            model: None,
            smt: "; not encoded\n".into(),
        };
        assert!(!solver_path_was_invoked(std::slice::from_ref(&unencoded)));
        let submitted = crate::middle::SolverCheck {
            name: "assert:fixture".into(),
            status: "PASS".into(),
            detail: crate::middle::PROVED_DETAIL_CERTIFIED.into(),
            model: None,
            smt: "(check-sat)\n".into(),
        };
        assert!(solver_path_was_invoked(&[unencoded, submitted]));
    }

    #[test]
    fn parsed_safe_source_without_functions_keeps_current_check_scope() {
        let source = "struct Marker { value: int }";
        let ast = crate::frontend::parse_source(source).unwrap();
        assert_eq!(crate::frontend::program_mode(&ast.items), None);
        let derived = derive_claim(source, "safe", true);
        assert!(derived.claim.parse_ok);
        assert_eq!(derived.source_mode, None);
        assert_eq!(derived.claim.verdict, "PASS");

        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            source,
            "safe",
            None,
            vec![],
            root.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceCheckV1)
        );
    }

    #[test]
    fn canonical_unrejected_failures_never_gain_current_check_scope() {
        for (name, source) in [
            ("malformed", "fn main( {"),
            ("invalid-typecheck", "fn main() { missing(); }"),
            ("solver-fail", "fn main() { assert(1 == 2); }"),
        ] {
            let root = tempfile::tempdir().unwrap();
            // Produce the precise attacker-desired shape, including canonical SARIF, report,
            // source policy, and analysis sidecars. Removing a command_rejection from an
            // existing bundle without regenerating those files would test presentation drift
            // rather than the no-rejection PASS requirement.
            let bundle = with_default_check_analysis(|| {
                build_evidence_bundle(
                    source,
                    "safe",
                    None,
                    vec![],
                    root.path(),
                    Some("safe-check"),
                    None,
                )
            })
            .unwrap();
            let claim_path = bundle.dir.join("pca.json");
            let evidence_path = bundle.dir.join("evidence.json");
            let claim: ClaimBlock =
                serde_json::from_slice(&std::fs::read(&claim_path).unwrap()).unwrap();
            let manifest: EvidenceManifest =
                serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
            assert_eq!(claim.verdict, "FAIL", "{name}");
            assert_eq!(manifest.verdict, "FAIL", "{name}");
            assert_eq!(claim.tier, "checked", "{name}");
            assert!(claim.rejection.is_none(), "{name}");
            assert!(!manifest
                .checks
                .iter()
                .any(|row| row.name == "command_rejection"));
            if name == "malformed" {
                assert!(!claim.parse_ok);
            }
            // Rehash the complete producer-shaped tree as an unsigned adversary could. Its
            // bytes and every independently rederived sidecar must still agree; only the
            // missing command refusal on a source-derived FAIL may deny check authority.
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert!(validate_bundle_recorded_files(&bundle.dir, false).unwrap());
            let derived = with_default_check_analysis(|| {
                derive_claim_bound_for_version(&bundle.dir, source, "safe", PCA_VERSION_CURRENT)
            });
            let mut expected_rows = derived.check_rows.clone();
            expected_rows.push(Check {
                name: "source_hash".into(),
                status: "PASS".into(),
                detail: manifest.source_hash.clone(),
            });
            expected_rows.push(Check {
                name: "build_log_hash".into(),
                status: "PASS".into(),
                detail: manifest.build_log_hash.clone(),
            });
            assert_eq!(manifest.checks, expected_rows, "{name}");
            assert!(presentation_sidecars_match(&bundle.dir, &manifest).unwrap());
            assert!(source_check_tree_matches(&bundle.dir, &derived).unwrap());
            assert!(evidence_json_matches(
                &bundle.dir,
                "analysis/check-config.json",
                &default_check_config()
            )
            .unwrap());
            assert!(with_default_check_analysis(|| source_policy_sidecars_match(
                &bundle.dir,
                source
            ))
            .unwrap());
            assert!(
                with_default_check_analysis(|| analysis_sidecars_match(&bundle.dir, &derived))
                    .unwrap()
            );
            assert!(source_claim_sidecars_match(&bundle.dir, source, false).unwrap());
            let mut expected_claim = derived.claim.clone();
            expected_claim.evidence_lane = Some("safe-check".into());
            expected_claim.claim_kind = Some("source_check_v1".into());
            assert!(
                claim_semantically_matches(&expected_claim, &claim),
                "{name}"
            );
            assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None, "{name}");
        }
    }

    #[test]
    fn version_three_claim_on_current_layout_is_only_a_migration_probe() {
        let root = tempfile::tempdir().unwrap();
        let source = "fn main() { let x = 1; assert(x == 1); }";
        let bundle = build_evidence_bundle(
            source,
            "safe",
            None,
            vec![],
            root.path(),
            Some("safe-build"),
            None,
        )
        .unwrap();
        // Construct a v3 semantic claim with version-specific derivation on a current layout.
        // This exercises the compatibility branch but is not an archived v3 producer bundle:
        // the repository currently retains an archived v2 fixture, not a source-bound v3 one.
        // Merely changing a v4 claim's version field leaves v4-only fields and is invalid.
        let legacy = derive_claim_bound_for_version(&bundle.dir, source, "safe", 3).claim;
        assert_eq!(legacy.pca_version, 3);
        assert!(legacy.evidence_lane.is_none());
        assert!(legacy.claim_kind.is_none());
        assert!(legacy.solver_execution.is_none());
        assert_eq!(legacy.solver_backend, "z3");
        write_json(&bundle.dir.join("pca.json"), &legacy).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::LegacySourceOnly),
            "a version-specific v3 source claim remains inspectable without current check authority"
        );
    }

    #[test]
    fn security_refusal_classification_excludes_invalid_and_undecided_findings() {
        let finding = |code: &str| crate::middle::SemanticDiagnostic {
            code: Some(code.into()),
            message: code.into(),
            span: None,
        };
        let mut refusal = crate::middle::TypecheckFailure {
            message: "source flow refused".into(),
            diagnostics: vec![finding("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY")],
            limit: None,
        };
        assert!(replayable_security_refusal(&refusal));
        refusal.diagnostics.push(finding("ANUBIS_UNKNOWN_FUNCTION"));
        assert!(!replayable_security_refusal(&refusal));
        refusal.diagnostics = vec![finding("ANUBIS_CONTRACT_UNPROVABLE")];
        assert!(!replayable_security_refusal(&refusal));
        refusal.diagnostics = vec![finding("ANUBIS_IFC2_LIMIT")];
        assert!(!replayable_security_refusal(&refusal));
        assert!(source_analysis_limit_refusal(&refusal));
        refusal.diagnostics = vec![
            finding("ANUBIS_SECRET_EXFILTRATION"),
            finding("ANUBIS_IFC2_LIMIT"),
        ];
        assert!(!replayable_security_refusal(&refusal));
        assert!(source_analysis_limit_refusal(&refusal));
        refusal.diagnostics = vec![finding("ANUBIS_SECRET_EXFILTRATION")];
        refusal.limit = Some("ANUBIS_ANALYSIS_LIMIT".into());
        assert!(!replayable_security_refusal(&refusal));
        assert!(source_analysis_limit_refusal(&refusal));
    }

    #[test]
    fn intrinsic_mode_elevators_never_inherit_a_safe_source_claim() {
        let ordinary = crate::frontend::parse_source("fn main() { let x = 1; }").unwrap();
        assert!(!items_have_unresolved_mode_elevator(&ordinary.items));

        let nested = crate::frontend::parse_source(
            "fn main() { let x = if true { @research { let y = 1; } 1 } else { 0 }; }",
        )
        .unwrap();
        assert_eq!(
            crate::frontend::program_mode(&nested.items),
            Some(crate::frontend::Mode::Safe)
        );
        assert!(items_have_unresolved_mode_elevator(&nested.items));

        for clause in ["requires", "ensures"] {
            let source = format!(
                "fn main() {clause}(if true {{ @research {{ let y = 1; }} true }} else {{ true }}) {{}}"
            );
            let contract = crate::frontend::parse_source(&source).unwrap();
            assert_eq!(
                crate::frontend::program_mode(&contract.items),
                Some(crate::frontend::Mode::Safe),
                "{clause}"
            );
            assert!(
                items_have_unresolved_mode_elevator(&contract.items),
                "{clause}"
            );
        }

        // `@exploit(...) @safe` currently fails to parse, so its syntax error
        // cannot serve as an intrinsic-mode refusal control here.
        for attribute in ["research", "poc", "fuzz", "proof", "defensive", "audit"] {
            let source = format!(
                "@{attribute}(authorization: \"unit-test\") @safe fn main() {{ let x = 1; }}"
            );
            let overwritten = crate::frontend::parse_source(&source).unwrap();
            assert_eq!(
                crate::frontend::program_mode(&overwritten.items),
                Some(crate::frontend::Mode::Safe),
                "{attribute}"
            );
            assert!(
                items_have_unresolved_mode_elevator(&overwritten.items),
                "{attribute}"
            );
        }

        let exploit_attribute = crate::frontend::parse_source("@exploit fn main() {}").unwrap();
        assert_eq!(
            crate::frontend::program_mode(&exploit_attribute.items),
            Some(crate::frontend::Mode::Safe)
        );
        assert!(items_have_unresolved_mode_elevator(
            &exploit_attribute.items
        ));

        let retained = crate::frontend::parse_source(
            "@safe @research(authorization: \"unit-test\") fn main() { let x = 1; }",
        )
        .unwrap();
        assert_eq!(
            crate::frontend::program_mode(&retained.items),
            Some(crate::frontend::Mode::Research)
        );
        assert!(!items_have_unresolved_mode_elevator(&retained.items));
    }

    #[test]
    #[ignore = "Research source checks require the repository's disposable Tart/VZ guest lane"]
    fn intrinsic_mode_elevator_bundle_cannot_be_relabelled_as_safe() {
        let root = tempfile::tempdir().unwrap();
        let source = "@research(authorization: \"unit-test\") @safe fn main() { let x = 1; }";
        let bundle = build_evidence_bundle(
            source,
            "safe",
            None,
            vec![],
            root.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        let claim_path = bundle.dir.join("pca.json");
        let mut claim: ClaimBlock =
            serde_json::from_slice(&std::fs::read(&claim_path).unwrap()).unwrap();
        assert_eq!(
            claim.claim_kind.as_deref(),
            Some("source_mode_unverified_v1")
        );
        assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None);
        claim.claim_kind = Some("source_check_v1".into());
        write_json(&claim_path, &claim).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            None,
            "a rehashed check lane cannot hide the intrinsic source-mode ambiguity"
        );
    }

    #[test]
    fn machine_local_environment_fields_remain_typed_producer_provenance() {
        let base = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; }",
            "safe",
            None,
            vec![],
            base.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        let environment_path = bundle.dir.join("environment.json");
        let manifest_path = bundle.dir.join("evidence.json");
        let mirror_path = bundle.dir.join("manifest.json");
        let mut environment: EnvironmentCapture =
            serde_json::from_slice(&std::fs::read(&environment_path).unwrap()).unwrap();
        assert_eq!(
            environment.machine_fields_status,
            "producer_reported_unverified"
        );
        environment.os = "claimed-other-host".into();
        environment.arch = "claimed-other-architecture".into();
        write_json(&environment_path, &environment).unwrap();
        let mut manifest: EvidenceManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest.environment_hash = sha256_bytes(&std::fs::read(&environment_path).unwrap());
        write_json(&manifest_path, &manifest).unwrap();
        write_json(&mirror_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceCheckV1),
            "the machine fields are explicitly producer-reported, not platform attestations"
        );

        environment.machine_fields_status = "independently_verified".into();
        write_json(&environment_path, &environment).unwrap();
        manifest.environment_hash = sha256_bytes(&std::fs::read(&environment_path).unwrap());
        write_json(&manifest_path, &manifest).unwrap();
        write_json(&mirror_path, &manifest).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None);
    }

    #[test]
    fn source_check_environment_rejects_rehashed_unknown_and_duplicate_fields() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; }",
            "safe",
            None,
            vec![],
            root.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceCheckV1)
        );
        let environment_path = bundle.dir.join("environment.json");
        let original = std::fs::read_to_string(&environment_path).unwrap();
        let manifest_path = bundle.dir.join("evidence.json");
        let mirror_path = bundle.dir.join("manifest.json");
        for extra in [
            "\n  \"authorization\": \"forged\",",
            "\n  \"os\": \"forged\",",
        ] {
            let forged = original.replacen('{', &format!("{{{extra}"), 1);
            std::fs::write(&environment_path, &forged).unwrap();
            let mut manifest: EvidenceManifest =
                serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
            manifest.environment_hash = sha256_bytes(forged.as_bytes());
            write_json(&manifest_path, &manifest).unwrap();
            write_json(&mirror_path, &manifest).unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None);
        }
    }

    #[test]
    fn accepted_source_check_rejects_rehashed_unexpected_attachments() {
        let base = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; assert(x == 1); }",
            "safe",
            None,
            vec![],
            base.path(),
            Some("safe-check"),
            None,
        )
        .unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::SourceCheckV1)
        );
        for attachment in [
            "artifact",
            "program-evidence.json",
            "backend/risc0/receipt.bin",
            "analysis/proofs/extra.smt2",
        ] {
            let path = bundle.dir.join(attachment);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"attacker-added authority").unwrap();
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert_eq!(verify_pca_scope(&bundle.dir).unwrap(), None, "{attachment}");
            std::fs::remove_file(&path).unwrap();
            if attachment.starts_with("backend/") {
                std::fs::remove_dir_all(bundle.dir.join("backend")).unwrap();
            }
            refresh_manifest_hashes(&bundle.dir).unwrap();
            assert_eq!(
                verify_pca_scope(&bundle.dir).unwrap(),
                Some(PcaScope::SourceCheckV1)
            );
        }
    }

    #[test]
    fn source_snapshot_never_absorbs_a_binary_leaf() {
        // Regression (evidence-pipeline corruption): a native build artifact (Mach-O/ELF) that slips
        // into the collected file tree must NOT be concatenated into the `source.anubis` snapshot.
        // Before the fix, a tree with no leaf literally named `source.anubis` concatenated EVERY leaf
        // — appending the artifact's bytes, inflating a tiny source to hundreds of KB and making
        // `anubis report`'s parse stage emit thousands of errors (verdict FAIL though `check` passed).
        let base = unique_dir("binary-leaf");
        let text = "fn main() { let x = 1; }";
        let mut binary = vec![0xcfu8, 0xfa, 0xed, 0xfe]; // Mach-O 64 magic
        binary.extend_from_slice(&[0u8, 1, 2, 0, 255, 0]); // NUL bytes + non-UTF-8
        binary.extend(std::iter::repeat_n(0xABu8, 4096));
        let files = vec![
            ("main.anb".to_string(), text.as_bytes().to_vec()),
            ("anubis_out".to_string(), binary.clone()),
        ];
        let bundle =
            build_evidence_bundle_tree(&files, "safe", None, vec![], &base, None, None, None)
                .unwrap();
        let snap = std::fs::read(bundle.dir.join("source.anubis")).unwrap();
        assert!(
            !snap.contains(&0u8),
            "source.anubis must contain no NUL byte"
        );
        assert!(
            std::str::from_utf8(&snap)
                .map(|s| s.contains("fn main"))
                .unwrap_or(false),
            "source.anubis must retain the real source text"
        );
        assert!(
            snap.len() < text.len() + 64,
            "source.anubis must not be inflated by the {}-byte artifact (got {} bytes)",
            binary.len(),
            snap.len()
        );
        // The merkle source_hash still covers BOTH leaves — the integrity anchor is unchanged.
        assert!(validate_bundle(&bundle.dir).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_pca_tolerates_tool_version_drift_but_not_semantic_tamper() {
        let base = unique_dir("toolversion");
        let good = "fn main() { let x = 7; print(x); }";
        let bundle = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        assert!(
            verify_pca(&bundle.dir).unwrap(),
            "freshly built PCA must verify"
        );

        // Record a DIFFERENT tool/provenance version, then regenerate the manifest so the hash layer
        // stays consistent. A bundle emitted by another tool version must still cold-verify — the
        // claim is about the program, not which build produced it (the "stranger can cold-verify"
        // guarantee). This is the exact regression that coupling the equality to `tool` introduced.
        let mut drifted = derive_claim_block(good, "safe");
        drifted.tool = "anubis 99.99.99".to_string();
        write_json(&bundle.dir.join("pca.json"), &drifted).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        assert!(
            verify_pca(&bundle.dir).unwrap(),
            "a differing tool version must NOT fail cold-verification"
        );

        // But the fix must not open a hole: with the tool still drifted, a flipped SEMANTIC field
        // (manifest regenerated so hashes stay consistent) must still fail closed.
        let mut lie = derive_claim_block(good, "safe");
        lie.tool = "anubis 99.99.99".to_string();
        lie.solver_all_discharged = !lie.solver_all_discharged;
        write_json(&bundle.dir.join("pca.json"), &lie).unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        assert!(
            !verify_pca(&bundle.dir).unwrap(),
            "a semantic claim difference must still fail closed even when the tool version differs"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_pca_cold_verifies_the_migrated_v2_real_receipt_fixture() {
        // Only the deterministic claim wrapper and its manifest hash were migrated. The committed
        // RISC0 receipt, ImageID, and journal are unchanged; cold semantic verification must still
        // re-derive the v2 bounded claim and bind those genuine artifacts.
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/zk_prove_bundle");
        assert!(
            validate_bundle(&fixture).unwrap(),
            "the migrated fixture's recorded hash layer must remain intact"
        );
        let claim: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture.join("pca.json")).unwrap())
                .unwrap();
        assert_eq!(claim["pca_version"], 2);
        assert!(claim.get("taint_clean").is_none());
        assert!(
            verify_pca(&fixture).unwrap(),
            "PCA v2 verifier must cold-verify the migrated real-receipt fixture"
        );
    }

    /// Plant risc0 sidecars into a bundle so `derive_zk_binding` sees a (structurally) genuine
    /// receipt. `real` toggles the metadata flags that gate a real binding.
    fn plant_receipt(dir: &Path, image_id: &str, receipt: &[u8], real: bool) {
        let r = dir.join("backend").join("risc0");
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(r.join("receipt.bin"), receipt).unwrap();
        std::fs::write(r.join("image_id.txt"), image_id).unwrap();
        let journal_sha = sha256_bytes(b"journal-120");
        let meta = serde_json::json!({
            "verify_status": if real { "passed" } else { "failed" },
            "fresh_receipt_generated": real,
            "dev_mode": !real,
            "mock_prover": false,
            "image_id_is_placeholder": false,
            "image_id": image_id,
            "committed_journal_sha256": journal_sha,
        });
        std::fs::write(
            r.join("risc0_metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn derive_zk_binding_only_binds_a_genuine_receipt() {
        let base = unique_dir("zkbind");
        let id = "1 2 3 4 5 6 7 8";
        // No sidecars → no binding.
        let d0 = base.join("none");
        std::fs::create_dir_all(&d0).unwrap();
        assert!(derive_zk_binding(&d0).is_none());
        // A genuine (real=true) receipt → a binding naming the ImageID + digests.
        let d1 = base.join("real");
        std::fs::create_dir_all(&d1).unwrap();
        plant_receipt(&d1, id, b"a-real-looking-receipt-blob", true);
        let zk = derive_zk_binding(&d1).expect("genuine receipt binds");
        assert_eq!(zk.image_id, id);
        assert_eq!(
            zk.receipt_sha256,
            sha256_bytes(b"a-real-looking-receipt-blob")
        );
        // dev_mode receipt → not a binding.
        let d2 = base.join("dev");
        std::fs::create_dir_all(&d2).unwrap();
        plant_receipt(&d2, id, b"blob", false);
        assert!(derive_zk_binding(&d2).is_none());
        // placeholder receipt → not a binding.
        let d3 = base.join("placeholder");
        std::fs::create_dir_all(&d3).unwrap();
        plant_receipt(&d3, id, b"RISC0_RECEIPT_NOT_GENERATED\n", true);
        assert!(derive_zk_binding(&d3).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_pca_binds_receipt_and_catches_receipt_tamper() {
        let base = unique_dir("zkverify");
        let good = "fn main() { print(1); }";
        let bundle = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        let id =
            "168166999 2647531960 2381486741 2168976393 291594100 2287811983 3816763757 3860919363";
        // Plant a genuine receipt, re-derive the (now zk-bound) claim, re-hash the manifest.
        plant_receipt(&bundle.dir, id, b"receipt-blob-v1", true);
        write_json(
            &bundle.dir.join("pca.json"),
            &derive_claim_block_bound(&bundle.dir, good, "safe"),
        )
        .unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        // The recorded claim now carries the binding, and verify re-derives it → passes.
        let recorded: ClaimBlock =
            serde_json::from_str(&std::fs::read_to_string(bundle.dir.join("pca.json")).unwrap())
                .unwrap();
        assert!(recorded.zk_present && recorded.zk_image_id.as_deref() == Some(id));
        assert!(verify_pca(&bundle.dir).unwrap());
        // Swap the receipt for different bytes and re-hash the manifest (hash layer satisfied) but
        // leave the recorded claim naming the old receipt digest → re-derivation fails closed.
        std::fs::write(
            bundle.dir.join("backend/risc0/receipt.bin"),
            b"receipt-blob-TAMPERED",
        )
        .unwrap();
        write_manifest_hashes(&bundle.dir).unwrap();
        assert!(validate_bundle(&bundle.dir).unwrap());
        assert!(!verify_pca(&bundle.dir).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sign_and_verify_pca_roundtrip_then_tamper_fails() {
        let base = unique_dir("sign");
        let good = "fn main() { print(1); }";
        let bundle = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        // Unsigned bundle: valid, and reports no signature.
        assert!(pca_signature_status(&bundle.dir).unwrap().is_none());
        assert!(verify_pca(&bundle.dir).unwrap());

        // Sign, then verify reports the signature verified by that signer.
        let (sk, vk) = generate_keypair().unwrap();
        assert_eq!(sign_pca(&bundle.dir, &sk).unwrap(), vk);
        let (ok, pk) = pca_signature_status(&bundle.dir).unwrap().unwrap();
        assert!(ok && pk == vk);
        assert!(verify_pca(&bundle.dir).unwrap());

        // The sidecar is outside its own signature. Its advertised algorithm
        // and signed scope must still agree with what the verifier implements.
        let sig_path = bundle.dir.join("pca.sig");
        let original_sig = std::fs::read(&sig_path).unwrap();
        for (field, false_value) in [("algorithm", "other"), ("signed", "sha256(pca.json)")] {
            let mut sidecar: serde_json::Value = serde_json::from_slice(&original_sig).unwrap();
            sidecar[field] = serde_json::json!(false_value);
            write_json(&sig_path, &sidecar).unwrap();
            assert!(!pca_signature_status(&bundle.dir).unwrap().unwrap().0);
            assert!(!verify_pca(&bundle.dir).unwrap());
            std::fs::write(&sig_path, &original_sig).unwrap();
        }

        // Tamper the signed claim block: the signature no longer verifies → fail closed.
        let mut lie = derive_claim_block(good, "safe");
        lie.verdict = "FAIL".into();
        write_json(&bundle.dir.join("pca.json"), &lie).unwrap();
        let (ok2, _) = pca_signature_status(&bundle.dir).unwrap().unwrap();
        assert!(!ok2);
        assert!(!verify_pca(&bundle.dir).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }
}
