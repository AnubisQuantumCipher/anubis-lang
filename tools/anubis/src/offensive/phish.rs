//! Social-engineering / phishing campaign planner — PLAN_ONLY (elite red teams plan; humans approve sends).

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::json;
use std::fs;
use std::path::Path;

/// Emit a structured phishing campaign plan. Never sends email or opens SMTP.
pub fn phish_plan(
    eng: &Engagement,
    engage_dir: &Path,
    target_role: &str,
    theme: &str,
) -> Result<serde_json::Value> {
    eng.validate_live()?;
    let theme = if theme.is_empty() {
        "password_reset"
    } else {
        theme
    };
    let role = if target_role.is_empty() {
        "user"
    } else {
        target_role
    };

    let plan = json!({
        "status": "PLAN_ONLY",
        "executed": false,
        "module": "phish_plan",
        "engagement_id": eng.engagement_id,
        "authorization": eng.authorization,
        "attck": ["T1566", "T1566.001", "T1566.002"],
        "target_role": role,
        "theme": theme,
        "pretexts": pretexts(theme),
        "channels": ["email", "sms_sim", "voice_vishing_script"],
        "required_approvals": [
            "ROE written authorization",
            "target population in-scope",
            "legal / HR if employees",
            "no production credential harvesting without dual control"
        ],
        "landing_page_lab_only": {
            "note": "Host only on lab loopback under engagement loot — never public",
            "suggested_path": "loot/phish/landing.html",
        },
        "metrics": [
            "click_rate",
            "credential_submit_rate_lab",
            "report_to_security_rate",
            "time_to_report"
        ],
        "opsec": [
            "Do not use real brand domains without written permission",
            "No SMS to real numbers outside ROE",
            "All artifacts under engagement dir + receipts"
        ],
        "next_aop": [
            "Write lab landing HTML under loot/phish (manual)",
            "Purple-report after tabletop",
            "Train users from gaps"
        ],
        "note": "Elite red teams plan phishing meticulously; AOP will not auto-send mail.",
    });

    let dir = engage_dir.join("loot/phish");
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("campaign_plan.json"),
        serde_json::to_string_pretty(&plan)?,
    )?;
    // Minimal lab landing stub (not a live phishing kit)
    fs::write(
        dir.join("landing_lab_stub.html"),
        format!(
            r#"<!doctype html><meta charset=utf-8>
<title>AOP LAB ONLY — {theme}</title>
<body style="font-family:system-ui;max-width:40rem;margin:2rem auto">
<h1>LAB PHISH LANDING (NON-OPERATIONAL)</h1>
<p>Engagement <code>{}</code> — theme <b>{theme}</b> — role <b>{role}</b>.</p>
<p>This page is a <strong>stub for purple-team tabletop</strong>. It does not collect credentials.</p>
</body>"#,
            eng.engagement_id
        ),
    )?;
    Ok(plan)
}

fn pretexts(theme: &str) -> Vec<&'static str> {
    match theme {
        "password_reset" => vec![
            "Mandatory password rotation notice",
            "SSO session expired — re-auth",
            "IT helpdesk ticket follow-up",
        ],
        "invoice" => vec![
            "Unpaid invoice PDF link",
            "Vendor portal document share",
            "Finance approval request",
        ],
        "shipping" => vec!["Package delivery exception", "Customs hold on shipment"],
        _ => vec![
            "Generic security awareness test lure",
            "Shared document notification",
            "Calendar invite update",
        ],
    }
}
