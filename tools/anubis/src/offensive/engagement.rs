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
    /// When true, `listen --mtls` / `listen` with this flag enables full rustls mTLS.
    /// Default false — plain HTTP remains the default listener path.
    #[serde(default)]
    pub mtls_listen: bool,
    /// Double-authorization second half for live process inject.
    /// Must be true (or program == "red_team") AND CLI `--allow-research-inject`.
    #[serde(default)]
    pub allow_live_inject: bool,
    /// When any operator has a non-empty token_hash, HTTP privileged routes require tokens.
    #[serde(default)]
    pub token_auth_enabled: bool,
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
        let id = format!(
            "eng-{}",
            &hex::encode(Sha256::digest(name.as_bytes()))[..12]
        );
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
            mtls_listen: false,
            allow_live_inject: false,
            token_auth_enabled: false,
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
        // Malformed kill dates hard-fail (no silent ignore).
        let kill =
            chrono::NaiveDate::parse_from_str(&self.kill_date, "%Y-%m-%d").map_err(|_| {
                anyhow!(
                    "ANUBIS_ENGAGE_KILL_DATE_INVALID: expected YYYY-MM-DD, got `{}`",
                    self.kill_date
                )
            })?;
        let today = Utc::now().date_naive();
        if today > kill {
            return Err(anyhow!(
                "ANUBIS_ENGAGE_KILL_DATE: engagement expired on {}",
                self.kill_date
            ));
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

    /// Recompute content hash of all fields except `content_hash` and compare to stored.
    pub fn verify_content_hash(&self) -> Result<()> {
        if self.content_hash.trim().is_empty() {
            return Err(anyhow!(
                "ANUBIS_ENGAGE_HASH_MISSING: engagement.json has empty content_hash; re-init or run engage rehash (no silent migrate)"
            ));
        }
        let mut clone = self.clone();
        clone.content_hash.clear();
        let body = serde_json::to_vec(&clone).map_err(|e| anyhow!("ANUBIS_ENGAGE_HASH: {e}"))?;
        let recomputed = hex::encode(Sha256::digest(&body));
        if recomputed != self.content_hash {
            return Err(anyhow!(
                "ANUBIS_ENGAGE_HASH_MISMATCH: stored content_hash does not match engagement body (tamper or edit without rehash)"
            ));
        }
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
            return Err(anyhow!("ANUBIS_ENGAGE_TARGET_MISSING: {}", path.display()));
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

    /// Multi-operator token auth: if the named operator has a token_hash, require a match.
    /// Operators with empty token_hash remain local/unauthenticated (lab default).
    pub fn assert_operator_token(&self, operator: &str, token: Option<&str>) -> Result<()> {
        let op = self
            .operators
            .iter()
            .find(|o| o.name == operator)
            .ok_or_else(|| anyhow!("ANUBIS_RBAC_UNKNOWN_OPERATOR: {operator}"))?;
        if op.token_hash.is_empty() {
            if self.token_auth_enabled {
                // Global token auth on, but this operator was not issued a token — deny
                // privileged use unless they get one issued.
                return Err(anyhow!(
                    "ANUBIS_TOKEN_NOT_ISSUED: operator `{operator}` has no token_hash; run operator-token-issue"
                ));
            }
            return Ok(());
        }
        let Some(t) = token.filter(|s| !s.trim().is_empty()) else {
            return Err(anyhow!(
                "ANUBIS_TOKEN_REQUIRED: operator `{operator}` requires X-Anubis-Token / --token"
            ));
        };
        let h = crypto::hash_token(t.trim());
        if h != op.token_hash {
            return Err(anyhow!("ANUBIS_TOKEN_INVALID: operator `{operator}`"));
        }
        Ok(())
    }

    /// Role + optional token gate for privileged console/CLI actions.
    pub fn assert_auth(&self, operator: &str, min: Role, token: Option<&str>) -> Result<()> {
        self.assert_role(operator, min)?;
        self.assert_operator_token(operator, token)
    }

    /// Second half of double authorization for live inject.
    pub fn live_inject_engagement_authorized(&self) -> bool {
        self.allow_live_inject || self.program.eq_ignore_ascii_case("red_team")
    }

    pub fn rehash(&mut self) {
        self.content_hash.clear();
        let body = serde_json::to_vec(self).unwrap_or_default();
        self.content_hash = hex::encode(Sha256::digest(&body));
    }
}

/// Issue (or rotate) an API token for an operator. Returns cleartext once; only the hash is stored.
pub fn operator_token_issue(engage_dir: &Path, operator: &str) -> Result<(String, Engagement)> {
    let path = if engage_dir.is_dir() {
        engage_dir.join("engagement.json")
    } else {
        engage_dir.to_path_buf()
    };
    let raw = fs::read_to_string(&path)
        .map_err(|e| anyhow!("ANUBIS_ENGAGE_LOAD: {}: {e}", path.display()))?;
    let mut eng: Engagement =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_ENGAGE_PARSE: {e}"))?;
    let token = crypto::issue_token();
    let hash = crypto::hash_token(&token);
    let op = eng
        .operators
        .iter_mut()
        .find(|o| o.name == operator)
        .ok_or_else(|| anyhow!("ANUBIS_RBAC_UNKNOWN_OPERATOR: {operator}"))?;
    op.token_hash = hash;
    eng.token_auth_enabled = eng.operators.iter().any(|o| !o.token_hash.is_empty());
    eng.rehash();
    fs::write(&path, serde_json::to_string_pretty(&eng)?)?;
    Ok((token, eng))
}

/// Clear token_hash for an operator (disables token gate for that operator).
pub fn operator_token_revoke(engage_dir: &Path, operator: &str) -> Result<Engagement> {
    let path = if engage_dir.is_dir() {
        engage_dir.join("engagement.json")
    } else {
        engage_dir.to_path_buf()
    };
    let raw = fs::read_to_string(&path)
        .map_err(|e| anyhow!("ANUBIS_ENGAGE_LOAD: {}: {e}", path.display()))?;
    let mut eng: Engagement =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_ENGAGE_PARSE: {e}"))?;
    let op = eng
        .operators
        .iter_mut()
        .find(|o| o.name == operator)
        .ok_or_else(|| anyhow!("ANUBIS_RBAC_UNKNOWN_OPERATOR: {operator}"))?;
    op.token_hash.clear();
    eng.token_auth_enabled = eng.operators.iter().any(|o| !o.token_hash.is_empty());
    eng.rehash();
    fs::write(&path, serde_json::to_string_pretty(&eng)?)?;
    Ok(eng)
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
    // Receipt MAC key (LAB_REAL HMAC; mode 0600). Not printed in engage-status.
    let _ = super::receipts::ensure_mac_key(dir);
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
    let eng: Engagement =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_ENGAGE_PARSE: {}", e))?;
    // No silent migrate: missing PSK / hash must be fixed explicitly (re-init or rehash).
    if eng.psk_hex.trim().is_empty() {
        return Err(anyhow!(
            "ANUBIS_ENGAGE_NO_PSK: engagement missing psk_hex; re-init engagement (silent PSK generation removed)"
        ));
    }
    eng.verify_content_hash()?;
    eng.validate_live()?;
    Ok(eng)
}

/// Recompute and persist `content_hash` after intentional engagement edits
/// (e.g. gate scripts adjusting dns_bind). Does not validate live kill-date
/// first so operators can rehash before fixing other fields.
pub fn rehash_engagement_file(path: &Path) -> Result<Engagement> {
    let p = if path.is_dir() {
        path.join("engagement.json")
    } else {
        path.to_path_buf()
    };
    let raw = fs::read_to_string(&p)
        .map_err(|e| anyhow!("ANUBIS_ENGAGE_LOAD: {}: {}", p.display(), e))?;
    let mut eng: Engagement =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_ENGAGE_PARSE: {}", e))?;
    eng.rehash();
    fs::write(&p, serde_json::to_string_pretty(&eng)?)
        .map_err(|e| anyhow!("ANUBIS_ENGAGE_WRITE: {}: {e}", p.display()))?;
    // Confirm the sealed hash verifies.
    eng.verify_content_hash()?;
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
        "mtls_listen": eng.mtls_listen,
        "allow_live_inject": eng.allow_live_inject,
        "token_auth_enabled": eng.token_auth_enabled,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_date_malformed_fails() {
        let mut eng = Engagement::default_lab("t", "auth-ok");
        eng.kill_date = "not-a-date".into();
        eng.rehash();
        let err = eng.validate_live().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_ENGAGE_KILL_DATE_INVALID"), "got {err}");
    }

    #[test]
    fn kill_date_expired_fails() {
        let mut eng = Engagement::default_lab("t", "auth-ok");
        eng.kill_date = "2000-01-01".into();
        eng.rehash();
        let err = eng.validate_live().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_ENGAGE_KILL_DATE"), "got {err}");
    }

    #[test]
    fn content_hash_mismatch_detected() {
        let mut eng = Engagement::default_lab("t", "auth-ok");
        eng.rehash();
        eng.name = "tampered".into(); // body changed without rehash
        let err = eng.verify_content_hash().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_ENGAGE_HASH_MISMATCH"), "got {err}");
    }

    #[test]
    fn content_hash_empty_fails() {
        let eng = Engagement::default_lab("t", "auth-ok");
        // default_lab leaves content_hash empty until rehash
        let err = eng.verify_content_hash().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_ENGAGE_HASH_MISSING"), "got {err}");
    }

    #[test]
    fn content_hash_ok_after_rehash() {
        let mut eng = Engagement::default_lab("t", "auth-ok");
        eng.rehash();
        eng.verify_content_hash().unwrap();
        eng.validate_live().unwrap();
    }

    #[test]
    fn rehash_file_after_edit_restores_verify() {
        let dir = std::env::temp_dir().join(format!("anubis-rehash-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut eng = Engagement::default_lab("rehash", "auth-ok");
        eng.rehash();
        let path = dir.join("engagement.json");
        fs::write(&path, serde_json::to_string_pretty(&eng).unwrap()).unwrap();
        // Mutate without rehash (simulates gate script edits).
        let mut d: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        d["dns_bind"] = serde_json::json!("127.0.0.1:55353");
        fs::write(&path, serde_json::to_string_pretty(&d).unwrap()).unwrap();
        assert!(load_engagement(&dir).is_err());
        rehash_engagement_file(&dir).unwrap();
        load_engagement(&dir).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}
