//! Lab-only persistence helpers (macOS LaunchAgent generator). Fail-closed to engagement paths.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Generate a LaunchAgent plist that runs an engagement agent at login (lab / authorized only).
pub fn generate_launch_agent(
    eng: &Engagement,
    engage_dir: &Path,
    agent_bin: &Path,
    label: &str,
) -> Result<PathBuf> {
    eng.validate_live()?;
    // Agent may live under the engagement workspace (always in-scope for that engagement).
    let under_engage = agent_bin.starts_with(engage_dir)
        || agent_bin
            .canonicalize()
            .ok()
            .and_then(|a| engage_dir.canonicalize().ok().map(|e| a.starts_with(e)))
            .unwrap_or(false);
    if !under_engage {
        eng.assert_path(agent_bin)?;
    }
    if !agent_bin.exists() {
        return Err(anyhow!(
            "ANUBIS_PERSIST_AGENT_MISSING: {}",
            agent_bin.display()
        ));
    }
    let out_dir = engage_dir.join("persistence");
    fs::create_dir_all(&out_dir)?;
    let label = if label.is_empty() {
        format!("com.anubis.aop.{}", eng.engagement_id)
    } else {
        label.to_string()
    };
    let plist_path = out_dir.join(format!("{label}.plist"));
    let abs_agent = agent_bin
        .canonicalize()
        .unwrap_or_else(|_| agent_bin.to_path_buf());
    let log_out = out_dir.join("agent.out.log");
    let log_err = out_dir.join("agent.err.log");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{agent}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{out}</string>
  <key>StandardErrorPath</key>
  <string>{err}</string>
  <key>WorkingDirectory</key>
  <string>{wd}</string>
  <!-- ANUBIS LAB ONLY: do not install outside authorized engagement scope -->
</dict>
</plist>
"#,
        label = label,
        agent = abs_agent.display(),
        out = log_out.display(),
        err = log_err.display(),
        wd = engage_dir
            .canonicalize()
            .unwrap_or_else(|_| engage_dir.to_path_buf())
            .display(),
    );
    fs::write(&plist_path, plist)?;
    let install_sh = out_dir.join("install_launchagent.sh");
    fs::write(
        &install_sh,
        format!(
            "#!/usr/bin/env bash\n# Lab-only installer — requires explicit operator action\nset -euo pipefail\nDEST=\"$HOME/Library/LaunchAgents/{label}.plist\"\ncp \"{plist}\" \"$DEST\"\nlaunchctl unload \"$DEST\" 2>/dev/null || true\nlaunchctl load \"$DEST\"\necho \"loaded $DEST\"\n",
            label = label,
            plist = plist_path.display(),
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&install_sh)?.permissions();
        p.set_mode(0o755);
        fs::set_permissions(&install_sh, p)?;
    }
    let meta = serde_json::json!({
        "kind": "macos_launch_agent",
        "label": label,
        "plist": plist_path,
        "agent": abs_agent,
        "install_script": install_sh,
        "engagement_id": eng.engagement_id,
        "note": "Lab persistence artifact only; install requires human operator",
    });
    fs::write(
        out_dir.join("persistence_meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;
    println!("LaunchAgent plist: {}", plist_path.display());
    println!("Install helper:    {}", install_sh.display());
    Ok(plist_path)
}

/// Research-gated process inject placeholder — emits plan + refuses silent execution.
pub fn inject_plan(
    eng: &Engagement,
    target_pid: u32,
    shellcode_path: &Path,
) -> Result<serde_json::Value> {
    eng.validate_live()?;
    eng.assert_path(shellcode_path)?;
    // We intentionally do NOT perform live process injection in this tranche.
    // Plan-only keeps the surface real for operators while remaining fail-closed by default.
    Ok(serde_json::json!({
        "status": "PLAN_ONLY",
        "module": "process_inject",
        "target_pid": target_pid,
        "shellcode": shellcode_path,
        "engagement_id": eng.engagement_id,
        "note": "Live process injection is research-gated and not auto-executed. Provide explicit future enablement + ROE.",
        "required_flags": ["--allow-research-inject", "engagement.program=red_team"],
        "implemented": false,
    }))
}
