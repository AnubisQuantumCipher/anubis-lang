//! Verify dependency evidence bundles and signer trust.

#[cfg(test)]
use crate::evidence::verify_pca;
use crate::evidence::{
    pca_signature_status, solver_execution_matches_obligations, validate_bundle, verify_pca_scope,
    ClaimBlock, PcaScope,
};
use crate::package::trust::TrustStore;
use std::io::Read;
use std::path::Path;

/// Module extensions recognized for package source binding (mirrors resolve::MODULE_EXTENSIONS).
const PACKAGE_MODULE_EXTS: &[&str] = &["anb", "anub", "anubis"];
const MAX_PACKAGE_WALK_DEPTH: usize = 128;
const MAX_PACKAGE_WALK_ENTRIES: usize = 16_384;
const MAX_PACKAGE_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE_TOTAL_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// Options controlling unsigned-dep policy.
#[derive(Debug, Clone, Default)]
pub struct ProofPolicy {
    /// Allow unsigned deps only when both env and CLI opt-in (caller enforces both).
    pub allow_unsigned: bool,
    /// Project-level trusted public keys (from Anubis.toml `[package.trust].signers`).
    pub project_signers: Vec<String>,
}

/// Verify a package's sealed `evidence/` directory **and** bind it to the package sources
/// the consumer will load (fail-closed if the signed claim is about different code).
///
/// - Missing evidence / invalid PCA → `ANUBIS_DEP_PROOF_UNVERIFIED`
/// - Evidence source unbound from package modules → `ANUBIS_DEP_PROOF_UNVERIFIED`
/// - Valid sig but signer not trusted → `ANUBIS_DEP_UNTRUSTED_SIGNER`
/// - Unsigned + !allow_unsigned → `ANUBIS_DEP_PROOF_UNVERIFIED`
///
/// Returns the signer public key when signed+verified+trusted, or `None` when
/// unsigned was explicitly allowed.
pub fn verify_dep_evidence(
    evidence_dir: &Path,
    trust: &TrustStore,
    policy: &ProofPolicy,
) -> Result<Option<String>, String> {
    verify_dep_evidence_for_package(None, evidence_dir, trust, policy)
}

/// Like [`verify_dep_evidence`], and when `package_root` is set, require
/// `evidence/source.anubis` (or multi-file merkle leaves) to match package module sources.
pub fn verify_dep_evidence_for_package(
    package_root: Option<&Path>,
    evidence_dir: &Path,
    trust: &TrustStore,
    policy: &ProofPolicy,
) -> Result<Option<String>, String> {
    if let Some(root) = package_root {
        ensure_no_package_dependencies(root)?;
    }
    if !evidence_dir.is_dir() {
        return Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: missing evidence directory `{}`",
            evidence_dir.display()
        ));
    }
    let scope = verify_pca_scope(evidence_dir)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: pca verify failed: {e}"))?;
    if scope.is_none() {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: evidence bundle failed hash/claim verification"
                .to_string(),
        );
    }
    if scope != Some(PcaScope::PackagePublishV1) {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: dependency requires a current package-publish claim; legacy, build-only, and ordinary check bundles cannot bind package identity"
                .to_string(),
        );
    }
    let root = package_root.ok_or_else(|| {
        "ANUBIS_DEP_PROOF_UNVERIFIED: package root required to verify published summary identity"
            .to_string()
    })?;
    // PCA verification also accepts an honestly re-derived disproof. A dependency
    // proof gate needs an accepted artifact and a PASS claim, not merely valid
    // evidence that the dependency failed a contract.
    let accepted = validate_bundle(evidence_dir)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: bundle validation failed: {e}"))?;
    let claim: ClaimBlock =
        serde_json::from_slice(&read_package_source(&evidence_dir.join("pca.json"))?)
            .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: PCA parse failed: {e}"))?;
    if !accepted
        || claim.verdict != "PASS"
        || claim.mode != "safe"
        || !solver_execution_matches_obligations(&claim)
    {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: dependency evidence requires a Safe PASS claim"
                .to_string(),
        );
    }
    if claim.zk_present {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: dependency advertises a ZK receipt that package admission cannot cryptographically verify"
                .into(),
        );
    }
    // The claim mode is supplied by the bundle producer. Re-derive the intrinsic
    // modes of every source module before a Safe consumer may mount it; a signed
    // "safe" label cannot turn an @research function into Safe code.
    ensure_safe_source(&read_package_source(&evidence_dir.join("source.anubis"))?)?;
    bind_evidence_to_package_sources(root, evidence_dir)?;
    ensure_safe_package_modules(root)?;
    let sealed_bytes =
        read_package_source(&evidence_dir.join(crate::package::summary::SUMMARIES_FILENAME))?;
    let sealed: crate::package::summary::PackageSummaries =
        serde_json::from_slice(&sealed_bytes)
            .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: summaries.json parse: {e}"))?;
    let live = crate::package::summary::extract_from_package(root)?;
    if sealed != live {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: published package summary differs from mounted package identity, source merkle, or functions"
                .to_string(),
        );
    }
    match pca_signature_status(evidence_dir)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: signature status: {e}"))?
    {
        None => {
            if policy.allow_unsigned {
                Ok(None)
            } else {
                Err(
                    "ANUBIS_DEP_PROOF_UNVERIFIED: dependency evidence is unsigned \
                     (sign with `anubis sign` / `anubis package publish --key`)"
                        .to_string(),
                )
            }
        }
        Some((false, pk)) => Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: invalid signature for signer `{pk}`"
        )),
        Some((true, pk)) => {
            if trust.allows(&pk, &policy.project_signers) {
                Ok(Some(pk))
            } else {
                Err(format!(
                    "ANUBIS_DEP_UNTRUSTED_SIGNER: signer `{pk}` is not in the trust store \
                     (`anubis trust add-signer` or [package.trust] signers)"
                ))
            }
        }
    }
}

fn ensure_safe_source(bytes: &[u8]) -> Result<(), String> {
    use crate::frontend::{Item, Mode};

    fn all_safe(items: &[Item]) -> bool {
        items.iter().all(|item| match item {
            Item::Fn { mode, body, .. } => {
                *mode == Mode::Safe && !crate::middle::body_has_mode_elevator(body)
            }
            Item::Module { items, .. } => all_safe(items),
            Item::Impl { methods, .. } | Item::Trait { methods, .. } => all_safe(methods),
            Item::Import { .. } => false,
            Item::Struct { .. } | Item::Enum { .. } => true,
        })
    }

    let source = std::str::from_utf8(bytes)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: non-UTF8 package source: {e}"))?;
    let ast = crate::frontend::parse_source(source)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: cannot parse package source: {e}"))?;
    if !all_safe(&ast.items) {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: Safe dependency contains Research/Exploit code, a mode elevator, or an import outside its analyzed PCA source"
                .to_string(),
        );
    }
    Ok(())
}

fn ensure_safe_package_modules(package_root: &Path) -> Result<(), String> {
    let src_root = package_source_root(package_root)?;
    for (_, source) in collect_package_modules(&src_root)? {
        ensure_safe_source(&source)?;
    }
    Ok(())
}

/// Bind signed evidence to the code the consumer mounts (closes swap-source attacks).
///
/// * **One module file** under package `src/` (or root): `evidence/source.anubis` must be
///   byte-identical to that file.
/// * **Multiple modules**: currently refused. The producer's PCA analyzes only its entry-file
///   snapshot, so even a verified Merkle root does not establish that every mounted module was
///   analyzed. A future package-closure claim must connect each module to the analyzed AST.
pub fn bind_evidence_to_package_sources(
    package_root: &Path,
    evidence_dir: &Path,
) -> Result<(), String> {
    let sealed_path = evidence_dir.join("source.anubis");
    let sealed = read_package_source(&sealed_path)?;
    let src_root = package_source_root(package_root)?;
    let modules = collect_package_modules(&src_root)?;
    if modules.is_empty() {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: package has no module sources (.anb) to bind evidence to"
                .to_string(),
        );
    }
    if modules.len() == 1 {
        if sealed != modules[0].1 {
            return Err(
                "ANUBIS_DEP_PROOF_UNVERIFIED: evidence source.anubis does not match package \
                 module source (signed claim unbound from code the consumer loads)"
                    .to_string(),
            );
        }
        return Ok(());
    }
    Err(
        "ANUBIS_DEP_PROOF_UNVERIFIED: multi-module package has no PCA claim binding every mounted module to the analyzed source"
            .to_string(),
    )
}

fn package_source_root(package_root: &Path) -> Result<std::path::PathBuf, String> {
    let src = package_root.join("src");
    match std::fs::symlink_metadata(&src) {
        Ok(meta) if meta.file_type().is_symlink() => Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: package src symlink is not a sealed source tree".into(),
        ),
        Ok(meta) if meta.is_dir() => Ok(src),
        Ok(_) => Err("ANUBIS_DEP_PROOF_UNVERIFIED: package src is not a directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(package_root.to_path_buf()),
        Err(e) => Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: cannot inspect package src: {e}"
        )),
    }
}

fn ensure_no_package_dependencies(package_root: &Path) -> Result<(), String> {
    let manifest = package_root.join(crate::project::MANIFEST_FILENAME);
    let text = String::from_utf8(read_package_source(&manifest)?)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: non-UTF8 package manifest: {e}"))?;
    let parsed = crate::project::AnubisManifest::parse(&text)?;
    if !parsed.dependencies.is_empty() {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: package dependencies are not bound to this PCA source closure"
                .into(),
        );
    }
    Ok(())
}

fn read_package_source(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| {
        format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: cannot inspect {}: {e}",
            path.display()
        )
    })?;
    if !meta.file_type().is_file() || meta.len() > MAX_PACKAGE_SOURCE_BYTES {
        return Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: source must be a bounded regular file: {}",
            path.display()
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| {
        format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: cannot open {}: {e}",
            path.display()
        )
    })?;
    let opened_meta = file.metadata().map_err(|e| e.to_string())?;
    if !opened_meta.is_file() || opened_meta.len() > MAX_PACKAGE_SOURCE_BYTES {
        return Err(format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: opened source is not a bounded regular file: {}",
            path.display()
        ));
    }
    let mut data = Vec::new();
    file.take(MAX_PACKAGE_SOURCE_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("ANUBIS_DEP_PROOF_UNVERIFIED: source read failed: {e}"))?;
    if data.len() as u64 > MAX_PACKAGE_SOURCE_BYTES {
        return Err("ANUBIS_DEP_PROOF_UNVERIFIED: source byte budget exceeded".into());
    }
    Ok(data)
}

fn collect_package_modules(src_root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    let meta = std::fs::symlink_metadata(src_root).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("ANUBIS_DEP_PROOF_UNVERIFIED: module root is not a directory".into());
    }
    let mut entries = 0usize;
    let mut source_bytes = 0u64;
    collect_modules_walk(
        src_root,
        src_root,
        &mut out,
        &mut entries,
        &mut source_bytes,
        0,
    )?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// The current publisher seals one analyzed source file. Refuse a package that
/// would subsequently fail dependency proof admission, before signing or
/// inserting it into the local registry.
pub fn ensure_single_module_publishable(package_root: &Path, entry: &Path) -> Result<(), String> {
    ensure_no_package_dependencies(package_root)?;
    let src_root = package_source_root(package_root)?;
    let modules = collect_package_modules(&src_root)?;
    if modules.len() != 1 || src_root.join(&modules[0].0) != entry {
        return Err(
            "ANUBIS_DEP_PROOF_UNVERIFIED: package publish requires one module until full module-closure PCA is implemented"
                .into(),
        );
    }
    ensure_safe_source(&read_package_source(entry)?)?;
    Ok(())
}

fn collect_modules_walk(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
    entries: &mut usize,
    source_bytes: &mut u64,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PACKAGE_WALK_DEPTH {
        return Err("ANUBIS_DEP_PROOF_UNVERIFIED: package module depth budget exceeded".into());
    }
    let rd = std::fs::read_dir(dir).map_err(|e| {
        format!(
            "ANUBIS_DEP_PROOF_UNVERIFIED: cannot read {}: {e}",
            dir.display()
        )
    })?;
    for ent in rd {
        let ent = ent.map_err(|e| e.to_string())?;
        *entries += 1;
        if *entries > MAX_PACKAGE_WALK_ENTRIES {
            return Err("ANUBIS_DEP_PROOF_UNVERIFIED: package entry budget exceeded".into());
        }
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "out" || name == "target" || name == "evidence" {
            continue;
        }
        if name.starts_with("evidence-") {
            continue;
        }
        let kind = ent.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "ANUBIS_DEP_PROOF_UNVERIFIED: package module tree contains symlink {}",
                path.display()
            ));
        }
        if kind.is_dir() {
            collect_modules_walk(root, &path, out, entries, source_bytes, depth + 1)?;
        } else if kind.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if PACKAGE_MODULE_EXTS.contains(&ext) {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let data = read_package_source(&path)?;
                *source_bytes = source_bytes
                    .checked_add(data.len() as u64)
                    .ok_or("ANUBIS_DEP_PROOF_UNVERIFIED: source byte budget overflow")?;
                if *source_bytes > MAX_PACKAGE_TOTAL_SOURCE_BYTES {
                    return Err(
                        "ANUBIS_DEP_PROOF_UNVERIFIED: package source byte budget exceeded".into(),
                    );
                }
                out.push((rel, data));
            }
        }
    }
    Ok(())
}

/// Prefer package-local `evidence/` then nested `evidence-*/` first match.
pub fn find_evidence_dir(package_root: &Path) -> Option<std::path::PathBuf> {
    let direct = package_root.join("evidence");
    if std::fs::symlink_metadata(&direct)
        .ok()
        .is_some_and(|m| m.is_dir() && !m.file_type().is_symlink())
        && direct.join("MANIFEST.sha256").is_file()
    {
        return Some(direct);
    }
    // Also accept a single evidence-* snapshot under package root.
    if let Ok(rd) = std::fs::read_dir(package_root) {
        for ent in rd.flatten() {
            let p = ent.path();
            if std::fs::symlink_metadata(&p)
                .ok()
                .is_some_and(|m| m.is_dir() && !m.file_type().is_symlink())
            {
                let name = ent.file_name().to_string_lossy().to_string();
                if name.starts_with("evidence-") && p.join("MANIFEST.sha256").is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{
        build_evidence_bundle, generate_keypair, refresh_manifest_hashes, sign_pca,
        EvidenceManifest,
    };
    use crate::package::merkle;

    #[test]
    fn verified_dependency_requires_package_publish_scope_and_mounted_identity() {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("pkg");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(
            package.join("Anubis.toml"),
            "[package]\nname = \"scope_test\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        let source = "fn main() { let x = 1; }";
        std::fs::write(package.join("src/main.anb"), source).unwrap();
        let bundle = build_evidence_bundle(
            source,
            "safe",
            None,
            vec![],
            &package.join("out"),
            Some(crate::evidence::PACKAGE_PUBLISH_LANE),
            None,
        )
        .unwrap();
        let summary = crate::package::summary::extract_from_package(&package).unwrap();
        crate::package::summary::write_to_evidence_dir(&bundle.dir, &summary).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        let policy = ProofPolicy {
            allow_unsigned: true,
            ..ProofPolicy::default()
        };
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::PackagePublishV1)
        );
        assert!(verify_dep_evidence_for_package(
            Some(&package),
            &bundle.dir,
            &TrustStore::default(),
            &policy,
        )
        .is_ok());

        let source_check = build_evidence_bundle(
            source,
            "safe",
            None,
            vec![],
            &package.join("out"),
            Some("safe-check"),
            None,
        )
        .unwrap();
        assert_eq!(
            verify_pca_scope(&source_check.dir).unwrap(),
            Some(PcaScope::SourceCheckV1)
        );
        assert!(verify_dep_evidence_for_package(
            Some(&package),
            &source_check.dir,
            &TrustStore::default(),
            &policy,
        )
        .is_err());

        let mut forged_summary = summary.clone();
        forged_summary.version = "2.0.0".into();
        crate::package::summary::write_to_evidence_dir(&bundle.dir, &forged_summary).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::PackagePublishV1),
            "package metadata is explicitly pending mounted-package verification"
        );
        assert!(verify_dep_evidence_for_package(
            Some(&package),
            &bundle.dir,
            &TrustStore::default(),
            &policy,
        )
        .is_err());
        crate::package::summary::write_to_evidence_dir(&bundle.dir, &summary).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(verify_dep_evidence_for_package(
            Some(&package),
            &bundle.dir,
            &TrustStore::default(),
            &policy,
        )
        .is_ok());

        // Previously a source-only PCA could enter this proof-required dependency gate.
        // Preserve an actual version-specific v3 source result for migration, while withholding
        // verified dependency admission. Rewriting only a v4 version number is not a v3 fixture.
        let path = bundle.dir.join("pca.json");
        let legacy =
            crate::evidence::derive_version_three_claim_for_test(&bundle.dir, source, "safe");
        assert_eq!(legacy.pca_version, 3);
        assert!(legacy.claim_kind.is_none());
        assert!(legacy.evidence_lane.is_none());
        std::fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert_eq!(
            verify_pca_scope(&bundle.dir).unwrap(),
            Some(PcaScope::LegacySourceOnly)
        );
        let error = verify_dep_evidence_for_package(
            Some(&package),
            &bundle.dir,
            &TrustStore::default(),
            &policy,
        )
        .unwrap_err();
        assert!(error.contains("current package-publish"), "{error}");
    }

    #[test]
    fn checked_disproof_never_authorizes_a_dependency() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn f(x: i64) { assert(x == 0); }",
            "safe",
            None,
            vec![],
            root.path(),
            None,
            None,
        )
        .unwrap();
        let policy = ProofPolicy {
            allow_unsigned: true,
            ..ProofPolicy::default()
        };
        assert!(verify_pca(&bundle.dir).unwrap());
        assert!(verify_dep_evidence(&bundle.dir, &TrustStore::default(), &policy).is_err());

        // Rehashing an unsigned manifest as though every check passed cannot
        // turn the separately re-derived FAIL claim into a package proof.
        let path = bundle.dir.join("evidence.json");
        let mut manifest: EvidenceManifest =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for check in &mut manifest.checks {
            check.status = "PASS".into();
        }
        manifest.verdict = "PASS".into();
        std::fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        refresh_manifest_hashes(&bundle.dir).unwrap();
        assert!(!verify_pca(&bundle.dir).unwrap());
        assert!(validate_bundle(&bundle.dir).unwrap());
        assert!(verify_dep_evidence(&bundle.dir, &TrustStore::default(), &policy).is_err());
    }

    #[test]
    fn research_claim_never_authorizes_a_safe_dependency() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "fn main() { let x = 1; }",
            "research",
            None,
            vec![],
            root.path(),
            None,
            None,
        )
        .unwrap();
        // A Research label over a Safe source is now refused by the source-derived PCA
        // itself. The hash-only validator still confirms these are intact recorded bytes.
        assert!(!verify_pca(&bundle.dir).unwrap());
        assert!(validate_bundle(&bundle.dir).unwrap());
        let policy = ProofPolicy {
            allow_unsigned: true,
            ..ProofPolicy::default()
        };
        assert!(verify_dep_evidence(&bundle.dir, &TrustStore::default(), &policy).is_err());

        let (secret_key, public_key) = generate_keypair().unwrap();
        sign_pca(&bundle.dir, &secret_key).unwrap();
        let mut trust = TrustStore::default();
        trust.add(&public_key, "test signer");
        assert!(!verify_pca(&bundle.dir).unwrap());
        assert!(verify_dep_evidence(&bundle.dir, &trust, &ProofPolicy::default()).is_err());
    }

    #[test]
    fn safe_claim_cannot_hide_an_intrinsic_research_function() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "@research(authorization: \"unit-test\") fn lab_value() { let x = 1; }",
            "safe",
            None,
            vec![],
            root.path(),
            None,
            None,
        )
        .unwrap();
        // The source's intrinsic Research mode wins over a forged Safe label even before
        // package-specific policy checks. Neither a refreshed hash nor a signature fixes it.
        assert!(!verify_pca(&bundle.dir).unwrap());
        assert!(validate_bundle(&bundle.dir).unwrap());
        let policy = ProofPolicy {
            allow_unsigned: true,
            ..ProofPolicy::default()
        };
        let error = verify_dep_evidence(&bundle.dir, &TrustStore::default(), &policy)
            .expect_err("an intrinsic Research function cannot be mounted as Safe");
        assert!(error.contains("ANUBIS_DEP_PROOF_UNVERIFIED"), "{error}");
    }

    #[test]
    fn safe_claim_cannot_leave_imported_code_outside_its_analysis() {
        let root = tempfile::tempdir().unwrap();
        let bundle = build_evidence_bundle(
            "import other; fn main() { let x = 1; }",
            "safe",
            None,
            vec![],
            root.path(),
            None,
            None,
        )
        .unwrap();
        let policy = ProofPolicy {
            allow_unsigned: true,
            ..ProofPolicy::default()
        };
        let error = verify_dep_evidence(&bundle.dir, &TrustStore::default(), &policy)
            .expect_err("entry-file PCA does not prove imported code");
        assert!(error.contains("import outside"), "{error}");
    }

    #[test]
    fn nested_mode_elevator_cannot_hide_in_a_safe_package() {
        let source = b"fn main() { let x = if true { @research { let y = 1; } 1 } else { 0 }; }";
        let ast = crate::frontend::parse_source(std::str::from_utf8(source).unwrap()).unwrap();
        let Some(crate::frontend::Item::Fn { mode, .. }) = ast.items.first() else {
            panic!("expected a parsed function");
        };
        assert_eq!(*mode, crate::frontend::Mode::Safe);
        let error = ensure_safe_source(source).unwrap_err();
        assert!(error.contains("mode elevator"), "{error}");
    }

    #[test]
    fn package_admission_does_not_borrow_unverified_zk_claim() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/zk_prove_bundle");
        let policy = ProofPolicy {
            allow_unsigned: true,
            ..ProofPolicy::default()
        };
        let error = verify_dep_evidence(&fixture, &TrustStore::default(), &policy)
            .expect_err("package admission has no cryptographic ZK verifier");
        assert!(error.contains("ZK receipt"), "{error}");
    }

    #[test]
    fn one_sealed_module_cannot_authorize_an_unsealed_second_module() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let evidence = root.path().join("evidence");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&evidence).unwrap();
        let a = b"fn first() { let x = 1; }";
        let b = b"fn second() { let x = 2; }";
        std::fs::write(src.join("first.anb"), a).unwrap();
        std::fs::write(src.join("second.anb"), b).unwrap();
        std::fs::write(evidence.join("source.anubis"), a).unwrap();
        let error = bind_evidence_to_package_sources(root.path(), &evidence).unwrap_err();
        assert!(error.contains("binding every mounted module"), "{error}");

        let claimed = merkle::merkle_root(vec![
            ("first.anb".into(), a.to_vec()),
            ("second.anb".into(), b.to_vec()),
        ]);
        std::fs::write(
            evidence.join("source-merkle-leaves.json"),
            serde_json::json!({"source_merkle_root": claimed}).to_string(),
        )
        .unwrap();
        let error = bind_evidence_to_package_sources(root.path(), &evidence).unwrap_err();
        assert!(error.contains("binding every mounted module"), "{error}");
        std::fs::write(src.join("second.anb"), b"fn second() { let x = 3; }").unwrap();
        assert!(bind_evidence_to_package_sources(root.path(), &evidence).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn module_walker_rejects_symlinked_source() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(root.path().join("outside.anb"), b"fn outside() {}").unwrap();
        std::os::unix::fs::symlink(root.path().join("outside.anb"), src.join("alias.anb")).unwrap();
        let error = collect_package_modules(&src).unwrap_err();
        assert!(error.contains("symlink"), "{error}");
    }

    #[test]
    fn module_walker_rejects_oversized_source_before_read() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::File::create(src.join("oversized.anb"))
            .unwrap()
            .set_len(MAX_PACKAGE_SOURCE_BYTES + 1)
            .unwrap();
        let error = collect_package_modules(&src).unwrap_err();
        assert!(error.contains("bounded regular file"), "{error}");
    }

    #[test]
    fn publish_preflight_rejects_second_module_before_sealing() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            root.path().join("Anubis.toml"),
            b"[package]\nname = \"unit\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let entry = src.join("main.anb");
        std::fs::write(&entry, b"fn main() {}").unwrap();
        ensure_single_module_publishable(root.path(), &entry).unwrap();
        std::fs::write(src.join("other.anb"), b"fn other() {}").unwrap();
        assert!(ensure_single_module_publishable(root.path(), &entry).is_err());
    }

    #[test]
    fn publish_preflight_rejects_declared_dependency_and_import() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let entry = src.join("main.anb");
        std::fs::write(&entry, b"import other; fn main() {}").unwrap();
        std::fs::write(
            root.path().join("Anubis.toml"),
            b"[package]\nname = \"unit\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(ensure_single_module_publishable(root.path(), &entry).is_err());
        std::fs::write(&entry, b"fn main() {}").unwrap();
        std::fs::write(
            root.path().join("Anubis.toml"),
            b"[package]\nname = \"unit\"\nversion = \"0.1.0\"\n[dependencies]\nother = \"0.1\"\n",
        )
        .unwrap();
        assert!(ensure_single_module_publishable(root.path(), &entry).is_err());
    }
}
