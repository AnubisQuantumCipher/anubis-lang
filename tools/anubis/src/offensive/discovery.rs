//! Discovery module — T10 (TA0007).
//!
//! Structured enumeration of networks, systems, users, and files inside
//! VZ guests. Host-side: engagement scope inventory. Guest-side: active
//! discovery against in-scope targets.

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// System information discovery — uname, users, processes, env.
pub fn system_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let uname = cmd_output("uname", &["-a"]);
    let hostname = cmd_output("hostname", &[]);
    let whoami = cmd_output("whoami", &[]);
    let id_out = cmd_output("id", &[]);
    let uptime = cmd_output("uptime", &[]);
    let df = cmd_output("df", &["-h"]);
    let groups = cmd_output("groups", &[]);
    let sw_vers = cmd_output("sw_vers", &[]);

    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "system_enum",
        "engagement_id": eng.engagement_id,
        "uname": uname,
        "hostname": hostname,
        "whoami": whoami,
        "id": id_out,
        "uptime": uptime,
        "df": df,
        "groups": groups,
        "sw_vers": sw_vers,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "attck": ["T1082", "T1033"],
        "executed": true,
    }))
}

/// Network interface and routing enumeration.
pub fn network_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let ifconfig = cmd_output("ifconfig", &[]);
    let netstat = cmd_output("netstat", &["-rn"]);
    let arp = cmd_output("arp", &["-a"]);
    let dns_config = cmd_output("scutil", &["--dns"]);

    let mut listening_ports: Vec<Value> = Vec::new();
    let netstat_l = cmd_output("netstat", &["-an"]);
    for line in netstat_l.lines() {
        if line.contains("LISTEN") || line.contains("*.") {
            listening_ports.push(json!(line.trim()));
        }
    }

    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "network_enum",
        "engagement_id": eng.engagement_id,
        "ifconfig": ifconfig,
        "routing_table": netstat,
        "arp_cache": arp,
        "dns_config": dns_config,
        "listening_ports": listening_ports,
        "attck": ["T1016", "T1049", "T1018"],
        "executed": true,
    }))
}

/// Process enumeration — running processes with user context.
pub fn process_enum(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let ps = cmd_output("ps", &["auxww"]);
    let mut processes: Vec<Value> = Vec::new();
    let mut security_relevant: Vec<Value> = Vec::new();

    let interesting = [
        "ssh", "sshd", "httpd", "nginx", "docker", "containerd",
        "postgres", "mysql", "mongod", "redis", "elasticsearch",
        "consul", "vault", "kubectl", "kubelet", "osqueryd",
        "falcon", "crowdstrike", "sentinel", "defender",
    ];

    for line in ps.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 11 {
            continue;
        }
        let user = parts[0];
        let pid = parts[1];
        let cmd = parts[10..].join(" ");
        let cmd_lower = cmd.to_lowercase();

        for pattern in &interesting {
            if cmd_lower.contains(pattern) {
                security_relevant.push(json!({
                    "user": user,
                    "pid": pid,
                    "command": cmd,
                    "matched": pattern,
                }));
                break;
            }
        }
        processes.push(json!({
            "user": user,
            "pid": pid,
            "command": cmd,
        }));
    }

    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "process_enum",
        "engagement_id": eng.engagement_id,
        "total_processes": processes.len(),
        "security_relevant": security_relevant,
        "attck": ["T1057"],
        "executed": true,
    }))
}

/// File and directory discovery — find sensitive files.
pub fn file_discovery(eng: &Engagement, search_root: &Path) -> Result<Value> {
    eng.validate_live()?;
    let mut sensitive_files: Vec<Value> = Vec::new();

    let patterns = [
        ("*.pem", "TLS/SSH key material"),
        ("*.key", "Private key"),
        ("*.pfx", "PKCS#12 certificate"),
        ("*.p12", "PKCS#12 certificate"),
        ("*.keystore", "Java keystore"),
        (".env", "Environment config"),
        ("*.conf", "Configuration file"),
        ("*.cfg", "Configuration file"),
        ("shadow", "Shadow password file"),
        ("passwd", "Password file"),
        (".bash_history", "Shell history"),
        (".zsh_history", "Shell history"),
        ("*.sqlite", "SQLite database"),
        ("*.db", "Database file"),
        ("credentials*", "Credential file"),
        ("secret*", "Secret file"),
        ("token*", "Token file"),
        ("*.kdbx", "KeePass database"),
    ];

    for (pattern, description) in &patterns {
        let output = Command::new("find")
            .args([
                search_root.to_str().unwrap_or("."),
                "-maxdepth", "4",
                "-name", pattern,
                "-type", "f",
            ])
            .output();

        if let Ok(o) = output {
            if o.status.success() {
                let stdout = String::from_utf8_lossy(&o.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let size = fs::metadata(trimmed).map(|m| m.len()).unwrap_or(0);
                    sensitive_files.push(json!({
                        "path": trimmed,
                        "pattern": pattern,
                        "description": description,
                        "size_bytes": size,
                    }));
                }
            }
        }
    }

    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "file_discovery",
        "engagement_id": eng.engagement_id,
        "search_root": search_root.display().to_string(),
        "sensitive_files_found": sensitive_files.len(),
        "files": sensitive_files,
        "attck": ["T1083", "T1005"],
        "executed": true,
    }))
}

/// Service banner grabbing on in-scope hosts.
pub fn service_banner(
    eng: &Engagement,
    host: &str,
    ports: &[u16],
) -> Result<Value> {
    eng.validate_live()?;
    eng.assert_host(host)?;

    let mut results: Vec<Value> = Vec::new();
    for &port in ports {
        let addr = format!("{host}:{port}");
        let addrs: Vec<_> = addr.to_socket_addrs().into_iter().flatten().collect();
        let mut banner = String::new();
        let mut open = false;

        for a in addrs {
            if let Ok(stream) = TcpStream::connect_timeout(&a, Duration::from_millis(500)) {
                open = true;
                let _ = stream.set_read_timeout(Some(Duration::from_millis(1000)));
                let mut buf = [0u8; 1024];
                if let Ok(n) = std::io::Read::read(&mut &stream, &mut buf) {
                    if n > 0 {
                        banner = String::from_utf8_lossy(&buf[..n])
                            .chars()
                            .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                            .take(256)
                            .collect();
                    }
                }
                break;
            }
        }

        results.push(json!({
            "port": port,
            "open": open,
            "banner": if banner.is_empty() { None } else { Some(&banner) },
        }));
    }

    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "service_banner",
        "engagement_id": eng.engagement_id,
        "host": host,
        "results": results,
        "attck": ["T1046"],
        "executed": true,
    }))
}

/// Cloud metadata service probe (PLAN_ONLY for safety).
pub fn cloud_metadata_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "cloud_metadata_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1552.005"],
        "endpoints": {
            "aws": "http://169.254.169.254/latest/meta-data/",
            "gcp": "http://metadata.google.internal/computeMetadata/v1/",
            "azure": "http://169.254.169.254/metadata/instance?api-version=2021-02-01",
        },
        "steps": [
            "Verify target is a cloud instance (engagement scope)",
            "Probe metadata endpoint from VZ guest with curl",
            "Extract IAM role credentials (AWS), service account tokens (GCP/Azure)",
            "Document findings — NEVER exfiltrate real cloud credentials",
        ],
        "policy": {
            "requires_vz_guest": true,
            "cloud_credentials_are_sensitive": true,
        },
    }))
}

/// Active Directory enumeration plan (PLAN_ONLY — Windows tranche).
pub fn ad_enum_plan(eng: &Engagement, domain: &str) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-discovery-v1",
        "module": "ad_enum_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "domain": domain,
        "attck": ["T1087.002", "T1069.002"],
        "tools": [
            "BloodHound / SharpHound (graph-based AD analysis)",
            "PowerView / ADModule (PowerShell AD enumeration)",
            "ldapsearch (LDAP queries from Linux)",
            "CrackMapExec / NetExec (network-based enumeration)",
        ],
        "steps": [
            format!("Verify domain {domain} is in engagement scope"),
            "Run from VZ guest with domain-joined credentials",
            "Enumerate: users, groups, GPOs, trusts, SPNs, ACLs",
            "Identify Kerberoastable accounts (SPN-enabled service accounts)",
            "Map attack paths to Domain Admin",
            "Document findings for purple-team debrief",
        ],
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
            "windows_only": true,
        },
    }))
}

fn cmd_output(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn system_enum_returns_os_info() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let result = system_enum(&eng).unwrap();
        assert_eq!(result["module"], "system_enum");
        assert_eq!(result["os"], std::env::consts::OS);
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn network_enum_runs_without_panic() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let result = network_enum(&eng);
        assert!(result.is_ok());
    }

    #[test]
    fn process_enum_finds_processes() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let result = process_enum(&eng).unwrap();
        assert!(result["total_processes"].as_u64().unwrap() > 0);
    }

    #[test]
    fn file_discovery_scans_temp() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let result = file_discovery(&eng, &std::env::temp_dir()).unwrap();
        assert_eq!(result["module"], "file_discovery");
        assert!(result["files"].is_array());
    }

    #[test]
    fn service_banner_rejects_out_of_scope() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let err = service_banner(&eng, "10.99.99.99", &[80]).unwrap_err().to_string();
        assert!(err.contains("SCOPE") || err.contains("DENIED"), "{err}");
    }

    #[test]
    fn cloud_metadata_plan_is_plan_only() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let result = cloud_metadata_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["executed"], false);
    }

    #[test]
    fn ad_enum_plan_is_plan_only() {
        let eng = Engagement::default_lab("disc-test", "lab-auth");
        let result = ad_enum_plan(&eng, "corp.local").unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }
}
