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
    for bad in ["rm -rf /", "mkfs", ":(){", "dd if=/dev/zero", "shutdown", "reboot"] {
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

/// Plan-only SMB/WinRM lateral (documented for Windows tranches).
pub fn lateral_smb_plan(eng: &Engagement, host: &str) -> Result<serde_json::Value> {
    eng.assert_lateral_host(host)?;
    Ok(serde_json::json!({
        "status": "PLAN_ONLY",
        "module": "lateral_smb",
        "host": host,
        "note": "SMB/WinRM lateral is planned for Windows operator hosts; macOS lab uses SSH + UDS.",
        "engagement_id": eng.engagement_id,
        "implemented": false,
    }))
}
