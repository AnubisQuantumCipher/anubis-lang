# Real-World Cryptography → Anubis language map

**Source:** David Wong, *Real-World Cryptography* (Manning, 2021).  
**Rule:** we put the book’s **boring, misuse-resistant practices** into the language — not DIY primitives.  
**Honesty:** this is applied crypto engineering, not a claim that reading the book makes Anubis “formally proven secure.”

## How the book shapes Anubis

| RWC chapter | Lesson | In Anubis |
|-------------|--------|-----------|
| **1** Introduction / Kerckhoffs | Only the key is secret; algorithms are public | Public audited crates; secrets via `secret_source` / info-flow |
| **1.8** Word of warning | Amateurs invent breakable crypto | `CRYPTO.md` humility; no invent-AEAD/Noise |
| **2** Hash functions | Hash ≠ MAC ≠ password hash; domain separation; TupleHash | `sha256`, `domain_hash`, **`tuple_hash` / `crypto::commit_parts`** |
| **2.6** Password hashing | Memory-hard KDF, unique salt | `password_hash` / Argon2id / PBKDF2 high iters |
| **3** MACs | Constant-time tag verify; no early-exit `==` | `hmac_sha256_verify`, `ANUBIS_CRYPTO_MISUSE` |
| **4** AEAD | ChaCha20-Poly1305 / AES-GCM; unique nonces; AAD | `aead_seal`/`open`, AAD args, **`aead_nonce_from_counter`** |
| **5** Key exchange | ECDH / X25519; never use raw shared as key | **`x25519_*`**, must HKDF (enforced in hybrid) |
| **6** Hybrid encryption | ECIES spirit: ephemeral DH + KDF + AEAD | **`hybrid_seal` / `hybrid_open`** (host) |
| **7** Signatures / ZKP | Prefer EdDSA; malleability awareness | `ed25519_*` (host); RISC0 ZKP path separate |
| **8** Randomness & secrets | CSPRNG; HKDF; secret management | `random_bytes`, `hkdf_sha256`, secret IFC |
| **9** Secure transport | TLS; prefer standards | Not reimplemented; use OS/TLS stacks |
| **10** E2E / Signal | X3DH, Double Ratchet — hard | **NOT DIY** — compose hybrid + ratchet only after review |
| **11–12** Auth / BFT | Passwords, PAKE, consensus | Password APIs; BFT not language surface |
| **13** Hardware crypto | SE / HSM / TEE | Residual Keychain/SE bind; VZ isolation |
| **14** Post-quantum | Shor breaks classical PK; ML-KEM era | **NOT DIY lattices**; future audited `std.pq` |
| **15** Next-gen | MPC, FHE, general ZKP | RISC0 path = ZKP proving; FHE/MPC not claimed |
| **16** When crypto fails | Misuse ~83%; don’t roll your own; good libs | Misuse checker + audited crates only on host |

## New surface (this integration)

```anubis
import std.crypto;

// Ch2 multi-part commitment
let h = crypto::commit_parts("v1", ["alice", "bob", "42"]);

// Ch4 sequential nonce (unique per key if counter never repeats)
let n = crypto::aead_nonce_counter(7);

// Ch5 + Ch6 hybrid envelope (host-only)
let keys = crypto::ecdh_keygen();       // [sk, pk]
let env = crypto::hybrid_encrypt(keys[1], "v1|to=bob", "hello");
// env = [eph_pk, nonce, ct]
let pt = crypto::hybrid_decrypt(keys[0], env[0], "v1|to=bob", env[1], env[2]);
```

## Permanent non-claims (from the book’s own caution)

- Inventing Noise/Signal-class protocols without review  
- DIY post-quantum KEMs/signatures  
- Claiming CAVP/FIPS from library use alone  
- Guest pure-crypto path as “production host crypto” (host = audited crates)

## Practitioner oath (RWC Ch16)

1. Boring primitives only (this map).  
2. Misuse tests exist (bad tag, wrong AAD, CT verify, hybrid fail-closed).  
3. No secret/tag compared with early-exit `==`.  
4. Nonce uniqueness is caller-owned (or hybrid’s random nonce).  
5. Raw X25519 shared secret → always HKDF before use (`hybrid_*` does this).  
6. External review for any multi-party protocol built on these APIs.
