//! Engagement reporting module — executive, technical, and compliance reports.
//!
//! Generates structured reports from engagement receipts and module outputs.
//! All reports are engagement-scoped and receipt-chain verified.

use super::engagement::Engagement;
use super::receipts;
use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Generate an executive summary report.
///
/// High-level findings for non-technical stakeholders.
/// Risk ratings, business impact, and remediation priorities.
pub fn executive_summary(
    eng: &Engagement,
    engage_dir: &Path,
    findings: &[Value],
) -> Result<Value> {
    eng.validate_live()?;

    let critical = findings.iter()
        .filter(|f| f["severity"].as_str() == Some("critical"))
        .count();
    let high = findings.iter()
        .filter(|f| f["severity"].as_str() == Some("high"))
        .count();
    let medium = findings.iter()
        .filter(|f| f["severity"].as_str() == Some("medium"))
        .count();
    let low = findings.iter()
        .filter(|f| f["severity"].as_str() == Some("low"))
        .count();
    let info = findings.iter()
        .filter(|f| f["severity"].as_str() == Some("info"))
        .count();

    let overall_risk = if critical > 0 {
        "CRITICAL"
    } else if high > 0 {
        "HIGH"
    } else if medium > 0 {
        "MEDIUM"
    } else {
        "LOW"
    };

    let mut attck_coverage: Vec<String> = Vec::new();
    for f in findings {
        if let Some(techniques) = f["attck"].as_array() {
            for t in techniques {
                if let Some(s) = t.as_str() {
                    if !attck_coverage.contains(&s.to_string()) {
                        attck_coverage.push(s.to_string());
                    }
                }
            }
        }
    }
    attck_coverage.sort();

    let receipt_integrity = receipts::verify_chain(engage_dir);
    let receipt_status = match &receipt_integrity {
        Ok(_) => "VERIFIED",
        Err(_) => "FAILED",
    };

    let report = json!({
        "schema": "aop-report-v1",
        "module": "executive_summary",
        "engagement_id": eng.engagement_id,
        "overall_risk": overall_risk,
        "findings_summary": {
            "total": findings.len(),
            "critical": critical,
            "high": high,
            "medium": medium,
            "low": low,
            "info": info,
        },
        "attck_techniques_covered": attck_coverage.len(),
        "attck_techniques": attck_coverage,
        "receipt_chain": receipt_status,
        "remediation_priorities": build_remediation_priorities(findings),
        "executed": true,
    });

    let report_path = engage_dir.join("reports/executive_summary.json");
    if let Some(parent) = report_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let report_str = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, &report_str)?;

    Ok(report)
}

/// Generate a technical report with full finding details.
pub fn technical_report(
    eng: &Engagement,
    engage_dir: &Path,
    findings: &[Value],
) -> Result<Value> {
    eng.validate_live()?;

    let receipt_integrity = receipts::verify_chain(engage_dir);
    let receipt_status = match &receipt_integrity {
        Ok(_) => "VERIFIED",
        Err(_) => "FAILED",
    };

    let mut categorized: std::collections::BTreeMap<String, Vec<&Value>> =
        std::collections::BTreeMap::new();
    for f in findings {
        let cat = f["category"].as_str().unwrap_or("uncategorized").to_string();
        categorized.entry(cat).or_default().push(f);
    }

    let sections: Vec<Value> = categorized
        .iter()
        .map(|(cat, items)| {
            json!({
                "category": cat,
                "finding_count": items.len(),
                "findings": items,
            })
        })
        .collect();

    let report = json!({
        "schema": "aop-report-v1",
        "module": "technical_report",
        "engagement_id": eng.engagement_id,
        "receipt_chain": receipt_status,
        "total_findings": findings.len(),
        "sections": sections,
        "attck_coverage": collect_attck_ids(findings),
        "executed": true,
    });

    let report_path = engage_dir.join("reports/technical_report.json");
    if let Some(parent) = report_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let report_str = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, &report_str)?;

    Ok(report)
}

/// Generate ATT&CK coverage matrix report.
pub fn attck_coverage_report(
    eng: &Engagement,
    findings: &[Value],
) -> Result<Value> {
    eng.validate_live()?;

    let tactics = [
        ("TA0043", "Reconnaissance"),
        ("TA0042", "Resource Development"),
        ("TA0001", "Initial Access"),
        ("TA0002", "Execution"),
        ("TA0003", "Persistence"),
        ("TA0004", "Privilege Escalation"),
        ("TA0005", "Defense Evasion"),
        ("TA0006", "Credential Access"),
        ("TA0007", "Discovery"),
        ("TA0008", "Lateral Movement"),
        ("TA0009", "Collection"),
        ("TA0010", "Exfiltration"),
        ("TA0011", "Command and Control"),
        ("TA0040", "Impact"),
    ];

    let technique_to_tactic = [
        ("T1595", "TA0043"), ("T1592", "TA0043"),
        ("T1583", "TA0042"), ("T1587", "TA0042"),
        ("T1566", "TA0001"), ("T1189", "TA0001"), ("T1195", "TA0001"),
        ("T1059", "TA0002"), ("T1203", "TA0002"),
        ("T1543", "TA0003"), ("T1546", "TA0003"), ("T1547", "TA0003"), ("T1053", "TA0003"),
        ("T1548", "TA0004"), ("T1068", "TA0004"), ("T1574", "TA0004"),
        ("T1055", "TA0005"), ("T1070", "TA0005"), ("T1518", "TA0005"),
        ("T1553", "TA0005"), ("T1562", "TA0005"), ("T1027", "TA0005"),
        ("T1110", "TA0006"), ("T1552", "TA0006"), ("T1555", "TA0006"),
        ("T1082", "TA0007"), ("T1083", "TA0007"), ("T1046", "TA0007"),
        ("T1016", "TA0007"), ("T1049", "TA0007"), ("T1057", "TA0007"),
        ("T1033", "TA0007"), ("T1018", "TA0007"), ("T1087", "TA0007"),
        ("T1069", "TA0007"), ("T1005", "TA0007"),
        ("T1021", "TA0008"),
        ("T1115", "TA0009"), ("T1074", "TA0009"), ("T1113", "TA0009"),
        ("T1056", "TA0009"), ("T1560", "TA0009"),
        ("T1048", "TA0010"), ("T1572", "TA0010"),
        ("T1071", "TA0011"), ("T1090", "TA0011"), ("T1219", "TA0011"),
    ];

    let observed_ids = collect_attck_ids(findings);
    let mut tactic_coverage: Vec<Value> = Vec::new();

    for (tactic_id, tactic_name) in &tactics {
        let techniques_in_tactic: Vec<&str> = technique_to_tactic
            .iter()
            .filter(|(_, t)| t == tactic_id)
            .map(|(tech, _)| *tech)
            .collect();

        let covered: Vec<&str> = techniques_in_tactic
            .iter()
            .filter(|t| observed_ids.iter().any(|o| o.starts_with(**t)))
            .copied()
            .collect();

        tactic_coverage.push(json!({
            "tactic_id": tactic_id,
            "tactic_name": tactic_name,
            "techniques_possible": techniques_in_tactic.len(),
            "techniques_tested": covered.len(),
            "coverage_pct": if techniques_in_tactic.is_empty() { 0.0 }
                else { covered.len() as f64 / techniques_in_tactic.len() as f64 * 100.0 },
            "covered": covered,
        }));
    }

    let total_possible: usize = tactic_coverage.iter()
        .map(|t| t["techniques_possible"].as_u64().unwrap_or(0) as usize)
        .sum();
    let total_covered: usize = tactic_coverage.iter()
        .map(|t| t["techniques_tested"].as_u64().unwrap_or(0) as usize)
        .sum();

    Ok(json!({
        "schema": "aop-report-v1",
        "module": "attck_coverage_report",
        "engagement_id": eng.engagement_id,
        "total_techniques_possible": total_possible,
        "total_techniques_tested": total_covered,
        "overall_coverage_pct": if total_possible == 0 { 0.0 }
            else { total_covered as f64 / total_possible as f64 * 100.0 },
        "tactic_coverage": tactic_coverage,
        "observed_technique_ids": observed_ids,
        "attck": observed_ids,
        "executed": true,
    }))
}

/// Generate Markdown report from JSON findings.
pub fn markdown_report(
    eng: &Engagement,
    engage_dir: &Path,
    title: &str,
    findings: &[Value],
) -> Result<Value> {
    eng.validate_live()?;

    let mut md = String::new();
    md.push_str(&format!("# {title}\n\n"));
    md.push_str(&format!("**Engagement:** {}\n\n", eng.engagement_id));

    let critical = findings.iter().filter(|f| f["severity"].as_str() == Some("critical")).count();
    let high = findings.iter().filter(|f| f["severity"].as_str() == Some("high")).count();
    let medium = findings.iter().filter(|f| f["severity"].as_str() == Some("medium")).count();
    let low = findings.iter().filter(|f| f["severity"].as_str() == Some("low")).count();

    md.push_str("## Summary\n\n");
    md.push_str(&format!("| Severity | Count |\n|---|---|\n"));
    md.push_str(&format!("| Critical | {critical} |\n"));
    md.push_str(&format!("| High | {high} |\n"));
    md.push_str(&format!("| Medium | {medium} |\n"));
    md.push_str(&format!("| Low | {low} |\n\n"));

    md.push_str("## Findings\n\n");
    for (i, f) in findings.iter().enumerate() {
        let title = f["title"].as_str().unwrap_or("Untitled");
        let severity = f["severity"].as_str().unwrap_or("info");
        let description = f["description"].as_str().unwrap_or("");
        md.push_str(&format!("### {}.  [{}] {}\n\n", i + 1, severity.to_uppercase(), title));
        if !description.is_empty() {
            md.push_str(&format!("{description}\n\n"));
        }
    }

    let report_path = engage_dir.join("reports/report.md");
    if let Some(parent) = report_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&report_path, &md)?;
    let hash = hex::encode(Sha256::digest(md.as_bytes()));

    Ok(json!({
        "schema": "aop-report-v1",
        "module": "markdown_report",
        "engagement_id": eng.engagement_id,
        "report_path": report_path.display().to_string(),
        "report_sha256": hash,
        "findings_count": findings.len(),
        "size_bytes": md.len(),
        "executed": true,
    }))
}

fn build_remediation_priorities(findings: &[Value]) -> Vec<Value> {
    let mut priorities: Vec<Value> = Vec::new();
    for f in findings {
        let severity = f["severity"].as_str().unwrap_or("info");
        if severity == "critical" || severity == "high" {
            priorities.push(json!({
                "finding": f["title"].as_str().unwrap_or("Unknown"),
                "severity": severity,
                "remediation": f["remediation"].as_str().unwrap_or("See technical report"),
            }));
        }
    }
    priorities
}

fn collect_attck_ids(findings: &[Value]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for f in findings {
        if let Some(techniques) = f["attck"].as_array() {
            for t in techniques {
                if let Some(s) = t.as_str() {
                    if !ids.contains(&s.to_string()) {
                        ids.push(s.to_string());
                    }
                }
            }
        }
    }
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    fn sample_findings() -> Vec<Value> {
        vec![
            json!({
                "title": "SUID binary abuse",
                "severity": "critical",
                "category": "privilege_escalation",
                "description": "SUID vim found — allows arbitrary command execution as root",
                "attck": ["T1548.001"],
                "remediation": "Remove SUID bit from vim",
            }),
            json!({
                "title": "World-writable PATH directory",
                "severity": "high",
                "category": "privilege_escalation",
                "description": "/usr/local/bin is world-writable",
                "attck": ["T1574.007"],
                "remediation": "chmod o-w /usr/local/bin",
            }),
            json!({
                "title": "SSH key without passphrase",
                "severity": "medium",
                "category": "credential_access",
                "description": "Unencrypted SSH private key found",
                "attck": ["T1552.004"],
                "remediation": "Encrypt SSH keys with passphrase",
            }),
        ]
    }

    #[test]
    fn executive_summary_generates() {
        let eng = Engagement::default_lab("report-test", "lab-auth");
        let dir = std::env::temp_dir().join("aop-report-test-exec");
        let _ = fs::create_dir_all(dir.join("receipts"));
        let result = executive_summary(&eng, &dir, &sample_findings()).unwrap();
        assert_eq!(result["overall_risk"], "CRITICAL");
        assert_eq!(result["findings_summary"]["total"], 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn technical_report_categorizes() {
        let eng = Engagement::default_lab("report-test", "lab-auth");
        let dir = std::env::temp_dir().join("aop-report-test-tech");
        let _ = fs::create_dir_all(dir.join("receipts"));
        let result = technical_report(&eng, &dir, &sample_findings()).unwrap();
        assert!(result["sections"].is_array());
        assert!(result["sections"].as_array().unwrap().len() >= 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn attck_coverage_produces_tactics() {
        let eng = Engagement::default_lab("report-test", "lab-auth");
        let result = attck_coverage_report(&eng, &sample_findings()).unwrap();
        assert!(result["tactic_coverage"].is_array());
        assert!(result["total_techniques_tested"].as_u64().unwrap() > 0);
    }

    #[test]
    fn markdown_report_writes_file() {
        let eng = Engagement::default_lab("report-test", "lab-auth");
        let dir = std::env::temp_dir().join("aop-report-test-md");
        let _ = fs::create_dir_all(&dir);
        let result = markdown_report(&eng, &dir, "Test Report", &sample_findings()).unwrap();
        assert!(result["report_path"].as_str().unwrap().contains("report.md"));
        assert!(result["size_bytes"].as_u64().unwrap() > 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
