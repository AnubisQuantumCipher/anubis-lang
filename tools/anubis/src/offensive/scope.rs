//! Hard engagement scope — the non-negotiable control plane for the offensive platform.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Kind of engagement-scoped target (serialized into status/evidence).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    LocalPath,
    LocalProcess,
    Host,
    Cidr,
}

/// Structured allow-list entry for operator tooling and evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedTarget {
    pub kind: TargetKind,
    pub value: String,
    #[serde(default)]
    pub notes: String,
}

/// Canonical error type for scope checks (aliases `anyhow::Error`).
pub type ScopeError = anyhow::Error;

/// Build the structured allow-list used by engage-status and doctor evidence.
pub fn build_allowed_targets(
    hosts: &[String],
    cidrs: &[String],
    paths: &[String],
    lateral_hosts: &[String],
) -> Vec<AllowedTarget> {
    let mut out = Vec::new();
    for h in hosts {
        out.push(AllowedTarget {
            kind: TargetKind::Host,
            value: h.clone(),
            notes: "allowed_hosts".into(),
        });
    }
    for c in cidrs {
        out.push(AllowedTarget {
            kind: TargetKind::Cidr,
            value: c.clone(),
            notes: "allowed_cidrs".into(),
        });
    }
    for p in paths {
        out.push(AllowedTarget {
            kind: TargetKind::LocalPath,
            value: p.clone(),
            notes: "allowed_paths".into(),
        });
    }
    for h in lateral_hosts {
        out.push(AllowedTarget {
            kind: TargetKind::Host,
            value: h.clone(),
            notes: "allowed_lateral_hosts".into(),
        });
    }
    out
}

/// Validate a structured target against host/path/cidr allow-lists.
pub fn target_in_scope(
    target: &AllowedTarget,
    allowed_hosts: &[String],
    allowed_cidrs: &[String],
    allowed_paths: &[String],
) -> Result<(), ScopeError> {
    match target.kind {
        TargetKind::Host => host_in_scope(&target.value, allowed_hosts, allowed_cidrs),
        TargetKind::Cidr => {
            if allowed_cidrs
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&target.value))
            {
                Ok(())
            } else {
                Err(anyhow!(
                    "ANUBIS_SCOPE_DENIED: cidr `{}` not in engagement allowed_cidrs",
                    target.value
                ))
            }
        }
        TargetKind::LocalPath | TargetKind::LocalProcess => {
            path_in_scope(Path::new(&target.value), allowed_paths)
        }
    }
}

/// Returns Ok if `host` is allowed by engagement scope (exact host or CIDR).
pub fn host_in_scope(
    host: &str,
    allowed_hosts: &[String],
    allowed_cidrs: &[String],
) -> Result<(), ScopeError> {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return Err(anyhow!("ANUBIS_SCOPE_INVALID: empty host"));
    }
    let host_only = host.split(':').next().unwrap_or(&host);
    for h in allowed_hosts {
        if h.eq_ignore_ascii_case(host_only) || h.eq_ignore_ascii_case(&host) {
            return Ok(());
        }
    }
    if let Ok(ip) = IpAddr::from_str(host_only) {
        for cidr in allowed_cidrs {
            if ip_in_cidr(ip, cidr)? {
                return Ok(());
            }
        }
    }
    Err(anyhow!(
        "ANUBIS_SCOPE_DENIED: host `{host_only}` not in engagement allowed_hosts/cidrs"
    ))
}

/// Returns Ok if `path` is equal to or a descendant of an allowed root.
///
/// Uses **component-aware** prefix matching only — never substring `contains`.
/// `/allowed-evil` does **not** match root `/allowed`. Relative `..` escapes and
/// unresolved paths default-deny when no safe containment can be established.
pub fn path_in_scope(path: &Path, allowed_paths: &[String]) -> Result<(), ScopeError> {
    if allowed_paths.is_empty() {
        return Err(anyhow!(
            "ANUBIS_SCOPE_DENIED: path `{}` not in engagement allowed_paths (empty allow-list)",
            path.display()
        ));
    }
    if path_has_parent_escape(path) {
        return Err(anyhow!(
            "ANUBIS_SCOPE_DENIED: path `{}` contains `..` escape",
            path.display()
        ));
    }

    // Prefer the real path when resolvable so a symlink under an allowed root
    // cannot escape to an out-of-scope target (canonical path is authoritative).
    let candidates: Vec<PathBuf> = if let Ok(canon) = path.canonicalize() {
        vec![canon]
    } else {
        path_scope_candidates(path)
    };
    if candidates.is_empty() {
        return Err(anyhow!(
            "ANUBIS_SCOPE_DENIED: path `{}` could not be resolved for scope check",
            path.display()
        ));
    }

    for ap in allowed_paths {
        let roots = allowed_root_candidates(ap);
        for root in &roots {
            if path_has_parent_escape(root) {
                continue;
            }
            // Prefer canonical root when it exists.
            let root_forms: Vec<PathBuf> = if let Ok(rc) = root.canonicalize() {
                vec![rc]
            } else {
                roots.clone()
            };
            for r in &root_forms {
                for cand in &candidates {
                    if is_component_descendant(cand, r) {
                        return Ok(());
                    }
                }
            }
        }
    }

    Err(anyhow!(
        "ANUBIS_SCOPE_DENIED: path `{}` not in engagement allowed_paths",
        path.display()
    ))
}

fn path_has_parent_escape(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Candidate forms of a path for scope checks (lexically cleaned + optional canonicalize).
fn path_scope_candidates(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cleaned = clean_path_components(path);
    out.push(cleaned.clone());
    if let Ok(canon) = path.canonicalize() {
        if !out.iter().any(|p| p == &canon) {
            out.push(canon);
        }
    } else if let Ok(cwd) = std::env::current_dir() {
        let joined = clean_path_components(&cwd.join(path));
        if !out.iter().any(|p| p == &joined) {
            out.push(joined);
        }
        if let Ok(canon) = cwd.join(path).canonicalize() {
            if !out.iter().any(|p| p == &canon) {
                out.push(canon);
            }
        }
    }
    out
}

fn allowed_root_candidates(allowed: &str) -> Vec<PathBuf> {
    let trimmed = allowed
        .trim()
        .trim_end_matches("/*")
        .trim_end_matches('*')
        .trim_end_matches('/');
    if trimmed.is_empty() {
        return Vec::new();
    }
    let root = PathBuf::from(trimmed);
    let mut out = path_scope_candidates(&root);
    // Keep the lexical form even if canonicalize fails (lab roots may not exist yet).
    let cleaned = clean_path_components(&root);
    if !out.iter().any(|p| p == &cleaned) {
        out.push(cleaned);
    }
    out
}

fn clean_path_components(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() && path.is_absolute() {
        PathBuf::from(std::path::MAIN_SEPARATOR.to_string())
    } else {
        out
    }
}

/// True if `path` equals `root` or is a proper descendant via path **components**
/// (not string prefix). `/allowed-evil` is not under `/allowed`.
fn is_component_descendant(path: &Path, root: &Path) -> bool {
    let path_c: Vec<_> = path.components().collect();
    let root_c: Vec<_> = root.components().collect();
    if root_c.is_empty() {
        return false;
    }
    if path_c.len() < root_c.len() {
        return false;
    }
    path_c
        .iter()
        .zip(root_c.iter())
        .all(|(a, b)| a.as_os_str() == b.as_os_str())
}

pub fn bind_addr_in_scope(
    bind: &str,
    allow_non_loopback: bool,
    allowed_hosts: &[String],
) -> Result<(), ScopeError> {
    let host = bind.split(':').next().unwrap_or(bind);
    let is_loopback =
        host == "127.0.0.1" || host == "::1" || host.eq_ignore_ascii_case("localhost");
    if is_loopback {
        return Ok(());
    }
    if !allow_non_loopback {
        return Err(anyhow!(
            "ANUBIS_SCOPE_DENIED: non-loopback bind `{bind}` requires engagement.allow_non_loopback_bind=true (and network_egress)"
        ));
    }
    host_in_scope(host, allowed_hosts, &[])
}

fn ip_in_cidr(ip: IpAddr, cidr: &str) -> Result<bool, ScopeError> {
    let (net_s, prefix_s) = cidr
        .split_once('/')
        .ok_or_else(|| anyhow!("ANUBIS_SCOPE_INVALID: bad cidr `{cidr}`"))?;
    let prefix: u32 = prefix_s
        .parse()
        .map_err(|_| anyhow!("ANUBIS_SCOPE_INVALID: bad prefix in `{cidr}`"))?;
    match (ip, IpAddr::from_str(net_s)) {
        (IpAddr::V4(ip), Ok(IpAddr::V4(net))) => {
            if prefix > 32 {
                return Err(anyhow!("ANUBIS_SCOPE_INVALID: IPv4 prefix > 32"));
            }
            let mask = if prefix == 0 {
                0u32
            } else {
                u32::MAX << (32 - prefix)
            };
            Ok((u32::from(ip) & mask) == (u32::from(net) & mask))
        }
        (IpAddr::V6(_), Ok(IpAddr::V6(_))) => Ok(prefix == 128 && net_s == ip.to_string()),
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loopback_host_allowed() {
        host_in_scope("127.0.0.1", &["127.0.0.1".into()], &[]).unwrap();
        host_in_scope("127.0.0.5", &[], &["127.0.0.0/8".into()]).unwrap();
    }

    #[test]
    fn external_host_denied() {
        assert!(host_in_scope("8.8.8.8", &["127.0.0.1".into()], &["127.0.0.0/8".into()]).is_err());
    }

    #[test]
    fn structured_targets_include_lateral() {
        let t = build_allowed_targets(
            &["127.0.0.1".into()],
            &["127.0.0.0/8".into()],
            &["/tmp/lab".into()],
            &["localhost".into()],
        );
        assert!(t
            .iter()
            .any(|x| x.kind == TargetKind::Host && x.notes == "allowed_lateral_hosts"));
        let host = AllowedTarget {
            kind: TargetKind::Host,
            value: "127.0.0.1".into(),
            notes: String::new(),
        };
        target_in_scope(&host, &["127.0.0.1".into()], &[], &[]).unwrap();
    }

    #[test]
    fn path_descendant_allowed_not_substring_sibling() {
        // `/allowed-evil` must NOT match root `/allowed` (substring / string-prefix trap).
        let roots = vec!["/allowed".into()];
        assert!(path_in_scope(Path::new("/allowed"), &roots).is_ok());
        assert!(path_in_scope(Path::new("/allowed/child"), &roots).is_ok());
        assert!(path_in_scope(Path::new("/allowed-evil"), &roots).is_err());
        assert!(path_in_scope(Path::new("/allowed-evil/x"), &roots).is_err());
    }

    #[test]
    fn path_parent_escape_denied() {
        let roots = vec!["/tmp/lab".into()];
        assert!(path_in_scope(Path::new("/tmp/lab/../etc/passwd"), &roots).is_err());
        assert!(path_in_scope(Path::new("/tmp/lab/../../etc"), &roots).is_err());
    }

    #[test]
    fn path_empty_allowlist_denied() {
        assert!(path_in_scope(Path::new("/tmp/lab"), &[]).is_err());
    }

    #[test]
    fn component_descendant_unit() {
        assert!(is_component_descendant(
            Path::new("/tmp/lab/a"),
            Path::new("/tmp/lab")
        ));
        assert!(!is_component_descendant(
            Path::new("/tmp/lab2"),
            Path::new("/tmp/lab")
        ));
        assert!(!is_component_descendant(
            Path::new("/allowed-evil"),
            Path::new("/allowed")
        ));
    }

    #[test]
    fn path_symlink_escape_denied_when_resolvable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("allowed");
        let outside = dir.path().join("secret.txt");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, b"secret").unwrap();
        let link = root.join("escape");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &link).expect("symlink");
            // Canonicalized link points outside root → must deny.
            assert!(
                path_in_scope(&link, &[root.to_string_lossy().into()]).is_err(),
                "symlink escape should be denied"
            );
        }
    }
}
