//! Phase 6 — package manager + proof-carrying dependencies.
//!
//! Resolves `[dependencies]`, pins content hashes in `Anubis.lock`, materializes a
//! content-addressed cache, verifies signed evidence, and mounts packages as modules.

pub mod cache;
pub mod confinement;
pub mod entitlements;
pub mod lock;
pub mod merkle;
pub mod proof;
pub mod registry;
pub mod resolve_deps;
pub mod semver;
pub mod summary;
pub mod trust;

pub use lock::{LockFile, LockedPackage, LOCK_FILENAME};
pub use resolve_deps::{resolve_workspace, ResolveOptions, ResolvedDep, ResolvedWorkspace};
pub use trust::TrustStore;

/// JSON value for `dep_closure.json` / top-level evidence binding of the verified closure.
pub fn dep_closure_value(ws: &ResolvedWorkspace) -> serde_json::Value {
    let packages: Vec<_> = ws
        .deps
        .values()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "version": d.version,
                "content_sha256": d.content_sha256,
                "signer_public_key": d.signer_public_key,
                "root": d.root.display().to_string(),
                "direct": d.direct,
            })
        })
        .collect();
    serde_json::json!({
        "schema": "anubis.dep_closure.v1",
        "packages": packages,
        "lock_version": ws.lock.version,
        "transitive": true,
    })
}
