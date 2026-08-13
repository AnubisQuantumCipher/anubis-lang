# Anubis builtins inventory (complete)

**Count: 213** — union of five name sets in `compiler/src/backends/run.rs`:

| Source | Role |
|---|---|
| `emit_builtin_call` match arms | executable / lowerable callees (178 pattern names) |
| `is_builtin_name` extra matches | statement / I/O / cap / secret names reserved even when not only in emit |
| `is_proof_input_builtin` | RISC0 proof surface (6) |
| `is_poc_kit_builtin` | research PoC kit (7) |
| `is_non_run_builtin` | analysis constructs not executable on the safe `run` path (11) |

Reproduce:

```bash
# See scratchpad/fleet_20260726/grok_thoth_stranger.md Task 2 for the exact Python extractor.
# Result must be 213. README historically understated "~150"; STDLIB_CORE said "~116" general-purpose.
```

**How to read status:**

- **run** — lowers via `emit_builtin_call` (or statement form) on `anubis run`
- **analysis** — checker/taint construct; not a normal safe-run user function
- **proof** — `anubis prove` / proof stubs
- **poc** — requires research/`--allow-research`

This file is the stranger-facing **complete** inventory. `LANGUAGE.md` still documents the general-purpose core in prose; `CRYPTO.md` documents *correct use* of crypto. Prefer this table when you need "does name X exist?".

### Runtime fail-closed (2026-07-27)

Many collection/math/crypto edge cases used to return a **silent wrong value** (`0`, `[]`,
no-op) under `anubis run`. That class is **gone** for the gated surface:

- Gate: `bash scripts/run_stdlib_failclosed_gate.sh` → **104/104 PASS** (re-verified 2026-07-27;
  fixture count grew from the earlier 32-set).
- Fixtures: `tests/fixtures/stdlib/*_should_fail_closed.anb` (exercise via **`anubis run`**, not
  `check` — these are runtime panics).
- Representative codes: `ANUBIS_EMPTY_COLLECTION`, `ANUBIS_NO_MATCH`, `ANUBIS_TYPE_ERROR`,
  `ANUBIS_INDEX_OUT_OF_BOUNDS`, `ANUBIS_CRYPTO_RANDOM_NEGATIVE_LENGTH`, …

Docs that still say "returns 0 on empty" for `first`/`pop`/`min`/`find`/… are **wrong**. See
`LANGUAGE.md` § Standard library.

---

## Previously undiscoverable surface (strict doc gap as of 2026-07-26)

These 29 names existed in the binary but did not appear in user-facing docs with backticks/tables (THOTH Round 1). Documented here so they are discoverable.

| Name | Category | Notes |
|---|---|---|
| `bytes_hex` | crypto | crypto · hex encode bytes |
| `cap_acquire_nonexportable` | capability_secret | capability · non-exportable linear cap |
| `cap_export` | capability_secret | capability · export attempt (static seal may reject) |
| `chacha20_poly1305_open` | crypto | crypto · AEAD open (ChaCha20-Poly1305) |
| `chacha20_poly1305_seal` | crypto | crypto · AEAD seal |
| `constant_time_eq` | crypto | crypto · constant-time equality (prefer over `==` on tags) |
| `ed25519_public_key` | crypto | crypto · derive public key from secret key material |
| `exit` | io_net_time | io · terminate process |
| `hmac_sha256_bytes` | crypto | crypto · HMAC-SHA256 raw bytes |
| `hmac_sha256_hex` | crypto | crypto · HMAC-SHA256 hex digest |
| `hybrid_open` | crypto | crypto · hybrid open path |
| `hybrid_seal` | crypto | crypto · hybrid seal path |
| `keychain_se_last_bind` | capability_secret | capability · last Keychain/SE bind probe result |
| `keychain_se_probe` | capability_secret | capability · Keychain / Secure Enclave probe |
| `now` | io_net_time | time · alias-style clock read |
| `password_hash_encode` | crypto | crypto · password hash encoding helper |
| `password_hash_pbkdf2_encode` | crypto | crypto · PBKDF2 encode helper |
| `password_hash_phc_raw` | crypto | crypto · PHC string raw path |
| `password_verify_encoding` | crypto | crypto · verify password encoding |
| `proof_input_u64` | proof | proof · u64 private witness input |
| `rand_gen` | crypto | rng · effect-facing random generation name |
| `random` | crypto | rng · random helper (prefer `random_bytes` for keys) |
| `remove_file` | io_net_time | io · delete file |
| `sha256_bytes` | crypto | crypto · SHA-256 raw bytes (cf. `sha256` hex forms) |
| `time_now` | io_net_time | time · clock read |
| `to_hex` | crypto | crypto/util · hex encode |
| `x25519_keygen` | crypto | crypto · X25519 key generation |
| `x25519_public_key` | crypto | crypto · X25519 public from secret |
| `x25519_shared` | crypto | crypto · X25519 shared secret |

---

## Full inventory by category

### string_convert (32)

`bool`, `capitalize`, `char_at`, `chars`, `chr`, `contains`, `ends_with`, `float`, `index_of`, `int`, `join`, `len`, `lines`, `lower`, `ord`, `pad_end`, `pad_start`, `parse_float`, `parse_float_opt`, `parse_int`, `parse_int_opt`, `repeat`, `replace`, `reverse`, `split`, `starts_with`, `str`, `substr`, `trim`, `type`, `upper`, `words`

### math (28)

`abs`, `acos`, `asin`, `atan`, `cbrt`, `ceil`, `clamp`, `cos`, `e`, `exp`, `factorial`, `floor`, `gcd`, `hypot`, `ln`, `log`, `log10`, `log2`, `max`, `min`, `pi`, `pow`, `round`, `sign`, `sin`, `sqrt`, `tan`, `trunc`

### list_map_functional (49)

`all`, `any`, `apply`, `call`, `chunk`, `compose`, `concat`, `count`, `drop`, `drop_while`, `each`, `entries`, `enumerate`, `filter`, `find`, `first`, `flat_map`, `flatten`, `get`, `has_key`, `identity`, `insert`, `is_empty`, `keys`, `last`, `map`, `map_values`, `max_by`, `merge`, `min_by`, `partition`, `pop`, `position`, `product`, `push`, `range`, `reduce`, `remove`, `slice`, `sort`, `sort_by`, `sum`, `take`, `take_while`, `times`, `unique`, `values`, `window`, `zip`

### io_net_time (22)

`append_file`, `args`, `connect`, `env`, `eprint`, `eprintln`, `exit`, `getenv`, `http_get`, `http_post`, `input`, `now`, `open`, `print`, `println`, `read_file`, `read_line`, `remove_file`, `send`, `time`, `time_now`, `write_file`

### crypto (41)

`aead_nonce_from_counter`, `aead_open`, `aead_seal`, `argon2id_hash`, `bytes_hex`, `chacha20_poly1305_open`, `chacha20_poly1305_seal`, `constant_time_eq`, `crypto_backend`, `ct_eq`, `domain_hash`, `ed25519_keygen`, `ed25519_public_key`, `ed25519_sign`, `ed25519_verify`, `hash_sha256`, `hkdf_sha256`, `hmac_sha256`, `hmac_sha256_bytes`, `hmac_sha256_hex`, `hmac_sha256_verify`, `hybrid_open`, `hybrid_seal`, `password_hash`, `password_hash_encode`, `password_hash_pbkdf2_encode`, `password_hash_phc`, `password_hash_phc_raw`, `password_verify`, `password_verify_encoding`, `pbkdf2_hmac_sha256`, `rand_gen`, `random`, `random_bytes`, `sha256`, `sha256_bytes`, `sha256_hex`, `to_hex`, `x25519_keygen`, `x25519_public_key`, `x25519_shared`

### capability_secret (7)

`cap_acquire`, `cap_acquire_nonexportable`, `cap_export`, `cap_use`, `keychain_se_last_bind`, `keychain_se_probe`, `secret_source`

### analysis (11)

`assert`, `assume`, `declassify`, `exec`, `memcpy`, `shell`, `sink`, `sql`, `symbolic`, `system`, `taint_source`

### proof (6)

`proof_assert`, `proof_commit_bool`, `proof_commit_u32`, `proof_input_bool`, `proof_input_u32`, `proof_input_u64`

### poc (7)

`cyclic`, `flat`, `p16`, `p32`, `p64`, `p8`, `target_run`

### control (3)

`break`, `continue`, `return`

### other (7)

`atan2`, `delete_file`, `network_send`, `panic`, `rand`, `tuple_hash`, `write`

---

## Capability + confidentiality callables (complete)

| Name | Role |
|---|---|
| `cap_acquire` | acquire a linear use-once capability token (string name, e.g. `"fs.write"`) |
| `cap_use` | consume the token exactly once |
| `cap_acquire_nonexportable` | acquire a non-exportable cap (Keychain/SE path) |
| `cap_export` | attempt export (sealed / fail-closed when forbidden) |
| `keychain_se_probe` | probe Keychain / Secure Enclave bind path |
| `keychain_se_last_bind` | last bind result |
| `secret_source` | mint a confidentiality-labelled value (also: annotate `let x: secret<T> = …`) |

**Effect authorization is separate from the token.** File write needs `uses(fs.write)` on the function (or verified-lane equivalent). `cap_acquire("fs.write")` alone does not open `write_file`.

## Crypto callables (complete list from the 213-union)

`aead_nonce_from_counter`, `aead_open`, `aead_seal`, `argon2id_hash`, `bytes_hex`, `chacha20_poly1305_open`, `chacha20_poly1305_seal`, `constant_time_eq`, `crypto_backend`, `ct_eq`, `domain_hash`, `ed25519_keygen`, `ed25519_public_key`, `ed25519_sign`, `ed25519_verify`, `hash_sha256`, `hkdf_sha256`, `hmac_sha256`, `hmac_sha256_bytes`, `hmac_sha256_hex`, `hmac_sha256_verify`, `hybrid_open`, `hybrid_seal`, `password_hash`, `password_hash_encode`, `password_hash_pbkdf2_encode`, `password_hash_phc`, `password_hash_phc_raw`, `password_verify`, `password_verify_encoding`, `pbkdf2_hmac_sha256`, `rand_gen`, `random`, `random_bytes`, `sha256`, `sha256_bytes`, `sha256_hex`, `to_hex`, `x25519_keygen`, `x25519_public_key`, `x25519_shared`

Correct *usage* rules: [`CRYPTO.md`](CRYPTO.md). Prefer `hmac_sha256_verify` / AEAD / `random_bytes` / Argon2id over inventing constructions.

---

## Related docs

| Doc | Role |
|---|---|
| [`LANGUAGE.md`](../../LANGUAGE.md) § Standard library | prose tour of the general-purpose core |
| [`STDLIB_CORE.md`](STDLIB_CORE.md) | analysis / proof / PoC subset |
| [`CRYPTO.md`](CRYPTO.md) | how to use crypto safely |
| [`INFORMATION_FLOW.md`](INFORMATION_FLOW.md) | taint + secret model |
| [`POC_KIT.md`](POC_KIT.md) | research PoC builtins |

_Generated for fleet THOTH Round 2 from `run.rs` on the working tree. Count verified: 213._
