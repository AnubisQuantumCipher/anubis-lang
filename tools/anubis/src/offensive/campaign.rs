//! Multi-phase campaign playbooks — how elite red teams structure an engagement.

use super::attck::{self, Tactic};
use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignPhase {
    pub name: String,
    pub tactic: String,
    pub tactic_id: String,
    pub objectives: Vec<String>,
    pub aop_commands: Vec<String>,
    pub success_criteria: Vec<String>,
    /// safe | plan_only | live_scoped | research_vz
    pub risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignPlaybook {
    pub schema_version: String,
    pub name: String,
    pub engagement_id: String,
    pub authorization: String,
    pub phases: Vec<CampaignPhase>,
    pub purple_team_debrief: Vec<String>,
    pub isolation_policy: String,
}

/// Standard full-spectrum lab playbook (authorized only).
pub fn default_playbook(eng: &Engagement) -> CampaignPlaybook {
    CampaignPlaybook {
        schema_version: "1.0".into(),
        name: format!("{}-full-spectrum", eng.name),
        engagement_id: eng.engagement_id.clone(),
        authorization: eng.authorization.clone(),
        isolation_policy: "ALL red-team/offensive execution MUST run inside Apple Virtualization (tart anubis-xcode + ANUBIS_VZ_GUEST=1). Host is control-plane only (plans, catalogs, receipt-verify of guest loot).".into(),
        phases: vec![
            phase(
                "0-Opsec & Charter",
                Tactic::ResourceDevelopment,
                vec![
                    "Seal ROE authorization".into(),
                    "OPSEC score engagement".into(),
                    "Issue operator tokens".into(),
                ],
                vec![
                    "anubis engage-status --dir <eng> --json".into(),
                    "anubis opsec-score --engage <eng>".into(),
                    "anubis operator-token-issue --engage <eng> --operator operator".into(),
                ],
                vec!["authorization non-empty".into(), "opsec grade >= B".into()],
                "safe",
            ),
            phase(
                "1-Reconnaissance",
                Tactic::Reconnaissance,
                vec![
                    "Enumerate in-scope hosts".into(),
                    "Service discovery on loopback/lab".into(),
                ],
                vec![
                    "anubis recon-hostinfo --engage <eng>".into(),
                    "anubis recon-scan --engage <eng> --host 127.0.0.1".into(),
                ],
                vec!["open_ports reported".into(), "out-of-scope host denied".into()],
                "live_scoped",
            ),
            phase(
                "2-Initial Access Planning",
                Tactic::InitialAccess,
                vec![
                    "Phishing campaign PLAN_ONLY".into(),
                    "Exploit module scaffold for lab fixture".into(),
                ],
                vec![
                    "anubis phish-plan --engage <eng> --target-role user".into(),
                    "anubis exploit-new --out <eng>/modules/lab.json --target poc_kit/bin/vuln_local".into(),
                ],
                vec!["phish status PLAN_ONLY".into(), "module JSON written".into()],
                "plan_only",
            ),
            phase(
                "3-Execution (VZ)",
                Tactic::Execution,
                vec![
                    "Crash PoC inside Apple VZ guest".into(),
                    "Mutation fuzz inside VZ".into(),
                ],
                vec![
                    "anubis vz exploit --allow-research --base anubis-xcode examples/security/poc_local_overflow.anb".into(),
                    "anubis vz fuzz --allow-research --base anubis-xcode poc_kit/bin/vuln_local".into(),
                ],
                vec!["guest evidence exported".into(), "isolation: tart-disposable-guest".into()],
                "research_vz",
            ),
            phase(
                "4-C2 & Discovery",
                Tactic::CommandAndControl,
                vec![
                    "Start multi-transport listener".into(),
                    "Deploy encrypted beacon agent".into(),
                    "Queue discovery tasks".into(),
                ],
                vec![
                    "anubis malleable-init --engage <eng>".into(),
                    "anubis listen --engage <eng>".into(),
                    "anubis agent-generate --engage <eng> --name agent0".into(),
                    "anubis task-queue --engage <eng> --module whoami --operator operator".into(),
                ],
                vec!["whoami result ok:true".into(), "receipt chain verifies".into()],
                "live_scoped",
            ),
            phase(
                "5-Persistence & Lateral (controlled)",
                Tactic::Persistence,
                vec![
                    "LaunchAgent artifact".into(),
                    "SSH lateral in-scope only".into(),
                    "SMB PLAN_ONLY".into(),
                    "Inject PLAN_ONLY or double-auth".into(),
                ],
                vec![
                    "anubis persist-launchagent --engage <eng> --agent <eng>/agents/agent0".into(),
                    "anubis lateral-ssh --engage <eng> --host 127.0.0.1 --cmd hostname".into(),
                    "anubis lateral-smb --engage <eng> --host 127.0.0.1".into(),
                    "anubis inject-plan --engage <eng> --pid 1 --shellcode <path>".into(),
                ],
                vec!["SMB executed:false".into(), "external lateral denied".into()],
                "plan_only",
            ),
            phase(
                "6-Purple Team Debrief",
                Tactic::Impact,
                vec![
                    "ATT&CK map of executed techniques".into(),
                    "Detection gap report for blue team".into(),
                    "Receipt verify + evidence package".into(),
                ],
                vec![
                    "anubis attck-catalog --json".into(),
                    "anubis purple-report --engage <eng> --out <eng>/loot/purple".into(),
                    "anubis receipt-verify --engage <eng> --json".into(),
                ],
                vec!["detection_gaps listed".into(), "receipt ok:true".into()],
                "safe",
            ),
        ],
        purple_team_debrief: vec![
            "For each executed technique ID, ask: what log/sensor should have fired?".into(),
            "Map C2 to network detections (beacon cadence, DoH, mTLS anomalies).".into(),
            "Map persistence to EDR LaunchAgent rules.".into(),
            "Map lateral SSH to identity + MFA telemetry.".into(),
            "Never treat PLAN_ONLY as executed coverage.".into(),
        ],
    }
}

fn phase(
    name: &str,
    tactic: Tactic,
    objectives: Vec<String>,
    cmds: Vec<String>,
    success: Vec<String>,
    risk: &str,
) -> CampaignPhase {
    CampaignPhase {
        name: name.into(),
        tactic: tactic.name().into(),
        tactic_id: tactic.id().into(),
        objectives,
        aop_commands: cmds,
        success_criteria: success,
        risk: risk.into(),
    }
}

pub fn write_playbook(eng: &Engagement, engage_dir: &Path) -> Result<std::path::PathBuf> {
    let pb = default_playbook(eng);
    let dir = engage_dir.join("campaigns");
    fs::create_dir_all(&dir)?;
    let path = dir.join("full_spectrum.json");
    fs::write(&path, serde_json::to_string_pretty(&pb)?)?;
    // Also markdown for operators
    let md = render_markdown(&pb);
    fs::write(dir.join("full_spectrum.md"), md)?;
    Ok(path)
}

fn render_markdown(pb: &CampaignPlaybook) -> String {
    let mut s = format!(
        "# Campaign: {}\n\nEngagement: `{}`\nAuth: {}\nIsolation: {}\n\n",
        pb.name, pb.engagement_id, pb.authorization, pb.isolation_policy
    );
    // 1-INDEXED. `enumerate()` starts at 0, so a 7-phase campaign rendered "Phase 0" through
    // "Phase 6" in an operator-facing report — off by one for every human who reads it, and for any
    // downstream reference to "phase 3" of an engagement. Caught by a unit test whose expectation
    // (phases 1..=7) was the correct one.
    for (i, ph) in pb.phases.iter().enumerate() {
        s.push_str(&format!(
            "## Phase {}: {} ({})\n\nRisk: `{}`\n\n### Objectives\n",
            i + 1,
            ph.name,
            ph.tactic_id,
            ph.risk
        ));
        for o in &ph.objectives {
            s.push_str(&format!("- {o}\n"));
        }
        s.push_str("\n### AOP commands\n```bash\n");
        for c in &ph.aop_commands {
            s.push_str(c);
            s.push('\n');
        }
        s.push_str("```\n\n### Success criteria\n");
        for c in &ph.success_criteria {
            s.push_str(&format!("- {c}\n"));
        }
        s.push('\n');
    }
    s.push_str("## Purple-team debrief\n");
    for d in &pb.purple_team_debrief {
        s.push_str(&format!("- {d}\n"));
    }
    s
}

pub fn status_json(eng: &Engagement, engage_dir: &Path) -> Result<serde_json::Value> {
    let path = engage_dir.join("campaigns/full_spectrum.json");
    if !path.exists() {
        return Err(anyhow!(
            "ANUBIS_CAMPAIGN_MISSING: run campaign-init first ({})",
            path.display()
        ));
    }
    let pb: CampaignPlaybook = serde_json::from_str(&fs::read_to_string(&path)?)?;
    Ok(json!({
        "ok": true,
        "playbook": pb.name,
        "phases": pb.phases.len(),
        "engagement_id": eng.engagement_id,
        "path": path.display().to_string(),
        "attck_coverage_hint": attck::catalog().len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn default_playbook_has_7_phases() {
        let eng = Engagement::default_lab("campaign-test", "unit test auth");
        let pb = default_playbook(&eng);
        assert_eq!(pb.phases.len(), 7, "expected 7 campaign phases");
    }

    #[test]
    fn default_playbook_phases_have_valid_tactic_ids() {
        let eng = Engagement::default_lab("campaign-test2", "unit test auth");
        let pb = default_playbook(&eng);
        for phase in &pb.phases {
            assert!(
                phase.tactic_id.starts_with("TA"),
                "bad tactic_id: {}",
                phase.tactic_id
            );
            assert!(!phase.name.is_empty());
            assert!(
                !phase.objectives.is_empty(),
                "empty objectives for {}",
                phase.name
            );
        }
    }

    #[test]
    fn render_markdown_contains_phase_headers_and_engagement_id() {
        let eng = Engagement::default_lab("campaign-md", "unit test auth");
        let pb = default_playbook(&eng);
        let md = render_markdown(&pb);
        assert!(md.contains("## Phase 1"), "missing phase 1 header");
        assert!(md.contains("## Phase 7"), "missing phase 7 header");
        assert!(
            md.contains(&eng.engagement_id),
            "missing engagement_id in markdown"
        );
    }
}
