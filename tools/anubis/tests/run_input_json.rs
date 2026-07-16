//! #3 proof-input ergonomics: `anubis run --input-json` feeds the SAME format `prove --input-json`
//! takes (resolved through the identical canonicalizing path), so a program that both runs natively AND
//! proves uses ONE input surface for both commands. This test drives the real CLI: a program that reads
//! `proof_input_u32("n")` must observe the value supplied to `run --input-json`.

use std::process::Command;

#[test]
fn run_input_json_feeds_proof_inputs_to_native_run() {
    let dir = std::env::temp_dir().join(format!(
        "anubis-run-inputjson-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("echo_input.anb");
    std::fs::write(
        &prog,
        "fn main() { let n = proof_input_u32(\"n\"); print(n); return 0; }\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("run")
        .arg(&prog)
        .arg("--input-json")
        .arg(r#"{"n":5}"#)
        .arg("--out")
        .arg(dir.join("out"))
        .output()
        .expect("run anubis");

    assert!(
        output.status.success(),
        "run --input-json must succeed, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains('5'),
        "the proof input n=5 supplied via --input-json must reach the native run; got stdout {stdout:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
