//! A long left-associated sum nests one SMT level per term. The native solver bounds nesting
//! (`anubis_solver::parse::MAX_SEXP_DEPTH`) and runs on its own stack, so a contract over a 200-term
//! sum must still be decided NATIVELY, with every obligation discharged by a machine-checked
//! refutation — not quietly handed to z3 uncertified. (With a 128-level cap, 78 of these 202
//! obligations lost their certificate.)

use std::process::Command;

#[test]
fn two_hundred_term_sum_is_fully_certified() {
    let dir = std::env::temp_dir().join(format!(
        "anubis-deep-sum-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sum = vec!["x"; 200].join(" + ");
    std::fs::write(
        dir.join("p.anb"),
        format!(
            "fn f(x: i64) -> i64 requires(x >= 0 && x <= 10) ensures(result >= 0) {{\n  return {sum};\n}}\nfn main() {{ println(f(3)); }}\n"
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg("p.anb")
        .current_dir(&dir)
        .output()
        .expect("run anubis check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "check failed:\n{text}");
    let line = text
        .lines()
        .find(|l| l.starts_with("certificates:"))
        .unwrap_or_else(|| panic!("no certificates line:\n{text}"));
    let counts = line
        .trim_start_matches("certificates: ")
        .split_whitespace()
        .next()
        .unwrap();
    let (certified, discharged) = counts.split_once('/').unwrap();
    assert_eq!(
        certified, discharged,
        "not every obligation certified: {line}"
    );
    assert!(!line.contains("trusted to the solver"), "{line}");
    let _ = std::fs::remove_dir_all(&dir);
}
