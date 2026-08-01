//! Purple-team reporting — convert engagement evidence into defender-facing ATT&CK coverage + gaps.

use super::attck;
use super::engagement::Engagement;
use super::receipts;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::BufRead;
use std::path::Path;

pub fn purple_report(eng: &Engagement, engage_dir: &Path, out_dir: &Path) -> Result<Value> {
    eng.validate_live()?;
    fs::create_dir_all(out_dir)?;

    // Collect action kinds from a verified receipt chain only. Raw actions.jsonl remains
    // useful operator context, but it is not receipt-bound evidence and must not create
    // ATT&CK coverage claims by itself.
    let mut actions: BTreeSet<String> = BTreeSet::new();
    let receipts_status = receipts::verify_chain(engage_dir)
        .map_err(|e| anyhow!("ANUBIS_PURPLE_RECEIPTS_INVALID: {e}"))?;
    let chain_path = engage_dir.join("evidence/receipts/chain.jsonl");
    if chain_path.exists() {
        let file = fs::File::open(&chain_path)?;
        let reader = std::io::BufReader::new(file);
        for (line_no, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let receipt: receipts::ActionReceipt = serde_json::from_str(&line)
                .map_err(|e| anyhow!("ANUBIS_PURPLE_RECEIPT_PARSE: line {}: {e}", line_no + 1))?;
            actions.insert(receipt.action);
        }
    }
    let verified_actions: Vec<String> = actions.iter().cloned().collect();

    let actions_path = engage_dir.join("evidence/actions.jsonl");
    let mut unverified_actions_ignored: BTreeSet<String> = BTreeSet::new();
    if actions_path.exists() {
        if let Ok(raw) = fs::read_to_string(&actions_path) {
            for line in raw.lines() {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if let Some(k) = v.get("kind").and_then(|x| x.as_str()) {
                        if !actions.contains(k) {
                            unverified_actions_ignored.insert(k.to_string());
                        }
                    }
                }
            }
        }
    }

    // Map each action → techniques
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let mut action_map = Vec::new();
    for a in &actions {
        let m = attck::map_action(a);
        for id in &m {
            covered.insert((*id).to_string());
        }
        action_map.push(json!({"action": a, "techniques": m}));
    }

    let catalog = attck::catalog();
    let mut gaps = Vec::new();
    let mut covered_rows = Vec::new();
    for t in &catalog {
        if t.execution_mode == "not_claimed" {
            continue;
        }
        if covered.contains(&t.id) {
            covered_rows.push(json!({
                "id": t.id,
                "name": t.name,
                "tactic": t.tactic.name(),
                "aop_surface": t.aop_surface,
                "detection_question": detection_question(&t.id),
            }));
        } else if t.execution_mode == "plan_only" || t.execution_mode == "plan_only_or_double_auth"
        {
            gaps.push(json!({
                "id": t.id,
                "name": t.name,
                "tactic": t.tactic.name(),
                "gap_type": "not_executed_this_engagement",
                "note": "Plan-only or unused — do not claim detection coverage",
                "blue_recommendation": detection_question(&t.id),
            }));
        } else {
            gaps.push(json!({
                "id": t.id,
                "name": t.name,
                "tactic": t.tactic.name(),
                "gap_type": "technique_available_but_not_seen_in_receipts",
                "aop_surface": t.aop_surface,
                "blue_recommendation": detection_question(&t.id),
            }));
        }
    }

    let report = json!({
        "schema": "aop-purple-v1",
        "engagement_id": eng.engagement_id,
        "authorization": eng.authorization,
        "actions_observed": verified_actions,
        "action_to_attck": action_map,
        "techniques_covered": covered.iter().cloned().collect::<Vec<_>>(),
        "covered_detail": covered_rows,
        "detection_gaps": gaps,
        "receipts": receipts_status,
        "coverage_policy": {
            "verified_receipts_only": true,
            "unverified_actions_ignored": unverified_actions_ignored.iter().cloned().collect::<Vec<_>>(),
            "note": "ATT&CK coverage is derived only from receipt-verified actions. Raw actions.jsonl observations do not create coverage claims."
        },
        "elite_debrief": [
            "Walk each covered technique with blue: which control should have fired?",
            "PLAN_ONLY surfaces are not detection tests until dual-auth live path is used under ROE.",
            "C2: validate beacon cadence, DoH, and mTLS detections separately.",
            "Persistence: LaunchAgent plist generation ≠ install — confirm EDR covers both.",
            "Lateral: out-of-scope denials prove the guardrail; in-scope SSH needs identity telemetry.",
            "Crash PoCs belong in VZ evidence folders with isolation labels.",
        ],
        "policy": {
            "authorized_only": true,
            "no_destructive_impact": true,
            "human_presses_send_on_disclosure": true,
        },
    });

    fs::write(
        out_dir.join("purple_report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    fs::write(out_dir.join("purple_report.md"), render_md(&report))?;
    Ok(report)
}

fn detection_question(tech_id: &str) -> &'static str {
    match tech_id {
        "T1595" => "Do network IDS/NDR alert on internal port sweeps?",
        "T1566" => "Do mail gateway + user reporting catch the planned lure themes?",
        "T1203" => "Does EDR capture crash/exploit telemetry on the lab fixture process?",
        "T1543.001" => "Does EDR alert on new LaunchAgents under ~/Library/LaunchAgents?",
        "T1055" => "Does EDR detect remote thread / anomalous memory writes (when dual-auth live)?",
        "T1027" => "Does malware detonation notice XOR-packed lab blobs?",
        "T1021.004" => "Is SSH lateral visible in identity + session logs?",
        "T1021.002" => "Are SMB admin share attempts monitored (plan stage only here)?",
        "T1071" | "T1071.004" => "Is beaconing / DoH / odd HTTPS client cert use detected?",
        "T1041" => "Is C2 channel data staging volume anomalous?",
        "T1218" => "Are LOLBin parent/child chains baselined?",
        "T1082" | "T1083" | "T1059" => "Is discovery command spam from beacons visible?",
        _ => "Which sensor should observe this technique end-to-end?",
    }
}

fn render_md(report: &Value) -> String {
    let mut s = String::from("# Purple Team Report (AOP)\n\n");
    s.push_str(&format!(
        "Engagement: `{}`\n\nAuthorization: {}\n\n",
        report["engagement_id"].as_str().unwrap_or(""),
        report["authorization"].as_str().unwrap_or("")
    ));
    if let Some(note) = report["coverage_policy"]["note"].as_str() {
        s.push_str(&format!("> {}\n\n", note));
    }
    let receipt_count = report["receipts"]["count"].as_u64().unwrap_or(0);
    let receipt_ok = report["receipts"]["ok"].as_bool().unwrap_or(false);
    s.push_str(&format!(
        "Receipt verification: ok=`{}` count=`{}`\n\n",
        receipt_ok, receipt_count
    ));
    if let Some(arr) = report["coverage_policy"]["unverified_actions_ignored"].as_array() {
        if !arr.is_empty() {
            let ignored = arr
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            s.push_str(&format!(
                "Ignored unverified observations (not counted toward coverage): {}\n\n",
                ignored
            ));
        }
    }
    s.push_str("## Covered techniques\n\n");
    if let Some(arr) = report["covered_detail"].as_array() {
        for t in arr {
            s.push_str(&format!(
                "- **{}** {} — detection: {}\n",
                t["id"].as_str().unwrap_or(""),
                t["name"].as_str().unwrap_or(""),
                t["detection_question"].as_str().unwrap_or("")
            ));
        }
    }
    s.push_str("\n## Detection gaps / not executed\n\n");
    if let Some(arr) = report["detection_gaps"].as_array() {
        for t in arr.iter().take(40) {
            s.push_str(&format!(
                "- **{}** {} ({}) — {}\n",
                t["id"].as_str().unwrap_or(""),
                t["name"].as_str().unwrap_or(""),
                t["gap_type"].as_str().unwrap_or(""),
                t["blue_recommendation"].as_str().unwrap_or("")
            ));
        }
    }
    s.push_str("\n## Elite debrief\n\n");
    if let Some(arr) = report["elite_debrief"].as_array() {
        for d in arr {
            s.push_str(&format!("- {}\n", d.as_str().unwrap_or("")));
        }
    }
    s
}

/// Thin wrapper used when chain verify shape differs.
#[allow(dead_code)]
fn _ensure_out(out: &Path) -> Result<()> {
    fs::create_dir_all(out).map_err(|e| anyhow!("out: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;
    use serde_json::json;
    use std::path::PathBuf;

    fn test_dirs(suffix: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "anubis-purple-test-{}-{}",
            std::process::id(),
            suffix,
        ));
        let engage = base.join("engage");
        let out = base.join("out");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(engage.join("evidence/receipts")).unwrap();
        fs::create_dir_all(&out).unwrap();
        (engage, out)
    }

    #[test]
    fn empty_engagement_has_zero_covered_techniques() {
        let (engage, out) = test_dirs("empty");
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        let report = purple_report(&eng, &engage, &out).unwrap();
        let covered = report["techniques_covered"].as_array().unwrap();
        assert!(
            covered.is_empty(),
            "no receipts → no covered techniques, got: {:?}",
            covered
        );
        let covered_detail = report["covered_detail"].as_array().unwrap();
        assert!(covered_detail.is_empty());
    }

    #[test]
    fn exploit_action_maps_to_t1203_only() {
        let (engage, out) = test_dirs("exploit");
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        receipts::seal_action(
            &engage,
            &eng.engagement_id,
            "vz_exploit_run",
            "operator",
            json!({}),
        )
        .unwrap();
        let report = purple_report(&eng, &engage, &out).unwrap();
        let covered: Vec<&str> = report["techniques_covered"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            covered.contains(&"T1203"),
            "vz_exploit_run must map to T1203: {:?}",
            covered
        );
        assert_eq!(
            covered.len(),
            1,
            "vz_exploit_run must map to ONLY T1203, got {:?} — false coverage",
            covered
        );
    }

    #[test]
    fn c2_cycle_action_maps_to_c2_and_exfil_only() {
        let (engage, out) = test_dirs("c2cycle");
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        receipts::seal_action(
            &engage,
            &eng.engagement_id,
            "vz_c2_cycle",
            "operator",
            json!({}),
        )
        .unwrap();
        let report = purple_report(&eng, &engage, &out).unwrap();
        let covered: Vec<&str> = report["techniques_covered"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            covered.contains(&"T1071"),
            "c2 must map to T1071: {:?}",
            covered
        );
        assert!(
            covered.contains(&"T1041"),
            "c2 must map to T1041: {:?}",
            covered
        );
        assert_eq!(
            covered.len(),
            2,
            "vz_c2_cycle must map to ONLY T1071+T1041, got {:?} — false coverage",
            covered
        );
    }

    #[test]
    fn not_claimed_techniques_absent_from_report() {
        let (engage, out) = test_dirs("notclaimed");
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        let report = purple_report(&eng, &engage, &out).unwrap();
        let gaps: Vec<&str> = report["detection_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["id"].as_str())
            .collect();
        let covered: Vec<&str> = report["covered_detail"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["id"].as_str())
            .collect();
        assert!(
            !gaps.contains(&"T1486"),
            "T1486 (not_claimed) must not appear in gaps: {:?}",
            gaps
        );
        assert!(
            !gaps.contains(&"T1003"),
            "T1003 (not_claimed) must not appear in gaps: {:?}",
            gaps
        );
        assert!(
            !covered.contains(&"T1486"),
            "T1486 must not appear in covered"
        );
        assert!(
            !covered.contains(&"T1003"),
            "T1003 must not appear in covered"
        );
    }

    #[test]
    fn tampered_chain_fails_closed() {
        let (engage, out) = test_dirs("tampered");
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        receipts::seal_action(
            &engage,
            &eng.engagement_id,
            "vz_exploit_run",
            "operator",
            json!({"step": 1}),
        )
        .unwrap();
        let chain = engage.join("evidence/receipts/chain.jsonl");
        let mut rows: Vec<Value> = fs::read_to_string(&chain)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect();
        rows[0]["action"] = json!("lateral ssh");
        let tampered = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&chain, tampered + "\n").unwrap();
        let err = purple_report(&eng, &engage, &out).unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_PURPLE_RECEIPTS_INVALID"),
            "expected fail-closed invalid-receipts error, got {err}"
        );
    }

    #[test]
    fn actions_jsonl_is_ignored_without_receipt_proof() {
        let (engage, out) = test_dirs("actionsjsonl");
        fs::write(
            engage.join("evidence/actions.jsonl"),
            r#"{"kind":"recon scan","ts":1234}"#,
        )
        .unwrap();
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        let report = purple_report(&eng, &engage, &out).unwrap();
        let covered: Vec<&str> = report["techniques_covered"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        let ignored: Vec<&str> = report["coverage_policy"]["unverified_actions_ignored"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            covered.is_empty(),
            "actions.jsonl alone must not create ATT&CK coverage: {:?}",
            covered
        );
        assert!(
            ignored.contains(&"recon scan"),
            "expected raw observation to be preserved as ignored context: {:?}",
            ignored
        );
    }

    #[test]
    fn report_writes_json_and_markdown() {
        let (engage, out) = test_dirs("files");
        let eng = Engagement::default_lab("purple-test", "lab-auth");
        purple_report(&eng, &engage, &out).unwrap();
        assert!(out.join("purple_report.json").exists());
        assert!(out.join("purple_report.md").exists());
        let md = fs::read_to_string(out.join("purple_report.md")).unwrap();
        assert!(md.contains("Purple Team Report"));
        assert!(md.contains("verified_receipts_only") || md.contains("Receipt verification:"));
    }
}
