//! Content-addressed package cache: `~/.anubis/cache/<name>-<ver>-<sha>/`.

use crate::package::merkle;
use std::path::{Path, PathBuf};

/// Default cache root: `$HOME/.anubis/cache` or temp fallback.
pub fn default_cache_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".anubis")
        .join("cache")
}

pub fn package_cache_dir(root: &Path, name: &str, version: &str, content_sha: &str) -> PathBuf {
    root.join(format!("{name}-{version}-{content_sha}"))
}

/// Materialize a package tree into the cache (copy). Returns cache path.
pub fn materialize_from_dir(
    cache_root: &Path,
    name: &str,
    version: &str,
    content_sha: &str,
    src_tree: &Path,
) -> Result<PathBuf, String> {
    let dest = package_cache_dir(cache_root, name, version, content_sha);
    if dest.is_dir() {
        verify_cache_dir(&dest, content_sha)?;
        return Ok(dest);
    }
    std::fs::create_dir_all(cache_root).map_err(|e| e.to_string())?;
    copy_dir_all(src_tree, &dest)?;
    verify_cache_dir(&dest, content_sha)?;
    Ok(dest)
}

/// Rehash cache directory; fail closed on mismatch.
pub fn verify_cache_dir(dir: &Path, expected_sha: &str) -> Result<(), String> {
    let actual = merkle::merkle_root_dir(dir)?;
    if actual != expected_sha {
        return Err(format!(
            "ANUBIS_CACHE_HASH_MISMATCH: cache `{}` has content {actual}, expected {expected_sha}",
            dir.display()
        ));
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for ent in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let ent = ent.map_err(|e| e.to_string())?;
        let from = ent.path();
        let to = dst.join(ent.file_name());
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "out" || name == "target" {
            continue;
        }
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_and_detect_tamper() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("pkg");
        std::fs::create_dir_all(pkg.join("src")).unwrap();
        std::fs::write(
            pkg.join("Anubis.toml"),
            "[package]\nname=\"p\"\nversion=\"1.0.0\"\n",
        )
        .unwrap();
        std::fs::write(pkg.join("src/lib.anb"), "pub fn f() { return 1; }\n").unwrap();
        let sha = merkle::merkle_root_dir(&pkg).unwrap();
        let cache = tmp.path().join("cache");
        let dest = materialize_from_dir(&cache, "p", "1.0.0", &sha, &pkg).unwrap();
        assert!(dest.is_dir());
        // Tamper
        std::fs::write(dest.join("src/lib.anb"), "pub fn f() { return 2; }\n").unwrap();
        let err = verify_cache_dir(&dest, &sha).unwrap_err();
        assert!(err.contains("ANUBIS_CACHE_HASH_MISMATCH"), "{err}");
    }
}
