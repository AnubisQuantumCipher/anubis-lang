//! Isolation gate for AOP + PoC kit (advance without regression).
//!
//! # Threat model
//!
//! The env/file markers checked by `in_vz_guest()` are a **safety** mechanism — they prevent
//! accidental host execution of crash-capable code. They are NOT a security barrier: any
//! process running as the user can set `ANUBIS_VZ_GUEST=1`. This is by design — the operator
//! is the trust root and can always recompile the binary without the gate.
//!
//! Defense-in-depth: the canonical `anubis vz exploit`/`fuzz` path layers a **guest-bound
//! run capability** (HMAC-validated, single-use nonce, guest/program/engagement-bound) on
//! top of the marker gate. When `ANUBIS_VZ_ENFORCE_RUN_CAP=1` is set, both the marker AND
//! a valid capability must be present. The capability cannot be forged without the HMAC key.
//!
//! # Two tiers
//!
//! ## A — Red-team / C2 platform (strict Apple Virtualization)
//! listen, agent, inject, lateral, exploit-run, engagement packer, etc.
//! Host → `ANUBIS_OFFENSIVE_HOST_FORBIDDEN`.
//!
//! ## B — Bounty PoC kit and research execution
//! `anubis run --allow-research` and `anubis fuzz` retain their full capability inside an
//! Anubis-managed disposable VZ/tart guest. The host is an orchestrator only; there is no crash-
//! capable host fallback.
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
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

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
#[cfg(test)]
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
///
/// When `ANUBIS_RUN_CAP_PATH` is set (host-minted guest-bound capability), also
/// validates and consumes that capability (single-use nonce). Guest identity is
/// taken from `ANUBIS_VZ_GUEST_ID` or defaults to `anubis-xcode-guest`.
pub fn require_vz_offensive(action: &str) -> Result<()> {
    if !in_vz_guest() {
        return Err(anyhow!(
            "ANUBIS_OFFENSIVE_HOST_FORBIDDEN: `{action}` is red-team/offensive platform execution and must run inside an Apple Virtualization guest (tart / Virtualization.framework), never on the host.\n\
             \n\
             Isolation (AOP platform):\n\
               1) ./target/release/anubis vz status   # golden: anubis-xcode\n\
               2) anubis vz exploit|fuzz|exec|c2-cycle|stress --base anubis-xcode …\n\
               3) Or guest env: export ANUBIS_VZ_GUEST=1 ; touch \"$HOME/.anubis-vz-guest\"\n\
             \n\
             PoC kit and research execution use the same mandatory boundary:\n\
               anubis vz exploit --allow-research --base anubis-xcode <program>\n\
               anubis vz fuzz --allow-research --base anubis-xcode <target>\n\
             The host remains an orchestration and evidence-collection surface only."
        ));
    }
    require_run_capability_if_configured(action)?;
    Ok(())
}

/// Guest-bound capability gate.
/// - When `ANUBIS_VZ_ENFORCE_RUN_CAP=1` (set by `anubis vz exploit|fuzz`), capability is **required**.
/// - When only `ANUBIS_RUN_CAP_PATH` is set, still validate if present.
fn require_run_capability_if_configured(action: &str) -> Result<()> {
    let enforce = std::env::var("ANUBIS_VZ_ENFORCE_RUN_CAP").ok().as_deref() == Some("1");
    let cap_path = match std::env::var("ANUBIS_RUN_CAP_PATH") {
        Ok(p) if !p.trim().is_empty() => p,
        _ if enforce => {
            return Err(anyhow!(
                "ANUBIS_RUN_CAP_REQUIRED: `{action}` requires a guest-bound run capability \
                 (ANUBIS_RUN_CAP_PATH + ANUBIS_RUN_CAP_KEY); host orchestrator must mint one"
            ));
        }
        _ => return Ok(()),
    };
    let key = std::env::var("ANUBIS_RUN_CAP_KEY").map_err(|_| {
        anyhow!("ANUBIS_RUN_CAP_KEY_MISSING: capability path set but no ANUBIS_RUN_CAP_KEY")
    })?;
    let program_digest = std::env::var("ANUBIS_PROGRAM_DIGEST").unwrap_or_else(|_| "*".into());
    let engagement_id = std::env::var("ANUBIS_ENGAGEMENT_ID").unwrap_or_else(|_| "*".into());
    let engagement_hash = std::env::var("ANUBIS_ENGAGEMENT_HASH").unwrap_or_else(|_| "*".into());
    let guest_id =
        std::env::var("ANUBIS_VZ_GUEST_ID").unwrap_or_else(|_| "anubis-xcode-guest".into());

    let cap = super::run_capability::read_cap(Path::new(&cap_path))?;
    // Persist seen nonces under /tmp for single-guest process; multi-process needs shared store.
    static SEEN: once_cell_noop::OnceLockMutex = once_cell_noop::OnceLockMutex;
    let seen = SEEN.get();
    let ctx = super::run_capability::ValidateCtx {
        key: &key,
        guest_id: &guest_id,
        program_digest: if program_digest == "*" {
            &cap.program_digest
        } else {
            &program_digest
        },
        engagement_id: if engagement_id == "*" {
            &cap.engagement_id
        } else {
            &engagement_id
        },
        engagement_hash: if engagement_hash == "*" {
            &cap.engagement_hash
        } else {
            &engagement_hash
        },
        // Isolation gate checks guest/program/engagement/nonce/expiry; typed
        // effect enforcement is done by call sites that know the effect IR name.
        effect: None,
        target: None,
        seen_nonces: seen,
    };
    super::run_capability::validate_and_consume(&cap, &ctx)
        .map_err(|e| anyhow!("ANUBIS_RUN_CAP_REJECTED for `{action}`: {e}"))
}

/// Tiny once-mutex without new deps (std only).
mod once_cell_noop {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    pub struct OnceLockMutex;
    impl OnceLockMutex {
        pub fn get(&'static self) -> &'static Mutex<HashSet<String>> {
            static CELL: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
            CELL.get_or_init(|| Mutex::new(HashSet::new()))
        }
    }
}

/// Policy for `anubis run --allow-research` (PoC kit + research programs).
///
/// Mandatory boundary: research execution is permitted only inside an Anubis VZ guest.
pub fn require_research_run_allowed(action: &str) -> Result<()> {
    if !in_vz_guest() {
        return Err(anyhow!(
            "ANUBIS_RESEARCH_HOST_FORBIDDEN: `{action}` must run inside a disposable Apple \
             Virtualization.framework guest; use `anubis vz exploit --allow-research --base \
             anubis-xcode <program>`"
        ));
    }
    require_run_capability_if_configured(action)
}

/// Policy for `anubis fuzz --target …`.
///
/// - VZ guest: OK only with guest-bound capability when ENFORCE is set
/// - Host: forbidden, including gold fixtures and environment overrides
pub fn require_fuzz_allowed(target: &Path) -> Result<()> {
    if !in_vz_guest() {
        return Err(anyhow!(
            "ANUBIS_FUZZ_HOST_FORBIDDEN: fuzz target `{}` requires an Apple Virtualization guest.\n\
             \n\
             Options:\n\
               anubis vz fuzz --allow-research --base anubis-xcode {}\n\
               # or in guest: export ANUBIS_VZ_GUEST=1",
            target.display(),
            target.display()
        ));
    }
    require_run_capability_if_configured(&format!("fuzz:{}", target.display()))
}

/// JSON status for doctor / gates.
pub fn isolation_status_json() -> serde_json::Value {
    json!({
        "in_vz_guest": in_vz_guest(),
        "poc_lab_host_override": false,
        "host_override_supported": false,
        "policy": {
            "aop_platform_requires_apple_virtualization": true,
            "poc_kit_host_lab_gold_allowed": false,
            "all_research_and_fuzz_require_vz": true,
        },
        "host_forbidden_aop": [
            "listen", "agent-generate", "task-queue",
            "inject-plan", "lateral-ssh", "lateral-smb",
            "exploit-run", "persist-launchagent", "pack-xor",
            "recon-scan", "string-scramble"
        ],
        "host_allowed_poc_kit": [],
        "host_allowed_control_plane": [
            "engage-init", "engage-status", "engage-rehash",
            "operator-token-issue", "operator-token-revoke",
            "offensive-doctor",
            "attck-catalog", "attck-map", "opsec-score",
            "campaign-init", "campaign-status",
            "phish-plan", "lolbas-catalog",
            "malleable-init", "malleable-validate",
            "purple-report", "receipt-verify",
            "recon-hostinfo",
            "bounty-report", "research-pack-*",
            "pattern-*", "gadget-*",
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

    #[test]
    fn host_forbidden_aop_list_is_pinned_and_exhaustive() {
        let status = isolation_status_json();
        let forbidden: Vec<&str> = status["host_forbidden_aop"]
            .as_array()
            .expect("host_forbidden_aop must be an array")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let expected = [
            "listen",
            "agent-generate",
            "task-queue",
            "inject-plan",
            "lateral-ssh",
            "lateral-smb",
            "exploit-run",
            "persist-launchagent",
            "pack-xor",
            "recon-scan",
            "string-scramble",
        ];
        let mut sorted_forbidden = forbidden.clone();
        sorted_forbidden.sort();
        let mut sorted_expected = expected.to_vec();
        sorted_expected.sort();
        assert_eq!(
            sorted_forbidden, sorted_expected,
            "host_forbidden_aop drifted from pinned set — update BOTH the gate call \
             in main.rs AND this test when adding/removing an offensive command"
        );
    }

    #[test]
    fn isolation_status_json_has_required_policy_fields() {
        let status = isolation_status_json();
        assert!(
            status["policy"]["aop_platform_requires_apple_virtualization"]
                .as_bool()
                .unwrap()
        );
        assert!(status["policy"]["all_research_and_fuzz_require_vz"]
            .as_bool()
            .unwrap());
        assert!(status["host_allowed_control_plane"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "recon-hostinfo"));
        assert!(status["host_allowed_control_plane"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "bounty-report"));
        assert!(
            !status["host_allowed_poc_kit"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "fuzz"),
            "fuzz must NOT appear in host_allowed — it requires VZ"
        );
    }
}
