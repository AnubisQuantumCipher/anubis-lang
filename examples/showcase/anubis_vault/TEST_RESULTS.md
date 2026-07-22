# Anubis Vault · First-time thorough retest

**Date:** 2026-07-22T13:02Z  
**Stance:** clean product build + full battery + extended first-time probes  
**Skills:** anubis-build-app · anubis-defensive-harden · anubis-vz-confine-sign · anubis-zero-fabrication-docs  

## Grand total

| Battery | Result |
|---------|--------|
| Fresh `anubis build` product binary | PASS (Mach-O arm64) |
| `scripts/thorough_test.sh` | **76 PASS / 0 FAIL** |
| First-time extended suite (E1–E8) | **33 PASS / 0 FAIL** |
| **Combined** | **109 PASS / 0 FAIL** |

Product binary this run:

```text
examples/showcase/anubis_vault/product/anubis_out
SHA256 e0aa2f2110e53fd719c4be9ce6c2fde109ca4212d96aef3c0263d153f40015e8
```

---

## Phase A — Clean build

```bash
rm -rf product data
./target/release/anubis build examples/showcase/anubis_vault/vault_contacts.anb -o product/
# → native artifact: product/anubis_out
```

## Phase B — Thorough script (76)

Static / harden / verified / caps run · sovereign 16/16 · create 100 · wrong master ·
list · delete-one · multi-delete · delete-all · destroy · n=1 · scale 500 · AEAD tamper ·
dual-vault isolation · binary sha stable · confine no net.send.

Re-run: `bash examples/showcase/anubis_vault/scripts/thorough_test.sh`

## Phase C — First-time extended (33)

| ID | What | Result |
|----|------|--------|
| E1 | Product binary no-args usage | PASS |
| E2 | `anubis run` create 12 → verify → destroy (source path, not only native bin) | PASS |
| E3 | Sovereign selftest inventory: gate 16/16, named section PASSes, `.avault` + 3 shares, no obvious secrets | PASS |
| E4 | Package forensics: AVCONTACTS1, salt 32 hex, argon2id verifier, 25 tab-rows, hex nonce/ct, **distinct salts** across vaults | PASS |
| E5 | Corrupt package (`NOTAVault`) not accepted as valid data vault | PASS |
| E6 | Isolated `build --evidence` + `verify` → `bundle valid: true` + confinement_manifest | PASS |
| E7 | `check --verified` vault + contacts; caps runtime write | PASS |
| E8 | Operator day: enroll 40 → list → delete one (39) → re-open PASS → wipe → destroy (path gone) | PASS |

Evidence bundle this run:

```text
examples/showcase/anubis_vault/fresh_evidence_app/out/evidence-*-safe
verdict: PASS · bundle valid: true
```

---

## What a first-time operator can trust

1. **Policy lane** — contracts + secret exfil reject + verified linear caps (check and run).  
2. **Sovereign crypto demo** — Argon2id, dual/duress, canary, clearance, budget, burn, AEAD, HMAC, Ed25519, air-gap export, n-of-n shred — 16/16.  
3. **Product contacts CLI** — build once; multi-op without recompile; wrong password does not mutate; AEAD tamper fails open; destroy unlinks.  
4. **Both dispatch paths** — `anubis run … -- create|…` and `product/anubis_out create|…`.  
5. **Sealed evidence** — isolated build, verifiable bundle, confinement without net.send.

## How to re-run everything

```bash
cd /Users/sicarii/anubis-lang
rm -rf examples/showcase/anubis_vault/product examples/showcase/anubis_vault/data
./target/release/anubis build examples/showcase/anubis_vault/vault_contacts.anb \
  -o examples/showcase/anubis_vault/product
bash examples/showcase/anubis_vault/scripts/thorough_test.sh
# optional: re-run extended probes from the session log or day-in-the-life manually
```
