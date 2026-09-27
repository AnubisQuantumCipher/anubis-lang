//! Public named proof-journal writes are egress under both their value and path condition.
//! These tests check classification only; guest receipt and journal witnesses need the
//! permitted disposable-guest lane.
use anubis_compiler::{
    frontend::{parse_source, Mode},
    middle::{ifc2_findings, typecheck_ex},
};

fn reject(source: &str, code: &str) {
    let ast = parse_source(source).expect("fixture must parse");
    let findings = ifc2_findings(&ast, Mode::Safe);
    assert!(
        findings.iter().any(|(got, _)| got == code),
        "IFC2 expected {code} for {source}: {findings:?}"
    );
    let err = typecheck_ex(ast, Mode::Safe, false)
        .expect_err("full Safe checker must reject the named journal flow");
    assert!(err.contains(code), "expected {code} for {source}: {err}");
}

fn accept(source: &str) {
    let ast = parse_source(source).expect("fixture must parse");
    let findings = ifc2_findings(&ast, Mode::Safe);
    assert!(
        findings.is_empty(),
        "IFC2 must accept {source}: {findings:?}"
    );
    let result = typecheck_ex(ast, Mode::Safe, false);
    assert!(
        result.is_ok(),
        "full Safe checker must accept {source}: {result:?}"
    );
}

#[test]
fn supported_named_commits_reject_secret_program_counters() {
    for source in [
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_secret_pc_invalid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_bool_secret_pc_invalid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_helper_secret_pc_invalid.anb"),
    ] {
        reject(source, "ANUBIS_IMPLICIT_FLOW");
    }
}

#[test]
fn named_commits_preserve_direct_data_and_integrity_refusals() {
    reject(
        include_str!(
            "../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_secret_value_invalid.anb"
        ),
        "ANUBIS_SECRET_EXFILTRATION",
    );
    reject(
        include_str!(
            "../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_tainted_value_invalid.anb"
        ),
        "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY",
    );
}

#[test]
fn named_commits_preserve_public_and_explicitly_released_programs() {
    for source in [
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_public_pc_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_bool_public_pc_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_declassified_pc_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_unconditional_public_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_constant_false_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_declassified_taint_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_user_shadow_valid.anb"),
        include_str!("../../tests/soundness/matrix/cases/ifc2_proof_journal_u32_secret_name_public_value_valid.anb"),
    ] {
        accept(source);
    }
}
