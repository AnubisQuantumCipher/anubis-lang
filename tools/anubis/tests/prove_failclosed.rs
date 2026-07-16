//! End-to-end fail-closed regression for `anubis prove` (the d67c0be / 1849038 PCA honesty invariant).
//!
//! A backend that cannot produce a fresh, verified ZK receipt (here `native`) MUST exit NONZERO and name
//! the reason, and must still write its failure evidence first — so a failed proof can never be mistaken
//! for a real one, and the gate scripts' `if ! anubis prove …` guards actually fire. This locks the
//! invariant end-to-end through the real CLI, not just at the pure `prove_exit_ok` helper.

use std::path::Path;
use std::process::Command;

#[test]
fn prove_native_backend_fails_closed_with_no_verified_receipt() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello.anb");
    assert!(example.exists(), "fixture missing: {}", example.display());
    let out = std::env::temp_dir().join(format!(
        "anubis-prove-failclosed-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    let _ = std::fs::remove_dir_all(&out);

    let output = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("prove")
        .arg(&example)
        .arg("--backend")
        .arg("native")
        .arg("--out")
        .arg(&out)
        .output()
        .expect("run anubis prove");

    // (1) Fail closed: no verified receipt ⇒ nonzero exit.
    assert!(
        !output.status.success(),
        "prove --backend native must exit NONZERO (no verified receipt) — a failed proof must not \
         report success"
    );
    // (2) The reason is named, so the failure is diagnosable, not silent.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ANUBIS_PROVE_NO_VERIFIED_RECEIPT"),
        "expected the fail-closed reason in stderr, got:\n{stderr}"
    );
    // (3) Failure evidence is written BEFORE the Err (canonical proof inputs land on disk).
    assert!(
        out.join("proof_input_canonical.json").exists(),
        "prove must write failure evidence before returning Err"
    );

    let _ = std::fs::remove_dir_all(&out);
}
