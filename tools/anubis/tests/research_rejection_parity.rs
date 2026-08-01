use std::path::PathBuf;
use std::process::{Command, Output};

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anubis-run-research-rejection-parity-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(args)
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn run_preserves_check_rejection_before_the_research_consent_boundary() {
    let root = temp_dir();
    let source = root.join("missing_auth.anb");
    std::fs::write(
        &source,
        r#"
@research
fn main() {
    print("never executed");
}
"#,
    )
    .unwrap();

    let path = source.to_str().unwrap();
    let check_out = root.join("check");
    let checked = run(&["check", path, "--out", check_out.to_str().unwrap()]);
    assert!(!checked.status.success());
    assert!(text(&checked).contains("ANUBIS_RESEARCH_MISSING_AUTHORIZATION"));

    let run_out = root.join("run");
    let executed = run(&["run", path, "--out", run_out.to_str().unwrap()]);
    assert!(!executed.status.success());
    let run_output = text(&executed);
    assert!(
        run_output.contains("ANUBIS_RESEARCH_MISSING_AUTHORIZATION"),
        "run must preserve check's policy rejection before asking for consent: {run_output}"
    );
    assert!(!run_output.contains("ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW"));

    let _ = std::fs::remove_dir_all(&root);
}
