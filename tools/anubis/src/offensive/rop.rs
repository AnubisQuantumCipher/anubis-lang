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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn pattern_create_correct_length() {
        assert_eq!(pattern_create(0).len(), 0);
        assert_eq!(pattern_create(1).len(), 1);
        assert_eq!(pattern_create(100).len(), 100);
        assert_eq!(pattern_create(5000).len(), 5000);
    }

    #[test]
    fn pattern_create_deterministic() {
        let a = pattern_create(256);
        let b = pattern_create(256);
        assert_eq!(a, b, "pattern_create must be deterministic");
    }

    #[test]
    fn pattern_create_uses_only_alpha() {
        let p = pattern_create(1000);
        assert!(
            p.chars().all(|c| c.is_ascii_lowercase()),
            "pattern must be lowercase alpha only"
        );
    }

    #[test]
    fn pattern_offset_finds_ascii_needle() {
        let r = pattern_offset(1000, "abc").unwrap();
        assert_eq!(r["found"], true);
        assert_eq!(r["offset"], 0, "abc should be at offset 0");

        let r = pattern_offset(1000, "bcd").unwrap();
        assert_eq!(r["found"], true);
        assert_eq!(r["offset"], 1);
    }

    #[test]
    fn pattern_offset_hex_needle() {
        let r = pattern_offset(1000, "0x616263").unwrap();
        assert_eq!(r["found"], true);
        assert_eq!(r["offset"], 0, "0x616263 is 'abc' at offset 0");
    }

    #[test]
    fn pattern_offset_not_found() {
        let r = pattern_offset(26, "ZZZZ").unwrap();
        assert_eq!(r["found"], false);
        assert!(r["offset"].is_null());
    }

    #[test]
    fn gadget_search_missing_file() {
        let err = gadget_search(Path::new("/nonexistent/gadgets.txt"), "ret").unwrap_err();
        assert!(
            err.to_string().contains("ANUBIS_ROP_GADGET_FILE_MISSING"),
            "got {err}"
        );
    }

    #[test]
    fn gadget_search_finds_matching_lines() {
        let dir = std::env::temp_dir().join(format!("anubis-rop-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let gf = dir.join("gadgets.txt");
        let mut f = std::fs::File::create(&gf).unwrap();
        writeln!(f, "0x1000 libfoo+0x10 ; pop rdi ; ret").unwrap();
        writeln!(f, "0x1004 libfoo+0x14 ; mov rax, rbx ; nop").unwrap();
        writeln!(f, "0x1008 libfoo+0x18 ; pop rsi ; ret").unwrap();
        drop(f);

        let r = gadget_search(&gf, "ret").unwrap();
        assert_eq!(r["count"], 2, "should find 2 gadgets with 'ret'");

        let r = gadget_search(&gf, "POP").unwrap();
        assert_eq!(r["count"], 2, "case-insensitive: POP matches pop");

        let r = gadget_search(&gf, "syscall").unwrap();
        assert_eq!(r["count"], 0, "no syscall gadgets");

        let r = gadget_search(&gf, "").unwrap();
        assert_eq!(r["count"], 3, "empty query matches all");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gadget_search_caps_at_50() {
        let dir = std::env::temp_dir().join(format!("anubis-rop-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let gf = dir.join("big.txt");
        let mut f = std::fs::File::create(&gf).unwrap();
        for i in 0..200 {
            writeln!(f, "0x{:04x} lib+{:x} ; ret", i, i).unwrap();
        }
        drop(f);

        let r = gadget_search(&gf, "ret").unwrap();
        assert_eq!(r["count"], 50, "should cap at 50 hits");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn browser_harness_rejects_remote_url() {
        let dir = std::env::temp_dir().join(format!("anubis-harness-{}", std::process::id()));
        let err = browser_harness_scaffold(&dir, "https://evil.example.com").unwrap_err();
        assert!(
            err.to_string().contains("ANUBIS_BROWSER_HARNESS_SCOPE"),
            "got {err}"
        );
    }

    #[test]
    fn browser_harness_allows_localhost() {
        let dir = std::env::temp_dir().join(format!("anubis-harness-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = browser_harness_scaffold(&dir, "http://127.0.0.1:8080/target").unwrap();
        assert!(path.exists());
        let html = std::fs::read_to_string(&path).unwrap();
        assert!(html.contains("127.0.0.1:8080/target"));
        assert!(html.contains("<iframe"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
