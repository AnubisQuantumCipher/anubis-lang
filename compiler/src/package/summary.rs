//! Package function summaries — sealed in evidence, re-derived on verify (fail-closed).
//!
//! Schema `anubis.summaries.v1`: every `pub fn` effect/taint/return annotation that the
//! consumer must inherit at call sites. Live re-typecheck of mounted sources is still the
//! enforcement engine; summaries are the *proof-carrying claim* that the sealed package
//! advertised those properties honestly.

use crate::frontend::{parse_source, Item, Visibility};
use crate::package::merkle;
use serde::{Deserialize, Serialize};
use std::path::Path;

const MODULE_EXTS: &[&str] = &["anb", "anub", "anubis"];

pub const SUMMARIES_FILENAME: &str = "summaries.json";
pub const SUMMARIES_SCHEMA: &str = "anubis.summaries.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageSummaries {
    pub schema: String,
    pub package: String,
    pub version: String,
    pub source_merkle: String,
    pub functions: Vec<FnSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FnSummary {
    pub name: String,
    pub effects: Vec<String>,
    pub params: Vec<ParamSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ret: Option<String>,
    pub returns_tainted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    pub tainted: bool,
}

/// Extract summaries from a single sealed source string (single-file packages / evidence builder).
pub fn extract_from_source_text(
    package: &str,
    version: &str,
    source: &str,
) -> Result<PackageSummaries, String> {
    let source_merkle = merkle::sha256_hex(source.as_bytes());
    let ast = parse_source(source).map_err(|e| {
        format!("ANUBIS_DEP_PROOF_UNVERIFIED: summary extract parse failed: {e}")
    })?;
    let mut functions = Vec::new();
    collect_fns(&ast.items, &mut functions);
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    functions.dedup_by(|a, b| a.name == b.name);
    Ok(PackageSummaries {
        schema: SUMMARIES_SCHEMA.into(),
        package: package.to_string(),
        version: version.to_string(),
        source_merkle,
        functions,
    })
}

/// Extract summaries from package module sources on disk.
pub fn extract_from_package(package_root: &Path) -> Result<PackageSummaries, String> {
    let name = read_pkg_field(package_root, "name").unwrap_or_else(|| "unknown".into());
    let version = read_pkg_field(package_root, "version").unwrap_or_else(|| "0.0.0".into());
    let src_root = {
        let s = package_root.join("src");
        if s.is_dir() {
            s
        } else {
            package_root.to_path_buf()
        }
    };
    let modules = collect_modules(&src_root)?;
    let source_merkle = merkle::merkle_root(modules.clone());
    let mut functions = Vec::new();
    for (_path, bytes) in &modules {
        let text = String::from_utf8_lossy(bytes);
        let ast = parse_source(&text).map_err(|e| {
            format!("ANUBIS_DEP_PROOF_UNVERIFIED: summary extract parse failed: {e}")
        })?;
        collect_fns(&ast.items, &mut functions);
    }
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    functions.dedup_by(|a, b| a.name == b.name);
    Ok(PackageSummaries {
        schema: SUMMARIES_SCHEMA.into(),
        package: name,
        version,
        source_merkle,
        functions,
    })
}

/// Write `summaries.json` into an evidence directory (before MANIFEST hash).
pub fn write_to_evidence_dir(evidence_dir: &Path, summaries: &PackageSummaries) -> Result<(), String> {
    let json = serde_json::to_string_pretty(summaries).map_err(|e| e.to_string())?;
    std::fs::write(evidence_dir.join(SUMMARIES_FILENAME), json).map_err(|e| e.to_string())
}

/// Re-extract from package and require sealed `summaries.json` matches exactly.
pub fn verify_against_package(package_root: &Path, evidence_dir: &Path) -> Result<(), String> {
    let sealed_path = evidence_dir.join(SUMMARIES_FILENAME);
    if !sealed_path.is_file() {
        // Legacy evidence without summaries: fail closed for new resolve (strict).
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: missing summaries.json (re-publish package with evidence)"
                .to_string(),
        );
    }
    let sealed_text = std::fs::read_to_string(&sealed_path).map_err(|e| e.to_string())?;
    let sealed: PackageSummaries = serde_json::from_str(&sealed_text).map_err(|e| {
        format!("ANUBIS_DEP_PROOF_UNVERIFIED: summaries.json parse: {e}")
    })?;
    if sealed.schema != SUMMARIES_SCHEMA {
        return Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: unsupported summaries schema `{}`",
            sealed.schema
        ));
    }
    let live = extract_from_package(package_root)?;
    // Compare the security-relevant fields (functions + source merkle). Package/version in the
    // sealed file may be placeholders when evidence was built from a single source string before
    // package-faithful overwrite — still require live re-derive of functions/merkle to match.
    if sealed.functions != live.functions || sealed.source_merkle != live.source_merkle {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: sealed summaries.json does not match package sources \
             (summary claim dishonest or source swapped)"
                .to_string(),
        );
    }
    Ok(())
}

fn collect_fns(items: &[Item], out: &mut Vec<FnSummary>) {
    for it in items {
        match it {
            Item::Fn {
                name,
                visibility,
                params,
                ret,
                effects,
                ..
            } => {
                if !matches!(visibility, Visibility::Public) {
                    continue;
                }
                let mut eff = effects.clone();
                eff.sort();
                eff.dedup();
                let params: Vec<ParamSummary> = params
                    .iter()
                    .map(|(n, ty)| {
                        let ty_s = if ty.is_empty() {
                            None
                        } else {
                            Some(ty.clone())
                        };
                        let tainted = ty_s
                            .as_ref()
                            .map(|t| t.to_ascii_lowercase().contains("tainted<"))
                            .unwrap_or(false);
                        ParamSummary {
                            name: n.clone(),
                            ty: ty_s,
                            tainted,
                        }
                    })
                    .collect();
                let returns_tainted = ret
                    .as_ref()
                    .map(|t| t.to_ascii_lowercase().contains("tainted<"))
                    .unwrap_or(false);
                out.push(FnSummary {
                    name: name.clone(),
                    effects: eff,
                    params,
                    ret: ret.clone(),
                    returns_tainted,
                });
            }
            Item::Module { items, .. } => collect_fns(items, out),
            _ => {}
        }
    }
}

fn read_pkg_field(root: &Path, field: &str) -> Option<String> {
    let text = std::fs::read_to_string(root.join("Anubis.toml")).ok()?;
    let m = crate::project::AnubisManifest::parse(&text).ok()?;
    match field {
        "name" if !m.package.name.is_empty() => Some(m.package.name),
        "version" if !m.package.version.is_empty() => Some(m.package.version),
        _ => None,
    }
}

fn collect_modules(src_root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    walk(src_root, src_root, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for ent in rd {
        let ent = ent.map_err(|e| e.to_string())?;
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "out" || name == "target" || name.starts_with("evidence") {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if MODULE_EXTS.contains(&ext) {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                out.push((rel, data));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_pub_effects_and_taint() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Anubis.toml"),
            "[package]\nname=\"x\"\nversion=\"1.0.0\"\n",
        )
        .unwrap();
        let mut f = std::fs::File::create(root.join("src/lib.anb")).unwrap();
        writeln!(
            f,
            "pub fn need_shell() uses(shell) {{ return 1; }}\npub fn id(x: tainted<u64>) -> tainted<u64> {{ return x; }}\nfn private() {{ return 0; }}\n"
        )
        .unwrap();
        let s = extract_from_package(root).unwrap();
        assert_eq!(s.package, "x");
        assert!(s.functions.iter().any(|f| f.name == "need_shell" && f.effects.iter().any(|e| e.contains("shell"))));
        let id = s.functions.iter().find(|f| f.name == "id").unwrap();
        assert!(id.params[0].tainted);
        assert!(id.returns_tainted);
        assert!(!s.functions.iter().any(|f| f.name == "private"));
    }
}
