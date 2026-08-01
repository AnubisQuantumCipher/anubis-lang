//! End-to-end acceptance matrix for the default contract execution boundary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace_tmp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "anubis-{label}-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ))
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

#[test]
fn false_contract_is_rejected_by_check_run_test_and_build() {
    let root = workspace_tmp("contract-matrix-false");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("false.anb");
    std::fs::write(
        &source,
        "fn impossible(x: i64) -> i64 requires(x == 100) ensures(result <= 100) { return x + 1; }\nfn main() { print(impossible(100)); }\n",
    )
    .unwrap();

    let source = path_text(&source);
    let check_out = root.join("check");
    let run_out = root.join("run");
    let build_out = root.join("build");
    let cases = [
        vec!["check", source, "--out", path_text(&check_out)],
        vec!["run", source, "--out", path_text(&run_out)],
        vec!["test", source],
        vec!["build", source, "--out", path_text(&build_out)],
    ];
    for args in cases {
        let output = run(&args);
        assert!(
            !output.status.success(),
            "{} must reject the false contract; output:\n{}",
            args[0],
            combined(&output)
        );
        assert!(
            combined(&output).contains("ANUBIS_ASSERTION_DISPROVED"),
            "{} must preserve the counterexample diagnostic; output:\n{}",
            args[0],
            combined(&output)
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn valid_contract_passes_check_run_test_and_build() {
    let root = workspace_tmp("contract-matrix-valid");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("valid.anb");
    std::fs::write(
        &source,
        "fn inc(x: i64) -> i64 requires(x >= 0) requires(x < 100) ensures(result == x + 1) { return x + 1; }\nfn main() { print(inc(41)); }\n",
    )
    .unwrap();

    let source = path_text(&source);
    let check_out = root.join("check");
    let run_out = root.join("run");
    let build_out = root.join("build");
    let cases = [
        vec!["check", source, "--out", path_text(&check_out)],
        vec!["run", source, "--out", path_text(&run_out)],
        vec!["test", source],
        vec!["build", source, "--out", path_text(&build_out)],
    ];
    for args in cases {
        let output = run(&args);
        assert!(
            output.status.success(),
            "{} must accept the valid contract; output:\n{}",
            args[0],
            combined(&output)
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn explicit_bypass_is_integrity_valid_but_unverified() {
    let root = workspace_tmp("contract-matrix-bypass");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("false.anb");
    std::fs::write(
        &source,
        "fn impossible() -> i64 ensures(result == 0) { return 1; }\nfn main() { print(impossible()); }\n",
    )
    .unwrap();

    let build_out = root.join("build");
    let output = run(&[
        "build",
        path_text(&source),
        "--no-verify",
        "--evidence",
        "--out",
        path_text(&build_out),
    ]);
    assert!(
        output.status.success(),
        "explicit bypass failed: {}",
        combined(&output)
    );
    assert!(combined(&output).contains("verdict: UNVERIFIED"));

    let bundle = build_out.join("unverified-evidence");
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(bundle.join("unverified.json")).unwrap())
            .unwrap();
    assert_eq!(record["status"], "UNVERIFIED");
    assert_eq!(record["truth"]["contracts_verified"], false);
    assert_eq!(record["truth"]["proof_execution_claimed"], false);
    assert_eq!(record["truth"]["receipt_verified"], false);

    let verify = run(&["verify", path_text(&bundle)]);
    assert!(
        verify.status.success(),
        "integrity envelope invalid: {}",
        combined(&verify)
    );
    assert!(combined(&verify).contains("assurance: UNVERIFIED"));
    std::fs::write(bundle.join("source.anubis"), "fn main(){print(9);}\n").unwrap();
    let tampered = run(&["verify", path_text(&bundle)]);
    assert!(
        !tampered.status.success(),
        "tampered UNVERIFIED envelope must fail integrity validation"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn build_evidence_excludes_unselected_manifestless_siblings() {
    let root = workspace_tmp("evidence-verdict-failclosed");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let entry = root.join("main.anb");
    std::fs::write(&entry, "fn main(){ print(1); }\n").unwrap();
    // A manifest-less invocation is a synthetic single-file project. An unrelated sibling must not
    // alter the checked program, the evidence verdict, or the sealed source snapshot.
    std::fs::write(
        root.join("hostile_sibling.anb"),
        "fn helper(){ let x=taint_source(\"operator\"); sink(x); }\n",
    )
    .unwrap();
    let output = run(&[
        "build",
        path_text(&entry),
        "--evidence",
        "--out",
        path_text(&root.join("out")),
    ]);
    assert!(
        output.status.success(),
        "unselected sibling must not poison entry evidence: {}",
        combined(&output)
    );
    assert!(combined(&output).contains("verdict: PASS"));
    let bundle = std::fs::read_dir(root.join("out"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("evidence-"))
        })
        .expect("evidence bundle");
    assert_eq!(
        std::fs::read_to_string(bundle.join("source.anubis")).unwrap(),
        "fn main(){ print(1); }\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn declassified_taint_executes_but_undeclassified_taint_is_rejected() {
    let root = workspace_tmp("taint-native-boundary");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let accepted = root.join("accepted.anb");
    let rejected = root.join("rejected.anb");
    std::fs::write(
        &accepted,
        "fn main() { let x = taint_source(\"operator\"); let y = declassify(x, \"fixed.v1\", \"bounded local workflow\"); sink(y); print(\"ok\"); }\n",
    )
    .unwrap();
    std::fs::write(
        &rejected,
        "fn main() { let x = taint_source(\"operator\"); sink(x); }\n",
    )
    .unwrap();

    let ok = run(&[
        "run",
        path_text(&accepted),
        "--out",
        path_text(&root.join("accepted-out")),
    ]);
    assert!(
        ok.status.success(),
        "declassified flow must run: {}",
        combined(&ok)
    );
    let bad = run(&[
        "run",
        path_text(&rejected),
        "--out",
        path_text(&root.join("rejected-out")),
    ]);
    assert!(!bad.status.success(), "undeclassified flow must not run");
    assert!(combined(&bad).contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn verified_taint_qualifiers_and_declassified_secrets_lower_natively() {
    let root = workspace_tmp("taint-secret-native-lowering");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("labels.anb");
    std::fs::write(
        &source,
        r#"
fn main() {
    let labelled: tainted<u32> = 7;
    let clean = declassify(labelled, "local.v1", "approved local display");
    sink(clean);
    let secret = secret_source("sealed-value");
    let released = declassify(secret, "local.v1", "approved local display");
    sink(released);
    print(clean);
    print(released);
}
"#,
    )
    .unwrap();

    let output = run(&[
        "run",
        path_text(&source),
        "--out",
        path_text(&root.join("out")),
    ]);
    assert!(
        output.status.success(),
        "verified label constructs must lower after the checker approves them: {}",
        combined(&output)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\nsealed-value\n");

    let _ = std::fs::remove_dir_all(&root);
}
