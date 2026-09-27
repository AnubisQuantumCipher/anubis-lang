//! Project model: the typed `Anubis.toml` manifest and the on-disk project layout.
//!
//! Phase 0 of the ecosystem work. Two pieces:
//!
//! - [`AnubisManifest`] — a real, `toml`-crate-parsed manifest (replacing the former hand-rolled
//!   line matcher). It carries `[package]` / `[dependencies]` (consumed by the Phase 6 package
//!   manager) alongside the existing `[backend.*]` / `[evidence]` config, all with serde defaults
//!   so a missing or minimal manifest is valid.
//!
//! - [`ProjectLayout`] — discovers the enclosing project for an entry `.anb` file (walking up to
//!   an `Anubis.toml`) and computes the `src_root` that Phase 1's multi-file module resolver maps
//!   module paths against. A file invoked with no manifest degenerates to a synthetic single-file
//!   project, preserving today's `anubis run foo.anb` behavior exactly.
//!
//! Fail-closed: a present-but-malformed `Anubis.toml` is a hard error (`ANUBIS_MANIFEST_PARSE`),
//! never silently ignored.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The language edition a package is written against.
///
/// SemVer versions the *implementation*; an edition versions the *language*. The
/// distinction is what lets a language change its mind without breaking code
/// that already exists: a package keeps declaring the edition it was written
/// for, the compiler keeps supporting every edition it has ever shipped, and a
/// later edition is free to reject what an earlier one accepted. Rust's editions
/// are the model.
///
/// Two rules here are chosen deliberately and are the whole point:
///
/// 1. **An unrecognised edition is refused, never assumed.** A compiler that
///    meets `edition = "2031"` does not know what that means, so compiling it
///    under today's rules would silently give the program semantics its author
///    never asked for. Refusing is the only safe answer, and it is what makes it
///    possible to add a future edition without endangering anything built now.
///
/// 2. **A missing edition is a warning now and an error at 1.0.** Rust defaulted
///    editionless manifests to 2015 and carries that default permanently. Anubis
///    can still avoid that debt because essentially no third-party code exists
///    yet; after 1.0 ships it becomes impossible, which is why the decision
///    belongs here rather than later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Edition {
    /// The first edition. The 1.0 frozen surface.
    E2026,
}

impl Edition {
    /// The edition assumed when a manifest declares none. Removed at 1.0, when
    /// the key becomes mandatory — see [`Edition`].
    pub const DEFAULT: Edition = Edition::E2026;

    /// Every edition this compiler understands, oldest first.
    pub const ALL: &'static [Edition] = &[Edition::E2026];

    pub fn as_str(self) -> &'static str {
        match self {
            Edition::E2026 => "2026",
        }
    }

    /// Parse a declared edition. Unknown values are refused with the reason and
    /// the list of editions this compiler actually supports, so the failure tells
    /// the reader whether to upgrade the compiler or fix the manifest.
    pub fn parse(text: &str) -> Result<Edition, String> {
        match text.trim() {
            "2026" => Ok(Edition::E2026),
            other => Err(format!(
                "ANUBIS_EDITION_UNKNOWN: `edition = \"{other}\"` is not an edition this compiler \
                 understands (supported: {}). A newer edition is refused rather than compiled \
                 under older rules, because that would give the program semantics its author did \
                 not ask for. Upgrade the compiler, or declare a supported edition.",
                Edition::ALL
                    .iter()
                    .map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

impl std::fmt::Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The parsed `Anubis.toml`. Every section defaults, so both an absent manifest and a
/// backend-only manifest (today's `Anubis.toml.example`) deserialize cleanly.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct AnubisManifest {
    pub package: PackageMeta,
    pub dependencies: BTreeMap<String, DepSpec>,
    pub backend: BackendConfig,
    pub evidence: EvidenceConfig,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    /// The declared language edition, verbatim. Validated by
    /// [`AnubisManifest::edition`]; kept as a string here so an unknown value
    /// produces a named refusal rather than a serde type error.
    #[serde(default)]
    pub edition: Option<String>,
    /// Project-local trusted dependency signers (`[package.trust] signers = [...]`).
    #[serde(default)]
    pub trust: PackageTrust,
}

/// Trusted Ed25519 verifying keys declared in the project manifest.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct PackageTrust {
    #[serde(default)]
    pub signers: Vec<String>,
}

/// A dependency spec: either a bare version string (`math = "1.2"`) or a detailed table
/// (`{ path = "..." }`, `{ git = "...", rev = "..." }`, `{ version = "1.2" }`). Consumed by the
/// Phase 6 resolver; parsed now so manifests are forward-compatible.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DepSpec {
    Version(String),
    Detailed {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        git: Option<String>,
        #[serde(default)]
        rev: Option<String>,
        /// Optional registry base (`file:///…` or `https://…`) overriding the default local registry.
        #[serde(default)]
        registry: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct BackendConfig {
    pub risc0: Risc0Config,
    pub risc0_metal: Risc0MetalConfig,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Risc0Config {
    pub enabled: bool,
    pub version: String,
    pub prove_mode: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Risc0MetalConfig {
    pub enabled: bool,
    pub reference_path: String,
    pub vendored_patch_path: String,
    pub require_tier2_metal: bool,
    pub allow_cpu_fallback: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct EvidenceConfig {
    pub schema_version: String,
    pub require_manifest: bool,
    pub require_tamper_check: bool,
}

/// The canonical manifest filename.
pub const MANIFEST_FILENAME: &str = "Anubis.toml";

impl AnubisManifest {
    /// Parse manifest text. A malformed manifest is a hard, fail-closed error.
    pub fn parse(text: &str) -> Result<AnubisManifest, String> {
        let manifest: AnubisManifest =
            toml::from_str(text).map_err(|e| format!("ANUBIS_MANIFEST_PARSE: {e}"))?;
        // Validate eagerly: a manifest naming an edition this compiler does not
        // understand must fail here, not at some later point where the program
        // has already been compiled under the wrong rules.
        manifest.edition()?;
        Ok(manifest)
    }

    /// The resolved language edition.
    ///
    /// An unrecognised edition is an error. A missing one resolves to
    /// [`Edition::DEFAULT`] today and becomes an error at 1.0; use
    /// [`AnubisManifest::edition_is_declared`] to warn about it in the meantime.
    pub fn edition(&self) -> Result<Edition, String> {
        match self.package.edition.as_deref() {
            Some(text) => Edition::parse(text),
            None => Ok(Edition::DEFAULT),
        }
    }

    /// Whether the manifest states its edition rather than inheriting the default.
    pub fn edition_is_declared(&self) -> bool {
        self.package.edition.is_some()
    }

    /// Load and parse the manifest at `path`.
    pub fn load(path: &Path) -> Result<AnubisManifest, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("ANUBIS_MANIFEST_PARSE: cannot read {}: {e}", path.display()))?;
        Self::parse(&text)
    }

    /// The configured risc0 metal-hybrid reference path, if set (the documented
    /// `[backend.risc0_metal].reference_path`). `None` when empty.
    pub fn metal_reference_path(&self) -> Option<PathBuf> {
        let p = self.backend.risc0_metal.reference_path.trim();
        (!p.is_empty()).then(|| PathBuf::from(p))
    }
}

/// The on-disk shape of the project an entry file belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectLayout {
    /// Project root — the directory containing `Anubis.toml`, or the entry file's directory for a
    /// manifest-less single-file invocation.
    pub root: PathBuf,
    /// The manifest path, if one was discovered.
    pub manifest_path: Option<PathBuf>,
    /// The parsed manifest (default when none was found).
    pub manifest: AnubisManifest,
    /// The source root that module paths resolve against — `root/src` when it exists, else `root`.
    pub src_root: PathBuf,
    /// The entry source file.
    pub entry: PathBuf,
    /// True when there is no enclosing manifest (a lone `.anb` file).
    pub single_file: bool,
}

impl ProjectLayout {
    /// Discover the project layout for an entry `.anb` file: walk up from its directory looking for
    /// an `Anubis.toml`. When found, that directory is the root and (if present) `root/src` is the
    /// module source root. When none is found, synthesize a single-file project rooted at the
    /// entry's directory — preserving today's `anubis run foo.anb` semantics.
    ///
    /// A discovered-but-malformed manifest is a fail-closed error.
    pub fn discover(entry: &Path) -> Result<ProjectLayout, String> {
        let entry = entry.to_path_buf();
        let start = entry
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut dir: Option<&Path> = Some(start.as_path());
        while let Some(d) = dir {
            let candidate = d.join(MANIFEST_FILENAME);
            if candidate.is_file() {
                let manifest = AnubisManifest::load(&candidate)?;
                let root = d.to_path_buf();
                let src = root.join("src");
                let src_root = if src.is_dir() { src } else { root.clone() };
                return Ok(ProjectLayout {
                    root,
                    manifest_path: Some(candidate),
                    manifest,
                    src_root,
                    entry,
                    single_file: false,
                });
            }
            dir = d.parent();
        }

        // No manifest anywhere up the tree: a lone source file is its own project.
        Ok(ProjectLayout {
            root: start.clone(),
            manifest_path: None,
            manifest: AnubisManifest::default(),
            src_root: start,
            entry,
            single_file: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
[package]
name = "demo"
version = "0.1.0"
description = "a demo package"
authors = ["Anubis"]

[dependencies]
math = "1.2"
utils = { path = "../utils" }
crypto = { git = "https://example/c.git", rev = "abc123" }

[backend.risc0]
enabled = true
version = "3.0.5"
prove_mode = "in-process"

[backend.risc0_metal]
enabled = true
reference_path = "/opt/metal-hybrid-prover"
require_tier2_metal = true
allow_cpu_fallback = false

[evidence]
schema_version = "1.0"
require_manifest = true
require_tamper_check = true
"#;

    #[test]
    fn parses_a_full_manifest_with_package_and_dependencies() {
        let m = AnubisManifest::parse(FULL).expect("parse");
        assert_eq!(m.package.name, "demo");
        assert_eq!(m.package.version, "0.1.0");
        assert_eq!(m.dependencies.len(), 3);
        assert_eq!(m.dependencies["math"], DepSpec::Version("1.2".into()));
        assert!(matches!(
            &m.dependencies["utils"],
            DepSpec::Detailed { path: Some(p), .. } if p == "../utils"
        ));
        assert!(matches!(
            &m.dependencies["crypto"],
            DepSpec::Detailed { git: Some(_), rev: Some(r), .. } if r == "abc123"
        ));
        assert!(m.backend.risc0.enabled);
        assert_eq!(
            m.metal_reference_path(),
            Some(PathBuf::from("/opt/metal-hybrid-prover"))
        );
        assert!(m.evidence.require_tamper_check);
    }

    #[test]
    fn backend_only_manifest_still_parses() {
        // The shape of the shipped Anubis.toml.example — no [package]/[dependencies].
        let text = r#"
[backend.risc0_metal]
reference_path = "/path/to/metal-hybrid-prover"
"#;
        let m = AnubisManifest::parse(text).expect("parse");
        assert_eq!(m.package.name, ""); // defaulted
        assert!(m.dependencies.is_empty());
        assert_eq!(
            m.metal_reference_path(),
            Some(PathBuf::from("/path/to/metal-hybrid-prover"))
        );
    }

    #[test]
    fn empty_and_malformed_manifests_are_handled_fail_closed() {
        // Empty manifest -> all defaults, no metal reference.
        let m = AnubisManifest::parse("").expect("empty parses to defaults");
        assert_eq!(m, AnubisManifest::default());
        assert_eq!(m.metal_reference_path(), None);
        // Malformed -> hard error, never silently ignored.
        let err = AnubisManifest::parse("this is not = = valid toml [[[").unwrap_err();
        assert!(err.starts_with("ANUBIS_MANIFEST_PARSE"), "got: {err}");
    }

    #[test]
    fn discover_synthesizes_a_single_file_project_when_no_manifest() {
        // A path with no Anubis.toml up-tree (system temp dir) is its own single-file project.
        let entry = std::env::temp_dir().join("anubis_layout_probe_xyz.anb");
        let layout = ProjectLayout::discover(&entry).expect("discover");
        assert!(layout.single_file);
        assert!(layout.manifest_path.is_none());
        assert_eq!(layout.manifest, AnubisManifest::default());
        assert_eq!(layout.src_root, layout.root);
    }

    #[test]
    fn an_edition_is_parsed_from_the_package_table() {
        let m = AnubisManifest::parse("[package]\nname = \"x\"\nedition = \"2026\"\n").unwrap();
        assert_eq!(m.edition().unwrap(), Edition::E2026);
        assert!(m.edition_is_declared());
    }

    #[test]
    fn an_unknown_edition_is_refused_not_assumed() {
        // The load-bearing rule: a compiler that does not know an edition must
        // refuse it, because compiling it under today's rules would give the
        // program semantics its author never asked for.
        let err = AnubisManifest::parse("[package]\nedition = \"2031\"\n").unwrap_err();
        assert!(err.starts_with("ANUBIS_EDITION_UNKNOWN"), "got: {err}");
        assert!(
            err.contains("2026"),
            "the refusal must name what IS supported: {err}"
        );
    }

    #[test]
    fn a_missing_edition_resolves_to_the_default_for_now() {
        let m = AnubisManifest::parse("[package]\nname = \"x\"\n").unwrap();
        assert_eq!(m.edition().unwrap(), Edition::DEFAULT);
        assert!(!m.edition_is_declared(), "so a caller can warn about it");
    }

    #[test]
    fn an_absent_manifest_still_has_an_edition() {
        assert_eq!(
            AnubisManifest::default().edition().unwrap(),
            Edition::DEFAULT
        );
    }

    #[test]
    fn editions_are_ordered_oldest_first() {
        // ALL is relied on for the "supported editions" message and for any
        // future migration ordering, so its order is a property, not an accident.
        let mut sorted = Edition::ALL.to_vec();
        sorted.sort();
        assert_eq!(sorted.as_slice(), Edition::ALL);
    }
}
