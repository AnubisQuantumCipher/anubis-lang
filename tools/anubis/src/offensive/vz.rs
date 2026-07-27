//! Host-side VZ control plane for the Anubis Offensive Platform (AOP).
//!
//! These are the top-level `anubis vz-status|vz-start|vz-exec|…` orchestration commands.
//! They are **host-allowed** control-plane surfaces (see `isolation::isolation_status_json`):
//! the host never runs exploit payloads; it boots/drives Tart guests that do.
//!
//! Canonical substrate: **Tart** (Apple Virtualization.framework) — same backend as
//! `anubis vz status|run|exec|…` (`tools/anubis/src/vz.rs`). The old `vmctl` path is
//! non-authoritative and only available under `ANUBIS_ALLOW_LEGACY_VMCTL=1` for
//! migration diagnostics (never isolation evidence).
//!
//! Personality (Anubis-shaped):
//! - host = orchestration + evidence collection
//! - guest = offensive execution
//! - plan-only stays plan-only elsewhere; live work needs a running guest
//! - fail closed with stable `ANUBIS_VZ_*` codes when the substrate is missing

use super::engagement::Engagement;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

const DEFAULT_TIMEOUT_SECS: u64 = 3600;
const EXPORTS_PREFIX: &str = "/exports/anubis-offensive";
const DEFAULT_SSH_USER: &str = "admin";
const WORKSPACE_ROOT_ENV: &str = "ANUBIS_WORKSPACE_ROOT";
const VMCTL_ENV: &str = "ANUBIS_VMCTL_PATH";
const DEFAULT_VMCTL: &str = "vmctl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VzGuest {
    pub name: String,
    pub role: VzRole,
    pub cpu_count: u32,
    pub memory_mib: u32,
    pub disk_gib: u32,
    pub distribution: String,
    pub running: bool,
    pub network: VzNetwork,
    /// Backend that reported this guest (`tart` or `legacy_vmctl`).
    #[serde(default = "default_backend_tart")]
    pub backend: String,
}

fn default_backend_tart() -> String {
    "tart".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VzRole {
    OffensiveLab,
    ExploitSandbox,
    FuzzTarget,
    AgentTest,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VzNetwork {
    Off,
    LoopbackOnly,
    Nat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VzExecResult {
    pub guest: String,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub network: VzNetwork,
    pub evidence_hash: String,
    #[serde(default = "default_backend_tart")]
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VzLabConfig {
    pub guest_name: String,
    pub engage_dir: PathBuf,
    pub sync_sources: bool,
    pub network: VzNetwork,
    pub timeout_secs: u64,
    pub auto_build: bool,
}

impl Default for VzLabConfig {
    fn default() -> Self {
        Self {
            guest_name: "anubis-xcode".into(),
            engage_dir: PathBuf::from("out/engagements/lab"),
            sync_sources: true,
            network: VzNetwork::Off,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            auto_build: true,
        }
    }
}

// ── Tart substrate (canonical) ──────────────────────────────────────────────

fn tart_bin() -> Result<&'static str> {
    let ok = Command::new("tart")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Ok("tart")
    } else {
        Err(anyhow!(
            "ANUBIS_VZ_BACKEND_MISSING: the VZ backend `tart` (Apple Virtualization.framework) \
             is not installed or not on PATH. Install with `brew install cirruslabs/cli/tart`. \
             Equivalent: `anubis vz status`."
        ))
    }
}

fn tart_capture(args: &[&str]) -> Result<String> {
    let bin = tart_bin()?;
    let out = Command::new(bin)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn `{bin} {}`", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "ANUBIS_VZ_COMMAND_FAILED: `tart {}` exited non-zero: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn tart_ssh_identity() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ANUBIS_VZ_SSH_KEY") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "ANUBIS_VZ_SSH_KEY_MISSING: ANUBIS_VZ_SSH_KEY={} is not a file",
            path.display()
        );
    }
    let path = dirs::home_dir()
        .ok_or_else(|| anyhow!("ANUBIS_VZ_SSH_KEY_MISSING: HOME unset"))?
        .join(".ssh/tart_anubis");
    if !path.is_file() {
        bail!(
            "ANUBIS_VZ_SSH_KEY_MISSING: canonical Tart identity `{}` does not exist \
             (create it or set ANUBIS_VZ_SSH_KEY). Same key as `anubis vz exec`.",
            path.display()
        );
    }
    Ok(path)
}

fn ssh_common_args() -> Result<Vec<std::ffi::OsString>> {
    let key = tart_ssh_identity()?;
    Ok(vec![
        "-i".into(),
        key.as_os_str().to_os_string(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
    ])
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Build a **single** SSH remote argv that forces bash.
///
/// OpenSSH joins multiple remote argv with spaces and runs them under the
/// user's login shell via `shell -c "…"`. Passing `bash`, `-lc`, `script` as
/// three argv therefore becomes:
///   zsh -c "bash -lc cd '/Users/admin' && export …; <script>"
/// so only `cd` runs under bash and the rest of the script executes under zsh
/// (macOS Tart guests default to zsh). That breaks `set -u`, globs, and C2.
/// One argv keeps the full script inside bash -lc.
fn ssh_remote_bash_lc(script: &str) -> String {
    format!("exec bash -lc {}", shell_single_quote(script))
}

/// Guest-side `cd` prefix. `$HOME` must not be single-quoted or it becomes a
/// literal directory name.
fn guest_cd_prefix(cwd: &str) -> String {
    if cwd == "$HOME" || cwd == "~" {
        "cd \"$HOME\"".into()
    } else {
        format!("cd {}", shell_single_quote(cwd))
    }
}

fn workspace_root() -> Option<String> {
    if let Ok(root) = std::env::var(WORKSPACE_ROOT_ENV) {
        if !root.is_empty() {
            return Some(root);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            if dir.join("Cargo.toml").is_file()
                && (dir.join("tools/anubis").is_dir() || dir.join("AGENTS.md").is_file())
            {
                return Some(dir.display().to_string());
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    let candidates = [
        PathBuf::from("/Users/sicarii/anubis-lang"),
        dirs::home_dir()?.join("anubis-lang"),
    ];
    for p in candidates {
        if p.is_dir() {
            return Some(p.display().to_string());
        }
    }
    None
}

/// True only when Tart reports the VM as running (do **not** trust a leftover `tart ip`
/// after stop — that returns a stale DHCP lease and made vz-start a no-op).
fn guest_is_running(name: &str) -> Result<bool> {
    let guests = vz_status()?;
    Ok(guests.iter().any(|g| g.name == name && g.running))
}

fn guest_ip(name: &str) -> Result<String> {
    if !guest_is_running(name)? {
        bail!(
            "ANUBIS_VZ_GUEST_NOT_RUNNING: `{name}` is not running \
             (`anubis vz-start --guest {name}` or `anubis vz run {name} --detach`)"
        );
    }
    tart_capture(&["ip", name]).with_context(|| {
        format!("VM `{name}` is running but has no IP yet — wait and retry, or re-start the guest")
    })
}

// ── Legacy vmctl (migration-only) ───────────────────────────────────────────

fn legacy_vmctl_enabled() -> bool {
    std::env::var("ANUBIS_ALLOW_LEGACY_VMCTL").ok().as_deref() == Some("1")
}

fn vmctl_bin() -> PathBuf {
    if let Ok(p) = std::env::var(VMCTL_ENV) {
        return PathBuf::from(p);
    }
    PathBuf::from(DEFAULT_VMCTL)
}

fn run_vmctl(args: &[&str]) -> Result<std::process::Output> {
    if !legacy_vmctl_enabled() {
        return Err(anyhow!(
            "ANUBIS_VZ_LEGACY_VMCTL_DISABLED: offensive vmctl path is non-authoritative and disabled. \
             Top-level `anubis vz-*` now uses Tart (same as `anubis vz`). \
             Set ANUBIS_ALLOW_LEGACY_VMCTL=1 only for migration diagnostics (not isolation evidence). \
             attempted args: {args:?}"
        ));
    }
    let bin = vmctl_bin();
    let mut cmd = Command::new(&bin);
    if let Some(root) = workspace_root() {
        cmd.env(WORKSPACE_ROOT_ENV, &root);
    }
    cmd.args(args);
    cmd.output()
        .map_err(|e| anyhow!("ANUBIS_VZ_VMCTL_SPAWN: {}: {e}", bin.display()))
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Query Tart VM inventory (canonical). Same substrate as `anubis vz list`.
pub fn vz_status() -> Result<Vec<VzGuest>> {
    if legacy_vmctl_enabled() {
        return vz_status_legacy_vmctl();
    }
    let _ = tart_bin()?;
    let raw = tart_capture(&["list", "--format", "json"]).unwrap_or_else(|_| "[]".into());
    let vms: Vec<serde_json::Value> =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_VZ_STATUS_PARSE: {e}"))?;
    let mut guests = Vec::new();
    for v in vms {
        let name = v["Name"]
            .as_str()
            .or_else(|| v["name"].as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let running = v["Running"]
            .as_bool()
            .or_else(|| v["running"].as_bool())
            .unwrap_or_else(|| {
                v["State"]
                    .as_str()
                    .or_else(|| v["state"].as_str())
                    .map(|s| s.eq_ignore_ascii_case("running"))
                    .unwrap_or(false)
            });
        // Tart JSON: Disk is capacity (GiB display); fall back to Size when absent.
        let disk = v["Disk"]
            .as_u64()
            .or_else(|| v["disk"].as_u64())
            .or_else(|| v["Size"].as_u64())
            .or_else(|| v["size"].as_u64())
            .unwrap_or(0) as u32;
        guests.push(VzGuest {
            name,
            role: VzRole::OffensiveLab,
            cpu_count: v["CPU"].as_u64().or_else(|| v["cpu"].as_u64()).unwrap_or(0) as u32,
            memory_mib: v["Memory"]
                .as_u64()
                .or_else(|| v["memory"].as_u64())
                .unwrap_or(0) as u32,
            disk_gib: disk,
            distribution: v["Source"]
                .as_str()
                .or_else(|| v["source"].as_str())
                .unwrap_or("local")
                .into(),
            running,
            network: VzNetwork::Off,
            backend: "tart".into(),
        });
    }
    Ok(guests)
}

fn vz_status_legacy_vmctl() -> Result<Vec<VzGuest>> {
    let output = run_vmctl(&["status", "--json"])?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ANUBIS_VZ_STATUS: {err}"));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let vms: Vec<serde_json::Value> =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_VZ_STATUS_PARSE: {e}"))?;
    let mut guests = Vec::new();
    for v in vms {
        let net_active = v
            .get("network_window_active")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        guests.push(VzGuest {
            name: v["name"].as_str().unwrap_or("").into(),
            role: VzRole::OffensiveLab,
            cpu_count: v["cpu_count"].as_u64().unwrap_or(0) as u32,
            memory_mib: v["memory_mib"].as_u64().unwrap_or(0) as u32,
            disk_gib: v["disk_gib"].as_u64().unwrap_or(0) as u32,
            distribution: v["distribution"].as_str().unwrap_or("").into(),
            running: v["running"].as_bool().unwrap_or(false)
                || v["status"].as_str() == Some("running"),
            network: if net_active {
                VzNetwork::Nat
            } else {
                VzNetwork::Off
            },
            backend: "legacy_vmctl".into(),
        });
    }
    Ok(guests)
}

/// Find a running guest suitable for offensive work.
pub fn find_offensive_guest(preferred: Option<&str>) -> Result<VzGuest> {
    let guests = vz_status()?;
    if let Some(name) = preferred {
        if let Some(g) = guests.iter().find(|g| g.name == name && g.running) {
            return Ok(g.clone());
        }
        return Err(anyhow!(
            "ANUBIS_VZ_GUEST_NOT_RUNNING: `{name}` is not running \
             (start with `anubis vz-start --guest {name}` or `anubis vz run {name} --detach`)"
        ));
    }
    if let Some(g) = guests
        .iter()
        .find(|g| g.name == "anubis-xcode" && g.running)
    {
        return Ok(g.clone());
    }
    guests.into_iter().find(|g| g.running).ok_or_else(|| {
        anyhow!(
            "ANUBIS_VZ_NO_RUNNING_GUEST: no running Tart guest found. \
                 Start one: `anubis vz run anubis-xcode --detach` (or `anubis vz-start`)."
        )
    })
}

/// Start a VZ guest if not already running (Tart headless detach).
pub fn vz_start(name: &str, network: &VzNetwork) -> Result<()> {
    if legacy_vmctl_enabled() {
        let net_flag = match network {
            VzNetwork::Off => "off",
            VzNetwork::LoopbackOnly => "off",
            VzNetwork::Nat => "nat",
        };
        let output = run_vmctl(&["start", name, "--net", net_flag])?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if err.contains("already running") {
                return Ok(());
            }
            return Err(anyhow!("ANUBIS_VZ_START: {err}"));
        }
        return Ok(());
    }

    let _ = tart_bin()?;
    // Must use Tart running-state, not a stale `tart ip` after stop.
    if guest_is_running(name)? {
        // Warm-check SSH so "started" means operable, not merely listed.
        if guest_ip(name).is_ok() && guest_home(name).is_ok() {
            return Ok(());
        }
        // Listed running but SSH dead — fall through is wrong; try stop+start once.
        let _ = tart_capture(&["stop", name]);
        std::thread::sleep(Duration::from_secs(2));
    }
    let bin = tart_bin()?;
    let mut args = vec!["run".to_string(), name.to_string(), "--no-graphics".into()];
    // Softnet ≈ NAT-like guest networking; default Tart is more host-local.
    if matches!(network, VzNetwork::Nat) {
        args.push("--net-softnet".into());
    }
    Command::new(bin)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("ANUBIS_VZ_START: failed to spawn tart run for `{name}`"))?;
    // Wait for Tart running + IP + SSH (home probe).
    for i in 0..45 {
        if guest_is_running(name).unwrap_or(false) {
            if let Ok(ip) = tart_capture(&["ip", name]) {
                if !ip.is_empty() {
                    // Soft SSH probe (may take a few seconds after IP appears).
                    if guest_home(name).is_ok() {
                        return Ok(());
                    }
                }
            }
        }
        if i % 5 == 4 {
            eprintln!("[vz-start] waiting for `{name}` SSH… ({i}0s)");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(anyhow!(
        "ANUBIS_VZ_NO_IP: `{name}` did not become SSH-reachable within ~90s after start"
    ))
}

/// Stop a VZ guest.
pub fn vz_stop(name: &str) -> Result<()> {
    if legacy_vmctl_enabled() {
        let output = run_vmctl(&["stop", name])?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("ANUBIS_VZ_STOP: {err}"));
        }
        return Ok(());
    }
    tart_capture(&["stop", name]).map(|_| ())
}

/// Execute a command inside a VZ guest over SSH (Tart). Network posture is the guest's.
pub fn vz_exec(
    guest: &str,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
) -> Result<VzExecResult> {
    if legacy_vmctl_enabled() {
        return vz_exec_legacy_vmctl(guest, command, cwd, timeout_secs);
    }
    let start = SystemTime::now();
    let ip = guest_ip(guest)?;
    let user = std::env::var("ANUBIS_VZ_SSH_USER").unwrap_or_else(|_| DEFAULT_SSH_USER.into());
    let working_dir = cwd.unwrap_or("$HOME");
    let remote_script = format!(
        "{cd} && export ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1; {cmd}",
        cd = guest_cd_prefix(working_dir),
        cmd = command,
    );
    // Single SSH remote argv — see ssh_remote_bash_lc.
    let remote = ssh_remote_bash_lc(&remote_script);
    let target = format!("{user}@{ip}");
    let args = ssh_common_args()?;
    // Soft timeout via ssh ConnectTimeout already set; command-level timeout when available.
    let timeout_s = timeout_secs.to_string();
    let use_timeout = Command::new("which")
        .arg("timeout")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let output = if use_timeout {
        let mut cmd = Command::new("timeout");
        cmd.arg(&timeout_s).arg("ssh");
        for a in &args {
            cmd.arg(a);
        }
        cmd.arg(&target)
            .arg(&remote)
            .output()
            .with_context(|| "ANUBIS_VZ_EXEC: failed to spawn timeout+ssh")?
    } else {
        let mut cmd = Command::new("ssh");
        for a in &args {
            cmd.arg(a);
        }
        cmd.arg(&target)
            .arg(&remote)
            .output()
            .with_context(|| "ANUBIS_VZ_EXEC: failed to spawn ssh")?
    };
    let duration = start.elapsed().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}{stderr}");
    let evidence_hash = hex::encode(Sha256::digest(combined.as_bytes()));
    Ok(VzExecResult {
        guest: guest.into(),
        command: command.into(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        duration_ms: duration.as_millis() as u64,
        network: VzNetwork::Off,
        evidence_hash,
        backend: "tart".into(),
    })
}

fn vz_exec_legacy_vmctl(
    guest: &str,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
) -> Result<VzExecResult> {
    let start = SystemTime::now();
    let working_dir = cwd.unwrap_or(EXPORTS_PREFIX);
    let mut args = vec!["exec", "--name", guest, "--cwd", working_dir, "--timeout"];
    let timeout_s = timeout_secs.to_string();
    args.push(&timeout_s);
    args.push("--");
    args.push("bash");
    args.push("-c");
    args.push(command);
    let output = run_vmctl(&args)?;
    let duration = start.elapsed().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}{stderr}");
    let evidence_hash = hex::encode(Sha256::digest(combined.as_bytes()));
    Ok(VzExecResult {
        guest: guest.into(),
        command: command.into(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        duration_ms: duration.as_millis() as u64,
        network: VzNetwork::Off,
        evidence_hash,
        backend: "legacy_vmctl".into(),
    })
}

/// Resolved guest-side AOP layout (absolute paths — never rely on rsync expanding `$HOME`).
#[derive(Debug, Clone)]
struct GuestLayout {
    home: String,
    /// Absolute: `/Users/admin/anubis-offensive`
    aop_root: String,
    /// Absolute: `…/anubis-offensive/engagement`
    engage: String,
    /// Absolute path to staged anubis binary on guest (may not exist until staged).
    bin: String,
}

fn guest_ssh_user() -> String {
    std::env::var("ANUBIS_VZ_SSH_USER").unwrap_or_else(|_| DEFAULT_SSH_USER.into())
}

fn rsync_ssh_transport() -> Result<String> {
    let key = tart_ssh_identity()?;
    let quoted_key = shell_single_quote(&key.to_string_lossy());
    Ok(format!(
        "ssh -i {quoted_key} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no \
         -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=15"
    ))
}

/// Absolute home directory on the guest.
///
/// Tart macOS golden guests use `/Users/<ssh-user>` (default `admin`). We prefer that
/// deterministic path (verified with `test -d`) over probing `$HOME` through ssh argv
/// reshaping, which produced empty strings and broke staging (`$HOME` in rsync dests
/// also never expands — always use absolute paths).
fn guest_home(guest: &str) -> Result<String> {
    let ip = guest_ip(guest)?;
    let user = guest_ssh_user();
    let predicted = format!("/Users/{user}");
    let target = format!("{user}@{ip}");
    // Verify path exists; also accept a probe that prints a path starting with /
    let out = Command::new("ssh")
        .args(ssh_common_args()?)
        .arg(&target)
        .arg(format!(
            "if [ -d {p} ]; then echo {p}; elif [ -n \"$HOME\" ] && [ -d \"$HOME\" ]; then echo \"$HOME\"; else exit 2; fi",
            p = shell_single_quote(&predicted)
        ))
        .output()
        .with_context(|| "ANUBIS_VZ_SYNC: failed to resolve guest home via ssh")?;
    if !out.status.success() {
        bail!(
            "ANUBIS_VZ_SYNC: guest home probe failed (predicted {predicted}): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let home = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with('/'))
        .unwrap_or("")
        .to_string();
    if home.is_empty() {
        bail!("ANUBIS_VZ_SYNC: guest returned empty home (predicted {predicted})");
    }
    Ok(home)
}

fn guest_layout(guest: &str) -> Result<GuestLayout> {
    // Ensure guest is reachable before probing home.
    let _ip = guest_ip(guest)?;
    let home = guest_home(guest)?;
    let aop_root = format!("{home}/anubis-offensive");
    Ok(GuestLayout {
        home: home.clone(),
        aop_root: aop_root.clone(),
        engage: format!("{aop_root}/engagement"),
        bin: format!("{aop_root}/bin/anubis"),
    })
}

fn ssh_run(guest: &str, remote: &str) -> Result<()> {
    let ip = guest_ip(guest)?;
    let user = guest_ssh_user();
    let target = format!("{user}@{ip}");
    // One remote argv (like Interactive ssh 'cmd'): avoids OpenSSH re-splitting
    // `bash -lc <script>` into a broken mkdir argv on macOS guests.
    let status = Command::new("ssh")
        .args(ssh_common_args()?)
        .arg(&target)
        .arg(remote)
        .status()
        .with_context(|| "ANUBIS_VZ_SSH: spawn failed")?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "ANUBIS_VZ_SSH: remote command failed (exit {:?}): {remote}",
            status.code()
        )
    }
}

/// rsync a local file or directory to an **absolute** guest path.
fn rsync_to_guest(guest: &str, local: &Path, remote_abs: &str) -> Result<()> {
    if !local.exists() {
        bail!("ANUBIS_VZ_SYNC: local path missing: {}", local.display());
    }
    let layout_ip = guest_ip(guest)?;
    let user = guest_ssh_user();
    let transport = rsync_ssh_transport()?;
    // Ensure remote parent exists (absolute path — no $HOME in rsync dest).
    let parent = Path::new(remote_abs)
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "/".into());
    ssh_run(guest, &format!("mkdir -p {}", shell_single_quote(&parent)))?;
    let dest = format!("{user}@{layout_ip}:{remote_abs}");
    let from = if local.is_dir() {
        format!("{}/", local.display())
    } else {
        local.display().to_string()
    };
    // For directories: rsync contents into remote_abs (remote_abs must be the dir itself).
    // For files: rsync file onto remote_abs file path.
    let status = Command::new("rsync")
        .args(["-az", "-e", &transport, &from, &dest])
        .status()
        .with_context(|| "ANUBIS_VZ_SYNC: failed to spawn rsync (is it installed?)")?;
    if !status.success() {
        bail!(
            "ANUBIS_VZ_SYNC_FAILED: rsync {} → guest:{remote_abs} exited non-zero",
            local.display()
        );
    }
    Ok(())
}

/// Build host-side staging tree + push absolute layout into the guest; verify engage loads.
fn ensure_guest_workspace(
    eng: &Engagement,
    engage_dir: &Path,
    guest: &str,
    project_root: &Path,
) -> Result<GuestLayout> {
    let layout = guest_layout(guest)?;
    let host_stage = find_exports_host_path(guest)?;

    // 1) Materialize engagement on host stage from operator engage_dir (full tree).
    let host_eng = host_stage.join("engagement");
    if host_eng.exists() {
        let _ = fs::remove_dir_all(&host_eng);
    }
    fs::create_dir_all(&host_eng)?;
    if engage_dir.is_dir() {
        // Copy tree: engagement.json + subdirs the operator already has.
        copy_dir_merge(engage_dir, &host_eng)?;
    }
    // Always write canonical serialized engagement (PSK/hash consistent with host load).
    let eng_json = serde_json::to_string_pretty(eng)?;
    fs::write(host_eng.join("engagement.json"), &eng_json)?;
    for sub in ["agents", "tasks", "loot", "evidence", "modules", "packs"] {
        fs::create_dir_all(host_eng.join(sub))?;
    }

    // 2) Stage host anubis binary for guest CLI.
    let host_bin = workspace_root()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("target/release/anubis");
    let host_bin_debug = workspace_root()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("target/debug/anubis");
    let bin_src = if host_bin.is_file() {
        Some(host_bin)
    } else if host_bin_debug.is_file() {
        Some(host_bin_debug)
    } else {
        None
    };
    if let Some(ref src) = bin_src {
        fs::create_dir_all(host_stage.join("bin"))?;
        fs::copy(src, host_stage.join("bin/anubis"))?;
    }

    // 3) Optional project marker (not full monorepo — heavy).
    if project_root.join("Cargo.toml").exists() {
        let src_marker = host_stage.join("src");
        fs::create_dir_all(&src_marker)?;
        fs::write(
            src_marker.join("STAGED_FROM.txt"),
            format!("{}\n", project_root.display()),
        )?;
    }

    // 4) Push absolute guest paths (mkdir + rsync). Never `user@host:$HOME/...`.
    // Paths are absolute and space-free on Tart guests (`/Users/admin/...`).
    let aop = &layout.aop_root;
    ssh_run(
        guest,
        &format!("mkdir -p {aop}/engagement {aop}/bin {aop}/modules {aop}/targets {aop}/results"),
    )?;
    rsync_to_guest(guest, &host_eng, &format!("{}/", layout.engage))?;
    if bin_src.is_some() {
        rsync_to_guest(guest, &host_stage.join("bin/anubis"), &layout.bin)?;
        ssh_run(
            guest,
            &format!("chmod +x {}", shell_single_quote(&layout.bin)),
        )?;
    }

    // 5) Verify engagement.json is loadable on guest.
    let verify = format!(
        "test -f {ej} || {{ echo ANUBIS_VZ_SYNC_VERIFY: missing engagement.json; exit 2; }}",
        ej = shell_single_quote(&format!("{}/engagement.json", layout.engage))
    );
    ssh_run(guest, &verify)?;

    Ok(layout)
}

/// Recursively copy `src` dir into `dst` (merge; overwrites files).
fn copy_dir_merge(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_merge(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &to)
                .with_context(|| format!("copy {} → {}", entry.path().display(), to.display()))?;
        }
    }
    Ok(())
}

/// Resolve a host fuzz/exploit target path (cwd-relative or workspace-relative).
fn resolve_host_target(target: &str) -> Result<PathBuf> {
    let p = PathBuf::from(target);
    if p.is_file() {
        return Ok(p.canonicalize().unwrap_or(p));
    }
    if let Some(root) = workspace_root() {
        let cand = PathBuf::from(root).join(target);
        if cand.is_file() {
            return Ok(cand.canonicalize().unwrap_or(cand));
        }
    }
    Err(anyhow!(
        "ANUBIS_POC_TARGET_MISSING: `{target}` is not a file on the host \
         (pass a host path such as `poc_kit/bin/vuln_local`; it is staged into the guest automatically)"
    ))
}

/// Shell prelude shared by guest AOP commands: set BIN + ENGAGE absolute paths.
fn guest_aop_prelude(layout: &GuestLayout) -> String {
    format!(
        r#"set -euo pipefail
export ANUBIS_VZ_GUEST=1 ANUBIS_OFFENSIVE_GATE_IN_GUEST=1
AOP={aop}
ENGAGE={engage}
if [ -x {bin} ]; then BIN={bin}
elif command -v anubis >/dev/null 2>&1; then BIN=$(command -v anubis)
elif [ -x "$HOME/anubis-lang/target/release/anubis" ]; then BIN="$HOME/anubis-lang/target/release/anubis"
else echo "ANUBIS_VZ_NO_GUEST_BIN: stage host target/release/anubis via vz-sync"; exit 127
fi
test -f "$ENGAGE/engagement.json" || {{
  echo "ANUBIS_ENGAGE_LOAD: $ENGAGE (missing engagement.json — run vz-sync first)"; exit 2
}}
"#,
        aop = shell_single_quote(&layout.aop_root),
        engage = shell_single_quote(&layout.engage),
        bin = shell_single_quote(&layout.bin),
    )
}

/// Sync engagement workspace into the guest (rsync over Tart SSH).
///
/// Guest layout (absolute, always):
///   `$HOME/anubis-offensive/engagement/engagement.json`
///   `$HOME/anubis-offensive/bin/anubis`   (host release binary, when available)
pub fn vz_sync_engagement(
    eng: &Engagement,
    engage_dir: &Path,
    guest: &str,
    project_root: &Path,
) -> Result<PathBuf> {
    if legacy_vmctl_enabled() {
        return vz_sync_engagement_legacy(eng, engage_dir, guest, project_root);
    }
    let layout = ensure_guest_workspace(eng, engage_dir, guest, project_root)?;
    // Host-side mirror path (exports tree) for operators inspecting the stage.
    let host_eng = find_exports_host_path(guest)?.join("engagement");
    eprintln!(
        "[vz-sync] guest {} → {} (host stage {})",
        guest,
        layout.engage,
        host_eng.display()
    );
    Ok(host_eng)
}

fn vz_sync_engagement_legacy(
    eng: &Engagement,
    _engage_dir: &Path,
    guest: &str,
    project_root: &Path,
) -> Result<PathBuf> {
    let exports_base = find_exports_host_path(guest)?;
    let dest = exports_base.join("engagement");
    fs::create_dir_all(&dest)?;
    let eng_json = serde_json::to_string_pretty(eng)?;
    fs::write(dest.join("engagement.json"), &eng_json)?;
    for sub in ["agents", "tasks", "loot", "evidence", "modules", "packs"] {
        fs::create_dir_all(dest.join(sub))?;
    }
    let src_dest = exports_base.join("src");
    if !src_dest.exists() && project_root.join("Cargo.toml").exists() {
        let output = run_vmctl(&["sync", "--name", guest])?;
        if !output.status.success() {
            eprintln!(
                "vz sync warning: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(dest)
}

fn find_exports_host_path(guest: &str) -> Result<PathBuf> {
    if let Some(root) = workspace_root() {
        let p = PathBuf::from(&root)
            .join("vm/exports")
            .join(guest)
            .join("anubis-offensive");
        if p.exists() {
            return Ok(p);
        }
        fs::create_dir_all(&p)?;
        return Ok(p);
    }
    Err(anyhow!(
        "ANUBIS_VZ_NO_EXPORTS: cannot locate host exports for `{guest}`"
    ))
}

/// Run an exploit module inside the VZ guest (crash-isolated).
pub fn vz_exploit_run(
    eng: &Engagement,
    engage_dir: &Path,
    guest: &str,
    module_path: &Path,
    out: &Path,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let layout = ensure_guest_workspace(eng, engage_dir, guest, Path::new("."))?;
    if !module_path.is_file() {
        bail!("ANUBIS_VZ_MODULE_MISSING: {}", module_path.display());
    }
    let remote_mod = format!("{}/modules/current_exploit.json", layout.aop_root);
    rsync_to_guest(guest, module_path, &remote_mod)?;
    let cmd = format!(
        "{prelude}
mkdir -p \"$AOP/results/exploit_run\"
\"$BIN\" exploit-run --engage \"$ENGAGE\" \
  --module {module} \
  --out \"$AOP/results/exploit_run\"
",
        prelude = guest_aop_prelude(&layout),
        module = shell_single_quote(&remote_mod),
    );
    let result = vz_exec(guest, &cmd, Some(&layout.home), DEFAULT_TIMEOUT_SECS)?;
    fs::create_dir_all(out)?;
    fs::write(
        out.join("vz_exec_meta.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

/// Run a fuzz campaign inside the VZ guest.
///
/// `target` is a **host** path (e.g. `poc_kit/bin/vuln_local`). It is staged into the guest
/// under `$HOME/anubis-offensive/targets/<basename>` before `anubis fuzz` runs.
pub fn vz_fuzz(
    eng: &Engagement,
    engage_dir: &Path,
    guest: &str,
    target: &str,
    runs: u32,
    seed: Option<u64>,
    out: &Path,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let layout = ensure_guest_workspace(eng, engage_dir, guest, Path::new("."))?;
    let host_target = resolve_host_target(target)?;
    let base = host_target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("target.bin");
    let remote_target = format!("{}/targets/{base}", layout.aop_root);
    rsync_to_guest(guest, &host_target, &remote_target)?;
    ssh_run(
        guest,
        &format!("chmod +x {}", shell_single_quote(&remote_target)),
    )?;
    let seed_flag = seed.map(|s| format!("--seed {s}")).unwrap_or_default();
    let cmd = format!(
        "{prelude}
mkdir -p \"$AOP/results/fuzz_run\"
\"$BIN\" fuzz --target {target} --runs {runs} {seed_flag} \
  --out \"$AOP/results/fuzz_run\"
",
        prelude = guest_aop_prelude(&layout),
        target = shell_single_quote(&remote_target),
    );
    let result = vz_exec(guest, &cmd, Some(&layout.home), DEFAULT_TIMEOUT_SECS)?;
    fs::create_dir_all(out)?;
    fs::write(
        out.join("vz_exec_meta.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

/// Build and test an agent binary inside the VZ guest.
pub fn vz_agent_test(
    eng: &Engagement,
    engage_dir: &Path,
    guest: &str,
    agent_name: &str,
    sleep_ms: u64,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let layout = ensure_guest_workspace(eng, engage_dir, guest, Path::new("."))?;
    let cmd = format!(
        "{prelude}
\"$BIN\" agent-generate \
  --engage \"$ENGAGE\" \
  --name {agent_name} --os \"$(uname -s | tr '[:upper:]' '[:lower:]')\" --sleep-ms {sleep_ms}
echo 'AGENT_BUILD_OK'
ls -la \"$ENGAGE/agents/{agent_name}\"
file \"$ENGAGE/agents/{agent_name}\"
",
        prelude = guest_aop_prelude(&layout),
    );
    vz_exec(guest, &cmd, Some(&layout.home), DEFAULT_TIMEOUT_SECS)
}

/// Return a guest-cycle copy of the engagement with ports/paths that do not
/// collide with macOS guest defaults (notably mDNS on UDP 5353). This copy is
/// staged only into the guest workspace; the operator's host engagement stays
/// untouched.
fn prepare_vz_c2_cycle_engagement(eng: &Engagement, agent_name: &str) -> Engagement {
    let mut cycle = eng.clone();
    let digest = Sha256::digest(format!("{}:{agent_name}", eng.engagement_id).as_bytes());
    let dns_port = 55_000u16 + (u16::from_be_bytes([digest[0], digest[1]]) % 5_000);
    let agent_fragment: String = agent_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let suffix = hex::encode(&digest[..4]);
    cycle.dns_bind = format!("127.0.0.1:{dns_port}");
    cycle.uds_path = format!("/tmp/anubis-aop-vz-c2-{agent_fragment}-{suffix}.sock");
    // The wrapper verifies the plain HTTP loopback C2 lifecycle. mTLS has its own
    // dedicated gate; keeping this false avoids requiring curl client cert flags here.
    cycle.mtls_listen = false;
    cycle.rehash();
    cycle
}

fn build_vz_c2_cycle_script(
    eng: &Engagement,
    layout: &GuestLayout,
    agent_name: &str,
    tasks: &[(&str, &str)],
) -> String {
    let mut task_lines = String::new();
    for (module, operator) in tasks {
        task_lines.push_str(&format!(
            "\"$BIN\" task-queue --engage \"$ENGAGE\" --agent-id '*' --module {module} --operator {operator}\n",
            module = shell_single_quote(module),
            operator = shell_single_quote(operator),
        ));
    }
    let c2_base = format!("http://{}", eng.c2_bind);
    // Notes:
    // - GET /results returns {"results":[...]} (no top-level ok). Success is a
    //   TaskResult with "ok":true inside the array once the agent posts.
    // - Cleanup must not rely on shell globs (zsh nomatch / bash nullglob).
    // - Wait for agent registration before queueing tasks so the first post-queue
    //   beacon drains the inbox.
    format!(
        r#"{prelude}
C2_BASE={c2_base}
AGENT_NAME={agent_name}
cleanup() {{
  kill "${{APID:-}}" "${{LPID:-}}" 2>/dev/null || true
  wait 2>/dev/null || true
}}
trap cleanup EXIT
pkill -f 'anubis listen' 2>/dev/null || true
# Kill every staged agent binary, not only this cycle's name — a leftover
# beaconing agent drains the '*' task queue and swallows results.
pkill -f "$ENGAGE/agents/" 2>/dev/null || true
sleep 0.3
rm -f "$ENGAGE/tasks/inbox.jsonl"
if [ -d "$ENGAGE/loot" ]; then
  find "$ENGAGE/loot" -maxdepth 1 -type f -name 'result-*.json' -delete 2>/dev/null || true
fi
find /tmp -maxdepth 1 -type s -name 'anubis-aop-vz-c2-*.sock' -delete 2>/dev/null || true
find /tmp -maxdepth 1 -type f -name 'anubis-aop-vz-c2-*.sock' -delete 2>/dev/null || true
# nohup + closed stdin: keep listen/agent alive for the full SSH session
# (plain `&` without nohup failed to deliver task results in dogfood runs).
nohup "$BIN" listen --engage "$ENGAGE" >/tmp/anubis-vz-c2-listen.out 2>/tmp/anubis-vz-c2-listen.err </dev/null &
LPID=$!
HEALTH='{{}}'
for i in $(seq 1 50); do
  sleep 0.2
  HEALTH=$(curl -fsS "$C2_BASE/health" 2>/dev/null || true)
  if printf '%s' "$HEALTH" | grep -q '"ok":true'; then
    break
  fi
done
if ! printf '%s' "$HEALTH" | grep -q '"ok":true'; then
  echo "ANUBIS_VZ_C2_LISTENER_NOT_READY base=$C2_BASE health=$HEALTH"; exit 3
fi
"$BIN" agent-generate --engage "$ENGAGE" --name "$AGENT_NAME" --os "$(uname -s | tr '[:upper:]' '[:lower:]')" --sleep-ms 300 2>&1
test -x "$ENGAGE/agents/$AGENT_NAME" || {{ echo "ANUBIS_VZ_C2_AGENT_MISSING"; exit 6; }}
# Pre-queue against wildcard so the agent's first beacon drains work immediately.
# (Post-start queue left tasks stranded in dogfood runs when the agent only
# re-beacons after a full sleep cycle under load.)
{task_lines}
nohup "$ENGAGE/agents/$AGENT_NAME" >/tmp/anubis-vz-c2-agent.out 2>/tmp/anubis-vz-c2-agent.err </dev/null &
APID=$!
AGENTS='{{}}'
RESULTS='{{}}'
for i in $(seq 1 50); do
  sleep 0.4
  RESULTS=$(curl -fsS "$C2_BASE/results" 2>/dev/null || true)
  AGENTS=$(curl -fsS "$C2_BASE/agents" 2>/dev/null || true)
  echo "POLL_$i results=$RESULTS"
  if printf '%s' "$RESULTS" | grep -q '"ok":true' && printf '%s' "$AGENTS" | grep -q '"agent_id"'; then
    break
  fi
done
echo "===RESULTS==="
echo "$RESULTS"
echo "===AGENTS==="
echo "$AGENTS"
if ! printf '%s' "$AGENTS" | grep -q '"agent_id"'; then
  echo "ANUBIS_VZ_C2_NO_AGENTS agent_err=$(cat /tmp/anubis-vz-c2-agent.err 2>/dev/null || true)"; exit 4
fi
if ! printf '%s' "$RESULTS" | grep -q '"ok":true'; then
  echo "ANUBIS_VZ_C2_NO_RESULTS agent_err=$(cat /tmp/anubis-vz-c2-agent.err 2>/dev/null || true) listen_err=$(tail -20 /tmp/anubis-vz-c2-listen.err 2>/dev/null || true) inbox=$(cat \"$ENGAGE/tasks/inbox.jsonl\" 2>/dev/null || true)"; exit 5
fi
"#,
        prelude = guest_aop_prelude(layout),
        c2_base = shell_single_quote(&c2_base),
        agent_name = shell_single_quote(agent_name),
    )
}

/// Run the full C2 cycle inside the VZ guest.
pub fn vz_c2_cycle(
    eng: &Engagement,
    engage_dir: &Path,
    guest: &str,
    agent_name: &str,
    tasks: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    // Stage the operator engagement first (absolute guest layout + host anubis).
    let layout = ensure_guest_workspace(eng, engage_dir, guest, Path::new("."))?;
    // Then overlay the cycle-local engagement (non-mDNS DNS port, unique UDS) as a
    // single-file replace. Avoid re-rsyncing the whole tree after prepare — a full
    // re-stage has been observed to leave a guest C2 that accepts beacons but never
    // surfaces task results.
    let cycle_eng = prepare_vz_c2_cycle_engagement(eng, agent_name);
    let host_eng_json = std::env::temp_dir().join(format!(
        "anubis-vz-c2-eng-{}-{}.json",
        guest,
        &hex::encode(Sha256::digest(agent_name.as_bytes()))[..8]
    ));
    fs::write(
        &host_eng_json,
        serde_json::to_string_pretty(&cycle_eng)?,
    )?;
    let remote_eng = format!("{}/engagement.json", layout.engage);
    rsync_to_guest(guest, &host_eng_json, &remote_eng)?;
    let _ = fs::remove_file(&host_eng_json);

    let script = build_vz_c2_cycle_script(&cycle_eng, &layout, agent_name, tasks);
    let host_script = std::env::temp_dir().join(format!(
        "anubis-vz-c2-{}-{}.sh",
        guest,
        &hex::encode(Sha256::digest(agent_name.as_bytes()))[..8]
    ));
    fs::write(
        &host_script,
        format!("#!/bin/bash\nset -euo pipefail\n{script}\n"),
    )?;
    let remote_script = format!("{}/results/vz_c2_cycle.sh", layout.aop_root);
    rsync_to_guest(guest, &host_script, &remote_script)?;
    let _ = fs::remove_file(&host_script);
    let cmd = format!(
        "chmod +x {rs} && bash {rs}",
        rs = shell_single_quote(&remote_script),
    );
    vz_exec(guest, &cmd, Some(&layout.home), timeout_secs)
}

/// Run the Anubis unit test suite inside the VZ guest.
pub fn vz_test_suite(guest: &str, filter: Option<&str>) -> Result<VzExecResult> {
    let filter_arg = filter.map(|f| format!("-- {f}")).unwrap_or_default();
    let cmd = format!(
        "export CARGO_TARGET_DIR=/tmp/target/anubis-offensive && \
         cd \"$HOME/anubis-lang\" && \
         cargo test --release -p anubis --offline {filter_arg}"
    );
    vz_exec(guest, &cmd, Some("$HOME"), DEFAULT_TIMEOUT_SECS)
}

/// Full stress battery: host-orchestrated disposable-guest gate (the real 34/34 path).
///
/// This is the Anubis-shaped answer to `vz-stress`: do not pretend a missing shell
/// script is a battery. Drive `scripts/run_offensive_platform_gate.sh`, which already
/// creates a Tart disposable guest, runs the sealed suite, and writes isolation evidence.
pub fn vz_stress_battery(eng: &Engagement, guest: &str, engage_dir: &Path) -> Result<VzExecResult> {
    eng.validate_live()?;
    let start = SystemTime::now();
    let root = workspace_root().ok_or_else(|| {
        anyhow!("ANUBIS_VZ_NO_WORKSPACE: cannot locate anubis-lang root for stress battery")
    })?;
    let script = PathBuf::from(&root).join("scripts/run_offensive_platform_gate.sh");
    if !script.is_file() {
        return Err(anyhow!(
            "ANUBIS_VZ_NO_STRESS_SCRIPT: {} not found — the stress battery is the disposable-guest \
             offensive gate (`scripts/run_offensive_platform_gate.sh`). Clone a complete tree or \
             run: bash scripts/run_offensive_platform_gate.sh --out out/offensive_gate",
            script.display()
        ));
    }
    let out_dir = engage_dir.join("loot/vz-stress");
    fs::create_dir_all(&out_dir)?;
    // Prefer host release binary for the gate orchestration.
    let host_bin = PathBuf::from(&root).join("target/release/anubis");
    let mut cmd = Command::new("bash");
    cmd.arg(&script)
        .arg("--out")
        .arg(&out_dir)
        .env("ANUBIS_WORKSPACE_ROOT", &root)
        .env(
            "ANUBIS_BIN",
            if host_bin.is_file() {
                host_bin.display().to_string()
            } else {
                String::new()
            },
        )
        // Gate uses golden base; guest name hint is informational for meta.
        .env("ANUBIS_VZ_STRESS_GUEST_HINT", guest);
    let output = cmd
        .output()
        .with_context(|| format!("ANUBIS_VZ_STRESS: failed to spawn {}", script.display()))?;
    let duration = start.elapsed().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}{stderr}");
    let evidence_hash = hex::encode(Sha256::digest(combined.as_bytes()));
    let result = VzExecResult {
        guest: guest.into(),
        command: format!("bash {} --out {}", script.display(), out_dir.display()),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        duration_ms: duration.as_millis() as u64,
        network: VzNetwork::Off,
        evidence_hash,
        backend: "tart-disposable-guest-gate".into(),
    };
    fs::write(
        out_dir.join("vz_stress_meta.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

/// Comprehensive VZ doctor — readiness for offensive sandboxing.
///
/// **Canonical backend is Tart** (`anubis vz` / top-level `vz-*` → Tart).
pub fn vz_doctor() -> Result<serde_json::Value> {
    let tart_available = tart_bin().is_ok();
    let tart_has_golden = if tart_available {
        tart_capture(&["list"])
            .ok()
            .map(|s| {
                s.lines()
                    .any(|l| l.split_whitespace().any(|t| t == "anubis-xcode"))
            })
            .unwrap_or(false)
    } else {
        false
    };
    let ssh_key = dirs::home_dir()
        .map(|h| h.join(".ssh/tart_anubis"))
        .filter(|p| p.is_file());

    let guests = if tart_available {
        vz_status().unwrap_or_default()
    } else {
        Vec::new()
    };
    let running: Vec<_> = guests.iter().filter(|g| g.running).collect();
    // The golden guest specifically — `vz-agent-test` / `vz-c2-cycle` / `vz-exec` run INSIDE it and
    // fail ANUBIS_VZ_GUEST_NOT_RUNNING when it is stopped, regardless of how many other guests are up.
    let golden_running = guests.iter().any(|g| g.name == "anubis-xcode" && g.running);
    let mut guest_list = Vec::new();
    for g in &guests {
        guest_list.push(serde_json::json!({
            "name": g.name,
            "running": g.running,
            "cpus": g.cpu_count,
            "memory_mib": g.memory_mib,
            "disk_gib": g.disk_gib,
            "distribution": g.distribution,
            "network": g.network,
            "backend": g.backend,
        }));
    }

    let exports_path =
        workspace_root().map(|r| PathBuf::from(r).join("vm/exports/anubis-xcode/anubis-offensive"));
    let exports_exist = exports_path.as_ref().map(|p| p.exists()).unwrap_or(false);
    let toolchain_staged = exports_path
        .as_ref()
        .map(|p| p.join("toolchain").exists())
        .unwrap_or(false);
    let binary_staged = exports_path
        .as_ref()
        .map(|p| p.join("src/Cargo.toml").exists())
        .unwrap_or(false);

    let gate_script = workspace_root()
        .map(|r| PathBuf::from(r).join("scripts/run_offensive_platform_gate.sh"))
        .filter(|p| p.is_file());

    let offensive_ready = tart_available && tart_has_golden && ssh_key.is_some();
    let vmctl_enabled = legacy_vmctl_enabled();
    let vmctl_path = vmctl_bin();
    let vmctl_exists = vmctl_path.exists()
        || Command::new("which")
            .arg(vmctl_path.to_str().unwrap_or("vmctl"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

    Ok(serde_json::json!({
        "canonical_backend": "tart",
        "vz_available": tart_available,
        "tart_available": tart_available,
        "tart_golden_anubis_xcode": tart_has_golden,
        "tart_ssh_key": ssh_key.as_ref().map(|p| p.display().to_string()),
        "tart_ssh_key_present": ssh_key.is_some(),
        "legacy_vmctl": {
            "present": vmctl_exists,
            "path": vmctl_path.display().to_string(),
            "enabled": vmctl_enabled,
            "classification": "LEGACY_NON_AUTHORITATIVE",
            "note": "Top-level vz-* uses Tart by default. ANUBIS_ALLOW_LEGACY_VMCTL=1 is migration-only.",
        },
        "offensive_guest_ready": offensive_ready,
        "running_guests": running.len(),
        "total_guests": guests.len(),
        "guests": guest_list,
        "exports_path": exports_path.as_ref().map(|p| p.display().to_string()),
        "exports_exist": exports_exist,
        "toolchain_staged": toolchain_staged,
        "binary_staged": binary_staged,
        "stress_gate_script": gate_script.as_ref().map(|p| p.display().to_string()),
        "stress_gate_script_present": gate_script.is_some(),
        "default_network": "off",
        // Each capability reports the predicate that actually governs the command it names.
        //
        // These all previously reported `offensive_ready` — "tart exists, the golden image exists, a
        // key exists" — which answers whether the SUBSTRATE is present, not whether the command
        // works. That conflation is why the doctor reported `agent_test: true` while `vz-agent-test`
        // exited 1. A doctor that reports a non-working command healthy is worse than the broken
        // command: it converts a known gap into a false assurance, and everything downstream cites
        // it.
        //
        // The distinction that matters here is SUBSTRATE vs RUNNING GUEST. Commands that only
        // orchestrate the host (status, start/stop, snapshot) need tart. Commands that execute
        // INSIDE a guest need one booted — `vz-agent-test` and `vz-c2-cycle` fail
        // ANUBIS_VZ_GUEST_NOT_RUNNING without it, so reporting them `true` on a stopped guest is
        // exactly the lie being removed.
        "capabilities": {
            "status": tart_available,
            "start_stop": tart_available,
            "snapshot": tart_available,
            // Host-side orchestration: needs the substrate, not a booted guest.
            "sync": offensive_ready,
            // Spin their OWN disposable guest, so a stopped golden image is fine.
            "exploit_sandbox": offensive_ready,
            "fuzz_sandbox": offensive_ready,
            // Execute inside the RUNNING golden guest.
            "exec": offensive_ready && golden_running,
            "agent_test": offensive_ready && golden_running,
            "c2_cycle": offensive_ready && golden_running,
            "unit_tests": offensive_ready && golden_running,
            "stress_battery": offensive_ready && gate_script.is_some(),
            "golden_guest_running": golden_running,
            "requires": "tart + anubis-xcode + ~/.ssh/tart_anubis",
            "requires_running_guest": "exec, agent_test, c2_cycle, unit_tests (`anubis vz-start`)",
        },
        "policy": {
            "network_default": "off",
            "crash_isolated": true,
            // NOT collected. The tart path stages files into the guest and runs commands there, but
            // nothing scrapes results back or seals an action into the engagement receipt chain —
            // there is no `seal_action` / loot-collection call on this path at all. Measured
            // directly: `vz exploit` and `vz fuzz` produced a SIGABRT and 14 unique crashes inside
            // disposable guests and left `receipt-verify` byte-identical, while `campaign-init`,
            // which only writes a Markdown file, advanced the chain. Hardcoding `true` here claimed
            // the opposite of what the lane measurably does.
            "evidence_collected": false,
            "evidence_gap": "tart path stages and executes but does not scrape results or seal receipts",
            "host_never_executes_payloads": true,
            "canonical_cli": "anubis vz status|run|exec|exploit|fuzz  (and top-level vz-* aliases)",
            "stress_is": "scripts/run_offensive_platform_gate.sh (disposable Tart guest, 34-check battery)",
        },
    }))
}

/// Snapshot a VZ guest by cloning it (Tart has no in-place snapshot; CoW clone is the model).
pub fn vz_snapshot(guest: &str, label: &str) -> Result<()> {
    if legacy_vmctl_enabled() {
        let output = run_vmctl(&["snapshot", "--name", guest, "--label", label])?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("ANUBIS_VZ_SNAPSHOT: {err}"));
        }
        return Ok(());
    }
    // Snapshot name: guest-label (must be a valid Tart VM name).
    let snap_name = format!("{guest}-{label}");
    tart_capture(&["clone", guest, &snap_name]).map(|_| ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_vmctl_fails_closed_without_allow_env() {
        std::env::remove_var("ANUBIS_ALLOW_LEGACY_VMCTL");
        let err = run_vmctl(&["status", "--json"]).unwrap_err().to_string();
        assert!(err.contains("ANUBIS_VZ_LEGACY_VMCTL_DISABLED"), "got {err}");
    }

    #[test]
    fn doctor_reports_tart_as_canonical() {
        // Does not require tart to be installed — just that the JSON shape is honest.
        if let Ok(report) = vz_doctor() {
            assert_eq!(report["canonical_backend"], "tart");
            assert!(report["policy"]["canonical_cli"]
                .as_str()
                .unwrap()
                .contains("anubis vz"));
            // Capabilities that used to be vmctl-only must not claim ready without tart substrate.
            if report["tart_available"] == false {
                assert_eq!(report["capabilities"]["agent_test"], false);
                assert_eq!(report["capabilities"]["c2_cycle"], false);
            }
        }
    }

    #[test]
    fn c2_cycle_script_queries_the_engagement_http_bind_and_requires_observations() {
        let mut eng = Engagement::default_lab("unit-c2-cycle", "unit-test authorization");
        eng.c2_bind = "127.0.0.1:45454".into();
        let layout = GuestLayout {
            home: "/Users/admin".into(),
            aop_root: "/Users/admin/anubis-offensive".into(),
            engage: "/Users/admin/anubis-offensive/engagement".into(),
            bin: "/Users/admin/anubis-offensive/bin/anubis".into(),
        };

        let script =
            build_vz_c2_cycle_script(&eng, &layout, "unit-agent", &[("whoami", "operator")]);

        assert!(
            script.contains("C2_BASE='http://127.0.0.1:45454'"),
            "{script}"
        );
        assert!(script.contains("\"$C2_BASE/results\""), "{script}");
        assert!(script.contains("\"$C2_BASE/agents\""), "{script}");
        assert!(!script.contains("127.0.0.1:14444"), "{script}");
        assert!(script.contains("ANUBIS_VZ_C2_NO_RESULTS"), "{script}");
        assert!(script.contains("ANUBIS_VZ_C2_NO_AGENTS"), "{script}");
        // No bare shell-expanded globs (zsh nomatch). find -name 'result-*.json' is OK.
        assert!(
            !script.contains("/loot/result-*.json") && !script.contains("loot/result-*.json\""),
            "use find -name, not shell globs: {script}"
        );
        assert!(
            script.contains("find \"$ENGAGE/loot\""),
            "expected find-based loot cleanup: {script}"
        );
        assert!(
            script.contains("NO_AGENTS_BEFORE_QUEUE") || script.contains("agent_id"),
            "{script}"
        );
    }

    #[test]
    fn c2_cycle_engagement_uses_non_mdns_dns_port_and_rehashes() {
        let mut eng = Engagement::default_lab("unit-c2-cycle-dns", "unit-test authorization");
        eng.dns_bind = "127.0.0.1:5353".into();
        let prepared = prepare_vz_c2_cycle_engagement(&eng, "unit-agent");

        assert_ne!(prepared.dns_bind, "127.0.0.1:5353");
        assert!(prepared.dns_bind.starts_with("127.0.0.1:"));
        assert!(prepared.uds_path.contains("unit-agent"));
        prepared
            .verify_content_hash()
            .expect("prepared engagement should be sealed");
    }

    #[test]
    fn ssh_remote_bash_lc_is_single_argv_wrapping_full_script() {
        let remote = ssh_remote_bash_lc("cd \"$HOME\" && echo hi");
        assert!(
            remote.starts_with("exec bash -lc "),
            "got {remote}"
        );
        // Must be one token for SSH: no unquoted multi-argv bash -lc split.
        assert!(
            !remote.contains("bash -lc cd "),
            "unquoted multi-argv form regresses zsh split: {remote}"
        );
        assert!(remote.contains("echo hi"), "{remote}");
        // Script body is single-quoted for the remote shell.
        assert!(remote.contains("'cd \"$HOME\" && echo hi'"), "{remote}");
    }

    #[test]
    fn guest_cd_prefix_expands_home_instead_of_literal_dollar_home() {
        assert_eq!(guest_cd_prefix("$HOME"), "cd \"$HOME\"");
        assert_eq!(guest_cd_prefix("~"), "cd \"$HOME\"");
        assert_eq!(
            guest_cd_prefix("/Users/admin"),
            "cd '/Users/admin'"
        );
    }

    #[test]
    fn dump_c2_cycle_script_for_manual_replay() {
        let mut eng = Engagement::default_lab("dump-c2", "unit-test authorization");
        eng.c2_bind = "127.0.0.1:4444".into();
        let layout = GuestLayout {
            home: "/Users/admin".into(),
            aop_root: "/Users/admin/anubis-offensive".into(),
            engage: "/Users/admin/anubis-offensive/engagement".into(),
            bin: "/Users/admin/anubis-offensive/bin/anubis".into(),
        };
        let script =
            build_vz_c2_cycle_script(&eng, &layout, "dumpreplay", &[("whoami", "operator")]);
        let path = "/tmp/aop-stagefix-20260727_083802/dumped_c2_script.sh";
        let _ = std::fs::create_dir_all("/tmp/aop-stagefix-20260727_083802");
        std::fs::write(path, format!("#!/bin/bash\n{script}\n")).unwrap();
        assert!(std::path::Path::new(path).is_file());
    }
}

mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}
