//! Resolve `[dependencies]` → locked packages + on-disk roots.
//!
//! **Transitive closure:** every package's own `Anubis.toml` deps are resolved recursively.
//! Cycles → `ANUBIS_DEP_CYCLE`. Same name, different version/content → `ANUBIS_DEP_VERSION_CONFLICT`.

use crate::package::cache;
use crate::package::lock::{LockFile, LockedPackage, LOCK_FILENAME};
use crate::package::merkle;
use crate::package::proof::{find_evidence_dir, verify_dep_evidence_for_package, ProofPolicy};
use crate::package::registry;
use crate::package::summary;
use crate::package::trust::TrustStore;
use crate::project::{AnubisManifest, DepSpec, ProjectLayout};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A fully resolved dependency ready for module mounting.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub name: String,
    pub version: String,
    /// Absolute path to package root (cache or path).
    pub root: PathBuf,
    /// Source root for module files (`root/src` if present else `root`).
    pub src_root: PathBuf,
    pub content_sha256: String,
    pub signer_public_key: Option<String>,
    /// True when this package is a direct dependency of the workspace root.
    pub direct: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedWorkspace {
    pub deps: BTreeMap<String, ResolvedDep>,
    pub lock: LockFile,
}

#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub cache_root: Option<PathBuf>,
    pub registry_root: Option<PathBuf>,
    /// Optional remote registry base URL (`file://` or `https://`). Overrides path-only layout.
    pub registry_url: Option<String>,
    pub trust_path: Option<PathBuf>,
    pub allow_unsigned: bool,
    /// If true, rewrite Anubis.lock (package lock command).
    pub write_lock: bool,
    /// If true, skip proof verification (internal tests only — never default).
    pub skip_proof: bool,
    /// If true, skip sealed summaries.json check (legacy tests only).
    pub skip_summaries: bool,
}

/// Resolve workspace dependencies for a project layout (full transitive closure).
pub fn resolve_workspace(
    layout: &ProjectLayout,
    opts: &ResolveOptions,
) -> Result<ResolvedWorkspace, String> {
    if layout.manifest.dependencies.is_empty() {
        return Ok(ResolvedWorkspace {
            deps: BTreeMap::new(),
            lock: LockFile::empty(),
        });
    }

    let lock_path = layout.root.join(LOCK_FILENAME);
    let existing_lock = if lock_path.is_file() {
        Some(LockFile::load(&lock_path)?)
    } else if opts.write_lock {
        None
    } else {
        return Err(
            "ANUBIS_LOCK_MISSING: Anubis.toml declares dependencies but Anubis.lock is absent \
             (run `anubis package lock`)"
                .to_string(),
        );
    };

    if !opts.write_lock {
        let lock = existing_lock.as_ref().unwrap();
        for name in layout.manifest.dependencies.keys() {
            if lock.get(name).is_none() {
                return Err(format!(
                    "ANUBIS_LOCK_STALE: dependency `{name}` is in Anubis.toml but not Anubis.lock \
                     (run `anubis package lock`)"
                ));
            }
        }
    }

    let cache_root = opts
        .cache_root
        .clone()
        .unwrap_or_else(cache::default_cache_root);
    let registry_root = opts
        .registry_root
        .clone()
        .unwrap_or_else(registry::default_registry_root);
    let registry_url = opts
        .registry_url
        .clone()
        .or_else(|| std::env::var("ANUBIS_REGISTRY_URL").ok());
    let trust_path = opts
        .trust_path
        .clone()
        .unwrap_or_else(crate::package::trust::default_trust_path);
    let trust = TrustStore::load(&trust_path)?;
    let project_signers = project_trust_keys(&layout.manifest);
    let policy = ProofPolicy {
        allow_unsigned: opts.allow_unsigned,
        project_signers,
    };

    let direct_names: BTreeSet<String> = layout.manifest.dependencies.keys().cloned().collect();

    let locked_packages: Vec<LockedPackage> = if opts.write_lock {
        let mut table: BTreeMap<String, LockedPackage> = BTreeMap::new();
        let mut stack: Vec<String> = Vec::new();
        for (name, spec) in &layout.manifest.dependencies {
            resolve_tree(
                name,
                spec,
                &layout.root,
                &layout.root,
                &registry_root,
                registry_url.as_deref(),
                &mut table,
                &mut stack,
            )?;
        }
        table.into_values().collect()
    } else {
        existing_lock.unwrap().package
    };

    let mut deps = BTreeMap::new();
    let mut new_lock = LockFile::empty();

    for locked in &locked_packages {
        let root = materialize_locked(locked, &layout.root, &cache_root, &registry_root, registry_url.as_deref())?;
        cache::verify_cache_dir(&root, &locked.content_sha256).or_else(|_| {
            let actual = merkle::merkle_root_dir(&root)?;
            if actual != locked.content_sha256 {
                Err(format!(
                    "ANUBIS_CACHE_HASH_MISMATCH: package `{}` at {} has {actual}, lock expects {}",
                    locked.name,
                    root.display(),
                    locked.content_sha256
                ))
            } else {
                Ok(())
            }
        })?;

        let mut signer = locked.signer_public_key.clone();
        if !opts.skip_proof {
            let ev = find_evidence_dir(&root).ok_or_else(|| {
                format!(
                    "ANUBIS_DEP_PROOF_UNVERIFIED: package `{}` has no evidence/ directory",
                    locked.name
                )
            })?;
            signer = verify_dep_evidence_for_package(Some(&root), &ev, &trust, &policy)?;
            if !opts.skip_summaries {
                summary::verify_against_package(&root, &ev)?;
            }
        }

        let src_root = {
            let s = root.join("src");
            if s.is_dir() {
                s
            } else {
                root.clone()
            }
        };

        let mut locked_out = locked.clone();
        if let Some(ref pk) = signer {
            locked_out.signer_public_key = Some(pk.clone());
        }
        new_lock.package.push(locked_out);

        deps.insert(
            locked.name.clone(),
            ResolvedDep {
                name: locked.name.clone(),
                version: locked.version.clone(),
                root,
                src_root,
                content_sha256: locked.content_sha256.clone(),
                signer_public_key: signer,
                direct: direct_names.contains(&locked.name),
            },
        );
    }

    // Deterministic lock order
    new_lock.package.sort_by(|a, b| a.name.cmp(&b.name));

    if opts.write_lock {
        new_lock.save(&lock_path)?;
    }

    Ok(ResolvedWorkspace {
        deps,
        lock: if opts.write_lock {
            new_lock
        } else {
            LockFile {
                version: 1,
                package: locked_packages,
            }
        },
    })
}

fn project_trust_keys(manifest: &AnubisManifest) -> Vec<String> {
    manifest
        .package
        .trust
        .signers
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Recursively resolve `name` and all of its transitive dependencies into `out`.
#[allow(clippy::too_many_arguments)] // recursive resolver threads its full state explicitly
fn resolve_tree(
    name: &str,
    spec: &DepSpec,
    workspace_root: &Path,
    base_root: &Path,
    registry_root: &Path,
    registry_url: Option<&str>,
    out: &mut BTreeMap<String, LockedPackage>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    if stack.iter().any(|s| s == name) {
        let cycle = stack
            .iter()
            .chain(std::iter::once(&name.to_string()))
            .cloned()
            .collect::<Vec<_>>()
            .join(" → ");
        return Err(format!("ANUBIS_DEP_CYCLE: dependency cycle: {cycle}"));
    }
    stack.push(name.to_string());

    let locked = resolve_one_fresh(name, spec, workspace_root, base_root, registry_root, registry_url)?;

    if let Some(prev) = out.get(name) {
        if prev.version != locked.version || prev.content_sha256 != locked.content_sha256 {
            stack.pop();
            return Err(format!(
                "ANUBIS_DEP_VERSION_CONFLICT: package `{name}` required as {}@{} (sha {}) and {}@{} (sha {})",
                prev.version,
                prev.version,
                &prev.content_sha256[..prev.content_sha256.len().min(12)],
                locked.version,
                locked.version,
                &locked.content_sha256[..locked.content_sha256.len().min(12)]
            ));
        }
        stack.pop();
        return Ok(());
    }

    // Materialize temporarily to walk nested Anubis.toml (path is already on disk).
    let pkg_root = peek_package_root(&locked, workspace_root, registry_root, registry_url)?;
    out.insert(name.to_string(), locked);

    if let Ok(text) = std::fs::read_to_string(pkg_root.join("Anubis.toml")) {
        if let Ok(nested) = AnubisManifest::parse(&text) {
            for (n, s) in &nested.dependencies {
                resolve_tree(
                    n,
                    s,
                    workspace_root,
                    &pkg_root,
                    registry_root,
                    registry_url,
                    out,
                    stack,
                )?;
            }
        }
    }

    stack.pop();
    Ok(())
}

fn peek_package_root(
    locked: &LockedPackage,
    workspace_root: &Path,
    registry_root: &Path,
    registry_url: Option<&str>,
) -> Result<PathBuf, String> {
    match locked.source.as_str() {
        "path" => {
            let p = locked
                .path
                .as_ref()
                .ok_or("ANUBIS_LOCK_STALE: path missing")?;
            let abs = resolve_path_field(p, workspace_root);
            if !abs.is_dir() {
                return Err(format!(
                    "ANUBIS_DEP_UNRESOLVED: path `{}` missing",
                    abs.display()
                ));
            }
            Ok(abs.canonicalize().unwrap_or(abs))
        }
        "registry" => {
            let url = locked.registry.as_deref().or(registry_url);
            if let Some(url) = url {
                return registry::fetch_remote_version(url, &locked.name, &locked.version);
            }
            let reg = registry::package_version_dir(registry_root, &locked.name, &locked.version);
            if reg.is_dir() {
                Ok(reg)
            } else {
                // non-canonical version dir
                let vers = registry::list_versions(registry_root, &locked.name)?;
                for v in vers {
                    if crate::package::semver::Version::parse(&v)
                        .ok()
                        .map(|pv| pv.to_string_canonical() == locked.version)
                        .unwrap_or(false)
                    {
                        return Ok(registry::package_version_dir(registry_root, &locked.name, &v));
                    }
                }
                Err(format!(
                    "ANUBIS_REGISTRY_MISS: `{}@{}`",
                    locked.name, locked.version
                ))
            }
        }
        "git" => {
            let g = locked.git.as_ref().ok_or("ANUBIS_LOCK_STALE: git missing")?;
            let rev = locked.rev.as_ref().ok_or("ANUBIS_GIT_REV_REQUIRED")?;
            fetch_git(g, rev, &locked.name)
        }
        other => Err(format!("ANUBIS_LOCK_STALE: unknown source `{other}`")),
    }
}

fn resolve_one_fresh(
    name: &str,
    spec: &DepSpec,
    workspace_root: &Path,
    base_root: &Path,
    registry_root: &Path,
    registry_url: Option<&str>,
) -> Result<LockedPackage, String> {
    match spec {
        DepSpec::Version(req) => {
            let (ver, path, sha) =
                registry::resolve_version_any(registry_root, registry_url, name, req)?;
            let (ev_sha, signer) = evidence_meta(&path);
            Ok(LockedPackage {
                name: name.to_string(),
                version: ver,
                source: "registry".into(),
                path: None,
                git: None,
                rev: None,
                registry: registry_url.map(|s| s.to_string()),
                content_sha256: sha,
                evidence_sha256: ev_sha,
                signer_public_key: signer,
            })
        }
        DepSpec::Detailed {
            version,
            path,
            git,
            rev,
            registry,
        } => {
            if let Some(p) = path {
                let abs = base_root.join(p);
                if !abs.is_dir() {
                    return Err(format!(
                        "ANUBIS_DEP_UNRESOLVED: path dependency `{name}` not a directory: {}",
                        abs.display()
                    ));
                }
                let abs = abs.canonicalize().unwrap_or(abs);
                let sha = merkle::merkle_root_dir(&abs)?;
                let ver = read_package_version(&abs).unwrap_or_else(|| "0.0.0".into());
                let (ev_sha, signer) = evidence_meta(&abs);
                let lock_path = path_for_lock(&abs, workspace_root);
                return Ok(LockedPackage {
                    name: name.to_string(),
                    version: ver,
                    source: "path".into(),
                    path: Some(lock_path),
                    git: None,
                    rev: None,
                    registry: None,
                    content_sha256: sha,
                    evidence_sha256: ev_sha,
                    signer_public_key: signer,
                });
            }
            if let Some(g) = git {
                let rev = rev.as_ref().ok_or_else(|| {
                    "ANUBIS_GIT_REV_REQUIRED: git dependencies must pin `rev` for Anubis.lock"
                        .to_string()
                })?;
                let dest = fetch_git(g, rev, name)?;
                let sha = merkle::merkle_root_dir(&dest)?;
                let ver = read_package_version(&dest).unwrap_or_else(|| "0.0.0".into());
                let (ev_sha, signer) = evidence_meta(&dest);
                return Ok(LockedPackage {
                    name: name.to_string(),
                    version: ver,
                    source: "git".into(),
                    path: None,
                    git: Some(g.clone()),
                    rev: Some(rev.clone()),
                    registry: None,
                    content_sha256: sha,
                    evidence_sha256: ev_sha,
                    signer_public_key: signer,
                });
            }
            if let Some(req) = version {
                let reg_url = registry.as_deref().or(registry_url);
                let (ver, path, sha) =
                    registry::resolve_version_any(registry_root, reg_url, name, req)?;
                let (ev_sha, signer) = evidence_meta(&path);
                return Ok(LockedPackage {
                    name: name.to_string(),
                    version: ver,
                    source: "registry".into(),
                    path: None,
                    git: None,
                    rev: None,
                    registry: reg_url.map(|s| s.to_string()),
                    content_sha256: sha,
                    evidence_sha256: ev_sha,
                    signer_public_key: signer,
                });
            }
            Err(format!(
                "ANUBIS_DEP_UNRESOLVED: dependency `{name}` has empty source (need version, path, or git+rev)"
            ))
        }
    }
}

fn path_for_lock(abs: &Path, workspace_root: &Path) -> String {
    let ws = workspace_root.canonicalize().unwrap_or_else(|_| workspace_root.to_path_buf());
    if let Ok(rel) = abs.strip_prefix(&ws) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        abs.display().to_string()
    }
}

fn resolve_path_field(p: &str, workspace_root: &Path) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() {
        pb
    } else {
        workspace_root.join(p)
    }
}

fn evidence_meta(root: &Path) -> (Option<String>, Option<String>) {
    let Some(ev) = find_evidence_dir(root) else {
        return (None, None);
    };
    let man = ev.join("MANIFEST.sha256");
    let ev_sha = std::fs::read(&man).ok().map(|b| merkle::sha256_hex(&b));
    let signer = crate::evidence::pca_signature_status(&ev)
        .ok()
        .and_then(|s| s.map(|(_, pk)| pk));
    (ev_sha, signer)
}

fn materialize_locked(
    locked: &LockedPackage,
    project_root: &Path,
    cache_root: &Path,
    registry_root: &Path,
    registry_url: Option<&str>,
) -> Result<PathBuf, String> {
    match locked.source.as_str() {
        "path" => {
            let p = locked
                .path
                .as_ref()
                .ok_or_else(|| "ANUBIS_LOCK_STALE: path dep missing path field".to_string())?;
            let abs = resolve_path_field(p, project_root);
            if !abs.is_dir() {
                return Err(format!(
                    "ANUBIS_DEP_UNRESOLVED: path `{}` missing",
                    abs.display()
                ));
            }
            Ok(abs.canonicalize().unwrap_or(abs))
        }
        "registry" | "git" => {
            let cached = cache::package_cache_dir(
                cache_root,
                &locked.name,
                &locked.version,
                &locked.content_sha256,
            );
            if cached.is_dir() {
                return Ok(cached);
            }
            if locked.source == "registry" {
                let url = locked.registry.as_deref().or(registry_url);
                let reg = if let Some(url) = url {
                    registry::fetch_remote_version(url, &locked.name, &locked.version)?
                } else {
                    let p = registry::package_version_dir(
                        registry_root,
                        &locked.name,
                        &locked.version,
                    );
                    if p.is_dir() {
                        p
                    } else {
                        return Err(format!(
                            "ANUBIS_DEP_UNRESOLVED: cannot materialize `{}@{}` (not in cache/registry)",
                            locked.name, locked.version
                        ));
                    }
                };
                return cache::materialize_from_dir(
                    cache_root,
                    &locked.name,
                    &locked.version,
                    &locked.content_sha256,
                    &reg,
                );
            }
            if locked.source == "git" {
                let g = locked.git.as_ref().ok_or("ANUBIS_LOCK_STALE: git missing")?;
                let rev = locked.rev.as_ref().ok_or("ANUBIS_GIT_REV_REQUIRED")?;
                let dest = fetch_git(g, rev, &locked.name)?;
                return cache::materialize_from_dir(
                    cache_root,
                    &locked.name,
                    &locked.version,
                    &locked.content_sha256,
                    &dest,
                );
            }
            Err(format!(
                "ANUBIS_DEP_UNRESOLVED: cannot materialize `{}@{}`",
                locked.name, locked.version
            ))
        }
        other => Err(format!("ANUBIS_LOCK_STALE: unknown source `{other}`")),
    }
}

fn read_package_version(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("Anubis.toml")).ok()?;
    let m = AnubisManifest::parse(&text).ok()?;
    if m.package.version.is_empty() {
        None
    } else {
        Some(m.package.version)
    }
}

fn fetch_git(url: &str, rev: &str, name: &str) -> Result<PathBuf, String> {
    let base = std::env::temp_dir()
        .join("anubis-git-fetch")
        .join(name)
        .join(rev);
    if base.join(".git").is_dir() || (base.is_dir() && base.join("Anubis.toml").is_file()) {
        return Ok(base);
    }
    if let Some(p) = base.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(&base)
        .status()
        .map_err(|e| format!("ANUBIS_DEP_UNRESOLVED: git clone failed: {e}"))?;
    if !status.success() {
        return Err(format!(
            "ANUBIS_DEP_UNRESOLVED: git clone of `{url}` failed"
        ));
    }
    let co = std::process::Command::new("git")
        .args(["-C"])
        .arg(&base)
        .args(["checkout", rev])
        .status()
        .map_err(|e| e.to_string())?;
    if !co.success() {
        let _ = std::process::Command::new("git")
            .args(["-C"])
            .arg(&base)
            .args(["fetch", "--depth", "1", "origin", rev])
            .status();
        let co2 = std::process::Command::new("git")
            .args(["-C"])
            .arg(&base)
            .args(["checkout", rev])
            .status()
            .map_err(|e| e.to_string())?;
        if !co2.success() {
            return Err(format!(
                "ANUBIS_DEP_UNRESOLVED: git checkout rev `{rev}` failed"
            ));
        }
    }
    Ok(base)
}
