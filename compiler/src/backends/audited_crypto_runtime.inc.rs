// ---- Cryptography via audited crates (RWC Ch16: don't roll your own) ----
// Native `anubis run` only. Crates: sha2, hmac, hkdf, chacha20poly1305, argon2,
// pbkdf2, getrandom, subtle, ed25519-dalek, x25519-dalek. Same AnubisValue surface
// as pure guest crypto for shared APIs; Ed25519 / X25519 / PHC are host-audited extras.
// Grounding: David Wong, Real-World Cryptography (Manning 2021).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

type HmacSha256 = Hmac<Sha256>;

fn anubis_hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Canonical crypto bytes. List elements MUST be integers in 0..=255 — fail closed on
/// truncation (silent `as u8` was a real key-corruption footgun).
fn anubis_crypto_bytes(v: &AnubisValue) -> Vec<u8> {
    match v {
        AnubisValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, x) in items.iter().enumerate() {
                let n = x.as_i64();
                if n < 0 || n > 255 {
                    panic!(
                        "ANUBIS_CRYPTO_BYTE_RANGE: list element [{}] = {} not in 0..=255 \
                         (refusing silent truncation of key/nonce/tag material)",
                        i, n
                    );
                }
                out.push(n as u8);
            }
            out
        }
        AnubisValue::Str(s) => s.as_bytes().to_vec(),
        AnubisValue::Int(n) => {
            // Single integer is NOT key material — force callers to use byte lists / strings.
            panic!(
                "ANUBIS_CRYPTO_BYTES_KIND: bare integer {} is not accepted as crypto input; \
                 use a byte list [0..255, ...] or a string",
                n
            );
        }
        AnubisValue::Bool(_) => {
            panic!("ANUBIS_CRYPTO_BYTES_KIND: bool is not accepted as crypto input");
        }
        other => other.display_string().into_bytes(),
    }
}

fn anubis_bytes_list(bytes: &[u8]) -> AnubisValue {
    anubis_mk_list(bytes.iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}

fn anubis_sha256_bytes(msg: Vec<u8>) -> [u8; 32] {
    let d = Sha256::digest(&msg);
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

fn anubis_sha256(v: AnubisValue) -> AnubisValue {
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(anubis_crypto_bytes(&v))))
}

/// Hex-encode arbitrary crypto bytes (for KATs / debugging — not a secret leak by itself).
fn anubis_bytes_hex(v: AnubisValue) -> AnubisValue {
    anubis_mk_str(anubis_hex_encode(&anubis_crypto_bytes(&v)))
}

fn anubis_sha256_bytes_val(v: AnubisValue) -> AnubisValue {
    anubis_bytes_list(&anubis_sha256_bytes(anubis_crypto_bytes(&v)))
}

fn anubis_hmac_sha256_raw(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // RFC 2104: any key length is valid. Fail closed if the library rejects the key —
    // never silently substitute a zero key (that would authenticate under a known key).
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).unwrap_or_else(|e| {
        panic!("ANUBIS_CRYPTO_HMAC_KEY: {}", e);
    });
    Mac::update(&mut mac, msg);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

fn anubis_hmac_sha256(key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let tag = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    anubis_mk_str(anubis_hex_encode(&tag))
}

fn anubis_hmac_sha256_bytes(key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let tag = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    anubis_bytes_list(&tag)
}

fn anubis_ct_eq(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let aa = anubis_crypto_bytes(&a);
    let bb = anubis_crypto_bytes(&b);
    if aa.len() != bb.len() {
        return AnubisValue::Bool(false);
    }
    AnubisValue::Bool(bool::from(aa.ct_eq(&bb)))
}

fn anubis_hmac_sha256_verify(key: AnubisValue, msg: AnubisValue, tag: AnubisValue) -> AnubisValue {
    let expected = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    let got = {
        let t = anubis_crypto_bytes(&tag);
        if t.len() == 32 {
            t
        } else {
            let s = tag.display_string();
            let mut out = Vec::new();
            let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
            if chars.len() == 64 && chars.iter().all(|c| c.is_ascii_hexdigit()) {
                let mut i = 0;
                while i < 64 {
                    let byte = u8::from_str_radix(&format!("{}{}", chars[i], chars[i + 1]), 16)
                        .unwrap_or(0);
                    out.push(byte);
                    i += 2;
                }
            }
            out
        }
    };
    if got.len() != 32 {
        return AnubisValue::Bool(false);
    }
    AnubisValue::Bool(bool::from(expected.ct_eq(got.as_slice())))
}

fn anubis_hkdf_sha256(
    ikm: AnubisValue,
    salt: AnubisValue,
    info: AnubisValue,
    length: AnubisValue,
) -> AnubisValue {
    use hkdf::Hkdf;
    let ikm_b = anubis_crypto_bytes(&ikm);
    let salt_b = anubis_crypto_bytes(&salt);
    let info_b = anubis_crypto_bytes(&info);
    let n = length.as_i64().max(0) as usize;
    if n == 0 {
        return anubis_mk_list(vec![]);
    }
    if n > 255 * 32 {
        panic!(
            "ANUBIS_CRYPTO_HKDF_TOO_LONG: requested {} bytes (max {})",
            n,
            255 * 32
        );
    }
    let salt_opt: Option<&[u8]> = if salt_b.is_empty() {
        None
    } else {
        Some(salt_b.as_slice())
    };
    let hk = Hkdf::<Sha256>::new(salt_opt, &ikm_b);
    let mut okm = vec![0u8; n];
    if hk.expand(&info_b, &mut okm).is_err() {
        panic!("ANUBIS_CRYPTO_HKDF_EXPAND_FAILED");
    }
    anubis_bytes_list(&okm)
}

fn anubis_domain_hash(label: AnubisValue, data: AnubisValue) -> AnubisValue {
    let lab = anubis_crypto_bytes(&label);
    let dat = anubis_crypto_bytes(&data);
    if lab.len() > u32::MAX as usize || dat.len() > u32::MAX as usize {
        panic!("ANUBIS_CRYPTO_DOMAIN_HASH_TOO_LARGE");
    }
    let mut msg = Vec::with_capacity(1 + 4 + lab.len() + 4 + dat.len());
    msg.push(0x01);
    msg.extend_from_slice(&(lab.len() as u32).to_be_bytes());
    msg.extend_from_slice(&lab);
    msg.extend_from_slice(&(dat.len() as u32).to_be_bytes());
    msg.extend_from_slice(&dat);
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(msg)))
}

/// TupleHash spirit (RWC Ch2): length-prefix each part so `H(a||b) ≠ H(ab)` ambiguity dies.
/// `parts` must be a list of strings or byte lists.
fn anubis_tuple_hash(label: AnubisValue, parts: AnubisValue) -> AnubisValue {
    let lab = anubis_crypto_bytes(&label);
    let AnubisValue::List(items) = parts else {
        panic!("ANUBIS_CRYPTO_TUPLE_HASH: parts must be a list");
    };
    if lab.len() > u32::MAX as usize || items.len() > u32::MAX as usize {
        panic!("ANUBIS_CRYPTO_TUPLE_HASH_TOO_LARGE");
    }
    let mut msg = Vec::new();
    msg.push(0x02); // domain version distinct from domain_hash
    msg.extend_from_slice(&(lab.len() as u32).to_be_bytes());
    msg.extend_from_slice(&lab);
    msg.extend_from_slice(&(items.len() as u32).to_be_bytes());
    for (i, p) in items.iter().enumerate() {
        let b = anubis_crypto_bytes(p);
        if b.len() > u32::MAX as usize {
            panic!("ANUBIS_CRYPTO_TUPLE_HASH_PART_TOO_LARGE: index {i}");
        }
        msg.extend_from_slice(&(b.len() as u32).to_be_bytes());
        msg.extend_from_slice(&b);
    }
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(msg)))
}

/// 12-byte nonce from a 64-bit counter (RWC Ch4: unique per key). Layout: 4 zero bytes + BE u64.
/// Suitable for moderate sequential protocols; never reuse a counter under the same key.
fn anubis_aead_nonce_from_counter(counter: AnubisValue) -> AnubisValue {
    let c = counter.as_i64();
    if c < 0 {
        panic!("ANUBIS_CRYPTO_NONCE_COUNTER: counter must be >= 0");
    }
    let mut n = [0u8; 12];
    n[4..12].copy_from_slice(&(c as u64).to_be_bytes());
    anubis_bytes_list(&n)
}

fn anubis_random_bytes(n: AnubisValue) -> AnubisValue {
    let n_raw = n.as_i64();
    if n_raw < 0 {
        panic!("ANUBIS_CRYPTO_RANDOM_NEGATIVE_LENGTH: byte count must be non-negative, got {}", n_raw);
    }
    let n = n_raw as usize;
    if n > 1 << 20 {
        panic!("ANUBIS_CRYPTO_RANDOM_TOO_LARGE: max 1MiB per call");
    }
    let mut buf = vec![0u8; n];
    if let Err(e) = getrandom::getrandom(&mut buf) {
        panic!("ANUBIS_CRYPTO_RANDOM_FAILED: {}", e);
    }
    anubis_bytes_list(&buf)
}

fn anubis_aead_parse_key_nonce(key: &AnubisValue, nonce: &AnubisValue) -> ([u8; 32], [u8; 12]) {
    let kb = anubis_crypto_bytes(key);
    let nb = anubis_crypto_bytes(nonce);
    if kb.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_AEAD_KEY_LEN: ChaCha20-Poly1305 key must be 32 bytes, got {}",
            kb.len()
        );
    }
    if nb.len() != 12 {
        panic!(
            "ANUBIS_CRYPTO_AEAD_NONCE_LEN: nonce must be 12 bytes (RWC: unique per key), got {}",
            nb.len()
        );
    }
    let mut k = [0u8; 32];
    let mut n = [0u8; 12];
    k.copy_from_slice(&kb);
    n.copy_from_slice(&nb);
    (k, n)
}

fn anubis_aead_seal(
    key: AnubisValue,
    nonce: AnubisValue,
    aad: AnubisValue,
    plaintext: AnubisValue,
) -> AnubisValue {
    let (k, n) = anubis_aead_parse_key_nonce(&key, &nonce);
    let aad_b = anubis_crypto_bytes(&aad);
    let pt = anubis_crypto_bytes(&plaintext);
    let cipher = ChaCha20Poly1305::new((&k).into());
    let nonce = Nonce::from_slice(&n);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &pt,
                aad: &aad_b,
            },
        )
        .unwrap_or_else(|_| panic!("ANUBIS_CRYPTO_AEAD_SEAL_FAILED"));
    anubis_bytes_list(&ct)
}

fn anubis_aead_open(
    key: AnubisValue,
    nonce: AnubisValue,
    aad: AnubisValue,
    ciphertext_and_tag: AnubisValue,
) -> AnubisValue {
    let (k, n) = anubis_aead_parse_key_nonce(&key, &nonce);
    let aad_b = anubis_crypto_bytes(&aad);
    let blob = anubis_crypto_bytes(&ciphertext_and_tag);
    if blob.len() < 16 {
        panic!("ANUBIS_CRYPTO_AEAD_OPEN_FAILED: ciphertext shorter than tag");
    }
    let cipher = ChaCha20Poly1305::new((&k).into());
    let nonce = Nonce::from_slice(&n);
    match cipher.decrypt(
        nonce,
        Payload {
            msg: &blob,
            aad: &aad_b,
        },
    ) {
        Ok(pt) => anubis_bytes_list(&pt),
        Err(_) => panic!("ANUBIS_CRYPTO_AEAD_OPEN_FAILED: authentication tag mismatch (fail closed)"),
    }
}

// ---- Password hashing: argon2 + pbkdf2 crates (RWC Ch8) ----

fn anubis_pbkdf2_hmac_sha256_raw(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    dk_len: usize,
) -> Vec<u8> {
    use pbkdf2::pbkdf2_hmac;
    if iterations < 1 {
        panic!("ANUBIS_CRYPTO_PBKDF2_ITERATIONS: must be >= 1");
    }
    if dk_len > 1024 * 1024 {
        panic!("ANUBIS_CRYPTO_PBKDF2_TOO_LONG: max 1MiB");
    }
    if salt.is_empty() {
        panic!("ANUBIS_CRYPTO_PBKDF2_SALT: salt must be non-empty (prefer >= 16 bytes)");
    }
    let mut okm = vec![0u8; dk_len];
    pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut okm);
    okm
}

fn anubis_pbkdf2_hmac_sha256(
    password: AnubisValue,
    salt: AnubisValue,
    iterations: AnubisValue,
    length: AnubisValue,
) -> AnubisValue {
    let iters = iterations.as_i64();
    if iters < 1 || iters > u32::MAX as i64 {
        panic!("ANUBIS_CRYPTO_PBKDF2_ITERATIONS: must be in 1..2^32-1");
    }
    let n = length.as_i64().max(0) as usize;
    let dk = anubis_pbkdf2_hmac_sha256_raw(
        &anubis_crypto_bytes(&password),
        &anubis_crypto_bytes(&salt),
        iters as u32,
        n,
    );
    anubis_bytes_list(&dk)
}

fn anubis_argon2id_raw(
    pwd: &[u8],
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    out_len: usize,
) -> Vec<u8> {
    use argon2::{Algorithm, Argon2, Params, Version};
    if salt.len() < 8 {
        panic!("ANUBIS_CRYPTO_ARGON2_SALT: salt must be >= 8 bytes (prefer 16)");
    }
    if out_len < 4 || out_len > 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_OUTLEN: must be 4..1024");
    }
    if m_cost < 8 || m_cost > 256 * 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_M: m_kib must be in 8..262144");
    }
    if t_cost < 1 || p_cost < 1 {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: t and p must be >= 1");
    }
    let params = Params::new(m_cost, t_cost, p_cost, Some(out_len)).unwrap_or_else(|e| {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: {}", e);
    });
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = vec![0u8; out_len];
    a2.hash_password_into(pwd, salt, &mut out)
        .unwrap_or_else(|e| panic!("ANUBIS_CRYPTO_ARGON2_FAILED: {}", e));
    out
}

fn anubis_argon2id_hash(
    password: AnubisValue,
    salt: AnubisValue,
    m_kib: AnubisValue,
    t: AnubisValue,
    p: AnubisValue,
    out_len: AnubisValue,
) -> AnubisValue {
    let m = m_kib.as_i64();
    let tt = t.as_i64();
    let pp = p.as_i64();
    let ol = out_len.as_i64();
    if m < 8 || m > 256 * 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_M: m_kib must be in 8..262144");
    }
    if tt < 1 || tt > 100 {
        panic!("ANUBIS_CRYPTO_ARGON2_T: time cost must be in 1..100");
    }
    if pp < 1 || pp > 16 {
        panic!("ANUBIS_CRYPTO_ARGON2_P: parallelism must be in 1..16");
    }
    if ol < 4 || ol > 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_OUTLEN: must be 4..1024");
    }
    let hash = anubis_argon2id_raw(
        &anubis_crypto_bytes(&password),
        &anubis_crypto_bytes(&salt),
        m as u32,
        tt as u32,
        pp as u32,
        ol as usize,
    );
    anubis_bytes_list(&hash)
}

fn anubis_hex_decode_loose(s: &str) -> Option<Vec<u8>> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() % 2 != 0 {
        return None;
    }
    if !chars.iter().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    let mut i = 0;
    while i < chars.len() {
        let byte = u8::from_str_radix(&format!("{}{}", chars[i], chars[i + 1]), 16).ok()?;
        out.push(byte);
        i += 2;
    }
    Some(out)
}

/// Production password hash via argon2 crate (Argon2id, OWASP-class params).
fn anubis_password_hash_encode(password: AnubisValue) -> AnubisValue {
    let salt_v = anubis_random_bytes(AnubisValue::Int(16));
    let salt = anubis_crypto_bytes(&salt_v);
    let hash = anubis_argon2id_raw(&anubis_crypto_bytes(&password), &salt, 19456, 2, 1, 32);
    let enc = format!(
        "anubis$argon2id$v=19$m=19456,t=2,p=1${}${}",
        anubis_hex_encode(&salt),
        anubis_hex_encode(&hash)
    );
    anubis_mk_str(enc)
}

fn anubis_password_hash_pbkdf2_encode(password: AnubisValue) -> AnubisValue {
    let salt_v = anubis_random_bytes(AnubisValue::Int(16));
    let salt = anubis_crypto_bytes(&salt_v);
    let hash = anubis_pbkdf2_hmac_sha256_raw(&anubis_crypto_bytes(&password), &salt, 600_000, 32);
    let enc = format!(
        "anubis$pbkdf2-sha256$i=600000${}${}",
        anubis_hex_encode(&salt),
        anubis_hex_encode(&hash)
    );
    anubis_mk_str(enc)
}

/// Standard PHC string (`$argon2id$v=19$m=…`) via the argon2 crate's PasswordHasher —
/// interoperable with other tools that speak PHC. Prefer this for long-lived password stores.
fn anubis_password_hash_phc(password: AnubisValue) -> AnubisValue {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Algorithm, Argon2, Params, Version,
    };
    let pwd = anubis_crypto_bytes(&password);
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(19456, 2, 1, None).unwrap_or_else(|e| {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: {}", e);
    });
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = a2
        .hash_password(&pwd, &salt)
        .unwrap_or_else(|e| panic!("ANUBIS_CRYPTO_PASSWORD_HASH_PHC: {}", e));
    anubis_mk_str(hash.to_string())
}

fn anubis_password_verify_phc_raw(password: &[u8], encoding: &str) -> bool {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    let Ok(parsed) = PasswordHash::new(encoding) else {
        return false;
    };
    Argon2::default()
        .verify_password(password, &parsed)
        .is_ok()
}

// Re-bind password_verify to also accept standard PHC strings (`$argon2id$…`).
fn anubis_password_verify_encoding(password: AnubisValue, encoding: AnubisValue) -> AnubisValue {
    let enc = encoding.display_string();
    let pwd = anubis_crypto_bytes(&password);
    // Standard PHC (argon2 crate / passlib / libsodium interop)
    if enc.starts_with("$argon2") {
        return AnubisValue::Bool(anubis_password_verify_phc_raw(&pwd, &enc));
    }
    // anubis$… custom encodings (argon2id / pbkdf2-sha256)
    let parts: Vec<&str> = enc.split('$').collect();
    if parts.len() < 5 || parts[0] != "anubis" {
        return AnubisValue::Bool(false);
    }
    let algo = parts[1];
    let salt = match anubis_hex_decode_loose(parts[parts.len() - 2]) {
        Some(s) if !s.is_empty() => s,
        _ => return AnubisValue::Bool(false),
    };
    let expected = match anubis_hex_decode_loose(parts[parts.len() - 1]) {
        Some(h) if !h.is_empty() => h,
        _ => return AnubisValue::Bool(false),
    };
    let got = if algo == "argon2id" {
        if parts.len() < 6 {
            return AnubisValue::Bool(false);
        }
        let mut m: Option<u32> = None;
        let mut t: Option<u32> = None;
        let mut p: Option<u32> = None;
        for kv in parts[3].split(',') {
            if let Some(v) = kv.strip_prefix("m=") {
                m = v.parse().ok();
            } else if let Some(v) = kv.strip_prefix("t=") {
                t = v.parse().ok();
            } else if let Some(v) = kv.strip_prefix("p=") {
                p = v.parse().ok();
            }
        }
        let (Some(m), Some(t), Some(p)) = (m, t, p) else {
            return AnubisValue::Bool(false);
        };
        anubis_argon2id_raw(&pwd, &salt, m, t, p, expected.len())
    } else if algo == "pbkdf2-sha256" {
        let iters: u32 = match parts[2].strip_prefix("i=").and_then(|v| v.parse().ok()) {
            Some(i) if i >= 1 => i,
            _ => return AnubisValue::Bool(false),
        };
        anubis_pbkdf2_hmac_sha256_raw(&pwd, &salt, iters, expected.len())
    } else {
        return AnubisValue::Bool(false);
    };
    if got.len() != expected.len() {
        return AnubisValue::Bool(false);
    }
    AnubisValue::Bool(bool::from(got.ct_eq(expected.as_slice())))
}

// ---- Ed25519 (RWC / modern signatures — audited ed25519-dalek) ----

fn anubis_ed25519_keygen() -> AnubisValue {
    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        panic!("ANUBIS_CRYPTO_ED25519_RNG: {}", e);
    }
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key();
    // Return [secret_key_32, public_key_32] as nested byte lists
    anubis_mk_list(vec![
        anubis_bytes_list(sk.to_bytes().as_slice()),
        anubis_bytes_list(pk.as_bytes()),
    ])
}

fn anubis_ed25519_public_key(secret_key: AnubisValue) -> AnubisValue {
    let sk_b = anubis_crypto_bytes(&secret_key);
    if sk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_ED25519_SK_LEN: secret key must be 32 bytes, got {}",
            sk_b.len()
        );
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&sk_b);
    let sk = SigningKey::from_bytes(&seed);
    anubis_bytes_list(sk.verifying_key().as_bytes())
}

fn anubis_ed25519_sign(secret_key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let sk_b = anubis_crypto_bytes(&secret_key);
    if sk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_ED25519_SK_LEN: secret key must be 32 bytes, got {}",
            sk_b.len()
        );
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&sk_b);
    let sk = SigningKey::from_bytes(&seed);
    let sig = sk.sign(&anubis_crypto_bytes(&msg));
    anubis_bytes_list(sig.to_bytes().as_slice())
}

fn anubis_ed25519_verify(public_key: AnubisValue, msg: AnubisValue, signature: AnubisValue) -> AnubisValue {
    let pk_b = anubis_crypto_bytes(&public_key);
    let sig_b = anubis_crypto_bytes(&signature);
    if pk_b.len() != 32 {
        return AnubisValue::Bool(false);
    }
    if sig_b.len() != 64 {
        return AnubisValue::Bool(false);
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_b);
    let Ok(pk) = VerifyingKey::from_bytes(&pk_arr) else {
        return AnubisValue::Bool(false);
    };
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_b);
    let sig = Signature::from_bytes(&sig_arr);
    AnubisValue::Bool(pk.verify(&anubis_crypto_bytes(&msg), &sig).is_ok())
}

/// Host runtime identity — useful for tests asserting audited path is live.
fn anubis_crypto_backend() -> AnubisValue {
    anubis_mk_str("audited-crates".into())
}

// ---- X25519 ECDH (RWC Ch5) + hybrid envelope (RWC Ch6 ECIES spirit) ----

fn anubis_x25519_from_sk_bytes(sk_b: &[u8]) -> StaticSecret {
    if sk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_X25519_SK_LEN: secret key must be 32 bytes, got {}",
            sk_b.len()
        );
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(sk_b);
    StaticSecret::from(seed)
}

fn anubis_x25519_from_pk_bytes(pk_b: &[u8]) -> X25519Public {
    if pk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_X25519_PK_LEN: public key must be 32 bytes, got {}",
            pk_b.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(pk_b);
    X25519Public::from(arr)
}

fn anubis_x25519_keygen() -> AnubisValue {
    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        panic!("ANUBIS_CRYPTO_X25519_RNG: {}", e);
    }
    let sk = StaticSecret::from(seed);
    let pk = X25519Public::from(&sk);
    anubis_mk_list(vec![
        anubis_bytes_list(sk.to_bytes().as_slice()),
        anubis_bytes_list(pk.as_bytes()),
    ])
}

fn anubis_x25519_public_key(secret_key: AnubisValue) -> AnubisValue {
    let sk = anubis_x25519_from_sk_bytes(&anubis_crypto_bytes(&secret_key));
    let pk = X25519Public::from(&sk);
    anubis_bytes_list(pk.as_bytes())
}

/// Raw Diffie–Hellman shared secret. RWC: never use raw shared as AEAD key — HKDF first.
fn anubis_x25519_shared(secret_key: AnubisValue, peer_public: AnubisValue) -> AnubisValue {
    let sk = anubis_x25519_from_sk_bytes(&anubis_crypto_bytes(&secret_key));
    let pk = anubis_x25519_from_pk_bytes(&anubis_crypto_bytes(&peer_public));
    let shared = sk.diffie_hellman(&pk);
    anubis_bytes_list(shared.as_bytes())
}

fn anubis_hybrid_derive_key(shared: &[u8], eph_pk: &[u8], recip_pk: &[u8]) -> [u8; 32] {
    use hkdf::Hkdf;
    // IKM = shared; salt = eph_pk || recip_pk (binds both static identities into the transcript).
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(eph_pk);
    salt.extend_from_slice(recip_pk);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut okm = [0u8; 32];
    if hk
        .expand(b"anubis-hybrid-v1|chacha20-poly1305", &mut okm)
        .is_err()
    {
        panic!("ANUBIS_CRYPTO_HYBRID_HKDF_FAILED");
    }
    okm
}

/// Hybrid seal (ECIES spirit, RWC Ch6): ephemeral X25519 + HKDF + ChaCha20-Poly1305.
/// Returns [eph_public_32, nonce_12, ciphertext_and_tag].
fn anubis_hybrid_seal(
    recipient_public: AnubisValue,
    aad: AnubisValue,
    plaintext: AnubisValue,
) -> AnubisValue {
    let recip_pk_b = anubis_crypto_bytes(&recipient_public);
    let recip_pk = anubis_x25519_from_pk_bytes(&recip_pk_b);
    let mut eph_seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut eph_seed) {
        panic!("ANUBIS_CRYPTO_HYBRID_RNG: {}", e);
    }
    let eph_sk = StaticSecret::from(eph_seed);
    let eph_pk = X25519Public::from(&eph_sk);
    let shared = eph_sk.diffie_hellman(&recip_pk);
    let key = anubis_hybrid_derive_key(shared.as_bytes(), eph_pk.as_bytes(), recip_pk.as_bytes());
    let mut nonce = [0u8; 12];
    if let Err(e) = getrandom::getrandom(&mut nonce) {
        panic!("ANUBIS_CRYPTO_HYBRID_NONCE_RNG: {}", e);
    }
    let cipher = ChaCha20Poly1305::new((&key).into());
    let aad_b = anubis_crypto_bytes(&aad);
    let pt = anubis_crypto_bytes(&plaintext);
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &pt,
                aad: &aad_b,
            },
        )
        .unwrap_or_else(|_| panic!("ANUBIS_CRYPTO_HYBRID_SEAL_FAILED"));
    anubis_mk_list(vec![
        anubis_bytes_list(eph_pk.as_bytes()),
        anubis_bytes_list(&nonce),
        anubis_bytes_list(&ct),
    ])
}

/// Hybrid open: recipient static secret + envelope fields from hybrid_seal.
fn anubis_hybrid_open(
    recipient_secret: AnubisValue,
    eph_public: AnubisValue,
    aad: AnubisValue,
    nonce: AnubisValue,
    ciphertext_and_tag: AnubisValue,
) -> AnubisValue {
    let recip_sk = anubis_x25519_from_sk_bytes(&anubis_crypto_bytes(&recipient_secret));
    let recip_pk = X25519Public::from(&recip_sk);
    let eph_pk_b = anubis_crypto_bytes(&eph_public);
    let eph_pk = anubis_x25519_from_pk_bytes(&eph_pk_b);
    let shared = recip_sk.diffie_hellman(&eph_pk);
    let key = anubis_hybrid_derive_key(shared.as_bytes(), eph_pk.as_bytes(), recip_pk.as_bytes());
    let (k, n) = {
        let nb = anubis_crypto_bytes(&nonce);
        if nb.len() != 12 {
            panic!(
                "ANUBIS_CRYPTO_HYBRID_NONCE_LEN: expected 12 bytes, got {}",
                nb.len()
            );
        }
        let mut nn = [0u8; 12];
        nn.copy_from_slice(&nb);
        (key, nn)
    };
    let cipher = ChaCha20Poly1305::new((&k).into());
    let aad_b = anubis_crypto_bytes(&aad);
    let blob = anubis_crypto_bytes(&ciphertext_and_tag);
    if blob.len() < 16 {
        panic!("ANUBIS_CRYPTO_HYBRID_OPEN_FAILED: ciphertext shorter than tag");
    }
    match cipher.decrypt(
        Nonce::from_slice(&n),
        Payload {
            msg: &blob,
            aad: &aad_b,
        },
    ) {
        Ok(pt) => anubis_bytes_list(&pt),
        Err(_) => panic!(
            "ANUBIS_CRYPTO_HYBRID_OPEN_FAILED: authentication tag mismatch (fail closed)"
        ),
    }
}
