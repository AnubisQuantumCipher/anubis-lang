//! #2: `prove` composes source the same way `run` does — it resolves `import`s / shared modules through
//! the shared load_program_items front-end. A proof program that imports a sibling module must get PAST
//! resolution + typecheck + lowering (so it reaches the fail-closed exit), where before this change prove
//! read the entry file raw and the qualified call to an imported fn was unresolved.

use std::process::Command;

#[test]
fn prove_resolves_imported_modules() {
    let dir = std::env::temp_dir().join(format!(
        "anubis-prove-imports-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("lib.anb"), "pub fn triple(x) { return x * 3; }").unwrap();
    let main = dir.join("main.anb");
    std::fs::write(
        &main,
        "import lib;\nfn main() { let n = proof_input_u32(\"n\"); print(lib::triple(n)); return 0; }",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("prove")
        .arg(&main)
        .arg("--backend")
        .arg("native")
        .arg("--input-json")
        .arg(r#"{"n":4}"#)
        .arg("--out")
        .arg(dir.join("out"))
        .output()
        .expect("run prove");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The qualified call `lib::triple` RESOLVED (import combined), so prove got through typecheck +
    // lowering and reached the native fail-closed exit — not an unresolved-name / parse failure.
    assert!(
        stderr.contains("ANUBIS_PROVE_NO_VERIFIED_RECEIPT"),
        "prove must resolve the import and reach the fail-closed exit.\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("unresolved") && !stderr.contains("parse:"),
        "the import must resolve, not error:\n{stderr}"
    );
    // Lowering happened (proof of a fully-composed program), visible in stdout.
    assert!(
        stdout.contains("lowered artifact"),
        "the combined program must lower.\nstdout={stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
