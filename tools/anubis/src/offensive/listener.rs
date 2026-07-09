//! Multi-transport C2 listener: HTTP + DNS + UDS, aop-2 encrypted, operator console.

use super::console;
use super::crypto;
use super::engagement::{Engagement, Role};
use super::protocol::{
    Beacon, BeaconResponse, EncryptedEnvelope, Task, TaskResult, PROTOCOL_V1, PROTOCOL_V2,
};
use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub struct State {
    pub agents: HashMap<String, serde_json::Value>,
    pub queue: HashMap<String, VecDeque<Task>>,
    pub results: Vec<TaskResult>,
}

pub fn listener_start(eng: &Engagement, engage_dir: &Path, _foreground: bool) -> Result<()> {
    eng.validate_live()?;
    super::scope::bind_addr_in_scope(
        &eng.c2_bind,
        eng.allow_non_loopback_bind && eng.network_egress,
        &eng.allowed_hosts,
    )?;

    fs::create_dir_all(engage_dir.join("listeners"))?;
    fs::create_dir_all(engage_dir.join("tasks"))?;
    let state = Arc::new(Mutex::new(State::default()));

    let meta = json!({
        "listener": "multi-transport-v2",
        "bind_http": eng.c2_bind,
        "bind_dns": eng.dns_bind,
        "uds": eng.uds_path,
        "transport": eng.transport,
        "protocol": PROTOCOL_V2,
        "encrypt": eng.encrypt_beacons,
        "mtls_ready": eng.mtls_ready,
        "engagement_id": eng.engagement_id,
        "started_unix": now_unix(),
    });
    fs::write(
        engage_dir.join("listeners/listener_meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;

    println!(
        "anubis listen: HTTP {} | DNS {} | UDS {} | protocol={} encrypt={}",
        eng.c2_bind, eng.dns_bind, eng.uds_path, PROTOCOL_V2, eng.encrypt_beacons
    );
    println!("  console: http://{}/", eng.c2_bind);
    println!("  POST /beacon /result /task | GET /health /agents /results /");

    let eng = eng.clone();
    let engage_dir = engage_dir.to_path_buf();

    // DNS transport thread
    if eng.transport == "dns" || eng.transport == "multi" {
        let st = state.clone();
        let eng_dns = eng.clone();
        let dir = engage_dir.clone();
        thread::spawn(move || {
            if let Err(e) = dns_loop(&eng_dns, &dir, st) {
                eprintln!("dns listener error: {e}");
            }
        });
    }

    // UDS transport thread
    if eng.transport == "uds" || eng.transport == "multi" {
        let st = state.clone();
        let eng_uds = eng.clone();
        let dir = engage_dir.clone();
        thread::spawn(move || {
            if let Err(e) = uds_loop(&eng_uds, &dir, st) {
                eprintln!("uds listener error: {e}");
            }
        });
    }

    // HTTP main thread
    let listener = TcpListener::bind(&eng.c2_bind)
        .map_err(|e| anyhow!("ANUBIS_LISTEN_BIND: {}: {}", eng.c2_bind, e))?;
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let state = state.clone();
                let eng = eng.clone();
                let engage_dir = engage_dir.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_http(&mut stream, &eng, &engage_dir, &state) {
                        let _ = write_raw(&mut stream, 500, b"error");
                        eprintln!("http client error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

fn handle_http(
    stream: &mut (impl Read + Write),
    eng: &Engagement,
    engage_dir: &Path,
    state: &Arc<Mutex<State>>,
) -> Result<()> {
    let mut buf = [0u8; 1 << 16];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let (method, path) = parse_request_line(&req);
    let body = extract_body(&req).to_string();
    let operator = header_value(&req, "X-Anubis-Operator").unwrap_or_else(|| "operator".into());

    drain_task_inbox(engage_dir, state)?;

    match (method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/console") => {
            let html = console::console_html(eng);
            write_html(stream, 200, &html)?;
        }
        ("GET", "/health") => write_json(
            stream,
            200,
            &json!({
                "ok": true,
                "engagement_id": eng.engagement_id,
                "protocol": PROTOCOL_V2,
                "encrypt": eng.encrypt_beacons,
                "transport": eng.transport,
                "mtls_ready": eng.mtls_ready,
            }),
        )?,
        ("GET", "/agents") => {
            let st = state.lock().unwrap();
            let agents: Vec<_> = st.agents.values().cloned().collect();
            write_json(stream, 200, &json!({ "agents": agents }))?;
        }
        ("GET", "/results") => {
            let st = state.lock().unwrap();
            write_json(stream, 200, &json!({ "results": st.results }))?;
        }
        ("POST", "/beacon") => {
            let beacon = decode_beacon(eng, &body)?;
            if beacon.engagement_id != eng.engagement_id {
                write_json(stream, 403, &json!({"error":"ANUBIS_C2_ENGAGEMENT_MISMATCH"}))?;
                return Ok(());
            }
            let resp = process_beacon(eng, engage_dir, state, &beacon)?;
            let out = encode_response(eng, &beacon.agent_id, &resp)?;
            write_raw(stream, 200, out.as_bytes())?;
        }
        ("POST", "/result") => {
            let result = decode_result(eng, &body)?;
            if result.engagement_id != eng.engagement_id {
                write_json(stream, 403, &json!({"error":"ANUBIS_C2_ENGAGEMENT_MISMATCH"}))?;
                return Ok(());
            }
            store_result(engage_dir, state, result)?;
            write_json(stream, 200, &json!({"ok": true}))?;
        }
        ("POST", "/task") => {
            if let Err(e) = eng.assert_role(&operator, Role::Operator) {
                write_json(stream, 403, &json!({"error": e.to_string()}))?;
                return Ok(());
            }
            let v: serde_json::Value = serde_json::from_str(&body)?;
            let agent_id = v
                .get("agent_id")
                .and_then(|x| x.as_str())
                .unwrap_or("*")
                .to_string();
            let module = v
                .get("module")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("module required"))?
                .to_string();
            let args = v
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let task = Task {
                id: format!(
                    "t-{}",
                    &hex::encode(Sha256::digest(
                        format!("{module}{agent_id}{}", now_unix()).as_bytes()
                    ))[..10]
                ),
                module,
                args,
            };
            let mut st = state.lock().unwrap();
            st.queue
                .entry(agent_id)
                .or_default()
                .push_back(task.clone());
            write_json(stream, 200, &json!({"queued": task, "operator": operator}))?;
        }
        _ => write_json(stream, 404, &json!({"error":"not found"}))?,
    }
    Ok(())
}

fn process_beacon(
    eng: &Engagement,
    engage_dir: &Path,
    state: &Arc<Mutex<State>>,
    beacon: &Beacon,
) -> Result<BeaconResponse> {
    let mut st = state.lock().unwrap();
    st.agents.insert(
        beacon.agent_id.clone(),
        json!({
            "agent_id": beacon.agent_id,
            "hostname": beacon.hostname,
            "os": beacon.os,
            "arch": beacon.arch,
            "pid": beacon.pid,
            "key_id": beacon.key_id,
            "last_beacon_unix": now_unix(),
        }),
    );
    let mut tasks = Vec::new();
    if let Some(q) = st.queue.get_mut(&beacon.agent_id) {
        while let Some(t) = q.pop_front() {
            tasks.push(t);
        }
    }
    if let Some(q) = st.queue.get_mut("*") {
        while let Some(t) = q.pop_front() {
            tasks.push(t);
        }
    }
    drop(st);
    let _ = append_evidence(
        engage_dir,
        "beacon",
        &json!({"agent_id": beacon.agent_id, "hostname": beacon.hostname, "protocol": beacon.protocol}),
    );
    Ok(BeaconResponse {
        protocol: if eng.encrypt_beacons {
            PROTOCOL_V2.into()
        } else {
            PROTOCOL_V1.into()
        },
        tasks,
        sleep_ms: eng.sleep_ms.max(beacon.sleep_ms).max(500),
        jitter_pct: eng.jitter_pct,
        die: false,
    })
}

fn store_result(
    engage_dir: &Path,
    state: &Arc<Mutex<State>>,
    result: TaskResult,
) -> Result<()> {
    {
        let mut st = state.lock().unwrap();
        st.results.push(result.clone());
    }
    let loot = engage_dir.join("loot");
    fs::create_dir_all(&loot)?;
    let fname = format!(
        "result-{}-{}.json",
        result.task_id,
        &hex::encode(Sha256::digest(result.output.as_bytes()))[..8]
    );
    fs::write(loot.join(fname), serde_json::to_string_pretty(&result)?)?;
    append_evidence(engage_dir, "task_result", &serde_json::to_value(&result)?)?;
    Ok(())
}

fn decode_beacon(eng: &Engagement, body: &str) -> Result<Beacon> {
    let body = body.trim();
    if body.contains("\"blob\"") || eng.encrypt_beacons {
        if let Ok(env) = serde_json::from_str::<EncryptedEnvelope>(body) {
            return crypto::open_json(&eng.psk_hex, &env.blob);
        }
    }
    Ok(serde_json::from_str(body)?)
}

fn decode_result(eng: &Engagement, body: &str) -> Result<TaskResult> {
    let body = body.trim();
    if body.contains("\"blob\"") || eng.encrypt_beacons {
        if let Ok(env) = serde_json::from_str::<EncryptedEnvelope>(body) {
            return crypto::open_json(&eng.psk_hex, &env.blob);
        }
    }
    Ok(serde_json::from_str(body)?)
}

fn encode_response(eng: &Engagement, agent_id: &str, resp: &BeaconResponse) -> Result<String> {
    if eng.encrypt_beacons {
        let blob = crypto::seal_json(&eng.psk_hex, resp)?;
        let env = EncryptedEnvelope {
            protocol: PROTOCOL_V2.into(),
            engagement_id: eng.engagement_id.clone(),
            agent_id: agent_id.into(),
            blob,
        };
        Ok(serde_json::to_string(&env)?)
    } else {
        Ok(serde_json::to_string(resp)?)
    }
}

/// Minimal DNS C2: TXT query name encodes agent id; response TXT carries base64 task blob length-limited.
fn dns_loop(eng: &Engagement, engage_dir: &Path, state: Arc<Mutex<State>>) -> Result<()> {
    let sock = UdpSocket::bind(&eng.dns_bind)
        .map_err(|e| anyhow!("ANUBIS_DNS_BIND: {}: {e}", eng.dns_bind))?;
    println!("dns listener on {}", eng.dns_bind);
    let mut buf = [0u8; 1500];
    loop {
        let (n, src) = sock.recv_from(&mut buf)?;
        // Very small lab DNS: if payload contains agent marker, enqueue presence
        let s = String::from_utf8_lossy(&buf[..n]);
        if s.contains("aop") || n > 12 {
            let agent_id = format!("dns-{}", &hex::encode(Sha256::digest(&buf[..n.min(32)]))[..8]);
            let mut st = state.lock().unwrap();
            st.agents.insert(
                agent_id.clone(),
                json!({
                    "agent_id": agent_id,
                    "transport": "dns",
                    "peer": src.to_string(),
                    "last_beacon_unix": now_unix(),
                }),
            );
            drop(st);
            let _ = append_evidence(
                engage_dir,
                "dns_query",
                &json!({"peer": src.to_string(), "bytes": n}),
            );
        }
        // Echo minimal DNS-like reply (not a full recursive server)
        let _ = sock.send_to(&buf[..n], src);
    }
}

fn uds_loop(eng: &Engagement, engage_dir: &Path, state: Arc<Mutex<State>>) -> Result<()> {
    let path = PathBuf::from(&eng.uds_path);
    let _ = fs::remove_file(&path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(&path)
        .map_err(|e| anyhow!("ANUBIS_UDS_BIND: {}: {e}", path.display()))?;
    println!("uds listener on {}", path.display());
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            continue;
        }
        // Accept beacon or result JSON / encrypted envelope
        if let Ok(beacon) = decode_beacon(eng, buf.trim()) {
            let resp = process_beacon(eng, engage_dir, &state, &beacon)?;
            let out = encode_response(eng, &beacon.agent_id, &resp)?;
            let _ = stream.write_all(out.as_bytes());
        } else if let Ok(result) = decode_result(eng, buf.trim()) {
            store_result(engage_dir, &state, result)?;
            let _ = stream.write_all(br#"{"ok":true}"#);
        }
    }
    Ok(())
}

fn drain_task_inbox(engage_dir: &Path, state: &Arc<Mutex<State>>) -> Result<()> {
    let inbox = engage_dir.join("tasks/inbox.jsonl");
    if !inbox.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(&inbox)?;
    if data.trim().is_empty() {
        return Ok(());
    }
    let mut st = state.lock().unwrap();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let agent_id = v
                .get("agent_id")
                .and_then(|x| x.as_str())
                .unwrap_or("*")
                .to_string();
            let module = v
                .get("module")
                .and_then(|x| x.as_str())
                .unwrap_or("whoami")
                .to_string();
            let args = v
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let task = Task {
                id: format!(
                    "t-{}",
                    &hex::encode(Sha256::digest(format!("{line}{}", now_unix()).as_bytes()))[..10]
                ),
                module,
                args,
            };
            st.queue.entry(agent_id).or_default().push_back(task);
        }
    }
    let _ = fs::write(&inbox, "");
    Ok(())
}

fn parse_request_line(req: &str) -> (String, String) {
    let line = req.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    (
        parts.next().unwrap_or("GET").into(),
        parts.next().unwrap_or("/").into(),
    )
}

fn extract_body(req: &str) -> &str {
    if let Some(idx) = req.find("\r\n\r\n") {
        &req[idx + 4..]
    } else if let Some(idx) = req.find("\n\n") {
        &req[idx + 2..]
    } else {
        ""
    }
}

fn header_value(req: &str, name: &str) -> Option<String> {
    for line in req.lines() {
        if let Some(rest) = line
            .strip_prefix(name)
            .or_else(|| line.strip_prefix(&name.to_ascii_lowercase()))
        {
            if let Some(v) = rest.strip_prefix(':') {
                return Some(v.trim().to_string());
            }
        }
        // case-insensitive scan
        if line.to_ascii_lowercase().starts_with(&name.to_ascii_lowercase()) {
            if let Some((_, v)) = line.split_once(':') {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn write_json(stream: &mut impl Write, status: u16, body: &impl Serialize) -> Result<()> {
    let payload = serde_json::to_vec(body)?;
    write_raw(stream, status, &payload)
}

fn write_html(stream: &mut impl Write, status: u16, html: &str) -> Result<()> {
    let reason = if status == 200 { "OK" } else { "Error" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        html.len()
    )?;
    stream.write_all(html.as_bytes())?;
    Ok(())
}

fn write_raw(stream: &mut impl Write, status: u16, payload: &[u8]) -> Result<()> {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(payload)?;
    Ok(())
}

fn append_evidence(engage_dir: &Path, kind: &str, value: &serde_json::Value) -> Result<()> {
    let dir = engage_dir.join("evidence");
    fs::create_dir_all(&dir)?;
    let line = json!({"ts_unix": now_unix(), "kind": kind, "data": value});
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("actions.jsonl"))?;
    writeln!(f, "{}", serde_json::to_string(&line)?)?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn queue_task_file(
    engage_dir: &Path,
    agent_id: &str,
    module: &str,
    args: &[String],
) -> Result<PathBuf> {
    let inbox = engage_dir.join("tasks/inbox.jsonl");
    fs::create_dir_all(engage_dir.join("tasks"))?;
    let line = json!({"agent_id": agent_id, "module": module, "args": args});
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&inbox)?;
    writeln!(f, "{}", serde_json::to_string(&line)?)?;
    Ok(inbox)
}
