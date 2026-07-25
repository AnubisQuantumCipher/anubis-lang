//! Isolation gate for AOP + PoC kit (advance without regression).
//!
//! # Two tiers
//!
//! ## A — Red-team / C2 platform (strict Apple Virtualization)
//! listen, agent, inject, lateral, exploit-run, engagement packer, etc.
//! Host → `ANUBIS_OFFENSIVE_HOST_FORBIDDEN`.
//!
//! ## B — Bounty PoC kit (existing Anubis surface, not regressed)
//! `anubis run --allow-research` and `anubis fuzz --target poc_kit/...` remain
//! the documented lab path from `docs/language/POC_KIT.md` and
//! `scripts/run_poc_kit_gate.sh`. Prefer `anubis vz exploit|fuzz` for primary
//! crash evidence; host gold-fixture runs are still valid lab smoke.
//!
//! Advance: fuzz against a **non–poc_kit** local target requires VZ (or
//! explicit `ANUBIS_POC_LAB_HOST=1`).
//!
//! Guest markers (any one — **explicit Anubis markers only**):
//! - `ANUBIS_VZ_GUEST=1`
//! - `ANUBIS_OFFENSIVE_GATE_IN_GUEST=1`
//! - `ANUBIS_ISOLATION` contains `tart` / `vz` / `virtualization`
//! - `/etc/anubis-vz-guest` or `$HOME/.anubis-vz-guest`
//!
//! **Not** a guest marker: `kern.hv_vmm_present=1` alone. GitHub Actions
//! `macos-latest` runners are themselves VMs and report hv_vmm_present=1; treating
//! that as "Anubis disposable guest" fail-opened AOP on public CI (G14 isolation
//! witness saw task-queue/recon-scan succeed with rc=0). Guest hops must set an
//! explicit Anubis marker (tart gate already exports `ANUBIS_VZ_GUEST=1`).

use anyhow::{anyhow, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

/// True when this process is inside an Anubis-managed VZ/tart guest.
pub fn in_vz_guest() -> bool {
    if env_truthy("ANUBIS_VZ_GUEST") || env_truthy("ANUBIS_OFFENSIVE_GATE_IN_GUEST") {
        return true;
    }
    if let Ok(iso) = std::env::var("ANUBIS_ISOLATION") {
        let l = iso.to_ascii_lowercase();
        if l.contains("tart") || l.contains("vz") || l.contains("virtualization") {
            return true;
        }
    }
    if Path::new("/etc/anubis-vz-guest").exists() {
        return true;
    }
    if let Some(home) = dirs_home() {
        if Path::new(&home).join(".anubis-vz-guest").exists() {
            return true;
        }
    }
    // Do NOT treat kern.hv_vmm_present alone as guest membership — CI runners
    // and many developer VMs set it without being Anubis tart disposable guests.
    false
}

/// Explicit operator override for host-side PoC lab (not for AOP C2).
pub fn poc_lab_host_override() -> bool {
    env_truthy("ANUBIS_POC_LAB_HOST")
}

fn env_truthy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        }
        Err(_) => false,
    }
}

fn dirs_home() -> Option<String> {
    std::env::var("HOME").ok()
}

fn hv_vmm_present() -> bool {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "kern.hv_vmm_present"])
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout);
                return s.trim() == "1";
            }
        }
    }
    false
}

/// Path is the in-repo gold lab fixture tree (`poc_kit/…`).
pub fn is_poc_kit_target(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("poc_kit/") || s.contains("poc_kit\\") {
        return true;
    }
    // Resolve and look for a `poc_kit` component
    let mut cur: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    // Walk parents
    for _ in 0..16 {
        if cur
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "poc_kit")
            .unwrap_or(false)
        {
            return true;
        }
        if !cur.pop() {
            break;
        }
    }
    // components check on the original path
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s == "poc_kit")
            .unwrap_or(false)
    })
}

/// Fail closed unless running inside Apple Virtualization guest.
/// Used for AOP red-team platform execution (C2, inject, lateral, …).
pub fn require_vz_offensive(action: &str) -> Result<()> {
    if in_vz_guest() {
        return Ok(());
    }
    Err(anyhow!(
        "ANUBIS_OFFENSIVE_HOST_FORBIDDEN: `{action}` is red-team/offensive platform execution and must run inside an Apple Virtualization guest (tart / Virtualization.framework), never on the host.\n\
         \n\
         Isolation (AOP platform):\n\
           1) ./target/release/anubis vz status   # golden: anubis-xcode\n\
           2) anubis vz exploit|fuzz|exec|c2-cycle|stress --base anubis-xcode …\n\
           3) Or guest env: export ANUBIS_VZ_GUEST=1 ; touch \"$HOME/.anubis-vz-guest\"\n\
         \n\
         PoC kit (packing / gold fixture) stays on the documented lab path:\n\
           anubis run examples/security/poc_*.anb --allow-research\n\
           anubis fuzz --target poc_kit/bin/vuln_local …\n\
           bash scripts/run_poc_kit_gate.sh\n\
         Prefer `anubis vz exploit|fuzz` for primary crash evidence."
    ))
}

/// Policy for `anubis run --allow-research` (PoC kit + research programs).
///
/// Never regressed: host continues to run packing smokes and gold crash PoCs.
/// Advance: prefer VZ for primary evidence (documented; not a hard block on host lab).
pub fn require_research_run_allowed(action: &str) -> Result<()> {
    if in_vz_guest() || poc_lab_host_override() {
        return Ok(());
    }
    // Host lab: allowed (existing Anubis PoC kit contract). Soft note on stderr.
    eprintln!(
        "[anubis isolation] `{action}` on host (lab PoC kit path). \
Primary crash evidence: prefer `anubis vz exploit --allow-research --base anubis-xcode …`. \
AOP C2/inject/lateral remain VZ-only."
    );
    Ok(())
}

/// Policy for `anubis fuzz --target …`.
///
/// - VZ guest: always OK  
/// - Host + `poc_kit/…` gold target: OK (run_poc_kit_gate)  
/// - Host + `ANUBIS_POC_LAB_HOST=1`: OK  
/// - Host + arbitrary path: require VZ (advance)
pub fn require_fuzz_allowed(target: &Path) -> Result<()> {
    if in_vz_guest() || poc_lab_host_override() {
        return Ok(());
    }
    if is_poc_kit_target(target) {
        eprintln!(
            "[anubis isolation] fuzz host lab gold fixture `{}`. \
Primary evidence: prefer `anubis vz fuzz --allow-research --base anubis-xcode …`.",
            target.display()
        );
        return Ok(());
    }
    Err(anyhow!(
        "ANUBIS_FUZZ_HOST_FORBIDDEN: fuzz of non–poc_kit target `{}` requires an Apple Virtualization guest.\n\
         \n\
         Options:\n\
           anubis vz fuzz --allow-research --base anubis-xcode {}\n\
           # or in guest: export ANUBIS_VZ_GUEST=1\n\
           # or gold lab only: anubis fuzz --target poc_kit/bin/vuln_local …\n\
           # emergency host lab: ANUBIS_POC_LAB_HOST=1 anubis fuzz --target …",
        target.display(),
        target.display()
    ))
}

/// JSON status for doctor / gates.
pub fn isolation_status_json() -> serde_json::Value {
    json!({
        "in_vz_guest": in_vz_guest(),
        "poc_lab_host_override": poc_lab_host_override(),
        "policy": {
            "aop_platform_requires_apple_virtualization": true,
            "poc_kit_host_lab_gold_allowed": true,
            "poc_kit_prefer_vz_for_primary_evidence": true,
            "fuzz_non_poc_kit_requires_vz": true,
        },
        "host_forbidden_aop": [
            "listen", "agent-generate", "task-queue",
            "inject-plan", "lateral-ssh", "lateral-smb",
            "exploit-run", "persist-launchagent", "pack-xor",
            "recon-scan", "string-scramble"
        ],
        "host_allowed_poc_kit": [
            "run --allow-research (packing + gold local harness)",
            "fuzz --target poc_kit/…",
            "bash scripts/run_poc_kit_gate.sh",
            "bash poc_kit/build_vuln.sh"
        ],
        "host_allowed_control_plane": [
            "engage-init", "engage-status", "offensive-doctor",
            "attck-catalog", "opsec-score", "campaign-init",
            "phish-plan", "lolbas-catalog", "malleable-init",
            "purple-report", "receipt-verify", "pattern-*", "gadget-*",
            "browser-harness", "exploit-new", "module-list",
            "vz-status", "vz-doctor", "vz-*"
        ],
        "guest_markers": [
            "ANUBIS_VZ_GUEST=1",
            "ANUBIS_OFFENSIVE_GATE_IN_GUEST=1",
            "ANUBIS_ISOLATION=*tart*|*vz*",
            "/etc/anubis-vz-guest",
            "$HOME/.anubis-vz-guest"
        ],
        // Informational only — NOT a guest marker (GHA macos runners set this).
        "hv_vmm_present_observed": hv_vmm_present(),
        "golden_base": "anubis-xcode",
        "ssh_key": "~/.ssh/tart_anubis",
        "poc_gold_target": "poc_kit/bin/vuln_local",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn poc_kit_paths_recognized() {
        assert!(is_poc_kit_target(Path::new("poc_kit/bin/vuln_local")));
        assert!(is_poc_kit_target(Path::new("examples/../poc_kit/x")));
        assert!(!is_poc_kit_target(Path::new("/tmp/other_binary")));
        assert!(!is_poc_kit_target(Path::new("target/release/anubis")));
    }

    #[test]
    fn host_forbid_message_is_stable_code() {
        // When not in guest, require_vz must fail closed with the stable code.
        // (May pass if this process is genuinely a VZ guest — still asserts code shape.)
        match require_vz_offensive("unit-test-action") {
            Ok(()) => assert!(in_vz_guest(), "require_vz Ok only when in_vz_guest is true"),
            Err(e) => {
                let s = format!("{e:#}");
                assert!(
                    s.contains("ANUBIS_OFFENSIVE_HOST_FORBIDDEN"),
                    "unexpected err: {s}"
                );
            }
        }
    }
}
