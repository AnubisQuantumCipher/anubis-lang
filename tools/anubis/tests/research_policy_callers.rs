use std::path::PathBuf;
use std::process::{Command, Output};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anubis-research-caller-policy-{tag}-{}-{}",
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

fn research_source() -> &'static str {
    r#"@research(authorization: "phase-1-caller-closure")
fn main() {
    print("never lowered on the host");
}
"#
}

#[test]
fn prove_research_lowering_requires_consent_and_vz_before_artifacts() {
    let root = temp_dir("prove");
    let source = root.join("research.anb");
    std::fs::write(&source, research_source()).unwrap();
    let path = source.to_str().unwrap();

    let missing_out = root.join("missing-consent");
    let missing = run(&[
        "prove",
        path,
        "--backend",
        "native",
        "--out",
        missing_out.to_str().unwrap(),
    ]);
    assert!(!missing.status.success());
    assert!(text(&missing).contains("ANUBIS_PROVE_RESEARCH_REQUIRES_ALLOW"));
    assert!(!missing_out.exists());

    let host_out = root.join("host-consent");
    let host = run(&[
        "prove",
        path,
        "--backend",
        "native",
        "--allow-research",
        "--out",
        host_out.to_str().unwrap(),
    ]);
    assert!(!host.status.success());
    assert!(text(&host).contains("ANUBIS_RESEARCH_HOST_FORBIDDEN"));
    assert!(!host_out.exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn exact_repl_research_execution_requires_consent_and_vz() {
    let source = research_source();

    let missing = run(&["repl", "--exact", "--eval", source]);
    assert!(!missing.status.success());
    assert!(text(&missing).contains("ANUBIS_REPL_RESEARCH_REQUIRES_ALLOW"));

    let host = run(&["repl", "--exact", "--allow-research", "--eval", source]);
    assert!(!host.status.success());
    assert!(text(&host).contains("ANUBIS_RESEARCH_HOST_FORBIDDEN"));
}
