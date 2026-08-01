//! Credential access module — T10 (TA0006).
//!
//! Live operations execute inside VZ guests only. Host-side: planning, hash
//! validation, SSH key audit. Guest-side: spray testing, keychain enum, token
//! extraction.
//!
//! Policy: no credential dumping automation against production systems.
//! `not_claimed` techniques (T1003 OS Credential Dumping) remain not_claimed.
//! What IS here: password-list testing against known hashes, SSH key permission
//! audit, keychain metadata enumeration, and credential spray PLANNING.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Validate a list of candidate passwords against a known SHA-256 hash.
///
/// This is the "offline hash attack" pattern: the operator has a hash from
/// engagement evidence and tests candidates locally. No network, no target
/// system access. Runs on host or guest.
pub fn hash_test(eng: &Engagement, target_hash: &str, wordlist_path: &Path) -> Result<Value> {
    eng.validate_live()?;
    if !wordlist_path.is_file() {
        return Err(anyhow!(
            "ANUBIS_CRED_WORDLIST_MISSING: {} not found",
            wordlist_path.display()
        ));
    }
    let target = target_hash.trim().to_lowercase();
    if target.len() != 64 || !target.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "ANUBIS_CRED_HASH_FORMAT: expected 64-char hex SHA-256 hash, got {} chars",
            target.len()
        ));
    }
    let raw = fs::read_to_string(wordlist_path)?;
    let mut tested = 0u64;
    let mut found: Option<String> = None;
    for line in raw.lines() {
        let candidate = line.trim();
        if candidate.is_empty() || candidate.starts_with('#') {
            continue;
        }
        tested += 1;
        let h = hex::encode(Sha256::digest(candidate.as_bytes()));
        if h == target {
            found = Some(candidate.to_string());
            break;
        }
    }
    Ok(json!({
        "schema": "aop-credential-v1",
        "module": "hash_test",
        "engagement_id": eng.engagement_id,
        "target_hash": target,
        "wordlist": wordlist_path.display().to_string(),
        "candidates_tested": tested,
        "cracked": found.is_some(),
        "plaintext": found,
        "attck": ["T1110.002"],
        "executed": true,
        "note": "Offline hash test — no network, no target system access",
    }))
}

/// Audit SSH key files for permission and configuration issues.
///
/// Checks `~/.ssh/` for: world-readable private keys, missing passphrase
/// indicators, authorized_keys anomalies. Host-side operator tool.
pub fn ssh_key_audit(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let ssh_dir = Path::new(&home).join(".ssh");
    let mut findings: Vec<Value> = Vec::new();
    let mut keys_found: Vec<Value> = Vec::new();

    if !ssh_dir.is_dir() {
        return Ok(json!({
            "schema": "aop-credential-v1",
            "module": "ssh_key_audit",
            "engagement_id": eng.engagement_id,
            "ssh_dir": ssh_dir.display().to_string(),
            "exists": false,
            "findings": [],
            "keys": [],
            "attck": ["T1552.004"],
            "executed": true,
        }));
    }

    let key_names = [
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "id_dsa",
        "id_rsa_anubis",
        "tart_anubis",
    ];

    for name in &key_names {
        let key_path = ssh_dir.join(name);
        if !key_path.is_file() {
            continue;
        }
        let meta = fs::metadata(&key_path)?;
        let perms = format!("{:o}", meta.permissions().mode());
        let size = meta.len();

        let content = fs::read_to_string(&key_path).unwrap_or_default();
        let encrypted = content.contains("ENCRYPTED");
        let key_type = if content.contains("RSA") {
            "rsa"
        } else if content.contains("ED25519") || content.contains("ed25519") {
            "ed25519"
        } else if content.contains("ECDSA") {
            "ecdsa"
        } else {
            "unknown"
        };

        if !perms.ends_with("600") && !perms.ends_with("400") {
            findings.push(json!({
                "severity": "high",
                "code": "WEAK_KEY_PERMS",
                "file": key_path.display().to_string(),
                "permissions": perms,
                "message": "Private key has permissions wider than 0600",
            }));
        }

        if !encrypted {
            findings.push(json!({
                "severity": "medium",
                "code": "UNENCRYPTED_KEY",
                "file": key_path.display().to_string(),
                "message": "Private key is not passphrase-protected",
            }));
        }

        keys_found.push(json!({
            "name": name,
            "path": key_path.display().to_string(),
            "type": key_type,
            "encrypted": encrypted,
            "permissions": perms,
            "size_bytes": size,
        }));
    }

    let auth_keys = ssh_dir.join("authorized_keys");
    if auth_keys.is_file() {
        let content = fs::read_to_string(&auth_keys).unwrap_or_default();
        let count = content
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .count();
        if count > 10 {
            findings.push(json!({
                "severity": "info",
                "code": "MANY_AUTHORIZED_KEYS",
                "count": count,
                "message": "Large authorized_keys file — review for stale entries",
            }));
        }
    }

    Ok(json!({
        "schema": "aop-credential-v1",
        "module": "ssh_key_audit",
        "engagement_id": eng.engagement_id,
        "ssh_dir": ssh_dir.display().to_string(),
        "exists": true,
        "keys": keys_found,
        "findings": findings,
        "attck": ["T1552.004"],
        "executed": true,
    }))
}

/// Credential spray plan — NEVER executes against real targets.
///
/// Emits a structured plan for operator review. The plan documents which
/// accounts, protocols, and lockout thresholds to consider. Execution is
/// manual or via VZ guest with explicit operator authorization.
pub fn spray_plan(
    eng: &Engagement,
    protocol: &str,
    targets: &[String],
    users: &[String],
    lockout_threshold: u32,
) -> Result<Value> {
    eng.validate_live()?;
    for t in targets {
        eng.assert_host(t)?;
    }
    let supported = ["ssh", "http_basic", "smb", "ldap", "kerberos"];
    if !supported.contains(&protocol) {
        return Err(anyhow!(
            "ANUBIS_CRED_SPRAY_PROTOCOL: unsupported `{protocol}` — expected one of {supported:?}"
        ));
    }

    let safe_attempts = if lockout_threshold == 0 {
        5
    } else {
        (lockout_threshold as f64 * 0.5).floor() as u32
    };

    Ok(json!({
        "schema": "aop-credential-v1",
        "module": "spray_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "protocol": protocol,
        "targets": targets,
        "users": users,
        "lockout_threshold": lockout_threshold,
        "safe_attempts_per_user": safe_attempts,
        "attck": ["T1110.003"],
        "steps": [
            "Verify all targets are in engagement scope (done)",
            format!("Use {protocol} with max {safe_attempts} attempts per user per lockout window"),
            "Wait lockout_window_minutes between rounds",
            "Log every attempt to engagement receipts",
            "Stop immediately on first valid credential",
            "Run inside VZ guest — never spray from host",
        ],
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
            "requires_operator_authorization": true,
        },
    }))
}

/// Enumerate environment variables for credential material (tokens, API keys).
///
/// Scans the current process environment for common credential patterns.
/// Runs on guest to inventory what the implant can see.
pub fn env_credential_scan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let patterns = [
        "TOKEN",
        "API_KEY",
        "SECRET",
        "PASSWORD",
        "CREDENTIAL",
        "AWS_ACCESS",
        "AWS_SECRET",
        "GITHUB_TOKEN",
        "GITLAB_TOKEN",
        "SLACK_TOKEN",
        "DISCORD_TOKEN",
        "DATABASE_URL",
        "MONGO_URI",
        "REDIS_URL",
        "PSK",
        "PRIVATE_KEY",
        "AUTH",
    ];
    let mut hits: Vec<Value> = Vec::new();
    let mut scanned = 0u32;

    for (key, _value) in std::env::vars() {
        scanned += 1;
        let upper = key.to_uppercase();
        for pat in &patterns {
            if upper.contains(pat) {
                hits.push(json!({
                    "variable": key,
                    "pattern_matched": pat,
                    "value_redacted": true,
                    "value_length": _value.len(),
                }));
                break;
            }
        }
    }

    Ok(json!({
        "schema": "aop-credential-v1",
        "module": "env_credential_scan",
        "engagement_id": eng.engagement_id,
        "variables_scanned": scanned,
        "credential_hits": hits.len(),
        "hits": hits,
        "attck": ["T1552.001"],
        "executed": true,
        "note": "Values are redacted — only variable names and lengths reported",
    }))
}

/// Keychain metadata enumeration plan (macOS).
///
/// PLAN_ONLY: documents the approach for keychain enumeration using
/// `security` CLI. Does not execute — requires VZ guest + operator auth.
pub fn keychain_enum_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-credential-v1",
        "module": "keychain_enum_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "platform": "macos",
        "attck": ["T1555.001"],
        "steps": [
            "Run inside VZ guest: security dump-keychain -d login.keychain-db",
            "Parse Internet and Generic password entries",
            "Extract service names, accounts, and creation dates (NOT passwords)",
            "Identify high-value targets: VPN, email, cloud services",
            "Document findings for purple-team debrief",
        ],
        "detection_question": "Does EDR alert on security CLI keychain access?",
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
            "no_password_extraction_without_dual_auth": true,
        },
    }))
}

/// Generate a credential testing report from engagement evidence.
pub fn credential_report(eng: &Engagement, engage_dir: &Path) -> Result<Value> {
    eng.validate_live()?;
    let mut sections: BTreeMap<String, Value> = BTreeMap::new();

    let cred_dir = engage_dir.join("evidence/credentials");
    if cred_dir.is_dir() {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(&cred_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                    files.push(entry.file_name().to_string_lossy().to_string());
                }
            }
        }
        sections.insert("evidence_files".into(), json!(files));
    }

    Ok(json!({
        "schema": "aop-credential-v1",
        "module": "credential_report",
        "engagement_id": eng.engagement_id,
        "sections": sections,
        "attck_coverage": ["T1110.002", "T1110.003", "T1552.001", "T1552.004", "T1555.001"],
        "recommendation": "Review credential findings in purple-team debrief with detection team",
    }))
}

use std::os::unix::fs::PermissionsExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn hash_test_cracks_known_sha256() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let tmp = std::env::temp_dir().join(format!("anubis-cred-hash-{}", std::process::id()));
        fs::write(&tmp, "wrong\npassword123\nanother\n").unwrap();
        let target = hex::encode(Sha256::digest(b"password123"));
        let result = hash_test(&eng, &target, &tmp).unwrap();
        assert_eq!(result["cracked"], true);
        assert_eq!(result["plaintext"], "password123");
        assert!(result["candidates_tested"].as_u64().unwrap() >= 2);
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn hash_test_reports_not_cracked() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let tmp = std::env::temp_dir().join(format!("anubis-cred-nocrack-{}", std::process::id()));
        fs::write(&tmp, "aaa\nbbb\nccc\n").unwrap();
        let target = hex::encode(Sha256::digest(b"not-in-list"));
        let result = hash_test(&eng, &target, &tmp).unwrap();
        assert_eq!(result["cracked"], false);
        assert!(result["plaintext"].is_null());
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn hash_test_rejects_bad_hash_format() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let tmp = std::env::temp_dir().join(format!("anubis-cred-badhash-{}", std::process::id()));
        fs::write(&tmp, "x\n").unwrap();
        let err = hash_test(&eng, "tooshort", &tmp).unwrap_err().to_string();
        assert!(err.contains("ANUBIS_CRED_HASH_FORMAT"), "{err}");
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn spray_plan_is_plan_only_and_scope_gated() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let err = spray_plan(&eng, "ssh", &["10.99.99.99".into()], &["admin".into()], 5)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("SCOPE") || err.contains("DENIED"),
            "out-of-scope target must be denied: {err}"
        );
    }

    #[test]
    fn spray_plan_rejects_unsupported_protocol() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let err = spray_plan(&eng, "telnet", &["127.0.0.1".into()], &["admin".into()], 5)
            .unwrap_err()
            .to_string();
        assert!(err.contains("ANUBIS_CRED_SPRAY_PROTOCOL"), "{err}");
    }

    #[test]
    fn spray_plan_computes_safe_attempts() {
        let mut eng = Engagement::default_lab("cred-test", "lab-auth");
        eng.allowed_hosts.push("10.0.0.1".into());
        let result = spray_plan(&eng, "ssh", &["10.0.0.1".into()], &["admin".into()], 10).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["executed"], false);
        assert_eq!(result["safe_attempts_per_user"], 5);
    }

    #[test]
    fn env_credential_scan_runs_and_redacts() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        std::env::set_var("ANUBIS_TEST_API_KEY", "secret-value-12345");
        let result = env_credential_scan(&eng).unwrap();
        assert_eq!(result["executed"], true);
        assert!(result["variables_scanned"].as_u64().unwrap() > 0);
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(
            !serialized.contains("secret-value-12345"),
            "value must be redacted"
        );
        std::env::remove_var("ANUBIS_TEST_API_KEY");
    }

    #[test]
    fn keychain_enum_plan_is_plan_only() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let result = keychain_enum_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["executed"], false);
        assert_eq!(result["platform"], "macos");
    }

    #[test]
    fn ssh_key_audit_runs_without_panic() {
        let eng = Engagement::default_lab("cred-test", "lab-auth");
        let result = ssh_key_audit(&eng);
        assert!(
            result.is_ok(),
            "ssh_key_audit should not panic: {:?}",
            result.err()
        );
    }
}
