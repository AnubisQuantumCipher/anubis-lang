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
    let dir = workdir(tag);
    let prog = dir.join(format!("{tag}.anb"));
    std::fs::write(&prog, source).unwrap();
    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg(&prog)
        .arg("--out")
        .arg(dir.join("out"))
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
