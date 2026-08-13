//! Embedded Anubis-source standard library (`import std.*`).
//!
//! Modules live under `compiler/stdlib/std/*.anb` and are baked into the compiler via
//! `include_str!`. Resolution never loads `std.*` from the user's project tree (no shadowing).

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Dotted module path → embedded source text.
pub static MODULES: &[(&str, &str)] = &[
    (
        "std.collections",
        include_str!("../../stdlib/std/collections.anb"),
    ),
    ("std.iter", include_str!("../../stdlib/std/iter.anb")),
    ("std.option", include_str!("../../stdlib/std/option.anb")),
    ("std.result", include_str!("../../stdlib/std/result.anb")),
    ("std.str", include_str!("../../stdlib/std/str.anb")),
    ("std.math", include_str!("../../stdlib/std/math.anb")),
    ("std.testing", include_str!("../../stdlib/std/testing.anb")),
    ("std.io", include_str!("../../stdlib/std/io.anb")),
    ("std.pwn", include_str!("../../stdlib/std/pwn.anb")),
    ("std.crypto", include_str!("../../stdlib/std/crypto.anb")),
    ("std.time", include_str!("../../stdlib/std/time.anb")),
    ("std.net", include_str!("../../stdlib/std/net.anb")),
    ("std.rand", include_str!("../../stdlib/std/rand.anb")),
];

/// True when `dotted` is a registered stdlib module path.
pub fn is_stdlib_module(dotted: &str) -> bool {
    MODULES.iter().any(|(p, _)| *p == dotted)
}

/// Embedded source for a stdlib module, if registered.
pub fn source(dotted: &str) -> Option<&'static str> {
    MODULES.iter().find(|(p, _)| *p == dotted).map(|(_, s)| *s)
}

/// All registered dotted paths (stable order of `MODULES`).
pub fn module_paths() -> impl Iterator<Item = &'static str> {
    MODULES.iter().map(|(p, _)| *p)
}

/// Virtual filesystem path used as the load-graph identity for an embedded module.
/// Never points at a real on-disk path under the project root.
pub fn virtual_path(dotted: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("anubis-stdlib://{dotted}"))
}

/// True when `path` is a virtual stdlib load identity.
pub fn is_virtual_path(path: &std::path::Path) -> bool {
    path.to_string_lossy().starts_with("anubis-stdlib://")
}

/// Recover the dotted module path from a virtual path, if any.
pub fn dotted_from_virtual(path: &std::path::Path) -> Option<String> {
    let s = path.to_string_lossy();
    s.strip_prefix("anubis-stdlib://").map(|d| d.to_string())
}

/// SHA-256 (hex lowercase) of each embedded module source, sorted by path.
pub fn content_digests() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (path, src) in MODULES {
        let mut h = Sha256::new();
        h.update(src.as_bytes());
        m.insert((*path).to_string(), hex::encode(h.finalize()));
    }
    m
}

/// Canonical manifest text: `sha256  path` lines, sorted by path (stable lockfile form).
pub fn manifest_text() -> String {
    // BTreeMap is already ordered by module path — do not re-sort by hash line.
    let digests = content_digests();
    let mut out = String::new();
    for (p, h) in digests {
        out.push_str(&format!("{h}  {p}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_std_modules_are_nonempty_and_parse() {
        for path in module_paths() {
            let src = source(path).expect(path);
            assert!(!src.trim().is_empty(), "{path} empty");
            crate::frontend::parse_source(src).unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    }

    #[test]
    fn digests_are_stable_length() {
        let d = content_digests();
        // Keep in lockstep with MODULES (collections…net) + checked-in MANIFEST.sha256.
        assert_eq!(d.len(), MODULES.len());
        for (p, h) in &d {
            assert!(p.starts_with("std."), "{p}");
            assert_eq!(h.len(), 64, "{p}");
        }
    }

    #[test]
    fn user_path_std_math_is_not_is_stdlib_without_registry_match() {
        // only exact dotted registry keys
        assert!(!is_stdlib_module("std"));
        assert!(is_stdlib_module("std.math"));
        assert!(!is_stdlib_module("collections"));
    }

    #[test]
    fn embedded_sources_match_checked_in_manifest() {
        let lock = include_str!("../../stdlib/MANIFEST.sha256");
        let live = manifest_text();
        assert_eq!(
            lock, live,
            "stdlib MANIFEST.sha256 out of date — regenerate after editing compiler/stdlib/std/*.anb"
        );
    }
}
