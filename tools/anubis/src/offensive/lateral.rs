//! Lateral movement helpers — only hosts in allowed_lateral_hosts ∩ host scope.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use std::process::Command;

/// Probe lateral target with a scoped remote command via SSH (lab/authorized).
/// Does not auto-bruteforce. Requires existing SSH auth on operator host.
pub fn lateral_ssh(
    eng: &Engagement,
    host: &str,
    user: &str,
    remote_cmd: &str,
) -> Result<serde_json::Value> {
    eng.validate_live()?;
    eng.assert_lateral_host(host)?;
    if remote_cmd.trim().is_empty() {
        return Err(anyhow!("ANUBIS_LATERAL_EMPTY_CMD"));
    }
    // Deny obvious destructive patterns without ROE expansion
    let lowered = remote_cmd.to_ascii_lowercase();
    for bad in [
        "rm -rf /",
        "mkfs",
        ":(){",
        "dd if=/dev/zero",
        "shutdown",
        "reboot",
    ] {
        if lowered.contains(bad) {
            return Err(anyhow!(
                "ANUBIS_LATERAL_CMD_BLOCKED: refused pattern `{bad}`"
            ));
        }
    }
    let target = if user.is_empty() {
        host.to_string()
    } else {
        format!("{user}@{host}")
    };
    let output = Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=accept-new",
            &target,
            remote_cmd,
        ])
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            Ok(serde_json::json!({
                "ok": o.status.success(),
                "host": host,
                "user": user,
                "cmd": remote_cmd,
                "exit_code": o.status.code(),
                "stdout": stdout,
                "stderr": stderr,
                "engagement_id": eng.engagement_id,
                "module": "lateral_ssh",
            }))
        }
        Err(e) => Err(anyhow!("ANUBIS_LATERAL_SSH_SPAWN: {e}")),
    }
}

/// Plan-only SMB/WinRM lateral (Windows tranche).
///
/// **Never executes** network SMB/WinRM. Emits a structured plan for operator review
/// under engagement scope. Live execution remains NOT CLAIMED / Windows-only future work.
pub fn lateral_smb_plan(eng: &Engagement, host: &str) -> Result<serde_json::Value> {
    eng.validate_live()?;
    eng.assert_lateral_host(host)?;
    Ok(serde_json::json!({
        "status": "PLAN_ONLY",
        "module": "lateral_smb",
        "host": host,
        "engagement_id": eng.engagement_id,
        "implemented": false,
        "executed": false,
        "steps": [
            "Verify host is in allowed_lateral_hosts ∩ host/cidr scope (done)",
            "On Windows operator host: authenticate with engagement-scoped credentials only",
            "Enumerate shares / WinRM endpoint (not run on this host)",
            "Copy lab agent via authorized channel; do not use unscoped credentials",
        ],
        "blocked_on_this_host": true,
        "note": "SMB/WinRM lateral plan only. macOS lab uses lateral-ssh + UDS. No SMB sockets opened.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn lateral_ssh_blocks_destructive_commands() {
        let mut eng = Engagement::default_lab("lateral-test", "unit test auth");
        // `assert_lateral_host` calls `assert_host` FIRST, so a lateral target must be in the
        // general scope AND the lateral allowlist — defence in depth. Populating only the latter
        // makes the call fail at ANUBIS_SCOPE_DENIED and never reach the command denylist under
        // test.
        eng.allowed_hosts.push("10.0.0.1".into());
        eng.allowed_lateral_hosts.push("10.0.0.1".into());
        let blocked = [
            "rm -rf /",
            "mkfs.ext4",
            ":(){ :|:&};:",
            "dd if=/dev/zero",
            "shutdown now",
            "reboot",
        ];
        for bad in blocked {
            let result = lateral_ssh(&eng, "10.0.0.1", "root", bad);
            let err = result.unwrap_err().to_string();
            assert!(err.contains("ANUBIS_LATERAL_CMD_BLOCKED"), "should block `{bad}`: {err}");
        }
    }

    #[test]
    fn lateral_ssh_rejects_empty_command() {
        let mut eng = Engagement::default_lab("lateral-test2", "unit test auth");
        eng.allowed_hosts.push("10.0.0.1".into());
        eng.allowed_lateral_hosts.push("10.0.0.1".into());
        let err = lateral_ssh(&eng, "10.0.0.1", "root", "  ").unwrap_err().to_string();
        assert!(err.contains("ANUBIS_LATERAL_EMPTY_CMD"), "{err}");
    }
}
