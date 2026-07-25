//! Slice-2: APPLY a derived confinement grant to a live tart guest boot.
//!
//! Core `anubis.confinement.v1` stays engagement-free and re-derivable.
//! This module emits a SEPARATE `anubis.confinement.applied.v1` artifact that records
//! the argv actually passed to `tart run` (engagement-dependent), so PCA re-derive is
//! not broken.
//!
//! Depth refinement (2026-07-25): engagement mounts (`--dir`) are **filtered fail-closed**
//! against the proven filesystem posture from the core manifest:
//! - `mount:none` (no fs.read/fs.write, or open effects) → any mount is `ANUBIS_APPLY_MOUNT_DENIED`
//! - `mount:read-only` → mounts forced to `:ro` (write suffix stripped / added)
//! - `mount:read-write` → mounts allowed as supplied
//! Network tart args remain pure from the core grant (`--net-host` when net-free / open).

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
    /// Tart argv flags derived from grants (e.g. `--net-host`).
    pub tart_args: Vec<String>,
    /// Host dirs mounted (`--dir` forms) after posture filtering (engagement-supplied, constrained).
    pub mounts: Vec<String>,
    /// Mounts that were requested but rewritten (e.g. forced `:ro`); empty when none.
    pub mounts_adjusted: Vec<String>,
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
    // Prefer fs.write grant if present, else fs.read, else none.
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

/// Normalize a tart `--dir` value. Forms accepted:
/// - `tag:path`
/// - `tag:path:ro`
/// - `path` (no tag — left as-is for tart)
fn force_readonly_mount(spec: &str) -> String {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    match parts.as_slice() {
        [tag, path, mode] if *mode == "ro" || *mode == "rw" => {
            format!("{tag}:{path}:ro")
        }
        [tag, path] if !path.is_empty() => format!("{tag}:{path}:ro"),
        _ => {
            // Unknown shape: append :ro if not already ending with :ro
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
///
/// Returns `(accepted_mounts, adjusted_notes)`.
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

/// Build the applied record (does not boot).
pub fn build_applied(
    program: &str,
    mounts: &[String],
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
    let tart_args = tart_args_from_manifest(&manifest);
    let network_posture = network_posture_from_manifest(&manifest);
    let mount_posture = mount_posture_from_manifest(&manifest);
    let (filtered_mounts, mounts_adjusted) =
        filter_mounts_for_posture(mounts, &mount_posture)?;

    let mut notes = vec![
        "Applied confinement is engagement-dependent and is NOT re-derived by PCA verify \
         (core confinement_manifest.json remains the sealed, re-derivable surface)."
            .into(),
        "tart --net-host is host-only, not a zero-NIC air-gap; use `vz native-preflight` \
         for the structural zero-NIC posture."
            .into(),
        format!(
            "Mount posture `{mount_posture}` is derived from the proven effect set; engagement \
             --dir mounts are filtered fail-closed against it (none rejects all mounts; \
             read-only forces :ro)."
        ),
    ];
    if !manifest.effects_bounded {
        notes.push(
            "effects UNBOUNDED — core confined most restrictively; applied inherits that posture."
                .into(),
        );
    }
    if network_posture.contains("unrestricted") {
        notes.push(
            "network:unrestricted-nat is recorded honestly — tart default NAT is not a confinement; \
             Softnet/allow-list residual."
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
        tart_args: tart_args.clone(),
        mounts: filtered_mounts,
        mounts_adjusted,
        notes,
    };
    Ok((applied, manifest))
}

/// Write applied JSON next to out path or cwd.
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

/// Build `tart run` argv: [name, flags..., tart_args..., --dir mounts...].
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
        argv.push("--no-wait".into()); // tart detach-ish; older tart uses --no-wait
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

/// Apply confinement + boot via tart. Returns applied artifact path.
/// Uses posture-filtered mounts from `build_applied` (not raw CLI mounts).
pub fn apply_and_run(
    program: &str,
    vm_name: &str,
    mounts: &[String],
    no_graphics: bool,
    detach: bool,
    applied_out: Option<&Path>,
) -> Result<std::path::PathBuf> {
    let (applied, _manifest) = build_applied(program, mounts)?;
    let path = write_applied(&applied, applied_out)?;
    eprintln!(
        "[anubis vz apply] program={} tart_args=[{}] mount_posture={} mounts={}",
        program,
        applied.tart_args.join(" "),
        applied.mount_posture,
        applied.mounts.len()
    );
    if !applied.mounts_adjusted.is_empty() {
        for a in &applied.mounts_adjusted {
            eprintln!("[anubis vz apply] mount adjusted: {a}");
        }
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
        let (applied, _) = build_applied(demo, &[]).expect("apply");
        assert!(
            applied.tart_args.iter().any(|a| a == "--net-host"),
            "net-free program should apply --net-host, got {:?}",
            applied.tart_args
        );
        assert_eq!(applied.mount_posture, "none");
        assert!(applied.network_posture.contains("host-only"));
        let argv = build_tart_run_argv("vm", true, true, &applied.tart_args, &[]);
        assert!(argv.contains(&"--net-host".to_string()));
        assert!(argv.contains(&"--no-graphics".to_string()));
    }

    #[test]
    fn net_free_program_rejects_engagement_mounts() {
        let demo = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/showcase/vz_confine_demo.anb"
        );
        if !Path::new(demo).exists() {
            eprintln!("skip: demo missing");
            return;
        }
        let err = build_applied(demo, &["workspace:/tmp/ws".into()])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("ANUBIS_APPLY_MOUNT_DENIED"),
            "expected mount denied, got {err}"
        );
    }

    #[test]
    fn read_only_posture_forces_ro_suffix() {
        let (accepted, adjusted) =
            filter_mounts_for_posture(&["ws:/tmp/data".into()], "read-only").unwrap();
        assert_eq!(accepted, vec!["ws:/tmp/data:ro".to_string()]);
        assert!(!adjusted.is_empty());
        let (already, adj2) =
            filter_mounts_for_posture(&["ws:/tmp/data:ro".into()], "read-only").unwrap();
        assert_eq!(already, vec!["ws:/tmp/data:ro".to_string()]);
        assert!(adj2.is_empty());
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
            build_applied(src.to_str().unwrap(), &["data:/tmp/d".into()]).expect("apply");
        assert_eq!(applied.mount_posture, "read-only");
        assert_eq!(applied.mounts, vec!["data:/tmp/d:ro".to_string()]);
        assert!(!applied.mounts_adjusted.is_empty());
    }

    #[test]
    fn rejects_unverified_program() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.anb");
        fs::write(&bad, "fn main( { broken\n").unwrap();
        let err = build_applied(bad.to_str().unwrap(), &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("ANUBIS_APPLY_UNVERIFIED") || err.contains("ANUBIS_APPLY_PARSE"),
            "{err}"
        );
    }
}
