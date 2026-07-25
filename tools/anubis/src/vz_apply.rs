//! Slice-2: APPLY a derived confinement grant to a live tart guest boot.
//!
//! Core `anubis.confinement.v1` stays engagement-free and re-derivable.
//! This module emits a SEPARATE `anubis.confinement.applied.v1` artifact that records
//! the argv actually passed to `tart run` (engagement-dependent), so PCA re-derive is
//! not broken.

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
    /// Tart argv flags derived from grants (e.g. `--net-host`).
    pub tart_args: Vec<String>,
    /// Host dirs mounted (`--dir` forms) from optional engagement.
    pub mounts: Vec<String>,
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
    let applied = AppliedConfinement {
        schema: APPLIED_SCHEMA.into(),
        program: program.into(),
        source_merkle: merkle,
        effects_bounded: manifest.effects_bounded,
        capabilities_present: manifest.capabilities_present.clone(),
        tart_args: tart_args.clone(),
        mounts: mounts.to_vec(),
        notes: vec![
            "Applied confinement is engagement-dependent and is NOT re-derived by PCA verify \
             (core confinement_manifest.json remains the sealed, re-derivable surface)."
                .into(),
            "tart --net-host is host-only, not a zero-NIC air-gap; use `vz native-preflight` \
             for the structural zero-NIC posture."
                .into(),
        ],
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
        "[anubis vz apply] program={} tart_args=[{}] mounts={}",
        program,
        applied.tart_args.join(" "),
        applied.mounts.len()
    );
    eprintln!("[anubis vz apply] wrote {}", path.display());

    if which_tart().is_none() {
        bail!(
            "ANUBIS_APPLY_NO_TART: tart not on PATH — applied manifest written, but live boot skipped"
        );
    }
    let argv = build_tart_run_argv(vm_name, no_graphics, detach, &applied.tart_args, mounts);
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
        let argv = build_tart_run_argv("vm", true, true, &applied.tart_args, &[]);
        assert!(argv.contains(&"--net-host".to_string()));
        assert!(argv.contains(&"--no-graphics".to_string()));
    }

    #[test]
    fn rejects_unverified_program() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.anb");
        fs::write(&bad, "fn main( { broken\n").unwrap();
        let err = build_applied(bad.to_str().unwrap(), &[]).unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_APPLY_UNVERIFIED") || err.contains("ANUBIS_APPLY_PARSE"),
            "{err}"
        );
    }
}
