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
