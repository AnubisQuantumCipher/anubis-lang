//! Guest-bound single-use research run capability.
//!
//! Host orchestrator mints a capability binding engagement, program, binary,
//! guest identity, effects, and a nonce. Guest execution must validate it
//! before crash-capable work. Fail-closed on expiry, replay, wrong guest,
//! or digest mismatch.
//!
//! LAB_REAL: HMAC over structured fields using engagement receipt MAC key
//! (or dedicated capability key). Not an offline PKI attestation.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const CAP_SCHEMA: &str = "anubis-run-cap-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCapability {
    pub schema: String,
    pub engagement_id: String,
    pub engagement_hash: String,
    pub authorization_digest: String,
    pub source_digest: String,
    pub compiler_digest: String,
    pub program_digest: String,
    pub guest_id: String,
    pub base_digest: String,
    pub confinement_digest: String,
    pub allowed_effects: Vec<String>,
    pub allowed_targets: Vec<String>,
    pub issued_unix: u64,
    pub expires_unix: u64,
    pub nonce: String,
    pub operator: String,
    /// HMAC over canonical material (excludes `mac` itself).
    pub mac: String,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn material(cap: &RunCapability) -> String {
    let effects = cap.allowed_effects.join(",");
    let targets = cap.allowed_targets.join(",");
    format!(
        "{schema}|{eng}|{eh}|{ad}|{sd}|{cd}|{pd}|{gid}|{bd}|{cfd}|{effects}|{targets}|{iss}|{exp}|{nonce}|{op}",
        schema = cap.schema,
        eng = cap.engagement_id,
        eh = cap.engagement_hash,
        ad = cap.authorization_digest,
        sd = cap.source_digest,
        cd = cap.compiler_digest,
        pd = cap.program_digest,
        gid = cap.guest_id,
        bd = cap.base_digest,
        cfd = cap.confinement_digest,
        effects = effects,
        targets = targets,
        iss = cap.issued_unix,
        exp = cap.expires_unix,
        nonce = cap.nonce,
        op = cap.operator,
    )
}

fn mac_hex(key: &str, material: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"anubis-run-cap-mac-v1|");
    h.update(key.as_bytes());
    h.update(b"|");
    h.update(material.as_bytes());
    hex::encode(h.finalize())
}

fn random_nonce() -> String {
    let mut buf = [0u8; 16];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub struct MintParams<'a> {
    pub key: &'a str,
    pub engagement_id: &'a str,
    pub engagement_hash: &'a str,
    pub authorization_digest: &'a str,
    pub source_digest: &'a str,
    pub compiler_digest: &'a str,
    pub program_digest: &'a str,
    pub guest_id: &'a str,
    pub base_digest: &'a str,
    pub confinement_digest: &'a str,
    pub allowed_effects: Vec<String>,
    pub allowed_targets: Vec<String>,
    pub operator: &'a str,
    pub ttl_secs: u64,
}

/// Mint a capability (host side). `ttl_secs` bounds lifetime.
pub fn mint(params: MintParams<'_>) -> RunCapability {
    let issued = now_unix();
    let mut cap = RunCapability {
        schema: CAP_SCHEMA.into(),
        engagement_id: params.engagement_id.into(),
        engagement_hash: params.engagement_hash.into(),
        authorization_digest: params.authorization_digest.into(),
        source_digest: params.source_digest.into(),
        compiler_digest: params.compiler_digest.into(),
        program_digest: params.program_digest.into(),
        guest_id: params.guest_id.into(),
        base_digest: params.base_digest.into(),
        confinement_digest: params.confinement_digest.into(),
        allowed_effects: params.allowed_effects,
        allowed_targets: params.allowed_targets,
        issued_unix: issued,
        expires_unix: issued.saturating_add(params.ttl_secs.max(1)),
        nonce: random_nonce(),
        operator: params.operator.into(),
        mac: String::new(),
    };
    cap.mac = mac_hex(params.key, &material(&cap));
    cap
}

/// Validation context supplied by the guest/runtime.
#[derive(Debug, Clone)]
pub struct ValidateCtx<'a> {
    pub key: &'a str,
    pub guest_id: &'a str,
    pub program_digest: &'a str,
    pub engagement_id: &'a str,
    pub engagement_hash: &'a str,
    pub effect: Option<&'a str>,
    pub target: Option<&'a str>,
    /// Nonces already consumed (replay window).
    pub seen_nonces: &'a Mutex<HashSet<String>>,
}

/// Offline structural checks (no key, no guest context, no nonce consume).
///
/// Used by the portable `anubis evidence-verify` path. Fail-closed on empty
/// critical fields or inverted lifetime.
pub fn verify_offline_structural(cap: &RunCapability) -> Result<()> {
    if cap.schema != CAP_SCHEMA {
        return Err(anyhow!("ANUBIS_RUN_CAP_SCHEMA: {}", cap.schema));
    }
    if cap.guest_id.trim().is_empty() {
        return Err(anyhow!("ANUBIS_RUN_CAP_EMPTY_GUEST"));
    }
    if cap.program_digest.trim().is_empty() {
        return Err(anyhow!("ANUBIS_RUN_CAP_EMPTY_PROGRAM_DIGEST"));
    }
    if cap.engagement_id.trim().is_empty() {
        return Err(anyhow!("ANUBIS_RUN_CAP_EMPTY_ENGAGEMENT"));
    }
    if cap.mac.trim().is_empty() {
        return Err(anyhow!("ANUBIS_RUN_CAP_EMPTY_MAC"));
    }
    if cap.nonce.trim().is_empty() {
        return Err(anyhow!("ANUBIS_RUN_CAP_EMPTY_NONCE"));
    }
    if cap.expires_unix < cap.issued_unix {
        return Err(anyhow!("ANUBIS_RUN_CAP_LIFETIME_INVERTED"));
    }
    if cap.allowed_effects.is_empty() {
        return Err(anyhow!("ANUBIS_RUN_CAP_EMPTY_EFFECTS"));
    }
    Ok(())
}

/// Offline MAC verification without consuming the nonce (portable auditor).
///
/// Does **not** check guest/program binding — those need a live ValidateCtx.
/// Classification: LAB_REAL_HMAC (not Ed25519).
pub fn verify_offline_mac(cap: &RunCapability, key: &str) -> Result<()> {
    verify_offline_structural(cap)?;
    let expect = mac_hex(key, &material(cap));
    if expect != cap.mac {
        return Err(anyhow!("ANUBIS_RUN_CAP_MAC_INVALID"));
    }
    Ok(())
}

/// Validate and **consume** nonce (single-use). Fail closed on any mismatch.
pub fn validate_and_consume(cap: &RunCapability, ctx: &ValidateCtx<'_>) -> Result<()> {
    if cap.schema != CAP_SCHEMA {
        return Err(anyhow!("ANUBIS_RUN_CAP_SCHEMA: {}", cap.schema));
    }
    let expect = mac_hex(ctx.key, &material(cap));
    if expect != cap.mac {
        return Err(anyhow!("ANUBIS_RUN_CAP_MAC_INVALID"));
    }
    let now = now_unix();
    if now > cap.expires_unix {
        return Err(anyhow!(
            "ANUBIS_RUN_CAP_EXPIRED: expires_unix={} now={}",
            cap.expires_unix,
            now
        ));
    }
    if now + 3600 < cap.issued_unix {
        // Issued in the far future — reject clock skew abuse.
        return Err(anyhow!("ANUBIS_RUN_CAP_NOT_YET_VALID"));
    }
    if cap.guest_id != ctx.guest_id {
        return Err(anyhow!(
            "ANUBIS_RUN_CAP_WRONG_GUEST: expected {} got {}",
            cap.guest_id,
            ctx.guest_id
        ));
    }
    if cap.program_digest != ctx.program_digest {
        return Err(anyhow!("ANUBIS_RUN_CAP_PROGRAM_MISMATCH"));
    }
    if cap.engagement_id != ctx.engagement_id {
        return Err(anyhow!("ANUBIS_RUN_CAP_ENGAGEMENT_MISMATCH"));
    }
    if cap.engagement_hash != ctx.engagement_hash {
        return Err(anyhow!("ANUBIS_RUN_CAP_ENGAGEMENT_HASH_MISMATCH"));
    }
    if let Some(effect) = ctx.effect {
        if !cap.allowed_effects.iter().any(|e| e == effect) {
            return Err(anyhow!(
                "ANUBIS_RUN_CAP_EFFECT_DENIED: `{effect}` not in capability"
            ));
        }
    }
    if let Some(target) = ctx.target {
        if !cap.allowed_targets.is_empty() && !cap.allowed_targets.iter().any(|t| t == target) {
            return Err(anyhow!(
                "ANUBIS_RUN_CAP_TARGET_DENIED: `{target}` not in capability"
            ));
        }
    }
    let mut seen = ctx
        .seen_nonces
        .lock()
        .map_err(|_| anyhow!("ANUBIS_RUN_CAP_NONCE_LOCK"))?;
    if !seen.insert(cap.nonce.clone()) {
        return Err(anyhow!("ANUBIS_RUN_CAP_REPLAY: nonce already consumed"));
    }
    Ok(())
}

pub fn write_cap(path: &Path, cap: &RunCapability) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(cap)?)?;
    Ok(())
}

pub fn read_cap(path: &Path) -> Result<RunCapability> {
    let raw = fs::read_to_string(path)
        .map_err(|e| anyhow!("ANUBIS_RUN_CAP_LOAD: {}: {e}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

/// Default path under engagement for the active capability.
#[allow(dead_code)] // public API for orchestrators / future CLI
pub fn default_cap_path(engage_dir: &Path) -> PathBuf {
    engage_dir.join("evidence/run_capability.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn base_mint(key: &str) -> RunCapability {
        mint(MintParams {
            key,
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            authorization_digest: "auth-bbb",
            source_digest: "src-ccc",
            compiler_digest: "comp-ddd",
            program_digest: "prog-eee",
            guest_id: "guest-tart-1",
            base_digest: "base-fff",
            confinement_digest: "conf-ggg",
            allowed_effects: vec!["process.spawn".into(), "vm.execute".into()],
            allowed_targets: vec!["/tmp/lab/target".into()],
            operator: "operator",
            ttl_secs: 600,
        })
    }

    #[test]
    fn offline_structural_and_mac() {
        let key = "test-key-32-bytes-long-enough!!";
        let cap = base_mint(key);
        verify_offline_structural(&cap).unwrap();
        verify_offline_mac(&cap, key).unwrap();
        assert!(verify_offline_mac(&cap, "wrong").is_err());
        let mut broken = cap.clone();
        broken.guest_id.clear();
        assert!(verify_offline_structural(&broken).is_err());
    }

    #[test]
    fn valid_cap_accepts_once() {
        let key = "test-key-32-bytes-long-enough!!";
        let cap = base_mint(key);
        let seen = Mutex::new(HashSet::new());
        let ctx = ValidateCtx {
            key,
            guest_id: "guest-tart-1",
            program_digest: "prog-eee",
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            effect: Some("process.spawn"),
            target: Some("/tmp/lab/target"),
            seen_nonces: &seen,
        };
        validate_and_consume(&cap, &ctx).unwrap();
        let err = validate_and_consume(&cap, &ctx).unwrap_err().to_string();
        assert!(err.contains("REPLAY"), "got {err}");
    }

    #[test]
    fn wrong_guest_denied() {
        let key = "k";
        let cap = base_mint(key);
        let seen = Mutex::new(HashSet::new());
        let ctx = ValidateCtx {
            key,
            guest_id: "other-guest",
            program_digest: "prog-eee",
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            effect: None,
            target: None,
            seen_nonces: &seen,
        };
        let err = validate_and_consume(&cap, &ctx).unwrap_err().to_string();
        assert!(err.contains("WRONG_GUEST"), "got {err}");
    }

    #[test]
    fn program_mismatch_denied() {
        let key = "k";
        let cap = base_mint(key);
        let seen = Mutex::new(HashSet::new());
        let ctx = ValidateCtx {
            key,
            guest_id: "guest-tart-1",
            program_digest: "wrong",
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            effect: None,
            target: None,
            seen_nonces: &seen,
        };
        assert!(validate_and_consume(&cap, &ctx)
            .unwrap_err()
            .to_string()
            .contains("PROGRAM_MISMATCH"));
    }

    #[test]
    fn effect_denied() {
        let key = "k";
        let cap = base_mint(key);
        let seen = Mutex::new(HashSet::new());
        let ctx = ValidateCtx {
            key,
            guest_id: "guest-tart-1",
            program_digest: "prog-eee",
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            effect: Some("net.connect"),
            target: None,
            seen_nonces: &seen,
        };
        assert!(validate_and_consume(&cap, &ctx)
            .unwrap_err()
            .to_string()
            .contains("EFFECT_DENIED"));
    }

    #[test]
    fn expired_denied() {
        let key = "k";
        let mut cap = base_mint(key);
        cap.expires_unix = 1;
        cap.mac = mac_hex(key, &material(&cap));
        let seen = Mutex::new(HashSet::new());
        let ctx = ValidateCtx {
            key,
            guest_id: "guest-tart-1",
            program_digest: "prog-eee",
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            effect: None,
            target: None,
            seen_nonces: &seen,
        };
        assert!(validate_and_consume(&cap, &ctx)
            .unwrap_err()
            .to_string()
            .contains("EXPIRED"));
    }

    #[test]
    fn bad_mac_denied() {
        let key = "k";
        let mut cap = base_mint(key);
        cap.mac = "00".repeat(32);
        let seen = Mutex::new(HashSet::new());
        let ctx = ValidateCtx {
            key,
            guest_id: "guest-tart-1",
            program_digest: "prog-eee",
            engagement_id: "eng-1",
            engagement_hash: "eh-aaa",
            effect: None,
            target: None,
            seen_nonces: &seen,
        };
        assert!(validate_and_consume(&cap, &ctx)
            .unwrap_err()
            .to_string()
            .contains("MAC_INVALID"));
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("anubis-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = default_cap_path(&dir);
        let key = "roundtrip-key";
        let cap = base_mint(key);
        write_cap(&path, &cap).unwrap();
        let loaded = read_cap(&path).unwrap();
        assert_eq!(loaded.nonce, cap.nonce);
        assert_eq!(loaded.mac, cap.mac);
        let _ = fs::remove_dir_all(&dir);
    }
}
