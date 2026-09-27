//! RT-LAMBDA-FNNAME: a closure that names a user function (or a builtin) as a VALUE must compile and
//! run. The native lowering captured every free value-use by cloning it as a local
//! (`let f = f.clone();`), and a function is not a local, so rustc failed with E0425 while `check`
//! accepted the program.

use std::process::Command;

fn run(src: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "anubis-lamfn-{tag}-{}-{}",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("p.anb");
    std::fs::write(&prog, src).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("run")
        .arg(&prog)
        .arg("--out")
        .arg(dir.join("out"))
        .output()
        .expect("run anubis");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "run must succeed, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn numbers(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))
        .map(String::from)
        .collect()
}

#[test]
fn closure_returning_a_user_function_compiles_and_runs() {
    let out = run(
        "fn f(x) { return x + 1; }\nfn main() { let g = |v| f; let h = g(0); print(h(41)); let m0 = { \"a\": 1 }; let mm = map_values(m0, |v| f); print(len(mm)); }\n",
        "userfn",
    );
    assert_eq!(numbers(&out), vec!["42", "1"], "got stdout {out:?}");
}

#[test]
fn closure_returning_a_builtin_compiles_and_runs() {
    let out = run(
        "fn main() { let g = |v| len; print(g(0)([1, 2, 3])); }\n",
        "builtin",
    );
    assert_eq!(numbers(&out), vec!["3"], "got stdout {out:?}");
}

#[test]
fn a_local_shadowing_a_builtin_is_still_captured() {
    let out = run(
        "fn main() { let len = 5; let k = |v| len; print(k(0)); }\n",
        "shadow",
    );
    assert_eq!(numbers(&out), vec!["5"], "got stdout {out:?}");
}
