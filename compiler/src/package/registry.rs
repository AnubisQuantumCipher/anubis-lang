//! Local + remote package registry.
//!
//! Layout (local or remote):
//! ```text
//! <base>/<name>/<version>/   # package tree
//! ```
//!
//! Remote bases:
//! - `file:///abs/path` — filesystem (same layout)
//! - `https://host/path` / `http://…` — fetched with `curl -fsSL` into a cache dir
//!   (`~/.anubis/remote-cache/` or temp). Index: GET `{base}/{name}/versions.txt`
//!   (one version per line). Package: GET recursive via `{base}/{name}/{version}/…`
//!   implemented as a single tarball URL `{base}/{name}/{version}.tar.gz` **or**
//!   directory listing when using `file://`.

use crate::package::merkle;
use crate::package::semver::{select_max_matching, VersionReq};
use std::path::{Path, PathBuf};

pub fn default_registry_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".anubis")
        .join("registry")
}

pub fn package_version_dir(registry: &Path, name: &str, version: &str) -> PathBuf {
    registry.join(name).join(version)
}

/// List version directory names under registry/name/.
pub fn list_versions(registry: &Path, name: &str) -> Result<Vec<String>, String> {
    let dir = registry.join(name);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut vers = Vec::new();
    for ent in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let ent = ent.map_err(|e| e.to_string())?;
        if ent.path().is_dir() {
            vers.push(ent.file_name().to_string_lossy().to_string());
        }
    }
    Ok(vers)
}

/// Resolve SemVer req against local registry; returns (version, path, content_sha).
pub fn resolve_version(
    registry: &Path,
    name: &str,
    req_str: &str,
) -> Result<(String, PathBuf, String), String> {
    resolve_version_any(registry, None, name, req_str)
}

/// Resolve against local root and/or remote URL.
pub fn resolve_version_any(
    local_root: &Path,
    remote_url: Option<&str>,
    name: &str,
    req_str: &str,
) -> Result<(String, PathBuf, String), String> {
    if let Some(url) = remote_url {
        return resolve_remote(url, name, req_str);
    }
    let req = VersionReq::parse(req_str)?;
    let vers = list_versions(local_root, name)?;
    if vers.is_empty() {
        return Err(format!(
            "ANUBIS_REGISTRY_MISS: package `{name}` not found in {}",
            local_root.display()
        ));
    }
    let chosen = select_max_matching(&req, vers.iter().map(|s| s.as_str())).map_err(|_| {
        format!("ANUBIS_REGISTRY_MISS: no version of `{name}` matches `{req_str}`")
    })?;
    let ver = chosen.to_string_canonical();
    let path = package_version_dir(local_root, name, &ver);
    if path.is_dir() {
        let sha = merkle::merkle_root_dir(&path)?;
        return Ok((ver, path, sha));
    }
    let vers = list_versions(local_root, name)?;
    for v in &vers {
        if let Ok(pv) = crate::package::semver::Version::parse(v) {
            if pv == chosen {
                let path = package_version_dir(local_root, name, v);
                let sha = merkle::merkle_root_dir(&path)?;
                return Ok((ver, path, sha));
            }
        }
    }
    Err(format!(
        "ANUBIS_REGISTRY_MISS: package `{name}` version `{ver}` missing on disk"
    ))
}

/// Fetch a concrete remote version into a durable local dir; return path.
pub fn fetch_remote_version(base_url: &str, name: &str, version: &str) -> Result<PathBuf, String> {
    let base = base_url.trim_end_matches('/');
    if let Some(path) = base.strip_prefix("file://") {
        let p = PathBuf::from(path).join(name).join(version);
        if p.is_dir() {
            return Ok(p);
        }
        return Err(format!(
            "ANUBIS_REGISTRY_MISS: file registry missing {}@{} at {}",
            name,
            version,
            p.display()
        ));
    }
    if base.starts_with("http://") || base.starts_with("https://") {
        return fetch_http_version(base, name, version);
    }
    // Bare filesystem path as registry base
    let p = PathBuf::from(base).join(name).join(version);
    if p.is_dir() {
        return Ok(p);
    }
    Err(format!(
        "ANUBIS_REGISTRY_MISS: registry missing {name}@{version}"
    ))
}

fn resolve_remote(
    base_url: &str,
    name: &str,
    req_str: &str,
) -> Result<(String, PathBuf, String), String> {
    let base = base_url.trim_end_matches('/');
    let versions = list_remote_versions(base, name)?;
    if versions.is_empty() {
        return Err(format!(
            "ANUBIS_REGISTRY_MISS: package `{name}` not found at {base}"
        ));
    }
    let req = VersionReq::parse(req_str)?;
    let chosen = select_max_matching(&req, versions.iter().map(|s| s.as_str())).map_err(|_| {
        format!("ANUBIS_REGISTRY_MISS: no version of `{name}` matches `{req_str}` at {base}")
    })?;
    let ver = chosen.to_string_canonical();
    // Prefer exact dir name match from listing
    let ver_dir = versions
        .iter()
        .find(|v| {
            crate::package::semver::Version::parse(v)
                .map(|pv| pv == chosen)
                .unwrap_or(false)
        })
        .cloned()
        .unwrap_or_else(|| ver.clone());
    let path = fetch_remote_version(base, name, &ver_dir)?;
    let sha = merkle::merkle_root_dir(&path)?;
    Ok((ver, path, sha))
}

fn list_remote_versions(base: &str, name: &str) -> Result<Vec<String>, String> {
    if let Some(path) = base.strip_prefix("file://") {
        return list_versions(&PathBuf::from(path), name);
    }
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return list_versions(&PathBuf::from(base), name);
    }
    // Prefer versions.txt; fall back to empty
    let url = format!("{base}/{name}/versions.txt");
    let body = http_get(&url)?;
    let vers: Vec<String> = body
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if !vers.is_empty() {
        return Ok(vers);
    }
    Err(format!(
        "ANUBIS_REGISTRY_MISS: empty versions.txt for `{name}` at {url}"
    ))
}

fn fetch_http_version(base: &str, name: &str, version: &str) -> Result<PathBuf, String> {
    let dest_root = remote_cache_root().join(name).join(version);
    if dest_root.is_dir() && dest_root.join("Anubis.toml").is_file() {
        return Ok(dest_root);
    }
    std::fs::create_dir_all(dest_root.parent().unwrap()).map_err(|e| e.to_string())?;
    // Prefer tarball
    let tarball_url = format!("{base}/{name}/{version}.tar.gz");
    let tarball = dest_root.with_extension("tar.gz");
    if http_get_to_file(&tarball_url, &tarball).is_ok() {
        let status = std::process::Command::new("tar")
            .args(["-xzf"])
            .arg(&tarball)
            .arg("-C")
            .arg(dest_root.parent().unwrap())
            .status()
            .map_err(|e| format!("ANUBIS_REGISTRY_MISS: tar extract: {e}"))?;
        if status.success() {
            // tarball may expand to name-version/ or version/
            if dest_root.is_dir() {
                let _ = std::fs::remove_file(&tarball);
                return Ok(dest_root);
            }
        }
    }
    // Fallback: versions as flat files not supported without listing — fail closed
    Err(format!(
        "ANUBIS_REGISTRY_MISS: cannot fetch {name}@{version} from {base} \
         (need {name}/{version}.tar.gz or file:// registry)"
    ))
}

fn remote_cache_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".anubis")
        .join("remote-cache")
}

fn http_get(url: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "60", url])
        .output()
        .map_err(|e| {
            format!("ANUBIS_REGISTRY_MISS: curl not available for remote registry: {e}")
        })?;
    if !out.status.success() {
        return Err(format!(
            "ANUBIS_REGISTRY_MISS: GET {url} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

fn http_get_to_file(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let status = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "3600", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| format!("ANUBIS_REGISTRY_MISS: curl: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(dest);
        return Err(format!("ANUBIS_REGISTRY_MISS: download failed: {url}"));
    }
    Ok(())
}

/// Publish a package tree into the local registry (copy).
pub fn publish_to_registry(
    registry: &Path,
    name: &str,
    version: &str,
    package_root: &Path,
) -> Result<PathBuf, String> {
    let dest = package_version_dir(registry, name, version);
    if dest.exists() {
        return Err(format!(
            "ANUBIS_REGISTRY_MISS: `{name}@{version}` already published at {}",
            dest.display()
        ));
    }
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    copy_tree(package_root, &dest)?;
    // Write versions.txt for remote-protocol parity
    let idx = registry.join(name).join("versions.txt");
    let mut lines = if idx.is_file() {
        std::fs::read_to_string(&idx).unwrap_or_default()
    } else {
        String::new()
    };
    if !lines.lines().any(|l| l.trim() == version) {
        if !lines.is_empty() && !lines.ends_with('\n') {
            lines.push('\n');
        }
        lines.push_str(version);
        lines.push('\n');
        let _ = std::fs::write(idx, lines);
    }
    Ok(dest)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for ent in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let ent = ent.map_err(|e| e.to_string())?;
        let from = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "out" || name == "target" || name == "Anubis.lock" {
            continue;
        }
        let to = dst.join(ent.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
