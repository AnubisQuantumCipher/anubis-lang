//! Signer trust store: `~/.anubis/trust/signers.toml` + project-level keys.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustStore {
    #[serde(default)]
    pub signer: Vec<TrustedSigner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedSigner {
    pub public_key: String,
    #[serde(default)]
    pub name: String,
}

pub fn default_trust_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".anubis")
        .join("trust")
        .join("signers.toml")
}

impl TrustStore {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str(&text).map_err(|e| format!("ANUBIS_DEP_UNTRUSTED_SIGNER: trust store: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn add(&mut self, public_key: &str, name: &str) {
        let pk = public_key.trim().to_string();
        if self.signer.iter().any(|s| s.public_key == pk) {
            return;
        }
        self.signer.push(TrustedSigner {
            public_key: pk,
            name: name.to_string(),
        });
    }

    pub fn contains(&self, public_key: &str) -> bool {
        let pk = public_key.trim();
        self.signer.iter().any(|s| s.public_key == pk)
    }

    /// Union with extra project-level keys.
    pub fn allows(&self, public_key: &str, project_keys: &[String]) -> bool {
        if self.contains(public_key) {
            return true;
        }
        let pk = public_key.trim();
        project_keys.iter().any(|k| k.trim() == pk)
    }

    pub fn all_keys(&self) -> BTreeSet<String> {
        self.signer.iter().map(|s| s.public_key.clone()).collect()
    }
}
