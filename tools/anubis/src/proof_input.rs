//! Parameterized RISC0 proof inputs: parse, canonicalize, hash, encode for guest.

use anyhow::{anyhow, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

pub const INPUT_SCHEMA_VERSION: &str = "1";
/// Anubis Binary Proof-input container magic (`ANBP` little-endian).
/// Used by `encode_anbp_blob` for an optional binary sidecar next to JSON v1.
pub const ANBP_MAGIC: u32 = 0x414E_4250;

#[derive(Debug, Clone)]
pub struct ProofInputs {
    /// Sorted map of name → integer (bool as 0/1).
    pub values: BTreeMap<String, i64>,
    pub mode: String,   // none | json | file
    pub source: String, // --input-json | path | empty
    pub canonical_json: String,
    pub sha256: String,
    pub redacted: bool,
}

impl ProofInputs {
    pub fn empty() -> Self {
        let canonical_json = "{}".to_string();
        let sha256 = hex::encode(Sha256::digest(canonical_json.as_bytes()));
        Self {
            values: BTreeMap::new(),
            mode: "none".into(),
            source: String::new(),
            canonical_json,
            sha256,
            redacted: false,
        }
    }

    pub fn from_json_str(raw: &str, mode: &str, source: &str) -> Result<Self> {
        let v: Value = serde_json::from_str(raw).map_err(|e| {
            anyhow!("ANUBIS_PROOF_INPUT_INVALID_JSON: {}", e)
        })?;
        let obj = v.as_object().ok_or_else(|| {
            anyhow!("ANUBIS_PROOF_INPUT_INVALID_JSON: top-level value must be a JSON object")
        })?;
        let mut values = BTreeMap::new();
        for (k, val) in obj {
            let n = match val {
                Value::Number(num) => {
                    if let Some(u) = num.as_u64() {
                        if u > i64::MAX as u64 {
                            return Err(anyhow!(
                                "ANUBIS_PROOF_INPUT_TYPE_MISMATCH: key `{k}` out of i64 range"
                            ));
                        }
                        u as i64
                    } else if let Some(i) = num.as_i64() {
                        i
                    } else {
                        return Err(anyhow!(
                            "ANUBIS_PROOF_INPUT_TYPE_MISMATCH: key `{k}` must be integer"
                        ));
                    }
                }
                Value::Bool(b) => i64::from(*b),
                _ => {
                    return Err(anyhow!(
                        "ANUBIS_PROOF_INPUT_UNSUPPORTED_TYPE: key `{k}` (only u32/i64/bool in v1)"
                    ));
                }
            };
            values.insert(k.clone(), n);
        }
        // Canonical JSON: sorted keys, compact
        let canonical_json = serde_json::to_string(&values).map_err(|e| {
            anyhow!("ANUBIS_PROOF_INPUT_INVALID_JSON: canonicalize: {}", e)
        })?;
        let sha256 = hex::encode(Sha256::digest(canonical_json.as_bytes()));
        Ok(Self {
            values,
            mode: mode.into(),
            source: source.into(),
            canonical_json,
            sha256,
            redacted: false,
        })
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!(
                "ANUBIS_PROOF_INPUT_FILE_MISSING: {}",
                path.display()
            ));
        }
        let raw = std::fs::read_to_string(path).map_err(|e| {
            anyhow!("ANUBIS_PROOF_INPUT_FILE_MISSING: {}: {}", path.display(), e)
        })?;
        Self::from_json_str(&raw, "file", &path.display().to_string())
    }

    pub fn metadata_json(&self) -> serde_json::Value {
        serde_json::json!({
            "input_mode": self.mode,
            "input_source": self.source,
            "input_sha256": self.sha256,
            "input_redacted": self.redacted,
            "input_schema_version": INPUT_SCHEMA_VERSION,
            "input_binary_magic": format!("0x{ANBP_MAGIC:08X}"),
            "input_keys": self.values.keys().cloned().collect::<Vec<_>>(),
            "input_canonical_json": if self.redacted { serde_json::Value::Null } else { serde_json::Value::String(self.canonical_json.clone()) },
        })
    }

    /// Encode inputs as a length-prefixed ANBP blob:
    /// `magic_u32_le | n_u32_le | (key_len_u16_le, key_utf8, value_i64_le)*` (sorted keys).
    pub fn encode_anbp_blob(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&ANBP_MAGIC.to_le_bytes());
        let n = self.values.len() as u32;
        out.extend_from_slice(&n.to_le_bytes());
        for (k, v) in &self.values {
            let kb = k.as_bytes();
            let klen = kb.len() as u16;
            out.extend_from_slice(&klen.to_le_bytes());
            out.extend_from_slice(kb);
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }
}

/// Decode and validate an ANBP blob; returns entry count on success.
pub fn decode_anbp_header(blob: &[u8]) -> Result<u32> {
    if blob.len() < 8 {
        return Err(anyhow!("ANUBIS_PROOF_INPUT_ANBP_SHORT"));
    }
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&blob[0..4]);
    let magic = u32::from_le_bytes(magic);
    if magic != ANBP_MAGIC {
        return Err(anyhow!(
            "ANUBIS_PROOF_INPUT_ANBP_BAD_MAGIC: got 0x{magic:08X}, want 0x{ANBP_MAGIC:08X}"
        ));
    }
    let mut n = [0u8; 4];
    n.copy_from_slice(&blob[4..8]);
    Ok(u32::from_le_bytes(n))
}

pub fn resolve_proof_inputs(
    input_json: Option<&str>,
    input_file: Option<&Path>,
) -> Result<ProofInputs> {
    match (input_json, input_file) {
        (Some(_), Some(_)) => Err(anyhow!(
            "ANUBIS_PROOF_INPUT_INVALID_JSON: pass only one of --input-json or --input-file"
        )),
        (Some(s), None) => ProofInputs::from_json_str(s, "json", "--input-json"),
        (None, Some(p)) => ProofInputs::from_file(p),
        (None, None) => Ok(ProofInputs::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hash_stable() {
        let a = ProofInputs::from_json_str(r#"{"n":5,"flag":true}"#, "json", "t").unwrap();
        let b = ProofInputs::from_json_str(r#"{"flag":true,"n":5}"#, "json", "t").unwrap();
        assert_eq!(a.sha256, b.sha256);
        assert_eq!(a.values.get("n"), Some(&5));
        assert_eq!(a.values.get("flag"), Some(&1));
    }

    #[test]
    fn rejects_nested() {
        let e = ProofInputs::from_json_str(r#"{"a":{"b":1}}"#, "json", "t").unwrap_err();
        assert!(e.to_string().contains("UNSUPPORTED_TYPE"));
    }

    #[test]
    fn anbp_blob_roundtrip_header() {
        let a = ProofInputs::from_json_str(r#"{"n":5}"#, "json", "t").unwrap();
        let blob = a.encode_anbp_blob();
        assert_eq!(decode_anbp_header(&blob).unwrap(), 1);
        assert_eq!(&blob[0..4], &ANBP_MAGIC.to_le_bytes());
        assert!(a.metadata_json()["input_binary_magic"]
            .as_str()
            .unwrap()
            .contains("414E4250"));
    }
}
