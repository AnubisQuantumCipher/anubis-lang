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

use crate::frontend::{parse_source, Expr, Item, Span, Stmt, AST};
use crate::project::ProjectLayout;
use std::collections::{BTreeMap, BTreeSet};
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

// ---------------------------------------------------------------------------------------------
// Combine pass: fold the module graph into ONE program the existing single-file compile pipeline
// consumes unchanged. Each non-root module's functions are namespaced (`<mod>__<name>`) and its
// intra-module calls rewritten to match; a qualified `alias::f(args)` call — which the parser emits
// as `EnumConstruct{enum_name: alias, variant: f, fields: args}` (identical to `Enum::Variant`) — is
// rewritten to a namespaced `Call` when `alias` names an imported module. Everything else is left
// exactly as-is, so a program with NO imports round-trips byte-for-byte (the pipeline never changes
// for single-file code).
// ---------------------------------------------------------------------------------------------

/// Sanitize a dotted module path into a Rust-identifier prefix (`"geo.vec"` -> `"geo_vec"`).
fn module_prefix(module_path: &str) -> String {
    module_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The import alias for a dotted path — its last segment (`"geo.vec"` -> `"vec"`, `"math"` -> `"math"`).
fn import_alias(dotted: &str) -> &str {
    dotted.rsplit('.').next().unwrap_or(dotted)
}

fn collect_fn_names(items: &[Item], out: &mut BTreeSet<String>) {
    for it in items {
        match it {
            Item::Fn { name, .. } => {
                out.insert(name.clone());
            }
            Item::Module { items, .. } => collect_fn_names(items, out),
            _ => {}
        }
    }
}

fn collect_enum_names(items: &[Item], out: &mut BTreeSet<String>) {
    for it in items {
        match it {
            Item::Enum { name, .. } => {
                out.insert(name.clone());
            }
            Item::Module { items, .. } => collect_enum_names(items, out),
            _ => {}
        }
    }
}

/// Per-module rewrite context.
struct RewriteCtx {
    /// This module's namespace prefix (`""` for the root/entry module — root names stay bare).
    prefix: String,
    /// Functions defined in this module (original names), used to namespace intra-module calls.
    local_fns: BTreeSet<String>,
    /// Import alias -> target module prefix, for rewriting `alias::f` calls.
    alias_to_prefix: BTreeMap<String, String>,
    /// Enum names visible in this module (+ builtins), for the module-vs-enum ambiguity check.
    local_enums: BTreeSet<String>,
    /// First fail-closed error encountered (e.g. an ambiguous path).
    error: Option<String>,
}

/// Combine a loaded module graph into a single flat item list, fail-closed on ambiguity.
pub fn combine_graph(graph: &ModuleGraph) -> Result<Vec<Item>, String> {
    let mut out: Vec<Item> = Vec::new();
    for m in &graph.modules {
        let prefix = module_prefix(&m.module_path); // "" for the root module
        let mut local_fns = BTreeSet::new();
        collect_fn_names(&m.ast.items, &mut local_fns);
        let mut local_enums: BTreeSet<String> =
            ["Option", "Result"].iter().map(|s| s.to_string()).collect();
        collect_enum_names(&m.ast.items, &mut local_enums);
        let mut alias_to_prefix = BTreeMap::new();
        for (dotted, _span) in &m.imports {
            alias_to_prefix.insert(import_alias(dotted).to_string(), module_prefix(dotted));
        }
        let mut ctx = RewriteCtx {
            prefix,
            local_fns,
            alias_to_prefix,
            local_enums,
            error: None,
        };

        let mut items = m.ast.items.clone();
        for it in &mut items {
            rewrite_item(it, &mut ctx);
        }
        if let Some(e) = ctx.error {
            return Err(e);
        }
        for it in items {
            if !matches!(it, Item::Import { .. }) {
                out.push(it);
            }
        }
    }
    Ok(out)
}

/// Discover the project for `entry`, load its import graph, and combine it into one item list.
pub fn combine_from_entry(entry: &Path) -> Result<Vec<Item>, String> {
    let layout = ProjectLayout::discover(entry)?;
    let graph = load_graph(&layout.entry, &layout.src_root)?;
    combine_graph(&graph)
}

fn rewrite_item(it: &mut Item, ctx: &mut RewriteCtx) {
    match it {
        Item::Fn { name, body, .. } => {
            if !ctx.prefix.is_empty() {
                *name = format!("{}__{}", ctx.prefix, name);
            }
            for s in body {
                rewrite_stmt(s, ctx);
            }
        }
        Item::Impl { methods, .. } => {
            // Rewrite method bodies (so a method can call `alias::f`), but do NOT rename methods —
            // method dispatch is by receiver type, not module. (Cross-module impls are a later phase.)
            for meth in methods {
                if let Item::Fn { body, .. } = meth {
                    for s in body {
                        rewrite_stmt(s, ctx);
                    }
                }
            }
        }
        Item::Module { items, .. } => {
            for inner in items {
                rewrite_item(inner, ctx);
            }
        }
        _ => {}
    }
}

fn rewrite_stmt(s: &mut Stmt, ctx: &mut RewriteCtx) {
    match s {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => rewrite_expr(init, ctx),
        Stmt::WhileLet { expr, body, .. } => {
            rewrite_expr(expr, ctx);
            for st in body {
                rewrite_stmt(st, ctx);
            }
        }
        Stmt::Assign { target, value } => {
            rewrite_expr(target, ctx);
            rewrite_expr(value, ctx);
        }
        Stmt::If { cond, then, else_ } => {
            rewrite_expr(cond, ctx);
            for st in then {
                rewrite_stmt(st, ctx);
            }
            if let Some(e) = else_ {
                for st in e {
                    rewrite_stmt(st, ctx);
                }
            }
        }
        Stmt::While { cond, body, invariant } => {
            rewrite_expr(cond, ctx);
            for st in body {
                rewrite_stmt(st, ctx);
            }
            for inv in invariant {
                rewrite_expr(inv, ctx);
            }
        }
        Stmt::Loop { body, invariant } => {
            for st in body {
                rewrite_stmt(st, ctx);
            }
            for inv in invariant {
                rewrite_expr(inv, ctx);
            }
        }
        Stmt::For { source, body, invariant, .. } => {
            match source {
                crate::frontend::ForSource::Range { start, end } => {
                    rewrite_expr(start, ctx);
                    rewrite_expr(end, ctx);
                }
                crate::frontend::ForSource::Collection { expr } => rewrite_expr(expr, ctx),
            }
            for st in body {
                rewrite_stmt(st, ctx);
            }
            for inv in invariant {
                rewrite_expr(inv, ctx);
            }
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            for st in body {
                rewrite_stmt(st, ctx);
            }
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                for st in b {
                    rewrite_stmt(st, ctx);
                }
            }
        }
        Stmt::ExprStmt(e) => rewrite_expr(e, ctx),
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
    }
}

fn rewrite_expr(e: &mut Expr, ctx: &mut RewriteCtx) {
    // 1. Recurse into children first (so a rewritten node's operands are already resolved).
    match e {
        Expr::Call { callee, args } => {
            if !ctx.prefix.is_empty() && ctx.local_fns.contains(callee) {
                *callee = format!("{}__{}", ctx.prefix, callee);
            }
            for a in args {
                rewrite_expr(a, ctx);
            }
        }
        Expr::CallExpr { callee, args } => {
            rewrite_expr(callee, ctx);
            for a in args {
                rewrite_expr(a, ctx);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            rewrite_expr(lhs, ctx);
            rewrite_expr(rhs, ctx);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => rewrite_expr(expr, ctx),
        Expr::ArrayLiteral { elements } => {
            for x in elements {
                rewrite_expr(x, ctx);
            }
        }
        Expr::Index { base, index } => {
            rewrite_expr(base, ctx);
            rewrite_expr(index, ctx);
        }
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => rewrite_expr(inner, ctx),
        Expr::Assume(x) | Expr::Assert(x) | Expr::Try(x) => rewrite_expr(x, ctx),
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                rewrite_expr(v, ctx);
            }
        }
        Expr::FieldAccess { base, .. } => rewrite_expr(base, ctx),
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                rewrite_expr(f, ctx);
            }
        }
        Expr::Match { scrutinee, arms, .. } => {
            rewrite_expr(scrutinee, ctx);
            for a in arms {
                if let Some(g) = &mut a.guard {
                    rewrite_expr(g, ctx);
                }
                rewrite_expr(&mut a.body, ctx);
                // Patterns are NOT rewritten: an enum pattern (`Status::Ok(n)`) is never a call.
            }
        }
        Expr::If { cond, then, else_, .. } => {
            rewrite_expr(cond, ctx);
            rewrite_expr(then, ctx);
            rewrite_expr(else_, ctx);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                rewrite_expr(k, ctx);
                rewrite_expr(v, ctx);
            }
        }
        Expr::Block { stmts, tail } => {
            for st in stmts {
                rewrite_stmt(st, ctx);
            }
            if let Some(t) = tail {
                rewrite_expr(t, ctx);
            }
        }
        Expr::Lambda { body, .. } => rewrite_expr(body, ctx),
        Expr::IfLet { scrutinee, then, else_, .. } => {
            rewrite_expr(scrutinee, ctx);
            rewrite_expr(then, ctx);
            rewrite_expr(else_, ctx);
        }
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => {}
    }

    // 2. Transform `alias::f(args)` (an EnumConstruct whose head is an imported module) into a
    //    namespaced Call. Done AFTER recursion so `args` are already rewritten, and via an owned
    //    replacement so we can reassign `*e` without a borrow conflict.
    let replacement = if let Expr::EnumConstruct {
        enum_name,
        variant,
        fields,
        field_names,
        ..
    } = e
    {
        if field_names.is_empty() && ctx.alias_to_prefix.contains_key(enum_name) {
            if ctx.local_enums.contains(enum_name) {
                ctx.error.get_or_insert(format!(
                    "ANUBIS_AMBIGUOUS_PATH: `{enum_name}` names both an imported module and an enum in scope; rename one to disambiguate"
                ));
                None
            } else {
                Some(Expr::Call {
                    callee: format!("{}__{}", ctx.alias_to_prefix[enum_name], variant),
                    args: std::mem::take(fields),
                })
            }
        } else {
            None
        }
    } else {
        None
    };
    if let Some(r) = replacement {
        *e = r;
    }
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

    // --- combine pass ---

    fn fn_names(items: &[Item]) -> BTreeSet<String> {
        let mut s = BTreeSet::new();
        collect_fn_names(items, &mut s);
        s
    }

    /// Recursively collect every `Call` callee in a set of items (enough of the walk for fixtures).
    fn callees(items: &[Item]) -> Vec<String> {
        fn ex(e: &Expr, out: &mut Vec<String>) {
            match e {
                Expr::Call { callee, args } => {
                    out.push(callee.clone());
                    for a in args {
                        ex(a, out);
                    }
                }
                Expr::Binary { lhs, rhs, .. } => {
                    ex(lhs, out);
                    ex(rhs, out);
                }
                Expr::EnumConstruct { enum_name, variant, fields, .. } => {
                    out.push(format!("ENUM:{enum_name}::{variant}"));
                    for f in fields {
                        ex(f, out);
                    }
                }
                _ => {}
            }
        }
        fn st(s: &Stmt, out: &mut Vec<String>) {
            if let Stmt::ExprStmt(e) | Stmt::Let { init: e, .. } = s {
                ex(e, out);
            }
        }
        let mut out = vec![];
        for it in items {
            if let Item::Fn { body, .. } = it {
                for s in body {
                    st(s, &mut out);
                }
            }
        }
        out
    }

    #[test]
    fn combine_namespaces_module_fns_and_rewrites_qualified_calls() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let entry = write(root, "main.anb", "import math;\nfn main() { print(math::add(2, 3)); }");
        write(root, "math.anb", "fn add(a, b) { return a + b; }");

        let graph = load_graph(&entry, root).unwrap();
        let items = combine_graph(&graph).unwrap();

        let names = fn_names(&items);
        assert!(names.contains("math__add"), "module fn namespaced: {names:?}");
        assert!(!names.contains("add"), "bare `add` should be gone: {names:?}");
        assert!(names.contains("main"), "root fn stays bare: {names:?}");

        // The qualified call `math::add(...)` (parsed as an EnumConstruct) became a real call.
        let cs = callees(&items);
        assert!(cs.contains(&"math__add".to_string()), "call rewritten: {cs:?}");
        assert!(
            !cs.iter().any(|c| c.starts_with("ENUM:math")),
            "no residual EnumConstruct for the module head: {cs:?}"
        );
        // No Import items survive into the combined program.
        assert!(!items.iter().any(|it| matches!(it, Item::Import { .. })));
    }

    #[test]
    fn combine_rewrites_intra_module_bare_calls() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let entry = write(root, "main.anb", "import util;\nfn main() { print(util::pub_api()); }");
        // util's public fn calls a sibling helper by bare name -> both must be namespaced together.
        write(
            root,
            "util.anb",
            "fn pub_api() { return helper() + 1; }\nfn helper() { return 41; }",
        );

        let graph = load_graph(&entry, root).unwrap();
        let items = combine_graph(&graph).unwrap();
        let names = fn_names(&items);
        assert!(names.contains("util__pub_api") && names.contains("util__helper"), "{names:?}");
        // pub_api's bare call to `helper` was namespaced to `util__helper`.
        let cs = callees(&items);
        assert!(cs.contains(&"util__helper".to_string()), "intra-module call: {cs:?}");
    }

    #[test]
    fn combine_is_identity_for_a_single_file_program() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // No imports: a normal enum-using program must be untouched (no rename, EnumConstruct kept).
        let src = "enum S { Ok, Err }\nfn main() { let x = S::Ok; print(1); }";
        let entry = write(root, "solo.anb", src);
        let graph = load_graph(&entry, root).unwrap();
        let items = combine_graph(&graph).unwrap();

        let parsed = parse_source(src).unwrap();
        assert_eq!(items.len(), parsed.items.len(), "item count unchanged");
        assert_eq!(fn_names(&items), fn_names(&parsed.items), "no renames");
        // The enum construct S::Ok is preserved (not mistaken for a module call).
        let cs = callees(&items);
        assert!(cs.iter().any(|c| c == "ENUM:S::Ok"), "enum construct kept: {cs:?}");
    }

    #[test]
    fn combine_fails_closed_on_module_enum_ambiguity() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // The entry both imports a module `math` AND declares an enum `math` -> ambiguous head.
        let entry = write(
            root,
            "main.anb",
            "import math;\nenum math { A, B }\nfn main() { print(math::add(1, 2)); }",
        );
        write(root, "math.anb", "fn add(a, b) { return a + b; }");
        let graph = load_graph(&entry, root).unwrap();
        let err = combine_graph(&graph).unwrap_err();
        assert!(err.starts_with("ANUBIS_AMBIGUOUS_PATH"), "got: {err}");
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
