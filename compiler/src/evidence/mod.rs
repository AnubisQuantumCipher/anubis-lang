//! Sovereign evidence / reproducibility system.
//! Produces timestamped tamper-evident bundles modeled on risc0-metal-hybrid evidence.

use chrono::Utc;
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
    #[serde(default)]
    pub manifest_signature: String,
    pub checks: Vec<Check>,
    pub verdict: String,
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

pub fn build_evidence_bundle(
    source: &str,
    mode: &str,
    artifact: Option<&str>,
    logs: Vec<String>,
    out_base: &Path,
    lane: Option<&str>,
) -> Result<EvidenceBundle, String> {
    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = out_base.join(format!("evidence-{}-{}", ts, mode));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let source_hash = sha256_bytes(source.as_bytes());
    let build_log = logs.join("\n");
    let build_log_hash = sha256_bytes(build_log.as_bytes());
    let artifact_data = artifact
        .map(std::fs::read)
        .transpose()
        .map_err(|e| format!("artifact read failed: {}", e))?;
    let artifact_hash = artifact_data.as_deref().map(sha256_bytes);
    let hybrid_sidecars = copy_hybrid_sidecars(artifact, &dir)?;

    std::fs::write(dir.join("source.anubis"), source).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("build.log"), &build_log).map_err(|e| e.to_string())?;
    if let Some(data) = &artifact_data {
        std::fs::write(dir.join("artifact"), data).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(dir.join("analysis")).map_err(|e| e.to_string())?;

    let mut checks = vec![];
    let mut hir_json = serde_json::json!({"functions": []});
    let mut mir_json = serde_json::json!([]);
    let mut taint_json = serde_json::json!([]);
    let mut solver_json = serde_json::json!([]);

    let parse_res = crate::frontend::parse_source(source);
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
                        crate::middle::replay_counterexample_for_ir(
                            &tainted,
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
    let manifest_signature = sha256_bytes(
        format!(
            "{}:{}:{}:{}",
            source_hash, build_log_hash, source_tree_hash, verdict
        )
        .as_bytes(),
    );

    let manifest = EvidenceManifest {
        timestamp: ts,
        tool: "anubis 0.2.0".into(),
        mode: mode.into(),
        source_hash,
        build_log_hash,
        artifact_hash,
        lane: lane.map(str::to_string),
        environment_hash,
        source_tree_hash,
        sarif_hash,
        bounty_report_hash,
        manifest_signature,
        checks,
        verdict,
    };

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("evidence.json"), &json).map_err(|e| e.to_string())?;
    // v1 schema prefers manifest.json as well
    std::fs::write(dir.join("manifest.json"), &json).map_err(|e| e.to_string())?;
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
    let source_ok =
        sha256_file(&dir.join("source.anubis")).is_some_and(|hash| hash == manifest.source_hash);
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
            } else if check.detail.contains("replay") || check.detail.contains("REPLAY_FAILED") {
                "ANUBIS_SOLVER_MODEL_REPLAY_FAILED".to_string()
            } else if check.detail.contains("unsupported") {
                "ANUBIS_SOLVER_UNSUPPORTED_EXPRESSION".to_string()
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
    let passed = verify_status == "passed"
        && fresh
        && !dev_mode
        && !mock_prover
        && !cache_used
        && !placeholder;
    Some(Check {
        name: "risc0_receipt_verify".into(),
        status: if passed { "PASS" } else { "FAIL" }.into(),
        detail: format!(
            "verify_status={} fresh_receipt_generated={} dev_mode={} mock_prover={} cache_used={} placeholder_image_id={}",
            verify_status, fresh, dev_mode, mock_prover, cache_used, placeholder
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
