//! Content-addressed Merkle roots over sorted (path, bytes) leaves.
//!
//! **Single-leaf identity:** one leaf's root equals `sha256(file_bytes)` so evidence
//! goldens that used `source_hash = sha256(source)` remain stable for single-file projects.

use sha2::{Digest, Sha256};

/// SHA-256 hex (lowercase) of raw bytes.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Merkle root over `(relative_path, content)` pairs.
///
/// Leaves are ordered by path (byte-wise). Each leaf hash is `sha256(path || 0x00 || content)`.
/// For **exactly one** leaf, the root is `sha256(content)` only (path ignored) so a single-file
/// evidence bundle matches the pre-Phase-6 `source_hash` of that file body.
pub fn merkle_root(mut files: Vec<(String, Vec<u8>)>) -> String {
    if files.is_empty() {
        return sha256_hex(b"");
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if files.len() == 1 {
        return sha256_hex(&files[0].1);
    }
    let mut level: Vec<[u8; 32]> = files
        .iter()
        .map(|(path, data)| {
            let mut h = Sha256::new();
            h.update(path.as_bytes());
            h.update([0u8]);
            h.update(data);
            let d = h.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&d);
            out
        })
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                let mut h = Sha256::new();
                h.update(level[i]);
                h.update(level[i + 1]);
                let d = h.finalize();
                let mut out = [0u8; 32];
                out.copy_from_slice(&d);
                next.push(out);
                i += 2;
            } else {
                // Odd leaf promotes unchanged.
                next.push(level[i]);
                i += 1;
            }
        }
        level = next;
    }
    hex::encode(level[0])
}

/// Walk a directory tree (non-recursive skip of `.git`, `out`, `target`) and hash all files.
pub fn merkle_root_dir(root: &std::path::Path) -> Result<String, String> {
    let files = collect_tree_files(root)?;
    Ok(merkle_root(files))
}

/// Collect relative posix paths + bytes under `root`.
pub fn collect_tree_files(root: &std::path::Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    collect_walk(root, root, &mut out)?;
    Ok(out)
}

fn collect_walk(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| {
        format!(
            "ANUBIS_DEP_UNRESOLVED: cannot read {}: {e}",
            dir.display()
        )
    })?;
    for ent in rd {
        let ent = ent.map_err(|e| e.to_string())?;
        let path = ent.path();
        let name = ent.file_name();
        let name_s = name.to_string_lossy();
        if name_s == ".git" || name_s == "out" || name_s == "target" || name_s == "Anubis.lock" {
            continue;
        }
        if path.is_dir() {
            collect_walk(root, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let data = std::fs::read(&path).map_err(|e| {
                format!("ANUBIS_DEP_UNRESOLVED: cannot read {}: {e}", path.display())
            })?;
            out.push((rel, data));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_leaf_equals_content_sha256() {
        let body = b"fn main() { print(1); }";
        let root = merkle_root(vec![("source.anubis".into(), body.to_vec())]);
        assert_eq!(root, sha256_hex(body));
    }

    #[test]
    fn empty_tree_is_sha_of_empty() {
        assert_eq!(merkle_root(vec![]), sha256_hex(b""));
    }

    #[test]
    fn multi_leaf_changes_when_any_file_changes() {
        let a = merkle_root(vec![
            ("a.anb".into(), b"one".to_vec()),
            ("b.anb".into(), b"two".to_vec()),
        ]);
        let b = merkle_root(vec![
            ("a.anb".into(), b"ONE".to_vec()),
            ("b.anb".into(), b"two".to_vec()),
        ]);
        assert_ne!(a, b);
    }

    #[test]
    fn path_order_is_canonical() {
        let x = merkle_root(vec![
            ("b.anb".into(), b"2".to_vec()),
            ("a.anb".into(), b"1".to_vec()),
        ]);
        let y = merkle_root(vec![
            ("a.anb".into(), b"1".to_vec()),
            ("b.anb".into(), b"2".to_vec()),
        ]);
        assert_eq!(x, y);
    }
}
