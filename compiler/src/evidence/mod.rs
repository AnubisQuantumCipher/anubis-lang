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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Check {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvironmentCapture {
    pub os: String,
    pub arch: String,
    pub rustc: String,
    pub cargo: String,
    pub z3: String,
    pub anubis: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SourceTreeEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug)]
pub struct EvidenceBundle {
    pub dir: PathBuf,
    pub manifest: EvidenceManifest,
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
        &[( "source.anubis".to_string(), source.as_bytes().to_vec() )],
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
    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = out_base.join(format!("evidence-{}-{}", ts, mode));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let source_hash = crate::package::merkle::merkle_root(files.to_vec());
    // Primary source body for re-derive: first leaf named source.anubis, else concat the SOURCE
    // leaves. The concat fallback filters to leaves that are valid UTF-8 text and free of NUL bytes —
    // a build artifact (Mach-O / ELF) is neither, so it can never be appended into the `source.anubis`
    // snapshot. Defense-in-depth: without this, a caller that passes a binary leaf (e.g. a native
    // artifact that slipped into the collected file tree) would inflate `source.anubis` with the
    // artifact's bytes, making `anubis report` fail to parse it. The merkle `source_hash` above is
    // still taken over ALL leaves, so bundle integrity is unchanged — only the human/parser-facing
    // snapshot is kept clean.
    let source = files
        .iter()
        .find(|(p, _)| p == "source.anubis" || p.ends_with("/source.anubis"))
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_else(|| {
            files
                .iter()
                .filter(|(_, b)| std::str::from_utf8(b).is_ok() && !b.contains(&0))
                .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
                .collect::<Vec<_>>()
                .join("\n")
        });
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
    if let Ok(sum) =
        crate::package::summary::extract_from_source_text("package", "0.0.0", &source)
    {
        let _ = crate::package::summary::write_to_evidence_dir(&dir, &sum);
    }
    // Optional multi-leaf listing for re-verify of Merkle source_hash.
    if files.len() > 1 {
        let leaves: Vec<serde_json::Value> = files
            .iter()
            .map(|(p, b)| {
                serde_json::json!({
                    "path": p,
                    "sha256": sha256_bytes(b),
                    "bytes": b.len(),
                })
            })
            .collect();
        write_json(
            &dir.join("source-merkle-leaves.json"),
            &serde_json::json!({ "source_merkle_root": source_hash, "leaves": leaves }),
        )?;
    }
    std::fs::create_dir_all(dir.join("analysis")).map_err(|e| e.to_string())?;

    let mut checks = vec![];
    let mut hir_json = serde_json::json!({"functions": []});
    let mut mir_json = serde_json::json!([]);
    let mut taint_json = serde_json::json!([]);
    let mut solver_json = serde_json::json!([]);

    let parse_res = crate::frontend::parse_source(&source);
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

    if let Ok(ast) = parse_res {
        let tc_mode = if mode == "research" {
            crate::frontend::Mode::Research
        } else {
            crate::frontend::Mode::Safe
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
                // save smt and replay for gate7
                if let Some(first) = solver_checks.first() {
                    let _ = std::fs::write(dir.join("analysis").join("solver.smt2"), &first.smt);
                    let replay = if first.status == "FAIL" && first.model.is_some() {
                        crate::middle::replay_counterexample(
                            &first.smt,
                            first.model.as_deref().unwrap_or(""),
                        )
                    } else {
                        true
                    };
                    let replay_json = serde_json::json!({
                        "status": if replay { "counterexample_replayed" } else { "replay_failed" },
                        "replay_valid": replay
                    });
                    let _ = std::fs::write(
                        dir.join("analysis").join("solver_replay.json"),
                        serde_json::to_string_pretty(&replay_json).unwrap(),
                    );
                }

                checks.push(Check {
                    name: "typecheck".into(),
                    status: "PASS".into(),
                    detail: format!(
                        "mode={} symbols={} functions={}",
                        mode,
                        tainted.symbols.len(),
                        tainted.hir.functions.len()
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
                let solver_status = if solver_checks.iter().all(|check| check.status == "PASS") {
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
            Err(err) => checks.push(Check {
                name: "typecheck".into(),
                status: "FAIL".into(),
                detail: err,
            }),
        }
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

    write_json(&dir.join("hir.json"), &hir_json)?;
    write_json(&dir.join("mir.json"), &mir_json)?;
    write_json(&dir.join("taint-traces.json"), &taint_json)?;
    write_json(&dir.join("solver.json"), &solver_json)?;

    let environment = capture_environment();
    write_json(&dir.join("environment.json"), &environment)?;

    let sarif = build_sarif(&checks);
    write_json(&dir.join("checks.sarif"), &sarif)?;

    let report = build_bounty_report(mode, lane, &checks);
    std::fs::write(dir.join("bounty-report.md"), &report).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("validate.sh"),
        "#!/usr/bin/env sh\nset -eu\n# Self-contained bundle validation (no 'anubis' CLI dependency to avoid arg parsing errors).\n# Checks that all files listed in MANIFEST.sha256 still match their recorded hashes.\nDIR=$(dirname \"$0\")\nif [ ! -f \"$DIR/MANIFEST.sha256\" ]; then\n  echo 'MISSING MANIFEST.sha256' >&2\n  exit 1\nfi\nwhile read -r line; do\n  [ -z \"$line\" ] && continue\n  hash=$(echo \"$line\" | cut -d' ' -f1)\n  file=$(echo \"$line\" | cut -d' ' -f2- | xargs)\n  if [ -f \"$DIR/$file\" ]; then\n    actual=$(shasum -a 256 \"$DIR/$file\" | cut -d' ' -f1)\n    if [ \"$actual\" != \"$hash\" ]; then\n      echo \"TAMPER: $file hash mismatch\" >&2\n      exit 1\n    fi\n  else\n    echo \"MISSING: $file\" >&2\n    exit 1\n  fi\ndone < \"$DIR/MANIFEST.sha256\"\necho 'validate.sh: OK'\n",
    )
    .map_err(|e| e.to_string())?;
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
        tracked_bundle_files(artifact_hash.is_some(), &hybrid_sidecars),
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
    // Proof-Carrying Artifact claim block — a deterministic verdict `verify` re-derives from the
    // source (plus a ZK receipt binding when the bundle carries a genuine receipt). Written before
    // the manifest hashing so it is covered by MANIFEST.sha256.
    write_json(
        &dir.join("pca.json"),
        &derive_claim_block_bound(&dir, &source, mode),
    )?;
    write_manifest_hashes(&dir)?;

    Ok(EvidenceBundle { dir, manifest })
}

pub fn validate_bundle(dir: &Path) -> Result<bool, String> {
    let manifest_path = dir.join("evidence.json");
    if !manifest_path.exists() {
        return Err("no evidence.json".into());
    }
    let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let manifest: EvidenceManifest =
        serde_json::from_str(&manifest_text).map_err(|e| e.to_string())?;

    let manifest_entries_ok = validate_manifest_hashes(dir)?;
    // Single-file: source_hash == sha256(source.anubis). Multi-file: matches recorded merkle.
    let source_ok = sha256_file(&dir.join("source.anubis"))
        .is_some_and(|hash| hash == manifest.source_hash)
        || dir.join("source-merkle-leaves.json").is_file()
            && std::fs::read_to_string(dir.join("source-merkle-leaves.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| {
                    v.get("source_merkle_root")
                        .and_then(|r| r.as_str())
                        .map(|r| r == manifest.source_hash)
                })
                .unwrap_or(false);
    let build_log_ok = manifest.build_log_hash.is_empty()
        || sha256_file(&dir.join("build.log")).is_some_and(|hash| hash == manifest.build_log_hash);
    let artifact_ok = match &manifest.artifact_hash {
        Some(expected) => sha256_file(&dir.join("artifact")).is_some_and(|hash| hash == *expected),
        None => true,
    };
    let env_ok = manifest.environment_hash.is_empty()
        || sha256_file(&dir.join("environment.json"))
            .is_some_and(|hash| hash == manifest.environment_hash);
    let source_tree_ok = manifest.source_tree_hash.is_empty()
        || sha256_file(&dir.join("source-tree.json"))
            .is_some_and(|hash| hash == manifest.source_tree_hash);
    let sarif_ok = manifest.sarif_hash.is_empty()
        || sha256_file(&dir.join("checks.sarif")).is_some_and(|hash| hash == manifest.sarif_hash);
    let report_ok = manifest.bounty_report_hash.is_empty()
        || sha256_file(&dir.join("bounty-report.md"))
            .is_some_and(|hash| hash == manifest.bounty_report_hash);
    let checks_ok = manifest.checks.iter().all(|c| c.status == "PASS");

    Ok(manifest_entries_ok
        && source_ok
        && build_log_ok
        && artifact_ok
        && env_ok
        && source_tree_ok
        && sarif_ok
        && report_ok
        && checks_ok
        && manifest.verdict == "PASS")
}

/// The Proof-Carrying Artifact claim block: a deterministic, independently-checkable summary of what
/// a program is claimed to be. Unlike the hash-based manifest, it records the *semantic* verdict
/// (parse / typecheck / taint / solver), so `anubis verify` can RE-DERIVE it from the source and
/// confirm the recorded claim is honest — not merely untampered. It carries no timestamp: the same
/// source + mode always yields the same block, which is what makes re-derivation a real cross-check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimBlock {
    pub pca_version: u32,
    pub source_sha256: String,
    pub mode: String,
    /// Assurance tier actually reached. v0: `"checked"` — parse + typecheck + taint + solver ran.
    pub tier: String,
    pub parse_ok: bool,
    pub typecheck_ok: bool,
    /// No tainted value reaches a sink without declassification.
    pub taint_clean: bool,
    pub solver_obligations: usize,
    pub solver_all_discharged: bool,
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

/// Re-derive the claim block from source. Deterministic and side-effect free — the single source of
/// truth used both when emitting a PCA and when verifying one, so the two agree exactly.
pub fn derive_claim_block(source: &str, mode: &str) -> ClaimBlock {
    let source_sha256 = sha256_bytes(source.as_bytes());
    let tc_mode = if mode == "research" {
        crate::frontend::Mode::Research
    } else {
        crate::frontend::Mode::Safe
    };
    let parse_res = crate::frontend::parse_source(source);
    let parse_ok = parse_res.is_ok();
    let mut typecheck_ok = false;
    let mut taint_clean = false;
    let mut solver_obligations = 0usize;
    let mut solver_all_discharged = true;
    if let Ok(ast) = parse_res {
        if let Ok(ir) = crate::middle::typecheck(ast, tc_mode) {
            typecheck_ok = true;
            let tainted = crate::middle::TaintPass::apply(ir);
            // A tainted flow that reaches a sink must be declassified to count as clean.
            taint_clean = tainted
                .taint_traces
                .iter()
                .all(|t| t.sink.is_none() || t.declassified);
            let solver_checks = crate::middle::SymbolicEngine::check_obligations(&tainted);
            solver_obligations = solver_checks.len();
            solver_all_discharged = solver_checks.iter().all(|c| c.status == "PASS");
        }
    }
    let verdict = if parse_ok && typecheck_ok && taint_clean && solver_all_discharged {
        "PASS"
    } else {
        "FAIL"
    };
    ClaimBlock {
        pca_version: 1,
        source_sha256,
        mode: mode.to_string(),
        tier: "checked".into(),
        parse_ok,
        typecheck_ok,
        taint_clean,
        solver_obligations,
        solver_all_discharged,
        zk_present: false,
        zk_image_id: None,
        zk_receipt_sha256: None,
        zk_journal_sha256: None,
        verdict: verdict.into(),
        tool: tool_identity(),
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
    let r = dir.join("backend").join("risc0");
    let receipt_path = r.join("receipt.bin");
    let image_id_path = r.join("image_id.txt");
    let meta_path = r.join("risc0_metadata.json");
    if !receipt_path.exists() || !image_id_path.exists() || !meta_path.exists() {
        return None;
    }
    let receipt_bytes = std::fs::read(&receipt_path).ok()?;
    // A placeholder receipt is written when proving failed — it is never a binding.
    if receipt_bytes.starts_with(b"RISC0_RECEIPT_NOT_GENERATED") {
        return None;
    }
    let image_id = std::fs::read_to_string(&image_id_path)
        .ok()?
        .trim()
        .to_string();
    // A real ImageID is eight whitespace-separated u32 words (not the failure sentinel).
    let words: Vec<&str> = image_id.split_whitespace().collect();
    if words.len() != 8 || words.iter().any(|w| w.parse::<u32>().is_err()) {
        return None;
    }
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).ok()?).ok()?;
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
    Some(ZkBinding {
        image_id,
        receipt_sha256: sha256_bytes(&receipt_bytes),
        journal_sha256,
    })
}

/// The full claim block for a bundle: the source-derived analysis claim, plus a ZK receipt binding
/// when the bundle carries a genuine receipt. Used both when emitting a PCA and when verifying one,
/// so the two agree exactly (including the ZK fields).
pub fn derive_claim_block_bound(dir: &Path, source: &str, mode: &str) -> ClaimBlock {
    let mut cb = derive_claim_block(source, mode);
    if let Some(zk) = derive_zk_binding(dir) {
        cb.zk_present = true;
        cb.zk_image_id = Some(zk.image_id);
        cb.zk_receipt_sha256 = Some(zk.receipt_sha256);
        cb.zk_journal_sha256 = Some(zk.journal_sha256);
    }
    cb
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

pub fn verify_pca(dir: &Path) -> Result<bool, String> {
    let hashes_ok = validate_bundle(dir)?;
    let pca_path = dir.join("pca.json");
    if !pca_path.exists() {
        // Legacy bundle without a claim block: hash validation is all that is available.
        return Ok(hashes_ok);
    }
    let recorded: ClaimBlock =
        serde_json::from_str(&std::fs::read_to_string(&pca_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let source = std::fs::read_to_string(dir.join("source.anubis")).map_err(|e| e.to_string())?;
    // Explicit source binding: the claim's recorded hash must be the hash of the bundle's own
    // source. (Also implied by `fresh == recorded`, but asserted directly so the source↔claim tie
    // can never drift.)
    let source_bound = recorded.source_sha256 == sha256_bytes(source.as_bytes());
    // Re-derive the full claim — including the ZK binding — from the bundle's own artifacts. A
    // tampered receipt, a swapped ImageID, or a claim that lies about carrying a receipt makes the
    // re-derived block differ from the recorded one and fails closed here (the CLI additionally
    // re-verifies the receipt cryptographically against the ImageID).
    let fresh = derive_claim_block_bound(dir, &source, &recorded.mode);
    // If the bundle is signed, the signature must verify over the current claim + manifest. An
    // unsigned bundle is still a valid (unsigned) PCA. A forged/invalid signature fails closed.
    let sig_ok = match pca_signature_status(dir)? {
        Some((ok, _signer)) => ok,
        None => true,
    };
    Ok(hashes_ok && source_bound && sig_ok && claim_semantically_matches(&fresh, &recorded))
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
    let pca = std::fs::read(dir.join("pca.json")).map_err(|e| format!("read pca.json: {e}"))?;
    let manifest =
        std::fs::read(dir.join("MANIFEST.sha256")).map_err(|e| format!("read manifest: {e}"))?;
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
    if !sig_path.exists() {
        return Ok(None);
    }
    let rec: PcaSignature =
        serde_json::from_str(&std::fs::read_to_string(&sig_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
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

fn capture_environment() -> EnvironmentCapture {
    EnvironmentCapture {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        rustc: command_output("rustc", &["--version"]),
        cargo: command_output("cargo", &["--version"]),
        z3: command_output("z3", &["--version"]),
        anubis: env!("CARGO_PKG_VERSION").into(),
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
        && patch_active
        && methods_patch_active
        && prover_patch_active
        && reference_ok
        && vendor_ok;
    Some(Check {
        name: "risc0_receipt_verify".into(),
        status: if passed { "PASS" } else { "FAIL" }.into(),
        detail: format!(
            "verify_status={} fresh_receipt_generated={} dev_mode={} mock_prover={} cache_used={} placeholder_image_id={} patch_crates_io_active={} methods_patch_crates_io_active={} prover_patch_crates_io_active={} reference_ok={} vendor_ok={}",
            verify_status,
            fresh,
            dev_mode,
            mock_prover,
            cache_used,
            placeholder,
            patch_active,
            methods_patch_active,
            prover_patch_active,
            reference_ok,
            vendor_ok
        ),
    })
}

fn tracked_bundle_files(has_artifact: bool, hybrid_sidecars: &[(String, String)]) -> Vec<String> {
    let mut files = vec![
        "source.anubis",
        "build.log",
        "hir.json",
        "mir.json",
        "taint-traces.json",
        "solver.json",
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

fn validate_manifest_hashes(dir: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(dir.join("MANIFEST.sha256")).map_err(|e| e.to_string())?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(expected) = parts.next() else {
            return Ok(false);
        };
        let Some(path) = parts.next() else {
            return Ok(false);
        };
        let actual = sha256_file(&dir.join(path));
        if actual.as_deref() != Some(expected) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod pca_tests {
    use super::*;

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
    fn verify_pca_rederives_claim_and_catches_a_consistent_lie() {
        let base = unique_dir("rederive");
        let good = "fn main() { let x = 1; print(x); }";
        let bundle = build_evidence_bundle(good, "safe", None, vec![], &base, None, None).unwrap();
        // A freshly built PCA verifies.
        assert!(verify_pca(&bundle.dir).unwrap());

        // Forge the claim block so it disagrees with the source, then regenerate the manifest so
        // every hash is internally consistent — a hash-only check would now be satisfied.
        let mut lie = derive_claim_block(good, "safe");
        lie.taint_clean = !lie.taint_clean;
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
        binary.extend(std::iter::repeat(0xABu8).take(4096));
        let files = vec![
            ("main.anb".to_string(), text.as_bytes().to_vec()),
            ("anubis_out".to_string(), binary.clone()),
        ];
        let bundle =
            build_evidence_bundle_tree(&files, "safe", None, vec![], &base, None, None, None)
                .unwrap();
        let snap = std::fs::read(bundle.dir.join("source.anubis")).unwrap();
        assert!(!snap.contains(&0u8), "source.anubis must contain no NUL byte");
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
    fn verify_pca_cold_verifies_the_committed_zk_receipt_fixture() {
        // The committed real-receipt fixture was frozen by an earlier tool version ("anubis 0.2.0").
        // Its claim re-derives and its manifest is intact, so it MUST cold-verify under the current
        // tool — the regression guard for the version-coupling bug that made verify reject it.
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/zk_prove_bundle");
        assert!(
            verify_pca(&fixture).unwrap(),
            "committed ZK receipt fixture must cold-verify (only the provenance tool version differs)"
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
