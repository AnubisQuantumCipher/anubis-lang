use std::io::Write;
use std::process::{Command, Output, Stdio};

fn exact_repl_with_parent_input(timeout: &str) -> Output {
    // An ordinary process exit makes inherited input observable without
    // printing tainted data or running a crash-capable witness.
    let source = r#"fn main() { let s = read_line(); if s != "" { exit(7); } }"#;
    let mut child = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(["repl", "--exact", "--eval", source])
        .env("ANUBIS_RUN_TIMEOUT_SECS", timeout)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn exact REPL");
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin
        .write_all(b"sentinel-from-parent\n")
        .expect("provide parent input");
    drop(stdin);

    child.wait_with_output().expect("wait for exact REPL")
}

#[test]
fn exact_repl_unbounded_child_keeps_prior_stdin_eof() {
    let output = exact_repl_with_parent_input("0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stdout={stdout} stderr={stderr}");
}

#[test]
fn exact_repl_timed_child_inherits_parent_stdin() {
    let output = exact_repl_with_parent_input("5");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stdout={stdout} stderr={stderr}");
    assert!(stderr.contains("exact run failed"), "stderr={stderr}");
}
