# Anubis Cryptography Surface (RWC / Shannon / PQ-aware)

**Full chapter map:** [`RWC_LANGUAGE_MAP.md`](RWC_LANGUAGE_MAP.md) (David Wong, *Real-World Cryptography*).

**Humility first (RWC Ch1.8 / Ch16):** this document and the Anubis crypto APIs teach
*correct use of boring primitives*. They do not make you a protocol designer. Do not invent
AEADs, ratchets, or “simpler Noise.” Prefer standards + review.

## Book grounding (extracted for Phase 5)

| Source | What we locked into the language |
|--------|----------------------------------|
| **David Wong, *Real-World Cryptography*** Ch1 | Kerckhoffs: only the key is secret; adversary knows the algorithm |
| RWC Ch2 | Hash ≠ MAC ≠ password hash; length-extension on SHA-2 Merkle–Damgård; domain separation |
| RWC Ch3 | HMAC construction; **constant-time tag verify**; never early-exit `==` on tags |
| RWC Ch4 | AEAD default (ChaCha20-Poly1305); unique nonces; **AAD binds** protocol metadata |
| RWC Ch8 | CSPRNG for keys/nonces/salts; **password hashing = memory-hard KDF** (Argon2id); unique salt |
| RWC Ch14/16 | PQ era is coming; 83% of breaks are *misuse*; boring crypto; don’t roll your own |
| **Zheng Vol 1** | Shannon model; authentication/integrity as first-class; fail closed |
| **Zheng Vol 2** | Lattices / PQ public-key — **future via audited ML-KEM/ML-DSA**, not DIY in `anubis run` |

## Layers

1. **Native microarchitecture** (`anubis run` → temporary **Cargo** project + audited crates):
   - Implementation: `backends/audited_crypto_runtime.inc.rs`
   - Crates (RustCrypto / PHC — **not** DIY):
     | API | Crate |
     |-----|--------|
     | SHA-256 | `sha2` |
     | HMAC-SHA256 + verify | `hmac` + `subtle` |
     | HKDF-SHA256 | `hkdf` |
     | CSPRNG | `getrandom` |
     | ChaCha20-Poly1305 AEAD | `chacha20poly1305` |
     | Argon2id / password_hash | `argon2` |
     | PBKDF2-HMAC-SHA256 | `pbkdf2` |
   - Surface (core): `sha256`, `hmac_sha256_verify`, `ct_eq` / `constant_time_eq`, `hkdf_sha256`,
     `domain_hash`, `random_bytes`, `aead_seal`/`aead_open`, `chacha20_poly1305_seal`/`_open`,
     `pbkdf2_hmac_sha256`, `argon2id_hash`, `password_hash` / `password_verify`,
     **`password_hash_phc`** (standard `$argon2id$…`), password encode helpers
     (`password_hash_encode`, `password_hash_pbkdf2_encode`, `password_hash_phc_raw`,
     `password_verify_encoding`), **`ed25519_keygen` / `ed25519_sign` / `ed25519_verify` /
     `ed25519_public_key`**, **`x25519_keygen` / `x25519_public_key` / `x25519_shared`**,
     byte helpers (`sha256_bytes`, `hmac_sha256_bytes` / `_hex`, `bytes_hex`, `to_hex`),
     hybrid seal/open (`hybrid_seal`, `hybrid_open`), `crypto_backend()` → `"audited-crates"`
   - **Full callable list:** [`BUILTINS.md`](BUILTINS.md) (213-name inventory; crypto section)
   - Byte lists: elements **must** be in `0..=255` (fail closed — no silent truncation)

2. **RISC0 guest microarchitecture** (prove path only): pure-Rust crypto still embedded
   (`pure_crypto_runtime.inc.rs` + `password_crypto_runtime.inc.rs`) so the zkVM guest
   Cargo.toml does not pull argon2/chacha into the guest target. Same Anubis API surface.

3. **Anubis stdlib** — `import std.crypto;` (composition, same rules):
   - `crypto::mac_verify`, `crypto::aead_encrypt` / `aead_decrypt`, `crypto::kdf_hkdf_sha256`,
     `crypto::rand_bytes`, `crypto::aead_keygen`, `crypto::commit`,
     `crypto::password_hash` / `password_verify`, `crypto::kdf_argon2id`,
     `crypto::kdf_pbkdf2_hmac_sha256`, …

## Rules of engagement (fail closed)

| Do | Do not |
|----|--------|
| `hmac_sha256_verify(k, m, tag)` | `hmac_sha256(k, m) == tag` (timing) |
| AEAD for encrypt+auth | CBC + hand MAC / invent EtM |
| Unique 12-byte nonce per key | Reuse nonce; all-zero nonce forever |
| Bind headers in AAD | Leave protocol metadata unauthenticated |
| `random_bytes` / `crypto::rand_bytes` for keys | `rand()` clock mix for secrets |
| HKDF for multi-key derivation | `sha256(key + msg)` as KDF |
| **`password_hash` / `password_verify`** | `sha256(password)` or unsalted fast hash |
| Argon2id (or PBKDF2 ≥ 600k) | Low-iteration PBKDF2 / raw hash “auth” |

Checker: `ANUBIS_CRYPTO_MISUSE` when `==`/`!=` is applied to an HMAC call **or** password/KDF call result.

## Password hashing (RWC Ch8 — what the book is trying to teach)

Low-entropy secrets (passwords) must resist **offline** brute force. That means:

1. **Unique salt** per user (CSPRNG, ≥ 16 bytes recommended; API enforces ≥ 8 for Argon2).
2. **Memory-hard** or deliberately slow KDF — **Argon2id** preferred (OWASP: ~19 MiB, t=2, p=1).
3. **Constant-time verify** of the stored digest — never string `==` on the encoding.

### Easy path (stdlib)

```anubis
import std.crypto;

fn register(pw) {
    return crypto::password_hash(pw);  // Argon2id + random salt → encoding string
}

fn login(pw, stored) {
    return crypto::password_verify(pw, stored);  // constant-time
}
```

Encoding form:

```text
anubis$argon2id$v=19$m=19456,t=2,p=1$<hexsalt>$<hexhash>
```

Fallback (PBKDF2-HMAC-SHA256 @ 600_000 iterations, OWASP-class):

```anubis
let stored = crypto::password_hash_pbkdf2(pw);
// anubis$pbkdf2-sha256$i=600000$...
```

### Parameterized KDFs (tests / key stretching)

```anubis
// RFC 6070-style / vectors — not for password *storage* at low iteration counts
let dk = crypto::kdf_pbkdf2_hmac_sha256("password", "salt", 4096, 32);

// Argon2id raw: m_kib, t, p, out_len
let h = crypto::kdf_argon2id("password", "somesalt", 32, 3, 1, 32);
```

## AEAD API

```anubis
import std.crypto;

fn main() {
    let key = crypto::aead_keygen();       // 32 CSPRNG bytes
    let nonce = crypto::aead_nonce();      // 12 CSPRNG bytes — unique per key!
    let aad = "v1|to=alice";               // authenticated metadata
    let ct = crypto::aead_encrypt(key, nonce, aad, "secret");
    let pt = crypto::aead_decrypt(key, nonce, aad, ct); // panics on tag fail
}
```

Wrong AAD, wrong key, or truncated ciphertext → `ANUBIS_CRYPTO_AEAD_OPEN_FAILED`.

## Post-quantum (honest)

Vol 2 is explicit: classical public-key dies to Shor. Anubis does **not** ship DIY lattices.
Future: `std.pq` behind audited ML-KEM/ML-DSA libraries (CNSA 2.0 alignment), not hand-rolled NTRU.
Symmetric surface (ChaCha20-Poly1305, HMAC-SHA-256, Argon2id) remains relevant under Grover
(use full key lengths — we default 256-bit keys).

## Practitioner oath (before claiming “secure”)

1. Boring primitives only (this surface).  
2. Misuse tests exist (bad tag, wrong AAD, CT verify, password_verify).  
3. No secret compared with early-exit `==`.  
4. Nonce uniqueness is caller-owned and documented.  
5. Passwords use Argon2id (or strong PBKDF2), never raw hash.  
6. External review for any *protocol* built on these APIs.

## Phase-5 lock checklist

| Capability | Status | Evidence |
|------------|--------|----------|
| HMAC + CT verify | LOCKED | crate `hmac` + `subtle`; `ANUBIS_CRYPTO_MISUSE` |
| HKDF-SHA256 | LOCKED | crate `hkdf` |
| CSPRNG | LOCKED | crate `getrandom` |
| ChaCha20-Poly1305 AEAD | LOCKED | crate `chacha20poly1305` |
| Domain-separated hash | LOCKED | `domain_hash` over `sha2` |
| PBKDF2-HMAC-SHA256 | LOCKED | crate `pbkdf2` |
| Argon2id | LOCKED | crate `argon2` |
| password_hash / verify | LOCKED | Argon2id default encoding, CT verify |
| password_hash_phc | LOCKED | standard PHC via argon2 PasswordHasher |
| Ed25519 sign/verify | LOCKED | crate `ed25519-dalek` (host only) |
| X25519 ECDH | LOCKED | crate `x25519-dalek` (host only) |
| Hybrid envelope (ECIES spirit) | LOCKED | ephemeral X25519 + HKDF + ChaCha20-Poly1305 |
| Tuple-style multi-part hash | LOCKED | `tuple_hash` / `crypto::commit_parts` |
| Counter AEAD nonce | LOCKED | `aead_nonce_from_counter` (caller uniqueness) |
| Fail-closed byte lists | LOCKED | `ANUBIS_CRYPTO_BYTE_RANGE` |
| Native emit path | LOCKED | cargo project (`compile_native_rust_to_exe`) |
| PQ public-key | DOCUMENTED | not DIY; future audited path |
| Full chapter map | DOC | `docs/language/RWC_LANGUAGE_MAP.md` |

**Practitioner note:** native crypto is “use the boring audited library.” Guest still carries a
pure fallback only because the zkVM dependency surface is constrained — not as the preferred
production story for host programs.

## Residual risks (honest — do not glaze)

| Risk | Reality |
|------|---------|
| Host vs guest crypto | **Different implementations.** Host = crates; guest = pure. Ed25519/PHC **host-only** (guest panics). Check `crypto::backend()`. |
| `anubis build` | cargo (same as run). Bare `rustc` cannot link audited crates — fixed. |
| Nonce uniqueness | Still **caller-owned**. Library cannot prevent reuse. |
| Password encoding | `anubis$…` **or** standard PHC via `password_hash_phc` / verify of `$argon2…`. |
| Byte lists | Out-of-range elements panic (`ANUBIS_CRYPTO_BYTE_RANGE`); bare ints rejected as key material. |
| Offline brute force | Argon2id m=19MiB,t=2 is OWASP-class default; tune for your threat model. |
| PQ / hybrid KEX | Not shipped DIY; future audited ML-KEM hybrids only. |
| Key zeroization | Not guaranteed after use (Rust drop); secrets may linger in process memory. |
