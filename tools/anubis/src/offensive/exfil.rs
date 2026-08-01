//! Exfiltration module — T10 (TA0010).
//!
//! Data exfiltration technique planning and detection gap testing.
//! Live operations: DNS-based data encoding, HTTP staging.
//! PLAN_ONLY: covert channels, steganography, cloud storage abuse.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// DNS exfiltration encoding — encode data as DNS-safe subdomain labels.
///
/// Does NOT send DNS queries. Encodes data to demonstrate the technique
/// and test DNS monitoring/DLP controls. Maps to T1048.003.
pub fn dns_encode(eng: &Engagement, data: &[u8], domain: &str) -> Result<Value> {
    eng.validate_live()?;
    if data.is_empty() {
        return Err(anyhow!("ANUBIS_EXFIL_EMPTY: no data to encode"));
    }

    let encoded = hex::encode(data);
    let chunk_size = 63; // max DNS label length
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(chunk_size)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();

    let queries: Vec<String> = chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| format!("{i}.{chunk}.{domain}"))
        .collect();

    Ok(json!({
        "schema": "aop-exfil-v1",
        "module": "dns_encode",
        "engagement_id": eng.engagement_id,
        "original_size": data.len(),
        "encoded_size": encoded.len(),
        "chunks": chunks.len(),
        "sample_queries": &queries[..queries.len().min(5)],
        "domain": domain,
        "note": "Encoding only — no DNS queries sent. Test DLP/DNS monitoring.",
        "attck": ["T1048.003"],
        "executed": true,
    }))
}

/// HTTP exfiltration staging — prepare files for HTTP-based exfil testing.
///
/// Creates a manifest of files to test against DLP/proxy controls.
/// Does NOT transmit data. Maps to T1048.002.
pub fn http_stage(eng: &Engagement, source_dir: &Path, max_files: usize) -> Result<Value> {
    eng.validate_live()?;
    if !source_dir.is_dir() {
        return Err(anyhow!(
            "ANUBIS_EXFIL_SOURCE_MISSING: {} not found",
            source_dir.display()
        ));
    }

    let mut manifest: Vec<Value> = Vec::new();
    let mut total_size = 0u64;
    let entries: Vec<_> = fs::read_dir(source_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .take(max_files)
        .collect();

    for entry in &entries {
        let path = entry.path();
        let meta = fs::metadata(&path)?;
        let content = fs::read(&path)?;
        let hash = hex::encode(Sha256::digest(&content));
        total_size += meta.len();

        manifest.push(json!({
            "file": path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            "size": meta.len(),
            "sha256": hash,
        }));
    }

    Ok(json!({
        "schema": "aop-exfil-v1",
        "module": "http_stage",
        "engagement_id": eng.engagement_id,
        "source_dir": source_dir.display().to_string(),
        "files": manifest.len(),
        "total_bytes": total_size,
        "manifest": manifest,
        "note": "Staging manifest only — no data transmitted. Test DLP/proxy controls.",
        "attck": ["T1048.002"],
        "executed": true,
    }))
}

/// Steganography exfiltration planning (PLAN_ONLY).
pub fn stego_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-exfil-v1",
        "module": "stego_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1027.003"],
        "techniques": [
            {
                "name": "LSB image steganography",
                "method": "Embed data in least-significant bits of PNG/BMP pixels",
                "tools": ["steghide", "openstego", "zsteg"],
                "detection": "Chi-square analysis, RS analysis, structural steganalysis",
            },
            {
                "name": "HTTPS covert channel",
                "method": "Encode data in TLS session parameters or HTTP headers",
                "tools": ["dnscat2", "iodine (DNS tunnel)", "ptunnel (ICMP)"],
                "detection": "Anomalous DNS query patterns, unusual ICMP payload sizes",
            },
            {
                "name": "Cloud storage abuse",
                "method": "Upload to legitimate cloud services (GDrive, S3, Dropbox)",
                "tools": ["rclone", "aws cli", "gsutil"],
                "detection": "CASB, DLP on egress, unusual upload volumes",
            },
        ],
        "detection_question": "Does DLP inspect image payloads and encrypted tunnel patterns?",
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Protocol tunneling plan (PLAN_ONLY).
pub fn tunnel_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-exfil-v1",
        "module": "tunnel_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1572"],
        "techniques": [
            {
                "name": "DNS tunneling",
                "method": "Encode TCP payload in DNS TXT/CNAME queries via iodine/dnscat2",
                "bandwidth": "~50 Kbps typical",
                "detection": "High query volume, unusual TXT record sizes, entropy analysis",
            },
            {
                "name": "ICMP tunneling",
                "method": "Embed data in ICMP echo request/reply payloads via ptunnel",
                "bandwidth": "~100 Kbps typical",
                "detection": "Large ICMP payloads, high ICMP frequency, payload entropy",
            },
            {
                "name": "SSH over HTTPS",
                "method": "Tunnel SSH through HTTPS CONNECT proxy via corkscrew/proxytunnel",
                "bandwidth": "Near line-speed",
                "detection": "Long-lived CONNECT sessions, unusual destination ports",
            },
            {
                "name": "WebSocket tunneling",
                "method": "Encode arbitrary protocol in WebSocket frames",
                "bandwidth": "Near line-speed",
                "detection": "Frame size/frequency analysis, protocol fingerprinting",
            },
        ],
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Full exfiltration assessment — combines encoding + staging + plans.
pub fn exfil_assessment(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let dns = dns_encode(eng, b"EXFIL-TEST-PAYLOAD", "test.lab.local")?;
    let stego = stego_plan(eng)?;
    let tunnel = tunnel_plan(eng)?;

    Ok(json!({
        "schema": "aop-exfil-v1",
        "module": "exfil_assessment",
        "engagement_id": eng.engagement_id,
        "dns_encoding": dns,
        "steganography": stego,
        "tunneling": tunnel,
        "attck": ["T1048.003", "T1048.002", "T1027.003", "T1572"],
        "executed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn dns_encode_produces_chunks() {
        let eng = Engagement::default_lab("exfil-test", "lab-auth");
        let result = dns_encode(&eng, b"hello world", "test.lab.local").unwrap();
        assert_eq!(result["module"], "dns_encode");
        assert!(result["chunks"].as_u64().unwrap() > 0);
        assert_eq!(result["executed"], true);
    }

    #[test]
    fn dns_encode_rejects_empty() {
        let eng = Engagement::default_lab("exfil-test", "lab-auth");
        let err = dns_encode(&eng, &[], "test.lab.local")
            .unwrap_err()
            .to_string();
        assert!(err.contains("ANUBIS_EXFIL_EMPTY"), "{err}");
    }

    #[test]
    fn http_stage_rejects_missing_dir() {
        let eng = Engagement::default_lab("exfil-test", "lab-auth");
        let err = http_stage(&eng, Path::new("/nonexistent/dir"), 10)
            .unwrap_err()
            .to_string();
        assert!(err.contains("ANUBIS_EXFIL_SOURCE_MISSING"), "{err}");
    }

    #[test]
    fn stego_plan_is_plan_only() {
        let eng = Engagement::default_lab("exfil-test", "lab-auth");
        let result = stego_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }

    #[test]
    fn tunnel_plan_is_plan_only() {
        let eng = Engagement::default_lab("exfil-test", "lab-auth");
        let result = tunnel_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
    }
}
