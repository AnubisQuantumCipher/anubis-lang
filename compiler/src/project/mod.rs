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
        toml::from_str(text).map_err(|e| format!("ANUBIS_MANIFEST_PARSE: {e}"))
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
}
