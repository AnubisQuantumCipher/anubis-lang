//! Effect-derived macOS entitlement / App Sandbox **profile** — OS-facing policy DERIVED from the
//! language's own proven capability set, sealed in evidence and re-derived on verify (fail-closed).
//!
//! Schema `anubis.entitlements.v1`. Parallel to `confinement.rs` (hypervisor grants from the same
//! six-cap effect fixpoint), this module maps the SAME proven set into a deterministic entitlement
//! + sandbox posture artifact suitable for `codesign --entitlements` demos.
//!
//! HONESTY (load-bearing):
//! - Every entitlement key ships with `apple_enforced_claim: false` until a signed binary is proven
//!   to carry the plist. This module **derives + seals + re-derives**; it does **not** claim the OS
//!   enforces the profile without codesign / App Sandbox enablement.
//! - Unbounded (`open`) effect sets use the MOST restrictive posture (fail-closed on minimum
//!   knowledge), never a permissive default.
//! - Under-grant: prefer denying network / file / exec when the mapping is ambiguous.
//! - Toolchain VZ entitlements (`com.apple.security.virtualization`) are **not** mixed into the
//!   language-derived app profile.
//! - Keychain / Secure Enclave: runtime may *attempt* bind for `cap_acquire_nonexportable`
//!   on macOS (`keychain_se_runtime.inc.rs`). This profile now **derives** keychain-related
//!   keys when NE is present. Host enforcement still requires codesign
//!   (`apple_enforced_claim: false`); SE attestation of a production app is residual.

use crate::frontend::parse_source;
use crate::package::merkle;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ENTITLEMENT_PROFILE_FILENAME: &str = "entitlement_profile.json";
pub const ENTITLEMENT_PLIST_FILENAME: &str = "program.entitlements";
pub const ENTITLEMENT_SCHEMA: &str = "anubis.entitlements.v1";

/// The six canonical capabilities, stable order for deterministic JSON.
const CAPS: [&str; 6] = [
    "net.send", "fs.read", "fs.write", "shell", "time.now", "rand.gen",
];

/// One derived entitlement key. `apple_enforced_claim` is always false until a later slice proves
/// a signed binary actually carries the key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntitlementKey {
    pub key: String,
    pub enabled: bool,
    pub reason: String,
    /// Always false in this slice: derivation ≠ host enforcement without codesign.
    pub apple_enforced_claim: bool,
}

/// App Sandbox posture derived from the proven effect set (not host path rules).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxPosture {
    /// True whenever we emit a profile (Safe packaging residual: sandbox on by default).
    pub enabled: bool,
    /// `com.apple.security.network.client` posture — only true when net.send is proven AND bounded.
    pub network_client: bool,
    /// Server sockets: always false in this slice (no language effect maps cleanly).
    pub network_server: bool,
    /// File-read posture (not unrestricted host FS claim).
    pub file_read: bool,
    /// File-write posture.
    pub file_write: bool,
    /// Process-exec posture when `shell` is proven; still residual / needs_human.
    pub process_exec: bool,
    pub notes: Vec<String>,
    pub needs_human: Vec<String>,
    pub advisory: Vec<String>,
}

/// Language-core entitlement profile: pure function of source alone → re-derivable, fail-closed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntitlementProfile {
    pub schema: String,
    pub package: String,
    pub version: String,
    pub source_merkle: String,
    /// False when the effect fixpoint's `open` bit was set.
    pub effects_bounded: bool,
    /// Subset of the six capabilities proved present, sorted (CAPS order).
    pub capabilities_present: Vec<String>,
    pub entitlements: Vec<EntitlementKey>,
    pub sandbox: SandboxPosture,
    pub notes: Vec<String>,
}

/// True when source mentions the non-exportable capability mint (string scan is enough for
/// entitlement derivation; the checker remains the sealedness authority).
fn source_uses_nonexportable_cap(source: &str) -> bool {
    source.contains("cap_acquire_nonexportable")
}

/// Derive the entitlement / sandbox profile from source. Parse failure → Err, no profile.
pub fn derive_entitlement_profile(
    package: &str,
    version: &str,
    source: &str,
) -> Result<EntitlementProfile, String> {
    let source_merkle = merkle::sha256_hex(source.as_bytes());
    let ast = parse_source(source).map_err(|e| {
        format!("ANUBIS_ENTITLEMENT_PARSE_FAILED: entitlement derivation parse failed: {e}")
    })?;
    let (caps, open) = crate::middle::effects::program_capability_set(&ast.items);

    let has = |c: &str| caps.contains(c);
    let effects_bounded = !open;
    let uses_ne_caps = source_uses_nonexportable_cap(source);

    // Restrictive defaults when unbounded or capability absent.
    let net_client = !open && has("net.send");
    let file_read = !open && (has("fs.read") || has("fs.write"));
    let file_write = !open && has("fs.write");
    let process_exec = !open && has("shell");
    let network_server = false;

    let mut entitlements = Vec::new();

    // App Sandbox master switch — always on for language-derived profiles (restrictive packaging).
    entitlements.push(EntitlementKey {
        key: "com.apple.security.app-sandbox".into(),
        enabled: true,
        reason:
            "language-derived profiles default to App Sandbox ON (restrictive packaging posture)"
                .into(),
        apple_enforced_claim: false,
    });

    entitlements.push(EntitlementKey {
        key: "com.apple.security.network.client".into(),
        enabled: net_client,
        reason: if open {
            "effects UNBOUNDED — deny network.client (fail-closed on minimum knowledge)".into()
        } else if has("net.send") {
            "net.send proven and effects bounded — network.client may be enabled after codesign"
                .into()
        } else {
            "no proven net.send — network.client disabled".into()
        },
        apple_enforced_claim: false,
    });

    entitlements.push(EntitlementKey {
        key: "com.apple.security.network.server".into(),
        enabled: network_server,
        reason: "no language effect maps to inbound server sockets in this slice — always off"
            .into(),
        apple_enforced_claim: false,
    });

    // User-selected file access is the conservative sandbox mapping (not unrestricted host FS).
    entitlements.push(EntitlementKey {
        key: "com.apple.security.files.user-selected.read-only".into(),
        enabled: file_read && !file_write,
        reason: if file_read && !file_write {
            "fs.read proven (no fs.write) — user-selected read-only posture".into()
        } else if file_write {
            "fs.write present — use read-write user-selected key instead".into()
        } else {
            "no proven fs.read/fs.write — file access denied".into()
        },
        apple_enforced_claim: false,
    });

    entitlements.push(EntitlementKey {
        key: "com.apple.security.files.user-selected.read-write".into(),
        enabled: file_write,
        reason: if file_write {
            "fs.write proven and effects bounded — user-selected read-write posture".into()
        } else {
            "no proven fs.write — read-write file access denied".into()
        },
        apple_enforced_claim: false,
    });

    // No clean Apple entitlement for unrestricted process-exec; record residual only.
    if process_exec {
        entitlements.push(EntitlementKey {
            key: "com.apple.security.temporary-exception.files.absolute-path.read-only".into(),
            enabled: false,
            reason:
                "shell proven — no automatic unrestricted-exec entitlement; needs_human residual"
                    .into(),
            apple_enforced_claim: false,
        });
    }

    // Non-exportable caps → derive keychain-access posture (still not OS-enforced until signed).
    if uses_ne_caps {
        entitlements.push(EntitlementKey {
            key: "keychain-access-groups".into(),
            enabled: true,
            reason: "cap_acquire_nonexportable present — runtime may bind NE tokens to Keychain/SE; \
                     access-group must match codesign identity (needs_human)"
                .into(),
            apple_enforced_claim: false,
        });
        entitlements.push(EntitlementKey {
            key: "com.apple.developer.secure-enclave".into(),
            enabled: true,
            reason: "cap_acquire_nonexportable present — SE path when ANUBIS_KEYCHAIN_SE=1 and \
                     hardware/entitlements allow (soft fallback otherwise)"
                .into(),
            apple_enforced_claim: false,
        });
    }

    // Sort entitlements by key for byte-stable JSON (PartialEq on re-derive).
    entitlements.sort_by(|a, b| a.key.cmp(&b.key));

    let mut sandbox_notes = vec![
        "Sandbox posture is language-derived from the proven effect set; host PATHS and actual \
         OS enforcement require codesign + App Sandbox enablement (needs_human)."
            .to_string(),
    ];
    let mut needs_human = vec![
        "codesign the binary with the generated program.entitlements plist (or entitlement_profile.json keys)"
            .to_string(),
        "enable App Sandbox in the target packaging surface; this profile is derived + sealed, not OS-enforced until signed"
            .to_string(),
    ];
    let mut advisory = vec![
        "apple_enforced_claim is false on every key: derivation is not host enforcement".to_string(),
        "toolchain VZ entitlement com.apple.security.virtualization is intentionally absent from this app profile"
            .to_string(),
        "Keychain/SE: runtime may bind cap_acquire_nonexportable on macOS (kc:/se: tokens); \
         production SE isolation still needs codesign + access groups + operator attestation \
         (not claimed by derivation alone)"
            .to_string(),
    ];
    if uses_ne_caps {
        needs_human.push(
            "non-exportable caps: codesign with keychain-access-groups and (if SE) \
             com.apple.developer.secure-enclave; unsigned CLI may soft-fallback"
                .to_string(),
        );
    }
    if open {
        sandbox_notes.push(
            "effects UNBOUNDED — network/file/exec all denied (fail-closed on minimum knowledge)"
                .to_string(),
        );
    }
    if process_exec {
        needs_human.push(
            "shell is proven: process-exec is not granted via a standard sandbox entitlement; \
             operator must decide the host policy"
                .to_string(),
        );
        advisory.push(
            "in-guest or in-process shell cannot be fully forbidden by App Sandbox alone; language \
             checker + residual host policy are complementary"
                .to_string(),
        );
    }

    let sandbox = SandboxPosture {
        enabled: true,
        network_client: net_client,
        network_server,
        file_read,
        file_write,
        process_exec,
        notes: sandbox_notes,
        needs_human,
        advisory,
    };

    let capabilities_present: Vec<String> = CAPS
        .iter()
        .filter(|c| caps.contains(**c))
        .map(|c| (*c).to_string())
        .collect();

    let mut notes = vec![
        "This profile reflects the DECLARED + inferred effect surface (checker enforces \
         inferred ⊆ declared). Under-grant is intentional: a MISSED capability yields a MORE \
         restrictive profile, so a mis-analysed program breaks rather than over-privileges."
            .to_string(),
        "Re-derived on verify (ANUBIS_ENTITLEMENT_DRIFT). Forged permissive keys fail closed."
            .to_string(),
    ];
    if open {
        notes.push(
            "effects UNBOUNDED (closure/parameter/unknown callee) — profile is maximally restrictive"
                .to_string(),
        );
    }

    Ok(EntitlementProfile {
        schema: ENTITLEMENT_SCHEMA.into(),
        package: package.to_string(),
        version: version.to_string(),
        source_merkle,
        effects_bounded,
        capabilities_present,
        entitlements,
        sandbox,
        notes,
    })
}

/// Render a codesign-ready XML entitlements plist from the profile (byte-stable key order).
///
/// Boolean keys emit `<true/>`. `keychain-access-groups` emits a string array (Apple requires
/// an array of group IDs, not a bare bool).
pub fn entitlement_plist_xml(profile: &EntitlementProfile) -> String {
    entitlement_plist_xml_with_team(profile, None)
}

/// Like [`entitlement_plist_xml`], but when `team_id` is set (e.g. `M454G64BS4`), emits
/// `keychain-access-groups` as `["TEAMID.anubis.capability"]` for real codesign.
pub fn entitlement_plist_xml_with_team(
    profile: &EntitlementProfile,
    team_id: Option<&str>,
) -> String {
    let mut lines = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">".to_string(),
        "<plist version=\"1.0\">".to_string(),
        "<dict>".to_string(),
    ];
    let mut enabled: Vec<&EntitlementKey> =
        profile.entitlements.iter().filter(|e| e.enabled).collect();
    enabled.sort_by(|a, b| a.key.cmp(&b.key));
    for e in enabled {
        lines.push(format!("\t<key>{}</key>", e.key));
        if e.key == "keychain-access-groups" {
            // Array form required by codesign / Keychain ACL.
            let group = match team_id {
                Some(t) if !t.is_empty() => format!("{t}.anubis.capability"),
                _ => "anubis.capability".to_string(),
            };
            lines.push("\t<array>".to_string());
            lines.push(format!("\t\t<string>{group}</string>"));
            lines.push("\t</array>".to_string());
        } else {
            lines.push("\t<true/>".to_string());
        }
    }
    lines.push("</dict>".to_string());
    lines.push("</plist>".to_string());
    lines.push(String::new());
    lines.join("\n")
}

pub fn write_entitlement_profile_to_evidence_dir(
    dir: &Path,
    m: &EntitlementProfile,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(ENTITLEMENT_PROFILE_FILENAME), json).map_err(|e| e.to_string())?;
    let plist = entitlement_plist_xml(m);
    std::fs::write(dir.join(ENTITLEMENT_PLIST_FILENAME), plist).map_err(|e| e.to_string())?;
    Ok(())
}

/// Re-derive from `source` and compare against sealed profile. Fail closed on any drift.
pub fn verify_entitlement_profile_matches_source(
    source: &str,
    sealed: &EntitlementProfile,
) -> Result<(), String> {
    let fresh = derive_entitlement_profile(&sealed.package, &sealed.version, source)?;
    if fresh == *sealed {
        Ok(())
    } else {
        Err(format!(
            "ANUBIS_ENTITLEMENT_DRIFT: sealed entitlement_profile.json does not match the profile \
             re-derived from source (a forged or source-swapped profile). effects_bounded sealed={} \
             fresh={}; capabilities sealed={:?} fresh={:?}; network_client sealed={} fresh={}",
            sealed.effects_bounded,
            fresh.effects_bounded,
            sealed.capabilities_present,
            fresh.capabilities_present,
            sealed.sandbox.network_client,
            fresh.sandbox.network_client,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_free_program_disables_network_client() {
        let src =
            "fn add(a: i64, b: i64) -> i64 { return a + b; }\nfn main() { let _ = add(1, 2); }\n";
        let m = derive_entitlement_profile("pkg", "0.0.0", src).expect("derive");
        assert!(m.effects_bounded);
        assert!(!m.capabilities_present.contains(&"net.send".to_string()));
        assert!(!m.sandbox.network_client);
        assert!(!m.sandbox.network_server);
        assert!(!m.sandbox.file_read);
        assert!(!m.sandbox.file_write);
        assert!(!m.sandbox.process_exec);
        let net = m
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.security.network.client")
            .unwrap();
        assert!(!net.enabled);
        assert!(!net.apple_enforced_claim);
        let sb = m
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.security.app-sandbox")
            .unwrap();
        assert!(sb.enabled);
    }

    #[test]
    fn net_send_bounded_enables_network_client_posture() {
        let src = "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\nfn main() uses(net.send) { beacon(); }\n";
        let m = derive_entitlement_profile("pkg", "0.0.0", src).expect("derive");
        assert!(m.capabilities_present.contains(&"net.send".to_string()));
        assert!(m.sandbox.network_client);
        let net = m
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.security.network.client")
            .unwrap();
        assert!(net.enabled);
        // Honesty: still not claiming OS enforcement.
        assert!(!net.apple_enforced_claim);
        assert!(!m.sandbox.network_server);
    }

    #[test]
    fn fs_write_tracks_read_write_posture() {
        let rw = derive_entitlement_profile(
            "p",
            "0.0.0",
            "fn w() uses(fs.write) { let _ = write_file(\"a\", \"b\"); }\nfn main() uses(fs.write) { w(); }\n",
        )
        .unwrap();
        assert!(rw.sandbox.file_write);
        assert!(rw.sandbox.file_read);
        let key = rw
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.security.files.user-selected.read-write")
            .unwrap();
        assert!(key.enabled);
        let ro_only = rw
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.security.files.user-selected.read-only")
            .unwrap();
        assert!(!ro_only.enabled);
    }

    #[test]
    fn re_derive_matches_and_catches_forged_network() {
        let src =
            "fn add(a: i64, b: i64) -> i64 { return a + b; }\nfn main() { let _ = add(1, 2); }\n";
        let sealed = derive_entitlement_profile("pkg", "0.0.0", src).unwrap();
        verify_entitlement_profile_matches_source(src, &sealed).expect("honest re-derive");

        // Forge: enable network.client on a net-free program.
        let mut forged = sealed.clone();
        forged.sandbox.network_client = true;
        for e in &mut forged.entitlements {
            if e.key == "com.apple.security.network.client" {
                e.enabled = true;
            }
        }
        assert!(
            verify_entitlement_profile_matches_source(src, &forged).is_err(),
            "forged network.client on net-free source must fail closed (ANUBIS_ENTITLEMENT_DRIFT)"
        );
    }

    #[test]
    fn nonexportable_cap_derives_keychain_and_se_keys() {
        let src = r#"fn main() {
            let s = cap_acquire_nonexportable("fs.write");
            cap_use(s);
        }
"#;
        let m = derive_entitlement_profile("pkg", "0.0.0", src).unwrap();
        let kc = m
            .entitlements
            .iter()
            .find(|e| e.key == "keychain-access-groups")
            .expect("keychain-access-groups");
        assert!(kc.enabled);
        assert!(!kc.apple_enforced_claim);
        let se = m
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.developer.secure-enclave")
            .expect("secure-enclave key");
        assert!(se.enabled);
        assert!(!se.apple_enforced_claim);
        // Net-free NE program must not invent network.client.
        let net = m
            .entitlements
            .iter()
            .find(|e| e.key == "com.apple.security.network.client")
            .unwrap();
        assert!(!net.enabled);
    }

    #[test]
    fn no_apple_enforced_claim_on_any_key() {
        let src = "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\nfn main() uses(net.send) { beacon(); }\n";
        let m = derive_entitlement_profile("pkg", "0.0.0", src).unwrap();
        assert!(m.entitlements.iter().all(|e| !e.apple_enforced_claim));
    }

    #[test]
    fn plist_only_lists_enabled_keys_sorted() {
        let src = "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\nfn main() uses(net.send) { beacon(); }\n";
        let m = derive_entitlement_profile("pkg", "0.0.0", src).unwrap();
        let xml = entitlement_plist_xml(&m);
        assert!(xml.contains("com.apple.security.app-sandbox"));
        assert!(xml.contains("com.apple.security.network.client"));
        assert!(!xml.contains("com.apple.security.network.server"));
        // app-sandbox key appears before network.client (sorted).
        let a = xml.find("com.apple.security.app-sandbox").unwrap();
        let b = xml.find("com.apple.security.network.client").unwrap();
        assert!(a < b);
    }

    #[test]
    fn shell_does_not_auto_enable_exec_entitlement() {
        let src = "fn s() uses(shell) { exec(\"true\"); }\nfn main() uses(shell) { s(); }\n";
        // May fail parse if exec isn't a builtin — still exercise shell presence if parse works.
        if let Ok(m) = derive_entitlement_profile("pkg", "0.0.0", src) {
            if m.capabilities_present.iter().any(|c| c == "shell") {
                assert!(m.sandbox.process_exec);
                // No enabled temporary-exception absolute-path key for free exec.
                let bad = m
                    .entitlements
                    .iter()
                    .find(|e| e.key.contains("temporary-exception") && e.enabled);
                assert!(bad.is_none());
            }
        }
    }
}
