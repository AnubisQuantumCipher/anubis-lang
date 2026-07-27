//! Malleable C2 HTTP profile — shapes beacon traffic like elite CS/Sliver profiles.
//! Profiles are engagement-scoped JSON; listener can load them for header/URI cosmetics.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalleableProfile {
    pub schema_version: String,
    pub name: String,
    /// URI paths for beacon POST (rotated).
    pub beacon_uris: Vec<String>,
    pub result_uris: Vec<String>,
    pub user_agent: String,
    /// Extra request headers name→value.
    #[serde(default)]
    pub headers: Vec<[String; 2]>,
    /// Server response header cosmetics.
    #[serde(default)]
    pub server_headers: Vec<[String; 2]>,
    /// Sleep metadata advertised (ms) — actual sleep still from engagement.
    #[serde(default)]
    pub sleep_hint_ms: u64,
    /// Transform: none | base64 | prepend_junk (lab-only labels).
    #[serde(default = "default_transform")]
    pub transform: String,
}

fn default_transform() -> String {
    "none".into()
}

impl Default for MalleableProfile {
    fn default() -> Self {
        Self {
            schema_version: "1.0".into(),
            name: "aop-default-jquery".into(),
            beacon_uris: vec!["/jquery-3.6.0.min.js".into(), "/api/v1/telemetry".into()],
            result_uris: vec!["/api/v1/events".into()],
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36".into(),
            headers: vec![
                ["Accept".into(), "*/*".into()],
                ["Accept-Language".into(), "en-US,en;q=0.9".into()],
            ],
            server_headers: vec![
                ["Server".into(), "nginx".into()],
                ["X-Content-Type-Options".into(), "nosniff".into()],
            ],
            sleep_hint_ms: 2000,
            transform: "none".into(),
        }
    }
}

impl MalleableProfile {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(anyhow!("ANUBIS_MALLEABLE_NAME"));
        }
        if self.beacon_uris.is_empty() {
            return Err(anyhow!("ANUBIS_MALLEABLE_NO_BEACON_URI"));
        }
        for u in self.beacon_uris.iter().chain(self.result_uris.iter()) {
            if !u.starts_with('/') {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_MUST_ABS: {u}"));
            }
            // `//host/path` is a PROTOCOL-RELATIVE URL. It starts with `/` so the absolute-path
            // check above passes it, and it contains no `://` so the scheme check below passed it
            // too — a beacon URI that silently resolves to an arbitrary EXTERNAL host, which is the
            // exact thing this validator exists to prevent. Backslash is rejected with it: some
            // clients normalise `\\` to `/`, so `\\evil.com` is the same hole spelled differently.
            if u.starts_with("//") || u.starts_with("/\\") || u.contains('\\') {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_HOSTILE: {u}"));
            }
            if u.contains("://") || u.contains("..") {
                return Err(anyhow!("ANUBIS_MALLEABLE_URI_HOSTILE: {u}"));
            }
        }
        if self.user_agent.len() > 512 {
            return Err(anyhow!("ANUBIS_MALLEABLE_UA_LONG"));
        }
        Ok(())
    }
}

pub fn write_default(engage_dir: &Path, name: &str) -> Result<std::path::PathBuf> {
    let mut p = MalleableProfile::default();
    if !name.is_empty() {
        p.name = name.into();
    }
    p.validate()?;
    let dir = engage_dir.join("profiles");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", sanitize(&p.name)));
    fs::write(&path, serde_json::to_string_pretty(&p)?)?;
    Ok(path)
}

pub fn load(path: &Path) -> Result<MalleableProfile> {
    let raw = fs::read_to_string(path)
        .map_err(|e| anyhow!("ANUBIS_MALLEABLE_LOAD: {}: {e}", path.display()))?;
    let p: MalleableProfile =
        serde_json::from_str(&raw).map_err(|e| anyhow!("ANUBIS_MALLEABLE_PARSE: {e}"))?;
    p.validate()?;
    Ok(p)
}

pub fn validate_file(path: &Path) -> Result<serde_json::Value> {
    let p = load(path)?;
    Ok(json!({
        "ok": true,
        "profile": p,
        "path": path.display().to_string(),
        "attck": ["T1071", "T1090"],
    }))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_validates_ok() {
        let p = MalleableProfile::default();
        p.validate().expect("default profile should validate");
        assert_eq!(p.name, "aop-default-jquery");
        assert!(!p.beacon_uris.is_empty());
        assert!(p.user_agent.starts_with("Mozilla/5.0"));
        assert_eq!(p.transform, "none");
    }

    #[test]
    fn validate_rejects_empty_name() {
        let mut p = MalleableProfile::default();
        p.name = String::new();
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_NAME"), "{err}");
    }

    #[test]
    fn validate_rejects_no_beacon_uris() {
        let mut p = MalleableProfile::default();
        p.beacon_uris.clear();
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_NO_BEACON_URI"), "{err}");
    }

    #[test]
    fn validate_rejects_non_absolute_uri() {
        let mut p = MalleableProfile::default();
        p.beacon_uris = vec!["relative/path".into()];
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_MUST_ABS"), "{err}");
    }

    #[test]
    fn validate_rejects_traversal_uri() {
        let mut p = MalleableProfile::default();
        p.beacon_uris = vec!["/ok/../etc/passwd".into()];
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_URI_HOSTILE"), "{err}");
    }

    #[test]
    fn validate_rejects_scheme_uri() {
        let mut p = MalleableProfile::default();
        p.beacon_uris = vec!["http://evil.com/beacon".into()];
        let err = p.validate().unwrap_err().to_string();
        // Rejected by the absolute-path rule, which fires FIRST — the scheme check below it is
        // unreachable for this input. The security property (a scheme URI is refused) holds; only
        // the diagnostic differs from the one this test originally expected.
        assert!(err.contains("ANUBIS_MALLEABLE_URI_MUST_ABS"), "{err}");
    }

    #[test]
    fn validate_rejects_protocol_relative_uri() {
        // `//host/path` passed BOTH checks before the fix: it starts with `/` so it is "absolute",
        // and has no `://` so it is not "hostile" — while resolving to an arbitrary external host.
        for u in ["//evil.com/beacon", "/\\evil.com/beacon", "/a\\b"] {
            let mut p = MalleableProfile::default();
            p.beacon_uris = vec![u.into()];
            let err = p.validate().unwrap_err().to_string();
            assert!(err.contains("ANUBIS_MALLEABLE_URI_HOSTILE"), "{u}: {err}");
        }
    }

    #[test]
    fn validate_rejects_long_user_agent() {
        let mut p = MalleableProfile::default();
        p.user_agent = "A".repeat(513);
        let err = p.validate().unwrap_err().to_string();
        assert!(err.contains("ANUBIS_MALLEABLE_UA_LONG"), "{err}");
    }

    #[test]
    fn sanitize_replaces_special_chars() {
        assert_eq!(sanitize("hello world!@#"), "hello_world___");
        assert_eq!(sanitize("abc-def_123"), "abc-def_123");
    }
}
