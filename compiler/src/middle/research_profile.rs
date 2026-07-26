//! Phase 3: Security Research HIR types (compiler-side stubs).
//!
//! Design doc: `docs/language/SECURITY_RESEARCH_PROFILE.md`.
//!
//! This module defines the **typed intermediate objects** that the research pipeline
//! will eventually require at the language surface. It is intentionally pure and
//! self-contained (no filesystem, no network, no Tart): constructors fail closed
//! when engagement/scope inputs are missing or inconsistent.
//!
//! ## Classification (honest)
//!
//! | Piece | Status |
//! |-------|--------|
//! | Profile / effect / scope types | LAB_REAL (typed IR + unit tests) |
//! | Language syntax / parser surface | NOT_IMPLEMENTED |
//! | Checker wiring into `typecheck` | NOT_IMPLEMENTED (types ready for consume) |
//! | Runtime mint of GuestRun | runtime spine in `tools/anubis` (LAB_REAL) |
//!
//! Raw strings/paths/hosts must not silently become scoped targets — all scoped
//! constructors require an engagement binding and an allow-list check.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Compiler-facing research execution profile.
///
/// Distinct from `frontend::Mode` (Safe/Research/Exploit parse modes): profiles
/// capture the *security research domain* (PoC, emulation, crypto, bounty) and
/// isolation obligations from the design doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchProfile {
    /// Default: host OK; no net/process mutation/FFI.
    Safe,
    /// Opt-in: PoC / crash / fuzz / debug — mandatory VZ.
    Research,
    /// Opt-in: ATT&CK-aligned defense validation — mandatory VZ.
    Emulation,
    /// Opt-in: pure math may host; leakage/fuzz needs VZ.
    CryptoResearch,
    /// Opt-in: scope-bound only; no arbitrary RCE.
    Bounty,
}

impl ResearchProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Research => "research",
            Self::Emulation => "emulation",
            Self::CryptoResearch => "crypto_research",
            Self::Bounty => "bounty",
        }
    }

    /// Parse a profile name (case-insensitive, snake or kebab).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "safe" => Some(Self::Safe),
            "research" => Some(Self::Research),
            "emulation" | "emulate" => Some(Self::Emulation),
            "crypto_research" | "crypto" => Some(Self::CryptoResearch),
            "bounty" => Some(Self::Bounty),
            _ => None,
        }
    }

    /// Whether disposable Tart/VZ isolation is **mandatory** for this profile.
    pub fn requires_vz(self) -> bool {
        matches!(
            self,
            Self::Research | Self::Emulation | Self::CryptoResearch | Self::Bounty
        )
    }

    /// Host-side pure math is allowed for crypto_research and safe only.
    pub fn allows_host_pure(self) -> bool {
        matches!(self, Self::Safe | Self::CryptoResearch)
    }
}

impl fmt::Display for ResearchProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Normalized security-research effect ids shared by checker, runtime, and VZ confine.
///
/// Extends the six gated capability ids used by Safe-mode with research-domain
/// effects from the design doc. Serialization uses the dotted form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityEffect {
    NetConnect,
    NetListen,
    FsRead,
    FsWrite,
    ProcessSpawn,
    ProcessInspect,
    DebugAttach,
    VmExecute,
    SecretUse,
    EvidenceEmit,
    HumanApprove,
    /// Legacy Safe-mode shell capability (maps to process.spawn for research IR).
    Shell,
    /// Legacy net.send (maps to net.connect for research IR).
    NetSend,
}

impl SecurityEffect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NetConnect => "net.connect",
            Self::NetListen => "net.listen",
            Self::FsRead => "fs.read",
            Self::FsWrite => "fs.write",
            Self::ProcessSpawn => "process.spawn",
            Self::ProcessInspect => "process.inspect",
            Self::DebugAttach => "debug.attach",
            Self::VmExecute => "vm.execute",
            Self::SecretUse => "secret.use",
            Self::EvidenceEmit => "evidence.emit",
            Self::HumanApprove => "human.approve",
            Self::Shell => "shell",
            Self::NetSend => "net.send",
        }
    }

    /// Parse a dotted effect name to a SecurityEffect (fail-closed → None).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "net.connect" | "connect" => Some(Self::NetConnect),
            "net.listen" | "listen" => Some(Self::NetListen),
            "fs.read" | "file_read" | "read_file" => Some(Self::FsRead),
            "fs.write" | "file_write" | "write_file" => Some(Self::FsWrite),
            "process.spawn" | "proc.spawn" => Some(Self::ProcessSpawn),
            "process.inspect" | "proc.inspect" => Some(Self::ProcessInspect),
            "debug.attach" => Some(Self::DebugAttach),
            "vm.execute" | "vm.exec" => Some(Self::VmExecute),
            "secret.use" => Some(Self::SecretUse),
            "evidence.emit" => Some(Self::EvidenceEmit),
            "human.approve" => Some(Self::HumanApprove),
            "shell" | "exec" | "system" | "target_run" => Some(Self::Shell),
            "net.send" | "network" | "send" => Some(Self::NetSend),
            _ => None,
        }
    }

    /// Canonical research IR form (legacy shell/net.send fold into research effects).
    pub fn normalize(self) -> Self {
        match self {
            Self::Shell => Self::ProcessSpawn,
            Self::NetSend => Self::NetConnect,
            other => other,
        }
    }

    /// Effects that are crash/research class and require guest-bound run capability.
    pub fn requires_run_capability(self) -> bool {
        matches!(
            self.normalize(),
            Self::ProcessSpawn
                | Self::ProcessInspect
                | Self::DebugAttach
                | Self::VmExecute
                | Self::NetListen
        )
    }
}

impl fmt::Display for SecurityEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Engagement identity bound into typed HIR (no full engagement file load).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngagementRef {
    pub engagement_id: String,
    pub engagement_hash: String,
    pub authorization_digest: String,
    pub kill_date_iso: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_paths: Vec<String>,
    pub allowed_cidrs: Vec<String>,
}

impl EngagementRef {
    /// Construct only when identity fields are non-empty (fail closed).
    pub fn new(
        engagement_id: impl Into<String>,
        engagement_hash: impl Into<String>,
        authorization_digest: impl Into<String>,
        kill_date_iso: impl Into<String>,
        allowed_hosts: Vec<String>,
        allowed_paths: Vec<String>,
        allowed_cidrs: Vec<String>,
    ) -> Result<Self, ResearchProfileError> {
        let engagement_id = engagement_id.into();
        let engagement_hash = engagement_hash.into();
        let authorization_digest = authorization_digest.into();
        let kill_date_iso = kill_date_iso.into();
        if engagement_id.trim().is_empty() {
            return Err(ResearchProfileError::MissingEngagementId);
        }
        if engagement_hash.trim().is_empty() {
            return Err(ResearchProfileError::MissingEngagementHash);
        }
        if authorization_digest.trim().is_empty() {
            return Err(ResearchProfileError::MissingAuthorization);
        }
        if kill_date_iso.trim().is_empty() {
            return Err(ResearchProfileError::MissingKillDate);
        }
        Ok(Self {
            engagement_id,
            engagement_hash,
            authorization_digest,
            kill_date_iso,
            allowed_hosts,
            allowed_paths,
            allowed_cidrs,
        })
    }

    pub fn host_allowed(&self, host: &str) -> bool {
        let h = host.trim().to_ascii_lowercase();
        if h.is_empty() {
            return false;
        }
        self.allowed_hosts
            .iter()
            .any(|a| a.trim().eq_ignore_ascii_case(&h) || a.trim() == "*")
    }

    pub fn path_allowed(&self, path: &str) -> bool {
        let p = path.trim();
        if p.is_empty() {
            return false;
        }
        // Exact or prefix match against engagement path allow-list.
        self.allowed_paths.iter().any(|a| {
            let a = a.trim();
            p == a || p.starts_with(&format!("{a}/")) || p.starts_with(a)
        })
    }
}

/// Path that has been proven in-scope for an engagement.
///
/// Cannot be constructed from a bare string without going through
/// [`ScopedPath::bind`] (private field + constructor gate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedPath {
    path: String,
    engagement_id: String,
    engagement_hash: String,
}

impl ScopedPath {
    /// Bind a path only if the engagement allow-list contains it.
    pub fn bind(path: &str, eng: &EngagementRef) -> Result<Self, ResearchProfileError> {
        let path = path.trim();
        if path.is_empty() {
            return Err(ResearchProfileError::EmptyPath);
        }
        if !eng.path_allowed(path) {
            return Err(ResearchProfileError::PathOutOfScope {
                path: path.into(),
                engagement_id: eng.engagement_id.clone(),
            });
        }
        Ok(Self {
            path: path.into(),
            engagement_id: eng.engagement_id.clone(),
            engagement_hash: eng.engagement_hash.clone(),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn engagement_id(&self) -> &str {
        &self.engagement_id
    }

    pub fn engagement_hash(&self) -> &str {
        &self.engagement_hash
    }
}

/// Host that has been proven in-scope for an engagement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedHost {
    host: String,
    engagement_id: String,
    engagement_hash: String,
}

impl ScopedHost {
    pub fn bind(host: &str, eng: &EngagementRef) -> Result<Self, ResearchProfileError> {
        let host = host.trim();
        if host.is_empty() {
            return Err(ResearchProfileError::EmptyHost);
        }
        if !eng.host_allowed(host) {
            return Err(ResearchProfileError::HostOutOfScope {
                host: host.into(),
                engagement_id: eng.engagement_id.clone(),
            });
        }
        Ok(Self {
            host: host.into(),
            engagement_id: eng.engagement_id.clone(),
            engagement_hash: eng.engagement_hash.clone(),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn engagement_id(&self) -> &str {
        &self.engagement_id
    }
}

/// Authorization charter digest (opaque; not a free-form string at call sites).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authorization {
    digest: String,
    engagement_id: String,
}

impl Authorization {
    pub fn from_engagement(eng: &EngagementRef) -> Self {
        Self {
            digest: eng.authorization_digest.clone(),
            engagement_id: eng.engagement_id.clone(),
        }
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn engagement_id(&self) -> &str {
        &self.engagement_id
    }
}

/// Provenance marker: value is verified (has evidence binding) vs unverified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLabel {
    Verified,
    Unverified,
    LabReal,
    PlanOnly,
}

/// Typed evidence envelope (payload + hash + trust label).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence<T> {
    pub payload: T,
    pub content_hash: String,
    pub trust: TrustLabel,
    pub engagement_id: String,
}

impl<T> Evidence<T> {
    pub fn lab_real(payload: T, content_hash: impl Into<String>, engagement_id: impl Into<String>) -> Self {
        Self {
            payload,
            content_hash: content_hash.into(),
            trust: TrustLabel::LabReal,
            engagement_id: engagement_id.into(),
        }
    }

    pub fn plan_only(payload: T, engagement_id: impl Into<String>) -> Self {
        Self {
            payload,
            content_hash: String::new(),
            trust: TrustLabel::PlanOnly,
            engagement_id: engagement_id.into(),
        }
    }
}

/// Guest-bound run plan — what the host orchestrator mints into a run capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestRun {
    pub profile: ResearchProfile,
    pub engagement_id: String,
    pub engagement_hash: String,
    pub program_digest: String,
    pub guest_id: String,
    pub allowed_effects: BTreeSet<String>,
    pub requires_run_capability: bool,
}

impl GuestRun {
    /// Build a guest run plan. Fail closed if profile requires VZ but guest_id empty,
    /// or if crash-class effects are present without run-capability obligation.
    pub fn plan(
        profile: ResearchProfile,
        eng: &EngagementRef,
        program_digest: impl Into<String>,
        guest_id: impl Into<String>,
        effects: &[SecurityEffect],
    ) -> Result<Self, ResearchProfileError> {
        let program_digest = program_digest.into();
        let guest_id = guest_id.into();
        if program_digest.trim().is_empty() {
            return Err(ResearchProfileError::MissingProgramDigest);
        }
        if profile.requires_vz() && guest_id.trim().is_empty() {
            return Err(ResearchProfileError::MissingGuestId {
                profile: profile.as_str().into(),
            });
        }
        let mut allowed_effects = BTreeSet::new();
        let mut needs_cap = false;
        for e in effects {
            let n = e.normalize();
            if n.requires_run_capability() {
                needs_cap = true;
            }
            allowed_effects.insert(n.as_str().to_string());
        }
        // Research profiles always require vm.execute when VZ-bound.
        if profile.requires_vz() {
            allowed_effects.insert(SecurityEffect::VmExecute.as_str().into());
            needs_cap = true;
        }
        Ok(Self {
            profile,
            engagement_id: eng.engagement_id.clone(),
            engagement_hash: eng.engagement_hash.clone(),
            program_digest,
            guest_id,
            allowed_effects,
            requires_run_capability: needs_cap,
        })
    }

    pub fn effect_list(&self) -> Vec<String> {
        self.allowed_effects.iter().cloned().collect()
    }
}

/// Fail-closed errors for research-profile constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResearchProfileError {
    MissingEngagementId,
    MissingEngagementHash,
    MissingAuthorization,
    MissingKillDate,
    MissingProgramDigest,
    MissingGuestId { profile: String },
    EmptyPath,
    EmptyHost,
    PathOutOfScope { path: String, engagement_id: String },
    HostOutOfScope { host: String, engagement_id: String },
    UnknownProfile { raw: String },
    UnknownEffect { raw: String },
}

impl fmt::Display for ResearchProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEngagementId => write!(f, "ANUBIS_RESEARCH_PROFILE: missing engagement_id"),
            Self::MissingEngagementHash => {
                write!(f, "ANUBIS_RESEARCH_PROFILE: missing engagement_hash")
            }
            Self::MissingAuthorization => {
                write!(f, "ANUBIS_RESEARCH_PROFILE: missing authorization digest")
            }
            Self::MissingKillDate => write!(f, "ANUBIS_RESEARCH_PROFILE: missing kill_date"),
            Self::MissingProgramDigest => {
                write!(f, "ANUBIS_RESEARCH_PROFILE: missing program_digest")
            }
            Self::MissingGuestId { profile } => write!(
                f,
                "ANUBIS_RESEARCH_PROFILE: profile `{profile}` requires a non-empty guest_id (VZ mandatory)"
            ),
            Self::EmptyPath => write!(f, "ANUBIS_RESEARCH_PROFILE: empty path"),
            Self::EmptyHost => write!(f, "ANUBIS_RESEARCH_PROFILE: empty host"),
            Self::PathOutOfScope {
                path,
                engagement_id,
            } => write!(
                f,
                "ANUBIS_RESEARCH_SCOPE: path `{path}` not in engagement `{engagement_id}` allow-list"
            ),
            Self::HostOutOfScope {
                host,
                engagement_id,
            } => write!(
                f,
                "ANUBIS_RESEARCH_SCOPE: host `{host}` not in engagement `{engagement_id}` allow-list"
            ),
            Self::UnknownProfile { raw } => {
                write!(f, "ANUBIS_RESEARCH_PROFILE: unknown profile `{raw}`")
            }
            Self::UnknownEffect { raw } => {
                write!(f, "ANUBIS_RESEARCH_PROFILE: unknown effect `{raw}`")
            }
        }
    }
}

impl std::error::Error for ResearchProfileError {}

/// Normalize a list of effect name strings into research IR (drops unknowns fail-closed via Result).
pub fn normalize_effect_set(raw: &[&str]) -> Result<BTreeSet<SecurityEffect>, ResearchProfileError> {
    let mut out = BTreeSet::new();
    for r in raw {
        match SecurityEffect::parse(r) {
            Some(e) => {
                out.insert(e.normalize());
            }
            None => {
                return Err(ResearchProfileError::UnknownEffect {
                    raw: (*r).to_string(),
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab_eng() -> EngagementRef {
        EngagementRef::new(
            "eng-lab-1",
            "hash-abc",
            "auth-digest-xyz",
            "2099-01-01T00:00:00Z",
            vec!["127.0.0.1".into(), "target.lab".into()],
            vec!["/tmp/anubis-lab".into(), "/opt/poc".into()],
            vec!["10.0.0.0/8".into()],
        )
        .expect("lab eng")
    }

    #[test]
    fn profile_parse_and_vz_obligation() {
        assert_eq!(ResearchProfile::parse("research"), Some(ResearchProfile::Research));
        assert_eq!(
            ResearchProfile::parse("crypto-research"),
            Some(ResearchProfile::CryptoResearch)
        );
        assert!(ResearchProfile::Research.requires_vz());
        assert!(ResearchProfile::Bounty.requires_vz());
        assert!(!ResearchProfile::Safe.requires_vz());
        assert!(ResearchProfile::Safe.allows_host_pure());
        assert!(ResearchProfile::CryptoResearch.allows_host_pure());
        assert!(!ResearchProfile::Research.allows_host_pure());
        assert!(ResearchProfile::parse("nope").is_none());
    }

    #[test]
    fn engagement_ref_rejects_empty_identity() {
        assert!(matches!(
            EngagementRef::new("", "h", "a", "k", vec![], vec![], vec![]),
            Err(ResearchProfileError::MissingEngagementId)
        ));
        assert!(matches!(
            EngagementRef::new("e", "", "a", "k", vec![], vec![], vec![]),
            Err(ResearchProfileError::MissingEngagementHash)
        ));
        assert!(matches!(
            EngagementRef::new("e", "h", "", "k", vec![], vec![], vec![]),
            Err(ResearchProfileError::MissingAuthorization)
        ));
    }

    #[test]
    fn scoped_path_fails_closed_out_of_scope() {
        let eng = lab_eng();
        let ok = ScopedPath::bind("/tmp/anubis-lab/poc.anb", &eng).expect("in scope");
        assert_eq!(ok.path(), "/tmp/anubis-lab/poc.anb");
        assert_eq!(ok.engagement_id(), "eng-lab-1");

        let err = ScopedPath::bind("/etc/passwd", &eng).unwrap_err();
        assert!(matches!(err, ResearchProfileError::PathOutOfScope { .. }));
        assert!(err.to_string().contains("ANUBIS_RESEARCH_SCOPE"));

        assert!(matches!(
            ScopedPath::bind("", &eng),
            Err(ResearchProfileError::EmptyPath)
        ));
    }

    #[test]
    fn scoped_host_fails_closed_out_of_scope() {
        let eng = lab_eng();
        let ok = ScopedHost::bind("target.lab", &eng).expect("in scope");
        assert_eq!(ok.host(), "target.lab");
        let err = ScopedHost::bind("evil.example", &eng).unwrap_err();
        assert!(matches!(err, ResearchProfileError::HostOutOfScope { .. }));
    }

    #[test]
    fn guest_run_requires_guest_for_research_profile() {
        let eng = lab_eng();
        let err = GuestRun::plan(
            ResearchProfile::Research,
            &eng,
            "deadbeef",
            "",
            &[SecurityEffect::ProcessSpawn],
        )
        .unwrap_err();
        assert!(matches!(err, ResearchProfileError::MissingGuestId { .. }));

        let plan = GuestRun::plan(
            ResearchProfile::Research,
            &eng,
            "deadbeef",
            "guest-xyz",
            &[SecurityEffect::Shell, SecurityEffect::FsRead],
        )
        .expect("plan");
        assert!(plan.requires_run_capability);
        assert!(plan.allowed_effects.contains("process.spawn")); // shell normalized
        assert!(plan.allowed_effects.contains("fs.read"));
        assert!(plan.allowed_effects.contains("vm.execute"));
    }

    #[test]
    fn safe_profile_no_guest_ok() {
        let eng = lab_eng();
        let plan = GuestRun::plan(
            ResearchProfile::Safe,
            &eng,
            "deadbeef",
            "",
            &[SecurityEffect::FsRead],
        )
        .expect("safe plan");
        assert!(!plan.requires_run_capability);
        assert!(!plan.allowed_effects.contains("vm.execute"));
    }

    #[test]
    fn security_effect_parse_and_normalize() {
        assert_eq!(
            SecurityEffect::parse("shell").map(|e| e.normalize()),
            Some(SecurityEffect::ProcessSpawn)
        );
        assert_eq!(
            SecurityEffect::parse("net.send").map(|e| e.normalize()),
            Some(SecurityEffect::NetConnect)
        );
        assert!(SecurityEffect::ProcessSpawn.requires_run_capability());
        assert!(!SecurityEffect::FsRead.requires_run_capability());
        assert!(SecurityEffect::parse("bogus.effect").is_none());
    }

    #[test]
    fn normalize_effect_set_fail_closed() {
        let ok = normalize_effect_set(&["fs.read", "shell"]).unwrap();
        assert!(ok.contains(&SecurityEffect::FsRead));
        assert!(ok.contains(&SecurityEffect::ProcessSpawn));
        assert!(matches!(
            normalize_effect_set(&["fs.read", "not.an.effect"]),
            Err(ResearchProfileError::UnknownEffect { .. })
        ));
    }

    #[test]
    fn evidence_trust_labels_honest() {
        let eng = lab_eng();
        let e = Evidence::lab_real("finding", "abc123", &eng.engagement_id);
        assert_eq!(e.trust, TrustLabel::LabReal);
        let p = Evidence::plan_only("smb plan", &eng.engagement_id);
        assert_eq!(p.trust, TrustLabel::PlanOnly);
        assert!(p.content_hash.is_empty());
    }

    #[test]
    fn authorization_from_engagement() {
        let eng = lab_eng();
        let a = Authorization::from_engagement(&eng);
        assert_eq!(a.digest(), "auth-digest-xyz");
        assert_eq!(a.engagement_id(), "eng-lab-1");
    }
}
