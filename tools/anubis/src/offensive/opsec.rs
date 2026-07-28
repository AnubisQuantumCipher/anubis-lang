//! OPSEC scoring for engagement configuration — elite red-team hygiene checklist.
//! Fail-closed recommendations; never silently green-lights bad tradecraft.

use super::engagement::Engagement;
use serde_json::{json, Value};

/// Score 0–100 (higher = better OPSEC for a lab engagement).
pub fn score_engagement(eng: &Engagement) -> Value {
    let mut score: i32 = 100;
    let mut findings: Vec<Value> = Vec::new();
    let mut pass: Vec<String> = Vec::new();

    // Authorization present
    if eng.authorization.trim().is_empty() {
        score -= 40;
        findings.push(finding(
            "critical",
            "NO_AUTHORIZATION",
            "Engagement lacks authorization string — refuse live ops",
        ));
    } else {
        pass.push("authorization_present".into());
    }

    // Loopback C2 default
    let bind_host = eng.c2_bind.split(':').next().unwrap_or("");
    if bind_host == "127.0.0.1" || bind_host == "::1" || bind_host == "localhost" {
        pass.push("c2_loopback".into());
    } else {
        score -= 25;
        findings.push(finding(
            "high",
            "C2_NON_LOOPBACK",
            "C2 bind is not loopback — require allow_non_loopback_bind + ROE",
        ));
    }

    if eng.network_egress {
        score -= 15;
        findings.push(finding(
            "medium",
            "NETWORK_EGRESS_ON",
            "network_egress=true expands blast radius",
        ));
    } else {
        pass.push("network_egress_off".into());
    }

    if eng.encrypt_beacons && !eng.psk_hex.is_empty() {
        pass.push("aop2_encrypted".into());
    } else {
        score -= 20;
        findings.push(finding(
            "high",
            "BEACONS_CLEARTEXT",
            "Encrypted beacons disabled or PSK missing",
        ));
    }

    if eng.mtls_ready {
        pass.push("mtls_certs_ready".into());
        if eng.mtls_listen {
            pass.push("mtls_listen_on".into());
        } else {
            score -= 5;
            findings.push(finding(
                "low",
                "MTLS_AVAILABLE_UNUSED",
                "mTLS certs exist but mtls_listen is false (HTTP default is OK for lab)",
            ));
        }
    }

    if eng.jitter_pct >= 10 && eng.jitter_pct <= 40 {
        pass.push("jitter_sane".into());
    } else if eng.jitter_pct == 0 {
        score -= 10;
        findings.push(finding(
            "medium",
            "NO_JITTER",
            "Zero jitter is a detection giveaway",
        ));
    }

    if eng.sleep_ms >= 1000 {
        pass.push("sleep_not_aggressive".into());
    } else {
        score -= 8;
        findings.push(finding(
            "medium",
            "AGGRESSIVE_SLEEP",
            "sleep_ms < 1000 looks automated",
        ));
    }

    if eng.token_auth_enabled {
        pass.push("operator_tokens".into());
    } else {
        score -= 5;
        findings.push(finding(
            "low",
            "NO_OPERATOR_TOKENS",
            "Issue operator tokens for multi-operator lab discipline",
        ));
    }

    // Kill date not far-future only if set to year 2099 lab default — note only
    if eng.kill_date.starts_with("2099") {
        score -= 3;
        findings.push(finding(
            "info",
            "KILL_DATE_LAB_DEFAULT",
            "Kill date is far-future lab default; set real ROE end date for production engagements",
        ));
    } else {
        pass.push("kill_date_set".into());
    }

    if eng.allow_live_inject {
        score -= 12;
        findings.push(finding(
            "high",
            "LIVE_INJECT_ENABLED",
            "allow_live_inject=true — ensure double-auth CLI flag discipline + VZ isolation",
        ));
    } else {
        pass.push("inject_plan_only_default".into());
    }

    score = score.clamp(0, 100);

    let grade = if score >= 90 {
        "A"
    } else if score >= 75 {
        "B"
    } else if score >= 60 {
        "C"
    } else if score >= 40 {
        "D"
    } else {
        "F"
    };

    json!({
        "schema": "aop-opsec-v1",
        "engagement_id": eng.engagement_id,
        "score": score,
        "grade": grade,
        "pass": pass,
        "findings": findings,
        "elite_checklist": elite_checklist(),
        "recommendation": if score >= 75 {
            "OPSEC acceptable for authorized lab engagement"
        } else {
            "Remediate findings before multi-operator or non-loopback ops"
        },
    })
}

fn finding(sev: &str, code: &str, msg: &str) -> Value {
    json!({ "severity": sev, "code": code, "message": msg })
}

/// Standing elite red-team OPSEC checklist (process, not just config).
pub fn elite_checklist() -> Vec<Value> {
    vec![
        json!({"id":"ROE","item":"Written ROE / authorization ID on every engagement"}),
        json!({"id":"SCOPE","item":"Explicit in-scope hosts, paths, CIDRs; fail closed outside"}),
        json!({"id":"VZ","item":"AOP C2/inject/lateral only inside Apple VZ; PoC kit gold fixture may host-lab; prefer vz exploit|fuzz for primary crash evidence"}),
        json!({"id":"RECEIPTS","item":"Hash-chained receipts for every operator action"}),
        json!({"id":"ENCRYPT","item":"Encrypted C2 (aop-2) + optional mTLS"}),
        json!({"id":"JITTER","item":"Beacon sleep + jitter to avoid fixed cadence"}),
        json!({"id":"LEAST","item":"Least privilege RBAC + operator tokens"}),
        json!({"id":"PLAN_ONLY","item":"Injection / SMB / phishing plans never auto-fire without dual auth"}),
        json!({"id":"KILL","item":"Kill date + freeze path on Abort"}),
        json!({"id":"PURPLE","item":"Map every live technique to ATT&CK for purple-team debrief"}),
        json!({"id":"NO_IMPACT","item":"No destructive Impact (ransomware-class) automation"}),
        json!({"id":"EVIDENCE","item":"Loot + report for defenders; human presses send on disclosure"}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn default_lab_scores_below_100() {
        let eng = Engagement::default_lab("opsec-test", "lab-auth");
        let result = score_engagement(&eng);
        let score = result["score"].as_i64().unwrap();
        // default_lab has kill_date=2099 (-3), token_auth_enabled=false (-5) → max 92
        assert!(score < 100, "default lab should have deductions: {score}");
        assert!(
            score > 60,
            "default lab with encryption should be decent: {score}"
        );
    }

    #[test]
    fn missing_authorization_is_critical_deduction() {
        let mut eng = Engagement::default_lab("opsec-test", "lab-auth");
        eng.authorization = String::new();
        // validate_live will reject empty auth, so score_engagement doesn't call it.
        // But score_engagement doesn't call validate_live — it checks directly.
        let result = score_engagement(&eng);
        let findings = result["findings"].as_array().unwrap();
        let has_no_auth = findings.iter().any(|f| {
            f["code"].as_str() == Some("NO_AUTHORIZATION")
                && f["severity"].as_str() == Some("critical")
        });
        assert!(has_no_auth, "missing auth must produce critical finding");
        let score = result["score"].as_i64().unwrap();
        assert!(score <= 60, "no auth should drop score hard: {score}");
    }

    #[test]
    fn non_loopback_c2_penalized() {
        let mut eng = Engagement::default_lab("opsec-test", "lab-auth");
        eng.c2_bind = "0.0.0.0:4444".into();
        let result = score_engagement(&eng);
        let findings = result["findings"].as_array().unwrap();
        let has_c2 = findings
            .iter()
            .any(|f| f["code"].as_str() == Some("C2_NON_LOOPBACK"));
        assert!(has_c2, "non-loopback C2 must produce finding");
    }

    #[test]
    fn live_inject_penalized() {
        let mut eng = Engagement::default_lab("opsec-test", "lab-auth");
        eng.allow_live_inject = true;
        let result = score_engagement(&eng);
        let findings = result["findings"].as_array().unwrap();
        let has_inject = findings
            .iter()
            .any(|f| f["code"].as_str() == Some("LIVE_INJECT_ENABLED"));
        assert!(has_inject, "allow_live_inject must produce finding");
    }

    #[test]
    fn elite_checklist_has_12_items() {
        let cl = elite_checklist();
        assert_eq!(cl.len(), 12, "elite checklist count: {}", cl.len());
        for item in &cl {
            assert!(item["id"].as_str().is_some(), "each item needs an id");
            assert!(item["item"].as_str().is_some(), "each item needs text");
        }
    }
}
