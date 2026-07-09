//! Hard engagement scope — the non-negotiable control plane for the offensive platform.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    LocalPath,
    LocalProcess,
    Host,
    Cidr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedTarget {
    pub kind: TargetKind,
    pub value: String,
    #[serde(default)]
    pub notes: String,
}

/// Returns Ok if `host` is allowed by engagement scope (exact host or CIDR).
pub fn host_in_scope(host: &str, allowed_hosts: &[String], allowed_cidrs: &[String]) -> Result<()> {
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

pub fn path_in_scope(path: &Path, allowed_paths: &[String]) -> Result<()> {
    let rel = path.to_string_lossy();
    for ap in allowed_paths {
        let trimmed = ap.trim_end_matches('*').trim_end_matches('/');
        if rel.starts_with(trimmed) || rel.contains(trimmed) {
            return Ok(());
        }
        let ap_path = PathBuf::from(ap.trim_end_matches("/*").trim_end_matches('*'));
        if path.starts_with(&ap_path) {
            return Ok(());
        }
    }
    // canonicalize when possible
    if let Ok(canon) = path.canonicalize() {
        let s = canon.to_string_lossy();
        for ap in allowed_paths {
            let trimmed = ap.trim_end_matches('*').trim_end_matches('/');
            if s.contains(trimmed) {
                return Ok(());
            }
        }
    }
    Err(anyhow!(
        "ANUBIS_SCOPE_DENIED: path `{}` not in engagement allowed_paths",
        path.display()
    ))
}

pub fn bind_addr_in_scope(
    bind: &str,
    allow_non_loopback: bool,
    allowed_hosts: &[String],
) -> Result<()> {
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

fn ip_in_cidr(ip: IpAddr, cidr: &str) -> Result<bool> {
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

/// Re-export name used by mod.rs
pub type ScopeError = anyhow::Error;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_host_allowed() {
        host_in_scope("127.0.0.1", &["127.0.0.1".into()], &[]).unwrap();
        host_in_scope("127.0.0.5", &[], &["127.0.0.0/8".into()]).unwrap();
    }

    #[test]
    fn external_host_denied() {
        assert!(
            host_in_scope("8.8.8.8", &["127.0.0.1".into()], &["127.0.0.0/8".into()]).is_err()
        );
    }
}
