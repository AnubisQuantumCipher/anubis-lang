//! Host-safe adversarial tests for the program-wide Safe/Research boundary.
//!
//! These tests never pass `--allow-research` and never execute a research artifact. They exercise
//! static checking plus the fail-before-lowering run gate; crash-capable execution remains VZ-only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn workspace_tmp(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("anubis-{label}-{}-{nanos}", std::process::id()))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(args)
        .output()
        .expect("run anubis CLI")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("temporary path must be UTF-8")
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap())
        .unwrap_or_else(|error| panic!("read JSON {}: {error}", path.display()))
}

fn evidence_dirs(out: &Path) -> Vec<PathBuf> {
    let mut dirs = std::fs::read_dir(out)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("evidence-"))
        })
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

#[test]
fn later_research_function_elevates_evidence_and_run_gate() {
    let root = workspace_tmp("mixed-mode-aggregate");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("mixed.anb");
    let check_out = root.join("check");
    let run_out = root.join("run");
    std::fs::write(
        &source,
        r#"
@safe
fn first() {}

@research(authorization: "authorized-static-regression")
fn later() {}

@safe
fn main() {
    print("not executed by this test");
}
"#,
    )
    .unwrap();

    let checked = run(&[
        "check",
        path_text(&source),
        "--evidence",
        "--out",
        path_text(&check_out),
    ]);
    assert!(
        checked.status.success(),
        "static mixed-mode check failed: {}",
        combined(&checked)
    );
    let summary = read_json(&check_out.join("check-summary.json"));
    let bundle = PathBuf::from(summary["bundle"].as_str().unwrap());
    let manifest = read_json(&bundle.join("manifest.json"));
    assert_eq!(manifest["verdict"], "PASS");
    assert_eq!(
        manifest["mode"], "research",
        "a later Research function must elevate program evidence"
    );

    let rejected = run(&["run", path_text(&source), "--out", path_text(&run_out)]);
    assert!(!rejected.status.success());
    assert!(
        combined(&rejected).contains("ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW"),
        "unexpected run rejection: {}",
        combined(&rejected)
    );
    assert!(
        !run_out.join("anubis_run").exists() && !run_out.join("anubis_run.rs").exists(),
        "research gate must reject before native lowering"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn explicit_safe_enclave_is_not_weakened_by_research_aggregate() {
    let root = workspace_tmp("mixed-mode-safe-enclave");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("safe-enclave.anb");
    let out = root.join("out");
    std::fs::write(
        &source,
        r#"
@research(authorization: "authorized-static-regression")
fn research_helper() {}

@safe
fn main() {
    let secret: tainted<u32> = symbolic();
    sink(secret);
}
"#,
    )
    .unwrap();

    let rejected = run(&["check", path_text(&source), "--out", path_text(&out)]);
    assert!(!rejected.status.success());
    assert!(
        combined(&rejected).contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
        "explicit Safe enclave lost enforcement: {}",
        combined(&rejected)
    );

    let summary = read_json(&out.join("check-summary.json"));
    assert_eq!(summary["verdict"], "FAIL");
    let bundle = PathBuf::from(summary["bundle"].as_str().unwrap());
    let manifest = read_json(&bundle.join("manifest.json"));
    let pca = read_json(&bundle.join("pca.json"));
    assert_eq!(manifest["mode"], "research");
    assert_eq!(manifest["verdict"], "FAIL");
    assert_eq!(pca["tier"], "rejected");
    assert_eq!(pca["verdict"], "FAIL");
    assert!(pca["rejection"]
        .as_str()
        .unwrap()
        .contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"));
    assert!(!bundle.join("artifact").exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn build_rejection_evidence_is_fail_only_and_artifact_free() {
    let root = workspace_tmp("build-rejection-evidence");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("false-contract.anb");
    let out = root.join("out");
    std::fs::write(
        &source,
        "fn impossible() -> i64 ensures(result == 0) { return 1; }\nfn main() { print(impossible()); }\n",
    )
    .unwrap();

    let rejected = run(&[
        "build",
        path_text(&source),
        "--evidence",
        "--out",
        path_text(&out),
    ]);
    assert!(!rejected.status.success());
    assert!(
        combined(&rejected).contains("ANUBIS_ASSERTION_DISPROVED"),
        "counterexample diagnostic was lost: {}",
        combined(&rejected)
    );
    assert!(combined(&rejected).contains("verdict: FAIL"));
    assert!(combined(&rejected).contains("artifact: NONE"));

    let bundles = evidence_dirs(&out);
    assert_eq!(
        bundles.len(),
        1,
        "expected one rejection bundle: {bundles:?}"
    );
    let manifest = read_json(&bundles[0].join("manifest.json"));
    let pca = read_json(&bundles[0].join("pca.json"));
    assert_eq!(manifest["verdict"], "FAIL");
    assert_eq!(pca["tier"], "rejected");
    assert_eq!(pca["verdict"], "FAIL");
    assert!(!bundles[0].join("artifact").exists());
    assert!(!out.join("anubis_out").exists());
    let integrity = Command::new("sh")
        .arg(bundles[0].join("validate.sh"))
        .output()
        .expect("run rejection-bundle integrity validator");
    assert!(
        integrity.status.success(),
        "rejection evidence must remain internally hash-valid: {}",
        combined(&integrity)
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn imported_program_evidence_analyzes_the_resolved_program() {
    let root = workspace_tmp("resolved-import-evidence");
    let _ = std::fs::remove_dir_all(&root);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/security/poc_stdlib_overflow.anb");
    let output = run(&[
        "check",
        path_text(&fixture),
        "--evidence",
        "--out",
        path_text(&root),
    ]);
    assert!(
        output.status.success(),
        "a valid imported program and its evidence must agree: {}",
        combined(&output)
    );

    let summary = read_json(&root.join("check-summary.json"));
    let bundle = PathBuf::from(summary["bundle"].as_str().unwrap());
    let manifest = read_json(&bundle.join("manifest.json"));
    assert_eq!(manifest["verdict"], "PASS");
    assert_eq!(manifest["mode"], "research");
    assert!(bundle.join("source-merkle-leaves.json").is_file());
    let resolved = std::fs::read_to_string(bundle.join("source.anubis")).unwrap();
    assert!(
        !resolved.contains("import std.pwn"),
        "semantic snapshot must contain the resolved program, not an unresolved import"
    );
    assert!(resolved.contains("pwn__run_local"));

    let verified = run(&["verify", path_text(&bundle)]);
    assert!(
        verified.status.success(),
        "resolved import evidence must cold-verify: {}",
        combined(&verified)
    );
    assert!(combined(&verified).contains("bundle valid: true"));

    let _ = std::fs::remove_dir_all(&root);
}
