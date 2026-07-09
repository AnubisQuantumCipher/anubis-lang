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
    Ok(serde_json::json!({
        "module": "xor_pack",
        "input": input,
        "packed": packed_path,
        "key_hex": hex::encode(key),
        "input_sha256": hex::encode(Sha256::digest(&data)),
        "packed_sha256": hex::encode(Sha256::digest(&packed)),
        "note": "Lab packer only — not a production crypter",
    }))
}

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
