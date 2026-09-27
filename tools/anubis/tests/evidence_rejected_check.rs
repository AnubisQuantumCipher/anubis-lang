//! The CLI's actual rejected-check bundle must be verifiable as an honest FAIL, while a
//! --verified check stays outside the default PCA re-derivation until its profile is recorded.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn anubis(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(args)
        .output()
        .expect("run anubis")
}

fn bundle_in(out: &Path) -> PathBuf {
    let mut bundles = std::fs::read_dir(out)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("evidence-"))
        });
    let bundle = bundles.next().expect("check emitted a bundle");
    assert!(bundles.next().is_none(), "check emitted one bundle");
    bundle
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn safe_information_flow_refusal_has_checked_negative_evidence_and_clean_twin() {
    let root = tempfile::tempdir().unwrap();
    let unsafe_source = root.path().join("unsafe.anb");
    std::fs::write(
        &unsafe_source,
        "fn main() { let raw = taint_source(\"password\"); sink(raw); }",
    )
    .unwrap();
    let unsafe_out = root.path().join("unsafe-out");
    let checked = anubis(&[
        "check",
        unsafe_source.to_str().unwrap(),
        "--evidence",
        "--out",
        unsafe_out.to_str().unwrap(),
    ]);
    assert!(!checked.status.success(), "{}", text(&checked));
    assert!(text(&checked).contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"));
    let unsafe_bundle = bundle_in(&unsafe_out);
    let claim: serde_json::Value =
        serde_json::from_slice(&std::fs::read(unsafe_bundle.join("pca.json")).unwrap()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(unsafe_bundle.join("evidence.json")).unwrap())
            .unwrap();
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(unsafe_out.join("check-summary.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["lane"], "safe-check");
    assert_eq!(claim["typecheck_refusal_kind"], "security_policy_finding");
    assert_eq!(claim["solver_execution"], "not_run");
    assert_eq!(claim["solver_all_discharged"], false);
    assert_eq!(claim["solver_obligations"], 0);
    assert!(manifest["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["name"] == "typecheck" && row["status"] == "FAIL"));
    assert!(!manifest["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["name"] == "solver"));
    assert_eq!(summary["pca_status"], "verified_source_check_v1");
    assert_eq!(summary["bounty_ready"], false);
    let replay = anubis(&["verify", unsafe_bundle.to_str().unwrap()]);
    assert!(replay.status.success(), "{}", text(&replay));
    assert!(text(&replay).contains("recorded verdict: FAIL"));
    assert!(text(&replay).contains("solver execution: not_run"));
    assert!(text(&replay).contains("bundle valid: true"));
    let offline = anubis(&["evidence-verify", unsafe_bundle.to_str().unwrap(), "--json"]);
    assert!(offline.status.success(), "{}", text(&offline));
    let report: serde_json::Value = serde_json::from_slice(&offline.stdout).unwrap();
    let pca = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"].as_str().is_some_and(|id| id.ends_with(".pca")))
        .unwrap();
    assert_eq!(pca["claim_scope"], "source_check_v1");
    assert_eq!(pca["status"], "PASS");
    assert_eq!(pca["recorded_verdict"], "FAIL");
    assert_eq!(pca["solver_execution"], "not_run");
    assert_eq!(pca["typecheck_refusal_kind"], "security_policy_finding");
    for id in [".proofs", ".solver_replay"] {
        let row = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"].as_str().is_some_and(|name| name.ends_with(id)))
            .unwrap();
        assert_eq!(row["status"], "SKIP");
        assert_eq!(row["classification"], "NOT_RUN");
    }

    let clean_source = root.path().join("clean.anb");
    std::fs::write(
        &clean_source,
        "fn main() { let raw = taint_source(\"password\"); let clean = declassify(raw, \"release-policy\", \"audited release\"); sink(clean); }",
    )
    .unwrap();
    let clean_out = root.path().join("clean-out");
    let clean = anubis(&[
        "check",
        clean_source.to_str().unwrap(),
        "--evidence",
        "--out",
        clean_out.to_str().unwrap(),
    ]);
    assert!(clean.status.success(), "{}", text(&clean));
    let clean_bundle = bundle_in(&clean_out);
    let clean_claim: serde_json::Value =
        serde_json::from_slice(&std::fs::read(clean_bundle.join("pca.json")).unwrap()).unwrap();
    assert_eq!(clean_claim["verdict"], "PASS");
    assert_eq!(clean_claim["solver_execution"], "not_run");
    assert_eq!(clean_claim["solver_obligations"], 0);
    let clean_replay = anubis(&["verify", clean_bundle.to_str().unwrap()]);
    assert!(clean_replay.status.success(), "{}", text(&clean_replay));

    let mixed_source = root.path().join("mixed-invalid.anb");
    std::fs::write(
        &mixed_source,
        "fn main() { let raw = taint_source(\"password\"); sink(raw); missing(); }",
    )
    .unwrap();
    let mixed_out = root.path().join("mixed-invalid-out");
    let mixed = anubis(&[
        "check",
        mixed_source.to_str().unwrap(),
        "--out",
        mixed_out.to_str().unwrap(),
    ]);
    assert!(!mixed.status.success(), "{}", text(&mixed));
    let mixed_bundle = bundle_in(&mixed_out);
    let mixed_manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(mixed_bundle.join("evidence.json")).unwrap())
            .unwrap();
    assert_eq!(mixed_manifest["lane"], "safe-typecheck-refusal-check");
    let mixed_replay = anubis(&["verify", mixed_bundle.to_str().unwrap()]);
    assert!(!mixed_replay.status.success(), "{}", text(&mixed_replay));
}

#[test]
fn secret_qualifier_refusal_is_replayed_without_a_solver_and_clean_release_passes() {
    let root = tempfile::tempdir().unwrap();
    for (name, source, accepted) in [
        (
            "secret-egress",
            "fn main() uses(net.send) { let k: secret<u64> = 42; send(\"host\", 80, k); }",
            false,
        ),
        (
            "secret-declassified",
            "fn main() uses(net.send) { let k: secret<u64> = 42; send(\"host\", 80, declassify(k, \"hash-only\", \"reviewed\")); }",
            true,
        ),
    ] {
        let input = root.path().join(format!("{name}.anb"));
        let out = root.path().join(format!("{name}-out"));
        std::fs::write(&input, source).unwrap();
        let check = anubis(&[
            "check",
            input.to_str().unwrap(),
            "--evidence",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(check.status.success(), accepted, "{}", text(&check));
        if !accepted {
            assert!(text(&check).contains("ANUBIS_SECRET_EXFILTRATION"));
        }
        let bundle = bundle_in(&out);
        let claim: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("pca.json")).unwrap()).unwrap();
        assert_eq!(claim["verdict"], if accepted { "PASS" } else { "FAIL" });
        assert_eq!(claim["solver_backend"], "unobserved");
        assert_eq!(claim["solver_execution"], "not_run");
        assert_eq!(claim["solver_obligations"], 0);
        if !accepted {
            assert_eq!(claim["typecheck_refusal_kind"], "security_policy_finding");
        }
        let verified = anubis(&["verify", bundle.to_str().unwrap()]);
        assert!(verified.status.success(), "{}", text(&verified));
        assert!(text(&verified).contains("bundle valid: true"));
    }
}

#[test]
#[ignore = "Research source checks require the repository's disposable Tart/VZ guest lane"]
fn nested_and_overwritten_research_modes_cannot_claim_a_safe_check() {
    let root = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "nested-research",
            "fn main() { let x = if true { @research { let y = 1; } 1 } else { 0 }; }",
        ),
        (
            "overwritten-attribute",
            "@research(authorization: \"unit-test\") @safe fn main() { let x = 1; }",
        ),
    ] {
        let input = root.path().join(format!("{name}.anb"));
        let out = root.path().join(format!("{name}-out"));
        std::fs::write(&input, source).unwrap();
        let _check = anubis(&[
            "check",
            input.to_str().unwrap(),
            "--evidence",
            "--out",
            out.to_str().unwrap(),
        ]);
        let bundle = bundle_in(&out);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("evidence.json")).unwrap()).unwrap();
        let claim: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("pca.json")).unwrap()).unwrap();
        let summary: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("check-summary.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["lane"], "safe-source-mode-unverified-check");
        assert_eq!(claim["claim_kind"], "source_mode_unverified_v1");
        assert_eq!(summary["pca_status"], "source_mode_unverified");
        assert_eq!(summary["check_evidence_verified"], false);
        let verified = anubis(&["verify", bundle.to_str().unwrap()]);
        assert!(!verified.status.success(), "{}", text(&verified));
        assert!(text(&verified).contains("bundle valid: false"));
    }

    let source = root.path().join("ordinary-safe.anb");
    let out = root.path().join("ordinary-safe-out");
    std::fs::write(&source, "fn main() { let x = 1; assert(x == 1); }").unwrap();
    let check = anubis(&[
        "check",
        source.to_str().unwrap(),
        "--evidence",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(check.status.success(), "{}", text(&check));
    let bundle = bundle_in(&out);
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("check-summary.json")).unwrap()).unwrap();
    assert_eq!(summary["pca_status"], "verified_source_check_v1");
    let verified = anubis(&["verify", bundle.to_str().unwrap()]);
    assert!(verified.status.success(), "{}", text(&verified));
}

#[test]
fn imported_program_can_check_without_claiming_unproved_snapshot_correspondence() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("lib.anb"), "pub fn one() { return 1; }").unwrap();
    let entry = root.path().join("main.anb");
    std::fs::write(&entry, "import lib; fn main() { let x = lib::one(); }").unwrap();
    let out = root.path().join("out");
    let check = anubis(&[
        "check",
        entry.to_str().unwrap(),
        "--evidence",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(check.status.success(), "{}", text(&check));
    let bundle = bundle_in(&out);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("evidence.json")).unwrap()).unwrap();
    let claim: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("pca.json")).unwrap()).unwrap();
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("check-summary.json")).unwrap()).unwrap();
    assert_eq!(manifest["lane"], "safe-resolved-snapshot-unverified-check");
    assert_eq!(claim["claim_kind"], "source_snapshot_unverified_v1");
    assert_eq!(summary["pca_status"], "resolved_snapshot_unverified");
    assert_eq!(summary["check_evidence_verified"], false);
    let verified = anubis(&["verify", bundle.to_str().unwrap()]);
    assert!(!verified.status.success(), "{}", text(&verified));
    assert!(text(&verified).contains("bundle valid: false"));
}

#[test]
fn parse_and_import_failures_emit_typed_unverified_input_bundles() {
    let root = tempfile::tempdir().unwrap();
    for (name, source, lane, kind, status) in [
        (
            "malformed",
            "fn main( {",
            "safe-invalid-input-check",
            "invalid_input_unverified_v1",
            "invalid_input_not_pca_verified",
        ),
        (
            "missing-import",
            "import definitely_missing; fn main() { let x = 1; }",
            "safe-invalid-input-check",
            "invalid_input_unverified_v1",
            "invalid_input_not_pca_verified",
        ),
        (
            "unknown-function",
            "fn main() { definitely_missing(); }",
            "safe-typecheck-refusal-check",
            "typecheck_refusal_unverified_v1",
            "typecheck_refusal_not_pca_verified",
        ),
    ] {
        let input = root.path().join(format!("{name}.anb"));
        let out = root.path().join(format!("out-{name}"));
        std::fs::write(&input, source).unwrap();
        let check = anubis(&[
            "check",
            input.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(!check.status.success(), "{}", text(&check));
        let bundle = bundle_in(&out);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("evidence.json")).unwrap()).unwrap();
        let claim: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("pca.json")).unwrap()).unwrap();
        let summary: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("check-summary.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["lane"], lane);
        assert_eq!(claim["claim_kind"], kind);
        assert_eq!(summary["pca_status"], status);
        assert_eq!(summary["bounty_ready"], false);
        let verify = anubis(&["verify", bundle.to_str().unwrap()]);
        assert!(!verify.status.success(), "{}", text(&verify));
        assert!(text(&verify).contains("bundle valid: false"));
        let offline = anubis(&["evidence-verify", bundle.to_str().unwrap(), "--json"]);
        assert!(!offline.status.success(), "{}", text(&offline));
        let report: serde_json::Value = serde_json::from_slice(&offline.stdout).unwrap();
        let pca = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["id"].as_str().is_some_and(|id| id.ends_with(".pca")))
            .unwrap();
        assert_eq!(pca["status"], "FAIL");
        assert_eq!(pca["claim_scope"], serde_json::Value::Null);
        assert_eq!(pca["producer_reported_claim_kind"], kind);
    }
}

#[test]
fn cli_rejected_check_verifies_as_fail_but_never_validates_as_pass() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("mixed.anb");
    std::fs::write(
        &source,
        "fn main() { let x = 1; assert(x == 1); assert(x == 2); }",
    )
    .unwrap();
    let source = source.to_str().unwrap();
    let out = root.path().join("default");
    let check = anubis(&[
        "check",
        source,
        "--evidence",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(!check.status.success(), "{}", text(&check));
    assert!(text(&check).contains("ANUBIS_ASSERTION_DISPROVED"));
    let bundle = bundle_in(&out);
    let bundle_path = bundle.to_str().unwrap();
    let claim: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("pca.json")).unwrap()).unwrap();
    assert_eq!(claim["solver_execution"], "ran");
    assert!(claim["solver_obligations"]
        .as_u64()
        .is_some_and(|count| count > 0));
    let verify = anubis(&["verify", bundle_path]);
    assert!(verify.status.success(), "{}", text(&verify));
    assert!(text(&verify).contains("recorded verdict: FAIL"));
    assert!(text(&verify).contains("bundle valid: true"));
    let varied_verify = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(["verify", bundle_path])
        .env("ANUBIS_NATIVE_CONFLICT_BUDGET", "1")
        .env("ANUBIS_NATIVE_GATE_CEILING", "1")
        .env("ANUBIS_NATIVE_CLAUSE_CEILING", "1")
        .env("ANUBIS_NATIVE_CERT_WORK", "1")
        .env("ANUBIS_NATIVE_TIME_BUDGET_MS", "1")
        .env("ANUBIS_NATIVE_AUTHORITATIVE", "0")
        .env("ANUBIS_REQUIRE_NATIVE_PROOFS", "1")
        .env("ANUBIS_WRAP_SAFETY", "0")
        .output()
        .expect("verify under changed ambient policy");
    assert!(varied_verify.status.success(), "{}", text(&varied_verify));
    assert!(text(&varied_verify).contains("bundle valid: true"));

    let report = anubis(&["report", bundle_path]);
    assert!(report.status.success(), "{}", text(&report));
    assert!(text(&report).contains("command_rejection"));
    let bounty_out = root.path().join("bounty");
    let bounty = anubis(&[
        "bounty-report",
        bundle_path,
        "--out",
        bounty_out.to_str().unwrap(),
    ]);
    assert!(bounty.status.success(), "{}", text(&bounty));
    let bounty_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bounty_out.join("bounty-report.json")).unwrap())
            .unwrap();
    assert_eq!(bounty_json["verdict"], "SCOPE_UNVERIFIED");
    assert_eq!(bounty_json["authorization_status"], "not_verified_by_pca");
    assert_eq!(bounty_json["scope_status"], "not_verified_by_pca");
    let validate = anubis(&["validate", bundle_path]);
    assert!(!validate.status.success(), "{}", text(&validate));

    let verified_out = root.path().join("verified");
    let verified_check = anubis(&[
        "check",
        source,
        "--verified",
        "--evidence",
        "--out",
        verified_out.to_str().unwrap(),
    ]);
    assert!(
        !verified_check.status.success(),
        "{}",
        text(&verified_check)
    );
    let verified_bundle = bundle_in(&verified_out);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(verified_bundle.join("evidence.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["lane"], "safe-verified-check");
    let verify = anubis(&["verify", verified_bundle.to_str().unwrap()]);
    assert!(!verify.status.success(), "{}", text(&verify));
    assert!(text(&verify).contains("bundle valid: false"));

    // A successful stronger check must not advertise its PCA as bounty-ready while the
    // verifier deliberately cannot reproduce that typecheck option.
    let passing_source = root.path().join("passing.anb");
    std::fs::write(&passing_source, "fn main() { let x = 1; assert(x == 1); }").unwrap();
    let ordinary_out = root.path().join("ordinary-pass");
    let ordinary_check = anubis(&[
        "check",
        passing_source.to_str().unwrap(),
        "--evidence",
        "--out",
        ordinary_out.to_str().unwrap(),
    ]);
    assert!(ordinary_check.status.success(), "{}", text(&ordinary_check));
    let ordinary_bundle = bundle_in(&ordinary_out);
    let ordinary_summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ordinary_out.join("check-summary.json")).unwrap())
            .unwrap();
    assert_eq!(ordinary_summary["pca_status"], "verified_source_check_v1");
    assert_eq!(ordinary_summary["bounty_ready"], false);
    assert_eq!(ordinary_summary["check_evidence_verified"], true);
    assert_eq!(
        ordinary_summary["authorization_status"],
        "not_verified_by_pca"
    );
    let ordinary_verify = anubis(&["verify", ordinary_bundle.to_str().unwrap()]);
    assert!(
        ordinary_verify.status.success(),
        "{}",
        text(&ordinary_verify)
    );
    assert!(text(&ordinary_verify).contains("check sidecars independently re-derived"));
    // Rehashing a strict check into a source-only build claim cannot make `report`
    // display the original check report as independently established evidence.
    let manifest_path = ordinary_bundle.join("evidence.json");
    let mirror_path = ordinary_bundle.join("manifest.json");
    let claim_path = ordinary_bundle.join("pca.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["lane"] = serde_json::json!("safe-build");
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    std::fs::write(&manifest_path, &manifest_bytes).unwrap();
    std::fs::write(&mirror_path, &manifest_bytes).unwrap();
    let mut claim: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&claim_path).unwrap()).unwrap();
    claim["evidence_lane"] = serde_json::json!("safe-build");
    claim["claim_kind"] = serde_json::json!("source_only_v1");
    std::fs::write(&claim_path, serde_json::to_vec_pretty(&claim).unwrap()).unwrap();
    anubis_compiler::evidence::refresh_manifest_hashes(&ordinary_bundle).unwrap();
    let downgraded_report = anubis(&["report", ordinary_bundle.to_str().unwrap()]);
    assert!(
        downgraded_report.status.success(),
        "{}",
        text(&downgraded_report)
    );
    assert!(text(&downgraded_report).contains("source-analysis PCA report"));
    assert!(!text(&downgraded_report).contains("lane: safe-check"));

    let passing_out = root.path().join("verified-pass");
    let passing_check = anubis(&[
        "check",
        passing_source.to_str().unwrap(),
        "--verified",
        "--evidence",
        "--out",
        passing_out.to_str().unwrap(),
    ]);
    assert!(passing_check.status.success(), "{}", text(&passing_check));
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(passing_out.join("check-summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["verdict"], "PASS");
    assert_eq!(summary["bounty_ready"], false);
    assert_eq!(summary["pca_status"], "unsupported_verified_check");
    let passing_bundle = bundle_in(&passing_out);
    let verify = anubis(&["verify", passing_bundle.to_str().unwrap()]);
    assert!(!verify.status.success(), "{}", text(&verify));
    assert!(text(&verify).contains("bundle valid: false"));

    let nondefault_out = root.path().join("nondefault");
    let nondefault = Command::new(env!("CARGO_BIN_EXE_anubis"))
        .args(["check", source, "--evidence", "--out"])
        .arg(&nondefault_out)
        .env("ANUBIS_NATIVE_CONFLICT_BUDGET", "1")
        .output()
        .expect("run nondefault check");
    assert!(!nondefault.status.success(), "{}", text(&nondefault));
    let nondefault_bundle = bundle_in(&nondefault_out);
    let nondefault_summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(nondefault_out.join("check-summary.json")).unwrap())
            .unwrap();
    assert_eq!(
        nondefault_summary["pca_status"],
        "unsupported_nondefault_check_policy"
    );
    assert_eq!(nondefault_summary["bounty_ready"], false);
    let nondefault_verify = anubis(&["verify", nondefault_bundle.to_str().unwrap()]);
    assert!(
        !nondefault_verify.status.success(),
        "{}",
        text(&nondefault_verify)
    );

    // `report` must never print a forged bundle report without first checking PCA.
    std::fs::write(
        bundle.join("bounty-report.md"),
        "FORGED: all checks passed\n",
    )
    .unwrap();
    anubis_compiler::evidence::refresh_manifest_hashes(&bundle).unwrap();
    let forged_report = anubis(&["report", bundle_path]);
    assert!(!forged_report.status.success(), "{}", text(&forged_report));
    assert!(!text(&forged_report).contains("FORGED: all checks passed"));
}
