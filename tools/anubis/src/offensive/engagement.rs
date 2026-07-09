//! Engagement lifecycle: init, load, status, evidence binding, RBAC, crypto material.

use super::crypto;
use super::scope;
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Operator,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operator {
    pub name: String,
    pub role: Role,
    /// Optional API token hash (sha256 hex). Empty = local-only CLI.
    #[serde(default)]
    pub token_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Engagement {
    pub schema_version: String,
    pub engagement_id: String,
    pub name: String,
    pub authorization: String,
    pub program: String,
    pub created_at: String,
    pub kill_date: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_cidrs: Vec<String>,
    pub allowed_paths: Vec<String>,
    /// Hosts allowed for lateral movement (must also pass host scope).
    #[serde(default)]
    pub allowed_lateral_hosts: Vec<String>,
    pub c2_bind: String,
    /// http | dns | uds | multi
    #[serde(default = "default_transport")]
    pub transport: String,
    /// DNS bind for dns transport (lab).
    #[serde(default = "default_dns_bind")]
    pub dns_bind: String,
    /// Unix domain socket path for uds transport.
    #[serde(default = "default_uds_path")]
    pub uds_path: String,
    pub network_egress: bool,
    pub allow_non_loopback_bind: bool,
    /// AES-256-GCM PSK (hex). Required for aop-2.
    #[serde(default)]
    pub psk_hex: String,
    /// Default agent sleep base (ms).
    #[serde(default = "default_sleep")]
    pub sleep_ms: u64,
    /// Jitter percent 0-100.
    #[serde(default = "default_jitter")]
    pub jitter_pct: u8,
    /// Enable encrypted beacons (aop-2).
    #[serde(default = "default_true")]
    pub encrypt_beacons: bool,
    /// mTLS certs generated under engagement/certs (ready; HTTP listener remains default).
    #[serde(default)]
    pub mtls_ready: bool,
    #[serde(default)]
    pub operators: Vec<Operator>,
    pub notes: String,
    #[serde(default)]
    pub content_hash: String,
}

fn default_transport() -> String {
    "http".into()
}
fn default_dns_bind() -> String {
    "127.0.0.1:5353".into()
}
fn default_uds_path() -> String {
    "/tmp/anubis-aop.sock".into()
}
fn default_sleep() -> u64 {
    2000
}
fn default_jitter() -> u8 {
    20
}
fn default_true() -> bool {
    true
}

impl Engagement {
    pub fn default_lab(name: &str, authorization: &str) -> Self {
        let id = format!("eng-{}", &hex::encode(Sha256::digest(name.as_bytes()))[..12]);
        Self {
            schema_version: "2.0".into(),
            engagement_id: id,
            name: name.into(),
            authorization: authorization.into(),
            program: "lab".into(),
            created_at: Utc::now().to_rfc3339(),
            kill_date: "2099-01-01".into(),
            allowed_hosts: vec!["127.0.0.1".into(), "localhost".into(), "::1".into()],
            allowed_cidrs: vec!["127.0.0.0/8".into()],
            allowed_paths: vec![
                "poc_kit".into(),
                "poc_kit/bin".into(),
                "out".into(),
                "/tmp/anubis-lab".into(),
                ".".into(),
            ],
            allowed_lateral_hosts: vec!["127.0.0.1".into(), "localhost".into()],
            c2_bind: "127.0.0.1:4444".into(),
            transport: "multi".into(),
            dns_bind: default_dns_bind(),
            uds_path: default_uds_path(),
            network_egress: false,
            allow_non_loopback_bind: false,
            psk_hex: crypto::generate_psk_hex(),
            sleep_ms: 2000,
            jitter_pct: 20,
            encrypt_beacons: true,
            mtls_ready: false,
            operators: vec![
                Operator {
                    name: "admin".into(),
                    role: Role::Admin,
                    token_hash: String::new(),
                },
                Operator {
                    name: "operator".into(),
                    role: Role::Operator,
                    token_hash: String::new(),
                },
                Operator {
                    name: "readonly".into(),
                    role: Role::ReadOnly,
                    token_hash: String::new(),
                },
            ],
            notes: "AOP v2 lab engagement — encrypted beacons, loopback C2, multi transport."
                .into(),
            content_hash: String::new(),
        }
    }

    pub fn validate_live(&self) -> Result<()> {
        if self.authorization.trim().is_empty() {
            return Err(anyhow!(
                "ANUBIS_ENGAGE_NO_AUTHORIZATION: engagement.authorization is required"
            ));
        }
        if let Ok(kill) = chrono::NaiveDate::parse_from_str(&self.kill_date, "%Y-%m-%d") {
            let today = Utc::now().date_naive();
            if today > kill {
                return Err(anyhow!(
                    "ANUBIS_ENGAGE_KILL_DATE: engagement expired on {}",
                    self.kill_date
                ));
            }
        }
        if self.encrypt_beacons && self.psk_hex.trim().is_empty() {
            return Err(anyhow!(
                "ANUBIS_ENGAGE_NO_PSK: encrypt_beacons requires psk_hex"
            ));
        }
        scope::bind_addr_in_scope(
            &self.c2_bind,
            self.allow_non_loopback_bind && self.network_egress,
            &self.allowed_hosts,
        )?;
        Ok(())
    }

    pub fn assert_host(&self, host: &str) -> Result<()> {
        self.validate_live()?;
        // Route through structured target validation (same allow-lists).
        scope::target_in_scope(
            &scope::AllowedTarget {
                kind: scope::TargetKind::Host,
                value: host.to_string(),
                notes: "assert_host".into(),
            },
            &self.allowed_hosts,
            &self.allowed_cidrs,
            &self.allowed_paths,
        )
    }

    pub fn assert_lateral_host(&self, host: &str) -> Result<()> {
        self.assert_host(host)?;
        let h = host.split(':').next().unwrap_or(host);
        if self
            .allowed_lateral_hosts
            .iter()
            .any(|x| x.eq_ignore_ascii_case(h) || x.eq_ignore_ascii_case(host))
        {
            return Ok(());
        }
        Err(anyhow!(
            "ANUBIS_LATERAL_DENIED: host `{h}` not in allowed_lateral_hosts"
        ))
    }

    pub fn assert_path(&self, path: &Path) -> Result<()> {
        self.validate_live()?;
        scope::target_in_scope(
            &scope::AllowedTarget {
                kind: scope::TargetKind::LocalPath,
                value: path.display().to_string(),
                notes: "assert_path".into(),
            },
            &self.allowed_hosts,
            &self.allowed_cidrs,
            &self.allowed_paths,
        )
    }

    pub fn assert_target_binary(&self, path: &Path) -> Result<()> {
        self.assert_path(path)?;
        if !path.exists() {
            return Err(anyhow!(
                "ANUBIS_ENGAGE_TARGET_MISSING: {}",
                path.display()
            ));
        }
        Ok(())
    }

    pub fn assert_role(&self, operator: &str, min: Role) -> Result<()> {
        let op = self
            .operators
            .iter()
            .find(|o| o.name == operator)
            .ok_or_else(|| anyhow!("ANUBIS_RBAC_UNKNOWN_OPERATOR: {operator}"))?;
        let rank = |r: &Role| match r {
            Role::ReadOnly => 1,
            Role::Operator => 2,
            Role::Admin => 3,
        };
        if rank(&op.role) < rank(&min) {
            return Err(anyhow!(
                "ANUBIS_RBAC_DENIED: operator `{operator}` role {:?} < {:?}",
                op.role,
                min
            ));
        }
        Ok(())
    }

    pub fn rehash(&mut self) {
        self.content_hash.clear();
        let body = serde_json::to_vec(self).unwrap_or_default();
        self.content_hash = hex::encode(Sha256::digest(&body));
    }
}

pub fn engage_init(dir: &Path, name: &str, authorization: &str) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut eng = Engagement::default_lab(name, authorization);
    // Always allow the engagement workspace itself (agents, loot, packs).
    let dir_s = dir.display().to_string();
    if !eng.allowed_paths.iter().any(|p| p == &dir_s) {
        eng.allowed_paths.push(dir_s);
    }
    if let Ok(canon) = dir.canonicalize() {
        let c = canon.display().to_string();
        if !eng.allowed_paths.iter().any(|p| p == &c) {
            eng.allowed_paths.push(c);
        }
    }
    // mTLS-ready cert material
    match crypto::generate_lab_certs(dir, &eng.name) {
        Ok(_) => eng.mtls_ready = true,
        Err(e) => {
            eng.notes = format!("{} (cert gen warning: {e})", eng.notes);
        }
    }
    eng.rehash();
    let path = dir.join("engagement.json");
    fs::write(&path, serde_json::to_string_pretty(&eng)?)?;
    for sub in [
        "agents",
        "listeners",
        "tasks",
        "loot",
        "evidence",
        "modules",
        "certs",
        "persistence",
        "packs",
    ] {
        fs::create_dir_all(dir.join(sub))?;
    }
    let readme = format!(
        "# Anubis Engagement: {}\n\nID: `{}`\nAuth: {}\nPSK: (see engagement.json psk_hex)\nC2: `{}`\nTransport: {}\nProtocol: aop-2 encrypted\nmTLS certs: {}\nKill: {}\n",
        eng.name,
        eng.engagement_id,
        eng.authorization,
        eng.c2_bind,
        eng.transport,
        eng.mtls_ready,
        eng.kill_date
    );
    fs::write(dir.join("README.md"), readme)?;
    Ok(path)
}

pub fn load_engagement(path: &Path) -> Result<Engagement> {
    let p = if path.is_dir() {
        path.join("engagement.json")
    } else {
        path.to_path_buf()
    };
    let raw = fs::read_to_string(&p)
        .map_err(|e| anyhow!("ANUBIS_ENGAGE_LOAD: {}: {}", p.display(), e))?;
    let mut eng: Engagement = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("ANUBIS_ENGAGE_PARSE: {}", e))?;
    // migrate v1 engagements missing fields
    if eng.psk_hex.is_empty() {
        eng.psk_hex = crypto::generate_psk_hex();
    }
    eng.validate_live()?;
    Ok(eng)
}

pub fn engage_status(path: &Path) -> Result<serde_json::Value> {
    let eng = load_engagement(path)?;
    let allowed_targets = scope::build_allowed_targets(
        &eng.allowed_hosts,
        &eng.allowed_cidrs,
        &eng.allowed_paths,
        &eng.allowed_lateral_hosts,
    );
    Ok(serde_json::json!({
        "engagement_id": eng.engagement_id,
        "name": eng.name,
        "authorization": eng.authorization,
        "program": eng.program,
        "kill_date": eng.kill_date,
        "c2_bind": eng.c2_bind,
        "transport": eng.transport,
        "dns_bind": eng.dns_bind,
        "uds_path": eng.uds_path,
        "network_egress": eng.network_egress,
        "encrypt_beacons": eng.encrypt_beacons,
        "mtls_ready": eng.mtls_ready,
        "jitter_pct": eng.jitter_pct,
        "sleep_ms": eng.sleep_ms,
        "operators": eng.operators,
        "allowed_hosts": eng.allowed_hosts,
        "allowed_cidrs": eng.allowed_cidrs,
        "allowed_lateral_hosts": eng.allowed_lateral_hosts,
        "allowed_paths": eng.allowed_paths,
        "allowed_targets": allowed_targets,
        "psk_present": !eng.psk_hex.is_empty(),
        "content_hash": eng.content_hash,
        "live": true,
        "protocol": super::protocol::PROTOCOL_VERSION,
        "rbac": {
            "roles": ["admin", "operator", "read_only"],
            "queue_requires": "operator",
            "admin_status_requires": "admin",
        },
        "receipts": super::receipts::verify_chain(path).unwrap_or_else(|e| serde_json::json!({
            "ok": false,
            "error": e.to_string(),
        })),
    }))
}
