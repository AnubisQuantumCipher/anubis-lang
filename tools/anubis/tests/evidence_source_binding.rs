use std::fs;
use std::process::Command;

#[test]
fn resolved_snapshot_refusal_precedes_native_artifact() {
    let tmp = tempfile::tempdir().expect("scratch directory");
    fs::write(tmp.path().join("lib.anb"), "pub fn helper() { 1 }").expect("write imported module");
    let entry = tmp.path().join("main.anb");
    fs::write(
        &entry,
        "import lib;\nfn main() { hybrid { gpu(metal){} cpu{} prove(risc0){ spec { forall x . true } } } }",
    )
    .expect("write source with an unsupported resolved snapshot");
    let out = tmp.path().join("build-out");
    let build = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(["build", "--evidence"])
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("run build --evidence");
    assert!(
        !build.status.success(),
        "snapshot mismatch must refuse the build"
    );
    let stdout = String::from_utf8_lossy(&build.stdout);
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(
        stderr.contains("ANUBIS_EVIDENCE_SNAPSHOT_"),
        "missing snapshot refusal: {stderr}"
    );
    assert!(
        !stdout.contains("native artifact:"),
        "refused snapshot advertised an artifact: {stdout}"
    );
    assert!(
        !out.join("anubis_out").exists(),
        "refused snapshot left an unsealed native artifact"
    );
    let bundles: Vec<_> = fs::read_dir(&out)
        .expect("read refusal output")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("evidence-"))
        })
        .collect();
    assert_eq!(bundles.len(), 1, "expected one refusal bundle");
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(bundles[0].join("manifest.json")).expect("read refusal manifest"),
    )
    .expect("parse refusal manifest");
    assert_eq!(manifest["verdict"], "FAIL");
    assert!(manifest["checks"]
        .as_array()
        .expect("manifest checks")
        .iter()
        .any(|check| check["name"] == "command_rejection" && check["status"] == "FAIL"));
    let pca: serde_json::Value =
        serde_json::from_slice(&fs::read(bundles[0].join("pca.json")).expect("read PCA"))
            .expect("parse PCA");
    assert_eq!(pca["tier"], "rejected");
    assert_eq!(pca["verdict"], "FAIL");
    assert!(pca["rejection"]
        .as_str()
        .expect("rejection reason")
        .starts_with("ANUBIS_EVIDENCE_SNAPSHOT_"));
    assert!(!bundles[0].join("artifact").exists());
}

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

    let (secret_key, _public_key) = anubis_compiler::evidence::generate_keypair().unwrap();
    anubis_compiler::evidence::sign_pca(bundle, &secret_key).unwrap();
    for command in ["verify", "validate"] {
        let signed = Command::new(env!("CARGO_BIN_EXE_anubis"))
            .arg(command)
            .arg(bundle)
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "signed {command} failed: {}",
            String::from_utf8_lossy(&signed.stderr)
        );
    }
    let signature_path = bundle.join("pca.sig");
    let mut signature: serde_json::Value =
        serde_json::from_slice(&fs::read(&signature_path).unwrap()).unwrap();
    signature["signature"] = serde_json::Value::String("0".repeat(128));
    fs::write(
        &signature_path,
        serde_json::to_vec_pretty(&signature).unwrap(),
    )
    .unwrap();
    for command in ["verify", "validate"] {
        let invalid = Command::new(env!("CARGO_BIN_EXE_anubis"))
            .arg(command)
            .arg(bundle)
            .output()
            .unwrap();
        assert!(
            !invalid.status.success(),
            "{command} must refuse an invalid present signature"
        );
    }
}
