//! `anubis check --message-format=json` — the machine-readable verdict.
//!
//! These drive the real CLI rather than the renderer, because every property
//! worth locking here is about what reaches a consumer's stdout: that the
//! stream parses without stripping banners, that the epistemic class survives
//! the trip, and above all that a failing check never reports `pass`.
//!
//! That last one is the whole risk of adding this flag. A consumer reads the
//! summary line to decide whether a program was accepted. If a refusal the
//! solver lane did not produce — a parse error, an undeclared effect — came
//! back as an empty finding list, the format would report acceptance for a
//! program the compiler refused, and it would do it in the machine-readable
//! channel where nobody is reading the prose.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anubis-msgfmt-{}-{}-{}",
        tag,
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(tag: &str, source: &str, extra: &[&str]) -> Output {
    let dir = workdir(tag);
    let prog = dir.join(format!("{tag}.anb"));
    std::fs::write(&prog, source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_anubis"));
    cmd.arg("check")
        .arg(&prog)
        .arg("--out")
        .arg(dir.join("out"));
    for a in extra {
        cmd.arg(a);
    }
    cmd.output().expect("run anubis check")
}

/// Every line of stdout, parsed. Panics with the offending line, because a
/// stream that does not parse is the failure this format exists to prevent.
fn lines_of(out: &Output) -> Vec<Value> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {l:?}"))
        })
        .collect()
}

fn summary(out: &Output) -> Value {
    lines_of(out).pop().expect("a stream always ends in a summary")
}

const DISPROVED: &str = "fn sub(a: i64, b: i64) -> i64\n\
     requires(a >= 0)\n\
     requires(b >= 0)\n\
     ensures(result >= 0)\n\
     { return a - b; }\n\
     fn main() { let r = sub(1, 3); }\n";

const PROVABLE: &str = "fn add(a: i64, b: i64) -> i64\n\
     requires(a >= 0)\n\
     requires(b >= 0)\n\
     requires(a <= 1000)\n\
     requires(b <= 1000)\n\
     ensures(result >= 0)\n\
     { return a + b; }\n\
     fn main() { let r = add(1, 3); }\n";

#[test]
fn a_disproved_postcondition_is_reported_with_its_counterexample() {
    let out = check("disproved", DISPROVED, &["--message-format=json"]);
    assert!(!out.status.success(), "a disproved contract must not exit 0");

    let lines = lines_of(&out);
    assert_eq!(lines.len(), 2, "one diagnostic, one summary: {lines:?}");

    let d = &lines[0];
    assert_eq!(d["$type"], "anubis.diagnostic");
    assert_eq!(d["schema"], "anubis-diagnostics/1");
    assert_eq!(d["code"], "ANUBIS_ASSERTION_DISPROVED");
    assert_eq!(d["status"], "disproved");
    assert_eq!(d["defect_locus"], "program");
    assert_eq!(d["agent_action"], "repair_program");
    assert_eq!(d["build_blocking"], true);

    // The asset no comparable toolchain ships: the concrete input that breaks it.
    let ce = &d["counterexample"];
    assert_eq!(ce["trustworthy"], true, "this model was replayed");
    assert_eq!(ce["model_completeness"], "complete");
    let vars: Vec<&str> = ce["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["var"].as_str().unwrap())
        .collect();
    assert_eq!(vars, vec!["a", "b"], "source names, not the SMT mangling");
    let decimals: Vec<&str> = ce["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["decimal"].as_str().unwrap())
        .collect();
    assert_eq!(decimals, vec!["1", "3"]);

    assert_eq!(summary(&out)["verdict"], "fail");
    assert_eq!(summary(&out)["counts"]["disproved"], 1);
}

#[test]
fn a_provable_program_is_a_pass_with_an_empty_finding_set() {
    let out = check("provable", PROVABLE, &["--message-format=json"]);
    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines = lines_of(&out);
    assert_eq!(lines.len(), 1, "a pass is the summary alone: {lines:?}");
    assert_eq!(lines[0]["verdict"], "pass");
    assert_eq!(lines[0]["counts"]["total"], 0);
}

#[test]
fn stdout_carries_the_stream_and_nothing_else() {
    // A consumer must be able to pipe stdout straight into a JSON reader. If a
    // banner shares the channel, every consumer grows a stripping heuristic,
    // and heuristics are where "no findings" and "could not parse" merge.
    let out = check("purity", PROVABLE, &["--message-format=json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("anubis check"),
        "banner leaked into the stream: {stdout}"
    );
    assert!(
        !stdout.contains("check passed"),
        "human verdict leaked into the stream: {stdout}"
    );
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        serde_json::from_str::<Value>(line).unwrap_or_else(|e| panic!("{e}: {line:?}"));
    }
}

#[test]
fn a_refusal_the_solver_never_saw_is_still_a_fail() {
    // The failure this test exists for: an undeclared effect is refused before
    // any obligation is formed, so the solver lane has nothing to report. A
    // stream that therefore said `pass` would tell an agent the compiler
    // accepted a program it refused.
    let out = check(
        "effect",
        "fn main() { write_file(\"/tmp/anubis_msgfmt_probe.txt\", \"x\"); }\n",
        &["--message-format=json"],
    );
    assert!(!out.status.success(), "an undeclared effect must not exit 0");
    let lines = lines_of(&out);
    let d = &lines[0];
    assert_eq!(d["family"], "frontend");
    assert_eq!(d["code"], "ANUBIS_EFFECT_FORBIDDEN_IN_MODE");
    assert_eq!(d["build_blocking"], true);
    assert!(
        d.get("budget").is_none(),
        "no solver ran, so no budget may be implied: {d}"
    );
    assert_eq!(summary(&out)["verdict"], "fail");
}

#[test]
fn a_parse_error_is_reported_per_error_with_a_location() {
    let out = check("parse", "fn main() {\n    let x = ;\n", &["--message-format=json"]);
    assert!(!out.status.success());
    let lines = lines_of(&out);
    let d = &lines[0];
    assert_eq!(d["code"], "ANUBIS_PARSE_ERROR");
    let loc = &d["location"];
    assert!(
        loc["file"].as_str().unwrap().ends_with("parse.anb"),
        "location names the file: {loc}"
    );
    assert!(loc["line"].as_u64().unwrap() >= 1, "1-based line: {loc}");
    assert!(loc["column"].as_u64().unwrap() >= 1, "1-based column: {loc}");
    assert_eq!(summary(&out)["verdict"], "fail");
}

/// One contract in the proven fragment, one on `bvsdiv` which is not. The
/// division obligation is the REG-002 residual: no machine-checked bit-blast
/// exists for it, so the native lane declines and z3 decides alone.
const MIXED: &str = "fn half(a: i64, b: i64) -> i64\n\
     requires(a >= 0)\n\
     requires(a <= 1000)\n\
     requires(b > 0)\n\
     ensures(result >= 0)\n\
     { return a / b; }\n\
     fn add(a: i64, b: i64) -> i64\n\
     requires(a >= 0)\n\
     requires(b >= 0)\n\
     requires(a <= 1000)\n\
     requires(b <= 1000)\n\
     ensures(result >= 0)\n\
     { return a + b; }\n\
     fn main() { let x = add(1, 2); let y = half(10, 2); }\n";

#[test]
fn a_pass_states_how_much_of_itself_rests_on_the_solvers_word() {
    // The point of the whole format. Without this a `pass` says the same thing
    // whether every obligation carried a re-checkable refutation or none did.
    let out = check("coverage", MIXED, &["--message-format=json"]);
    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = summary(&out);
    assert_eq!(s["verdict"], "pass");

    let cov = &s["coverage"];
    let certified = cov["certified"].as_u64().unwrap();
    let trusted = cov["trusted_to_solver"].as_u64().unwrap();
    let discharged = cov["discharged"].as_u64().unwrap();
    assert_eq!(certified + trusted, discharged, "the parts must sum");
    assert!(discharged > 0, "this program discharges obligations");
    assert_eq!(trusted, 1, "exactly the division obligation: {cov}");

    // The admission is specific, not a bare count.
    let uncertified = cov["uncertified"].as_array().unwrap();
    assert_eq!(uncertified.len(), 1);
    assert!(
        uncertified[0].as_str().unwrap().contains("bvsdiv"),
        "names the obligation with no witness: {cov}"
    );
}

#[test]
fn a_program_with_nothing_to_prove_reports_no_coverage_at_all() {
    // Not `0/0`. That would read as "nothing was witnessed" rather than
    // "nothing was attempted", and a consumer would be right to be alarmed.
    let out = check("nocov", "fn main() { }\n", &["--message-format=json"]);
    assert!(out.status.success());
    let s = summary(&out);
    assert_eq!(s["verdict"], "pass");
    assert!(
        s.get("coverage").is_none(),
        "coverage must be absent, not zero: {s}"
    );
}

#[test]
fn the_human_verdict_states_coverage_and_names_the_residual() {
    let out = check("covhuman", MIXED, &[]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("check passed"), "{stdout}");
    assert!(
        stdout.contains("re-checkable witness"),
        "the verdict must state coverage: {stdout}"
    );
    assert!(
        stdout.contains("bvsdiv"),
        "and name what has none: {stdout}"
    );
    // It must not dress the ratio up as end-to-end verification: the chain from
    // CNF back to source is still the compiler's word.
    let lower = stdout.to_lowercase();
    for overclaim in ["fully verified", "proven correct", "guaranteed"] {
        assert!(!lower.contains(overclaim), "overclaim in verdict: {stdout}");
    }
}

#[test]
fn an_unknown_format_is_refused_rather_than_silently_treated_as_human() {
    // Defaulting here would hand a consumer that typed `jsonl` an empty stdout,
    // which reads exactly like a clean run to anything counting findings.
    let out = check("badfmt", PROVABLE, &["--message-format=jsonl"]);
    assert!(!out.status.success(), "an unknown format must not exit 0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ANUBIS_MESSAGE_FORMAT_UNKNOWN"),
        "the refusal must name itself: {stderr}"
    );
    assert!(
        out.stdout.is_empty(),
        "a refused format emits no stream at all"
    );
}

#[test]
fn human_output_is_unchanged_when_the_flag_is_absent() {
    let out = check("human", PROVABLE, &[]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("anubis check"), "banner: {stdout}");
    assert!(stdout.contains("check passed"), "verdict: {stdout}");
    assert!(
        !stdout.contains("anubis-diagnostics/1"),
        "the stream must not appear unless asked for: {stdout}"
    );
}

#[test]
fn no_line_of_the_stream_advertises_a_way_around_the_refusal() {
    // Locked at the CLI, not just the renderer: an agent reads this stream to
    // decide what to do next, and a bypass named here is one it will take.
    let out = check("nobypass", DISPROVED, &["--message-format=json"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
    for forbidden in [
        "--no-verify",
        "no_verify",
        "suppress",
        "confidence",
        "skip verification",
        "ignore this",
    ] {
        assert!(
            !stdout.contains(forbidden),
            "stream named {forbidden:?}: {stdout}"
        );
    }
    // And nothing claims a fix it never re-proved.
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(s) = v.get("suggestions") {
                assert_eq!(
                    s.as_array().map(|a| a.len()),
                    Some(0),
                    "no suggestion ships before the re-proof gate exists"
                );
            }
        }
    }
}
