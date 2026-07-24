//! Production DNS / DoH C2 codec for AOP-2.
#![allow(dead_code)] // public codec surface; not all helpers are used by the listener path
//!
//! Wire shape (lab-safe, loopback default):
//! - Beacon / result payload = base32 (RFC 4648, no padding) of the aop-2 JSON/envelope bytes.
//! - Split into DNS labels of ≤60 chars under zone `aop.c2`.
//! - QNAME: `<seq>.<total>.<kind>.<lab0>.<lab1>….aop.c2`
//!   where `kind` is `b` (beacon), `r` (result), `p` (poll/task pull).
//! - Response: TXT RDATA carrying base32 response chunks (same codec).
//! - DoH: RFC 8484 `application/dns-message` POST/GET plus convenience
//!   `POST /doh` with JSON `{"qname":"…"}` / raw DNS wire.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine};
use serde::{Deserialize, Serialize};

pub const C2_ZONE: &str = "aop.c2";
pub const LABEL_MAX: usize = 60;
/// Max payload labels per name (keep under ~200 chars of labels + zone).
pub const MAX_PAYLOAD_LABELS: usize = 3;

/// DNS C2 kind encoded in the QNAME.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsKind {
    Beacon,
    Result,
    Poll,
}

impl DnsKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DnsKind::Beacon => "b",
            DnsKind::Result => "r",
            DnsKind::Poll => "p",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "b" => Some(DnsKind::Beacon),
            "r" => Some(DnsKind::Result),
            "p" => Some(DnsKind::Poll),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsC2Message {
    pub seq: u16,
    pub total: u16,
    pub kind: DnsKind,
    /// Raw (possibly partial) payload bytes for this fragment.
    pub payload: Vec<u8>,
}

/// RFC 4648 base32 alphabet (uppercase), no padding.
const B32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32_encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity((data.len() * 8).div_ceil(5));
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;
    for &b in data {
        buffer = (buffer << 8) | u64::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(B32_ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(B32_ALPHABET[idx] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim().to_ascii_uppercase().replace('=', "");
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;
    for c in s.chars() {
        let v = match c {
            'A'..='Z' => c as u8 - b'A',
            '2'..='7' => c as u8 - b'2' + 26,
            // ignore separators
            '.' | '-' | '_' | ' ' => continue,
            _ => return Err(anyhow!("ANUBIS_DNS_B32_CHAR: invalid '{c}'")),
        };
        buffer = (buffer << 5) | u64::from(v);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

/// Encode full payload into one or more QNAMEs (fragmented).
pub fn encode_qnames(kind: DnsKind, payload: &[u8]) -> Result<Vec<String>> {
    let b32 = base32_encode(payload);
    if b32.is_empty() {
        // empty poll / heartbeat
        return Ok(vec![format!("0.1.{}.x.{}", kind.as_str(), C2_ZONE)]);
    }
    let chunks: Vec<&str> = b32
        .as_bytes()
        .chunks(LABEL_MAX)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    // Group into messages of MAX_PAYLOAD_LABELS labels each
    let mut out = Vec::new();
    let groups: Vec<&[&str]> = chunks.chunks(MAX_PAYLOAD_LABELS).collect();
    let total = groups.len() as u16;
    if total == 0 {
        return Ok(vec![format!("0.1.{}.x.{}", kind.as_str(), C2_ZONE)]);
    }
    for (i, g) in groups.iter().enumerate() {
        let labels = g.join(".");
        out.push(format!(
            "{}.{}.{}.{}.{}",
            i,
            total,
            kind.as_str(),
            labels,
            C2_ZONE
        ));
    }
    Ok(out)
}

/// Parse a QNAME into a C2 fragment.
pub fn decode_qname(qname: &str) -> Result<DnsC2Message> {
    let q = qname.trim_end_matches('.').to_ascii_lowercase();
    let zone = C2_ZONE.to_ascii_lowercase();
    if !q.ends_with(&zone) {
        return Err(anyhow!("ANUBIS_DNS_ZONE: qname not under {C2_ZONE}"));
    }
    let prefix = &q[..q.len() - zone.len()].trim_end_matches('.');
    let parts: Vec<&str> = prefix.split('.').filter(|p| !p.is_empty()).collect();
    // seq.total.kind.payload_labels...
    if parts.len() < 3 {
        return Err(anyhow!("ANUBIS_DNS_QNAME_SHORT"));
    }
    let seq: u16 = parts[0].parse().map_err(|_| anyhow!("ANUBIS_DNS_SEQ"))?;
    let total: u16 = parts[1].parse().map_err(|_| anyhow!("ANUBIS_DNS_TOTAL"))?;
    let kind = DnsKind::parse(parts[2]).ok_or_else(|| anyhow!("ANUBIS_DNS_KIND"))?;
    let payload_labels = if parts.len() > 3 {
        parts[3..].join("")
    } else {
        String::new()
    };
    let payload = if payload_labels == "x" || payload_labels.is_empty() {
        Vec::new()
    } else {
        base32_decode(&payload_labels)?
    };
    Ok(DnsC2Message {
        seq,
        total,
        kind,
        payload,
    })
}

/// Reassemble ordered fragments (caller groups by session).
pub fn reassemble(frags: &[DnsC2Message]) -> Result<Vec<u8>> {
    if frags.is_empty() {
        return Ok(Vec::new());
    }
    let total = frags[0].total;
    let kind = frags[0].kind;
    let mut ordered: Vec<Option<&DnsC2Message>> = vec![None; total as usize];
    for f in frags {
        if f.total != total || f.kind != kind {
            return Err(anyhow!("ANUBIS_DNS_REASSEMBLE_MISMATCH"));
        }
        if f.seq as usize >= ordered.len() {
            return Err(anyhow!("ANUBIS_DNS_SEQ_OOB"));
        }
        ordered[f.seq as usize] = Some(f);
    }
    let mut out = Vec::new();
    for (i, slot) in ordered.iter().enumerate() {
        let f = slot.ok_or_else(|| anyhow!("ANUBIS_DNS_MISSING_FRAG: {i}"))?;
        out.extend_from_slice(&f.payload);
    }
    Ok(out)
}

/// Encode response bytes as DNS TXT strings (split ≤255 per TXT chunk, base32).
pub fn encode_txt_payload(payload: &[u8]) -> Vec<String> {
    let b32 = base32_encode(payload);
    if b32.is_empty() {
        return vec!["OK".into()];
    }
    b32.as_bytes()
        .chunks(200)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect()
}

pub fn decode_txt_payload(txts: &[String]) -> Result<Vec<u8>> {
    let joined: String = txts
        .iter()
        .map(|t| t.trim())
        .filter(|t| *t != "OK")
        .collect();
    if joined.is_empty() {
        return Ok(Vec::new());
    }
    base32_decode(&joined)
}

// --- Minimal DNS wire codec (query + TXT answer) ---

#[derive(Debug, Clone)]
pub struct DnsQuery {
    pub id: u16,
    pub qname: String,
    pub qtype: u16,
}

/// Parse a DNS query message; returns first question.
pub fn parse_dns_query(buf: &[u8]) -> Result<DnsQuery> {
    if buf.len() < 12 {
        return Err(anyhow!("ANUBIS_DNS_WIRE_SHORT"));
    }
    let id = u16::from_be_bytes([buf[0], buf[1]]);
    let qdcount = u16::from_be_bytes([buf[4], buf[5]]);
    if qdcount == 0 {
        return Err(anyhow!("ANUBIS_DNS_NO_QUESTION"));
    }
    let mut i = 12usize;
    let (qname, ni) = read_name(buf, i)?;
    i = ni;
    if i + 4 > buf.len() {
        return Err(anyhow!("ANUBIS_DNS_QTYPE_SHORT"));
    }
    let qtype = u16::from_be_bytes([buf[i], buf[i + 1]]);
    Ok(DnsQuery { id, qname, qtype })
}

fn read_name(buf: &[u8], mut i: usize) -> Result<(String, usize)> {
    let mut labels = Vec::new();
    let mut jumps = 0;
    let mut return_i = None;
    loop {
        if i >= buf.len() {
            return Err(anyhow!("ANUBIS_DNS_NAME_OOB"));
        }
        let len = buf[i];
        if len == 0 {
            i += 1;
            break;
        }
        if len & 0xc0 == 0xc0 {
            if i + 1 >= buf.len() {
                return Err(anyhow!("ANUBIS_DNS_PTR_OOB"));
            }
            let off = (((len as usize) & 0x3f) << 8) | buf[i + 1] as usize;
            if return_i.is_none() {
                return_i = Some(i + 2);
            }
            i = off;
            jumps += 1;
            if jumps > 10 {
                return Err(anyhow!("ANUBIS_DNS_PTR_LOOP"));
            }
            continue;
        }
        i += 1;
        let end = i + len as usize;
        if end > buf.len() {
            return Err(anyhow!("ANUBIS_DNS_LABEL_OOB"));
        }
        labels.push(String::from_utf8_lossy(&buf[i..end]).into_owned());
        i = end;
    }
    let end_i = return_i.unwrap_or(i);
    Ok((labels.join("."), end_i))
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() {
            continue;
        }
        let b = label.as_bytes();
        out.push(b.len() as u8);
        out.extend_from_slice(b);
    }
    out.push(0);
}

/// Build a DNS response with TXT answers for the given query.
pub fn build_txt_response(query: &DnsQuery, txts: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    out.extend_from_slice(&query.id.to_be_bytes());
    // flags: response, recursion available, no error
    out.extend_from_slice(&0x8180u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    out.extend_from_slice(&(txts.len() as u16).to_be_bytes()); // ANCOUNT
    out.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    out.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    write_name(&mut out, &query.qname);
    out.extend_from_slice(&query.qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // IN
    for txt in txts {
        write_name(&mut out, &query.qname);
        out.extend_from_slice(&16u16.to_be_bytes()); // TXT
        out.extend_from_slice(&1u16.to_be_bytes()); // IN
        out.extend_from_slice(&60u32.to_be_bytes()); // TTL
        let data = txt.as_bytes();
        let rdlen = (1 + data.len().min(255)) as u16;
        out.extend_from_slice(&rdlen.to_be_bytes());
        let chunk = &data[..data.len().min(255)];
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
    out
}

/// Build a minimal DNS query (for DoH client / self-test).
pub fn build_query(id: u16, qname: &str, qtype: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&0x0100u16.to_be_bytes()); // RD
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    write_name(&mut out, qname);
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out
}

/// DoH JSON convenience body (lab).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DohJsonRequest {
    /// Full QNAME under aop.c2
    pub qname: String,
    #[serde(default = "default_txt")]
    pub qtype: String,
}

fn default_txt() -> String {
    "TXT".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DohJsonResponse {
    pub ok: bool,
    pub qname: String,
    pub txt: Vec<String>,
    #[serde(default)]
    pub payload_b32: String,
}

/// Encode DNS wire for DoH GET `?dns=` base64url parameter.
pub fn doh_get_param(wire: &[u8]) -> String {
    B64URL.encode(wire)
}

pub fn doh_decode_param(s: &str) -> Result<Vec<u8>> {
    B64URL
        .decode(s.trim())
        .map_err(|e| anyhow!("ANUBIS_DOH_B64: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b32_roundtrip() {
        let msg = b"{\"protocol\":\"aop-2\",\"blob\":\"abc\"}";
        let e = base32_encode(msg);
        let d = base32_decode(&e).unwrap();
        assert_eq!(d, msg);
    }

    #[test]
    fn qname_roundtrip_small() {
        let payload = br#"{"protocol":"aop-2","agent_id":"a1"}"#;
        let names = encode_qnames(DnsKind::Beacon, payload).unwrap();
        assert!(!names.is_empty());
        let mut frags = Vec::new();
        for n in &names {
            frags.push(decode_qname(n).unwrap());
        }
        let got = reassemble(&frags).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn dns_wire_txt_roundtrip_shape() {
        let q = build_query(0x1234, "0.1.b.x.aop.c2", 16);
        let parsed = parse_dns_query(&q).unwrap();
        assert_eq!(parsed.id, 0x1234);
        assert_eq!(parsed.qname, "0.1.b.x.aop.c2");
        let resp = build_txt_response(&parsed, &["HELLO".into()]);
        assert!(resp.len() > 12);
        assert_eq!(&resp[0..2], &0x1234u16.to_be_bytes());
    }
}
