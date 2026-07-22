# Anubis Vault Sovereign — Threat Model

**Classification of this document:** public design note  
**Product:** `examples/showcase/anubis_vault/vault.anb`  
**Users in mind:** whistleblowers, journalists, IC/military operators, government officials, high-risk counsel, civil-society defenders  

---

## 1. Assets

| Asset | Sensitivity | Notes |
|-------|-------------|--------|
| Master passphrase (REAL) | Critical | Never printed; declassified only into KDF |
| Master passphrase (DURESS) | High | Opens decoy only; must be distinct and practiced |
| Compartment AEAD keys | Critical | Derived; 32-byte; never printed |
| Entry plaintexts | Critical–SCI | `secret<>`; reveal budget limited |
| Catalog fingerprint | Public | HMAC + Ed25519 sealed |
| Audit chain tip | Public | Event codes only — no secrets |
| Export package | High | Ciphertext; needs vault key offline |

## 2. Adversaries

1. **Coercive state actor** (border / custody) — wants *a* working password  
2. **Forensic lab** — disk image, offline GPU cracking  
3. **Network observer** — metadata and traffic (this demo has none)  
4. **Compromised colleague** — partial clearance, curious or coerced  
5. **Malicious courier** — tampers with export blob  
6. **Opportunistic thief** — unlocked laptop, short window  

## 3. Security properties we aim for

| Property | Status in demo |
|----------|----------------|
| Confidentiality of entry plaintexts at rest (AEAD) | REAL (runtime crypto) |
| Integrity of catalog | REAL (HMAC + Ed25519) |
| Plausible deniability under duress | REAL (separate decoy key + compartment) |
| Need-to-know (clearance) | REAL (runtime + proved rank helpers) |
| Time-boxed access | REAL (window check) |
| Session reveal budget | REAL (linear 0..5) |
| Emergency burn (session) | REAL (fail-closed deny) |
| Air-gap export | REAL (no net; sealed package) |
| Threshold recovery algebra | REAL (n-of-n XOR, SMT `result == k`) |
| Memory zeroization / SE binding | NOT CLAIMED |
| Full Shamir m-of-n | NOT CLAIMED |
| Resistance to rubber-hose | NOT CLAIMED (human + legal) |

## 4. Trust boundary

```
[ operator brain ]  --passphrases-->  [ Anubis process ]
                                          |
                    +---------------------+---------------------+
                    | check lane          | run lane            |
                    | SMT + secret<>      | RustCrypto AEAD/KDF |
                    +---------------------+---------------------+
                                          |
                              [ local disk: ciphertext only ]
                              [ no network sockets in demo ]
```

Trusted computing base today includes: Anubis compiler/runtime, RustCrypto, OS, hardware.

## 5. Residual risks (honest)

- Duress only works if the operator **uses the duress passphrase under stress**.  
- Decoy vault must be **lived-in** (recent, boring activity) or it fails psychological inspection.  
- Argon2id is **m=19456 KiB (19 MiB), t=2, p=1** in sovereign v2; raise further if the threat model demands.  
- Burn does not erase ciphertext from SSD wear-leveling; plan physical destruction.  
- A compiler or crypto-runtime bug is a vault bug — run the evidence gates.

## 6. Intended use

Authorized, defensive protection of legitimate secrets.  
Not for concealing crime. Not a weapon. Not a substitute for operational doctrine.
