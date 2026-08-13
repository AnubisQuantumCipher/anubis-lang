//! REG-002 mitigation end-to-end regression (2026-08-13).
//!
//! The native authoritative solver decides only over the machine-checked bit-vector fragment (see
//! `formal/Anubis/BitBlast.lean`). Obligations outside that fragment — division, remainder,
//! nonlinear, floats, strings, quantifiers — fall through to z3 alone; a compromised z3 could
//! forge `unsat` here and no cross-check would catch it. This is the REG-002 residual named in
//! `docs/CLAIMS.md`.
//!
//! Two opt-in mitigations exposed at `compiler/src/middle/mod.rs` in `run_z3_obligation_with_smt`:
//! `ANUBIS_Z3_ONLY_LOG=<path>` writes one JSONL record per z3-only obligation, and
//! `ANUBIS_REQUIRE_NATIVE_PROOFS=1` refuses to trust z3 alone (failing closed with
//! `ANUBIS_Z3_ONLY_UNTRUSTED`). This test locks both end-to-end through the real CLI.
//!
//! Fixture: a division program (`bvsdiv` is deliberately outside the proven fragment). It passes
//! under default settings, produces exactly one audit record under `ANUBIS_Z3_ONLY_LOG`, and
//! fails closed under `ANUBIS_REQUIRE_NATIVE_PROOFS=1`.

use std::path::Path;
use std::process::Command;

const FIXTURE: &str = "../../tests/fixtures/language_core/\
                       z3_only_log_records_declined_obligation.anb";

#[test]
fn default_accepts_division_but_records_z3_only_when_log_env_is_set() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());
    let log_path = std::env::temp_dir().join(format!(
        "reg002-log-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    let _ = std::fs::remove_file(&log_path);

    let output = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg(&fixture)
        .env("ANUBIS_Z3_ONLY_LOG", &log_path)
        // Ensure native-authoritative path is on (default true) so the None-arm hook fires.
        .env("ANUBIS_NATIVE_AUTHORITATIVE", "1")
        // No require-native flag — expect PASS.
        .env_remove("ANUBIS_REQUIRE_NATIVE_PROOFS")
        .output()
        .expect("run anubis check");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "default check must PASS (native declines on bvsdiv → z3 answers unsat); \
         got exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    );

    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    let n_records = log.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        n_records >= 1,
        "expected at least one z3-only audit record from the division ensures; \
         got {n_records} lines. log:\n{log}"
    );
    // Every record must be a JSONL line naming the kind and the verdict.
    for line in log.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            line.contains("\"kind\":\"z3-only\""),
            "audit record missing `kind` tag: {line}"
        );
        assert!(
            line.contains("\"verdict\":\"unsat\"") || line.contains("\"verdict\":\"sat\""),
            "audit record missing decisive verdict: {line}"
        );
    }

    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn require_native_proofs_rejects_the_same_program_with_a_named_reason() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());

    let output = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg(&fixture)
        .env("ANUBIS_REQUIRE_NATIVE_PROOFS", "1")
        .env("ANUBIS_NATIVE_AUTHORITATIVE", "1")
        .output()
        .expect("run anubis check");

    assert!(
        !output.status.success(),
        "check must FAIL under ANUBIS_REQUIRE_NATIVE_PROOFS=1 on a z3-only obligation"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ANUBIS_Z3_ONLY_UNTRUSTED"),
        "expected the named REG-002 fail-closed reason in output; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
