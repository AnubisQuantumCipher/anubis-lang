//! A bounded, path-sensitive source and import snapshot.
//!
//! This is a data/identity primitive, not package admission or production evidence.
//! In particular, its caller must still prove that these exact bytes and import edges
//! are what parsing, checking, and lowering consumed. It performs no filesystem I/O,
//! import resolution, or validation of an AST against the supplied edges.
//!
//! The v2 digest is SHA-256 of a domain tag followed by length-prefixed fields in
//! the order documented by `hash_graph`. It deliberately differs from the legacy
//! package Merkle root, whose single-leaf case does not bind the path.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// This schema is not a package-proof or evidence-bundle version.
pub const GRAPH_SCHEMA: &str = "anubis.source_graph.v2";

const MAX_PATH_BYTES: usize = 1024;
const MAX_NAME_BYTES: usize = 1024;
const MAX_NODES: usize = 16_384;
const MAX_EDGES: usize = 16_384;
const MAX_DEPTH: usize = 128;
const MAX_NODE_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;

/// The caller can lower the validation/hash work budget for a particular
/// operation. These limits apply after `Vec<SourceNode>` and `Vec<ImportEdge>`
/// are constructed. A future source reader must cap reads before allocation;
/// this type does not bound its caller's prior allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphLimits {
    pub nodes: usize,
    pub edges: usize,
    pub depth: usize,
    pub node_bytes: usize,
    pub total_bytes: usize,
}

impl Default for GraphLimits {
    fn default() -> Self {
        Self {
            nodes: MAX_NODES,
            edges: MAX_EDGES,
            depth: MAX_DEPTH,
            node_bytes: MAX_NODE_BYTES,
            total_bytes: MAX_TOTAL_BYTES,
        }
    }
}

impl GraphLimits {
    fn validate(self) -> Result<Self, GraphError> {
        if self.nodes == 0
            || self.depth == 0
            || self.node_bytes == 0
            || self.total_bytes == 0
            || self.nodes > MAX_NODES
            || self.edges > MAX_EDGES
            || self.depth > MAX_DEPTH
            || self.node_bytes > MAX_NODE_BYTES
            || self.total_bytes > MAX_TOTAL_BYTES
        {
            return Err(GraphError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GraphError {
    #[error("invalid source-graph limits")]
    InvalidLimits,
    #[error("source-graph {kind} limit exceeded")]
    Limit { kind: &'static str },
    #[error("invalid portable source path: {reason}")]
    Path { reason: &'static str },
    #[error("invalid source-graph namespace or import spelling")]
    Namespace,
    #[error("invalid source-graph origin identity")]
    Origin,
    #[error("source-graph entry is missing")]
    MissingEntry,
    #[error("source-graph path appears more than once in one origin")]
    DuplicatePath,
    #[error("source-graph paths collide on case-insensitive platforms")]
    CaseCollision,
    #[error("source-graph module namespace is ambiguous")]
    NamespaceCollision,
    #[error("source-graph lowered module prefix is ambiguous")]
    PrefixCollision,
    #[error("source-graph import alias is ambiguous")]
    AliasCollision,
    #[error("source-graph import edge has a missing endpoint")]
    MissingEndpoint,
    #[error("source-graph import target has a different canonical namespace")]
    TargetNamespace,
    #[error("source-graph import span is outside its source bytes")]
    Span,
    #[error("source-graph import occurrences have duplicate or overlapping spans")]
    DuplicateImportOccurrence,
    #[error("source-graph contains an import cycle")]
    Cycle,
    #[error("source-graph compilation contains an unreachable node")]
    UnreachableNode,
}

/// Logical, `/`-separated source identity. It is never an absolute filesystem
/// locator. The current portable subset is ASCII: a non-ASCII filename is an
/// explicit refusal, not a lossy conversion or Unicode normalization guess.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortablePath(String);

impl PortablePath {
    pub fn parse(value: &str) -> Result<Self, GraphError> {
        if value.is_empty() || value.len() > MAX_PATH_BYTES {
            return Err(GraphError::Path {
                reason: "empty or oversized path",
            });
        }
        if value.bytes().any(
            |b| !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'),
        ) {
            return Err(GraphError::Path {
                reason: "non-portable or non-ASCII character",
            });
        }
        for part in value.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.ends_with('.') {
                return Err(GraphError::Path {
                    reason: "empty, dot, or trailing-dot component",
                });
            }
            let stem = part.split('.').next().unwrap_or(part);
            let upper = stem.to_ascii_uppercase();
            if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (upper.len() == 4
                    && (upper.starts_with("COM") || upper.starts_with("LPT"))
                    && upper.as_bytes()[3].is_ascii_digit()
                    && upper.as_bytes()[3] != b'0')
            {
                return Err(GraphError::Path {
                    reason: "reserved device component",
                });
            }
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Origin is part of the graph identity. The dependency content digest and
/// embedded registry digest are inputs from a future independently checked
/// closure; storing them here does not validate those external claims.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceOrigin {
    Project,
    Dependency {
        name: String,
        version: String,
        content_digest: [u8; 32],
    },
    EmbeddedStdlib {
        registry_digest: [u8; 32],
    },
}

/// A path is relative to its origin's source root. Different locked packages
/// may both contain `src/lib.anb`; neither file can stand in for the other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceKey {
    pub origin: SourceOrigin,
    pub path: PortablePath,
}

impl SourceKey {
    pub fn new(origin: SourceOrigin, path: PortablePath) -> Self {
        Self { origin, path }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceNode {
    pub path: PortablePath,
    /// Canonical dotted namespace used by the checker. The entry may use the
    /// empty namespace; no other node may do so.
    pub namespace: String,
    pub origin: SourceOrigin,
    /// Exact bytes supplied by the future snapshot reader, never reopened here.
    pub bytes: Vec<u8>,
}

impl SourceNode {
    pub fn key(&self) -> SourceKey {
        SourceKey::new(self.origin.clone(), self.path.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImportEdge {
    pub source: SourceKey,
    pub target: SourceKey,
    /// Exact dotted spelling from the source import. Alternate spellings of
    /// one target are refused until the resolver itself canonicalizes them.
    pub requested: String,
    pub target_namespace: String,
    /// Byte range of the import in `source`.
    pub span_start: usize,
    pub span_end: usize,
}

/// Compilation graphs require all caller-supplied nodes reachable from the
/// entry. `CallerDeclaredPackageSurface` may include unimported modules, but
/// this primitive cannot establish that the caller supplied every importable
/// module. It grants no admission or evidence status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphCoverage {
    Compilation,
    CallerDeclaredPackageSurface,
}

#[derive(Debug, Clone)]
pub struct SourceGraphSnapshot {
    entry: SourceKey,
    coverage: GraphCoverage,
    nodes: BTreeMap<SourceKey, SourceNode>,
    edges: Vec<ImportEdge>,
    digest: [u8; 32],
}

impl SourceGraphSnapshot {
    /// Validate the graph supplied by a caller and hash its exact identity.
    /// Span checks establish only that supplied edges do not overlap; a future
    /// parser/loader must separately compare each parsed import occurrence
    /// against exactly one supplied edge and prove no occurrence was omitted.
    pub fn new(
        entry: SourceKey,
        coverage: GraphCoverage,
        nodes: Vec<SourceNode>,
        mut edges: Vec<ImportEdge>,
        limits: GraphLimits,
    ) -> Result<Self, GraphError> {
        let limits = limits.validate()?;
        if nodes.is_empty() {
            return Err(GraphError::MissingEntry);
        }
        if nodes.len() > limits.nodes {
            return Err(GraphError::Limit { kind: "node" });
        }
        if edges.len() > limits.edges {
            return Err(GraphError::Limit { kind: "edge" });
        }

        let mut by_key = BTreeMap::new();
        let mut case_paths = BTreeSet::new();
        let mut namespaces = BTreeMap::new();
        let mut prefixes = BTreeMap::new();
        let mut total_bytes = 0usize;
        for node in nodes {
            let key = node.key();
            validate_namespace(&node.namespace, key == entry)?;
            validate_origin(&node.origin)?;
            if node.bytes.len() > limits.node_bytes {
                return Err(GraphError::Limit { kind: "node byte" });
            }
            total_bytes = total_bytes
                .checked_add(node.bytes.len())
                .ok_or(GraphError::Limit { kind: "total byte" })?;
            if total_bytes > limits.total_bytes {
                return Err(GraphError::Limit { kind: "total byte" });
            }
            let namespace = node.namespace.clone();
            if by_key.insert(key.clone(), node).is_some() {
                return Err(GraphError::DuplicatePath);
            }
            if !case_paths.insert((key.origin.clone(), key.path.as_str().to_ascii_lowercase())) {
                return Err(GraphError::CaseCollision);
            }
            if let Some(previous) = namespaces.insert(namespace.clone(), key.clone()) {
                if previous != key {
                    return Err(GraphError::NamespaceCollision);
                }
            }
            let prefix = lowered_prefix(&namespace);
            if let Some(previous) = prefixes.insert(prefix, key.clone()) {
                if previous != key {
                    return Err(GraphError::PrefixCollision);
                }
            }
        }
        if !by_key.contains_key(&entry) {
            return Err(GraphError::MissingEntry);
        }

        let mut aliases = BTreeMap::<(SourceKey, String), SourceKey>::new();
        let mut occurrences = BTreeMap::<SourceKey, Vec<(usize, usize)>>::new();
        for edge in &edges {
            let source = by_key
                .get(&edge.source)
                .ok_or(GraphError::MissingEndpoint)?;
            let target = by_key
                .get(&edge.target)
                .ok_or(GraphError::MissingEndpoint)?;
            validate_namespace(&edge.requested, false)?;
            if edge.target_namespace != target.namespace {
                return Err(GraphError::TargetNamespace);
            }
            if edge.requested != target.namespace {
                return Err(GraphError::TargetNamespace);
            }
            if edge.span_start >= edge.span_end || edge.span_end > source.bytes.len() {
                return Err(GraphError::Span);
            }
            occurrences
                .entry(edge.source.clone())
                .or_default()
                .push((edge.span_start, edge.span_end));
            let alias = edge.requested.rsplit('.').next().unwrap_or(&edge.requested);
            let key = (edge.source.clone(), alias.to_owned());
            if let Some(previous) = aliases.insert(key, edge.target.clone()) {
                if previous != edge.target {
                    return Err(GraphError::AliasCollision);
                }
            }
        }
        for spans in occurrences.values_mut() {
            spans.sort_unstable();
            if spans.windows(2).any(|pair| pair[1].0 < pair[0].1) {
                return Err(GraphError::DuplicateImportOccurrence);
            }
        }
        edges.sort();
        validate_topology(&entry, coverage, &by_key, &edges, limits.depth)?;
        let digest = hash_graph(&entry, coverage, &by_key, &edges);
        Ok(Self {
            entry,
            coverage,
            nodes: by_key,
            edges,
            digest,
        })
    }

    pub fn entry(&self) -> &SourceKey {
        &self.entry
    }

    pub fn coverage(&self) -> GraphCoverage {
        self.coverage
    }

    pub fn nodes(&self) -> &BTreeMap<SourceKey, SourceNode> {
        &self.nodes
    }

    pub fn edges(&self) -> &[ImportEdge] {
        &self.edges
    }

    pub fn digest_hex(&self) -> String {
        hex::encode(self.digest)
    }
}

fn validate_namespace(value: &str, allow_empty: bool) -> Result<(), GraphError> {
    if value.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(GraphError::Namespace)
        };
    }
    if value.len() > MAX_NAME_BYTES {
        return Err(GraphError::Namespace);
    }
    for part in value.split('.') {
        let mut chars = part.bytes();
        if !matches!(chars.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(GraphError::Namespace);
        }
    }
    Ok(())
}

fn validate_origin(origin: &SourceOrigin) -> Result<(), GraphError> {
    if let SourceOrigin::Dependency { name, version, .. } = origin {
        if name.is_empty()
            || version.is_empty()
            || name.len() > MAX_NAME_BYTES
            || version.len() > MAX_NAME_BYTES
            || name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'-')
            || version
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && b != b'.' && b != b'-' && b != b'+')
        {
            return Err(GraphError::Origin);
        }
    }
    Ok(())
}

fn lowered_prefix(namespace: &str) -> String {
    namespace.replace('.', "_")
}

fn validate_topology(
    entry: &SourceKey,
    coverage: GraphCoverage,
    nodes: &BTreeMap<SourceKey, SourceNode>,
    edges: &[ImportEdge],
    max_depth: usize,
) -> Result<(), GraphError> {
    let mut adjacency = BTreeMap::<SourceKey, Vec<SourceKey>>::new();
    let mut indegree = BTreeMap::<SourceKey, usize>::new();
    let mut longest = BTreeMap::<SourceKey, usize>::new();
    for node in nodes.keys() {
        adjacency.insert(node.clone(), Vec::new());
        indegree.insert(node.clone(), 0);
        longest.insert(node.clone(), 1);
    }
    for edge in edges {
        adjacency
            .get_mut(&edge.source)
            .ok_or(GraphError::MissingEndpoint)?
            .push(edge.target.clone());
        let count = indegree
            .get_mut(&edge.target)
            .ok_or(GraphError::MissingEndpoint)?;
        *count = count
            .checked_add(1)
            .ok_or(GraphError::Limit { kind: "edge" })?;
    }
    // Kahn order avoids recursive stack growth and makes the longest import
    // chain explicit. A DFS with a visited/black mark can miss a deeper route
    // to a node previously reached through a short route.
    let mut ready: BTreeSet<SourceKey> = indegree
        .iter()
        .filter_map(|(key, degree)| (*degree == 0).then_some(key.clone()))
        .collect();
    let mut processed = 0usize;
    while let Some(key) = ready.pop_first() {
        processed += 1;
        let child_depth = longest[&key]
            .checked_add(1)
            .ok_or(GraphError::Limit { kind: "depth" })?;
        for next in &adjacency[&key] {
            let next_depth = longest.get_mut(next).ok_or(GraphError::MissingEndpoint)?;
            *next_depth = (*next_depth).max(child_depth);
            let degree = indegree.get_mut(next).ok_or(GraphError::MissingEndpoint)?;
            *degree -= 1;
            if *degree == 0 {
                ready.insert(next.clone());
            }
        }
    }
    if processed != nodes.len() {
        return Err(GraphError::Cycle);
    }
    if longest.values().any(|depth| *depth > max_depth) {
        return Err(GraphError::Limit { kind: "depth" });
    }
    if coverage == GraphCoverage::Compilation {
        let mut reached = BTreeSet::new();
        let mut pending = vec![entry.clone()];
        while let Some(key) = pending.pop() {
            if reached.insert(key.clone()) {
                pending.extend(adjacency[&key].iter().cloned());
            }
        }
        if reached.len() != nodes.len() {
            return Err(GraphError::UnreachableNode);
        }
    }
    Ok(())
}

fn put_bytes(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_be_bytes());
    h.update(bytes);
}

fn put_usize(h: &mut Sha256, value: usize) {
    h.update((value as u64).to_be_bytes());
}

fn put_origin(h: &mut Sha256, origin: &SourceOrigin) {
    match origin {
        SourceOrigin::Project => h.update([0]),
        SourceOrigin::Dependency {
            name,
            version,
            content_digest,
        } => {
            h.update([1]);
            put_bytes(h, name.as_bytes());
            put_bytes(h, version.as_bytes());
            h.update(content_digest);
        }
        SourceOrigin::EmbeddedStdlib { registry_digest } => {
            h.update([2]);
            h.update(registry_digest);
        }
    }
}

fn put_key(h: &mut Sha256, key: &SourceKey) {
    put_origin(h, &key.origin);
    put_bytes(h, key.path.as_str().as_bytes());
}

/// Canonical byte stream: tag and NUL; caller-declared coverage; entry key;
/// node count; nodes sorted by `(origin, path)` with each node encoded as
/// `(origin, path, namespace, exact bytes)`; edge count; edges sorted by the
/// Rust `ImportEdge` tuple and encoded as `(source key, target key, requested,
/// target namespace, start, end)`.
/// Every variable-length field uses an unsigned big-endian length before its
/// bytes. Numeric fields are unsigned big-endian. Nodes sort by origin and
/// path; edges sort lexicographically by the field tuple above. The tag changes on
/// any encoding change; the legacy source Merkle root is never reused here.
fn hash_graph(
    entry: &SourceKey,
    coverage: GraphCoverage,
    nodes: &BTreeMap<SourceKey, SourceNode>,
    edges: &[ImportEdge],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(GRAPH_SCHEMA.as_bytes());
    h.update([0]);
    h.update([match coverage {
        GraphCoverage::Compilation => 0,
        GraphCoverage::CallerDeclaredPackageSurface => 1,
    }]);
    put_key(&mut h, entry);
    put_usize(&mut h, nodes.len());
    for (key, node) in nodes {
        put_key(&mut h, key);
        put_bytes(&mut h, node.namespace.as_bytes());
        put_bytes(&mut h, &node.bytes);
    }
    put_usize(&mut h, edges.len());
    for edge in edges {
        put_key(&mut h, &edge.source);
        put_key(&mut h, &edge.target);
        put_bytes(&mut h, edge.requested.as_bytes());
        put_bytes(&mut h, edge.target_namespace.as_bytes());
        put_usize(&mut h, edge.span_start);
        put_usize(&mut h, edge.span_end);
    }
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> PortablePath {
        PortablePath::parse(s).unwrap()
    }

    fn project_key(s: &str) -> SourceKey {
        SourceKey::new(SourceOrigin::Project, path(s))
    }

    fn dependency_origin(name: &str, digest: u8) -> SourceOrigin {
        SourceOrigin::Dependency {
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            content_digest: [digest; 32],
        }
    }

    fn node(path_value: &str, namespace: &str, bytes: &[u8]) -> SourceNode {
        SourceNode {
            path: path(path_value),
            namespace: namespace.to_owned(),
            origin: SourceOrigin::Project,
            bytes: bytes.to_vec(),
        }
    }

    fn edge(
        source: &str,
        target: &str,
        requested: &str,
        namespace: &str,
        start: usize,
        end: usize,
    ) -> ImportEdge {
        ImportEdge {
            source: project_key(source),
            target: project_key(target),
            requested: requested.to_owned(),
            target_namespace: namespace.to_owned(),
            span_start: start,
            span_end: end,
        }
    }

    fn graph(
        nodes: Vec<SourceNode>,
        edges: Vec<ImportEdge>,
    ) -> Result<SourceGraphSnapshot, GraphError> {
        SourceGraphSnapshot::new(
            project_key("main.anb"),
            GraphCoverage::Compilation,
            nodes,
            edges,
            GraphLimits::default(),
        )
    }

    #[test]
    fn single_leaf_digest_binds_path_and_is_not_legacy_merkle() {
        let a = graph(vec![node("main.anb", "", b"fn main() {}")], vec![]).unwrap();
        let b = SourceGraphSnapshot::new(
            project_key("other.anb"),
            GraphCoverage::Compilation,
            vec![node("other.anb", "", b"fn main() {}")],
            vec![],
            GraphLimits::default(),
        )
        .unwrap();
        assert_ne!(a.digest_hex(), b.digest_hex());
        assert_ne!(
            a.digest_hex(),
            crate::package::merkle::sha256_hex(b"fn main() {}")
        );
    }

    #[test]
    fn digest_binds_bytes_namespace_edge_span_and_origin() {
        let source = b"import a.b;";
        let make = |name: &str, body: &[u8], start: usize, origin: SourceOrigin| {
            let mut target = node("a/b.anb", name, body);
            target.origin = origin;
            let mut import = edge("main.anb", "a/b.anb", name, name, start, source.len());
            import.target = target.key();
            graph(vec![node("main.anb", "", source), target], vec![import])
                .unwrap()
                .digest_hex()
        };
        let baseline = make("a.b", b"pub fn f() {}", 0, SourceOrigin::Project);
        assert_ne!(
            baseline,
            make("a.b", b"pub fn f() { 1 }", 0, SourceOrigin::Project)
        );
        assert_ne!(
            baseline,
            make("a_b", b"pub fn f() {}", 0, SourceOrigin::Project)
        );
        assert_ne!(
            baseline,
            make("a.b", b"pub fn f() {}", 1, SourceOrigin::Project)
        );
        assert_ne!(
            baseline,
            make(
                "a.b",
                b"pub fn f() {}",
                0,
                SourceOrigin::EmbeddedStdlib {
                    registry_digest: [7; 32]
                }
            )
        );
    }

    #[test]
    fn ordering_is_canonical_but_import_spelling_is_bound() {
        let nodes = vec![
            node("main.anb", "", b"import a.b; import c.d;"),
            node("a/b.anb", "a.b", b"fn b() {}"),
            node("c/d.anb", "c.d", b"fn d() {}"),
        ];
        let edges = vec![
            edge("main.anb", "a/b.anb", "a.b", "a.b", 0, 11),
            edge("main.anb", "c/d.anb", "c.d", "c.d", 12, 23),
        ];
        let a = graph(nodes.clone(), edges.clone()).unwrap();
        let mut reversed_nodes = nodes;
        reversed_nodes.reverse();
        let mut reversed_edges = edges;
        reversed_edges.reverse();
        let b = graph(reversed_nodes, reversed_edges).unwrap();
        assert_eq!(a.digest_hex(), b.digest_hex());
        let renamed = graph(
            vec![
                node("main.anb", "", b"import a.b; import c.d;"),
                node("a/b.anb", "a.b", b"fn b() {}"),
                node("c/d.anb", "alt.d", b"fn d() {}"),
            ],
            vec![
                edge("main.anb", "a/b.anb", "a.b", "a.b", 0, 11),
                edge("main.anb", "c/d.anb", "alt.d", "alt.d", 12, 23),
            ],
        )
        .unwrap();
        assert_ne!(a.digest_hex(), renamed.digest_hex());
    }

    #[test]
    fn portable_paths_fail_closed() {
        for invalid in [
            "",
            "/tmp/a.anb",
            "a//b.anb",
            "a/../b.anb",
            "a\\b.anb",
            "C:/a.anb",
            "café.anb",
            "con.anb",
            "a./b.anb",
        ] {
            assert!(PortablePath::parse(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            PortablePath::parse("src/out/ok.anb").unwrap().as_str(),
            "src/out/ok.anb"
        );
        assert_eq!(
            PortablePath::parse("src/target/ok.anub").unwrap().as_str(),
            "src/target/ok.anub"
        );
        assert_eq!(
            PortablePath::parse("src/evidence-old/ok.anubis")
                .unwrap()
                .as_str(),
            "src/evidence-old/ok.anubis"
        );
    }

    #[test]
    fn namespace_prefix_and_alias_collisions_are_refused() {
        assert_eq!(
            graph(
                vec![
                    node("main.anb", "", b"import same;"),
                    node("left.anb", "same", b"x"),
                    node("right.anb", "same", b"y")
                ],
                vec![]
            )
            .unwrap_err(),
            GraphError::NamespaceCollision
        );
        assert_eq!(
            graph(
                vec![
                    node("main.anb", "", b"import a.b; import a_b;"),
                    node("a/b.anb", "a.b", b"x"),
                    node("a_b.anb", "a_b", b"y")
                ],
                vec![]
            )
            .unwrap_err(),
            GraphError::PrefixCollision
        );
        assert_eq!(
            graph(
                vec![
                    node("main.anb", "", b"import a.foo; import b.foo;"),
                    node("a/foo.anb", "a.foo", b"x"),
                    node("b/foo.anb", "b.foo", b"y")
                ],
                vec![
                    edge("main.anb", "a/foo.anb", "a.foo", "a.foo", 0, 13),
                    edge(
                        "main.anb",
                        "b/foo.anb",
                        "b.foo",
                        "b.foo",
                        14,
                        b"import a.foo; import b.foo;".len()
                    )
                ]
            )
            .unwrap_err(),
            GraphError::AliasCollision
        );
    }

    #[test]
    fn alternate_spelling_for_one_target_is_refused_until_resolver_canonicalizes_it() {
        let source = b"import sample; import sample.lib;";
        let snapshot = graph(
            vec![
                node("main.anb", "", source),
                node("sample/lib.anb", "sample", b"x"),
            ],
            vec![
                edge("main.anb", "sample/lib.anb", "sample", "sample", 0, 14),
                edge(
                    "main.anb",
                    "sample/lib.anb",
                    "sample.lib",
                    "sample",
                    15,
                    source.len(),
                ),
            ],
        );
        assert_eq!(snapshot.unwrap_err(), GraphError::TargetNamespace);
    }

    #[test]
    fn distinct_import_occurrences_may_share_a_target_but_one_span_cannot_have_two_edges() {
        let source = b"import child; import child;";
        let first = edge("main.anb", "child.anb", "child", "child", 0, 13);
        let second = edge("main.anb", "child.anb", "child", "child", 14, source.len());
        let nodes = vec![
            node("main.anb", "", source),
            node("child.anb", "child", b"x"),
        ];
        assert!(graph(nodes.clone(), vec![first.clone(), second]).is_ok());
        assert_eq!(
            graph(nodes.clone(), vec![first.clone(), first.clone()]).unwrap_err(),
            GraphError::DuplicateImportOccurrence
        );
        let overlapping = edge("main.anb", "child.anb", "child", "child", 1, 13);
        assert_eq!(
            graph(nodes, vec![first, overlapping]).unwrap_err(),
            GraphError::DuplicateImportOccurrence
        );
        let same_span_different_target = edge("main.anb", "other.anb", "other", "other", 0, 13);
        assert_eq!(
            graph(
                vec![
                    node("main.anb", "", source),
                    node("child.anb", "child", b"x"),
                    node("other.anb", "other", b"y"),
                ],
                vec![
                    edge("main.anb", "child.anb", "child", "child", 0, 13),
                    same_span_different_target,
                ],
            )
            .unwrap_err(),
            GraphError::DuplicateImportOccurrence
        );
    }

    #[test]
    fn identical_relative_paths_in_distinct_dependency_origins_remain_distinct() {
        let mut alpha = node("src/lib.anb", "alpha", b"pub fn f() {} ");
        alpha.origin = dependency_origin("alpha", 1);
        let mut beta = node("src/lib.anb", "beta", b"pub fn f() {} ");
        beta.origin = dependency_origin("beta", 2);
        let mut alpha_edge = edge("main.anb", "src/lib.anb", "alpha", "alpha", 0, 13);
        alpha_edge.target = alpha.key();
        let mut beta_edge = edge("main.anb", "src/lib.anb", "beta", "beta", 14, 26);
        beta_edge.target = beta.key();
        let snapshot = graph(
            vec![
                node("main.anb", "", b"import alpha; import beta;"),
                alpha.clone(),
                beta.clone(),
            ],
            vec![alpha_edge.clone(), beta_edge.clone()],
        )
        .unwrap();
        assert!(snapshot.nodes().contains_key(&alpha.key()));
        assert!(snapshot.nodes().contains_key(&beta.key()));

        beta.origin = dependency_origin("beta", 3);
        beta_edge.target = beta.key();
        let changed_origin = graph(
            vec![
                node("main.anb", "", b"import alpha; import beta;"),
                alpha,
                beta,
            ],
            vec![alpha_edge, beta_edge],
        )
        .unwrap();
        assert_ne!(snapshot.digest_hex(), changed_origin.digest_hex());
    }

    #[test]
    fn case_collision_duplicate_path_missing_endpoint_and_span_refuse() {
        assert_eq!(
            graph(
                vec![node("main.anb", "", b"x"), node("main.anb", "x", b"x")],
                vec![]
            )
            .unwrap_err(),
            GraphError::DuplicatePath
        );
        assert_eq!(
            graph(
                vec![node("main.anb", "", b"x"), node("Main.anb", "Other", b"x")],
                vec![]
            )
            .unwrap_err(),
            GraphError::CaseCollision
        );
        assert_eq!(
            graph(
                vec![node("main.anb", "", b"x")],
                vec![edge("main.anb", "missing.anb", "missing", "missing", 0, 1)]
            )
            .unwrap_err(),
            GraphError::MissingEndpoint
        );
        assert_eq!(
            graph(
                vec![node("main.anb", "", b"x"), node("child.anb", "child", b"y")],
                vec![edge("main.anb", "child.anb", "child", "child", 0, 2)]
            )
            .unwrap_err(),
            GraphError::Span
        );
        assert_eq!(
            graph(
                vec![node("main.anb", "", b"x"), node("child.anb", "child", b"y")],
                vec![edge("main.anb", "child.anb", "child", "wrong", 0, 1)]
            )
            .unwrap_err(),
            GraphError::TargetNamespace
        );
        let repeated = edge("main.anb", "child.anb", "child", "child", 0, 1);
        assert_eq!(
            graph(
                vec![node("main.anb", "", b"x"), node("child.anb", "child", b"y")],
                vec![repeated.clone(), repeated]
            )
            .unwrap_err(),
            GraphError::DuplicateImportOccurrence
        );
    }

    #[test]
    fn depth_budget_checks_the_longest_route_to_a_shared_target() {
        let nodes = vec![
            node("main.anb", "", b"xy"),
            node("leaf.anb", "leaf", b"x"),
            node("via.anb", "via", b"x"),
            node("middle.anb", "middle", b"x"),
        ];
        let edges = vec![
            edge("main.anb", "leaf.anb", "leaf", "leaf", 0, 1),
            edge("main.anb", "via.anb", "via", "via", 1, 2),
            edge("via.anb", "middle.anb", "middle", "middle", 0, 1),
            edge("middle.anb", "leaf.anb", "leaf", "leaf", 0, 1),
        ];
        let longest_route = ["main.anb", "via.anb", "middle.anb", "leaf.anb"];
        let too_shallow = GraphLimits {
            depth: longest_route.len() - 1,
            ..GraphLimits::default()
        };
        assert_eq!(
            SourceGraphSnapshot::new(
                project_key("main.anb"),
                GraphCoverage::Compilation,
                nodes.clone(),
                edges.clone(),
                too_shallow,
            )
            .unwrap_err(),
            GraphError::Limit { kind: "depth" }
        );
        let just_enough = GraphLimits {
            depth: longest_route.len(),
            ..GraphLimits::default()
        };
        assert!(SourceGraphSnapshot::new(
            project_key("main.anb"),
            GraphCoverage::Compilation,
            nodes,
            edges,
            just_enough,
        )
        .is_ok());
    }

    #[test]
    fn zero_edge_budget_accepts_a_single_file_and_rejects_an_import() {
        let limits = GraphLimits {
            edges: 0,
            ..GraphLimits::default()
        };
        assert!(SourceGraphSnapshot::new(
            project_key("main.anb"),
            GraphCoverage::Compilation,
            vec![node("main.anb", "", b"x")],
            vec![],
            limits,
        )
        .is_ok());
        assert_eq!(
            SourceGraphSnapshot::new(
                project_key("main.anb"),
                GraphCoverage::Compilation,
                vec![node("main.anb", "", b"x")],
                vec![edge("main.anb", "main.anb", "main", "", 0, 1)],
                limits,
            )
            .unwrap_err(),
            GraphError::Limit { kind: "edge" }
        );
    }

    #[test]
    fn coverage_cycle_and_budgets_refuse() {
        let disconnected = vec![node("main.anb", "", b"x"), node("other.anb", "other", b"y")];
        assert_eq!(
            graph(disconnected.clone(), vec![]).unwrap_err(),
            GraphError::UnreachableNode
        );
        assert!(SourceGraphSnapshot::new(
            project_key("main.anb"),
            GraphCoverage::CallerDeclaredPackageSurface,
            disconnected,
            vec![],
            GraphLimits::default()
        )
        .is_ok());
        assert_eq!(
            graph(
                vec![
                    node("main.anb", "", b"x"),
                    node("a.anb", "a", b"x"),
                    node("b.anb", "b", b"x"),
                ],
                vec![
                    edge("main.anb", "a.anb", "a", "a", 0, 1),
                    edge("a.anb", "b.anb", "b", "b", 0, 1),
                    edge("b.anb", "a.anb", "a", "a", 0, 1),
                ]
            )
            .unwrap_err(),
            GraphError::Cycle
        );
        let limits = GraphLimits {
            nodes: 1,
            ..GraphLimits::default()
        };
        assert_eq!(
            SourceGraphSnapshot::new(
                project_key("main.anb"),
                GraphCoverage::Compilation,
                vec![node("main.anb", "", b"x"), node("other.anb", "other", b"y")],
                vec![],
                limits
            )
            .unwrap_err(),
            GraphError::Limit { kind: "node" }
        );
        let limits = GraphLimits {
            node_bytes: 1,
            ..GraphLimits::default()
        };
        assert_eq!(
            SourceGraphSnapshot::new(
                project_key("main.anb"),
                GraphCoverage::Compilation,
                vec![node("main.anb", "", b"xy")],
                vec![],
                limits
            )
            .unwrap_err(),
            GraphError::Limit { kind: "node byte" }
        );
        let limits = GraphLimits {
            total_bytes: 1,
            ..GraphLimits::default()
        };
        assert_eq!(
            SourceGraphSnapshot::new(
                project_key("main.anb"),
                GraphCoverage::Compilation,
                vec![node("main.anb", "", b"x"), node("child.anb", "child", b"y")],
                vec![],
                limits
            )
            .unwrap_err(),
            GraphError::Limit { kind: "total byte" }
        );
        let limits = GraphLimits {
            depth: 1,
            ..GraphLimits::default()
        };
        assert_eq!(
            SourceGraphSnapshot::new(
                project_key("main.anb"),
                GraphCoverage::Compilation,
                vec![node("main.anb", "", b"x"), node("child.anb", "child", b"y")],
                vec![edge("main.anb", "child.anb", "child", "child", 0, 1)],
                limits
            )
            .unwrap_err(),
            GraphError::Limit { kind: "depth" }
        );
    }
}
