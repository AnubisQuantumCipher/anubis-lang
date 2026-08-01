//! Post-exploitation module — persistence, lateral movement prep, cleanup.
//!
//! Combines persistence mechanism enumeration (TA0003), credential harvesting
//! summaries, and engagement cleanup procedures.

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// Persistence mechanism enumeration — find and assess persistence vectors.
///
/// Scans for cron jobs, launch daemons/agents, login items, shell profiles,
/// and systemd services. Maps to TA0003.
pub fn persistence_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let mut vectors: Vec<Value> = Vec::new();

    // Shell profile persistence
    let home = std::env::var("HOME").unwrap_or_default();
    let profiles = [
        (".bashrc", "T1546.004"),
        (".bash_profile", "T1546.004"),
        (".zshrc", "T1546.004"),
        (".profile", "T1546.004"),
        (".bash_login", "T1546.004"),
    ];
    for (file, tid) in &profiles {
        let path = Path::new(&home).join(file);
        if path.exists() {
            let writable = fs::metadata(&path)
                .map(|m| m.permissions().mode() & 0o200 != 0)
                .unwrap_or(false);
            vectors.push(json!({
                "type": "shell_profile",
                "path": path.display().to_string(),
                "exists": true,
                "writable": writable,
                "attck": tid,
                "risk": if writable { "high" } else { "low" },
            }));
        }
    }

    // Launch agents (macOS persistence)
    let agent_dirs = [
        format!("{home}/Library/LaunchAgents"),
        "/Library/LaunchAgents".to_string(),
        "/Library/LaunchDaemons".to_string(),
    ];
    for dir in &agent_dirs {
        let p = Path::new(dir);
        if p.is_dir() {
            let count = fs::read_dir(p)
                .map(|rd| rd.filter_map(|e| e.ok()).count())
                .unwrap_or(0);
            let writable = fs::metadata(p)
                .map(|m| m.permissions().mode() & 0o022 != 0)
                .unwrap_or(false);
            vectors.push(json!({
                "type": "launch_agent_dir",
                "path": dir,
                "entries": count,
                "dir_writable": writable,
                "attck": "T1543.004",
                "risk": if writable { "high" } else { "info" },
            }));
        }
    }

    // Login items (macOS)
    let login_items = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of every login item",
        ])
        .output();
    if let Ok(o) = login_items {
        if o.status.success() {
            let items = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !items.is_empty() {
                vectors.push(json!({
                    "type": "login_items",
                    "items": items,
                    "attck": "T1547.015",
                }));
            }
        }
    }

    // Systemd services (Linux)
    let systemd = Command::new("systemctl")
        .args(["list-unit-files", "--type=service", "--state=enabled"])
        .output();
    if let Ok(o) = systemd {
        if o.status.success() {
            let out = String::from_utf8_lossy(&o.stdout);
            let count = out.lines().count().saturating_sub(2);
            vectors.push(json!({
                "type": "systemd_enabled",
                "enabled_services": count,
                "attck": "T1543.002",
            }));
        }
    }

    Ok(json!({
        "schema": "aop-postex-v1",
        "module": "persistence_enum",
        "engagement_id": eng.engagement_id,
        "vectors": vectors,
        "total_vectors": vectors.len(),
        "attck": ["T1546.004", "T1543.004", "T1543.002", "T1547.015"],
        "executed": true,
    }))
}

/// Persistence implant planning (PLAN_ONLY — never auto-deploys).
pub fn persistence_implant_plan(eng: &Engagement, mechanism: &str) -> Result<Value> {
    eng.validate_live()?;
    let plan = match mechanism {
        "launch_agent" => json!({
            "mechanism": "macOS LaunchAgent",
            "steps": [
                "Create plist in ~/Library/LaunchAgents/",
                "Set ProgramArguments to implant binary path",
                "Set RunAtLoad=true, KeepAlive=true",
                "launchctl load the plist",
            ],
            "detection": "LaunchAgent directory monitoring, plist analysis",
            "cleanup": "launchctl unload + rm plist + rm binary",
            "attck": "T1543.004",
        }),
        "cron" => json!({
            "mechanism": "cron job",
            "steps": [
                "crontab -e to add callback entry",
                "Schedule at regular interval (e.g., */15 * * * *)",
                "Point to beacon script or binary",
            ],
            "detection": "crontab monitoring, new cron job alerts",
            "cleanup": "crontab -e to remove entry + rm script",
            "attck": "T1053.003",
        }),
        "shell_profile" => json!({
            "mechanism": "shell profile modification",
            "steps": [
                "Append beacon command to .bashrc/.zshrc",
                "Use backgrounding (&) to avoid blocking shell startup",
                "Obfuscate with base64 encoding",
            ],
            "detection": "File integrity monitoring on profile files",
            "cleanup": "Remove appended lines from profile files",
            "attck": "T1546.004",
        }),
        "systemd" => json!({
            "mechanism": "systemd service",
            "steps": [
                "Create .service file in /etc/systemd/system/",
                "Set Type=simple, Restart=always",
                "systemctl enable + start",
            ],
            "detection": "New service unit creation, systemd journal analysis",
            "cleanup": "systemctl disable + stop + rm service file",
            "attck": "T1543.002",
        }),
        _ => json!({
            "mechanism": mechanism,
            "error": "Unknown mechanism — supported: launch_agent, cron, shell_profile, systemd",
        }),
    };

    Ok(json!({
        "schema": "aop-postex-v1",
        "module": "persistence_implant_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "plan": plan,
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Engagement cleanup checklist — artifacts to remove post-engagement.
pub fn cleanup_checklist(eng: &Engagement, engage_dir: &Path) -> Result<Value> {
    eng.validate_live()?;
    let mut artifacts: Vec<Value> = Vec::new();

    let subdirs = ["loot", "receipts", "reports", "tools"];
    for sub in &subdirs {
        let p = engage_dir.join(sub);
        if p.is_dir() {
            let count = fs::read_dir(&p)
                .map(|rd| rd.filter_map(|e| e.ok()).count())
                .unwrap_or(0);
            artifacts.push(json!({
                "directory": p.display().to_string(),
                "files": count,
                "action": "secure_delete",
            }));
        }
    }

    Ok(json!({
        "schema": "aop-postex-v1",
        "module": "cleanup_checklist",
        "engagement_id": eng.engagement_id,
        "artifacts": artifacts,
        "checklist": [
            "Remove all persistence mechanisms deployed during engagement",
            "Delete staged files and loot archives",
            "Revoke any credentials created or modified",
            "Clear command history on engaged systems",
            "Verify receipt chain integrity before archival",
            "Generate final purple-team report",
            "Secure-delete engagement directory (srm -sz)",
        ],
        "attck": ["T1070"],
        "executed": true,
    }))
}

/// Full post-exploitation assessment.
pub fn postex_assessment(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let persistence = persistence_enum(eng)?;

    Ok(json!({
        "schema": "aop-postex-v1",
        "module": "postex_assessment",
        "engagement_id": eng.engagement_id,
        "persistence": persistence,
        "attck": [
            "T1546.004", "T1543.004", "T1543.002",
            "T1547.015", "T1053.003", "T1070"
        ],
        "executed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn persistence_enum_runs() {
        let eng = Engagement::default_lab("postex-test", "lab-auth");
        let result = persistence_enum(&eng).unwrap();
        assert_eq!(result["module"], "persistence_enum");
        assert!(result["vectors"].is_array());
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn persistence_implant_plan_launch_agent() {
        let eng = Engagement::default_lab("postex-test", "lab-auth");
        let result = persistence_implant_plan(&eng, "launch_agent").unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn persistence_implant_plan_unknown_mechanism() {
        let eng = Engagement::default_lab("postex-test", "lab-auth");
        let result = persistence_implant_plan(&eng, "unknown_thing").unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert!(result["plan"]["error"].is_string());
    }

    #[test]
    fn cleanup_checklist_runs() {
        let eng = Engagement::default_lab("postex-test", "lab-auth");
        let result = cleanup_checklist(&eng, Path::new("/tmp/fake-engage")).unwrap();
        assert_eq!(result["module"], "cleanup_checklist");
        assert!(result["checklist"].is_array());
    }

    #[test]
    fn postex_assessment_combines() {
        let eng = Engagement::default_lab("postex-test", "lab-auth");
        let result = postex_assessment(&eng).unwrap();
        assert_eq!(result["module"], "postex_assessment");
        assert!(result["persistence"].is_object());
    }
}
