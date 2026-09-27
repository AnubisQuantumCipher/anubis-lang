//! Separate CLI processes checking the same source must emit identical symbolic identities.
//! A process-global counter made the obligation text depend on unrelated earlier checks and
//! prevented exact source-derived evidence replay.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SOURCE: &str = r#"
fn source() -> i64 ensures(result == 1) { return 1; }
fn need(v: i64) -> i64 requires(v > 0) { return v; }
fn main() {
    match 1 { x => { need(x); } }
    let value = match 1 { x => need(x) };
    need(source());
}
"#;

fn check(source: &Path, out: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg(source)
        .args(["--evidence", "--out"])
        .arg(out)
        .output()
        .expect("run anubis check")
}

fn bundle(out: &Path) -> PathBuf {
    let mut paths = std::fs::read_dir(out)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("evidence-"))
        });
    let found = paths.next().expect("evidence bundle");
    assert!(paths.next().is_none(), "one evidence bundle");
    found
}

#[test]
fn cold_process_checks_emit_the_same_contract_and_match_queries() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("program.anb");
    std::fs::write(&source, SOURCE).unwrap();
    let left_out = dir.path().join("left");
    let right_out = dir.path().join("right");
    let left = check(&source, &left_out);
    let right = check(&source, &right_out);
    for output in [&left, &right] {
        assert!(
            output.status.success(),
            "check refused: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let left = bundle(&left_out);
    let right = bundle(&right_out);
    let left_solver = std::fs::read(left.join("solver.json")).unwrap();
    let right_solver = std::fs::read(right.join("solver.json")).unwrap();
    assert_eq!(
        left_solver, right_solver,
        "solver inventory must be byte-stable"
    );
    let solver_text = String::from_utf8(left_solver).unwrap();
    assert!(solver_text.contains("contractarg"), "{solver_text}");
    assert!(solver_text.contains("mbind"), "{solver_text}");
    assert!(solver_text.contains("armbind"), "{solver_text}");
    assert_eq!(
        std::fs::read(left.join("analysis/proofs.json")).unwrap(),
        std::fs::read(right.join("analysis/proofs.json")).unwrap(),
        "proof index must be byte-stable"
    );
    let proof_dir = left.join("analysis/proofs");
    for entry in std::fs::read_dir(proof_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "smt2") {
            let name = path.file_name().unwrap();
            assert_eq!(
                std::fs::read(&path).unwrap(),
                std::fs::read(right.join("analysis/proofs").join(name)).unwrap(),
                "SMT query must be byte-stable: {}",
                name.to_string_lossy()
            );
        }
    }
}
