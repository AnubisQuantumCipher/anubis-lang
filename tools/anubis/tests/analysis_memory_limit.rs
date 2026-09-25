//! `anubis check` stays within its memory budget (`anubis_compiler::resource`).
//!
//! A 27-line valid program made one of the checker's analyses (the contract carrier's `let`
//! inlining) copy an expression that doubles with every `let` (review of the analysis limits,
//! 2026-09-24). Unbounded, the check grew until the kernel's out-of-memory killer ended it and
//! whatever shared its session. The carrier now bounds each resolution, so that program passes; the
//! budget still has to hold for whatever analysis grows next. A 1000-term sum costs the checker far
//! more than 64 MiB (its obligations grow with the square of the sum's length), so with the budget
//! set there the check must refuse with `ANUBIS_ANALYSIS_LIMIT` and exit with status 1, quickly; an
//! ordinary program under the same budget must still pass.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anubis-memlimit-{}-{}-{}",
        tag,
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(tag: &str, source: &str, budget_mib: &str) -> (Output, Duration) {
    check_with(tag, source, budget_mib, &[])
}

fn check_with(tag: &str, source: &str, budget_mib: &str, extra: &[&str]) -> (Output, Duration) {
    let dir = workdir(tag);
    let prog = dir.join(format!("{tag}.anb"));
    std::fs::write(&prog, source).unwrap();
    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg(&prog)
        .arg("--out")
        .arg(dir.join("out"))
        .args(extra)
        .env("ANUBIS_ANALYSIS_MEMORY_MIB", budget_mib)
        .output()
        .unwrap();
    (out, start.elapsed())
}

fn doubling_lets(n: usize) -> String {
    let mut s = String::from("fn main() {\n    let a = 1;\n");
    for _ in 0..n {
        s.push_str("    let a = a + a;\n");
    }
    s.push_str("    print(1);\n}\n");
    s
}

fn long_sum(n: usize) -> String {
    format!(
        "fn main() {{\n    let a = 1{};\n    print(a);\n}}\n",
        " + 1".repeat(n)
    )
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_check_past_its_memory_budget_is_refused() {
    let (out, took) = check("sum", &long_sum(1000), "64");
    let text = text(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("ANUBIS_ANALYSIS_LIMIT"), "{text}");
    assert!(!text.contains("check passed"), "{text}");
    // Reported alone: the fail-closed answers behind it are not findings (this program has no
    // secret; the refusal used to list one "past the analysis limit").
    assert!(!text.contains("past the analysis limit"), "{text}");
    assert!(!text.contains("ANUBIS_SECRET_EXFILTRATION"), "{text}");
    assert!(took < Duration::from_secs(60), "took {took:?}");
}

#[test]
fn an_ordinary_check_passes_under_the_same_budget() {
    let (out, _) = check("ordinary", &doubling_lets(4), "64");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

/// The program that used to exhaust memory: the carrier no longer doubles it.
#[test]
fn doubling_lets_no_longer_grow_the_check() {
    let (out, took) = check("doubling", &doubling_lets(24), "64");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(took < Duration::from_secs(60), "took {took:?}");
}

/// An expression chain far past the parser's bound is one diagnostic, not a stack overflow: a
/// 100,000-term sum used to abort the checker while it copied the tree.
#[test]
fn an_overlong_chain_is_a_diagnostic() {
    let (out, _) = check("chain", &long_sum(100_000), "64");
    let text = text(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("expression chain is too long"), "{text}");
}

/// A malformed file whose errors cascade is reported in bounded space: 3000 nested blocks used to
/// render every error with its whole line, needing a 1.3 GB allocation.
#[test]
fn a_cascade_of_parse_errors_is_bounded() {
    let mut body = String::from("s");
    for _ in 0..3000 {
        body = format!("{{ let t = {body}; t }}");
    }
    let src = format!("fn main() {{\n    let g = |s| {body};\n    print(g(0));\n}}");
    let (out, took) = check("cascade", &src, "64");
    let text = text(&out);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        &text[..text.len().min(2000)]
    );
    assert!(
        text.contains("more parse errors"),
        "{}",
        &text[..text.len().min(2000)]
    );
    assert!(text.len() < 2_000_000, "{} bytes of output", text.len());
    assert!(took < Duration::from_secs(60), "took {took:?}");
}

/// A finding the analysis made is still reported when a limit is reached elsewhere (review of the
/// checker limits: a secret exfiltration beside a self-calling closure was hidden behind the limit),
/// and in JSON it is its own diagnostic, the program's.
#[test]
fn a_finding_beside_a_limit_is_kept() {
    let src = "fn leak() uses(net.send) {\n    send(\"host\", 80, secret_source(\"api_key\"));\n}\n\
               fn main() uses(net.send) {\n    leak();\n    let g = |s| 0;\n    g = |s| g(s);\n    g(0);\n}\n";
    let (out, _) = check("finding", src, "100000");
    let text = text(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("ANUBIS_ANALYSIS_LIMIT"), "{text}");
    assert!(text.contains("ANUBIS_SECRET_EXFILTRATION"), "{text}");
    // A depth limit does not tell anyone to add memory.
    assert!(!text.contains("ANUBIS_ANALYSIS_MEMORY_MIB"), "{text}");
    let (json, _) = check_with("finding-json", src, "100000", &["--message-format", "json"]);
    let stdout = String::from_utf8_lossy(&json.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("anubis.diagnostic"))
        .collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert!(
        lines[0].contains("\"defect_locus\":\"capability\""),
        "{stdout}"
    );
    assert!(lines[1].contains("ANUBIS_SECRET_EXFILTRATION"), "{stdout}");
    assert!(
        lines[1].contains("\"defect_locus\":\"program\""),
        "{stdout}"
    );
}

/// The hard memory exit still ends the JSON stream with a refusal, not with nothing.
#[test]
fn the_hard_exit_reports_in_json() {
    let src = format!(
        "fn main() {{\n    let s = {};\n    print(len(s));\n}}\n",
        vec!["\"ab\""; 2000].join(" + ")
    );
    let (out, _) = check_with("hard", &src, "1", &["--message-format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("ANUBIS_ANALYSIS_LIMIT"), "{stdout}");
    assert!(stdout.contains("anubis.summary"), "{stdout}");
}
