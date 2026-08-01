use std::fs;
use std::process::Command;

#[test]
fn manifestless_build_evidence_binds_only_the_requested_program() {
    let tmp = tempfile::tempdir().expect("scratch directory");
    let good = tmp.path().join("good.anb");
    let sibling = tmp.path().join("broken.anb");
    let prior = tmp.path().join("evidence-old");
    let out = tmp.path().join("build-out");

    let good_source = "fn main() uses(io.print) { print(7); }\n";
    let rejected_source =
        "fn main() uses(io.print) { let token: secret<string> = \"s\"; print(token); }\n";
    fs::write(&good, good_source).expect("write requested program");
    fs::write(&sibling, rejected_source).expect("write unrelated sibling");
    fs::create_dir(&prior).expect("create prior evidence directory");
    fs::write(prior.join("source.anubis"), rejected_source)
        .expect("write unrelated prior evidence snapshot");

    let build = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(["build", "--evidence"])
        .arg(&good)
        .args(["-o"])
        .arg(&out)
        .output()
        .expect("run build --evidence");
    assert!(
        build.status.success(),
        "requested clean program must not inherit a sibling's verdict:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let bundles: Vec<_> = fs::read_dir(&out)
        .expect("read build output")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("evidence-"))
        })
        .collect();
    assert_eq!(bundles.len(), 1, "exactly one evidence bundle expected");
    let bundle = &bundles[0];

    assert_eq!(
        fs::read(bundle.join("source.anubis")).expect("read sealed source"),
        good_source.as_bytes(),
        "sealed source must be byte-identical to the requested manifest-less program"
    );
    assert!(
        !bundle.join("source-merkle-leaves.json").exists(),
        "a manifest-less program with no imports is one source leaf"
    );

    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(bundle.join("evidence.json")).expect("read evidence manifest"),
    )
    .expect("parse evidence manifest");
    assert_eq!(evidence["verdict"], "PASS");
    assert_eq!(
        evidence["source_hash"],
        anubis_compiler::package::merkle::sha256_hex(good_source.as_bytes())
    );

    let verify = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("verify")
        .arg(bundle)
        .output()
        .expect("verify evidence bundle");
    assert!(
        verify.status.success(),
        "fresh bundle must verify:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}
