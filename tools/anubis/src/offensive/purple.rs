//! Purple-team reporting — convert engagement evidence into defender-facing ATT&CK coverage + gaps.

use super::attck;
use super::engagement::Engagement;
use super::receipts;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub fn purple_report(eng: &Engagement, engage_dir: &Path, out_dir: &Path) -> Result<Value> {
    eng.validate_live()?;
    fs::create_dir_all(out_dir)?;

    // Collect action kinds from receipt chain + evidence jsonl
    let mut actions: BTreeSet<String> = BTreeSet::new();
    if let Ok(chain) = receipts::verify_chain(engage_dir) {
        if let Some(arr) = chain.get("actions").and_then(|a| a.as_array()) {
            for a in arr {
                if let Some(s) = a.as_str() {
                    actions.insert(s.to_string());
                }
            }
        }
        // tip format may only have count — also parse chain file
    }
    let chain_path = engage_dir.join("evidence/receipts/chain.jsonl");
    if chain_path.exists() {
        if let Ok(raw) = fs::read_to_string(&chain_path) {
            for line in raw.lines() {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if let Some(a) = v.get("action").and_then(|x| x.as_str()) {
                        actions.insert(a.to_string());
                    }
                }
            }
        }
    }
    let actions_path = engage_dir.join("evidence/actions.jsonl");
    if actions_path.exists() {
        if let Ok(raw) = fs::read_to_string(&actions_path) {
            for line in raw.lines() {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if let Some(k) = v.get("kind").and_then(|x| x.as_str()) {
                        actions.insert(k.to_string());
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
        "actions_observed": actions.iter().cloned().collect::<Vec<_>>(),
        "action_to_attck": action_map,
        "techniques_covered": covered.iter().cloned().collect::<Vec<_>>(),
        "covered_detail": covered_rows,
        "detection_gaps": gaps,
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
