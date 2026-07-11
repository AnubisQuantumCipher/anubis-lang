//! Multi-file module resolution: map `import` paths to source files and load the transitive import
//! graph, fail-closed on cycles and path escapes.
//!
//! Phase 1 of the ecosystem work. This turns a dotted `import a.b;` into a concrete file under the
//! project's `src_root` (see [`crate::project::ProjectLayout`]), parses it, and walks the transitive
//! import graph into a [`ModuleGraph`] the rest of the pipeline consumes. It is the self-contained,
//! unit-tested *resolution core*; the call-site rewrite (module-vs-enum disambiguation) and codegen
//! namespacing land in the following Phase-1 increments and build on this.
//!
//! Fail-closed by construction: an unresolved import, an import cycle, or a file that resolves
//! outside the source root is a hard `ANUBIS_IMPORT_*` error, never a silent skip.

use crate::frontend::{parse_source, Item, Span, AST};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Source-file extensions Anubis recognizes, in resolution-priority order.
pub const MODULE_EXTENSIONS: &[&str] = &["anb", "anub", "anubis"];

/// A single loaded source module.
#[derive(Debug, Clone)]
pub struct LoadedModule {
    /// Dotted module path — `""` for the entry/root module, `"a.b"` for a module reached via
    /// `import a.b`.
    pub module_path: String,
    /// The canonical source file this module was loaded from.
    pub file: PathBuf,
    /// The module's parsed AST.
    pub ast: AST,
    /// The dotted paths this module imports, each with its `import` span for diagnostics.
    pub imports: Vec<(String, Span)>,
}

/// The transitive import graph rooted at an entry file, in dependency order: a module always
/// appears before any module that imports it, so a later lowering pass can concatenate in one walk.
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    /// The canonical entry file.
    pub entry: PathBuf,
    /// Loaded modules in dependency (post-)order; the entry module is last.
    pub modules: Vec<LoadedModule>,
}

/// Map a dotted module path to a concrete source file under `src_root`, fail-closed.
///
/// `a.b` resolves to the first existing of `src_root/a/b.{anb,anub,anubis}` then
/// `src_root/a/b/mod.{...}`. A path with no source file is `ANUBIS_IMPORT_UNRESOLVED`; a file that
/// resolves OUTSIDE `src_root` (e.g. via a symlink) is `ANUBIS_IMPORT_ESCAPE`.
pub fn resolve_module_file(src_root: &Path, dotted: &str) -> Result<PathBuf, String> {
    if dotted.trim().is_empty() {
        return Err("ANUBIS_IMPORT_UNRESOLVED: empty module path".to_string());
    }
    let rel: PathBuf = dotted.split('.').collect(); // "a.b" -> a/b
    let mut candidates = Vec::new();
    for ext in MODULE_EXTENSIONS {
        candidates.push(src_root.join(&rel).with_extension(ext));
    }
    for ext in MODULE_EXTENSIONS {
        candidates.push(src_root.join(&rel).join(format!("mod.{ext}")));
    }

    let canon_root = src_root
        .canonicalize()
        .unwrap_or_else(|_| src_root.to_path_buf());
    for cand in candidates {
        if cand.is_file() {
            let canon = cand.canonicalize().unwrap_or(cand);
            if !canon.starts_with(&canon_root) {
                return Err(format!(
                    "ANUBIS_IMPORT_ESCAPE: module `{dotted}` resolves outside the source root ({})",
                    canon.display()
                ));
            }
            return Ok(canon);
        }
    }
    Err(format!(
        "ANUBIS_IMPORT_UNRESOLVED: no source file for module `{dotted}` under {}",
        src_root.display()
    ))
}

/// The dotted import paths declared by a parsed module.
pub fn collect_imports(ast: &AST) -> Vec<(String, Span)> {
    ast.items
        .iter()
        .filter_map(|it| match it {
            Item::Import { path, span } => Some((path.clone(), span.clone())),
            _ => None,
        })
        .collect()
}

/// Load the transitive import graph rooted at `entry`, resolving imports against `src_root`.
///
/// Depth-first with three-color marking so a re-imported module is loaded once and a back-edge is a
/// fail-closed `ANUBIS_IMPORT_CYCLE`. Returns modules in dependency order (imports before importers).
pub fn load_graph(entry: &Path, src_root: &Path) -> Result<ModuleGraph, String> {
    let entry_canon = entry.canonicalize().map_err(|e| {
        format!(
            "ANUBIS_IMPORT_UNRESOLVED: cannot read entry `{}`: {e}",
            entry.display()
        )
    })?;
    let mut color: BTreeMap<PathBuf, Color> = BTreeMap::new();
    let mut order: Vec<LoadedModule> = Vec::new();
    load_dfs(&entry_canon, String::new(), src_root, &mut color, &mut order)?;
    Ok(ModuleGraph {
        entry: entry_canon,
        modules: order,
    })
}

#[derive(PartialEq, Clone, Copy)]
enum Color {
    Gray,
    Black,
}

fn load_dfs(
    file: &Path,
    module_path: String,
    src_root: &Path,
    color: &mut BTreeMap<PathBuf, Color>,
    order: &mut Vec<LoadedModule>,
) -> Result<(), String> {
    match color.get(file) {
        Some(Color::Black) => return Ok(()), // already fully loaded
        Some(Color::Gray) => {
            return Err(format!(
                "ANUBIS_IMPORT_CYCLE: module `{}` ({}) is part of an import cycle",
                if module_path.is_empty() {
                    "<entry>"
                } else {
                    &module_path
                },
                file.display()
            ));
        }
        None => {}
    }
    color.insert(file.to_path_buf(), Color::Gray);

    let src = std::fs::read_to_string(file).map_err(|e| {
        format!(
            "ANUBIS_IMPORT_UNRESOLVED: cannot read `{}`: {e}",
            file.display()
        )
    })?;
    let ast = parse_source(&src)
        .map_err(|e| format!("ANUBIS_IMPORT_PARSE: in `{}`: {e}", file.display()))?;
    let imports = collect_imports(&ast);

    for (dotted, _span) in &imports {
        let child = resolve_module_file(src_root, dotted)?;
        load_dfs(&child, dotted.clone(), src_root, color, order)?;
    }

    color.insert(file.to_path_buf(), Color::Black);
    order.push(LoadedModule {
        module_path,
        file: file.to_path_buf(),
        ast,
        imports,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn resolves_dotted_paths_to_flat_and_nested_and_mod_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "math.anb", "fn add(a, b) { return a + b; }");
        write(root, "geo/vec.anb", "fn dot() { return 0; }");
        write(root, "net/mod.anb", "fn get() { return 1; }");

        assert_eq!(
            resolve_module_file(root, "math").unwrap(),
            root.join("math.anb").canonicalize().unwrap()
        );
        assert_eq!(
            resolve_module_file(root, "geo.vec").unwrap(),
            root.join("geo/vec.anb").canonicalize().unwrap()
        );
        // Directory module via mod.anb.
        assert_eq!(
            resolve_module_file(root, "net").unwrap(),
            root.join("net/mod.anb").canonicalize().unwrap()
        );
    }

    #[test]
    fn unresolved_import_is_fail_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_module_file(tmp.path(), "nope.missing").unwrap_err();
        assert!(err.starts_with("ANUBIS_IMPORT_UNRESOLVED"), "got: {err}");
    }

    #[test]
    fn load_graph_orders_dependencies_before_importers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let entry = write(root, "main.anb", "import math;\nfn main() { print(math::add(2, 3)); }");
        write(root, "math.anb", "fn add(a, b) { return a + b; }");

        let graph = load_graph(&entry, root).unwrap();
        assert_eq!(graph.modules.len(), 2);
        // Dependency (math) loads before the importer (entry, module_path "").
        assert_eq!(graph.modules[0].module_path, "math");
        assert_eq!(graph.modules[1].module_path, "");
        assert_eq!(graph.modules[1].imports.len(), 1);
        assert_eq!(graph.modules[1].imports[0].0, "math");
    }

    #[test]
    fn import_cycle_is_fail_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let entry = write(root, "a.anb", "import b;\nfn main() { print(1); }");
        write(root, "b.anb", "import a;\nfn f() { return 2; }");

        let err = load_graph(&entry, root).unwrap_err();
        assert!(err.starts_with("ANUBIS_IMPORT_CYCLE"), "got: {err}");
    }

    #[test]
    fn a_module_imported_twice_loads_once() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let entry = write(
            root,
            "main.anb",
            "import a;\nimport b;\nfn main() { print(1); }",
        );
        write(root, "a.anb", "import shared;\nfn fa() { return shared::v(); }");
        write(root, "b.anb", "import shared;\nfn fb() { return shared::v(); }");
        write(root, "shared.anb", "fn v() { return 7; }");

        let graph = load_graph(&entry, root).unwrap();
        // shared appears exactly once despite two importers (diamond).
        let shared_count = graph
            .modules
            .iter()
            .filter(|m| m.module_path == "shared")
            .count();
        assert_eq!(shared_count, 1, "diamond dependency loaded more than once");
        // 4 modules total: shared, a, b, entry.
        assert_eq!(graph.modules.len(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_fail_closed() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret.anb", "fn s() { return 0; }");
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // A module file inside src_root that symlinks to a file outside it.
        symlink(
            outside.path().join("secret.anb"),
            root.join("escape.anb"),
        )
        .unwrap();
        let err = resolve_module_file(root, "escape").unwrap_err();
        assert!(err.starts_with("ANUBIS_IMPORT_ESCAPE"), "got: {err}");
    }
}
