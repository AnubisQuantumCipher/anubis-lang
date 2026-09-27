//! Bounded acquisition of exact bytes below one source root.
//!
//! This is a private filesystem snapshot, not a verified source graph, parser,
//! package admission decision, or atomic view of a mutable external tree. A
//! later resolver must consume these bytes without reopening the source paths.
//! The reader includes every regular file under the selected root, including
//! files below directories named `out`, `target`, and `evidence`.
//! Linux and Android require a genuine `/proc/self/fd` view of this process to
//! reopen held `O_PATH` inodes without a file-name race. Other targets refuse.

use super::source_graph::{GraphError, PortablePath};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

// The byte, node, and depth ceilings match the source-graph primitive. The
// separate entry/work ceilings also bound traversal of directories containing
// non-source files or many names that would otherwise never become nodes.
const HARD_NODES: usize = 16_384;
const HARD_ENTRIES: usize = 65_536;
const HARD_NAME_BYTES: usize = 16 * 1024 * 1024;
const HARD_DEPTH: usize = 128;
const HARD_NODE_BYTES: usize = 64 * 1024 * 1024;
const HARD_TOTAL_BYTES: usize = 512 * 1024 * 1024;
const HARD_WORK: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureLimits {
    /// Files and directories below the root. The root itself is not counted.
    pub nodes: usize,
    /// Directory entries below the root, including non-regular entries.
    pub entries: usize,
    /// Aggregate bytes of raw directory-entry names retained during capture.
    pub name_bytes: usize,
    /// Maximum component depth below the root.
    pub depth: usize,
    pub node_bytes: usize,
    pub total_bytes: usize,
    /// Traversal operations, charged for directory entries and opened nodes.
    pub work: usize,
}

impl Default for CaptureLimits {
    fn default() -> Self {
        Self {
            nodes: HARD_NODES,
            entries: HARD_ENTRIES,
            name_bytes: HARD_NAME_BYTES,
            depth: HARD_DEPTH,
            node_bytes: HARD_NODE_BYTES,
            total_bytes: HARD_TOTAL_BYTES,
            work: HARD_WORK,
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
impl CaptureLimits {
    fn validate(self) -> Result<Self, CaptureError> {
        if self.nodes == 0
            || self.entries == 0
            || self.name_bytes == 0
            || self.depth == 0
            || self.node_bytes == 0
            || self.total_bytes == 0
            || self.work == 0
            || self.nodes > HARD_NODES
            || self.entries > HARD_ENTRIES
            || self.name_bytes > HARD_NAME_BYTES
            || self.depth > HARD_DEPTH
            || self.node_bytes > HARD_NODE_BYTES
            || self.total_bytes > HARD_TOTAL_BYTES
            || self.work > HARD_WORK
        {
            return Err(CaptureError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("source capture is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("source capture requires a nonempty root without parent components")]
    InvalidRoot,
    #[error("invalid source capture limits")]
    InvalidLimits,
    #[error("source capture {kind} limit exceeded")]
    Limit { kind: &'static str },
    #[error("unsupported source path below {location}: {reason}")]
    UnsupportedPath {
        location: String,
        reason: &'static str,
    },
    #[error("invalid portable source path: {0}")]
    PortablePath(#[from] GraphError),
    #[error("source capture refuses a symlink at {location}")]
    Symlink { location: String },
    #[error("source capture refuses a non-regular entry at {location}")]
    NonRegular { location: String },
    #[error("source capture saw a duplicate directory entry at {location}")]
    DuplicateEntry { location: String },
    #[error("source capture requires a genuine procfs fd directory: {source}")]
    ProcFdUnavailable {
        #[source]
        source: std::io::Error,
    },
    #[error("source capture requires a genuine procfs fd directory")]
    ProcFdNotProcfs,
    #[error("held source inode changed while reopening at {location}")]
    IdentityChanged { location: String },
    #[error("source capture {operation} at {location}: {source}")]
    Io {
        operation: &'static str,
        location: String,
        #[source]
        source: std::io::Error,
    },
}

/// Exact captured bytes keyed by source-relative paths. It deliberately has no
/// `verified` field or source-graph digest. `get` never reopens external files.
#[derive(Debug)]
pub struct CapturedTree {
    files: BTreeMap<PortablePath, Vec<u8>>,
}

impl CapturedTree {
    pub fn capture(root: &Path, limits: CaptureLimits) -> Result<Self, CaptureError> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            unix::capture(root, limits.validate()?)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            let _ = (root, limits);
            Err(CaptureError::UnsupportedPlatform)
        }
    }

    pub fn get(&self, path: &PortablePath) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn paths(&self) -> impl Iterator<Item = &PortablePath> {
        self.files.keys()
    }
}

#[cfg(all(test, not(any(target_os = "linux", target_os = "android"))))]
mod unsupported_platform_tests {
    use super::{CaptureError, CaptureLimits, CapturedTree};

    #[test]
    fn reader_refuses_without_a_reviewed_inode_reopen_route() {
        assert!(matches!(
            CapturedTree::capture(std::path::Path::new("."), CaptureLimits::default()),
            Err(CaptureError::UnsupportedPlatform)
        ));
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod unix {
    use super::{CaptureError, CaptureLimits, CapturedTree, PortablePath};
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::{CStr, CString};
    use std::fs::File;
    use std::io::{self, Read};
    use std::mem::MaybeUninit;
    use std::os::fd::{AsRawFd, FromRawFd, RawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Component, Path};

    const ROOT_LOCATION: &str = "<source-root>";
    const MAX_RAW_COMPONENT_BYTES: usize = 1024;

    pub(super) fn capture(
        root: &Path,
        limits: CaptureLimits,
    ) -> Result<CapturedTree, CaptureError> {
        let mut components = Vec::new();
        for component in root.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => {
                    components.push(
                        CString::new(name.as_bytes()).map_err(|_| CaptureError::InvalidRoot)?,
                    );
                }
                Component::ParentDir | Component::Prefix(_) => {
                    return Err(CaptureError::InvalidRoot)
                }
            }
        }
        if root.as_os_str().is_empty() {
            return Err(CaptureError::InvalidRoot);
        }

        // The anchor is opened once. Every subsequent source component is
        // reached through an O_PATH handle under its actual parent. This does
        // not invoke a device's open operation even if an entry is swapped.
        let proc_fd_dir = open_proc_fd_dir()?;
        let mut dir = File::open(if root.is_absolute() { "/" } else { "." }).map_err(|source| {
            CaptureError::Io {
                operation: "open anchor",
                location: ROOT_LOCATION.into(),
                source,
            }
        })?;
        for component in &components {
            let held = open_path_child(dir.as_raw_fd(), component, ROOT_LOCATION)?;
            let kind = entry_kind(&held, ROOT_LOCATION)?;
            if kind == EntryKind::Symlink {
                return Err(CaptureError::Symlink {
                    location: ROOT_LOCATION.into(),
                });
            }
            if kind != EntryKind::Directory {
                return Err(CaptureError::NonRegular {
                    location: ROOT_LOCATION.into(),
                });
            }
            dir = reopen_held(
                &proc_fd_dir,
                &held,
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                EntryKind::Directory,
                ROOT_LOCATION,
            )?;
        }

        let mut state = State {
            limits,
            nodes: 0,
            entries: 0,
            name_bytes: 0,
            work: 0,
            total_bytes: 0,
            files: BTreeMap::new(),
        };
        state.walk(&dir, &proc_fd_dir, None, 0)?;
        Ok(CapturedTree { files: state.files })
    }

    struct State {
        limits: CaptureLimits,
        nodes: usize,
        entries: usize,
        name_bytes: usize,
        work: usize,
        total_bytes: usize,
        files: BTreeMap<PortablePath, Vec<u8>>,
    }

    impl State {
        fn charge(
            &mut self,
            kind: &'static str,
            value: usize,
            ceiling: usize,
        ) -> Result<(), CaptureError> {
            let counter = match kind {
                "node" => &mut self.nodes,
                "directory entry" => &mut self.entries,
                "work" => &mut self.work,
                _ => unreachable!("only traversal counters are charged"),
            };
            *counter = counter
                .checked_add(value)
                .ok_or(CaptureError::Limit { kind })?;
            if *counter > ceiling {
                return Err(CaptureError::Limit { kind });
            }
            Ok(())
        }

        fn walk(
            &mut self,
            dir: &File,
            proc_fd_dir: &File,
            parent: Option<&PortablePath>,
            depth: usize,
        ) -> Result<(), CaptureError> {
            let directory_location = parent.map(PortablePath::as_str).unwrap_or(ROOT_LOCATION);
            let names = read_names(dir, self, directory_location)?;
            for name in names {
                let text =
                    std::str::from_utf8(&name).map_err(|_| CaptureError::UnsupportedPath {
                        location: directory_location.into(),
                        reason: "non-ASCII or non-UTF-8 component",
                    })?;
                if !text.is_ascii() {
                    return Err(CaptureError::UnsupportedPath {
                        location: directory_location.into(),
                        reason: "non-ASCII component",
                    });
                }
                let path = if let Some(parent) = parent {
                    PortablePath::parse(&format!("{}/{}", parent.as_str(), text))?
                } else {
                    PortablePath::parse(text)?
                };
                let next_depth = depth
                    .checked_add(1)
                    .ok_or(CaptureError::Limit { kind: "depth" })?;
                if next_depth > self.limits.depth {
                    return Err(CaptureError::Limit { kind: "depth" });
                }
                self.charge("node", 1, self.limits.nodes)?;
                self.charge("work", 1, self.limits.work)?;
                let c_name = CString::new(name).map_err(|_| CaptureError::UnsupportedPath {
                    location: directory_location.into(),
                    reason: "NUL component",
                })?;
                let location = path.as_str();
                let held = open_path_child(dir.as_raw_fd(), &c_name, location)?;
                match entry_kind(&held, location)? {
                    EntryKind::Directory => {
                        let child = reopen_held(
                            proc_fd_dir,
                            &held,
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                            EntryKind::Directory,
                            location,
                        )?;
                        self.walk(&child, proc_fd_dir, Some(&path), next_depth)?;
                    }
                    EntryKind::Regular => {
                        let bytes = self.read_regular(&held, proc_fd_dir, location)?;
                        if self.files.contains_key(&path) {
                            return Err(CaptureError::DuplicateEntry {
                                location: location.into(),
                            });
                        }
                        self.files.insert(path, bytes);
                    }
                    EntryKind::Symlink => {
                        return Err(CaptureError::Symlink {
                            location: location.into(),
                        })
                    }
                    EntryKind::Other => {
                        return Err(CaptureError::NonRegular {
                            location: location.into(),
                        })
                    }
                }
            }
            Ok(())
        }

        fn read_regular(
            &mut self,
            held: &File,
            proc_fd_dir: &File,
            location: &str,
        ) -> Result<Vec<u8>, CaptureError> {
            let meta = held
                .metadata()
                .map_err(|source| io_error("stat file", location, source))?;
            if !meta.file_type().is_file() {
                return Err(CaptureError::NonRegular {
                    location: location.into(),
                });
            }
            if meta.len() > self.limits.node_bytes as u64 {
                return Err(CaptureError::Limit { kind: "file byte" });
            }
            let remaining = self
                .limits
                .total_bytes
                .checked_sub(self.total_bytes)
                .ok_or(CaptureError::Limit { kind: "total byte" })?;
            if meta.len() > remaining as u64 {
                return Err(CaptureError::Limit { kind: "total byte" });
            }

            let mut file = reopen_held(
                proc_fd_dir,
                held,
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK,
                EntryKind::Regular,
                location,
            )?;

            // fstat is advisory for the allocation bound; take(limit + 1)
            // detects a regular file growing while it is read.
            let bound = self.limits.node_bytes.min(remaining);
            let mut bytes = Vec::with_capacity(meta.len() as usize);
            (&mut file)
                .take((bound + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|source| io_error("read file", location, source))?;
            if bytes.len() > self.limits.node_bytes {
                return Err(CaptureError::Limit { kind: "file byte" });
            }
            if bytes.len() > remaining {
                return Err(CaptureError::Limit { kind: "total byte" });
            }
            self.total_bytes = self
                .total_bytes
                .checked_add(bytes.len())
                .ok_or(CaptureError::Limit { kind: "total byte" })?;
            Ok(bytes)
        }
    }

    #[derive(PartialEq, Eq)]
    enum EntryKind {
        Directory,
        Regular,
        Symlink,
        Other,
    }

    fn open_path_child(dir_fd: RawFd, name: &CStr, location: &str) -> Result<File, CaptureError> {
        // O_PATH does not invoke a special file's open operation. O_NOFOLLOW
        // makes a symlink itself, rather than its target, the held inode.
        open_child(
            dir_fd,
            name,
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            "hold source entry",
            location,
        )
    }

    fn entry_kind(held: &File, location: &str) -> Result<EntryKind, CaptureError> {
        let meta = held
            .metadata()
            .map_err(|source| io_error("stat held entry", location, source))?;
        let kind = meta.file_type();
        Ok(if kind.is_dir() {
            EntryKind::Directory
        } else if kind.is_file() {
            EntryKind::Regular
        } else if kind.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::Other
        })
    }

    fn open_proc_fd_dir() -> Result<File, CaptureError> {
        let proc_fd_dir = File::open("/proc/self/fd")
            .map_err(|source| CaptureError::ProcFdUnavailable { source })?;
        let mut stat = MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: the descriptor is live and `stat` points to writable memory.
        let rc = unsafe { libc::fstatfs(proc_fd_dir.as_raw_fd(), stat.as_mut_ptr()) };
        if rc != 0 {
            return Err(CaptureError::ProcFdUnavailable {
                source: io::Error::last_os_error(),
            });
        }
        // SAFETY: fstatfs returned success and initialized `stat`.
        let stat = unsafe { stat.assume_init() };
        if stat.f_type as u64 != libc::PROC_SUPER_MAGIC as u64 {
            return Err(CaptureError::ProcFdNotProcfs);
        }

        // Check that this procfs fd directory addresses our process. A procfs
        // directory bind-mounted from another PID must not authorize reopening
        // an unrelated descriptor whose number happens to match ours.
        let guard_name =
            CString::new(proc_fd_dir.as_raw_fd().to_string()).expect("decimal fd has no NUL");
        // SAFETY: the proc directory descriptor is live; the name
        // is NUL terminated and openat returns a newly owned descriptor.
        let fd = unsafe {
            libc::openat(
                proc_fd_dir.as_raw_fd(),
                guard_name.as_ptr(),
                libc::O_PATH | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(CaptureError::ProcFdUnavailable {
                source: io::Error::last_os_error(),
            });
        }
        // SAFETY: openat succeeded and ownership is transferred exactly once.
        let reopened_guard = unsafe { File::from_raw_fd(fd) };
        let guard_meta = proc_fd_dir
            .metadata()
            .map_err(|source| CaptureError::ProcFdUnavailable { source })?;
        let reopened_meta = reopened_guard
            .metadata()
            .map_err(|source| CaptureError::ProcFdUnavailable { source })?;
        if guard_meta.dev() != reopened_meta.dev() || guard_meta.ino() != reopened_meta.ino() {
            return Err(CaptureError::ProcFdNotProcfs);
        }
        Ok(proc_fd_dir)
    }

    fn reopen_held(
        proc_fd_dir: &File,
        held: &File,
        flags: libc::c_int,
        expected: EntryKind,
        location: &str,
    ) -> Result<File, CaptureError> {
        let fd_name = CString::new(held.as_raw_fd().to_string()).expect("decimal fd has no NUL");
        // This name is inside the held, checked procfs directory, not an
        // external source path. The kernel's procfs fd link names the exact
        // O_PATH inode, which remains live through this open.
        let file = open_child(
            proc_fd_dir.as_raw_fd(),
            &fd_name,
            flags,
            "reopen held inode",
            location,
        )?;
        let held_meta = held
            .metadata()
            .map_err(|source| io_error("stat held inode", location, source))?;
        let file_meta = file
            .metadata()
            .map_err(|source| io_error("stat reopened inode", location, source))?;
        if held_meta.dev() != file_meta.dev()
            || held_meta.ino() != file_meta.ino()
            || entry_kind(held, location)? != expected
            || entry_kind(&file, location)? != expected
        {
            return Err(CaptureError::IdentityChanged {
                location: location.into(),
            });
        }
        Ok(file)
    }

    fn open_child(
        dir_fd: RawFd,
        name: &CStr,
        flags: libc::c_int,
        operation: &'static str,
        location: &str,
    ) -> Result<File, CaptureError> {
        // SAFETY: `name` is NUL terminated and `dir_fd` is live; the returned
        // descriptor is uniquely owned. Source paths use open_path_child's
        // O_PATH | O_NOFOLLOW. This helper also reopens verified procfs links.
        let fd = unsafe { libc::openat(dir_fd, name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io_error(operation, location, io::Error::last_os_error()));
        }
        // SAFETY: openat succeeded and ownership is transferred exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    struct DirStream(*mut libc::DIR);

    impl Drop for DirStream {
        fn drop(&mut self) {
            // SAFETY: fdopendir returned this stream and it is closed once.
            unsafe { libc::closedir(self.0) };
        }
    }

    fn read_names(
        dir: &File,
        state: &mut State,
        location: &str,
    ) -> Result<Vec<Vec<u8>>, CaptureError> {
        // fdopendir owns its descriptor, so duplicate the live directory handle.
        // SAFETY: fcntl receives a live descriptor and no user pointers.
        let duplicate = unsafe { libc::fcntl(dir.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        if duplicate < 0 {
            return Err(io_error(
                "duplicate directory",
                location,
                io::Error::last_os_error(),
            ));
        }
        // SAFETY: `duplicate` is a unique open descriptor. On fdopendir
        // failure we close it; on success DirStream owns and closes it.
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            let source = io::Error::last_os_error();
            // SAFETY: fdopendir did not consume the descriptor on failure.
            unsafe { libc::close(duplicate) };
            return Err(io_error("enumerate directory", location, source));
        }
        let stream = DirStream(stream);
        let mut names = BTreeSet::new();
        loop {
            // SAFETY: errno is thread local and readdir's pointer remains valid
            // until the next readdir on this stream. Validate and budget the
            // borrowed name before making any owned copy.
            let entry = unsafe {
                *errno_ptr() = 0;
                libc::readdir(stream.0)
            };
            if entry.is_null() {
                // SAFETY: read the same thread-local errno set before readdir.
                let errno = unsafe { *errno_ptr() };
                if errno != 0 {
                    return Err(io_error(
                        "enumerate directory",
                        location,
                        io::Error::from_raw_os_error(errno),
                    ));
                }
                break;
            }
            state.charge("work", 1, state.limits.work)?;
            // SAFETY: a live dirent has a NUL-terminated d_name field.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            record_name(name, state, location, &mut names)?;
        }
        Ok(names.into_iter().collect())
    }

    fn record_name(
        name: &[u8],
        state: &mut State,
        location: &str,
        names: &mut BTreeSet<Vec<u8>>,
    ) -> Result<(), CaptureError> {
        if name.is_empty() || name.len() > MAX_RAW_COMPONENT_BYTES {
            return Err(CaptureError::Limit { kind: "path byte" });
        }
        if !name.iter().all(
            |byte| matches!(*byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'),
        ) {
            return Err(CaptureError::UnsupportedPath {
                location: location.into(),
                reason: "non-portable or non-ASCII component",
            });
        }
        state.charge("directory entry", 1, state.limits.entries)?;
        if names.contains(name) {
            return Err(CaptureError::DuplicateEntry {
                location: location.into(),
            });
        }
        let next_bytes = state
            .name_bytes
            .checked_add(name.len())
            .ok_or(CaptureError::Limit { kind: "name byte" })?;
        if next_bytes > state.limits.name_bytes {
            return Err(CaptureError::Limit { kind: "name byte" });
        }
        state.name_bytes = next_bytes;
        names.insert(name.to_vec());
        Ok(())
    }

    #[cfg(target_os = "linux")]
    unsafe fn errno_ptr() -> *mut libc::c_int {
        // SAFETY: libc exposes the current thread's errno cell on Linux.
        unsafe { libc::__errno_location() }
    }

    #[cfg(target_os = "android")]
    unsafe fn errno_ptr() -> *mut libc::c_int {
        // SAFETY: Android's bionic exposes its thread-local errno via __errno.
        unsafe { libc::__errno() }
    }

    fn io_error(operation: &'static str, location: &str, source: io::Error) -> CaptureError {
        CaptureError::Io {
            operation,
            location: location.into(),
            source,
        }
    }

    #[cfg(test)]
    mod review_tests {
        use super::*;
        use std::fs;
        use std::os::unix::ffi::OsStrExt;

        fn state(limits: CaptureLimits) -> State {
            State {
                limits,
                nodes: 0,
                entries: 0,
                name_bytes: 0,
                work: 0,
                total_bytes: 0,
                files: BTreeMap::new(),
            }
        }

        #[test]
        fn raw_names_are_bounded_and_duplicates_refuse_before_copy() {
            let limits = CaptureLimits {
                name_bytes: 3,
                ..CaptureLimits::default()
            };
            let mut state = state(limits);
            let mut names = BTreeSet::new();
            record_name(b"one", &mut state, ROOT_LOCATION, &mut names).unwrap();
            assert!(matches!(
                record_name(b"two", &mut state, ROOT_LOCATION, &mut names),
                Err(CaptureError::Limit { kind: "name byte" })
            ));
            assert_eq!(names.len(), 1);
            assert!(matches!(
                record_name(b"one", &mut state, ROOT_LOCATION, &mut names),
                Err(CaptureError::DuplicateEntry { .. })
            ));
            assert!(matches!(
                record_name(b"\xff", &mut state, ROOT_LOCATION, &mut names),
                Err(CaptureError::UnsupportedPath { .. })
            ));
            assert_eq!(names.len(), 1);
        }

        #[test]
        fn held_regular_inode_survives_name_replaced_by_fifo() {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("unit.anb");
            fs::write(&source, b"captured").unwrap();
            let dir = File::open(temp.path()).unwrap();
            let proc_fd_dir = open_proc_fd_dir().unwrap();
            let name = CString::new("unit.anb").unwrap();
            let held = open_path_child(dir.as_raw_fd(), &name, "unit.anb").unwrap();
            assert!(matches!(
                entry_kind(&held, "unit.anb"),
                Ok(EntryKind::Regular)
            ));

            fs::rename(&source, temp.path().join("moved.anb")).unwrap();
            let c_source = CString::new(source.as_os_str().as_bytes()).unwrap();
            // SAFETY: the pathname is NUL terminated and points into the temp dir.
            assert_eq!(unsafe { libc::mkfifo(c_source.as_ptr(), 0o600) }, 0);
            let bytes = state(CaptureLimits::default())
                .read_regular(&held, &proc_fd_dir, "unit.anb")
                .unwrap();
            assert_eq!(bytes, b"captured");

            let replacement = open_path_child(dir.as_raw_fd(), &name, "unit.anb").unwrap();
            assert!(matches!(
                entry_kind(&replacement, "unit.anb"),
                Ok(EntryKind::Other)
            ));
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::{CaptureError, CaptureLimits, CapturedTree, PortablePath};
    use std::ffi::{CString, OsString};
    use std::fs;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::fs::symlink;

    fn path(name: &str) -> PortablePath {
        PortablePath::parse(name).expect("portable fixture path")
    }

    #[test]
    fn captures_exact_bytes_from_output_named_and_hidden_source_directories() {
        let temp = tempfile::tempdir().unwrap();
        for directory in ["out", "target", "evidence", ".hidden"] {
            let dir = temp.path().join(directory);
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("unit.anb"), directory.as_bytes()).unwrap();
        }
        let captured = CapturedTree::capture(temp.path(), CaptureLimits::default()).unwrap();
        for directory in ["out", "target", "evidence", ".hidden"] {
            let key = path(&format!("{directory}/unit.anb"));
            assert_eq!(captured.get(&key), Some(directory.as_bytes()));
        }
        assert_eq!(captured.paths().count(), 4);
    }

    #[test]
    fn refuses_symlinked_root_ancestor_and_final_entry() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("unit.anb"), b"ok").unwrap();
        symlink(&real, temp.path().join("link")).unwrap();
        assert!(matches!(
            CapturedTree::capture(&temp.path().join("link"), CaptureLimits::default()),
            Err(CaptureError::Symlink { .. })
        ));
        assert!(matches!(
            CapturedTree::capture(&temp.path().join("link/subdir"), CaptureLimits::default()),
            Err(CaptureError::Symlink { .. })
        ));

        let root = temp.path().join("source");
        fs::create_dir(&root).unwrap();
        symlink(&real, root.join("ancestor")).unwrap();
        assert!(matches!(
            CapturedTree::capture(&root, CaptureLimits::default()),
            Err(CaptureError::Symlink { .. })
        ));
        fs::remove_file(root.join("ancestor")).unwrap();
        symlink(real.join("unit.anb"), root.join("final.anb")).unwrap();
        assert!(matches!(
            CapturedTree::capture(&root, CaptureLimits::default()),
            Err(CaptureError::Symlink { .. })
        ));
    }

    #[test]
    fn refuses_fifo_and_socket_without_opening_them_as_streams() {
        let temp = tempfile::tempdir().unwrap();
        let fifo = temp.path().join("pipe");
        let c_fifo = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: the pathname is NUL terminated and points into the temp dir.
        assert_eq!(unsafe { libc::mkfifo(c_fifo.as_ptr(), 0o600) }, 0);
        assert!(matches!(
            CapturedTree::capture(temp.path(), CaptureLimits::default()),
            Err(CaptureError::NonRegular { .. })
        ));
        fs::remove_file(&fifo).unwrap();

        let _listener = std::os::unix::net::UnixListener::bind(temp.path().join("socket")).unwrap();
        assert!(matches!(
            CapturedTree::capture(temp.path(), CaptureLimits::default()),
            Err(CaptureError::NonRegular { .. })
        ));
    }

    #[test]
    fn refuses_non_ascii_and_non_utf8_names_without_lossy_conversion() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("é.anb"), b"ok").unwrap();
        assert!(matches!(
            CapturedTree::capture(temp.path(), CaptureLimits::default()),
            Err(CaptureError::UnsupportedPath { .. })
        ));
        fs::remove_file(temp.path().join("é.anb")).unwrap();
        let raw = OsString::from_vec(vec![0xff, b'.', b'a', b'n', b'b']);
        fs::write(temp.path().join(raw), b"ok").unwrap();
        assert!(matches!(
            CapturedTree::capture(temp.path(), CaptureLimits::default()),
            Err(CaptureError::UnsupportedPath { .. })
        ));
    }

    #[test]
    fn enforces_file_total_node_depth_and_work_limits() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a.anb"), b"abc").unwrap();
        let mut limits = CaptureLimits {
            node_bytes: 2,
            ..CaptureLimits::default()
        };
        assert!(matches!(
            CapturedTree::capture(temp.path(), limits),
            Err(CaptureError::Limit { kind: "file byte" })
        ));

        fs::write(temp.path().join("b.anb"), b"ab").unwrap();
        limits.node_bytes = 3;
        limits.total_bytes = 4;
        assert!(matches!(
            CapturedTree::capture(temp.path(), limits),
            Err(CaptureError::Limit { kind: "total byte" })
        ));
        limits.total_bytes = CaptureLimits::default().total_bytes;
        limits.nodes = 1;
        assert!(matches!(
            CapturedTree::capture(temp.path(), limits),
            Err(CaptureError::Limit { kind: "node" })
        ));
        limits.nodes = CaptureLimits::default().nodes;
        limits.entries = 1;
        assert!(matches!(
            CapturedTree::capture(temp.path(), limits),
            Err(CaptureError::Limit {
                kind: "directory entry"
            })
        ));
        limits.entries = CaptureLimits::default().entries;
        limits.work = 1;
        assert!(matches!(
            CapturedTree::capture(temp.path(), limits),
            Err(CaptureError::Limit { kind: "work" })
        ));

        fs::remove_file(temp.path().join("a.anb")).unwrap();
        fs::remove_file(temp.path().join("b.anb")).unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("nested/deep.anb"), b"ok").unwrap();
        limits.work = CaptureLimits::default().work;
        limits.depth = 1;
        assert!(matches!(
            CapturedTree::capture(temp.path(), limits),
            Err(CaptureError::Limit { kind: "depth" })
        ));
    }

    #[test]
    fn capture_is_unchanged_by_later_external_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unit.anb");
        fs::write(&source, b"before").unwrap();
        let captured = CapturedTree::capture(temp.path(), CaptureLimits::default()).unwrap();
        let replacement = temp.path().join("replacement.anb");
        fs::write(&replacement, b"after").unwrap();
        fs::remove_file(&source).unwrap();
        symlink(replacement, &source).unwrap();
        assert_eq!(captured.get(&path("unit.anb")), Some(&b"before"[..]));
    }
}
