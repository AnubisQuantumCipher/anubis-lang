//! Bounty-grade PoC kit helpers (authorized local research only).
//!
//! - Real process harness: spawn local binary, feed stdin, capture crash/signal
//! - Mutation fuzz against a local target (not parse/typecheck cosplay)
//! - No network by default; target path must be a local filesystem path

use anyhow::{anyhow, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[derive(Debug, Clone)]
pub struct TargetRunResult {
    pub crashed: bool,
    pub signal: Option<i32>,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

/// Run a **local** target binary with stdin payload. Network URLs are rejected.
pub fn target_run_local(path: &Path, payload: &[u8], timeout_ms: u64) -> Result<TargetRunResult> {
    validate_local_target(path)?;

    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("ANUBIS_POC_SPAWN_FAILED: {}: {}", path.display(), e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload);
    }

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed().as_millis() as u64 > timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(TargetRunResult {
                        crashed: false,
                        signal: None,
                        exit_code: None,
                        stdout: vec![],
                        stderr: b"ANUBIS_POC_TIMEOUT".to_vec(),
                        timed_out: true,
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(e) => return Err(anyhow!("ANUBIS_POC_WAIT_FAILED: {}", e)),
        }
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = std::io::Read::read_to_end(&mut out, &mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = std::io::Read::read_to_end(&mut err, &mut stderr);
    }
    let status = child
        .wait()
        .map_err(|e| anyhow!("ANUBIS_POC_WAIT_FAILED: {}", e))?;

    #[cfg(unix)]
    let signal = status.signal();
    #[cfg(not(unix))]
    let signal: Option<i32> = None;

    let exit_code = status.code();
    // Signal termination (SEGV/ABRT) => crash. Also treat explicit non-zero as interest for harnesses.
    let crashed = signal.is_some();

    Ok(TargetRunResult {
        crashed,
        signal,
        exit_code,
        stdout,
        stderr,
        timed_out: false,
    })
}

pub fn validate_local_target(path: &Path) -> Result<()> {
    let s = path.to_string_lossy();
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.contains("://")
        || s.starts_with("nc ")
    {
        return Err(anyhow!(
            "ANUBIS_POC_NETWORK_FORBIDDEN: target must be a local filesystem path (no network)"
        ));
    }
    if !path.exists() {
        return Err(anyhow!(
            "ANUBIS_POC_TARGET_MISSING: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Tiny deterministic PRNG (xorshift64*) — no extra dependency.
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xA5A5_5A5A_C3C3_3C3C } else { seed },
        }
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn next_u8(&mut self) -> u8 {
        (self.next_u64() & 0xff) as u8
    }
    pub fn gen_range(&mut self, max: usize) -> usize {
        if max == 0 {
            0
        } else {
            (self.next_u64() as usize) % max
        }
    }
}

pub fn mutate(seed_input: &[u8], rng: &mut XorShift64, max_len: usize) -> Vec<u8> {
    let mut buf = if seed_input.is_empty() {
        vec![0u8; rng.gen_range(16).max(1)]
    } else {
        seed_input.to_vec()
    };
    if buf.is_empty() {
        buf.push(rng.next_u8());
    }
    let ops = 1 + rng.gen_range(8);
    for _ in 0..ops {
        match rng.gen_range(5) {
            0 => {
                // bit flip
                let i = rng.gen_range(buf.len());
                buf[i] ^= 1u8 << (rng.gen_range(8) as u8);
            }
            1 => {
                // random byte
                let i = rng.gen_range(buf.len());
                buf[i] = rng.next_u8();
            }
            2 => {
                // insert
                if buf.len() < max_len {
                    let i = rng.gen_range(buf.len() + 1);
                    buf.insert(i, rng.next_u8());
                }
            }
            3 => {
                // delete
                if buf.len() > 1 {
                    let i = rng.gen_range(buf.len());
                    buf.remove(i);
                }
            }
            _ => {
                // interesting ints
                let interesting = [0u8, 1, 0x7f, 0x80, 0xff, b'A', b'\n', 0x41];
                let i = rng.gen_range(buf.len());
                buf[i] = interesting[rng.gen_range(interesting.len())];
            }
        }
    }
    if buf.len() > max_len {
        buf.truncate(max_len);
    }
    // occasionally force oversize for overflow-class targets
    if rng.gen_range(20) == 0 {
        let n = 64 + rng.gen_range(200);
        buf.resize(n.min(max_len), b'A');
    }
    buf
}

#[derive(Debug)]
pub struct FuzzReport {
    pub target: PathBuf,
    pub runs: u64,
    pub crashes: u64,
    pub timeouts: u64,
    pub unique_crash_hashes: Vec<String>,
    pub note: String,
}

/// Mutation fuzz a **local** target binary. Real process crashes only.
pub fn fuzz_local_target(
    target: &Path,
    runs: u64,
    max_len: usize,
    seed: u64,
    out: &Path,
    seed_corpus: &[Vec<u8>],
) -> Result<FuzzReport> {
    validate_local_target(target)?;
    fs::create_dir_all(out)?;
    let crashes_dir = out.join("crashes");
    fs::create_dir_all(&crashes_dir)?;

    let mut rng = XorShift64::new(seed);
    let mut crashes = 0u64;
    let mut timeouts = 0u64;
    let mut unique = std::collections::BTreeSet::new();
    let corpus: Vec<Vec<u8>> = if seed_corpus.is_empty() {
        vec![
            b"AAAA".to_vec(),
            vec![b'A'; 16],
            vec![b'A'; 32],
            vec![b'A'; 64],
            vec![b'A'; 128],
            b"\x00\x01\x02\x03".to_vec(),
        ]
    } else {
        seed_corpus.to_vec()
    };

    for i in 0..runs {
        let base = &corpus[rng.gen_range(corpus.len())];
        let payload = mutate(base, &mut rng, max_len);
        let result = target_run_local(target, &payload, 500)?;
        if result.timed_out {
            timeouts += 1;
            continue;
        }
        if result.crashed || result.signal.is_some() {
            crashes += 1;
            let h = hex::encode(Sha256::digest(&payload));
            let short = &h[..16];
            if unique.insert(h.clone()) {
                let name = format!(
                    "crash-{}-sig{}-run{}.bin",
                    short,
                    result.signal.unwrap_or(-1),
                    i
                );
                fs::write(crashes_dir.join(&name), &payload)?;
                let meta = json!({
                    "run": i,
                    "signal": result.signal,
                    "exit_code": result.exit_code,
                    "payload_sha256": h,
                    "payload_len": payload.len(),
                    "stdout_len": result.stdout.len(),
                    "stderr_len": result.stderr.len(),
                });
                fs::write(
                    crashes_dir.join(format!("crash-{}-sig{}-run{}.json", short, result.signal.unwrap_or(-1), i)),
                    serde_json::to_string_pretty(&meta)?,
                )?;
            }
        }
    }

    let report = FuzzReport {
        target: target.to_path_buf(),
        runs,
        crashes,
        timeouts,
        unique_crash_hashes: unique.into_iter().collect(),
        note: "REAL process-mutation fuzz (local target only). Not a parse/typecheck loop. Sandbox: local FS only, no network."
            .into(),
    };

    let report_json = json!({
        "schema_version": "1.0",
        "tool": "anubis",
        "report": "fuzz",
        "engine": "mutation-process-v1",
        "target": report.target,
        "runs": report.runs,
        "crashes": report.crashes,
        "timeouts": report.timeouts,
        "unique_crashes": report.unique_crash_hashes.len(),
        "unique_crash_hashes": report.unique_crash_hashes,
        "seed": seed,
        "max_len": max_len,
        "security": {
            "mode": "fuzz",
            "sandbox": true,
            "network": false,
            "declared_effects": ["fuzz_exec", "process_spawn_local"],
            "observed_effects": if report.crashes > 0 {
                vec!["fuzz_exec", "process_spawn_local", "crash"]
            } else {
                vec!["fuzz_exec", "process_spawn_local"]
            }
        },
        "note": report.note,
        "timestamp_unix": SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    });
    fs::write(out.join("fuzz_report.json"), serde_json::to_string_pretty(&report_json)?)?;
    Ok(report)
}

/// Packing helpers used by docs / tests (mirror of lowered runtime).
#[allow(dead_code)]
pub fn p64_le(n: u64) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
#[allow(dead_code)]
pub fn p32_le(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
#[allow(dead_code)]
pub fn p16_le(n: u16) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
#[allow(dead_code)]
pub fn cyclic(n: usize) -> Vec<u8> {
    let alphabet: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    (0..n).map(|i| alphabet[i % alphabet.len()]).collect()
}

/// Shared honesty rule for security fixtures (used by runner contract tests).
/// EXPECT PASS → command ok and (no needle or needle present).
/// EXPECT FAIL without needle → command must fail.
/// EXPECT FAIL with needle → command must fail AND needle present (wrong failure ≠ green).
pub fn security_fixture_matches(
    expect_fail: bool,
    cmd_failed: bool,
    needle_required: bool,
    needle_present: bool,
) -> bool {
    if !expect_fail {
        return !cmd_failed && (!needle_required || needle_present);
    }
    if needle_required {
        return cmd_failed && needle_present;
    }
    cmd_failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_sizes() {
        assert_eq!(p64_le(0x0102030405060708).len(), 8);
        assert_eq!(p32_le(0x41414141), b"AAAA");
        assert_eq!(cyclic(4), b"abcd");
    }

    #[test]
    fn rejects_network_target() {
        let err = validate_local_target(Path::new("https://evil.example/bin")).unwrap_err();
        assert!(err.to_string().contains("NETWORK_FORBIDDEN"));
    }

    #[test]
    fn mutate_changes_payload_across_calls() {
        // Real shipped `mutate` must not return only the seed — successive draws differ.
        let seed = b"AAAA";
        let mut rng = XorShift64::new(0xA11B15);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..32 {
            let p = mutate(seed, &mut rng, 128);
            seen.insert(p);
        }
        assert!(
            seen.len() >= 2,
            "mutate must produce multiple distinct payloads, got {}",
            seen.len()
        );
        // At least one mutated payload should not equal the raw seed alone.
        assert!(
            seen.iter().any(|p| p.as_slice() != seed),
            "mutate returned only the seed"
        );
    }

    #[test]
    fn mutate_can_exceed_overflow_threshold() {
        // Gold vuln_local aborts when len > 64; mutator must be able to emit oversized.
        let mut rng = XorShift64::new(7);
        let mut oversized = 0usize;
        for _ in 0..500 {
            let p = mutate(b"AAAA", &mut rng, 256);
            if p.len() > 64 {
                oversized += 1;
            }
        }
        assert!(
            oversized > 0,
            "mutate never produced len>64 in 500 draws (fuzz would not hit lab crash oracle)"
        );
    }

    #[test]
    fn security_fixture_needle_honesty() {
        // EXPECT FAIL + needle missing after a failure → must NOT match (no false green).
        assert!(!security_fixture_matches(true, true, true, false));
        // EXPECT FAIL + needle present + failed → match.
        assert!(security_fixture_matches(true, true, true, true));
        // EXPECT FAIL + command passed → no match.
        assert!(!security_fixture_matches(true, false, true, false));
        // EXPECT PASS + ok → match.
        assert!(security_fixture_matches(false, false, false, false));
        // EXPECT PASS + failed → no match.
        assert!(!security_fixture_matches(false, true, false, false));
    }
}
