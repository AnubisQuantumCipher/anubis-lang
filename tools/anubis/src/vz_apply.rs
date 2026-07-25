//! Slice-2: APPLY a derived confinement grant to a live tart guest boot.
//!
//! Core `anubis.confinement.v1` stays engagement-free and re-derivable.
//! This module emits a SEPARATE `anubis.confinement.applied.v1` artifact that records
//! the argv actually passed to `tart run` (engagement-dependent), so PCA re-derive is
//! not broken.
//!
//! Depth refinements (2026-07-25):
//! - **Mounts:** engagement `--dir` filtered fail-closed against proven mount posture.
//! - **Network (Softnet/hostname dual):** engagement network flags filtered fail-closed
//!   against proven network posture:
//!   - host-only (no net.send / open): keep `--net-host`; refuse `--allow-host` / `--allow-open-nat`
//!   - unrestricted-nat (net.send): default force host-only (not open NAT); `--allow-host` stages
//!     DNS-pinned egress policy (tart does NOT enforce per-hostname — honesty); `--allow-open-nat`
//!     is explicit residual opt-in to full NAT.
//! Applied network may be more restrictive than the language proof, never more open without opt-in.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

pub const APPLIED_SCHEMA: &str = "anubis.confinement.applied.v1";
pub const APPLIED_FILENAME: &str = "confinement_applied.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedConfinement {
    pub schema: String,
    pub program: String,
    pub source_merkle: String,
    pub effects_bounded: bool,
    pub capabilities_present: Vec<String>,
    /// Derived from core grants: `network:host-only` | `network:unrestricted-nat` | …
    pub network_posture: String,
    /// Derived from core grants: `none` | `read-only` | `read-write`.
    pub mount_posture: String,
    /// Tart argv flags after network+grant filtering (e.g. `--net-host`).
    pub tart_args: Vec<String>,
    /// Host dirs mounted (`--dir` forms) after posture filtering.
    pub mounts: Vec<String>,
    /// Mounts that were rewritten (e.g. forced `:ro`).
    pub mounts_adjusted: Vec<String>,
    /// Engagement hostname allow-list (may be empty).
    pub allow_hosts: Vec<String>,
    /// DNS-pinned IPv4 strings from EgressPolicy (sorted); empty when no hostname policy.
    pub egress_pinned_ipv4: Vec<String>,
    /// `host-only` | `hostname-policy-staged` | `open-nat-opt-in`
    pub network_apply_mode: String,
    /// True only when tart argv enforces the net posture (`--net-host`). Hostname policy is staged.
    pub network_tart_enforced: bool,
    pub notes: Vec<String>,
}

/// Result of fail-closed network engagement filtering.
#[derive(Debug, Clone)]
pub struct NetworkApply {
    /// Net-related tart flags to merge into final tart_args (`--net-host` or empty).
    pub net_tart_flags: Vec<String>,
    pub allow_hosts: Vec<String>,
    pub egress_pinned_ipv4: Vec<String>,
    pub network_apply_mode: String,
    pub network_tart_enforced: bool,
    pub notes: Vec<String>,
}

/// Collect engagement-independent tart args from a derived core manifest.
pub fn tart_args_from_manifest(
    manifest: &anubis_compiler::package::confinement::ConfinementManifest,
) -> Vec<String> {
    let mut args = Vec::new();
    for g in &manifest.grants {
        for a in &g.tart_args {
            if !args.contains(a) {
                args.push(a.clone());
            }
        }
    }
    args
}

/// Network posture string from core grants (deterministic).
pub fn network_posture_from_manifest(
    manifest: &anubis_compiler::package::confinement::ConfinementManifest,
) -> String {
    manifest
        .grants
        .iter()
        .find(|g| g.capability == "net.send")
        .map(|g| g.hypervisor_grant.clone())
        .unwrap_or_else(|| "network:host-only".into())
}

/// Mount posture: `none` | `read-only` | `read-write` from core grants.
pub fn mount_posture_from_manifest(
    manifest: &anubis_compiler::package::confinement::ConfinementManifest,
) -> String {
    let rw = manifest
        .grants
        .iter()
        .find(|g| g.capability == "fs.write")
        .map(|g| g.hypervisor_grant.as_str());
    let ro = manifest
        .grants
        .iter()
        .find(|g| g.capability == "fs.read")
        .map(|g| g.hypervisor_grant.as_str());
    match (rw, ro) {
        (Some(g), _) if g.contains("read-write") => "read-write".into(),
        (_, Some(g)) if g.contains("read-only") => "read-only".into(),
        (Some(g), _) if g.contains("read-only") => "read-only".into(),
        _ => "none".into(),
    }
}

fn force_readonly_mount(spec: &str) -> String {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    match parts.as_slice() {
        [tag, path, mode] if *mode == "ro" || *mode == "rw" => {
            format!("{tag}:{path}:ro")
        }
        [tag, path] if !path.is_empty() => format!("{tag}:{path}:ro"),
        _ => {
            if spec.ends_with(":ro") {
                spec.to_string()
            } else if spec.ends_with(":rw") {
                format!("{}ro", &spec[..spec.len() - 2])
            } else {
                format!("{spec}:ro")
            }
        }
    }
}

/// Filter engagement mounts against proven mount posture. Fail closed.
pub fn filter_mounts_for_posture(
    requested: &[String],
    mount_posture: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    if requested.is_empty() {
        return Ok((vec![], vec![]));
    }
    match mount_posture {
        "none" => Err(anyhow!(
            "ANUBIS_APPLY_MOUNT_DENIED: program proves no fs.read/fs.write (mount posture none) \
             — refusing {} engagement mount(s). Host paths are only allowed when the proven \
             effect set includes filesystem access (fail-closed).",
            requested.len()
        )),
        "read-only" => {
            let mut out = Vec::new();
            let mut adjusted = Vec::new();
            for m in requested {
                let forced = force_readonly_mount(m);
                if forced != *m {
                    adjusted.push(format!("{m} → {forced} (forced :ro by mount:read-only posture)"));
                }
                out.push(forced);
            }
            Ok((out, adjusted))
        }
        "read-write" => Ok((requested.to_vec(), vec![])),
        other => Err(anyhow!(
            "ANUBIS_APPLY_MOUNT_DENIED: unknown mount posture `{other}` — fail closed"
        )),
    }
}

/// Fail-closed network dual of mount filtering.
///
/// - host-only: keep `--net-host`; refuse allow-host / open-nat expansion.
/// - unrestricted-nat: default force host-only; allow-host stages DNS policy; open-nat is opt-in.
pub fn filter_network_for_posture(
    network_posture: &str,
    allow_hosts: &[String],
    allow_open_nat: bool,
) -> Result<NetworkApply> {
    let host_only = network_posture.contains("host-only");
    let unrestricted = network_posture.contains("unrestricted");

    if host_only {
        if allow_open_nat {
            return Err(anyhow!(
                "ANUBIS_APPLY_NET_DENIED: proven network posture is host-only (no net.send / open \
                 effects) — refusing --allow-open-nat (would expand guest egress beyond the proof)"
            ));
        }
        if !allow_hosts.is_empty() {
            return Err(anyhow!(
                "ANUBIS_APPLY_NET_DENIED: proven network posture is host-only — refusing {} \
                 --allow-host entr(y/ies). There is no guest internet egress to allow-list; use a \
                 program that proves net.send, or drop --allow-host.",
                allow_hosts.len()
            ));
        }
        return Ok(NetworkApply {
            net_tart_flags: vec!["--net-host".into()],
            allow_hosts: vec![],
            egress_pinned_ipv4: vec![],
            network_apply_mode: "host-only".into(),
            network_tart_enforced: true,
            notes: vec![
                "network apply: host-only (tart --net-host enforced). Not a zero-NIC air-gap."
                    .into(),
            ],
        });
    }

    if unrestricted {
        // Explicit residual: full NAT.
        if allow_open_nat {
            if !allow_hosts.is_empty() {
                return Err(anyhow!(
                    "ANUBIS_APPLY_NET_DENIED: --allow-open-nat cannot combine with --allow-host \
                     (open NAT has no per-hostname tart enforcement; pick one mode)"
                ));
            }
            return Ok(NetworkApply {
                net_tart_flags: vec![],
                allow_hosts: vec![],
                egress_pinned_ipv4: vec![],
                network_apply_mode: "open-nat-opt-in".into(),
                network_tart_enforced: false,
                notes: vec![
                    "network apply: open-nat-opt-in — full tart NAT, NOT a confinement. Explicit \
                     residual; Softnet/hostname not used."
                        .into(),
                ],
            });
        }

        // Hostname policy staged (DNS pin for native/Softnet residual).
        if !allow_hosts.is_empty() {
            let policy = crate::vz_egress_gateway::EgressPolicy::from_allow_hosts(allow_hosts)
                .map_err(|e| anyhow!("ANUBIS_APPLY_NET_DENIED: egress policy: {e}"))?;
            let mut ipv4: Vec<String> = policy
                .allowed_ipv4
                .iter()
                .map(|ip| ip.to_string())
                .collect();
            ipv4.sort();
            return Ok(NetworkApply {
                // Tart cannot enforce per-hostname — stay host-only until Softnet residual.
                net_tart_flags: vec!["--net-host".into()],
                allow_hosts: allow_hosts.to_vec(),
                egress_pinned_ipv4: ipv4,
                network_apply_mode: "hostname-policy-staged".into(),
                network_tart_enforced: true, // --net-host only; hostname is staged
                notes: vec![
                    "network apply: hostname-policy-staged — allow-list DNS-pinned and recorded; \
                     tart enforces host-only only (NOT per-hostname). Per-hostname enforcement is \
                     native VZ egress pump / Softnet residual."
                        .into(),
                ],
            });
        }

        // net.send proven but no open-nat and no allow-host → fail closed to host-only.
        return Ok(NetworkApply {
            net_tart_flags: vec!["--net-host".into()],
            allow_hosts: vec![],
            egress_pinned_ipv4: vec![],
            network_apply_mode: "host-only".into(),
            network_tart_enforced: true,
            notes: vec![
                "network apply: net.send proved but no --allow-open-nat / --allow-host — default \
                 fail-closed to host-only (not unrestricted NAT). Opt in to open NAT with \
                 --allow-open-nat, or stage a hostname policy with --allow-host."
                    .into(),
            ],
        });
    }

    Err(anyhow!(
        "ANUBIS_APPLY_NET_DENIED: unknown network posture `{network_posture}` — fail closed"
    ))
}

/// Merge grant tart_args with network apply: drop grant net flags, use filtered net flags.
fn merge_tart_args(grant_args: &[String], net: &NetworkApply) -> Vec<String> {
    let mut out: Vec<String> = grant_args
        .iter()
        .filter(|a| *a != "--net-host" && !a.starts_with("--net"))
        .cloned()
        .collect();
    for a in &net.net_tart_flags {
        if !out.contains(a) {
            out.push(a.clone());
        }
    }
    out
}

/// Engagement options for apply (mounts + network).
#[derive(Debug, Clone, Default)]
pub struct ApplyEngagement {
    pub mounts: Vec<String>,
    pub allow_hosts: Vec<String>,
    pub allow_open_nat: bool,
}

/// Build the applied record (does not boot).
pub fn build_applied(
    program: &str,
    engagement: &ApplyEngagement,
) -> Result<(
    AppliedConfinement,
    anubis_compiler::package::confinement::ConfinementManifest,
)> {
    let src = fs::read_to_string(program).with_context(|| format!("read program `{program}`"))?;
    let merkle = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(src.as_bytes());
        hex::encode(h.finalize())
    };
    let ast = anubis_compiler::parse_source(&src)
        .map_err(|e| anyhow!("ANUBIS_APPLY_PARSE_FAILED: {e}"))?;
    let mode = crate::first_mode(&ast.items).unwrap_or(anubis_compiler::frontend::Mode::Safe);
    anubis_compiler::typecheck(ast, mode).map_err(|e| {
        anyhow!(
            "ANUBIS_APPLY_UNVERIFIED: refuse apply from a program that does not pass check: {e}"
        )
    })?;
    let manifest =
        anubis_compiler::package::confinement::derive_confinement("program", "0.0.0", &src)
            .map_err(|e| anyhow!("{e}"))?;
    let grant_args = tart_args_from_manifest(&manifest);
    let network_posture = network_posture_from_manifest(&manifest);
    let mount_posture = mount_posture_from_manifest(&manifest);
    let (filtered_mounts, mounts_adjusted) =
        filter_mounts_for_posture(&engagement.mounts, &mount_posture)?;
    let net = filter_network_for_posture(
        &network_posture,
        &engagement.allow_hosts,
        engagement.allow_open_nat,
    )?;
    let tart_args = merge_tart_args(&grant_args, &net);

    let mut notes = vec![
        "Applied confinement is engagement-dependent and is NOT re-derived by PCA verify \
         (core confinement_manifest.json remains the sealed, re-derivable surface)."
            .into(),
        "tart --net-host is host-only, not a zero-NIC air-gap; use `vz native-preflight` \
         for the structural zero-NIC posture."
            .into(),
        format!(
            "Mount posture `{mount_posture}` is derived from the proven effect set; engagement \
             --dir mounts are filtered fail-closed against it."
        ),
        format!(
            "Network posture `{network_posture}` → apply mode `{}` (tart_enforced={}).",
            net.network_apply_mode, net.network_tart_enforced
        ),
    ];
    notes.extend(net.notes.clone());
    if !manifest.effects_bounded {
        notes.push(
            "effects UNBOUNDED — core confined most restrictively; applied inherits that posture."
                .into(),
        );
    }

    let applied = AppliedConfinement {
        schema: APPLIED_SCHEMA.into(),
        program: program.into(),
        source_merkle: merkle,
        effects_bounded: manifest.effects_bounded,
        capabilities_present: manifest.capabilities_present.clone(),
        network_posture,
        mount_posture,
        tart_args,
        mounts: filtered_mounts,
        mounts_adjusted,
        allow_hosts: net.allow_hosts,
        egress_pinned_ipv4: net.egress_pinned_ipv4,
        network_apply_mode: net.network_apply_mode,
        network_tart_enforced: net.network_tart_enforced,
        notes,
    };
    Ok((applied, manifest))
}

/// Convenience: mounts-only engagement (backward-compatible call sites).
pub fn build_applied_mounts_only(
    program: &str,
    mounts: &[String],
) -> Result<(
    AppliedConfinement,
    anubis_compiler::package::confinement::ConfinementManifest,
)> {
    build_applied(
        program,
        &ApplyEngagement {
            mounts: mounts.to_vec(),
            ..Default::default()
        },
    )
}

pub fn write_applied(applied: &AppliedConfinement, out: Option<&Path>) -> Result<std::path::PathBuf> {
    let path = match out {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()?.join(APPLIED_FILENAME),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(applied)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn build_tart_run_argv(
    name: &str,
    no_graphics: bool,
    detach: bool,
    tart_args: &[String],
    mounts: &[String],
) -> Vec<String> {
    let mut argv = vec!["run".to_string(), name.to_string()];
    if no_graphics {
        argv.push("--no-graphics".into());
    }
    if detach {
        argv.push("--no-wait".into());
    }
    for a in tart_args {
        argv.push(a.clone());
    }
    for m in mounts {
        argv.push("--dir".into());
        argv.push(m.clone());
    }
    argv
}

pub fn apply_and_run(
    program: &str,
    vm_name: &str,
    engagement: &ApplyEngagement,
    no_graphics: bool,
    detach: bool,
    applied_out: Option<&Path>,
) -> Result<std::path::PathBuf> {
    let (applied, _manifest) = build_applied(program, engagement)?;
    let path = write_applied(&applied, applied_out)?;
    eprintln!(
        "[anubis vz apply] program={} mode={} tart_args=[{}] mount_posture={} mounts={}",
        program,
        applied.network_apply_mode,
        applied.tart_args.join(" "),
        applied.mount_posture,
        applied.mounts.len()
    );
    for a in &applied.mounts_adjusted {
        eprintln!("[anubis vz apply] mount adjusted: {a}");
    }
    if !applied.allow_hosts.is_empty() {
        eprintln!(
            "[anubis vz apply] hostname policy staged: hosts={:?} pinned_ipv4={:?}",
            applied.allow_hosts, applied.egress_pinned_ipv4
        );
    }
    eprintln!("[anubis vz apply] wrote {}", path.display());

    if which_tart().is_none() {
        bail!(
            "ANUBIS_APPLY_NO_TART: tart not on PATH — applied manifest written, but live boot skipped"
        );
    }
    let argv = build_tart_run_argv(
        vm_name,
        no_graphics,
        detach,
        &applied.tart_args,
        &applied.mounts,
    );
    eprintln!("[anubis vz apply] tart {}", argv.join(" "));
    let status = Command::new("tart")
        .args(&argv)
        .status()
        .context("spawn tart")?;
    if !status.success() {
        bail!("ANUBIS_APPLY_TART_FAILED: tart exit {status}");
    }
    Ok(path)
}

fn which_tart() -> Option<std::path::PathBuf> {
    Command::new("which")
        .arg("tart")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eng_mounts(m: &[&str]) -> ApplyEngagement {
        ApplyEngagement {
            mounts: m.iter().map(|s| (*s).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn net_free_demo_gets_net_host_arg() {
        let demo = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/showcase/vz_confine_demo.anb"
        );
        if !Path::new(demo).exists() {
            eprintln!("skip: demo missing");
            return;
        }
        let (applied, _) = build_applied(demo, &ApplyEngagement::default()).expect("apply");
        assert!(
            applied.tart_args.iter().any(|a| a == "--net-host"),
            "net-free program should apply --net-host, got {:?}",
            applied.tart_args
        );
        assert_eq!(applied.mount_posture, "none");
        assert!(applied.network_posture.contains("host-only"));
        assert_eq!(applied.network_apply_mode, "host-only");
        assert!(applied.network_tart_enforced);
    }

    #[test]
    fn net_free_program_rejects_engagement_mounts() {
        let demo = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/showcase/vz_confine_demo.anb"
        );
        if !Path::new(demo).exists() {
            return;
        }
        let err = build_applied(demo, &eng_mounts(&["workspace:/tmp/ws"]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("ANUBIS_APPLY_MOUNT_DENIED"), "{err}");
    }

    #[test]
    fn net_free_rejects_allow_host_and_open_nat() {
        let demo = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/showcase/vz_confine_demo.anb"
        );
        if !Path::new(demo).exists() {
            return;
        }
        let e1 = build_applied(
            demo,
            &ApplyEngagement {
                allow_hosts: vec!["127.0.0.1".into()],
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(e1.contains("ANUBIS_APPLY_NET_DENIED"), "{e1}");
        let e2 = build_applied(
            demo,
            &ApplyEngagement {
                allow_open_nat: true,
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(e2.contains("ANUBIS_APPLY_NET_DENIED"), "{e2}");
    }

    #[test]
    fn net_send_defaults_to_host_only_not_open_nat() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("net.anb");
        fs::write(
            &src,
            r#"fn main() uses(net.send) { send("h", 80, "x"); }
"#,
        )
        .unwrap();
        let (applied, _) =
            build_applied(src.to_str().unwrap(), &ApplyEngagement::default()).expect("apply");
        assert!(applied.network_posture.contains("unrestricted"));
        assert!(
            applied.tart_args.iter().any(|a| a == "--net-host"),
            "net.send without opt-in must not get open NAT, got {:?}",
            applied.tart_args
        );
        assert_eq!(applied.network_apply_mode, "host-only");
    }

    #[test]
    fn net_send_allow_host_stages_hostname_policy() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("net.anb");
        fs::write(
            &src,
            r#"fn main() uses(net.send) { send("h", 80, "x"); }
"#,
        )
        .unwrap();
        let (applied, _) = build_applied(
            src.to_str().unwrap(),
            &ApplyEngagement {
                allow_hosts: vec!["127.0.0.1".into()],
                ..Default::default()
            },
        )
        .expect("apply");
        assert_eq!(applied.network_apply_mode, "hostname-policy-staged");
        assert_eq!(applied.allow_hosts, vec!["127.0.0.1".to_string()]);
        assert!(
            applied.egress_pinned_ipv4.iter().any(|ip| ip == "127.0.0.1"),
            "pinned {:?}",
            applied.egress_pinned_ipv4
        );
        // Tart still host-only (not per-hostname enforced).
        assert!(applied.tart_args.iter().any(|a| a == "--net-host"));
    }

    #[test]
    fn net_send_open_nat_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("net.anb");
        fs::write(
            &src,
            r#"fn main() uses(net.send) { send("h", 80, "x"); }
"#,
        )
        .unwrap();
        let (applied, _) = build_applied(
            src.to_str().unwrap(),
            &ApplyEngagement {
                allow_open_nat: true,
                ..Default::default()
            },
        )
        .expect("apply");
        assert_eq!(applied.network_apply_mode, "open-nat-opt-in");
        assert!(!applied.tart_args.iter().any(|a| a == "--net-host"));
        assert!(!applied.network_tart_enforced);
    }

    #[test]
    fn read_only_posture_forces_ro_suffix() {
        let (accepted, adjusted) =
            filter_mounts_for_posture(&["ws:/tmp/data".into()], "read-only").unwrap();
        assert_eq!(accepted, vec!["ws:/tmp/data:ro".to_string()]);
        assert!(!adjusted.is_empty());
    }

    #[test]
    fn none_posture_rejects_any_mount() {
        let err = filter_mounts_for_posture(&["x:/y".into()], "none").unwrap_err();
        assert!(err.to_string().contains("ANUBIS_APPLY_MOUNT_DENIED"));
    }

    #[test]
    fn read_write_posture_allows_mounts_as_is() {
        let (m, adj) =
            filter_mounts_for_posture(&["ws:/tmp/data".into()], "read-write").unwrap();
        assert_eq!(m, vec!["ws:/tmp/data".to_string()]);
        assert!(adj.is_empty());
    }

    #[test]
    fn fs_read_program_apply_forces_ro_on_mount() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("ro.anb");
        fs::write(
            &src,
            r#"fn main() uses(fs.read) { let _ = read_file("a"); }
"#,
        )
        .unwrap();
        let (applied, _) =
            build_applied(src.to_str().unwrap(), &eng_mounts(&["data:/tmp/d"])).expect("apply");
        assert_eq!(applied.mount_posture, "read-only");
        assert_eq!(applied.mounts, vec!["data:/tmp/d:ro".to_string()]);
    }

    #[test]
    fn rejects_unverified_program() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.anb");
        fs::write(&bad, "fn main( { broken\n").unwrap();
        let err = build_applied(bad.to_str().unwrap(), &ApplyEngagement::default())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("ANUBIS_APPLY_UNVERIFIED") || err.contains("ANUBIS_APPLY_PARSE"),
            "{err}"
        );
    }

    #[test]
    fn filter_network_host_only_unit() {
        let n = filter_network_for_posture("network:host-only", &[], false).unwrap();
        assert_eq!(n.network_apply_mode, "host-only");
        assert!(n.net_tart_flags.contains(&"--net-host".into()));
    }
}
