// ---- Cryptography (pure std — no external crates; FIPS-aligned SHA-256) ----
// Used by real evidence ledgers. Domain: UTF-8 string bytes of display form.
fn anubis_hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// SHA-256 (FIPS 180-4). Pure Rust so `rustc` of emitted programs needs no cargo deps.
fn anubis_sha256_bytes(mut msg: Vec<u8>) -> [u8; 32] {
    let bit_len = (msg.len() as u64).saturating_mul(8);
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Canonical byte extraction for crypto: lists of 0..255 ints, or UTF-8 of strings/display.
/// RWC Ch2/Ch3: crypto APIs must not silently re-encode secrets through display formatting.
fn anubis_crypto_bytes(v: &AnubisValue) -> Vec<u8> {
    match v {
        AnubisValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, x) in items.iter().enumerate() {
                let n = x.as_i64();
                if n < 0 || n > 255 {
                    panic!(
                        "ANUBIS_CRYPTO_BYTE_RANGE: list element [{}] = {} not in 0..=255",
                        i, n
                    );
                }
                out.push(n as u8);
            }
            out
        }
        AnubisValue::Str(s) => s.as_bytes().to_vec(),
        AnubisValue::Int(n) => {
            panic!(
                "ANUBIS_CRYPTO_BYTES_KIND: bare integer {} is not accepted as crypto input",
                n
            );
        }
        AnubisValue::Bool(_) => {
            panic!("ANUBIS_CRYPTO_BYTES_KIND: bool is not accepted as crypto input");
        }
        other => other.display_string().into_bytes(),
    }
}

fn anubis_crypto_backend() -> AnubisValue {
    anubis_mk_str("pure-guest".into())
}

fn anubis_ed25519_keygen() -> AnubisValue {
    panic!("ANUBIS_CRYPTO_ED25519_HOST_ONLY: Ed25519 requires audited ed25519-dalek (native run/build), not the zkVM pure guest");
}
fn anubis_ed25519_public_key(_sk: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_ED25519_HOST_ONLY");
}
fn anubis_ed25519_sign(_sk: AnubisValue, _msg: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_ED25519_HOST_ONLY");
}
fn anubis_ed25519_verify(_pk: AnubisValue, _msg: AnubisValue, _sig: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_ED25519_HOST_ONLY");
}
fn anubis_password_hash_phc(_password: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_PHC_HOST_ONLY: PHC password hashing uses argon2 crate (native run/build)");
}
fn anubis_x25519_keygen() -> AnubisValue {
    panic!("ANUBIS_CRYPTO_X25519_HOST_ONLY: X25519 requires audited x25519-dalek (native run/build)");
}
fn anubis_x25519_public_key(_sk: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_X25519_HOST_ONLY");
}
fn anubis_x25519_shared(_sk: AnubisValue, _pk: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_X25519_HOST_ONLY");
}
fn anubis_hybrid_seal(_pk: AnubisValue, _aad: AnubisValue, _pt: AnubisValue) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_HYBRID_HOST_ONLY: hybrid envelope requires X25519+AEAD audited crates");
}
fn anubis_hybrid_open(
    _sk: AnubisValue,
    _eph: AnubisValue,
    _aad: AnubisValue,
    _n: AnubisValue,
    _ct: AnubisValue,
) -> AnubisValue {
    panic!("ANUBIS_CRYPTO_HYBRID_HOST_ONLY");
}
fn anubis_tuple_hash(label: AnubisValue, parts: AnubisValue) -> AnubisValue {
    // Length-prefixed multi-part hash works on pure path (SHA-256 only).
    let lab = anubis_crypto_bytes(&label);
    let AnubisValue::List(items) = parts else {
        panic!("ANUBIS_CRYPTO_TUPLE_HASH: parts must be a list");
    };
    let mut msg = Vec::new();
    msg.push(0x02);
    msg.extend_from_slice(&(lab.len() as u32).to_be_bytes());
    msg.extend_from_slice(&lab);
    msg.extend_from_slice(&(items.len() as u32).to_be_bytes());
    for p in &items {
        let b = anubis_crypto_bytes(p);
        msg.extend_from_slice(&(b.len() as u32).to_be_bytes());
        msg.extend_from_slice(&b);
    }
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(msg)))
}
fn anubis_aead_nonce_from_counter(counter: AnubisValue) -> AnubisValue {
    let c = counter.as_i64();
    if c < 0 {
        panic!("ANUBIS_CRYPTO_NONCE_COUNTER: counter must be >= 0");
    }
    let mut n = [0u8; 12];
    n[4..12].copy_from_slice(&(c as u64).to_be_bytes());
    anubis_bytes_list(&n)
}

fn anubis_bytes_list(bytes: &[u8]) -> AnubisValue {
    anubis_mk_list(bytes.iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}

fn anubis_sha256(v: AnubisValue) -> AnubisValue {
    let bytes = anubis_crypto_bytes(&v);
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(bytes)))
}

fn anubis_bytes_hex(v: AnubisValue) -> AnubisValue {
    anubis_mk_str(anubis_hex_encode(&anubis_crypto_bytes(&v)))
}

fn anubis_sha256_bytes_val(v: AnubisValue) -> AnubisValue {
    anubis_bytes_list(&anubis_sha256_bytes(anubis_crypto_bytes(&v)))
}

fn anubis_hmac_sha256_raw(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // RFC 2104 HMAC-SHA256 — do NOT use raw SHA-256 as a MAC (RWC Ch3.6 length-extension).
    let mut k = key.to_vec();
    if k.len() > 64 {
        k = anubis_sha256_bytes(k).to_vec();
    }
    if k.len() < 64 {
        k.resize(64, 0);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = ipad.to_vec();
    inner.extend_from_slice(msg);
    let inner_hash = anubis_sha256_bytes(inner);
    let mut outer = opad.to_vec();
    outer.extend_from_slice(&inner_hash);
    anubis_sha256_bytes(outer)
}

fn anubis_hmac_sha256(key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let tag = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    anubis_mk_str(anubis_hex_encode(&tag))
}

fn anubis_hmac_sha256_bytes(key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let tag = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    anubis_bytes_list(&tag)
}

/// Constant-time equality (RWC Ch3.3.4). Length mismatch → false without leaking which byte
/// differed; same-length compare is data-independent.
fn anubis_ct_eq(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let aa = anubis_crypto_bytes(&a);
    let bb = anubis_crypto_bytes(&b);
    if aa.len() != bb.len() {
        return AnubisValue::Bool(false);
    }
    let mut v = 0u8;
    for i in 0..aa.len() {
        v |= aa[i] ^ bb[i];
    }
    AnubisValue::Bool(v == 0)
}

/// RWC Ch3: MAC verification MUST be constant-time — never `hmac(...) == tag` with early exit.
fn anubis_hmac_sha256_verify(key: AnubisValue, msg: AnubisValue, tag: AnubisValue) -> AnubisValue {
    let expected = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    let got = {
        let t = anubis_crypto_bytes(&tag);
        if t.len() == 32 {
            t
        } else {
            // Accept hex string tags (legacy API surface).
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
    let mut v = 0u8;
    for i in 0..32 {
        v |= expected[i] ^ got[i];
    }
    AnubisValue::Bool(v == 0)
}

/// RFC 5869 HKDF-SHA256 (RWC Ch3/Ch8 key derivation — prefer HKDF over ad-hoc hash concat).
fn anubis_hkdf_sha256(
    ikm: AnubisValue,
    salt: AnubisValue,
    info: AnubisValue,
    length: AnubisValue,
) -> AnubisValue {
    let ikm_b = anubis_crypto_bytes(&ikm);
    let mut salt_b = anubis_crypto_bytes(&salt);
    if salt_b.is_empty() {
        salt_b = vec![0u8; 32];
    }
    let info_b = anubis_crypto_bytes(&info);
    // RFC 5869 §2.3: L ∈ [1, 255*HashLen]. Prior code silently coerced negative L to 0 via
    // `.max(0)` and returned an empty byte list — a SILENT_WRONG that would feed a downstream
    // `ensures(len(key) == 32)` and let a contract hold "for the wrong reason" (the caller
    // sees an empty vec and never checks). Fail closed on non-positive length, matching
    // `anubis_random_bytes`'s posture. NEGATIVE inputs are honestly reported so a signed
    // overflow at the call site is caught rather than laundered.
    let n_raw = length.as_i64();
    if n_raw < 1 {
        panic!("ANUBIS_CRYPTO_HKDF_LENGTH: L must be >= 1 (RFC 5869), got {}", n_raw);
    }
    let n = n_raw as usize;
    if n > 255 * 32 {
        panic!("ANUBIS_CRYPTO_HKDF_TOO_LONG: requested {} bytes (max {})", n, 255 * 32);
    }
    let prk = anubis_hmac_sha256_raw(&salt_b, &ikm_b);
    let mut okm = Vec::with_capacity(n);
    let mut t: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;
    while okm.len() < n {
        let mut block = Vec::new();
        block.extend_from_slice(&t);
        block.extend_from_slice(&info_b);
        block.push(counter);
        t = anubis_hmac_sha256_raw(&prk, &block).to_vec();
        okm.extend_from_slice(&t);
        counter = counter.wrapping_add(1);
        if counter == 0 {
            panic!("ANUBIS_CRYPTO_HKDF_OVERFLOW");
        }
    }
    okm.truncate(n);
    anubis_bytes_list(&okm)
}

/// Domain-separated hash (RWC Ch2.5 TupleHash idea): length-prefix label then data.
/// Prevents ambiguous concatenation attacks when hashing multi-part input.
fn anubis_domain_hash(label: AnubisValue, data: AnubisValue) -> AnubisValue {
    let lab = anubis_crypto_bytes(&label);
    let dat = anubis_crypto_bytes(&data);
    if lab.len() > u32::MAX as usize || dat.len() > u32::MAX as usize {
        panic!("ANUBIS_CRYPTO_DOMAIN_HASH_TOO_LARGE");
    }
    let mut msg = Vec::with_capacity(1 + 4 + lab.len() + 4 + dat.len());
    msg.push(0x01); // version/domain tag
    msg.extend_from_slice(&(lab.len() as u32).to_be_bytes());
    msg.extend_from_slice(&lab);
    msg.extend_from_slice(&(dat.len() as u32).to_be_bytes());
    msg.extend_from_slice(&dat);
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(msg)))
}

/// CSPRNG (RWC Ch8): /dev/urandom — never SystemTime-seeded PRNG for secrets.
fn anubis_random_bytes(n: AnubisValue) -> AnubisValue {
    use std::io::Read;
    let n_raw = n.as_i64();
    if n_raw < 0 {
        panic!("ANUBIS_CRYPTO_RANDOM_NEGATIVE_LENGTH: byte count must be non-negative, got {}", n_raw);
    }
    let n = n_raw as usize;
    if n > 1 << 20 {
        panic!("ANUBIS_CRYPTO_RANDOM_TOO_LARGE: max 1MiB per call");
    }
    let mut buf = vec![0u8; n];
    match std::fs::File::open("/dev/urandom") {
        Ok(mut f) => {
            if let Err(e) = f.read_exact(&mut buf) {
                panic!("ANUBIS_CRYPTO_RANDOM_FAILED: {}", e);
            }
        }
        Err(e) => panic!("ANUBIS_CRYPTO_RANDOM_UNAVAILABLE: {}", e),
    }
    anubis_bytes_list(&buf)
}

// ---- ChaCha20-Poly1305 AEAD (RFC 8439) — pure std, boring default (RWC Ch4) ----
// Nonce is 12 bytes; key is 32 bytes. Tag is 16 bytes appended to ciphertext.
// FAIL-CLOSED on wrong key/nonce/tag sizes. Do not invent EtM; use this construction.

fn anubis_chacha20_quarter(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

fn anubis_chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[0] = 0x6170_7865;
    state[1] = 0x3320_646e;
    state[2] = 0x7962_2d32;
    state[3] = 0x6b20_6574;
    for i in 0..8 {
        state[4 + i] = u32::from_le_bytes([
            key[i * 4],
            key[i * 4 + 1],
            key[i * 4 + 2],
            key[i * 4 + 3],
        ]);
    }
    state[12] = counter;
    for i in 0..3 {
        state[13 + i] = u32::from_le_bytes([
            nonce[i * 4],
            nonce[i * 4 + 1],
            nonce[i * 4 + 2],
            nonce[i * 4 + 3],
        ]);
    }
    let mut working = state;
    for _ in 0..10 {
        anubis_chacha20_quarter(&mut working, 0, 4, 8, 12);
        anubis_chacha20_quarter(&mut working, 1, 5, 9, 13);
        anubis_chacha20_quarter(&mut working, 2, 6, 10, 14);
        anubis_chacha20_quarter(&mut working, 3, 7, 11, 15);
        anubis_chacha20_quarter(&mut working, 0, 5, 10, 15);
        anubis_chacha20_quarter(&mut working, 1, 6, 11, 12);
        anubis_chacha20_quarter(&mut working, 2, 7, 8, 13);
        anubis_chacha20_quarter(&mut working, 3, 4, 9, 14);
    }
    for i in 0..16 {
        working[i] = working[i].wrapping_add(state[i]);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[i * 4..(i + 1) * 4].copy_from_slice(&working[i].to_le_bytes());
    }
    out
}

fn anubis_chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    let mut ctr = counter;
    let mut off = 0;
    while off < out.len() {
        let block = anubis_chacha20_block(key, ctr, nonce);
        let n = (out.len() - off).min(64);
        for i in 0..n {
            out[off + i] ^= block[i];
        }
        off += n;
        ctr = ctr.wrapping_add(1);
    }
    out
}

/// Poly1305 (RFC 8439) with 5×26-bit limbs — full 130-bit field (u128 alone is insufficient).
fn anubis_poly1305_mac(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    fn load26(b: &[u8], i: usize, shift: u32) -> u64 {
        let mut v = b[i] as u64 | ((b[i + 1] as u64) << 8) | ((b[i + 2] as u64) << 16) | ((b[i + 3] as u64) << 24);
        v >>= shift;
        v & 0x3ff_ffff
    }
    let mut t = [0u8; 16];
    t.copy_from_slice(&key[0..16]);
    t[3] &= 15;
    t[7] &= 15;
    t[11] &= 15;
    t[15] &= 15;
    t[4] &= 252;
    t[8] &= 252;
    t[12] &= 252;
    let r0 = load26(&t, 0, 0);
    let r1 = load26(&t, 3, 2);
    let r2 = load26(&t, 6, 4);
    let r3 = load26(&t, 9, 6);
    let r4 = (t[12] as u64 | ((t[13] as u64) << 8) | ((t[14] as u64) << 16) | ((t[15] as u64) << 24)) >> 8;
    let r4 = r4 & 0x3ff_ffff;
    let s1 = r1.wrapping_mul(5);
    let s2 = r2.wrapping_mul(5);
    let s3 = r3.wrapping_mul(5);
    let s4 = r4.wrapping_mul(5);
    let mut h0 = 0u64;
    let mut h1 = 0u64;
    let mut h2 = 0u64;
    let mut h3 = 0u64;
    let mut h4 = 0u64;
    let mut offset = 0;
    while offset < msg.len() {
        let end = (offset + 16).min(msg.len());
        let mut block = [0u8; 17];
        let n = end - offset;
        block[..n].copy_from_slice(&msg[offset..end]);
        block[n] = 1;
        let t0 = block[0] as u64
            | ((block[1] as u64) << 8)
            | ((block[2] as u64) << 16)
            | ((block[3] as u64) << 24);
        let t1 = block[4] as u64
            | ((block[5] as u64) << 8)
            | ((block[6] as u64) << 16)
            | ((block[7] as u64) << 24);
        let t2 = block[8] as u64
            | ((block[9] as u64) << 8)
            | ((block[10] as u64) << 16)
            | ((block[11] as u64) << 24);
        let t3 = block[12] as u64
            | ((block[13] as u64) << 8)
            | ((block[14] as u64) << 16)
            | ((block[15] as u64) << 24);
        let t4 = block[16] as u64;
        h0 = h0.wrapping_add(t0 & 0x3ff_ffff);
        h1 = h1.wrapping_add(((t0 >> 26) | (t1 << 6)) & 0x3ff_ffff);
        h2 = h2.wrapping_add(((t1 >> 20) | (t2 << 12)) & 0x3ff_ffff);
        h3 = h3.wrapping_add(((t2 >> 14) | (t3 << 18)) & 0x3ff_ffff);
        h4 = h4.wrapping_add((t3 >> 8) | (t4 << 24));
        // h *= r
        let mut d0 = (h0 as u128) * (r0 as u128)
            + (h1 as u128) * (s4 as u128)
            + (h2 as u128) * (s3 as u128)
            + (h3 as u128) * (s2 as u128)
            + (h4 as u128) * (s1 as u128);
        let mut d1 = (h0 as u128) * (r1 as u128)
            + (h1 as u128) * (r0 as u128)
            + (h2 as u128) * (s4 as u128)
            + (h3 as u128) * (s3 as u128)
            + (h4 as u128) * (s2 as u128);
        let mut d2 = (h0 as u128) * (r2 as u128)
            + (h1 as u128) * (r1 as u128)
            + (h2 as u128) * (r0 as u128)
            + (h3 as u128) * (s4 as u128)
            + (h4 as u128) * (s3 as u128);
        let mut d3 = (h0 as u128) * (r3 as u128)
            + (h1 as u128) * (r2 as u128)
            + (h2 as u128) * (r1 as u128)
            + (h3 as u128) * (r0 as u128)
            + (h4 as u128) * (s4 as u128);
        let mut d4 = (h0 as u128) * (r4 as u128)
            + (h1 as u128) * (r3 as u128)
            + (h2 as u128) * (r2 as u128)
            + (h3 as u128) * (r1 as u128)
            + (h4 as u128) * (r0 as u128);
        // partial reduction
        let mut c = d0 >> 26;
        h0 = (d0 as u64) & 0x3ff_ffff;
        d1 += c;
        c = d1 >> 26;
        h1 = (d1 as u64) & 0x3ff_ffff;
        d2 += c;
        c = d2 >> 26;
        h2 = (d2 as u64) & 0x3ff_ffff;
        d3 += c;
        c = d3 >> 26;
        h3 = (d3 as u64) & 0x3ff_ffff;
        d4 += c;
        c = d4 >> 26;
        h4 = (d4 as u64) & 0x3ff_ffff;
        h0 = h0.wrapping_add((c as u64).wrapping_mul(5));
        c = (h0 >> 26) as u128;
        h0 &= 0x3ff_ffff;
        h1 = h1.wrapping_add(c as u64);
        offset += 16;
    }
    // final reduction
    let mut c = h1 >> 26;
    h1 &= 0x3ff_ffff;
    h2 = h2.wrapping_add(c);
    c = h2 >> 26;
    h2 &= 0x3ff_ffff;
    h3 = h3.wrapping_add(c);
    c = h3 >> 26;
    h3 &= 0x3ff_ffff;
    h4 = h4.wrapping_add(c);
    c = h4 >> 26;
    h4 &= 0x3ff_ffff;
    h0 = h0.wrapping_add(c.wrapping_mul(5));
    c = h0 >> 26;
    h0 &= 0x3ff_ffff;
    h1 = h1.wrapping_add(c);
    // h + -p
    let mut g0 = h0.wrapping_add(5);
    c = g0 >> 26;
    g0 &= 0x3ff_ffff;
    let mut g1 = h1.wrapping_add(c);
    c = g1 >> 26;
    g1 &= 0x3ff_ffff;
    let mut g2 = h2.wrapping_add(c);
    c = g2 >> 26;
    g2 &= 0x3ff_ffff;
    let mut g3 = h3.wrapping_add(c);
    c = g3 >> 26;
    g3 &= 0x3ff_ffff;
    let g4 = h4.wrapping_add(c).wrapping_sub(1 << 26);
    let mut mask = (g4 >> 63).wrapping_sub(1);
    g0 &= mask;
    g1 &= mask;
    g2 &= mask;
    g3 &= mask;
    // mask for h if g underflows
    mask = !mask;
    h0 = (h0 & mask) | g0;
    h1 = (h1 & mask) | g1;
    h2 = (h2 & mask) | g2;
    h3 = (h3 & mask) | g3;
    h4 = (h4 & mask) | (g4 & !mask);
    // pack + s
    let mut f0 = h0 | (h1 << 26);
    let mut f1 = (h1 >> 6) | (h2 << 20);
    let mut f2 = (h2 >> 12) | (h3 << 14);
    let mut f3 = (h3 >> 18) | (h4 << 8);
    let s0 = u32::from_le_bytes([key[16], key[17], key[18], key[19]]) as u64;
    let s1 = u32::from_le_bytes([key[20], key[21], key[22], key[23]]) as u64;
    let s2 = u32::from_le_bytes([key[24], key[25], key[26], key[27]]) as u64;
    let s3 = u32::from_le_bytes([key[28], key[29], key[30], key[31]]) as u64;
    f0 = f0.wrapping_add(s0);
    f1 = f1.wrapping_add(s1).wrapping_add(f0 >> 32);
    f2 = f2.wrapping_add(s2).wrapping_add(f1 >> 32);
    f3 = f3.wrapping_add(s3).wrapping_add(f2 >> 32);
    let mut tag = [0u8; 16];
    tag[0..4].copy_from_slice(&(f0 as u32).to_le_bytes());
    tag[4..8].copy_from_slice(&(f1 as u32).to_le_bytes());
    tag[8..12].copy_from_slice(&(f2 as u32).to_le_bytes());
    tag[12..16].copy_from_slice(&(f3 as u32).to_le_bytes());
    tag
}

fn anubis_poly1305_pad16(len: usize) -> usize {
    (16 - (len % 16)) % 16
}

fn anubis_aead_build_mac_input(aad: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(aad);
    m.extend(std::iter::repeat(0u8).take(anubis_poly1305_pad16(aad.len())));
    m.extend_from_slice(ciphertext);
    m.extend(std::iter::repeat(0u8).take(anubis_poly1305_pad16(ciphertext.len())));
    m.extend_from_slice(&(aad.len() as u64).to_le_bytes());
    m.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    m
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

/// AEAD seal: returns ciphertext || tag (16). AAD bound into tag (RWC Ch4).
fn anubis_aead_seal(key: AnubisValue, nonce: AnubisValue, aad: AnubisValue, plaintext: AnubisValue) -> AnubisValue {
    let (k, n) = anubis_aead_parse_key_nonce(&key, &nonce);
    let aad_b = anubis_crypto_bytes(&aad);
    let pt = anubis_crypto_bytes(&plaintext);
    // Poly1305 one-time key = ChaCha20 block 0
    let otk_block = anubis_chacha20_block(&k, 0, &n);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&otk_block[0..32]);
    let ct = anubis_chacha20_xor(&k, 1, &n, &pt);
    let mac_in = anubis_aead_build_mac_input(&aad_b, &ct);
    let tag = anubis_poly1305_mac(&otk, &mac_in);
    let mut out = ct;
    out.extend_from_slice(&tag);
    anubis_bytes_list(&out)
}

/// AEAD open: fails closed on tag mismatch (constant-time compare).
fn anubis_aead_open(key: AnubisValue, nonce: AnubisValue, aad: AnubisValue, ciphertext_and_tag: AnubisValue) -> AnubisValue {
    let (k, n) = anubis_aead_parse_key_nonce(&key, &nonce);
    let aad_b = anubis_crypto_bytes(&aad);
    let blob = anubis_crypto_bytes(&ciphertext_and_tag);
    if blob.len() < 16 {
        panic!("ANUBIS_CRYPTO_AEAD_OPEN_FAILED: ciphertext shorter than tag");
    }
    let (ct, tag_got) = blob.split_at(blob.len() - 16);
    let otk_block = anubis_chacha20_block(&k, 0, &n);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&otk_block[0..32]);
    let mac_in = anubis_aead_build_mac_input(&aad_b, ct);
    let tag_exp = anubis_poly1305_mac(&otk, &mac_in);
    let mut v = 0u8;
    for i in 0..16 {
        v |= tag_exp[i] ^ tag_got[i];
    }
    if v != 0 {
        panic!("ANUBIS_CRYPTO_AEAD_OPEN_FAILED: authentication tag mismatch (fail closed)");
    }
    let pt = anubis_chacha20_xor(&k, 1, &n, ct);
    anubis_bytes_list(&pt)
}

