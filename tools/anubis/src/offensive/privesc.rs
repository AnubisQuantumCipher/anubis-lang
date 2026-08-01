//! Privilege escalation module — T10 (TA0004).
//!
//! Enumeration runs inside VZ guests. The host never executes privesc
//! payloads. Kernel exploit and entitlement abuse are PLAN_ONLY.

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// SUID/SGID binary enumeration.
///
/// Finds set-uid/set-gid binaries under common paths inside a VZ guest.
/// Maps to T1548.001 (Setuid and Setgid).
pub fn suid_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let search_paths = ["/usr/bin", "/usr/sbin", "/usr/local/bin", "/bin", "/sbin"];
    let mut suid_bins: Vec<Value> = Vec::new();
    let mut sgid_bins: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for base in &search_paths {
        let base_path = Path::new(base);
        if !base_path.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(base_path) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("{base}: {e}"));
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mode = meta.permissions().mode();
            let is_suid = mode & 0o4000 != 0;
            let is_sgid = mode & 0o2000 != 0;
            if is_suid {
                suid_bins.push(json!({
                    "path": path.display().to_string(),
                    "mode": format!("{:o}", mode),
                    "size": meta.len(),
                }));
            }
            if is_sgid {
                sgid_bins.push(json!({
                    "path": path.display().to_string(),
                    "mode": format!("{:o}", mode),
                    "size": meta.len(),
                }));
            }
        }
    }

    let gtfobins_interesting = [
        "nmap", "vim", "find", "bash", "less", "more", "nano", "cp", "mv", "python", "python3",
        "perl", "ruby", "node", "php", "awk", "env", "strace", "ltrace", "gdb", "docker", "pkexec",
    ];
    let exploitable: Vec<&Value> = suid_bins
        .iter()
        .filter(|b| {
            let p = b["path"].as_str().unwrap_or("");
            let name = Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            gtfobins_interesting.contains(&name)
        })
        .collect();

    Ok(json!({
        "schema": "aop-privesc-v1",
        "module": "suid_enum",
        "engagement_id": eng.engagement_id,
        "suid_binaries": suid_bins,
        "sgid_binaries": sgid_bins,
        "suid_count": suid_bins.len(),
        "sgid_count": sgid_bins.len(),
        "gtfobins_matches": exploitable,
        "search_paths": search_paths,
        "errors": errors,
        "attck": ["T1548.001"],
        "executed": true,
    }))
}

/// Sudo configuration audit.
///
/// Parses `sudo -l` output to find NOPASSWD entries, wildcard rules,
/// and env_keep escalation paths.
pub fn sudo_audit(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let output = Command::new("sudo").args(["-l", "-n"]).output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let mut findings: Vec<Value> = Vec::new();

            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.contains("NOPASSWD") {
                    findings.push(json!({
                        "severity": "high",
                        "code": "SUDO_NOPASSWD",
                        "rule": trimmed,
                        "message": "NOPASSWD sudo rule — potential privesc vector",
                    }));
                }
                if trimmed.contains("ALL") && trimmed.contains("(ALL)") {
                    findings.push(json!({
                        "severity": "critical",
                        "code": "SUDO_ALL",
                        "rule": trimmed,
                        "message": "Unrestricted sudo access",
                    }));
                }
                if trimmed.contains("env_keep") {
                    findings.push(json!({
                        "severity": "medium",
                        "code": "SUDO_ENV_KEEP",
                        "rule": trimmed,
                        "message": "env_keep may allow LD_PRELOAD or PATH hijack",
                    }));
                }
                if trimmed.contains('*') {
                    findings.push(json!({
                        "severity": "high",
                        "code": "SUDO_WILDCARD",
                        "rule": trimmed,
                        "message": "Wildcard in sudo rule — argument injection possible",
                    }));
                }
            }

            Ok(json!({
                "schema": "aop-privesc-v1",
                "module": "sudo_audit",
                "engagement_id": eng.engagement_id,
                "sudo_available": o.status.success() || !stdout.is_empty(),
                "stdout": stdout,
                "stderr": stderr,
                "findings": findings,
                "attck": ["T1548.003"],
                "executed": true,
            }))
        }
        Err(e) => Ok(json!({
            "schema": "aop-privesc-v1",
            "module": "sudo_audit",
            "engagement_id": eng.engagement_id,
            "sudo_available": false,
            "error": e.to_string(),
            "findings": [],
            "attck": ["T1548.003"],
            "executed": true,
        })),
    }
}

/// Writable path audit — find world-writable directories in PATH.
///
/// If any PATH directory is writable by the current user, an attacker
/// can plant a trojan binary that runs with the victim's privileges.
pub fn writable_path_audit(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut writable: Vec<Value> = Vec::new();
    let mut checked = 0u32;

    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        checked += 1;
        let p = Path::new(dir);
        if !p.is_dir() {
            continue;
        }
        if let Ok(meta) = fs::metadata(p) {
            let mode = meta.permissions().mode();
            if mode & 0o002 != 0 {
                writable.push(json!({
                    "path": dir,
                    "mode": format!("{:o}", mode),
                    "severity": "high",
                    "message": "World-writable directory in PATH",
                }));
            } else if mode & 0o020 != 0 {
                writable.push(json!({
                    "path": dir,
                    "mode": format!("{:o}", mode),
                    "severity": "medium",
                    "message": "Group-writable directory in PATH",
                }));
            }
        }
    }

    Ok(json!({
        "schema": "aop-privesc-v1",
        "module": "writable_path_audit",
        "engagement_id": eng.engagement_id,
        "path_dirs_checked": checked,
        "writable_dirs": writable,
        "writable_count": writable.len(),
        "attck": ["T1574.007"],
        "executed": true,
    }))
}

/// Cron job enumeration — find user and system cron entries.
pub fn cron_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let mut entries: Vec<Value> = Vec::new();

    let user_cron = Command::new("crontab").arg("-l").output();
    if let Ok(o) = user_cron {
        if o.status.success() {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                entries.push(json!({
                    "source": "user_crontab",
                    "entry": trimmed,
                    "writable": false,
                }));
            }
        }
    }

    let system_cron_dirs = ["/etc/cron.d", "/etc/cron.daily", "/etc/cron.hourly"];
    for dir in &system_cron_dirs {
        let p = Path::new(dir);
        if !p.is_dir() {
            continue;
        }
        if let Ok(rd) = fs::read_dir(p) {
            for entry in rd.flatten() {
                let path = entry.path();
                let writable = fs::metadata(&path)
                    .map(|m| m.permissions().mode() & 0o002 != 0)
                    .unwrap_or(false);
                entries.push(json!({
                    "source": dir,
                    "file": path.display().to_string(),
                    "writable": writable,
                }));
            }
        }
    }

    // macOS LaunchDaemons/LaunchAgents
    let launch_dirs = ["/Library/LaunchDaemons", "/Library/LaunchAgents"];
    let home = std::env::var("HOME").unwrap_or_default();
    let user_agents = format!("{home}/Library/LaunchAgents");

    for dir in launch_dirs
        .iter()
        .chain(std::iter::once(&user_agents.as_str()))
    {
        let p = Path::new(dir);
        if !p.is_dir() {
            continue;
        }
        if let Ok(rd) = fs::read_dir(p) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("plist") {
                    continue;
                }
                let writable = fs::metadata(&path)
                    .map(|m| m.permissions().mode() & 0o022 != 0)
                    .unwrap_or(false);
                entries.push(json!({
                    "source": dir,
                    "file": path.display().to_string(),
                    "writable": writable,
                    "type": "launchd_plist",
                }));
            }
        }
    }

    let writable_count = entries.iter().filter(|e| e["writable"] == true).count();

    Ok(json!({
        "schema": "aop-privesc-v1",
        "module": "cron_enum",
        "engagement_id": eng.engagement_id,
        "entries": entries,
        "total": entries.len(),
        "writable_count": writable_count,
        "attck": ["T1053.003", "T1053.004"],
        "executed": true,
    }))
}

/// Kernel exploit planning — PLAN_ONLY.
pub fn kernel_exploit_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let uname = Command::new("uname").arg("-a").output();
    let kernel_info = uname
        .as_ref()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    Ok(json!({
        "schema": "aop-privesc-v1",
        "module": "kernel_exploit_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "kernel_info": kernel_info,
        "attck": ["T1068"],
        "steps": [
            "Identify kernel version and patch level",
            "Cross-reference with known CVEs (searchsploit, exploit-db)",
            "Validate exploit applicability in VZ guest",
            "Test PoC in isolated VZ sandbox (never on host)",
            "Document crash behavior and privilege state change",
        ],
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
            "kernel_info_collected": !kernel_info.is_empty(),
        },
    }))
}

/// Full privilege escalation enumeration — combines all checks.
pub fn privesc_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let suid = suid_enum(eng)?;
    let sudo = sudo_audit(eng)?;
    let paths = writable_path_audit(eng)?;
    let cron = cron_enum(eng)?;
    let kernel = kernel_exploit_plan(eng)?;

    let mut total_findings = 0u32;
    for section in [&suid, &sudo, &paths] {
        if let Some(f) = section.get("findings").and_then(|v| v.as_array()) {
            total_findings += f.len() as u32;
        }
    }
    total_findings += paths["writable_count"].as_u64().unwrap_or(0) as u32;
    total_findings += suid
        .get("gtfobins_matches")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as u32)
        .unwrap_or(0);

    Ok(json!({
        "schema": "aop-privesc-v1",
        "module": "privesc_enum",
        "engagement_id": eng.engagement_id,
        "suid": suid,
        "sudo": sudo,
        "writable_paths": paths,
        "cron": cron,
        "kernel": kernel,
        "total_findings": total_findings,
        "attck": ["T1548.001", "T1548.003", "T1574.007", "T1053.003", "T1068"],
        "executed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn suid_enum_returns_structured_result() {
        let eng = Engagement::default_lab("privesc-test", "lab-auth");
        let result = suid_enum(&eng).unwrap();
        assert_eq!(result["module"], "suid_enum");
        assert!(result["suid_binaries"].is_array());
        assert!(result["sgid_binaries"].is_array());
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn sudo_audit_does_not_panic() {
        let eng = Engagement::default_lab("privesc-test", "lab-auth");
        let result = sudo_audit(&eng);
        assert!(result.is_ok());
    }

    #[test]
    fn writable_path_audit_checks_path_env() {
        let eng = Engagement::default_lab("privesc-test", "lab-auth");
        let result = writable_path_audit(&eng).unwrap();
        assert!(result["path_dirs_checked"].as_u64().unwrap() > 0);
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn cron_enum_returns_structured_result() {
        let eng = Engagement::default_lab("privesc-test", "lab-auth");
        let result = cron_enum(&eng).unwrap();
        assert_eq!(result["module"], "cron_enum");
        assert!(result["entries"].is_array());
    }

    #[test]
    fn kernel_exploit_plan_is_plan_only() {
        let eng = Engagement::default_lab("privesc-test", "lab-auth");
        let result = kernel_exploit_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["executed"], false);
    }

    #[test]
    fn privesc_enum_combines_all_checks() {
        let eng = Engagement::default_lab("privesc-test", "lab-auth");
        let result = privesc_enum(&eng).unwrap();
        assert_eq!(result["module"], "privesc_enum");
        assert!(result["suid"].is_object());
        assert!(result["sudo"].is_object());
        assert!(result["writable_paths"].is_object());
        assert!(result["cron"].is_object());
        assert!(result["kernel"].is_object());
    }
}
