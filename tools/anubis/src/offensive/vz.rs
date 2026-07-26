//! Apple Virtualization.framework integration for sandboxed offensive execution.
//!
//! Every high-risk offensive operation (exploit, fuzz, agent test, lateral probe)
//! can run inside a network-isolated VZ guest. The host never executes untrusted
//! payloads. Crash isolation, network isolation, and evidence collection happen
//! at the hypervisor boundary.
//!
//! T8 — VZ sandbox integration.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const VMCTL_ENV: &str = "ANUBIS_VMCTL_PATH";
const DEFAULT_VMCTL: &str = "vmctl";
const WORKSPACE_ROOT_ENV: &str = "ANUBIS_WORKSPACE_ROOT";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;
const EXPORTS_PREFIX: &str = "/exports/anubis-offensive";

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

fn vmctl_bin() -> PathBuf {
    if let Ok(p) = std::env::var(VMCTL_ENV) {
        return PathBuf::from(p);
    }
    if let Ok(p) = which_vmctl() {
        return p;
    }
    PathBuf::from(DEFAULT_VMCTL)
}

fn which_vmctl() -> Result<PathBuf> {
    let candidates = [dirs::home_dir()
        .unwrap_or_default()
        .join(".local/bin/vmctl")];
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }
    let output = Command::new("which")
        .arg("vmctl")
        .output()
        .map_err(|e| anyhow!("vmctl lookup: {e}"))?;
    if output.status.success() {
        let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    Err(anyhow!("ANUBIS_VZ_NO_VMCTL: vmctl not found"))
}

fn workspace_root() -> Option<String> {
    if let Ok(root) = std::env::var(WORKSPACE_ROOT_ENV) {
        if !root.is_empty() {
            return Some(root);
        }
    }
    // Anubis Lang tree only (cwd walk, then known install path).
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

fn run_vmctl(args: &[&str]) -> Result<std::process::Output> {
    // Canonical isolation is Tart via `anubis vz` (tools/anubis/src/vz.rs).
    // Legacy vmctl execution is fail-closed unless explicitly re-enabled for
    // migration diagnostics only — never for isolation evidence.
    if std::env::var("ANUBIS_ALLOW_LEGACY_VMCTL").ok().as_deref() != Some("1") {
        return Err(anyhow!(
            "ANUBIS_VZ_LEGACY_VMCTL_DISABLED: offensive vmctl path is non-authoritative and disabled. \
             Use `anubis vz status|exploit|fuzz|exec|sync` (tart / Apple Virtualization.framework). \
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
    let output = cmd
        .output()
        .map_err(|e| anyhow!("ANUBIS_VZ_VMCTL_SPAWN: {}: {e}", bin.display()))?;
    Ok(output)
}

/// Query the status of all VZ guests.
pub fn vz_status() -> Result<Vec<VzGuest>> {
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
            "ANUBIS_VZ_GUEST_NOT_RUNNING: `{name}` is not running"
        ));
    }
    // Prefer anubis-xcode golden base, then any running guest
    if let Some(g) = guests
        .iter()
        .find(|g| g.name == "anubis-xcode" && g.running)
    {
        return Ok(g.clone());
    }
    guests
        .into_iter()
        .find(|g| g.running)
        .ok_or_else(|| anyhow!("ANUBIS_VZ_NO_RUNNING_GUEST: no running VZ guest found"))
}

/// Start a VZ guest if not already running.
pub fn vz_start(name: &str, network: &VzNetwork) -> Result<()> {
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
    Ok(())
}

/// Stop a VZ guest.
pub fn vz_stop(name: &str) -> Result<()> {
    let output = run_vmctl(&["stop", name])?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ANUBIS_VZ_STOP: {err}"));
    }
    Ok(())
}

/// Execute a command inside a VZ guest. Network-isolated by default.
pub fn vz_exec(
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
    })
}

/// Sync engagement workspace sources into the VZ guest exports.
pub fn vz_sync_engagement(
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
    // Sync key source files for in-guest builds
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

/// Find the host-side exports directory for a given guest.
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
    _engage_dir: &Path,
    guest: &str,
    module_path: &Path,
    out: &Path,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let module_json = fs::read_to_string(module_path)?;
    let exports = find_exports_host_path(guest)?;
    let guest_module = exports.join("modules/current_exploit.json");
    fs::create_dir_all(exports.join("modules"))?;
    fs::write(&guest_module, &module_json)?;
    let cmd = format!(
        "cd {prefix}/src && {prefix}/bin/anubis exploit-run \
         --engage {prefix}/engagement \
         --module {prefix}/modules/current_exploit.json \
         --out {prefix}/results/exploit_run",
        prefix = EXPORTS_PREFIX,
    );
    let result = vz_exec(guest, &cmd, None, DEFAULT_TIMEOUT_SECS)?;
    fs::create_dir_all(out)?;
    let results_host = exports.join("results/exploit_run");
    if results_host.exists() {
        for entry in fs::read_dir(&results_host)? {
            let entry = entry?;
            fs::copy(entry.path(), out.join(entry.file_name()))?;
        }
    }
    fs::write(
        out.join("vz_exec_meta.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

/// Run a fuzz campaign inside the VZ guest (crash-isolated, no egress).
pub fn vz_fuzz(
    eng: &Engagement,
    guest: &str,
    target: &str,
    runs: u32,
    seed: Option<u64>,
    out: &Path,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let seed_flag = seed.map(|s| format!("--seed {s}")).unwrap_or_default();
    let cmd = format!(
        "cd {prefix}/src && {prefix}/bin/anubis fuzz \
         --target '{target}' --runs {runs} {seed_flag} \
         --out {prefix}/results/fuzz_run",
        prefix = EXPORTS_PREFIX,
    );
    let result = vz_exec(guest, &cmd, None, DEFAULT_TIMEOUT_SECS)?;
    fs::create_dir_all(out)?;
    let exports = find_exports_host_path(guest)?;
    let results_host = exports.join("results/fuzz_run");
    if results_host.exists() {
        for entry in fs::read_dir(&results_host)? {
            let entry = entry?;
            fs::copy(entry.path(), out.join(entry.file_name()))?;
        }
    }
    fs::write(
        out.join("vz_exec_meta.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

/// Build and test an agent binary inside the VZ guest.
pub fn vz_agent_test(
    eng: &Engagement,
    guest: &str,
    agent_name: &str,
    sleep_ms: u64,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let cmd = format!(
        "cd {prefix}/src && \
         {prefix}/bin/anubis agent-generate \
           --engage {prefix}/engagement \
           --name {agent_name} --os linux --sleep-ms {sleep_ms} && \
         echo 'AGENT_BUILD_OK' && \
         ls -la {prefix}/engagement/agents/{agent_name} && \
         file {prefix}/engagement/agents/{agent_name}",
        prefix = EXPORTS_PREFIX,
    );
    vz_exec(guest, &cmd, None, DEFAULT_TIMEOUT_SECS)
}

/// Run the full C2 cycle inside the VZ guest: listener + agent + task dispatch.
pub fn vz_c2_cycle(
    eng: &Engagement,
    guest: &str,
    agent_name: &str,
    tasks: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let mut task_lines = String::new();
    for (module, operator) in tasks {
        task_lines.push_str(&format!(
            "{prefix}/bin/anubis task-queue --engage {prefix}/engagement \
             --module {module} --operator {operator}\n",
            prefix = EXPORTS_PREFIX,
        ));
    }
    let cmd = format!(
        r#"cd {prefix}/src
{prefix}/bin/anubis listen --engage {prefix}/engagement &
LPID=$!
sleep 1.5
{prefix}/bin/anubis agent-generate --engage {prefix}/engagement --name {agent_name} --os linux --sleep-ms 600 2>&1
{prefix}/engagement/agents/{agent_name} &
APID=$!
sleep 1
{task_lines}
for i in $(seq 1 15); do
  sleep 0.6
  RES=$(curl -s http://127.0.0.1:14444/results 2>/dev/null || echo '{{}}')
  echo "$RES" | grep -q '"ok":true' && break
done
echo "===RESULTS==="
curl -s http://127.0.0.1:14444/results
echo ""
echo "===AGENTS==="
curl -s http://127.0.0.1:14444/agents
kill $APID 2>/dev/null; kill $LPID 2>/dev/null
wait 2>/dev/null
"#,
        prefix = EXPORTS_PREFIX,
    );
    vz_exec(guest, &cmd, None, timeout_secs)
}

/// Run the Anubis unit test suite inside the VZ guest.
pub fn vz_test_suite(guest: &str, filter: Option<&str>) -> Result<VzExecResult> {
    let filter_arg = filter.map(|f| format!("-- {f}")).unwrap_or_default();
    let cmd = format!(
        "export CARGO_TARGET_DIR=/tmp/target/anubis-offensive && \
         cd {prefix}/src && \
         cargo test --release -p anubis --offline {filter_arg}",
        prefix = EXPORTS_PREFIX,
    );
    vz_exec(guest, &cmd, None, DEFAULT_TIMEOUT_SECS)
}

/// Full stress battery inside VZ: engagement lifecycle, scope, RBAC, C2, crypto, exploit.
pub fn vz_stress_battery(
    eng: &Engagement,
    guest: &str,
    _engage_dir: &Path,
) -> Result<VzExecResult> {
    eng.validate_live()?;
    let script_path = find_exports_host_path(guest)?.join("run_stress_expanded.sh");
    if !script_path.exists() {
        return Err(anyhow!(
            "ANUBIS_VZ_NO_STRESS_SCRIPT: {} not found",
            script_path.display()
        ));
    }
    let cmd = format!(
        "bash {prefix}/run_stress_expanded.sh {prefix}/src {prefix}/results-expanded",
        prefix = EXPORTS_PREFIX,
    );
    vz_exec(guest, &cmd, None, DEFAULT_TIMEOUT_SECS)
}

/// Comprehensive VZ doctor — readiness for offensive sandboxing.
///
/// **Canonical backend is Tart** (`anubis vz *` → `tools/anubis/src/vz.rs`).
/// The vmctl path below is **LEGACY / non-authoritative** for isolation evidence.
pub fn vz_doctor() -> Result<serde_json::Value> {
    let tart_available = Command::new("tart")
        .arg("list")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let tart_has_golden = if tart_available {
        Command::new("tart")
            .arg("list")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().any(|l| l.split_whitespace().any(|t| t == "anubis-xcode")))
            .unwrap_or(false)
    } else {
        false
    };
    let ssh_key = dirs::home_dir()
        .map(|h| h.join(".ssh/tart_anubis"))
        .filter(|p| p.is_file());

    // Legacy vmctl probe — never used alone as vz_available for AOP isolation claims.
    let vmctl_path = vmctl_bin();
    let vmctl_exists = vmctl_path.exists()
        || Command::new("which")
            .arg(vmctl_path.to_str().unwrap_or("vmctl"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    let guests = if vmctl_exists {
        vz_status().unwrap_or_default()
    } else {
        Vec::new()
    };
    let running: Vec<_> = guests.iter().filter(|g| g.running).collect();
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
            "backend": "legacy_vmctl",
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

    // Authoritative readiness = tart + golden + SSH key (not vmctl).
    let offensive_ready = tart_available && tart_has_golden && ssh_key.is_some();
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
            "classification": "LEGACY_NON_AUTHORITATIVE",
            "note": "Do not use vmctl alone for isolation evidence; use anubis vz (tart).",
        },
        "offensive_guest_ready": offensive_ready,
        "running_guests_legacy_vmctl": running.len(),
        "total_guests_legacy_vmctl": guests.len(),
        "guests_legacy_vmctl": guest_list,
        "exports_path": exports_path.map(|p| p.display().to_string()),
        "exports_exist": exports_exist,
        "toolchain_staged": toolchain_staged,
        "binary_staged": binary_staged,
        "default_network": "off",
        "capabilities": {
            "exploit_sandbox": offensive_ready,
            "fuzz_sandbox": offensive_ready,
            "agent_test": offensive_ready,
            "c2_cycle": offensive_ready,
            "stress_battery": offensive_ready,
            "unit_tests": offensive_ready,
            "requires": "tart + anubis-xcode + ~/.ssh/tart_anubis",
        },
        "policy": {
            "network_default": "off",
            "crash_isolated": true,
            "evidence_collected": true,
            "host_never_executes_payloads": true,
            "canonical_cli": "anubis vz status|exploit|fuzz|exec|sync",
        },
    }))
}

/// Snapshot a VZ guest state for reproducible offensive testing.
pub fn vz_snapshot(guest: &str, label: &str) -> Result<()> {
    let output = run_vmctl(&["snapshot", "--name", guest, "--label", label])?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ANUBIS_VZ_SNAPSHOT: {err}"));
    }
    Ok(())
}

#[cfg(test)]
mod legacy_vmctl_tests {
    use super::*;

    #[test]
    fn run_vmctl_fails_closed_without_allow_env() {
        // Ensure flag is unset for this process.
        std::env::remove_var("ANUBIS_ALLOW_LEGACY_VMCTL");
        let err = run_vmctl(&["status", "--json"]).unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_VZ_LEGACY_VMCTL_DISABLED"),
            "got {err}"
        );
    }
}

mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}
