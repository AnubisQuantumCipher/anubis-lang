//! Verify dependency evidence bundles and signer trust.

use crate::evidence::{pca_signature_status, verify_pca};
use crate::package::merkle;
use crate::package::trust::TrustStore;
use std::path::Path;

/// Module extensions recognized for package source binding (mirrors resolve::MODULE_EXTENSIONS).
const PACKAGE_MODULE_EXTS: &[&str] = &["anb", "anub", "anubis"];

/// Options controlling unsigned-dep policy.
#[derive(Debug, Clone, Default)]
pub struct ProofPolicy {
    /// Allow unsigned deps only when both env and CLI opt-in (caller enforces both).
    pub allow_unsigned: bool,
    /// Project-level trusted public keys (from Anubis.toml `[package.trust].signers`).
    pub project_signers: Vec<String>,
}

/// Verify a package's sealed `evidence/` directory **and** bind it to the package sources
/// the consumer will load (fail-closed if the signed claim is about different code).
///
/// - Missing evidence / invalid PCA → `ANUBIS_DEP_PROOF_UNVERIFIED`
/// - Evidence source unbound from package modules → `ANUBIS_DEP_PROOF_UNVERIFIED`
/// - Valid sig but signer not trusted → `ANUBIS_DEP_UNTRUSTED_SIGNER`
/// - Unsigned + !allow_unsigned → `ANUBIS_DEP_PROOF_UNVERIFIED`
///
/// Returns the signer public key when signed+verified+trusted, or `None` when
/// unsigned was explicitly allowed.
pub fn verify_dep_evidence(
    evidence_dir: &Path,
    trust: &TrustStore,
    policy: &ProofPolicy,
) -> Result<Option<String>, String> {
    verify_dep_evidence_for_package(None, evidence_dir, trust, policy)
}

/// Like [`verify_dep_evidence`], and when `package_root` is set, require
/// `evidence/source.anubis` (or multi-file merkle leaves) to match package module sources.
pub fn verify_dep_evidence_for_package(
    package_root: Option<&Path>,
    evidence_dir: &Path,
    trust: &TrustStore,
    policy: &ProofPolicy,
) -> Result<Option<String>, String> {
    if !evidence_dir.is_dir() {
        return Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: missing evidence directory `{}`",
            evidence_dir.display()
        ));
    }
    let ok = verify_pca(evidence_dir)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: pca verify failed: {e}"))?;
    if !ok {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: evidence bundle failed hash/claim verification"
                .to_string(),
        );
    }
    if let Some(root) = package_root {
        bind_evidence_to_package_sources(root, evidence_dir)?;
        // Sealed summaries must re-derive from the package the consumer mounts.
        // (skip only when evidence lacks summaries.json for pre-summary fixtures —
        // resolve_deps enforces summaries by default via summary::verify_against_package.)
        if evidence_dir
            .join(crate::package::summary::SUMMARIES_FILENAME)
            .is_file()
        {
            crate::package::summary::verify_against_package(root, evidence_dir)?;
        }
    }
    match pca_signature_status(evidence_dir)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: signature status: {e}"))?
    {
        None => {
            if policy.allow_unsigned {
                Ok(None)
            } else {
                Err(
                    "ANUBIS_DEP_PROOF_UNVERIFIED: dependency evidence is unsigned \
                     (sign with `anubis sign` / `anubis package publish --key`)"
                        .to_string(),
                )
            }
        }
        Some((false, pk)) => Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: invalid signature for signer `{pk}`"
        )),
        Some((true, pk)) => {
            if trust.allows(&pk, &policy.project_signers) {
                Ok(Some(pk))
            } else {
                Err(format!(
                    "ANUBIS_DEP_UNTRUSTED_SIGNER: signer `{pk}` is not in the trust store \
                     (`anubis trust add-signer` or [package.trust] signers)"
                ))
            }
        }
    }
}

/// Bind signed evidence to the code the consumer mounts (closes swap-source attacks).
///
/// * **One module file** under package `src/` (or root): `evidence/source.anubis` must be
///   byte-identical to that file.
/// * **Multiple modules**: `source-merkle-leaves.json` must record a Merkle root equal to the
///   package module tree; if leaves are absent, sealed source must match exactly one module
///   (single-entry publish path).
pub fn bind_evidence_to_package_sources(
    package_root: &Path,
    evidence_dir: &Path,
) -> Result<(), String> {
    let sealed_path = evidence_dir.join("source.anubis");
    let sealed = std::fs::read(&sealed_path).map_err(|e| {
        format!("ANUBIS_DEP_PROOF_UNVERIFIED: cannot read evidence source.anubis: {e}")
    })?;
    let src_root = {
        let s = package_root.join("src");
        if s.is_dir() {
            s
        } else {
            package_root.to_path_buf()
        }
    };
    let modules = collect_package_modules(&src_root)?;
    if modules.is_empty() {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: package has no module sources (.anb) to bind evidence to"
                .to_string(),
        );
    }
    if modules.len() == 1 {
        if sealed != modules[0].1 {
            return Err(
                "ANUBIS_DEP_PROOF_UNVERIFIED: evidence source.anubis does not match package \
                 module source (signed claim unbound from code the consumer loads)"
                    .to_string(),
            );
        }
        return Ok(());
    }
    let leaves_path = evidence_dir.join("source-merkle-leaves.json");
    if leaves_path.is_file() {
        let actual = merkle::merkle_root(modules.clone());
        let text = std::fs::read_to_string(&leaves_path).map_err(|e| e.to_string())?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: {e}"))?;
        let claimed = v
            .get("source_merkle_root")
            .and_then(|r| r.as_str())
            .unwrap_or("");
        if claimed != actual {
            return Err(
                "ANUBIS_DEP_PROOF_UNVERIFIED: evidence source merkle root does not match \
                 package module tree"
                    .to_string(),
            );
        }
        return Ok(());
    }
    // Multi-file package with single-entry sealed evidence (publish-one-file path):
    // sealed body must equal exactly one module on disk.
    if modules.iter().any(|(_, b)| b == &sealed) {
        return Ok(());
    }
    Err(
        "ANUBIS_DEP_PROOF_UNVERIFIED: evidence source.anubis matches no package module \
         (and no source-merkle-leaves.json for multi-file bind)"
            .to_string(),
    )
}

fn collect_package_modules(src_root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    collect_modules_walk(src_root, src_root, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn collect_modules_walk(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| {
        format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: cannot read {}: {e}",
            dir.display()
        )
    })?;
    for ent in rd {
        let ent = ent.map_err(|e| e.to_string())?;
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "out" || name == "target" || name == "evidence" {
            continue;
        }
        if name.starts_with("evidence-") {
            continue;
        }
        if path.is_dir() {
            collect_modules_walk(root, &path, out)?;
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if PACKAGE_MODULE_EXTS.contains(&ext) {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                out.push((rel, data));
            }
        }
    }
    Ok(())
}

/// Prefer package-local `evidence/` then nested `evidence-*/` first match.
pub fn find_evidence_dir(package_root: &Path) -> Option<std::path::PathBuf> {
    let direct = package_root.join("evidence");
    if direct.is_dir() && direct.join("MANIFEST.sha256").is_file() {
        return Some(direct);
    }
    // Also accept a single evidence-* snapshot under package root.
    if let Ok(rd) = std::fs::read_dir(package_root) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                let name = ent.file_name().to_string_lossy().to_string();
                if name.starts_with("evidence-") && p.join("MANIFEST.sha256").is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}
