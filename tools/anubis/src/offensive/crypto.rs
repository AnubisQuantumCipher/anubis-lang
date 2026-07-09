//! Engagement PSK crypto for AOP-2 (AES-256-GCM).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};

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

/// Generate a self-signed lab cert pair (PEM) for mTLS-ready engagements.
pub fn generate_lab_certs(engage_dir: &std::path::Path, cn: &str) -> Result<(String, String)> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
    let mut params = CertificateParams::new(vec![cn.to_string()])?;
    params
        .subject_alt_names
        .push(SanType::DnsName("localhost".try_into()?));
    params
        .subject_alt_names
        .push(SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::LOCALHOST,
        )));
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    params.distinguished_name = dn;
    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    let dir = engage_dir.join("certs");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("server.crt.pem"), &cert_pem)?;
    std::fs::write(dir.join("server.key.pem"), &key_pem)?;
    let fp = hex::encode(Sha256::digest(cert_pem.as_bytes()));
    std::fs::write(dir.join("fingerprint.sha256"), format!("{fp}\n"))?;
    Ok((cert_pem, key_pem))
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
}
