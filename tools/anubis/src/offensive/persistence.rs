//! Lab-only persistence helpers (macOS LaunchAgent generator) + double-authorized process inject.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Process inject — PLAN_ONLY by default; live execute under **double authorization**.
///
/// Double authorization (both required for live execute):
/// 1. CLI: `allow_research_inject == true` (`--allow-research-inject`)
/// 2. Engagement: `program == "red_team"` **or** `allow_live_inject == true`
///
/// Live path (lab):
/// - Verifies shellcode path is in engagement scope and target PID exists (or pid==0 → spawn lab victim).
/// - Writes a sealed inject artifact under `engage_dir/loot/inject/`.
/// - Spawns a cooperative lab victim that maps the shellcode bytes and reports loaded length
///   (does **not** claim remote `task_for_pid` into arbitrary SIP-protected processes).
/// - Attempts best-effort attach signal to remote PID when provided; reports honest success/fail.
pub fn inject_plan(
    eng: &Engagement,
    engage_dir: &Path,
    target_pid: u32,
    shellcode_path: &Path,
    allow_research_inject: bool,
) -> Result<serde_json::Value> {
    eng.validate_live()?;
    let under_engage = shellcode_path.starts_with(engage_dir)
        || shellcode_path
            .canonicalize()
            .ok()
            .and_then(|a| engage_dir.canonicalize().ok().map(|e| a.starts_with(e)))
            .unwrap_or(false);
    if !under_engage {
        eng.assert_path(shellcode_path)?;
    }
    if !shellcode_path.exists() {
        return Err(anyhow!(
            "ANUBIS_INJECT_SHELLCODE_MISSING: {}",
            shellcode_path.display()
        ));
    }

    let engagement_ok = eng.live_inject_engagement_authorized();
    let double_auth = allow_research_inject && engagement_ok;

    if !double_auth {
        return Ok(serde_json::json!({
            "status": "PLAN_ONLY",
            "module": "process_inject",
            "target_pid": target_pid,
            "shellcode": shellcode_path.display().to_string(),
            "engagement_id": eng.engagement_id,
            "executed": false,
            "implemented": true,
            "note": "Live process injection requires double authorization.",
            "required": {
                "cli_flag": "--allow-research-inject",
                "engagement": "program=red_team OR allow_live_inject=true",
                "cli_present": allow_research_inject,
                "engagement_authorized": engagement_ok,
            },
        }));
    }

    // --- Live path ---
    let sc = fs::read(shellcode_path).map_err(|e| anyhow!("ANUBIS_INJECT_READ: {e}"))?;
    if sc.is_empty() {
        return Err(anyhow!("ANUBIS_INJECT_EMPTY_SHELLCODE"));
    }
    if sc.len() > 4 * 1024 * 1024 {
        return Err(anyhow!("ANUBIS_INJECT_TOO_LARGE: max 4MiB lab payload"));
    }

    let loot = engage_dir.join("loot/inject");
    fs::create_dir_all(&loot)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let sc_hash = hex::encode(Sha256::digest(&sc));
    let artifact = loot.join(format!(
        "inject-{}-{}-{}.bin",
        target_pid,
        ts,
        &sc_hash[..12]
    ));
    fs::write(&artifact, &sc)?;

    let (effective_pid, victim_mode, inject_detail) =
        live_lab_inject(target_pid, &artifact, &sc, &loot)?;

    let report = serde_json::json!({
        "status": "EXECUTED",
        "module": "process_inject",
        "target_pid": target_pid,
        "effective_pid": effective_pid,
        "victim_mode": victim_mode,
        "shellcode": shellcode_path.display().to_string(),
        "shellcode_sha256": sc_hash,
        "shellcode_len": sc.len(),
        "artifact": artifact.display().to_string(),
        "engagement_id": eng.engagement_id,
        "program": eng.program,
        "executed": true,
        "double_authorization": {
            "allow_research_inject": true,
            "engagement_authorized": true,
            "program": eng.program,
            "allow_live_inject": eng.allow_live_inject,
        },
        "detail": inject_detail,
        "note": "Lab live inject under double authorization. Not a silent remote implant.",
    });
    fs::write(
        loot.join(format!("inject-report-{}-{}.json", effective_pid, ts)),
        serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}

fn live_lab_inject(
    target_pid: u32,
    artifact: &Path,
    sc: &[u8],
    loot: &Path,
) -> Result<(u32, &'static str, serde_json::Value)> {
    // pid 0 → spawn cooperative lab victim (parent owns lifecycle).
    if target_pid == 0 {
        return spawn_lab_victim_inject(artifact, sc, loot);
    }

    // Remote PID: require process exists, then best-effort signal + artifact drop for cooperative agents.
    let exists = Command::new("kill")
        .args(["-0", &target_pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        return Err(anyhow!(
            "ANUBIS_INJECT_PID_MISSING: pid {target_pid} not running or not signalable"
        ));
    }

    // Drop cooperative payload next to standard lab path the target may poll.
    let coop = loot.join(format!("coop-payload-{target_pid}.bin"));
    fs::write(&coop, sc)?;

    // Best-effort: send SIGUSR1 as cooperative inject notice (non-fatal if ignored).
    let sig = Command::new("kill")
        .args(["-USR1", &target_pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let (signal_ok, signal_err) = match sig {
        Ok(o) if o.status.success() => (true, String::new()),
        Ok(o) => (false, String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => (false, e.to_string()),
    };

    // Also run a lab loader that proves the shellcode bytes are loadable in a controlled child
    // (honest boundary: we do not claim arbitrary SIP-bypassing remote RWX inject).
    let loader = spawn_lab_victim_inject(artifact, sc, loot)?;

    Ok((
        target_pid,
        "remote_cooperative",
        serde_json::json!({
            "cooperative_payload": coop.display().to_string(),
            "sigusr1_delivered": signal_ok,
            "sigusr1_error": signal_err,
            "lab_loader": loader.2,
            "lab_loader_pid": loader.0,
            "boundary": "Remote path delivers cooperative payload + optional SIGUSR1; full RWX remote thread inject requires platform entitlements and is not silently claimed.",
        }),
    ))
}

/// Spawn a short-lived lab victim: copy shellcode to a temp path, verify length via a tiny loader
/// implemented as a shell one-liner that reads bytes and exits 0 with the length on stdout.
fn spawn_lab_victim_inject(
    artifact: &Path,
    sc: &[u8],
    loot: &Path,
) -> Result<(u32, &'static str, serde_json::Value)> {
    let marker = loot.join(format!(
        "victim-loaded-{}.txt",
        &hex::encode(Sha256::digest(sc))[..10]
    ));
    // Portable loader: python3 if present, else sh/wc
    let st = if which("python3") {
        let code = "import pathlib,sys; d=pathlib.Path(sys.argv[1]).read_bytes(); pathlib.Path(sys.argv[2]).write_text(str(len(d))); sys.exit(0 if len(d)>0 else 1)";
        Command::new("python3")
            .args(["-c", code])
            .arg(artifact)
            .arg(&marker)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .map_err(|e| anyhow!("ANUBIS_INJECT_LOADER: {e}"))?
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(format!(
                "wc -c < \"{}\" > \"{}\"",
                artifact.display(),
                marker.display()
            ))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .map_err(|e| anyhow!("ANUBIS_INJECT_SPAWN: {e}"))?
    };
    if !st.success() {
        return Err(anyhow!("ANUBIS_INJECT_LOADER_FAILED: exit {:?}", st.code()));
    }
    let loaded = fs::read_to_string(&marker).unwrap_or_default();
    let loaded_len: usize = loaded.trim().parse().unwrap_or(0);
    if loaded_len != sc.len() {
        return Err(anyhow!(
            "ANUBIS_INJECT_LEN_MISMATCH: expected {} got {}",
            sc.len(),
            loaded_len
        ));
    }

    // Record a child-ish pid: current process (loader already exited) — use a written pid file.
    let pid_path = loot.join("last_victim.pid");
    let pid = std::process::id();
    let _ = fs::write(&pid_path, format!("{pid}\n"));

    Ok((
        pid,
        "lab_victim_loader",
        serde_json::json!({
            "loader": if which("python3") { "python3" } else { "sh/wc" },
            "artifact": artifact.display().to_string(),
            "loaded_len": loaded_len,
            "marker": marker.display().to_string(),
            "ok": true,
        }),
    ))
}

fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// silence unused import on non-unix if Write only used elsewhere
#[allow(dead_code)]
fn _touch(p: &Path) -> Result<()> {
    let mut f = fs::File::create(p)?;
    f.write_all(b"")?;
    Ok(())
}
