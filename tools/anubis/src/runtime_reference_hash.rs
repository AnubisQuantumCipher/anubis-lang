use sha2::Digest;
use std::path::Path;

pub(crate) const TREE_HASH_ALGORITHM: &str = "anubis-tree-v2-sha256-lenprefixed";
pub(crate) const REFERENCE_TREE_HASH_SCOPE: &str = "reference-root-excluding-.git-target-.DS_Store";
pub(crate) const VENDOR_TREE_HASH_SCOPE: &str = "vendor-subtree-excluding-.git-target-.DS_Store";
pub(crate) const DEFAULT_REFERENCE_HASH_SCOPE: &str =
    "vendor-only-default-excluding-.git-target-.DS_Store";
pub(crate) const TREE_HASH_SNAPSHOT_CONSISTENCY: &str =
    "two consecutive complete walks must match; not atomic against adversarial flip-back mutation";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeReferenceHashes {
    pub(crate) reference_tree_hash: String,
    pub(crate) reference_tree_hash_scope: &'static str,
    pub(crate) reference_tree_hash_complete: bool,
    pub(crate) vendor_tree_hash: String,
    pub(crate) vendor_tree_hash_scope: &'static str,
    pub(crate) vendor_tree_hash_complete: bool,
    pub(crate) complete: bool,
}

/// Hash the source that actually establishes the runtime reference identity.
///
/// The default in-repo reference uses the repository root only as a locator for
/// `vendor/risc0-circuit-rv32im`. Hashing that root would also read unrelated
/// VM pins, generated evidence, worktrees, and scratch output. Explicit external
/// references remain full-tree hashed because their root is itself the selected
/// reference project.
pub(crate) fn runtime_reference_hashes(
    root: &Path,
    vendor: &Path,
    config_source: &str,
) -> RuntimeReferenceHashes {
    let vendor_tree_hash = hash_tree_or_missing(vendor);
    let vendor_tree_hash_complete = is_sha256_hex(&vendor_tree_hash);
    if config_source == "default:in-repo-vendor" {
        return RuntimeReferenceHashes {
            reference_tree_hash: vendor_tree_hash.clone(),
            reference_tree_hash_scope: DEFAULT_REFERENCE_HASH_SCOPE,
            reference_tree_hash_complete: vendor_tree_hash_complete,
            vendor_tree_hash,
            vendor_tree_hash_scope: VENDOR_TREE_HASH_SCOPE,
            vendor_tree_hash_complete,
            complete: vendor_tree_hash_complete,
        };
    }

    let reference_tree_hash = hash_tree_or_missing(root);
    let reference_tree_hash_complete = is_sha256_hex(&reference_tree_hash);
    RuntimeReferenceHashes {
        reference_tree_hash,
        reference_tree_hash_scope: REFERENCE_TREE_HASH_SCOPE,
        reference_tree_hash_complete,
        vendor_tree_hash,
        vendor_tree_hash_scope: VENDOR_TREE_HASH_SCOPE,
        vendor_tree_hash_complete,
        complete: reference_tree_hash_complete && vendor_tree_hash_complete,
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(unix)]
fn path_identity_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_identity_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

fn hash_tree_or_missing(root: &Path) -> String {
    hash_tree_stable_with(root, || {})
}

fn hash_tree_stable_with<F>(root: &Path, between_walks: F) -> String
where
    F: FnOnce(),
{
    let first = hash_tree_once(root);
    if !is_sha256_hex(&first) {
        return first;
    }
    between_walks();
    let second = hash_tree_once(root);
    if first != second {
        return "UNSTABLE".into();
    }
    first
}

fn hash_tree_once(root: &Path) -> String {
    if !root.exists() {
        return "MISSING".into();
    }
    let mut files = vec![];
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let Ok(entry) = entry else {
            return "UNREADABLE".into();
        };
        let path = entry.path();
        let Ok(relative_path) = path.strip_prefix(root) else {
            return "UNREADABLE".into();
        };
        if should_skip_tree_hash_path(relative_path) {
            continue;
        }
        if entry.file_type().is_symlink() {
            return "UNSUPPORTED_SYMLINK".into();
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return "UNSUPPORTED_FILE_TYPE".into();
        }
        files.push((path_identity_bytes(relative_path), path.to_path_buf()));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = sha2::Sha256::new();
    hasher.update(b"anubis-tree-v2\0");
    hasher.update((files.len() as u64).to_be_bytes());
    for (relative_path, path) in files {
        hasher.update([b'F']);
        hasher.update((relative_path.len() as u64).to_be_bytes());
        hasher.update(&relative_path);
        let Ok(bytes) = std::fs::read(&path) else {
            return "UNREADABLE".into();
        };
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    hex::encode(hasher.finalize())
}

fn should_skip_tree_hash_path(path: &Path) -> bool {
    path.components().any(|component| {
        let part = component.as_os_str().to_string_lossy();
        matches!(part.as_ref(), ".git" | "target" | ".DS_Store")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let vendor = temp.path().join("vendor/risc0-circuit-rv32im");
        fs::create_dir_all(&vendor).expect("create vendor");
        fs::write(vendor.join("Cargo.toml"), "[package]\nname='fake'\n")
            .expect("write vendor source");
        fs::write(temp.path().join("unrelated.bin"), b"outside vendor")
            .expect("write root-only file");
        (temp, vendor)
    }

    #[test]
    fn default_in_repo_reference_hashes_only_the_vendor_tree() {
        let (temp, vendor) = fixture();
        let before = runtime_reference_hashes(temp.path(), &vendor, "default:in-repo-vendor");

        assert_eq!(
            before.reference_tree_hash_scope,
            DEFAULT_REFERENCE_HASH_SCOPE
        );
        assert_eq!(before.vendor_tree_hash_scope, VENDOR_TREE_HASH_SCOPE);
        assert_eq!(before.reference_tree_hash, before.vendor_tree_hash);
        assert!(before.reference_tree_hash_complete);
        assert!(before.vendor_tree_hash_complete);
        assert!(before.complete);

        fs::create_dir_all(vendor.join("target/debug")).expect("create excluded vendor target");
        fs::write(vendor.join("target/debug/generated.bin"), b"ignored")
            .expect("write excluded vendor target");
        fs::create_dir_all(vendor.join(".git/objects")).expect("create excluded vendor git dir");
        fs::write(vendor.join(".git/objects/object"), b"ignored")
            .expect("write excluded vendor git file");
        fs::write(vendor.join(".DS_Store"), b"ignored").expect("write excluded vendor metadata");
        let after_excluded_change =
            runtime_reference_hashes(temp.path(), &vendor, "default:in-repo-vendor");
        assert_eq!(before, after_excluded_change);

        fs::write(temp.path().join("unrelated.bin"), b"changed outside vendor")
            .expect("rewrite root-only file");
        let after_root_change =
            runtime_reference_hashes(temp.path(), &vendor, "default:in-repo-vendor");
        assert_eq!(
            before, after_root_change,
            "unrelated repository artifacts must not change the default Metal reference identity"
        );

        fs::write(vendor.join("Cargo.toml"), "[package]\nname='changed'\n")
            .expect("rewrite vendor source");
        let after_vendor_change =
            runtime_reference_hashes(temp.path(), &vendor, "default:in-repo-vendor");
        assert_ne!(
            before.vendor_tree_hash,
            after_vendor_change.vendor_tree_hash
        );
        assert_eq!(
            after_vendor_change.reference_tree_hash,
            after_vendor_change.vendor_tree_hash
        );
    }

    #[test]
    fn excluded_component_above_the_selected_root_does_not_empty_the_hash() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("target/reference");
        fs::create_dir_all(&root).expect("reference root");
        fs::write(root.join("Cargo.toml"), b"before").expect("reference file");

        let before = hash_tree_or_missing(&root);
        fs::write(root.join("Cargo.toml"), b"after").expect("mutate reference file");
        let after = hash_tree_or_missing(&root);

        assert_ne!(
            before, after,
            "only relative path components may be excluded"
        );
    }

    #[test]
    fn explicit_reference_hashes_the_declared_filtered_root_and_vendor_separately() {
        let (temp, vendor) = fixture();
        let before = runtime_reference_hashes(temp.path(), &vendor, "cli:--metal-reference");

        assert_eq!(before.reference_tree_hash_scope, REFERENCE_TREE_HASH_SCOPE);
        assert_eq!(before.vendor_tree_hash_scope, VENDOR_TREE_HASH_SCOPE);
        assert_ne!(before.reference_tree_hash, before.vendor_tree_hash);
        assert!(before.reference_tree_hash_complete);
        assert!(before.vendor_tree_hash_complete);
        assert!(before.complete);

        fs::create_dir_all(temp.path().join("target/cache")).expect("target tree");
        fs::write(temp.path().join("target/cache/ignored.bin"), b"ignored").expect("target file");
        fs::create_dir_all(temp.path().join(".git/objects")).expect("git tree");
        fs::write(temp.path().join(".git/objects/ignored"), b"ignored").expect("git file");
        fs::write(temp.path().join(".DS_Store"), b"ignored").expect("metadata file");
        let after_excluded =
            runtime_reference_hashes(temp.path(), &vendor, "cli:--metal-reference");
        assert_eq!(
            before.reference_tree_hash, after_excluded.reference_tree_hash,
            "excluded VCS/build/metadata components are outside the declared scope"
        );

        fs::write(temp.path().join("unrelated.bin"), b"changed outside vendor")
            .expect("rewrite root-only file");
        let after = runtime_reference_hashes(temp.path(), &vendor, "cli:--metal-reference");
        assert_ne!(before.reference_tree_hash, after.reference_tree_hash);
        assert_eq!(before.vendor_tree_hash, after.vendor_tree_hash);
    }

    #[test]
    fn tree_hash_frames_paths_and_contents_to_prevent_concatenation_collisions() {
        let left = tempfile::tempdir().expect("left tempdir");
        let right = tempfile::tempdir().expect("right tempdir");
        fs::write(left.path().join("a"), b"bc").expect("write left");
        fs::write(right.path().join("ab"), b"c").expect("write right");

        assert_ne!(
            hash_tree_or_missing(left.path()),
            hash_tree_or_missing(right.path()),
            "length-prefixing must distinguish path/content boundaries"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_rejects_symlinks_instead_of_omitting_target_content() {
        use std::os::unix::fs::symlink;

        let tree = tempfile::tempdir().expect("tree");
        let link = tree.path().join("reference-link");
        symlink("target-a", &link).expect("first symlink");
        assert_eq!(hash_tree_or_missing(tree.path()), "UNSUPPORTED_SYMLINK");
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_rejects_special_files_instead_of_omitting_them() {
        use std::os::unix::net::UnixListener;

        let tree = tempfile::tempdir().expect("tree");
        let _socket = UnixListener::bind(tree.path().join("control.sock")).expect("bind socket");
        assert_eq!(hash_tree_or_missing(tree.path()), "UNSUPPORTED_FILE_TYPE");
    }

    #[test]
    fn tree_hash_rejects_a_tree_that_changes_between_complete_walks() {
        let tree = tempfile::tempdir().expect("tree");
        let source = tree.path().join("source.rs");
        fs::write(&source, b"before").expect("write source");

        let hash = hash_tree_stable_with(tree.path(), || {
            fs::write(&source, b"after").expect("mutate between walks");
        });

        assert_eq!(hash, "UNSTABLE");
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_rejects_an_unreadable_subtree() {
        use std::os::unix::fs::PermissionsExt;

        let tree = tempfile::tempdir().expect("tree");
        let denied = tree.path().join("denied");
        fs::create_dir(&denied).expect("denied dir");
        fs::write(denied.join("secret"), b"must not be skipped").expect("denied file");
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).expect("deny traversal");

        let hash = hash_tree_or_missing(tree.path());

        fs::set_permissions(&denied, fs::Permissions::from_mode(0o700)).expect("restore traversal");
        assert_eq!(hash, "UNREADABLE");
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_distinguishes_non_utf8_path_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let left = std::path::PathBuf::from(OsString::from_vec(b"source-\xfe".to_vec()));
        let right = std::path::PathBuf::from(OsString::from_vec(b"source-\xff".to_vec()));

        assert_ne!(path_identity_bytes(&left), path_identity_bytes(&right));
    }
}
