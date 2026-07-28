//! Lab evasion helpers: XOR packer, sleep-jitter note, string scramble.

use anyhow::Result;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub fn xor_pack(input: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return input.to_vec();
    }
    input
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect()
}

pub fn pack_file(input: &Path, out_dir: &Path) -> Result<serde_json::Value> {
    fs::create_dir_all(out_dir)?;
    let data = fs::read(input)?;
    let mut key = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut key);
    let packed = xor_pack(&data, &key);
    let packed_path = out_dir.join(format!(
        "{}.xor.pack",
        input.file_name().and_then(|s| s.to_str()).unwrap_or("blob")
    ));
    fs::write(&packed_path, &packed)?;
    let stub = format!(
        r#"// Anubis lab XOR unpack stub (C)
// key = {key}
#include <stddef.h>
static unsigned char KEY[] = {{ {key_c} }};
static void unpack(unsigned char *buf, size_t n) {{
  for (size_t i = 0; i < n; i++) buf[i] ^= KEY[i % sizeof(KEY)];
}}
"#,
        key = hex::encode(key),
        key_c = key
            .iter()
            .map(|b| format!("0x{b:02x}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    fs::write(out_dir.join("unpack_stub.c"), stub)?;
    let name = input.file_name().and_then(|s| s.to_str()).unwrap_or("blob");
    // Lab string scramble of the basename — used by agent string tables / unpack notes.
    let name_scramble = scramble_string(name);
    Ok(serde_json::json!({
        "module": "xor_pack",
        "input": input,
        "packed": packed_path,
        "key_hex": hex::encode(key),
        "input_sha256": hex::encode(Sha256::digest(&data)),
        "packed_sha256": hex::encode(Sha256::digest(&packed)),
        "name_scramble": name_scramble,
        "note": "Lab packer only — not a production crypter",
    }))
}

/// Lab string XOR scramble (obfuscation helper for notes/stubs — not crypto).
///
/// Returns a JSON object with key_hex, encoded_hex, and original_len.
/// Decode: XOR encoded bytes with key bytes (cyclic).
pub fn scramble_string(s: &str) -> serde_json::Value {
    let mut key = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut key);
    let enc: Vec<u8> = s
        .bytes()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect();
    serde_json::json!({
        "module": "string_scramble",
        "key_hex": hex::encode(key),
        "encoded_hex": hex::encode(enc),
        "original_len": s.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_pack_empty_key_is_identity() {
        let data = b"AAAA sensitive payload";
        let packed = xor_pack(data, &[]);
        assert_eq!(&packed, data, "empty key must return input unchanged");
    }

    #[test]
    fn xor_pack_is_its_own_inverse() {
        let data = b"secret material that must survive roundtrip";
        let key = b"lab-key-16bytes!";
        let packed = xor_pack(data, key);
        let unpacked = xor_pack(&packed, key);
        assert_eq!(&unpacked, &data[..], "XOR roundtrip must recover original");
    }

    #[test]
    fn xor_pack_output_differs_from_input() {
        let data = b"this is not noise";
        let key = b"\xff";
        let packed = xor_pack(data, key);
        assert_ne!(&packed, &data[..], "non-trivial key must change output");
    }

    #[test]
    fn xor_pack_single_byte_key_cycles() {
        let data = vec![0x41; 8]; // "AAAAAAAA"
        let key = [0x20u8]; // XOR 0x41 ^ 0x20 = 0x61 = 'a'
        let packed = xor_pack(&data, &key);
        assert!(
            packed.iter().all(|&b| b == 0x61),
            "single-byte key must cycle: {:?}",
            packed
        );
    }

    #[test]
    fn scramble_string_roundtrip() {
        let original = "sensitive_binary_name.exe";
        let result = scramble_string(original);
        let key_hex = result["key_hex"].as_str().unwrap();
        let enc_hex = result["encoded_hex"].as_str().unwrap();
        let key = hex::decode(key_hex).unwrap();
        let enc = hex::decode(enc_hex).unwrap();
        assert_eq!(
            result["original_len"].as_u64().unwrap(),
            original.len() as u64
        );
        let decoded: Vec<u8> = enc
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ key[i % key.len()])
            .collect();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            original,
            "scramble must be reversible with the key"
        );
    }

    #[test]
    fn pack_file_produces_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "anubis-packer-test-{}-packfile",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("payload.bin");
        std::fs::write(&input, b"lab payload content for testing").unwrap();
        let out = dir.join("packed");
        let result = pack_file(&input, &out).unwrap();
        assert!(
            out.join("payload.bin.xor.pack").exists(),
            "packed file missing"
        );
        assert!(out.join("unpack_stub.c").exists(), "stub file missing");
        assert_eq!(result["module"].as_str().unwrap(), "xor_pack");
        assert!(
            !result["key_hex"].as_str().unwrap().is_empty(),
            "key must be non-empty"
        );
        let packed = std::fs::read(out.join("payload.bin.xor.pack")).unwrap();
        let original = b"lab payload content for testing";
        assert_ne!(
            &packed[..],
            &original[..],
            "packed must differ from original"
        );
        let key = hex::decode(result["key_hex"].as_str().unwrap()).unwrap();
        let unpacked = xor_pack(&packed, &key);
        assert_eq!(
            &unpacked,
            &original[..],
            "unpack with key must recover original"
        );
    }

    #[test]
    fn pack_file_key_appears_in_stub() {
        let dir =
            std::env::temp_dir().join(format!("anubis-packer-test-{}-stubkey", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("test.bin");
        std::fs::write(&input, b"x").unwrap();
        let out = dir.join("packed");
        let result = pack_file(&input, &out).unwrap();
        let stub = std::fs::read_to_string(out.join("unpack_stub.c")).unwrap();
        let key_hex = result["key_hex"].as_str().unwrap();
        assert!(
            stub.contains(key_hex),
            "unpack stub must contain the key hex for operator use"
        );
    }
}
