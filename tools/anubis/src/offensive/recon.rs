//! Scoped reconnaissance — elite red-team discovery, fail-closed to engagement scope.
//! Never scans hosts outside allowed_hosts / allowed_cidrs / allowed_lateral_hosts.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Common lab ports elite teams probe first.
const DEFAULT_PORTS: &[u16] = &[22, 80, 443, 445, 3389, 4444, 5353, 8080, 8443];

pub fn recon_scan(eng: &Engagement, host: &str, ports: Option<&[u16]>) -> Result<Value> {
    eng.validate_live()?;
    eng.assert_host(host)?;

    let h = host.split(':').next().unwrap_or(host);
    // Resolve only if in scope (assert_host already checked string membership)
    let port_list: Vec<u16> = ports
        .map(|p| p.to_vec())
        .unwrap_or_else(|| DEFAULT_PORTS.to_vec());

    let mut open = Vec::new();
    let mut closed = Vec::new();
    let mut errors = Vec::new();

    for port in port_list {
        let target = format!("{h}:{port}");
        match try_connect(&target, Duration::from_millis(400)) {
            Ok(true) => open.push(port),
            Ok(false) => closed.push(port),
            Err(e) => {
                closed.push(port);
                errors.push(json!({"port": port, "error": e}));
            }
        }
    }

    Ok(json!({
        "schema": "aop-recon-v1",
        "module": "recon_scan",
        "host": h,
        "engagement_id": eng.engagement_id,
        "open_ports": open,
        "closed_or_filtered": closed,
        "errors": errors,
        "attck": ["T1595", "T1592"],
        "scope": "in_engagement_only",
        "executed": true,
    }))
}

fn try_connect(addr: &str, timeout: Duration) -> Result<bool, String> {
    let addrs: Vec<SocketAddr> = addr.to_socket_addrs().map_err(|e| e.to_string())?.collect();
    if addrs.is_empty() {
        return Err("resolve_empty".into());
    }
    for a in addrs {
        match TcpStream::connect_timeout(&a, timeout) {
            Ok(_) => return Ok(true),
            Err(_) => continue,
        }
    }
    Ok(false)
}

/// Host-info recon without network: engagement + local OS facts for the operator console.
pub fn recon_hostinfo(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-recon-v1",
        "module": "recon_hostinfo",
        "engagement_id": eng.engagement_id,
        "allowed_hosts": eng.allowed_hosts,
        "allowed_cidrs": eng.allowed_cidrs,
        "allowed_lateral_hosts": eng.allowed_lateral_hosts,
        "c2_bind": eng.c2_bind,
        "transport": eng.transport,
        "operator_hostname": hostname(),
        "operator_os": std::env::consts::OS,
        "operator_arch": std::env::consts::ARCH,
        "attck": ["T1592", "T1082"],
        "executed": true,
        "note": "Local operator environment only — not a remote implant",
    }))
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Deny proof helper for docs/gates.
#[allow(dead_code)]
pub fn recon_scan_out_of_scope(eng: &Engagement, host: &str) -> Result<Value> {
    match recon_scan(eng, host, Some(&[80])) {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("SCOPE") || msg.contains("DENIED") || msg.contains("LATERAL") {
                Ok(json!({
                    "ok": false,
                    "denied": true,
                    "host": host,
                    "error": msg,
                    "policy": "fail_closed_scope",
                }))
            } else {
                Err(anyhow!(msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn recon_scan_rejects_out_of_scope_host() {
        let eng = Engagement::default_lab("recon-test", "lab-auth");
        let err = recon_scan(&eng, "10.0.0.1", Some(&[80])).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("SCOPE") || msg.contains("DENIED"),
            "out-of-scope host must be denied: {msg}"
        );
    }

    #[test]
    fn recon_scan_out_of_scope_helper_returns_denied() {
        let eng = Engagement::default_lab("recon-test", "lab-auth");
        let result = recon_scan_out_of_scope(&eng, "192.168.1.1").unwrap();
        assert_eq!(result["denied"].as_bool(), Some(true));
        assert_eq!(result["ok"].as_bool(), Some(false));
        assert_eq!(result["policy"].as_str(), Some("fail_closed_scope"));
    }

    #[test]
    fn recon_hostinfo_includes_operator_env() {
        let eng = Engagement::default_lab("recon-test", "lab-auth");
        let result = recon_hostinfo(&eng).unwrap();
        assert_eq!(result["module"].as_str(), Some("recon_hostinfo"));
        assert!(result["operator_os"].as_str().is_some());
        assert!(result["operator_arch"].as_str().is_some());
        assert!(result["allowed_hosts"].as_array().is_some());
        assert_eq!(result["executed"].as_bool(), Some(true));
    }

    #[test]
    fn recon_scan_loopback_does_not_error_on_scope() {
        let eng = Engagement::default_lab("recon-test", "lab-auth");
        // Use port 1 which is almost certainly closed — we're testing scope, not connectivity
        let result = recon_scan(&eng, "127.0.0.1", Some(&[1]));
        assert!(
            result.is_ok(),
            "loopback must be in scope: {:?}",
            result.err()
        );
        let v = result.unwrap();
        assert_eq!(v["host"].as_str(), Some("127.0.0.1"));
    }
}
