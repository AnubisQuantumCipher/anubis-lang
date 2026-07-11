//! ROP / gadget helpers and browser-harness scaffold (operator-side).

use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;

/// Classic de Bruijn cyclic pattern (same alphabet as PoC kit).
pub fn pattern_create(n: usize) -> String {
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    (0..n)
        .map(|i| alphabet[i % alphabet.len()] as char)
        .collect()
}

/// Find offset of a 4-byte little-endian value or ASCII subsequence in cyclic pattern.
pub fn pattern_offset(pattern_len: usize, needle: &str) -> Result<serde_json::Value> {
    let pat = pattern_create(pattern_len.max(needle.len() + 8));
    // needle: prefer explicit hex ("0x...." or even-length hex-only), else raw ASCII bytes.
    // Note: bare "abcd" is valid hex but must be treated as ASCII for pattern work.
    let raw = needle.trim_start_matches("0x");
    let bytes = if needle.starts_with("0x") || needle.starts_with("0X") {
        hex::decode(raw).unwrap_or_else(|_| needle.as_bytes().to_vec())
    } else if raw.len() >= 8
        && raw.len().is_multiple_of(2)
        && raw.chars().all(|c| c.is_ascii_hexdigit())
        && raw.chars().any(|c| c.is_ascii_digit())
    {
        // long hex-looking (e.g. 61616161) — decode
        hex::decode(raw).unwrap_or_else(|_| needle.as_bytes().to_vec())
    } else {
        needle.as_bytes().to_vec()
    };
    let hay = pat.as_bytes();
    if let Some(pos) = hay.windows(bytes.len()).position(|w| w == bytes.as_slice()) {
        return Ok(serde_json::json!({
            "found": true,
            "offset": pos,
            "needle": needle,
            "pattern_len": pattern_len,
        }));
    }
    Ok(serde_json::json!({
        "found": false,
        "offset": null,
        "needle": needle,
        "pattern_len": pattern_len,
    }))
}

/// Load gadgets from a text file (one "addr module+off ; instr" per line) and filter.
pub fn gadget_search(gadget_file: &Path, contains: &str) -> Result<serde_json::Value> {
    if !gadget_file.exists() {
        return Err(anyhow!(
            "ANUBIS_ROP_GADGET_FILE_MISSING: {}",
            gadget_file.display()
        ));
    }
    let text = fs::read_to_string(gadget_file)?;
    let mut hits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if contains.is_empty()
            || line
                .to_ascii_lowercase()
                .contains(&contains.to_ascii_lowercase())
        {
            hits.push(serde_json::json!({"line": i + 1, "gadget": line}));
            if hits.len() >= 50 {
                break;
            }
        }
    }
    Ok(serde_json::json!({
        "module": "gadget_search",
        "file": gadget_file,
        "query": contains,
        "count": hits.len(),
        "hits": hits,
    }))
}

/// Write a browser harness HTML that loads a local lab target URL (scope is operator responsibility).
pub fn browser_harness_scaffold(out_dir: &Path, target_url: &str) -> Result<std::path::PathBuf> {
    if target_url.contains("://")
        && !target_url.starts_with("http://127.0.0.1")
        && !target_url.starts_with("http://localhost")
        && !target_url.starts_with("file:")
    {
        return Err(anyhow!(
            "ANUBIS_BROWSER_HARNESS_SCOPE: only localhost/file lab URLs in this tranche"
        ));
    }
    fs::create_dir_all(out_dir)?;
    let path = out_dir.join("browser_harness.html");
    let html = format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Anubis Browser Harness (lab)</title></head>
<body>
<h1>Anubis Browser Chain Harness (lab)</h1>
<p>Target: <code>{url}</code></p>
<iframe id="t" src="{url}" style="width:100%;height:60vh;border:1px solid #333"></iframe>
<script>
// Lab harness only — captures load errors for local target pages.
const t = document.getElementById('t');
t.addEventListener('load', () => console.log('frame loaded'));
window.addEventListener('error', e => console.error('harness error', e.message));
</script>
</body></html>
"#,
        url = target_url
    );
    fs::write(&path, html)?;
    Ok(path)
}
