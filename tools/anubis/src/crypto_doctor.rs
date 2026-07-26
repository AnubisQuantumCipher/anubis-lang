//! Crypto surface doctor — honest inventory of RWC-aligned capabilities.
//!
//! Host-side, no VZ. Reports what Anubis can claim about cryptography without
//! overselling CAVP, PQ DIY, or guest pure-path production fitness.

use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct CryptoCap {
    pub id: &'static str,
    pub rwc_chapter: &'static str,
    pub status: &'static str,
    pub host: &'static str,
    pub guest_zkvm: &'static str,
    pub notes: &'static str,
}

pub fn catalog() -> Vec<CryptoCap> {
    vec![
        CryptoCap {
            id: "sha256",
            rwc_chapter: "2",
            status: "LAB_REAL",
            host: "audited sha2",
            guest_zkvm: "pure",
            notes: "integrity/commitment only — not a MAC",
        },
        CryptoCap {
            id: "domain_hash / tuple_hash",
            rwc_chapter: "2",
            status: "LAB_REAL",
            host: "length-prefix + sha2",
            guest_zkvm: "length-prefix + pure sha2",
            notes: "TupleHash spirit; not NIST TupleHash byte-identical",
        },
        CryptoCap {
            id: "hmac_sha256 + verify",
            rwc_chapter: "3",
            status: "LAB_REAL",
            host: "hmac + subtle CT",
            guest_zkvm: "pure HMAC",
            notes: "ANUBIS_CRYPTO_MISUSE rejects tag == compares",
        },
        CryptoCap {
            id: "aead chacha20-poly1305",
            rwc_chapter: "4",
            status: "LAB_REAL",
            host: "chacha20poly1305 crate",
            guest_zkvm: "pure AEAD",
            notes: "nonce uniqueness caller-owned; AAD bind required for protocol meta",
        },
        CryptoCap {
            id: "aead_nonce_from_counter",
            rwc_chapter: "4",
            status: "LAB_REAL",
            host: "yes",
            guest_zkvm: "yes",
            notes: "unique only if counter never repeats under a key",
        },
        CryptoCap {
            id: "x25519 ecdh",
            rwc_chapter: "5",
            status: "LAB_REAL",
            host: "x25519-dalek",
            guest_zkvm: "HOST_ONLY panic",
            notes: "raw shared must not be AEAD key (checker + hybrid path)",
        },
        CryptoCap {
            id: "hybrid_seal/open",
            rwc_chapter: "6",
            status: "LAB_REAL",
            host: "X25519+HKDF+ChaCha",
            guest_zkvm: "HOST_ONLY panic",
            notes: "ECIES spirit; not full ECIES/X25519 standards suite",
        },
        CryptoCap {
            id: "ed25519 sign/verify",
            rwc_chapter: "7",
            status: "LAB_REAL",
            host: "ed25519-dalek",
            guest_zkvm: "HOST_ONLY panic",
            notes: "prefer EdDSA; not RSA-PSS path",
        },
        CryptoCap {
            id: "hkdf_sha256",
            rwc_chapter: "8",
            status: "LAB_REAL",
            host: "hkdf crate",
            guest_zkvm: "pure HKDF",
            notes: "multi-key derivation from IKM",
        },
        CryptoCap {
            id: "password_hash argon2id",
            rwc_chapter: "8",
            status: "LAB_REAL",
            host: "argon2 crate",
            guest_zkvm: "HOST_ONLY / pure limited",
            notes: "never raw sha256(password)",
        },
        CryptoCap {
            id: "secret IFC",
            rwc_chapter: "8/16",
            status: "LAB_REAL",
            host: "checker",
            guest_zkvm: "n/a",
            notes: "secret_source + ANUBIS_SECRET_EXFILTRATION",
        },
        CryptoCap {
            id: "tls / noise / signal",
            rwc_chapter: "9–10",
            status: "NOT_IMPLEMENTED",
            host: "use OS stacks",
            guest_zkvm: "n/a",
            notes: "do not invent transport in Anubis",
        },
        CryptoCap {
            id: "post-quantum KEM/sig",
            rwc_chapter: "14",
            status: "NOT_IMPLEMENTED",
            host: "future audited only",
            guest_zkvm: "n/a",
            notes: "no DIY lattices",
        },
        CryptoCap {
            id: "CAVP / FIPS cert",
            rwc_chapter: "16",
            status: "NOT_IMPLEMENTED",
            host: "n/a",
            guest_zkvm: "n/a",
            notes: "library use ≠ certification",
        },
    ]
}

pub fn report_json() -> serde_json::Value {
    let caps = catalog();
    let lab = caps.iter().filter(|c| c.status == "LAB_REAL").count();
    let not_impl = caps.iter().filter(|c| c.status == "NOT_IMPLEMENTED").count();
    json!({
        "schema": "anubis-crypto-doctor-v1",
        "identity": "proof-carrying systems language with RWC-aligned crypto surface",
        "book": "David Wong, Real-World Cryptography (Manning 2021)",
        "host_crypto_backend": "audited-crates (when anubis run lowers native)",
        "guest_crypto_backend": "pure-guest residual (zkVM); not preferred production host path",
        "counts": { "lab_real": lab, "not_implemented": not_impl, "total": caps.len() },
        "capabilities": caps,
        "oath": [
            "Boring primitives only",
            "Misuse tests exist",
            "No early-exit == on tags/passwords",
            "Nonce uniqueness is caller-owned",
            "Raw ECDH shared → HKDF or hybrid_*",
            "External review for multi-party protocols",
        ],
        "honesty": [
            "LAB_REAL means implemented + tested on host path, not CAVP",
            "HMAC/run-cap remain LAB_REAL_HMAC when applicable",
            "Do not claim Ed25519 for engagement receipt MACs",
        ],
    })
}

pub fn print_human() {
    let r = report_json();
    println!("anubis crypto-doctor — RWC-aligned surface inventory");
    println!("identity: {}", r["identity"].as_str().unwrap_or(""));
    println!(
        "host backend: {} | guest: {}",
        r["host_crypto_backend"], r["guest_crypto_backend"]
    );
    println!(
        "counts: LAB_REAL={} NOT_IMPLEMENTED={} total={}",
        r["counts"]["lab_real"], r["counts"]["not_implemented"], r["counts"]["total"]
    );
    println!();
    println!(
        "{:<28} {:<6} {:<16} {}",
        "ID", "RWC", "STATUS", "NOTES"
    );
    for c in catalog() {
        println!(
            "{:<28} {:<6} {:<16} {}",
            c.id, c.rwc_chapter, c.status, c.notes
        );
    }
    println!();
    println!("oath:");
    if let Some(arr) = r["oath"].as_array() {
        for o in arr {
            println!("  - {}", o.as_str().unwrap_or(""));
        }
    }
    println!("map: docs/language/RWC_LANGUAGE_MAP.md");
}
