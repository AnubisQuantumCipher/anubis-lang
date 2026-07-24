//! Engagement PSK crypto for AOP-2 (AES-256-GCM) + lab mTLS cert material.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

/// Derive a 32-byte key from engagement PSK material.
pub fn derive_key(psk_hex_or_raw: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    if let Ok(bytes) = hex::decode(psk_hex_or_raw.trim()) {
        if bytes.len() >= 32 {
            out.copy_from_slice(&bytes[..32]);
            return out;
        }
        let h = Sha256::digest(&bytes);
        out.copy_from_slice(&h);
        return out;
    }
    let h = Sha256::digest(psk_hex_or_raw.as_bytes());
    out.copy_from_slice(&h);
    out
}

pub fn generate_psk_hex() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Encrypt plaintext → base64(nonce||ciphertext).
pub fn seal(psk: &str, plaintext: &[u8]) -> Result<String> {
    let key = derive_key(psk);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("cipher: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow!("ANUBIS_CRYPTO_SEAL_FAILED"))?;
    let mut packed = Vec::with_capacity(12 + ct.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ct);
    Ok(B64.encode(packed))
}

/// Decrypt base64(nonce||ciphertext) → plaintext.
pub fn open(psk: &str, b64: &str) -> Result<Vec<u8>> {
    let key = derive_key(psk);
    let packed = B64
        .decode(b64.trim())
        .map_err(|e| anyhow!("ANUBIS_CRYPTO_B64: {e}"))?;
    if packed.len() < 13 {
        return Err(anyhow!("ANUBIS_CRYPTO_SHORT"));
    }
    let (nonce_bytes, ct) = packed.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("cipher: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| anyhow!("ANUBIS_CRYPTO_OPEN_FAILED"))
}

pub fn seal_json(psk: &str, value: &impl serde::Serialize) -> Result<String> {
    let raw = serde_json::to_vec(value)?;
    seal(psk, &raw)
}

pub fn open_json<T: serde::de::DeserializeOwned>(psk: &str, b64: &str) -> Result<T> {
    let raw = open(psk, b64)?;
    Ok(serde_json::from_slice(&raw)?)
}

/// SHA-256 hex of an operator API token (never store cleartext).
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Issue a fresh high-entropy operator token (returns cleartext once).
pub fn issue_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    // url-safe-ish hex is fine for lab headers
    hex::encode(buf)
}

/// Paths written by generate_lab_certs.
#[derive(Debug, Clone)]
pub struct LabCertPaths {
    pub ca_crt: std::path::PathBuf,
    pub ca_key: std::path::PathBuf,
    pub server_crt: std::path::PathBuf,
    pub server_key: std::path::PathBuf,
    pub client_crt: std::path::PathBuf,
    pub client_key: std::path::PathBuf,
    pub fingerprint_sha256: String,
}

/// Generate a lab CA + server + client cert chain for full mTLS.
/// Writes under `engage_dir/certs/`.
pub fn generate_lab_certs(engage_dir: &Path, cn: &str) -> Result<LabCertPaths> {
    use rcgen::{
        BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose, SanType,
    };

    let dir = engage_dir.join("certs");
    std::fs::create_dir_all(&dir)?;

    // --- CA ---
    let ca_key = KeyPair::generate()?;
    let mut ca_params = CertificateParams::new(vec![format!("{cn}-ca")])?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    ca_params
        .distinguished_name
        .push(DnType::CommonName, format!("{cn}-ca"));
    let ca_cert = ca_params.self_signed(&ca_key)?;
    let ca_crt_pem = ca_cert.pem();
    let ca_key_pem = ca_key.serialize_pem();

    // --- server ---
    let server_key = KeyPair::generate()?;
    let mut server_params = CertificateParams::new(vec![cn.to_string(), "localhost".into()])?;
    server_params
        .subject_alt_names
        .push(SanType::DnsName("localhost".try_into()?));
    server_params
        .subject_alt_names
        .push(SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::LOCALHOST,
        )));
    server_params
        .distinguished_name
        .push(DnType::CommonName, cn);
    server_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    server_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let server_cert = server_params.signed_by(&server_key, &ca_cert, &ca_key)?;
    let server_crt_pem = server_cert.pem();
    let server_key_pem = server_key.serialize_pem();

    // --- client ---
    let client_key = KeyPair::generate()?;
    let mut client_params = CertificateParams::new(vec![format!("{cn}-client")])?;
    client_params
        .distinguished_name
        .push(DnType::CommonName, format!("{cn}-client"));
    client_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    client_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];
    let client_cert = client_params.signed_by(&client_key, &ca_cert, &ca_key)?;
    let client_crt_pem = client_cert.pem();
    let client_key_pem = client_key.serialize_pem();

    let paths = LabCertPaths {
        ca_crt: dir.join("ca.crt.pem"),
        ca_key: dir.join("ca.key.pem"),
        server_crt: dir.join("server.crt.pem"),
        server_key: dir.join("server.key.pem"),
        client_crt: dir.join("client.crt.pem"),
        client_key: dir.join("client.key.pem"),
        fingerprint_sha256: hex::encode(Sha256::digest(server_crt_pem.as_bytes())),
    };

    std::fs::write(&paths.ca_crt, &ca_crt_pem)?;
    std::fs::write(&paths.ca_key, &ca_key_pem)?;
    std::fs::write(&paths.server_crt, &server_crt_pem)?;
    std::fs::write(&paths.server_key, &server_key_pem)?;
    std::fs::write(&paths.client_crt, &client_crt_pem)?;
    std::fs::write(&paths.client_key, &client_key_pem)?;
    std::fs::write(
        dir.join("fingerprint.sha256"),
        format!("{}\n", paths.fingerprint_sha256),
    )?;
    // Back-compat: older code only looked at server.crt.pem / server.key.pem
    Ok(paths)
}

/// Build a rustls ServerConfig that requires client certs signed by the lab CA (full mTLS).
pub fn mtls_server_config(engage_dir: &Path) -> Result<Arc<rustls::ServerConfig>> {
    let certs_dir = engage_dir.join("certs");
    let server_crt = std::fs::read(certs_dir.join("server.crt.pem"))
        .map_err(|e| anyhow!("ANUBIS_MTLS_SERVER_CRT: {e}"))?;
    let server_key = std::fs::read(certs_dir.join("server.key.pem"))
        .map_err(|e| anyhow!("ANUBIS_MTLS_SERVER_KEY: {e}"))?;
    let ca_crt = std::fs::read(certs_dir.join("ca.crt.pem"))
        .or_else(|_| std::fs::read(certs_dir.join("server.crt.pem")))
        .map_err(|e| anyhow!("ANUBIS_MTLS_CA_CRT: {e}"))?;

    let cert_chain = rustls_pemfile::certs(&mut server_crt.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("ANUBIS_MTLS_PARSE_SERVER_CRT: {e}"))?;
    if cert_chain.is_empty() {
        return Err(anyhow!("ANUBIS_MTLS_EMPTY_SERVER_CHAIN"));
    }
    let key = rustls_pemfile::private_key(&mut server_key.as_slice())
        .map_err(|e| anyhow!("ANUBIS_MTLS_PARSE_KEY: {e}"))?
        .ok_or_else(|| anyhow!("ANUBIS_MTLS_NO_PRIVATE_KEY"))?;

    let mut root = rustls::RootCertStore::empty();
    for c in rustls_pemfile::certs(&mut ca_crt.as_slice()) {
        let c = c.map_err(|e| anyhow!("ANUBIS_MTLS_PARSE_CA: {e}"))?;
        root.add(c)
            .map_err(|e| anyhow!("ANUBIS_MTLS_ADD_CA: {e}"))?;
    }
    if root.is_empty() {
        return Err(anyhow!("ANUBIS_MTLS_EMPTY_CA_STORE"));
    }

    let client_verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root))
        .build()
        .map_err(|e| anyhow!("ANUBIS_MTLS_CLIENT_VERIFIER: {e}"))?;

    let mut cfg = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(cert_chain, key)
        .map_err(|e| anyhow!("ANUBIS_MTLS_SERVER_CONFIG: {e}"))?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(cfg))
}

/// Build a rustls ClientConfig that presents the lab client cert (for mTLS self-tests / agents).
#[allow(dead_code)]
pub fn mtls_client_config(engage_dir: &Path) -> Result<Arc<rustls::ClientConfig>> {
    let certs_dir = engage_dir.join("certs");
    let client_crt = std::fs::read(certs_dir.join("client.crt.pem"))
        .map_err(|e| anyhow!("ANUBIS_MTLS_CLIENT_CRT: {e}"))?;
    let client_key = std::fs::read(certs_dir.join("client.key.pem"))
        .map_err(|e| anyhow!("ANUBIS_MTLS_CLIENT_KEY: {e}"))?;
    let ca_crt = std::fs::read(certs_dir.join("ca.crt.pem"))
        .or_else(|_| std::fs::read(certs_dir.join("server.crt.pem")))
        .map_err(|e| anyhow!("ANUBIS_MTLS_CA_CRT: {e}"))?;

    let mut root = rustls::RootCertStore::empty();
    for c in rustls_pemfile::certs(&mut ca_crt.as_slice()) {
        let c = c.map_err(|e| anyhow!("ANUBIS_MTLS_PARSE_CA: {e}"))?;
        root.add(c)
            .map_err(|e| anyhow!("ANUBIS_MTLS_ADD_CA: {e}"))?;
    }
    let cert_chain = rustls_pemfile::certs(&mut client_crt.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("ANUBIS_MTLS_PARSE_CLIENT_CRT: {e}"))?;
    let key = rustls_pemfile::private_key(&mut client_key.as_slice())
        .map_err(|e| anyhow!("ANUBIS_MTLS_PARSE_CLIENT_KEY: {e}"))?
        .ok_or_else(|| anyhow!("ANUBIS_MTLS_NO_CLIENT_KEY"))?;

    let cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root)
        .with_client_auth_cert(cert_chain, key)
        .map_err(|e| anyhow!("ANUBIS_MTLS_CLIENT_CONFIG: {e}"))?;
    Ok(Arc::new(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let psk = generate_psk_hex();
        let ct = seal(&psk, b"hello aop-2").unwrap();
        let pt = open(&psk, &ct).unwrap();
        assert_eq!(pt, b"hello aop-2");
    }

    #[test]
    fn token_hash_stable() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }
}
