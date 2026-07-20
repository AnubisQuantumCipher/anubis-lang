//! VZ confinement manifest — a hypervisor confinement policy DERIVED from the language's own proven
//! capability set, sealed in evidence and re-derived on verify (fail-closed).
//!
//! Schema `anubis.confinement.v1`. The checker already PROVES a whole-program capability set (the six
//! canonical caps in `middle::effects` — `fs.read`, `fs.write`, `net.send`, `shell`, `time.now`,
//! `rand.gen`) via the transitive effect fixpoint unioned with declared `uses(...)`. Until now that
//! proof was consumed for diagnostics and discarded. This module turns the SAME proven set into a
//! deterministic, source-only-re-derivable manifest that maps each capability to a concrete Apple
//! Virtualization (tart) hypervisor grant — a real SECOND, independent boundary whose grant is
//! consistent-by-construction with what `anubis check` proved, and re-derived + byte-compared on
//! verify so a grant that contradicts the proven effect set fails closed.
//!
//! HONESTY (the load-bearing property). The manifest reflects the DECLARED + inferred effect surface
//! (the checker enforces `inferred ⊆ declared`). The effect fixpoint may under-approximate
//! higher-order / closure flows — but the confinement lattice is monotone in the SAFE direction: a
//! MISSED capability yields a MORE restrictive grant (host-only network, no mounts), so a
//! mis-analysed guest breaks rather than leaks. The hypervisor is the backstop for exactly the
//! undeclared flows the static analysis might miss. And when the effect set is UNBOUNDED (`open`), we
//! confine MOST restrictively, never permissively — deny on minimum knowledge.
//!
//! tart cannot enforce everything an ideal confinement would: it has no zero-NIC air-gap, no
//! per-hostname egress, and cannot gate an in-guest shell. Every grant records `tart_enforced` and
//! the residual `advisory` / `needs_human` honestly; anything requiring the native
//! objc2-virtualization FFI is marked `[NEEDS-HUMAN]` (entitlement + signing identity).

use crate::frontend::parse_source;
use crate::package::merkle;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFINEMENT_FILENAME: &str = "confinement_manifest.json";
pub const CONFINEMENT_SCHEMA: &str = "anubis.confinement.v1";

/// The six canonical capabilities, in the order they appear in the manifest (deterministic).
const CAPS: [&str; 6] = [
    "net.send",
    "fs.read",
    "fs.write",
    "shell",
    "time.now",
    "rand.gen",
];

/// One capability's derived hypervisor grant. `tart_enforced` is TRUE only when the tart CLI
/// verifiably applies this grant on the host; otherwise the grant is advisory or needs the native
/// FFI, recorded in `advisory` / `needs_human` — never a fabricated enforcement claim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityGrant {
    pub capability: String,
    pub present: bool,
    /// e.g. `network:host-only`, `network:unrestricted-nat`, `mount:read-only`, `mount:none`,
    /// `informational:in-guest-shell-not-hypervisor-gated`.
    pub hypervisor_grant: String,
    pub tart_enforced: bool,
    /// The engagement-INDEPENDENT tart args this capability implies (e.g. `--net-host`). Path/CIDR
    /// args are engagement-derived and live in the applied manifest (slice-2), not here.
    pub tart_args: Vec<String>,
    pub advisory: Vec<String>,
    pub needs_human: Vec<String>,
}

/// The language-core confinement manifest: a pure, engagement-free function of source alone, so it is
/// re-derivable and cannot drift from the proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfinementManifest {
    pub schema: String,
    pub package: String,
    pub version: String,
    pub source_merkle: String,
    /// False when the effect fixpoint's `open` bit was set (a closure / parameter / unknown callee).
    /// When false the grants default to the MOST restrictive posture (fail-closed).
    pub effects_bounded: bool,
    /// The subset of the six capabilities the checker proved present, sorted.
    pub capabilities_present: Vec<String>,
    pub grants: Vec<CapabilityGrant>,
    pub notes: Vec<String>,
}

/// Derive the confinement manifest from source. Fails closed: a parse failure returns `Err` and emits
/// no manifest (a program that does not parse gets no derived grants).
pub fn derive_confinement(
    package: &str,
    version: &str,
    source: &str,
) -> Result<ConfinementManifest, String> {
    let source_merkle = merkle::sha256_hex(source.as_bytes());
    let ast = parse_source(source)
        .map_err(|e| format!("ANUBIS_CONFINE_PARSE_FAILED: confinement derivation parse failed: {e}"))?;
    let (caps, open) = crate::middle::effects::program_capability_set(&ast.items);

    let has = |c: &str| caps.contains(c);
    let effects_bounded = !open;

    // Network posture. MUST-FIX #3: an UNBOUNDED effect set (`open`) confines MOST restrictively —
    // host-only, never the permissive NAT default. Otherwise: a proven-net-free program is confined to
    // host-only; a program that declares net.send gets the (permissive, honest) NAT default, whose
    // tightening to an allow-list is engagement-side (slice-2 applied manifest).
    let net_present = has("net.send");
    let network_host_only = open || !net_present;

    // Filesystem posture. Unbounded => none. Else fs.write => read-write, fs.read => read-only, else
    // none. The actual host PATHS are engagement-supplied (applied manifest); the core records posture.
    let mount_posture = if open {
        "none"
    } else if has("fs.write") {
        "read-write"
    } else if has("fs.read") {
        "read-only"
    } else {
        "none"
    };

    let mut grants = Vec::new();
    for cap in CAPS {
        let present = has(cap);
        let (grant, enforced, args, advisory, needs_human): (
            String,
            bool,
            Vec<String>,
            Vec<String>,
            Vec<String>,
        ) = match cap {
            "net.send" => {
                if network_host_only {
                    (
                        "network:host-only".into(),
                        true,
                        vec!["--net-host".into()],
                        vec![
                            "--net-host still lets the guest reach the HOST and other host-only guests \
                             (a potential exfil channel), so it is not a true air-gap."
                                .into(),
                        ],
                        vec![
                            "a true zero-NIC air-gap for a proven-net-free program is FULLY ENFORCED by \
                             the native backend `anubis vz native-preflight` (objc2-virtualization, zero \
                             network devices) on a binary signed with `scripts/build_signed_anubis.sh` — \
                             an ad-hoc signature suffices for local use, so this is no longer [NEEDS-HUMAN] \
                             (only notarization-for-distribution is). tart's `--net-host` (the enforced \
                             grant here) is the host-only fallback where the native lane is unavailable."
                                .into(),
                        ],
                    )
                } else {
                    (
                        "network:unrestricted-nat".into(),
                        false,
                        vec![],
                        vec![
                            "net.send is declared, so the guest gets tart's default NAT (full internet \
                             egress). Restricting egress to an allow-list needs an engagement scope + \
                             Softnet (applied manifest, slice-2)."
                                .into(),
                        ],
                        vec![],
                    )
                }
            }
            "fs.read" | "fs.write" => {
                let g = format!("mount:{mount_posture}");
                if mount_posture == "none" {
                    (g, true, vec![], vec![], vec![])
                } else {
                    (
                        g,
                        false,
                        vec![],
                        vec![],
                        vec![
                            "the host PATHS to mount are engagement-supplied; without --engagement \
                             allowed_paths no host directory is exposed (fail-closed). Applied mounts \
                             (--dir=<tag>:<path>[:ro]) land in the applied manifest, slice-2."
                                .into(),
                        ],
                    )
                }
            }
            "shell" => (
                "informational:in-guest-shell-not-hypervisor-gated".into(),
                false,
                vec![],
                vec![
                    "a shell inside a full-OS guest cannot be forbidden by tart; the LANGUAGE checker \
                     gates shell. The two boundaries are complementary, not redundant."
                        .into(),
                ],
                vec![],
            ),
            _ => (
                "informational:not-a-vm-confinement-dimension".into(),
                false,
                vec![],
                vec![],
                vec![],
            ),
        };
        grants.push(CapabilityGrant {
            capability: cap.to_string(),
            present,
            hypervisor_grant: grant,
            tart_enforced: enforced,
            tart_args: args,
            advisory,
            needs_human,
        });
    }

    let mut notes = vec![
        "This manifest reflects the DECLARED + inferred effect surface (the checker enforces \
         inferred ⊆ declared). The effect fixpoint may under-approximate higher-order/closure flows; \
         the hypervisor boundary is the backstop for undeclared flows. Safety relies on the \
         miss-direction being fail-closed: a MISSED capability yields a MORE restrictive grant \
         (host-only / mount:none), so a mis-analysed guest breaks rather than leaks."
            .to_string(),
        "tart cannot give a guest zero network devices, cannot restrict egress by hostname (IPv4 CIDR \
         only via Softnet), and cannot gate an in-guest shell. The native backend `anubis vz \
         native-preflight` (objc2-virtualization) closes the first two: a proven-net-free program gets a \
         real zero-NIC air-gap, and a net-using program gets a per-hostname egress substrate — on a \
         binary signed with `scripts/build_signed_anubis.sh` (an ad-hoc signature suffices locally, so \
         NOT [NEEDS-HUMAN]; only notarization-for-distribution is). An in-guest shell remains a \
         full-OS-guest concern gated by the LANGUAGE checker, not the hypervisor."
            .to_string(),
    ];
    if open {
        notes.push(
            "effects UNBOUNDED (a closure/parameter/unknown callee set the effect open bit) — \
             confining MOST restrictively (host-only network + mount:none). A permissive posture is \
             an explicit operator opt-in, never the default (fail-closed on minimum knowledge)."
                .to_string(),
        );
    }

    let capabilities_present: Vec<String> = CAPS
        .iter()
        .filter(|c| caps.contains(**c))
        .map(|c| c.to_string())
        .collect();

    Ok(ConfinementManifest {
        schema: CONFINEMENT_SCHEMA.into(),
        package: package.to_string(),
        version: version.to_string(),
        source_merkle,
        effects_bounded,
        capabilities_present,
        grants,
        notes,
    })
}

pub fn write_confinement_to_evidence_dir(dir: &Path, m: &ConfinementManifest) -> Result<(), String> {
    let json = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(CONFINEMENT_FILENAME), json).map_err(|e| e.to_string())
}

/// Re-derive the confinement manifest from `source` and byte-compare against a `sealed` manifest —
/// the consistent-BY-CONSTRUCTION cross-check (MUST-FIX #4). A hash-consistent but forged grant (one
/// that claims e.g. `network:host-only` while the source proves `net.send`) fails closed here: the
/// re-derivation is a pure function of the source, so it cannot be made to agree with a dishonest
/// sealed manifest. Package name/version/merkle are re-derived from the same source, so only the
/// grant semantics are compared for drift.
pub fn verify_confinement_matches_source(
    source: &str,
    sealed: &ConfinementManifest,
) -> Result<(), String> {
    let fresh = derive_confinement(&sealed.package, &sealed.version, source)?;
    if fresh == *sealed {
        Ok(())
    } else {
        Err(format!(
            "ANUBIS_CONFINE_DRIFT: sealed confinement_manifest.json does not match the grant re-derived \
             from source (a forged or source-swapped grant). effects_bounded sealed={} fresh={}; \
             capabilities sealed={:?} fresh={:?}",
            sealed.effects_bounded,
            fresh.effects_bounded,
            sealed.capabilities_present,
            fresh.capabilities_present,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_free_program_confines_to_host_only() {
        let src = "fn add(a: i64, b: i64) -> i64 { return a + b; }\nfn main() { let _ = add(1, 2); }\n";
        let m = derive_confinement("pkg", "0.0.0", src).expect("derive");
        assert!(m.effects_bounded, "no closures => bounded");
        assert!(!m.capabilities_present.contains(&"net.send".to_string()));
        let net = m.grants.iter().find(|g| g.capability == "net.send").unwrap();
        assert_eq!(net.hypervisor_grant, "network:host-only");
        assert!(net.tart_enforced);
        assert_eq!(net.tart_args, vec!["--net-host".to_string()]);
        // Honest residual recorded (MUST-FIX #5).
        assert!(net.advisory.iter().any(|a| a.contains("HOST")));
        assert!(net.needs_human.iter().any(|n| n.contains("air-gap")));
    }

    #[test]
    fn net_program_gets_unrestricted_nat_recorded_honestly() {
        let src = "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\nfn main() uses(net.send) { beacon(); }\n";
        let m = derive_confinement("pkg", "0.0.0", src).expect("derive");
        assert!(m.capabilities_present.contains(&"net.send".to_string()));
        let net = m.grants.iter().find(|g| g.capability == "net.send").unwrap();
        assert_eq!(net.hypervisor_grant, "network:unrestricted-nat");
        assert!(!net.tart_enforced, "NAT default is permissive, not a confinement");
    }

    #[test]
    fn fs_posture_tracks_read_vs_write() {
        let ro = derive_confinement(
            "p",
            "0.0.0",
            "fn r() uses(fs.read) { let _ = read_file(\"a\"); }\nfn main() uses(fs.read) { r(); }\n",
        )
        .unwrap();
        let g = ro.grants.iter().find(|g| g.capability == "fs.read").unwrap();
        assert_eq!(g.hypervisor_grant, "mount:read-only");
        let rw = derive_confinement(
            "p",
            "0.0.0",
            "fn w() uses(fs.write) { let _ = write_file(\"a\", \"b\"); }\nfn main() uses(fs.write) { w(); }\n",
        )
        .unwrap();
        let g = rw.grants.iter().find(|g| g.capability == "fs.write").unwrap();
        assert_eq!(g.hypervisor_grant, "mount:read-write");
    }

    #[test]
    fn re_derive_matches_and_catches_a_forged_grant() {
        let src = "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\nfn main() uses(net.send) { beacon(); }\n";
        let sealed = derive_confinement("pkg", "0.0.0", src).unwrap();
        // Honest re-derive matches.
        verify_confinement_matches_source(src, &sealed).expect("honest manifest re-derives");
        // Forge the grant: claim host-only for a net-sending program.
        let mut forged = sealed.clone();
        for g in &mut forged.grants {
            if g.capability == "net.send" {
                g.hypervisor_grant = "network:host-only".into();
                g.tart_args = vec!["--net-host".into()];
            }
        }
        assert!(
            verify_confinement_matches_source(src, &forged).is_err(),
            "a forged host-only grant over a net.send source must fail closed"
        );
    }
}
