# Anubis Vault · Sovereign v2

Operational password manager for high-threat environments (whistleblowers, journalists, IC/military, government, high-risk counsel).

**Standing rule:** any Anubis language work loads `.claude/skills/anubis-*/SKILL.md` first
and follows the skill procedure — never freehand.

| Skill | How it was applied |
|-------|--------------------|
| **anubis-ship-cadence** | SCOPE → DO → VERIFY → (commit only on green). CODE vs APP vs DOCS surfaces separated. |
| **anubis-build-app** | Isolated dir `product_app/` (only contacts source) → `check` → `build --evidence` → `verify` → `report` → `sign` |
| **anubis-defensive-harden** | Lever 3 reject (`vault_secret_leak_rejects`) · Lever 4 accept (`vault_verified_caps --verified`) · `delete_file` needs `uses(fs.write)` |
| **anubis-vz-confine-sign** | `vz confine` → caps `fs.read`+`fs.write`, no `net.send` (`product_app/confinement.json`) |
| **anubis-zero-fabrication-docs** | Claims command-grounded; boundaries explicit; `PRODUCT_PROOF.md` |

Skills **not** claimed for this app slice (wrong surface unless CODE reopens them):
`contract-lane-slice`, `soundness-hunt`, `vm-seal-evidence` (required before calling a
**compiler** change a sealed release), `lean-core-proof`, `selfhost-engine-port`,
`offensive-engagement`, `zk-prove-app`, `ci-greenup`.

---

## Product binary (build once · multi-op)

The unfinished product gaps (recompile-per-op, no in-language unlink, demo-only destroy) are closed:

| Gap (before) | Fix (now) | Proof |
|--------------|-----------|-------|
| Recompile on every `anubis run` | `anubis build` → native Mach-O once | `product/anubis_out` reused for create/verify/list/delete/destroy |
| No file delete builtin | `delete_file` / `remove_file` → `fs.write` | smoke + destroy unlinks path; missing path is idempotent OK |
| Destroy left a file | overwrite + `delete_file` | `test ! -e vault` after destroy |
| Bulk CRUD only via source runner | same binary, 500 contacts | create→verify→delete-one→delete-all→destroy |

```bash
# Build product CLI once
./target/release/anubis build examples/showcase/anubis_vault/vault_contacts.anb \
  -o examples/showcase/anubis_vault/product
BIN=examples/showcase/anubis_vault/product/anubis_out
VAULT=examples/showcase/anubis_vault/data/contacts.avault
MASTER='your-master-passphrase'

$BIN create  "$VAULT" "$MASTER" 500
$BIN verify  "$VAULT" "$MASTER"
$BIN list    "$VAULT" "$MASTER" 20
$BIN delete-one "$VAULT" "$MASTER" c0250   # ids are c0000..cNNNN (pad4)
$BIN delete-all "$VAULT" "$MASTER"
$BIN destroy "$VAULT"                      # overwrite + unlink; path gone
```

**Measured on this host (2026-07-22):** build once → create 500 → verify 8 sample opens PASS → delete-one remaining=499 → delete-all → destroy path unlinked. Binary SHA256 unchanged across all ops (`product/anubis_out`).

### Thorough regression battery

```bash
bash examples/showcase/anubis_vault/scripts/thorough_test.sh
# → ALL THOROUGH TESTS PASSED — 74 PASS / 0 FAIL  (see TEST_RESULTS.md)
```

Language surface added for product destroy:

- `delete_file(path)` / `remove_file(path)` — capability `fs.write` (same as write/append)
- Safe mode rejects without `uses(fs.write)`
- Runtime: `std::fs::remove_file`; `NotFound` is success (idempotent)

---

## Commands (ground truth)

From repo root, binary `./target/release/anubis`:

```bash
# Default policy lane (contracts + secret<> + effects declaration)
./target/release/anubis check examples/showcase/anubis_vault/vault.anb
# → check passed

# Runtime (declassify requires research surface)
./target/release/anubis run examples/showcase/anubis_vault/vault.anb --allow-research
# → SELFTEST GATE PASSED: 16/16

# Defensive-harden Lever 3 REJECT (secret → print)
./target/release/anubis check examples/showcase/anubis_vault/vault_secret_leak_rejects.anb
# → exit 1, ANUBIS_SECRET_EXFILTRATION

# Defensive-harden Lever 4 ACCEPT — verified linear fs.write (check AND run)
./target/release/anubis check --verified examples/showcase/anubis_vault/vault_verified_caps.anb
./target/release/anubis run examples/showcase/anubis_vault/vault_verified_caps.anb
# → check passed; writes /tmp/anubis_vault_verified_cap_demo.txt

# Main vault + product CLI both pass verified lane
./target/release/anubis check --verified examples/showcase/anubis_vault/vault.anb
./target/release/anubis check --verified examples/showcase/anubis_vault/vault_contacts.anb
# → check passed

# Hypervisor grant derived from proven effects
./target/release/anubis vz confine examples/showcase/anubis_vault/vault.anb \
  --out examples/showcase/anubis_vault/confinement.json
# → capabilities: fs.write; net.send absent

# Evidence seal (isolated app/ — only vault.anb)
cp examples/showcase/anubis_vault/vault.anb examples/showcase/anubis_vault/app/
./target/release/anubis build examples/showcase/anubis_vault/app/vault.anb \
  --evidence --out examples/showcase/anubis_vault/app/out
./target/release/anubis verify examples/showcase/anubis_vault/app/out/evidence-*-safe
# → verdict: PASS, bundle valid: true (includes confinement_manifest.json)
```

---

## What is ✅ real (this tree)

| Feature | Evidence |
|---------|----------|
| Argon2id **m=19456 KiB (19 MiB)**, t=2, p=1 | `kdf_master` in `vault.anb`; package header `kdf=argon2id,m=19456,t=2,p=1` |
| Random 16-byte salts (`to_hex`) | run log + package `salt_hex=` |
| Master **verifiers** (`password_hash_encode` / `password_verify_encoding`) | enroll + unlock + wrong-master reject selftests |
| Unlock **canary** AEAD | selftest unlock_canary |
| REAL / DECOY / DEAD DROP compartments | separate HKDF labels + demo sections A/B/B2/C |
| Duress verifier ≠ REAL verifier | selftest duress_verify |
| Clearance lattice + time window + reveal budget + burn | section D/G selftests |
| ChaCha20-Poly1305 entries + HMAC catalog + Ed25519 | section E |
| Sealed `.avault` package on disk | `/tmp/anubis_vault_sovereign_v2.avault` after run |
| n-of-n XOR key shred + `ct_eq` recombine | section F + 3 share files under `/tmp` |
| SMT policy contracts | `anubis check` + evidence `solver: PASS` |
| `secret<>` (print of master rejected in companion) | `vault_secret_leak_rejects.anb` |
| Evidence PCA + optional Ed25519 sign | `app/out/evidence-*-safe`, `bundle valid: true`, `signed: true` after `sign` |
| Confinement from proven effects | `confinement.json` + bundle `confinement_manifest.json` |
| **Native product binary** (no recompile per op) | `anubis build vault_contacts.anb -o product/` → multi-op on `product/anubis_out` |
| **In-language destroy (unlink)** | `delete_file` under `fs.write`; destroy overwrites then unlinks; path absent after |
| **Bulk CRUD 500 contacts** | create / verify / delete-one / delete-all / destroy on same binary |
| Contacts package on disk | `AVCONTACTS1` portable AEAD package + Argon2id verifier (ciphertext only) |
| **`check --verified` main vault** | `vault.anb` + `vault_contacts.anb` — linear `cap_acquire`/`cap_use` |
| **Runtime capabilities** | `cap_acquire` / `cap_use` / `secret_source` lower natively and execute |
| **Thorough regression** | `scripts/thorough_test.sh` → **76 PASS / 0 FAIL** (`TEST_RESULTS.md`) |

### Product surface (complete)

This tree is a finished **CLI high-threat vault** on Anubis: crypto, dual/duress worlds, policy gates, sealed packages, product contacts CRUD, build-once native binary, verified-lane authority, in-language destroy, confinement without net.send.

Operator practice (duress drills, share custody, physical media) is doctrine — not an unfinished feature of the code.

---

## Architecture (short)

```
master REAL ──password_verify──► Argon2id(19MiB)+HKDF ──► key_real ──AEAD entries
master DURESS ─password_verify─► Argon2id+HKDF ──────────► key_decoy (coercion world)
session: clearance × window × reveal budget × burn
persist: .avault (salt, canary, catalog MAC, Ed25519 sig, sealed entry hex)
recovery: 3 XOR shares (all required); SMT proves byte identity
```

---

## Files

| Path | Role |
|------|------|
| `vault.anb` | Sovereign selftest app (check + run, 16/16) |
| `vault_contacts.anb` | **Product** contacts vault CLI (create/verify/list/delete/destroy) |
| `vault_secret_leak_rejects.anb` | Harden Lever 3 reject |
| `vault_verified_caps.anb` | Harden Lever 4 accept (`check --verified` **and** `run`) |
| `confinement.json` | Output of `vz confine` |
| `app/vault.anb` | Isolated copy for evidence |
| `app/out/evidence-*-safe/` | PCA bundle |
| `product/anubis_out` | Built product binary (gitignored; rebuild with `anubis build`) |
| `data/` | Runtime vault files (gitignored) |
| `keys/` | Local Ed25519 PCA keys — **do not publish private keys** |
| `THREAT_MODEL.md` | Operator threat model |

---

## Operator reminders

1. Practice duress unlock under stress.  
2. Never co-locate all three recovery shares.  
3. Treat `/tmp` demo paths as lab only.  
4. Physical destruction of media is outside the language.  
5. `keys/signing.key` is a secret; keep it out of git remotes.
