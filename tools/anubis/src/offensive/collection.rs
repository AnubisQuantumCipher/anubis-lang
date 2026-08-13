//! Collection module — T10 (TA0009).
//!
//! Data collection and staging inside VZ guests. Host-side: file staging
//! from engagement evidence. Guest-side: clipboard, screen capture planning,
//! automated file harvest.

use super::engagement::Engagement;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

fn content_preview(content: &str) -> String {
    content.chars().take(200).collect()
}

fn clipboard_content_evidence(content: &[u8]) -> (usize, String, String) {
    let hash = hex::encode(Sha256::digest(content));
    let preview = content_preview(&String::from_utf8_lossy(content));
    (content.len(), hash, preview)
}

/// Clipboard capture (macOS pbpaste / Linux xclip).
///
/// Captures current clipboard contents. Maps to T1115.
/// Runs inside VZ guest for implant simulation.
pub fn clipboard_capture(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    let (cmd, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("pbpaste", &[])
    } else {
        ("xclip", &["-selection", "clipboard", "-o"])
    };

    let output = Command::new(cmd).args(args).output();
    match output {
        Ok(o) if o.status.success() => {
            let (content_length, hash, preview) = clipboard_content_evidence(&o.stdout);
            Ok(json!({
                "schema": "aop-collection-v1",
                "module": "clipboard_capture",
                "engagement_id": eng.engagement_id,
                "content_length": content_length,
                "content_sha256": hash,
                "content_preview": preview,
                "attck": ["T1115"],
                "executed": true,
            }))
        }
        _ => Ok(json!({
            "schema": "aop-collection-v1",
            "module": "clipboard_capture",
            "engagement_id": eng.engagement_id,
            "content_length": 0,
            "error": "clipboard not available or empty",
            "attck": ["T1115"],
            "executed": true,
        })),
    }
}

/// Stage files from a target directory into engagement loot.
///
/// Copies files matching patterns into `engage_dir/loot/staged/`.
/// Computes SHA-256 for each file. Maps to T1074.001.
pub fn stage_files(
    eng: &Engagement,
    engage_dir: &Path,
    source_dir: &Path,
    patterns: &[String],
    max_file_size: u64,
) -> Result<Value> {
    eng.validate_live()?;
    if !source_dir.is_dir() {
        return Err(anyhow!(
            "ANUBIS_COLLECT_SOURCE_MISSING: {} not found",
            source_dir.display()
        ));
    }

    let staging = engage_dir.join("loot/staged");
    fs::create_dir_all(&staging)?;

    let mut staged: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut total_bytes = 0u64;

    for pattern in patterns {
        let output = Command::new("find")
            .args([
                source_dir.to_str().unwrap_or("."),
                "-maxdepth", "3",
                "-name", pattern,
                "-type", "f",
            ])
            .output();

        if let Ok(o) = output {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let src = Path::new(trimmed);
                let meta = match fs::metadata(src) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if meta.len() > max_file_size {
                    skipped.push(json!({
                        "path": trimmed,
                        "reason": "exceeds_max_size",
                        "size": meta.len(),
                    }));
                    continue;
                }

                let content = match fs::read(src) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let hash = hex::encode(Sha256::digest(&content));
                let dest_name = format!("{hash}_{}", src.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"));
                let dest = staging.join(&dest_name);
                fs::write(&dest, &content)?;

                total_bytes += meta.len();
                staged.push(json!({
                    "source": trimmed,
                    "staged_as": dest.display().to_string(),
                    "sha256": hash,
                    "size": meta.len(),
                    "pattern": pattern,
                }));
            }
        }
    }

    Ok(json!({
        "schema": "aop-collection-v1",
        "module": "stage_files",
        "engagement_id": eng.engagement_id,
        "source_dir": source_dir.display().to_string(),
        "staging_dir": staging.display().to_string(),
        "files_staged": staged.len(),
        "files_skipped": skipped.len(),
        "total_bytes": total_bytes,
        "staged": staged,
        "skipped": skipped,
        "attck": ["T1074.001"],
        "executed": true,
    }))
}

/// Screen capture planning (PLAN_ONLY — requires VZ guest + GUI).
pub fn screen_capture_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-collection-v1",
        "module": "screen_capture_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1113"],
        "macos_method": "screencapture -x /tmp/screenshot.png",
        "linux_method": "xdotool + import (ImageMagick)",
        "steps": [
            "Run inside VZ guest with GUI access",
            "Capture at configurable interval",
            "Stage screenshots to engagement loot",
            "Hash each capture for evidence chain",
        ],
        "detection_question": "Does EDR detect screencapture/import CLI usage?",
        "policy": {
            "never_auto_executes": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Keylogging plan (PLAN_ONLY — never auto-executes).
pub fn keylog_plan(eng: &Engagement) -> Result<Value> {
    eng.validate_live()?;
    Ok(json!({
        "schema": "aop-collection-v1",
        "module": "keylog_plan",
        "status": "PLAN_ONLY",
        "executed": false,
        "engagement_id": eng.engagement_id,
        "attck": ["T1056.001"],
        "techniques": {
            "macos": [
                "CGEventTap (Quartz event services) — requires accessibility permissions",
                "IOKit HID — lower level, kernel extension or DriverKit",
            ],
            "linux": [
                "/dev/input/eventN — requires root or input group",
                "X11 XRecord extension",
                "LD_PRELOAD interception",
            ],
        },
        "detection_question": "Does EDR detect IOKit/CGEventTap hooking or /dev/input reads?",
        "policy": {
            "never_auto_executes": true,
            "requires_dual_auth": true,
            "requires_vz_guest": true,
        },
    }))
}

/// Archive engagement loot into a compressed evidence bundle.
pub fn archive_loot(eng: &Engagement, engage_dir: &Path, out: &Path) -> Result<Value> {
    eng.validate_live()?;
    let loot_dir = engage_dir.join("loot");
    if !loot_dir.is_dir() {
        return Err(anyhow!("ANUBIS_COLLECT_NO_LOOT: {} not found", loot_dir.display()));
    }

    fs::create_dir_all(out)?;
    let archive_name = format!("loot_{}.tar.gz", eng.engagement_id);
    let archive_path = out.join(&archive_name);

    let status = Command::new("tar")
        .args([
            "czf",
            archive_path.to_str().unwrap_or("loot.tar.gz"),
            "-C", engage_dir.to_str().unwrap_or("."),
            "loot",
        ])
        .status();

    let success = status.map(|s| s.success()).unwrap_or(false);
    let archive_size = if success {
        fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    let archive_hash = if success {
        let content = fs::read(&archive_path).unwrap_or_default();
        hex::encode(Sha256::digest(&content))
    } else {
        String::new()
    };

    Ok(json!({
        "schema": "aop-collection-v1",
        "module": "archive_loot",
        "engagement_id": eng.engagement_id,
        "archive": archive_path.display().to_string(),
        "archive_size": archive_size,
        "archive_sha256": archive_hash,
        "success": success,
        "attck": ["T1560.001"],
        "executed": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offensive::engagement::Engagement;

    #[test]
    fn clipboard_capture_does_not_panic() {
        let eng = Engagement::default_lab("collect-test", "lab-auth");
        let result = clipboard_capture(&eng);
        assert!(result.is_ok());
    }

    #[test]
    fn clipboard_preview_truncates_at_a_character_boundary() {
        let content = format!("{}’suffix", "a".repeat(199));
        let preview = content_preview(&content);
        assert_eq!(preview.chars().count(), 200);
        assert!(preview.ends_with('’'));
    }

    #[test]
    fn clipboard_evidence_hashes_raw_bytes_not_lossy_preview_text() {
        let raw = [0xff, b'a', 0x00, 0xfe];
        let (length, hash, preview) = clipboard_content_evidence(&raw);
        assert_eq!(length, raw.len());
        assert_eq!(hash, hex::encode(Sha256::digest(raw)));
        assert_ne!(hash, hex::encode(Sha256::digest(preview.as_bytes())));
        assert!(preview.contains('\u{fffd}'));
    }

    #[test]
    fn stage_files_rejects_missing_source() {
        let eng = Engagement::default_lab("collect-test", "lab-auth");
        let err = stage_files(
            &eng,
            Path::new("/tmp/fake-engage"),
            Path::new("/nonexistent/dir"),
            &["*.txt".into()],
            1024 * 1024,
        ).unwrap_err().to_string();
        assert!(err.contains("ANUBIS_COLLECT_SOURCE_MISSING"), "{err}");
    }

    #[test]
    fn screen_capture_plan_is_plan_only() {
        let eng = Engagement::default_lab("collect-test", "lab-auth");
        let result = screen_capture_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["executed"], false);
    }

    #[test]
    fn keylog_plan_is_plan_only() {
        let eng = Engagement::default_lab("collect-test", "lab-auth");
        let result = keylog_plan(&eng).unwrap();
        assert_eq!(result["status"], "PLAN_ONLY");
        assert_eq!(result["executed"], false);
    }

    #[test]
    fn archive_loot_rejects_missing_loot_dir() {
        let eng = Engagement::default_lab("collect-test", "lab-auth");
        let err = archive_loot(
            &eng,
            Path::new("/nonexistent/engage"),
            Path::new("/tmp/out"),
        ).unwrap_err().to_string();
        assert!(err.contains("ANUBIS_COLLECT_NO_LOOT"), "{err}");
    }
}
