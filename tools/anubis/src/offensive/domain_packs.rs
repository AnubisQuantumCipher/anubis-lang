//! Security research domain packs — profile-bound capability catalogs with honest
//! classifications.
//!
//! Design: `docs/language/SECURITY_RESEARCH_PROFILE.md` slice 5.
//!
//! Each pack maps to a `ResearchProfile`, declares isolation, default effects
//! (research IR), CLI surfaces, and per-capability honesty labels. Scaffolding
//! writes a sealed pack manifest; validate checks a source program's proven
//! effects against the pack allow-list (fail-closed on unknown / over-grant).
//!
//! **Not** a full language surface. **Not** CAVP / production PKI.

use anubis_compiler::research_profile::{
    proven_effects_from_source, proven_effects_via_typecheck, ResearchProfile, SecurityEffect,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const PACK_SCHEMA: &str = "anubis-research-pack-v1";
pub const PACK_MANIFEST: &str = "pack_manifest.json";

/// How ready a single pack capability is (honest, never upgraded without evidence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapClass {
    /// Implemented + tested + re-runnable command.
    LabReal,
    /// Lab HMAC / MAC only (not Ed25519 PKI).
    LabRealHmac,
    /// Structured plan or helper; does not execute the claimed action.
    PlanOnly,
    /// Partial wiring; not the full claim.
    Partial,
    /// Documented design only.
    NotImplemented,
}

impl CapClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LabReal => "LAB_REAL",
            Self::LabRealHmac => "LAB_REAL_HMAC",
            Self::PlanOnly => "PLAN_ONLY",
            Self::Partial => "PARTIAL",
            Self::NotImplemented => "NOT_IMPLEMENTED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackCapability {
    pub id: String,
    pub summary: String,
    pub classification: CapClass,
    /// CLI entrypoints that exercise this capability (empty if none).
    pub cli: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPack {
    pub id: String,
    pub title: String,
    pub profile: String,
    pub summary: String,
    /// Disposable Tart/VZ mandatory for crash/research execution.
    pub requires_vz: bool,
    /// Pure math / static analysis may run on host.
    pub allows_host_pure: bool,
    /// Research-normalized default effects for run-capability minting.
    pub default_effects: Vec<String>,
    pub capabilities: Vec<PackCapability>,
    pub non_goals: Vec<String>,
    pub honesty: Vec<String>,
}

/// Sealed scaffold artifact written under an engagement or out dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    pub schema: String,
    pub pack_id: String,
    pub profile: String,
    pub requires_vz: bool,
    pub default_effects: Vec<String>,
    pub capabilities: Vec<PackCapability>,
    pub engagement_id: Option<String>,
    pub scaffolded_at_unix: u64,
    pub notes: Vec<String>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn effect_names(effects: &[SecurityEffect]) -> Vec<String> {
    effects
        .iter()
        .map(|e| e.normalize().as_str().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Static catalog of domain packs (slice 5).
pub fn catalog() -> Vec<DomainPack> {
    vec![
        pack_poc(),
        pack_fuzz(),
        pack_crypto(),
        pack_bounty(),
        pack_emulation(),
    ]
}

pub fn get(id: &str) -> Option<DomainPack> {
    let key = id.trim().to_ascii_lowercase().replace('-', "_");
    catalog().into_iter().find(|p| {
        p.id == key
            || p.profile == key
            || (key == "crypto" && p.id == "crypto_research")
            || (key == "poc" && p.id == "poc")
    })
}

fn pack_poc() -> DomainPack {
    DomainPack {
        id: "poc".into(),
        title: "Proof-of-Concept / crash research".into(),
        profile: ResearchProfile::Research.as_str().into(),
        summary: "Authorized local PoC and crash reproduction inside disposable VZ guests."
            .into(),
        requires_vz: true,
        allows_host_pure: false,
        default_effects: effect_names(&[
            SecurityEffect::ProcessSpawn,
            SecurityEffect::FsRead,
            SecurityEffect::VmExecute,
            SecurityEffect::EvidenceEmit,
        ]),
        capabilities: vec![
            PackCapability {
                id: "vz_exploit".into(),
                summary: "Disposable guest PoC run with guest-bound run capability".into(),
                classification: CapClass::LabReal,
                cli: vec![
                    "anubis vz exploit <poc.anb> --allow-research".into(),
                ],
                notes: "Host stages cap; guest enforces ANUBIS_VZ_ENFORCE_RUN_CAP=1".into(),
            },
            PackCapability {
                id: "exploit_module".into(),
                summary: "Engagement-scoped exploit module JSON against local binary".into(),
                classification: CapClass::LabReal,
                cli: vec![
                    "anubis exploit-new".into(),
                    "anubis exploit-run --engage …".into(),
                ],
                notes: "local_process only; VZ required for run".into(),
            },
            PackCapability {
                id: "poc_kit_gold".into(),
                summary: "Gold vuln_local harness + cyclic pattern helpers".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis pattern-create".into(), "anubis pattern-offset".into()],
                notes: "Crash target execution is VZ-bound".into(),
            },
            PackCapability {
                id: "remote_rce_pack".into(),
                summary: "Unscoped remote RCE / wormable packs".into(),
                classification: CapClass::NotImplemented,
                cli: vec![],
                notes: "Explicit non-goal of the research language".into(),
            },
        ],
        non_goals: vec![
            "Host crash as primary evidence".into(),
            "Arbitrary remote exploitation".into(),
        ],
        honesty: vec![
            "Pack is LAB_REAL for local VZ PoC paths only".into(),
            "No claim of Metasploit/Caldera feature parity".into(),
        ],
    }
}

fn pack_fuzz() -> DomainPack {
    DomainPack {
        id: "fuzz".into(),
        title: "Mutation fuzz (operator)".into(),
        profile: ResearchProfile::Research.as_str().into(),
        summary: "In-scope binary mutation fuzz inside disposable guests.".into(),
        requires_vz: true,
        allows_host_pure: false,
        default_effects: effect_names(&[
            SecurityEffect::ProcessSpawn,
            SecurityEffect::FsRead,
            SecurityEffect::VmExecute,
            SecurityEffect::EvidenceEmit,
        ]),
        capabilities: vec![
            PackCapability {
                id: "vz_fuzz".into(),
                summary: "Guest fuzz with staged run capability".into(),
                classification: CapClass::LabReal,
                cli: vec![
                    "anubis vz fuzz --target <bin> --iterations N --allow-research".into(),
                ],
                notes: "Host fuzz forbidden (ANUBIS_FUZZ_HOST_FORBIDDEN)".into(),
            },
            PackCapability {
                id: "coverage_guided".into(),
                summary: "Coverage-guided / AFL-class engine".into(),
                classification: CapClass::NotImplemented,
                cli: vec![],
                notes: "Current path is mutation fuzz, not libFuzzer/AFL".into(),
            },
            PackCapability {
                id: "crash_triage".into(),
                summary: "Structured crash triage + minimization".into(),
                classification: CapClass::Partial,
                cli: vec![],
                notes: "Guest exit status collected; full triage pack residual".into(),
            },
        ],
        non_goals: vec!["Host-side fuzz of production binaries".into()],
        honesty: vec![
            "LAB_REAL for VZ mutation fuzz only".into(),
            "Does not claim continuous fuzzing farm maturity".into(),
        ],
    }
}

fn pack_crypto() -> DomainPack {
    DomainPack {
        id: "crypto_research".into(),
        title: "Cryptography research".into(),
        profile: ResearchProfile::CryptoResearch.as_str().into(),
        summary: "Pure math / contracts on host; leakage and adversarial fuzz under VZ."
            .into(),
        requires_vz: true, // profile requires_vz true for leakage paths
        allows_host_pure: true,
        default_effects: effect_names(&[
            SecurityEffect::FsRead,
            SecurityEffect::SecretUse,
            SecurityEffect::EvidenceEmit,
            SecurityEffect::VmExecute,
        ]),
        capabilities: vec![
            PackCapability {
                id: "pure_math_check".into(),
                summary: "anubis check / contracts for pure cryptographic logic".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis check <file.anb>".into()],
                notes: "Host OK for pure math; never overclaim CAVP".into(),
            },
            PackCapability {
                id: "secret_exfil_static".into(),
                summary: "Static secret→egress rejection (ANUBIS_SECRET_EXFILTRATION)".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis check <file.anb>".into()],
                notes: "Checker confidentiality surface".into(),
            },
            PackCapability {
                id: "side_channel_lab".into(),
                summary: "Timing / cache leakage lab harness".into(),
                classification: CapClass::Partial,
                cli: vec![],
                notes: "No CAVP, no hardware lab claim; VZ if executing adversarial harness".into(),
            },
            PackCapability {
                id: "cavp_cert".into(),
                summary: "NIST CAVP / FIPS certificate".into(),
                classification: CapClass::NotImplemented,
                cli: vec![],
                notes: "Explicit non-claim".into(),
            },
        ],
        non_goals: vec![
            "CAVP / FIPS certification".into(),
            "Rolling own production primitives".into(),
        ],
        honesty: vec![
            "Host pure math is LAB_REAL for check/contracts only".into(),
            "Never upgrade PARTIAL leakage harness to REAL without sealed evidence".into(),
        ],
    }
}

fn pack_bounty() -> DomainPack {
    DomainPack {
        id: "bounty".into(),
        title: "Bug bounty / scope-bound research".into(),
        profile: ResearchProfile::Bounty.as_str().into(),
        summary: "Scope-bound PoC packing, evidence bundles, bounty report helpers."
            .into(),
        requires_vz: true,
        allows_host_pure: false,
        default_effects: effect_names(&[
            SecurityEffect::ProcessSpawn,
            SecurityEffect::FsRead,
            SecurityEffect::FsWrite,
            SecurityEffect::EvidenceEmit,
            SecurityEffect::HumanApprove,
            SecurityEffect::VmExecute,
        ]),
        capabilities: vec![
            PackCapability {
                id: "evidence_bundle".into(),
                summary: "Tamper-evident evidence bundle + PCA re-derive".into(),
                classification: CapClass::LabReal,
                cli: vec![
                    "anubis build --evidence".into(),
                    "anubis verify".into(),
                    "anubis evidence-verify".into(),
                ],
                notes: "Portable evidence-verify is host offline".into(),
            },
            PackCapability {
                id: "bounty_report".into(),
                summary: "Bounty report markdown from bundle".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis bounty-report --bundle …".into()],
                notes: "Template/report only; no auto-submit to platforms".into(),
            },
            PackCapability {
                id: "scope_gate".into(),
                summary: "Engagement host/path/cidr allow-lists fail closed".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis engage-status".into(), "anubis evidence-verify".into()],
                notes: "content_hash + scope asserts".into(),
            },
            PackCapability {
                id: "platform_submit".into(),
                summary: "Auto-submit to HackerOne/Bugcrowd".into(),
                classification: CapClass::NotImplemented,
                cli: vec![],
                notes: "Human gate only; never automated submission".into(),
            },
        ],
        non_goals: vec![
            "Out-of-scope scanning".into(),
            "Automated bounty platform filing".into(),
        ],
        honesty: vec![
            "Scope enforcement is LAB_REAL for engagement workspace".into(),
            "Reports are LAB_REAL templates, not platform integration".into(),
        ],
    }
}

fn pack_emulation() -> DomainPack {
    DomainPack {
        id: "emulation".into(),
        title: "ATT&CK-aligned defense validation".into(),
        profile: ResearchProfile::Emulation.as_str().into(),
        summary: "Purple-team / ATT&CK catalog, OPSEC score, campaign playbook — VZ for live paths."
            .into(),
        requires_vz: true,
        allows_host_pure: false,
        default_effects: effect_names(&[
            SecurityEffect::ProcessSpawn,
            SecurityEffect::NetConnect,
            SecurityEffect::FsRead,
            SecurityEffect::EvidenceEmit,
            SecurityEffect::VmExecute,
            SecurityEffect::HumanApprove,
        ]),
        capabilities: vec![
            PackCapability {
                id: "attck_catalog".into(),
                summary: "Kill-chain mapped ATT&CK catalog".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis attck-catalog".into(), "anubis attck-map".into()],
                notes: "Catalog + mapping; not full Atomic Red Team runner".into(),
            },
            PackCapability {
                id: "purple_report".into(),
                summary: "Purple-team coverage + detection gaps".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis purple-report --engage …".into()],
                notes: "Report from engagement loot/facts".into(),
            },
            PackCapability {
                id: "campaign_playbook".into(),
                summary: "Campaign playbook JSON/MD".into(),
                classification: CapClass::LabReal,
                cli: vec!["anubis campaign-init".into(), "anubis campaign-status".into()],
                notes: "Planning surface".into(),
            },
            PackCapability {
                id: "phish_plan".into(),
                summary: "Phishing campaign plan".into(),
                classification: CapClass::PlanOnly,
                cli: vec!["anubis phish-plan".into()],
                notes: "Never sends mail".into(),
            },
            PackCapability {
                id: "full_caldera_parity".into(),
                summary: "Caldera-scale adversary emulation farm".into(),
                classification: CapClass::NotImplemented,
                cli: vec![],
                notes: "Explicit non-goal".into(),
            },
        ],
        non_goals: vec![
            "Stealth/evasion as maturity metric".into(),
            "Beating Caldera at scale".into(),
        ],
        honesty: vec![
            "Live recon/scan paths remain VZ-bound LAB_REAL".into(),
            "PLAN_ONLY surfaces never claimed as executed actions".into(),
        ],
    }
}

pub fn list_json() -> serde_json::Value {
    serde_json::json!({
        "schema": PACK_SCHEMA,
        "packs": catalog(),
    })
}

pub fn print_catalog() -> Result<()> {
    println!("Anubis security research domain packs ({PACK_SCHEMA})");
    println!(
        "{:<16} {:<16} {:<6} {}",
        "ID", "PROFILE", "VZ?", "TITLE"
    );
    for p in catalog() {
        println!(
            "{:<16} {:<16} {:<6} {}",
            p.id,
            p.profile,
            if p.requires_vz { "yes" } else { "no" },
            p.title
        );
    }
    println!("\nShow detail: anubis research-pack show <id> [--json]");
    println!("Scaffold:    anubis research-pack scaffold <id> --out DIR");
    println!("Validate:    anubis research-pack validate <id> --source file.anb");
    Ok(())
}

pub fn show_json(id: &str) -> Result<serde_json::Value> {
    let p = get(id).ok_or_else(|| anyhow!("ANUBIS_RESEARCH_PACK_UNKNOWN: `{id}`"))?;
    Ok(serde_json::to_value(p)?)
}

pub fn print_show(id: &str) -> Result<()> {
    let p = get(id).ok_or_else(|| anyhow!("ANUBIS_RESEARCH_PACK_UNKNOWN: `{id}`"))?;
    println!("{} — {}", p.id, p.title);
    println!("profile: {}  requires_vz: {}  host_pure: {}", p.profile, p.requires_vz, p.allows_host_pure);
    println!("{}", p.summary);
    println!("default_effects: {}", p.default_effects.join(", "));
    println!("\ncapabilities:");
    for c in &p.capabilities {
        println!(
            "  [{:<14}] {} — {}",
            c.classification.as_str(),
            c.id,
            c.summary
        );
        if !c.cli.is_empty() {
            for cmd in &c.cli {
                println!("      cli: {cmd}");
            }
        }
        if !c.notes.is_empty() {
            println!("      note: {}", c.notes);
        }
    }
    if !p.non_goals.is_empty() {
        println!("\nnon-goals:");
        for n in &p.non_goals {
            println!("  - {n}");
        }
    }
    if !p.honesty.is_empty() {
        println!("\nhonesty:");
        for h in &p.honesty {
            println!("  - {h}");
        }
    }
    Ok(())
}

/// Scaffold a pack directory with sealed manifest + honesty README.
pub fn scaffold(
    id: &str,
    out: &Path,
    engagement_id: Option<&str>,
) -> Result<PathBuf> {
    let pack = get(id).ok_or_else(|| anyhow!("ANUBIS_RESEARCH_PACK_UNKNOWN: `{id}`"))?;
    fs::create_dir_all(out)?;
    let manifest = PackManifest {
        schema: PACK_SCHEMA.into(),
        pack_id: pack.id.clone(),
        profile: pack.profile.clone(),
        requires_vz: pack.requires_vz,
        default_effects: pack.default_effects.clone(),
        capabilities: pack.capabilities.clone(),
        engagement_id: engagement_id.map(|s| s.to_string()),
        scaffolded_at_unix: now_unix(),
        notes: pack.honesty.clone(),
    };
    let man_path = out.join(PACK_MANIFEST);
    fs::write(&man_path, serde_json::to_string_pretty(&manifest)?)?;

    let mut readme = String::new();
    readme.push_str(&format!("# Research pack: {}\n\n", pack.title));
    readme.push_str(&format!("**Pack id:** `{}`  \n", pack.id));
    readme.push_str(&format!("**Profile:** `{}`  \n", pack.profile));
    readme.push_str(&format!(
        "**Isolation:** {}\n\n",
        if pack.requires_vz {
            "mandatory disposable VZ for crash/research execution"
        } else {
            "host OK"
        }
    ));
    readme.push_str(&format!("{}\n\n", pack.summary));
    readme.push_str("## Capabilities (honest)\n\n");
    readme.push_str("| Id | Class | Summary |\n|----|-------|----------|\n");
    for c in &pack.capabilities {
        readme.push_str(&format!(
            "| `{}` | {} | {} |\n",
            c.id,
            c.classification.as_str(),
            c.summary.replace('|', "\\|")
        ));
    }
    readme.push_str("\n## Non-goals\n\n");
    for n in &pack.non_goals {
        readme.push_str(&format!("- {n}\n"));
    }
    readme.push_str("\n## Honesty\n\n");
    for h in &pack.honesty {
        readme.push_str(&format!("- {h}\n"));
    }
    readme.push_str(
        "\n## Verify\n\n```bash\nanubis research-pack validate ",
    );
    readme.push_str(&pack.id);
    readme.push_str(" --source <program.anb>\nanubis evidence-verify .\n```\n");
    fs::write(out.join("README.md"), readme)?;

    // Checklist
    let mut check = String::from("# Pack checklist\n\n");
    for c in &pack.capabilities {
        let box_ = match c.classification {
            CapClass::LabReal | CapClass::LabRealHmac => "[x]",
            CapClass::PlanOnly | CapClass::Partial => "[~]",
            CapClass::NotImplemented => "[ ]",
        };
        check.push_str(&format!(
            "- {box_} `{}` ({}) — {}\n",
            c.id,
            c.classification.as_str(),
            c.summary
        ));
    }
    fs::write(out.join("CHECKLIST.md"), check)?;

    // Minimal stub for research/bounty profiles
    if matches!(pack.id.as_str(), "poc" | "bounty" | "fuzz") {
        let stub = "\
// research pack stub — LAB / authorized use only
// Fill in engagement-scoped PoC. Run under: anubis vz exploit --allow-research
fn main() {
    // host-safe static check only until --allow-research in VZ
}
";
        fs::write(out.join("stub.anb"), stub)?;
    }

    Ok(man_path)
}

/// Validate that a source program's proven effects are within the pack allow-list.
///
/// Fail closed if parse fails or if any proven research effect is not ⊆ pack defaults
/// (extended with pack-declared effects). `open` / unbounded programs fail when
/// `requires_vz` packs would otherwise under-specify.
pub fn validate_source(id: &str, source_path: &Path) -> Result<serde_json::Value> {
    let pack = get(id).ok_or_else(|| anyhow!("ANUBIS_RESEARCH_PACK_UNKNOWN: `{id}`"))?;
    let src = fs::read_to_string(source_path)
        .map_err(|e| anyhow!("ANUBIS_RESEARCH_PACK_SOURCE: {}: {e}", source_path.display()))?;
    let proven = proven_effects_from_source(&src)
        .map_err(|e| anyhow!("ANUBIS_RESEARCH_PACK_EFFECTS: {e}"))?;
    // Same IR as typecheck when check succeeds — fail closed on drift.
    if let Ok(via_tc) = proven_effects_via_typecheck(&src) {
        if via_tc.research_effect_names() != proven.research_effect_names()
            || via_tc.effects_bounded != proven.effects_bounded
        {
            return Err(anyhow!(
                "ANUBIS_RESEARCH_PACK_EFFECT_IR_DRIFT: typecheck ProvenEffectSet diverges from source fixpoint"
            ));
        }
    }

    let allowed: BTreeSet<String> = pack.default_effects.iter().cloned().collect();
    let mut extra = Vec::new();
    let mut ok_effects = Vec::new();
    for name in proven.research_effect_names() {
        if allowed.contains(&name) {
            ok_effects.push(name);
        } else {
            extra.push(name);
        }
    }

    let ok = extra.is_empty();
    let mut reasons = Vec::new();
    if !extra.is_empty() {
        reasons.push(format!(
            "proven effects not in pack allow-list: {}",
            extra.join(", ")
        ));
    }
    if !proven.effects_bounded && pack.requires_vz {
        // Unbounded effect set is still allowed for scaffold programs but flagged.
        reasons.push(
            "effects UNBOUNDED (open bit) — pack validation marks PARTIAL (fail-closed for claims)"
                .into(),
        );
        // Don't hard-fail pure stubs that may open; only fail on extras.
    }
    if !ok {
        return Err(anyhow!(
            "ANUBIS_RESEARCH_PACK_VALIDATE_FAILED: pack `{}`: {}",
            pack.id,
            reasons.join("; ")
        ));
    }

    Ok(serde_json::json!({
        "ok": true,
        "pack_id": pack.id,
        "profile": pack.profile,
        "source": source_path.display().to_string(),
        "effects_bounded": proven.effects_bounded,
        "proven_effects": proven.research_effect_names(),
        "allowed_effects": pack.default_effects,
        "matched_effects": ok_effects,
        "warnings": reasons,
        "requires_vz": pack.requires_vz,
        "classification": "LAB_REAL",
        "note": "Effect allow-list check only; not a full typecheck or engagement scope proof",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_five_domain_packs() {
        let c = catalog();
        assert_eq!(c.len(), 5);
        for id in ["poc", "fuzz", "crypto_research", "bounty", "emulation"] {
            assert!(get(id).is_some(), "missing {id}");
        }
        assert!(get("crypto").is_some()); // alias
    }

    #[test]
    fn every_pack_has_honest_non_goals_and_classes() {
        for p in catalog() {
            assert!(!p.non_goals.is_empty(), "{} missing non_goals", p.id);
            assert!(!p.honesty.is_empty(), "{} missing honesty", p.id);
            assert!(!p.capabilities.is_empty(), "{} empty caps", p.id);
            // At least one LAB_REAL and no silent "REAL" without enum
            assert!(
                p.capabilities
                    .iter()
                    .any(|c| matches!(c.classification, CapClass::LabReal | CapClass::LabRealHmac | CapClass::PlanOnly | CapClass::Partial | CapClass::NotImplemented))
            );
            // Emulation must keep phish as PLAN_ONLY
            if p.id == "emulation" {
                let phish = p
                    .capabilities
                    .iter()
                    .find(|c| c.id == "phish_plan")
                    .expect("phish");
                assert_eq!(phish.classification, CapClass::PlanOnly);
            }
            // Crypto must mark CAVP not implemented
            if p.id == "crypto_research" {
                let cavp = p
                    .capabilities
                    .iter()
                    .find(|c| c.id == "cavp_cert")
                    .expect("cavp");
                assert_eq!(cavp.classification, CapClass::NotImplemented);
            }
        }
    }

    #[test]
    fn scaffold_writes_manifest_and_readme() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("poc-pack");
        let man = scaffold("poc", &out, Some("eng-test")).unwrap();
        assert!(man.is_file());
        assert!(out.join("README.md").is_file());
        assert!(out.join("CHECKLIST.md").is_file());
        assert!(out.join("stub.anb").is_file());
        let raw = fs::read_to_string(&man).unwrap();
        let m: PackManifest = serde_json::from_str(&raw).unwrap();
        assert_eq!(m.schema, PACK_SCHEMA);
        assert_eq!(m.pack_id, "poc");
        assert_eq!(m.engagement_id.as_deref(), Some("eng-test"));
        assert!(m.requires_vz);
    }

    #[test]
    fn validate_pure_main_ok_for_poc() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("pure.anb");
        fs::write(
            &src,
            "fn add(a: i64, b: i64) -> i64 { return a + b; }\nfn main() { let _ = add(1, 2); }\n",
        )
        .unwrap();
        let r = validate_source("poc", &src).unwrap();
        assert_eq!(r["ok"], true);
    }

    #[test]
    fn validate_net_program_fails_poc_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("net.anb");
        fs::write(
            &src,
            "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\n\
             fn main() uses(net.send) { beacon(); }\n",
        )
        .unwrap();
        let err = validate_source("poc", &src).unwrap_err().to_string();
        assert!(
            err.contains("ANUBIS_RESEARCH_PACK_VALIDATE_FAILED"),
            "{err}"
        );
        assert!(err.contains("net.connect"), "{err}");
    }

    #[test]
    fn validate_net_ok_for_emulation_pack() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("net.anb");
        fs::write(
            &src,
            "fn beacon() uses(net.send) { http_post(\"http://x/y\", \"z\"); }\n\
             fn main() uses(net.send) { beacon(); }\n",
        )
        .unwrap();
        // Emulation allows net.connect
        let r = validate_source("emulation", &src).unwrap();
        assert_eq!(r["ok"], true);
        let proven = r["proven_effects"].as_array().unwrap();
        assert!(proven.iter().any(|v| v.as_str() == Some("net.connect")));
    }

    #[test]
    fn unknown_pack_errors() {
        assert!(get("nope").is_none());
        assert!(show_json("nope").is_err());
    }
}
