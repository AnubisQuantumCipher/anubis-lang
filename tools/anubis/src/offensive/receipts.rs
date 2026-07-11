//! Tamper-evident engagement action receipts.
//!
//! Every operator/platform action can append a hash-chained receipt under
//! `evidence/receipts/chain.jsonl` with a tip at `evidence/receipts/tip.json`.
//! Recompute the chain to detect silent rewrites.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RECEIPT_SCHEMA: &str = "1";
pub const GENESIS_PREV: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReceipt {
    pub schema_version: String,
    pub seq: u64,
    pub ts_unix: u64,
    pub engagement_id: String,
    pub action: String,
    pub operator: String,
    pub payload: serde_json::Value,
    pub payload_sha256: String,
    pub prev_hash: String,
    pub receipt_hash: String,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn receipts_dir(engage_dir: &Path) -> PathBuf {
    engage_dir.join("evidence/receipts")
}

fn chain_path(engage_dir: &Path) -> PathBuf {
    receipts_dir(engage_dir).join("chain.jsonl")
}

fn tip_path(engage_dir: &Path) -> PathBuf {
    receipts_dir(engage_dir).join("tip.json")
}

fn canonical_payload(payload: &serde_json::Value) -> String {
    // Stable enough for receipts: serde compact. Prefer object key order as produced.
    serde_json::to_string(payload).unwrap_or_else(|_| "null".into())
}

fn payload_hash(payload: &serde_json::Value) -> String {
    hex::encode(Sha256::digest(canonical_payload(payload).as_bytes()))
}

/// Hash binding for one receipt (does not include receipt_hash itself).
pub fn compute_receipt_hash(
    seq: u64,
    ts_unix: u64,
    engagement_id: &str,
    action: &str,
    operator: &str,
    payload_sha256: &str,
    prev_hash: &str,
) -> String {
    let material = format!(
        "anubis-receipt-v1|{seq}|{ts_unix}|{engagement_id}|{action}|{operator}|{payload_sha256}|{prev_hash}"
    );
    hex::encode(Sha256::digest(material.as_bytes()))
}

fn load_tip(engage_dir: &Path) -> (u64, String) {
    let p = tip_path(engage_dir);
    if let Ok(raw) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let seq = v.get("seq").and_then(|x| x.as_u64()).unwrap_or(0);
            let h = v
                .get("receipt_hash")
                .and_then(|x| x.as_str())
                .unwrap_or(GENESIS_PREV)
                .to_string();
            return (seq, h);
        }
    }
    (0, GENESIS_PREV.to_string())
}

/// Append a hash-chained action receipt. Returns the sealed receipt.
pub fn seal_action(
    engage_dir: &Path,
    engagement_id: &str,
    action: &str,
    operator: &str,
    payload: serde_json::Value,
) -> Result<ActionReceipt> {
    fs::create_dir_all(receipts_dir(engage_dir))?;
    let (last_seq, prev_hash) = load_tip(engage_dir);
    let seq = last_seq + 1;
    let ts_unix = now_unix();
    let payload_sha256 = payload_hash(&payload);
    let receipt_hash = compute_receipt_hash(
        seq,
        ts_unix,
        engagement_id,
        action,
        operator,
        &payload_sha256,
        &prev_hash,
    );
    let receipt = ActionReceipt {
        schema_version: RECEIPT_SCHEMA.into(),
        seq,
        ts_unix,
        engagement_id: engagement_id.into(),
        action: action.into(),
        operator: operator.into(),
        payload,
        payload_sha256,
        prev_hash,
        receipt_hash: receipt_hash.clone(),
    };
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(chain_path(engage_dir))?;
    writeln!(f, "{}", serde_json::to_string(&receipt)?)?;
    let tip = json!({
        "seq": seq,
        "receipt_hash": receipt_hash,
        "action": action,
        "ts_unix": ts_unix,
    });
    fs::write(tip_path(engage_dir), serde_json::to_string_pretty(&tip)?)?;
    Ok(receipt)
}

/// Recompute the chain; fail closed if any link is broken.
pub fn verify_chain(engage_dir: &Path) -> Result<serde_json::Value> {
    let path = chain_path(engage_dir);
    if !path.exists() {
        return Ok(json!({
            "ok": true,
            "empty": true,
            "count": 0,
            "tip": GENESIS_PREV,
        }));
    }
    let file = fs::File::open(&path)?;
    let reader = std::io::BufReader::new(file);
    let mut prev = GENESIS_PREV.to_string();
    let mut count = 0u64;
    let mut last_hash = GENESIS_PREV.to_string();
    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let r: ActionReceipt = serde_json::from_str(&line)
            .map_err(|e| anyhow!("ANUBIS_RECEIPT_PARSE: line {}: {e}", line_no + 1))?;
        count += 1;
        if r.seq != count {
            return Err(anyhow!(
                "ANUBIS_RECEIPT_SEQ_GAP: expected seq {count}, got {}",
                r.seq
            ));
        }
        if r.prev_hash != prev {
            return Err(anyhow!(
                "ANUBIS_RECEIPT_CHAIN_BREAK: seq {} prev_hash mismatch",
                r.seq
            ));
        }
        let ph = payload_hash(&r.payload);
        if ph != r.payload_sha256 {
            return Err(anyhow!("ANUBIS_RECEIPT_PAYLOAD_TAMPER: seq {}", r.seq));
        }
        let expect = compute_receipt_hash(
            r.seq,
            r.ts_unix,
            &r.engagement_id,
            &r.action,
            &r.operator,
            &r.payload_sha256,
            &r.prev_hash,
        );
        if expect != r.receipt_hash {
            return Err(anyhow!("ANUBIS_RECEIPT_HASH_MISMATCH: seq {}", r.seq));
        }
        prev = r.receipt_hash.clone();
        last_hash = r.receipt_hash;
    }
    // Tip must match last
    let (tip_seq, tip_hash) = load_tip(engage_dir);
    if count > 0 && (tip_seq != count || tip_hash != last_hash) {
        return Err(anyhow!(
            "ANUBIS_RECEIPT_TIP_MISMATCH: tip_seq={tip_seq} count={count}"
        ));
    }
    Ok(json!({
        "ok": true,
        "empty": count == 0,
        "count": count,
        "tip": last_hash,
        "tip_seq": count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_eng(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "anubis-receipt-test-{}-{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn chain_seals_and_verifies() {
        let eng = tmp_eng("seal");
        seal_action(&eng, "e1", "test_a", "op", json!({"x": 1})).unwrap();
        seal_action(&eng, "e1", "test_b", "op", json!({"y": 2})).unwrap();
        let v = verify_chain(&eng).unwrap();
        assert_eq!(v["count"], 2);
        assert_eq!(v["ok"], true);
        let _ = fs::remove_dir_all(&eng);
    }

    #[test]
    fn tamper_detected() {
        let eng = tmp_eng("tamper");
        seal_action(&eng, "e1", "test_a", "op", json!({"x": 1})).unwrap();
        let chain = chain_path(&eng);
        let mut lines: Vec<String> = fs::read_to_string(&chain)
            .unwrap()
            .lines()
            .map(|s| s.to_string())
            .collect();
        let mut r: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        r["payload"] = json!({"x": 999});
        lines[0] = serde_json::to_string(&r).unwrap();
        fs::write(&chain, lines.join("\n") + "\n").unwrap();
        assert!(verify_chain(&eng).is_err());
        let _ = fs::remove_dir_all(&eng);
    }
}
