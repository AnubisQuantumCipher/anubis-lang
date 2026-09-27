//! P-SORT-1 end-to-end: a native verdict on a query z3 REJECTS as malformed must fail closed, and the
//! evidence bundle must not publish that native refutation as a proof.
//!
//! A `z3` shim on PATH stands in for "the emitted SMT is ill-sorted" by answering with a sort error.
//! The programs are well-formed and their obligations are in the native fragment, so the native solver
//! proves them; before the fix the cross-check ignored z3's `(error …)` and the native PASS stood, and
//! `--evidence` filed a `rup_refutation` for it. The later cases are the shapes a four-lens review
//! (2026-09-23) found the first version of the guard still let through.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM: &str = "fn f(x: i64) requires(x > 0) { println(x); }\nfn main() { f(3); }\n";

/// The rejection every shim below prints (a z3 sort error), with a trailing verdict for what was left.
const SORT_ERROR: &str = "echo '(error \"line 3 column 8: Sort mismatch (simulated)\")'";

fn scratch(tag: &str, shim_body: &str, program: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anubis-z3-rejects-{tag}-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let shim = dir.join("bin").join("z3");
    std::fs::write(&shim, format!("#!/bin/sh\n{shim_body}\n")).unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("p.anb"), program).unwrap();
    dir
}

fn check(dir: &Path, evidence: bool) -> (bool, String) {
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_anubis"));
    cmd.arg("check");
    if evidence {
        cmd.arg("--evidence");
    }
    let out = cmd
        .arg("p.anb")
        .current_dir(dir)
        .env("PATH", path)
        .env("ANUBIS_NATIVE_AUTHORITATIVE", "1")
        .env_remove("ANUBIS_REQUIRE_NATIVE_PROOFS")
        .output()
        .expect("run anubis check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// The real z3 on PATH, for shims that must delegate some queries to it.
fn real_z3() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("z3"))
            .find(|z| z.is_file())
    })
}

fn assert_rejection_fails_closed(tag: &str, shim_body: &str, program: &str) {
    let dir = scratch(tag, shim_body, program);
    let (ok, text) = check(&dir, false);
    assert!(
        !ok,
        "{tag}: a query z3 rejects must not pass on the native verdict alone:\n{text}"
    );
    assert!(
        text.contains("solver rejected the emitted SMT") || text.contains("rejected the query"),
        "{tag}: the refusal must name z3's rejection:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn native_proof_of_a_query_z3_rejects_fails_closed() {
    assert_rejection_fails_closed(
        "plain",
        &format!("cat >/dev/null\n{SORT_ERROR}\necho sat"),
        PROGRAM,
    );
}

#[test]
fn evidence_does_not_publish_a_refutation_the_check_refused() {
    let dir = scratch(
        "evidence",
        &format!("cat >/dev/null\n{SORT_ERROR}\necho sat"),
        PROGRAM,
    );
    let (ok, text) = check(&dir, true);
    assert!(!ok, "check must fail closed:\n{text}");
    let bundle = std::fs::read_dir(dir.join("out"))
        .expect("evidence dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("evidence-"))
        })
        .expect("evidence bundle");
    let proofs =
        std::fs::read_to_string(bundle.join("analysis").join("proofs.json")).expect("proofs.json");
    assert!(
        !proofs.contains("\"rup_refutation\""),
        "a refused obligation was published as a refutation:\n{proofs}"
    );
    assert!(
        proofs.contains("refutation_not_accepted_by_check"),
        "the refused native refutation must be named, not omitted:\n{proofs}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejection_after_a_warning_line_fails_closed() {
    assert_rejection_fails_closed(
        "warning",
        &format!("cat >/dev/null\necho 'WARNING: something'\n{SORT_ERROR}\necho unsat"),
        PROGRAM,
    );
}

#[test]
fn rejection_mentioning_out_of_memory_is_not_a_resource_limit() {
    // z3 copies string literals and symbols into its sort errors; only the EXACT `(error "out of
    // memory")` line is a resource limit.
    assert_rejection_fails_closed(
        "oomtext",
        "cat >/dev/null\necho '(error \"line 3 column 8: unknown constant out of memory\")'\necho unsat",
        PROGRAM,
    );
}

#[test]
fn rejection_by_a_z3_that_does_not_read_a_large_query_fails_closed() {
    // A query over the 64 KiB pipe buffer: z3 rejects it and exits before reading, so writing it
    // fails with EPIPE. That is not "z3 unavailable"; its answer is still on stdout.
    let name = "v".repeat(9000);
    let reqs: String = (0..11)
        .map(|i| format!("requires({name} > {i}) "))
        .collect();
    let program =
        format!("fn f({name}: i64) {reqs}{{ println({name}); }}\nfn main() {{ f(20); }}\n");
    assert_rejection_fails_closed("noread", &format!("{SORT_ERROR}\nexit 1"), &program);
}

#[test]
fn rejection_mentioning_undecided_is_classified_as_a_rejection() {
    let dir = scratch(
        "undecided",
        "cat >/dev/null\necho '(error \"line 3 column 8: unknown constant undecided\")'\necho unsat",
        PROGRAM,
    );
    let (ok, text) = check(&dir, false);
    assert!(!ok, "must fail closed:\n{text}");
    assert!(
        !text.contains("undecided within solver budget"),
        "a rejection was reported as a budget limit because z3's message contains `undecided`:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn vacuity_query_rejected_by_z3_fails_closed_as_a_solver_alarm() {
    // Obligation queries (they end in `(get-model)`) go to the real z3; the vacuity premise query is
    // rejected. The division keeps the premise query out of the native fragment, so z3 alone answers
    // it: before, its rejection read as "unknown" and the PASS stood.
    let Some(z3) = real_z3() else {
        eprintln!("z3 not on PATH — skipping");
        return;
    };
    let shim = format!(
        "q=$(cat)\ncase \"$q\" in\n  *\"(get-model)\"*) printf '%s\\n' \"$q\" | {} \"$@\" ;;\n  *) {SORT_ERROR}; echo sat ;;\nesac",
        z3.display()
    );
    let program = "fn g(x: i64) -> i64 requires(x < 100) ensures(result > 1000) {\n  assume(x / 3 > 1000);\n  return x;\n}\nfn main() { println(g(5)); }\n";
    let dir = scratch("vacuity", &shim, program);
    let (ok, text) = check(&dir, false);
    assert!(
        !ok,
        "a vacuity query z3 rejects must not leave the PASS standing:\n{text}"
    );
    assert!(
        text.contains("solver rejected the emitted SMT"),
        "the refusal must name the rejection:\n{text}"
    );
    assert!(
        !text.contains("self-contradictory"),
        "a solver alarm was blamed on the program:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
