//! RT-STRIDX: `p["key"]` on a struct must read the field NAMED `key`. The native runtime used to try
//! the key as a position first; a non-numeric string parses to 0, so every string key read the
//! FIRST declared field. With a `secret` first field, `print(p["pub_n"])` printed the secret, and
//! the checker (which models a string key as that field) accepted it. An integer index keeps its
//! positional meaning (`r[0]`).

use std::process::Command;

fn run(src: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "anubis-stridx-{tag}-{}-{}",
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

#[test]
fn string_key_reads_the_named_field_not_the_first() {
    let out = run(
        "struct P { a: i64, b: i64 }\nfn main() { let p = P { a: 42, b: 7 }; print(p[\"b\"]); print(p[\"a\"]); print(p[0]); }\n",
        "named",
    );
    let lines: Vec<&str> = out
        .lines()
        .filter(|l| !l.trim().is_empty() && l.trim().chars().all(|c| c.is_ascii_digit()))
        .collect();
    assert_eq!(
        lines,
        vec!["7", "42", "42"],
        "p[\"b\"] must be field b, p[\"a\"] field a, and p[0] the first field; got stdout {out:?}"
    );
}

#[test]
fn public_string_key_does_not_release_a_secret_first_field() {
    let out = run(
        "struct S { k: secret<i64>, pub_n: i64 }\nfn main() { let p = S { k: 42, pub_n: 1 }; print(p[\"pub_n\"]); }\n",
        "secret",
    );
    assert!(
        !out.contains("42"),
        "p[\"pub_n\"] must not print the secret first field; got stdout {out:?}"
    );
    assert!(out.contains('1'), "p[\"pub_n\"] must print 1; got stdout {out:?}");
}
