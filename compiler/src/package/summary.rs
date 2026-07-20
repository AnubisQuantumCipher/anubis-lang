//! Package function summaries — sealed in evidence, re-derived on verify (fail-closed).
//!
//! Schema `anubis.summaries.v2`: every `pub fn` effect/taint/return annotation AND its declared
//! `requires`/`ensures` contracts — the surface a consumer must inherit at call sites. Live
//! re-typecheck of mounted sources is still the enforcement engine; summaries are the
//! *proof-carrying claim* that the sealed package advertised those properties honestly. v2 adds
//! contracts: sealing them makes a dependency's advertised pre/postconditions tamper-evident (the
//! summary is re-derived and byte-compared on verify, and the source merkle covers the contract
//! text). Cross-package call-site ENFORCEMENT is live (Phase-6 DoD, verified 2026-07-20): the
//! consumer combines the (hash-pinned, evidence-verified) dependency source and re-typechecks it, so
//! an imported fn's `requires` is DISCHARGED at the consumer's call site, its `ensures` is assumed,
//! and its effects/taint are inherited — see `phase6_cross_module_summary_enforced_at_call_sites`.
//! Re-proving the dep body in the same run also catches a dep that ADVERTISES a contract its body
//! does not satisfy, so the sealed summary is a trusted-then-verified claim, not a trusted one.

use crate::frontend::{parse_source, Expr, Item, Stmt, Visibility};
use crate::package::merkle;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DECLASSIFY_AUDIT_FILENAME: &str = "declassify_audit.json";
pub const DECLASSIFY_AUDIT_SCHEMA: &str = "anubis.declassify_audit.v1";

/// One `declassify(value, policy, reason)` call site — the compliance-ready declassification log
/// (operator directive 2026-07-20). An auditor can open the bundle and see EVERY place a developer
/// deliberately released private data, the policy they cited, and the reason — GDPR/SOC2 material. A
/// malformed (empty policy/reason) declassify is recorded with `well_formed: false` (it does NOT
/// release the label; see `declassify_wellformed` in the checker).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclassifyRecord {
    pub function: String,
    pub policy: String,
    pub reason: String,
    pub well_formed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeclassifyAudit {
    pub schema: String,
    pub package: String,
    pub version: String,
    pub source_merkle: String,
    pub declassifications: Vec<DeclassifyRecord>,
}

/// Walk the sealed source for every `declassify` call, per enclosing function — the declassification
/// audit trail. (Line precision needs an AST span on `Expr::Declassify`, a follow-up; policy/reason/
/// function is the compliance-material core.)
pub fn extract_declassify_audit(
    package: &str,
    version: &str,
    source: &str,
) -> Result<DeclassifyAudit, String> {
    let source_merkle = merkle::sha256_hex(source.as_bytes());
    let ast = parse_source(source)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: declassify audit parse failed: {e}"))?;
    let mut records = Vec::new();
    collect_declassify(&ast.items, &mut records);
    Ok(DeclassifyAudit {
        schema: DECLASSIFY_AUDIT_SCHEMA.into(),
        package: package.to_string(),
        version: version.to_string(),
        source_merkle,
        declassifications: records,
    })
}

pub fn write_audit_to_evidence_dir(dir: &Path, audit: &DeclassifyAudit) -> Result<(), String> {
    let json = serde_json::to_string_pretty(audit).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(DECLASSIFY_AUDIT_FILENAME), json).map_err(|e| e.to_string())
}

fn collect_declassify(items: &[Item], out: &mut Vec<DeclassifyRecord>) {
    for it in items {
        match it {
            Item::Fn { name, body, .. } => {
                for s in body {
                    walk_stmt_dcl(s, name, out);
                }
            }
            Item::Module { items, .. } => collect_declassify(items, out),
            _ => {}
        }
    }
}

fn walk_stmt_dcl(s: &Stmt, fname: &str, out: &mut Vec<DeclassifyRecord>) {
    match s {
        Stmt::Let { init, .. } => walk_expr_dcl(init, fname, out),
        Stmt::LetPattern { init, .. } => walk_expr_dcl(init, fname, out),
        Stmt::Assign { target, value } => {
            walk_expr_dcl(target, fname, out);
            walk_expr_dcl(value, fname, out);
        }
        Stmt::If { cond, then, else_ } => {
            walk_expr_dcl(cond, fname, out);
            then.iter().for_each(|s| walk_stmt_dcl(s, fname, out));
            if let Some(e) = else_ {
                e.iter().for_each(|s| walk_stmt_dcl(s, fname, out));
            }
        }
        Stmt::While { cond, body, .. } => {
            walk_expr_dcl(cond, fname, out);
            body.iter().for_each(|s| walk_stmt_dcl(s, fname, out));
        }
        Stmt::WhileLet { expr, body, .. } => {
            walk_expr_dcl(expr, fname, out);
            body.iter().for_each(|s| walk_stmt_dcl(s, fname, out));
        }
        Stmt::For { body, .. } | Stmt::Loop { body, .. } => {
            body.iter().for_each(|s| walk_stmt_dcl(s, fname, out));
        }
        Stmt::ExprStmt(e) => walk_expr_dcl(e, fname, out),
        _ => {}
    }
}

fn walk_expr_dcl(e: &Expr, fname: &str, out: &mut Vec<DeclassifyRecord>) {
    match e {
        Expr::Declassify { inner, policy, reason } => {
            let p = policy.clone().unwrap_or_default();
            let r = reason.clone().unwrap_or_default();
            let well_formed = !p.trim().is_empty() && !r.trim().is_empty();
            out.push(DeclassifyRecord {
                function: fname.to_string(),
                policy: p,
                reason: r,
                well_formed,
            });
            walk_expr_dcl(inner, fname, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr_dcl(lhs, fname, out);
            walk_expr_dcl(rhs, fname, out);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Try(expr) => walk_expr_dcl(expr, fname, out),
        Expr::Tainted { inner, .. } => walk_expr_dcl(inner, fname, out),
        Expr::Call { args, .. } => args.iter().for_each(|a| walk_expr_dcl(a, fname, out)),
        Expr::CallExpr { callee, args } => {
            walk_expr_dcl(callee, fname, out);
            args.iter().for_each(|a| walk_expr_dcl(a, fname, out));
        }
        Expr::Index { base, index } => {
            walk_expr_dcl(base, fname, out);
            walk_expr_dcl(index, fname, out);
        }
        Expr::FieldAccess { base, .. } => walk_expr_dcl(base, fname, out),
        Expr::ArrayLiteral { elements } => elements.iter().for_each(|x| walk_expr_dcl(x, fname, out)),
        Expr::StructLiteral { fields, .. } => {
            fields.iter().for_each(|(_, x)| walk_expr_dcl(x, fname, out))
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().for_each(|x| walk_expr_dcl(x, fname, out)),
        Expr::MapLiteral { entries, .. } => entries.iter().for_each(|(k, v)| {
            walk_expr_dcl(k, fname, out);
            walk_expr_dcl(v, fname, out);
        }),
        Expr::Match { scrutinee, arms, .. } => {
            walk_expr_dcl(scrutinee, fname, out);
            arms.iter().for_each(|a| walk_expr_dcl(&a.body, fname, out));
        }
        Expr::If { cond, then, else_, .. } => {
            walk_expr_dcl(cond, fname, out);
            walk_expr_dcl(then, fname, out);
            walk_expr_dcl(else_, fname, out);
        }
        Expr::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_dcl(scrutinee, fname, out);
            walk_expr_dcl(then, fname, out);
            walk_expr_dcl(else_, fname, out);
        }
        Expr::Block { stmts, tail } => {
            stmts.iter().for_each(|s| walk_stmt_dcl(s, fname, out));
            if let Some(t) = tail {
                walk_expr_dcl(t, fname, out);
            }
        }
        Expr::Lambda { body, .. } => walk_expr_dcl(body, fname, out),
        _ => {}
    }
}

const MODULE_EXTS: &[&str] = &["anb", "anub", "anubis"];

pub const SUMMARIES_FILENAME: &str = "summaries.json";
pub const SUMMARIES_SCHEMA: &str = "anubis.summaries.v2";

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
    /// Whether the function is part of the package's PUBLIC API surface (`pub fn`). ALL functions are
    /// now sealed so a bundle reviewer can see every function's analyzed pre/postconditions and effects
    /// (operator directive 2026-07-20 — the interproc analysis must be VISIBLE, not just the API). This
    /// flag distinguishes the exported contract surface from internal helpers.
    #[serde(default)]
    pub public: bool,
    pub effects: Vec<String>,
    pub params: Vec<ParamSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ret: Option<String>,
    pub returns_tainted: bool,
    /// B2 preconditions declared as `requires(P)`, rendered to canonical source form. A consumer
    /// that calls this function must establish each `requires` at the call site. Empty when the
    /// function declares no preconditions (then omitted from the sealed JSON for compactness).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// B2 postconditions declared as `ensures(Q)` (may reference `result`), rendered to canonical
    /// source form. A consumer may assume each `ensures` holds of the call's result. Empty when the
    /// function declares no postconditions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ensures: Vec<String>,
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
                requires,
                ensures,
                ..
            } => {
                let public = matches!(visibility, Visibility::Public);
                let mut eff = effects.clone();
                eff.sort();
                eff.dedup();
                // Contracts are rendered to canonical source form and preserved in declaration
                // order (NOT sorted): a precondition/postcondition list is an ordered conjunction as
                // authored, and re-deriving it must reproduce the sealed text byte-for-byte.
                let requires: Vec<String> =
                    requires.iter().map(crate::doc::expr_to_src).collect();
                let ensures: Vec<String> = ensures.iter().map(crate::doc::expr_to_src).collect();
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
                    public,
                    effects: eff,
                    params,
                    ret: ret.clone(),
                    returns_tainted,
                    requires,
                    ensures,
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
        // ALL functions are now sealed (operator directive 2026-07-20): the interproc analysis must be
        // visible, not just the API. `private` is included but marked non-public; `pub` fns are public.
        let private = s.functions.iter().find(|f| f.name == "private").unwrap();
        assert!(!private.public);
        assert!(s.functions.iter().find(|f| f.name == "need_shell").unwrap().public);
        assert!(id.public);
    }

    #[test]
    fn seals_requires_and_ensures_contracts() {
        // v2: a pub fn's declared requires/ensures are captured into the summary (source-rendered),
        // and re-deriving from swapped-contract sources fails closed (tamper-evidence).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Anubis.toml"),
            "[package]\nname=\"c\"\nversion=\"1.0.0\"\n",
        )
        .unwrap();
        let src = root.join("src/lib.anb");
        std::fs::write(
            &src,
            "pub fn inc(x: u32) -> u32 requires(x > 0) ensures(result >= x) { return x + 1; }\n",
        )
        .unwrap();

        let s = extract_from_package(root).unwrap();
        assert_eq!(s.schema, "anubis.summaries.v2");
        let inc = s.functions.iter().find(|f| f.name == "inc").unwrap();
        // Precondition and postcondition both captured, in declaration order.
        assert_eq!(inc.requires.len(), 1, "requires captured");
        assert_eq!(inc.ensures.len(), 1, "ensures captured");
        assert!(inc.requires[0].contains('x') && inc.requires[0].contains('>'), "requires text: {}", inc.requires[0]);
        assert!(inc.ensures[0].contains("result"), "ensures references result: {}", inc.ensures[0]);

        // Tamper-evidence: seal, then weaken the precondition in the source; re-derive must reject.
        let ev = tmp.path().join("evidence");
        std::fs::create_dir_all(&ev).unwrap();
        write_to_evidence_dir(&ev, &s).unwrap();
        verify_against_package(root, &ev).expect("honest package re-derives to the sealed summary");

        std::fs::write(
            &src,
            "pub fn inc(x: u32) -> u32 requires(x > 100) ensures(result >= x) { return x + 1; }\n",
        )
        .unwrap();
        let tampered = verify_against_package(root, &ev);
        assert!(
            tampered.is_err(),
            "swapping the requires must fail the sealed re-derive (contract tamper caught)"
        );
    }
}
