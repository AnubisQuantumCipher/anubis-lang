//! Private project-only resolver over a previously captured source tree.
//!
//! This comparison path does no filesystem I/O. It deliberately does not
//! admit packages, attach source-graph v2 identities, or change the production
//! checker, CLI, or native lowerer. `CapturedTree` already owns the project
//! bytes; embedded stdlib bytes come from the compiler's static registry.
//! Dependency mounts and published-package surface enumeration are separate
//! work and must not be inferred from a successful compilation graph here.

use super::{collect_enum_names, collect_fn_names, collect_imports, import_alias, module_prefix};
use crate::frontend::{parse_source, Item, Span, AST};
use crate::package::source_graph::PortablePath;
use crate::package::source_graph_reader::CapturedTree;
use crate::stdlib;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

const MAX_MODULES: usize = 16_384;
const MAX_IMPORTS: usize = 16_384;
const MAX_DEPTH: usize = 128;
const MAX_PARSED_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolveLimits {
    pub modules: usize,
    pub imports: usize,
    pub depth: usize,
    pub parsed_bytes: usize,
}

impl Default for ResolveLimits {
    fn default() -> Self {
        Self {
            modules: MAX_MODULES,
            imports: MAX_IMPORTS,
            depth: MAX_DEPTH,
            parsed_bytes: MAX_PARSED_BYTES,
        }
    }
}

impl ResolveLimits {
    fn validate(self) -> Result<Self, CapturedResolveError> {
        if self.modules == 0
            || self.imports == 0
            || self.depth == 0
            || self.parsed_bytes == 0
            || self.modules > MAX_MODULES
            || self.imports > MAX_IMPORTS
            || self.depth > MAX_DEPTH
            || self.parsed_bytes > MAX_PARSED_BYTES
        {
            return Err(CapturedResolveError::InvalidLimits);
        }
        Ok(self)
    }
}

/// `Project` keys are relative to the captured source root. The embedded
/// variant names a static registry entry and can never resolve to `src/std`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ModuleKey {
    Project(PortablePath),
    EmbeddedStdlib(String),
}

/// Every parsed `import` occurrence survives independently, even when two
/// occurrences select the same module. `span` is a byte range in `importer`.
#[derive(Debug, Clone)]
pub(crate) struct CapturedImport {
    pub importer: ModuleKey,
    pub requested: String,
    pub span: Span,
    pub target: ModuleKey,
}

/// `ast.trait_env` is the parser's original-item sidecar, retained per module
/// rather than reconstructed from an already desugared flat item list.
#[derive(Debug, Clone)]
pub(crate) struct CapturedModule<'a> {
    pub key: ModuleKey,
    pub namespace: String,
    pub bytes: &'a [u8],
    pub ast: AST,
    pub imports: Vec<CapturedImport>,
}

/// Modules appear after their imports. This is a compilation closure only;
/// unimported package modules are not enumerated or certified by this type.
#[derive(Debug, Clone)]
pub(crate) struct CapturedProjectGraph<'a> {
    pub entry: ModuleKey,
    pub modules: Vec<CapturedModule<'a>>,
}

#[derive(Debug, Error)]
pub(crate) enum CapturedResolveError {
    #[error("invalid captured resolver limits")]
    InvalidLimits,
    #[error("captured resolver {kind} limit exceeded")]
    Limit { kind: &'static str },
    #[error("entry is not present in the captured tree: {path:?}")]
    MissingEntry { path: PortablePath },
    #[error("source bytes are not UTF-8 for {module:?}")]
    InvalidUtf8 { module: ModuleKey },
    #[error("source parse failed in {module:?}: {message}")]
    Parse { module: ModuleKey, message: String },
    #[error("nested import `{requested}` in {importer:?} at {span:?} is unsupported")]
    NestedImport {
        importer: ModuleKey,
        requested: String,
        span: Span,
    },
    #[error("invalid import spelling `{requested}` in {importer:?} at {span:?}")]
    InvalidSpelling {
        importer: ModuleKey,
        requested: String,
        span: Span,
    },
    #[error("import span is invalid in {importer:?} at {span:?}")]
    InvalidSpan { importer: ModuleKey, span: Span },
    #[error("unknown embedded stdlib import `{requested}` in {importer:?} at {span:?}")]
    UnknownStdlib {
        importer: ModuleKey,
        requested: String,
        span: Span,
    },
    #[error("unresolved project import `{requested}` in {importer:?} at {span:?}")]
    Unresolved {
        importer: ModuleKey,
        requested: String,
        span: Span,
    },
    #[error("embedded stdlib {importer:?} imports non-stdlib `{requested}` at {span:?}")]
    StdlibBoundary {
        importer: ModuleKey,
        requested: String,
        span: Span,
    },
    #[error("captured import cycle through {module:?}")]
    Cycle { module: ModuleKey },
    #[error("one source {module:?} was selected as both `{first}` and `{second}`")]
    ConflictingNamespace {
        module: ModuleKey,
        first: String,
        second: String,
    },
    #[error("module prefix `{prefix}` collides between {first:?} and {second:?}")]
    PrefixCollision {
        prefix: String,
        first: ModuleKey,
        second: ModuleKey,
    },
    #[error("portable source paths collide by case between {first:?} and {second:?}")]
    CaseCollision { first: ModuleKey, second: ModuleKey },
    #[error("lowered function `{name}` collides between {first:?} and {second:?}")]
    FunctionCollision {
        name: String,
        first: ModuleKey,
        second: ModuleKey,
    },
    #[error("import alias `{alias}` in {importer:?} selects both {first:?} and {second:?}")]
    AliasCollision {
        importer: ModuleKey,
        alias: String,
        first: ModuleKey,
        second: ModuleKey,
    },
    #[error("import alias `{alias}` collides with an enum in {importer:?}")]
    EnumAliasCollision { importer: ModuleKey, alias: String },
    #[error("captured import path cannot be represented portably: `{requested}`")]
    InvalidPortablePath { requested: String },
    #[error("trait {name} collides between {first:?} and {second:?}")]
    TraitNameCollision {
        name: String,
        first: ModuleKey,
        second: ModuleKey,
    },
    #[error("captured module combine failed: {message}")]
    Combine { message: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Gray,
    Black,
}

struct State<'a> {
    tree: &'a CapturedTree,
    limits: ResolveLimits,
    module_count: usize,
    import_count: usize,
    parsed_bytes: usize,
    colors: BTreeMap<ModuleKey, Color>,
    namespaces: BTreeMap<ModuleKey, String>,
    prefixes: BTreeMap<String, ModuleKey>,
    casefold_paths: BTreeMap<String, ModuleKey>,
    lowered_functions: BTreeMap<String, ModuleKey>,
    modules: Vec<CapturedModule<'a>>,
}

/// Build a private compilation graph from `entry`, which is relative to the
/// captured project source root. No pathname is reopened. Callers must first
/// establish that dependency mounts are absent; this API does not resolve
/// dependencies or bind the result to checker input or native output.
pub(crate) fn load_captured_project<'a>(
    tree: &'a CapturedTree,
    entry: PortablePath,
    limits: ResolveLimits,
) -> Result<CapturedProjectGraph<'a>, CapturedResolveError> {
    let limits = limits.validate()?;
    if tree.get(&entry).is_none() {
        return Err(CapturedResolveError::MissingEntry { path: entry });
    }
    let entry = ModuleKey::Project(entry);
    let mut state = State {
        tree,
        limits,
        module_count: 0,
        import_count: 0,
        parsed_bytes: 0,
        colors: BTreeMap::new(),
        namespaces: BTreeMap::new(),
        prefixes: BTreeMap::new(),
        casefold_paths: BTreeMap::new(),
        lowered_functions: BTreeMap::new(),
        modules: Vec::new(),
    };
    state.visit(entry.clone(), String::new(), 1)?;
    Ok(CapturedProjectGraph {
        entry,
        modules: state.modules,
    })
}

impl<'a> State<'a> {
    fn bytes_for(&self, key: &ModuleKey) -> Result<&'a [u8], CapturedResolveError> {
        match key {
            ModuleKey::Project(path) => self
                .tree
                .get(path)
                .ok_or_else(|| CapturedResolveError::MissingEntry { path: path.clone() }),
            ModuleKey::EmbeddedStdlib(name) => {
                stdlib::source(name).map(str::as_bytes).ok_or_else(|| {
                    CapturedResolveError::UnknownStdlib {
                        importer: key.clone(),
                        requested: name.clone(),
                        span: Span::default(),
                    }
                })
            }
        }
    }

    fn visit(
        &mut self,
        key: ModuleKey,
        namespace: String,
        depth: usize,
    ) -> Result<(), CapturedResolveError> {
        if self.colors.get(&key) == Some(&Color::Gray) {
            return Err(CapturedResolveError::Cycle { module: key });
        }
        if let Some(first) = self.namespaces.get(&key) {
            if first != &namespace {
                return Err(CapturedResolveError::ConflictingNamespace {
                    module: key,
                    first: first.clone(),
                    second: namespace,
                });
            }
        }
        if self.colors.get(&key) == Some(&Color::Black) {
            return Ok(());
        }
        if depth > self.limits.depth {
            return Err(CapturedResolveError::Limit { kind: "depth" });
        }
        self.module_count = self
            .module_count
            .checked_add(1)
            .ok_or(CapturedResolveError::Limit { kind: "module" })?;
        if self.module_count > self.limits.modules {
            return Err(CapturedResolveError::Limit { kind: "module" });
        }

        if let ModuleKey::Project(path) = &key {
            let folded = path.as_str().to_ascii_lowercase();
            if let Some(first) = self.casefold_paths.get(&folded) {
                if first != &key {
                    return Err(CapturedResolveError::CaseCollision {
                        first: first.clone(),
                        second: key,
                    });
                }
            } else {
                self.casefold_paths.insert(folded, key.clone());
            }
        }

        let prefix = module_prefix(&namespace);
        if let Some(first) = self.prefixes.get(&prefix) {
            if first != &key {
                return Err(CapturedResolveError::PrefixCollision {
                    prefix,
                    first: first.clone(),
                    second: key,
                });
            }
        } else {
            self.prefixes.insert(prefix.clone(), key.clone());
        }
        self.namespaces.insert(key.clone(), namespace.clone());
        self.colors.insert(key.clone(), Color::Gray);

        let bytes = self.bytes_for(&key)?;
        self.parsed_bytes =
            self.parsed_bytes
                .checked_add(bytes.len())
                .ok_or(CapturedResolveError::Limit {
                    kind: "parsed byte",
                })?;
        if self.parsed_bytes > self.limits.parsed_bytes {
            return Err(CapturedResolveError::Limit {
                kind: "parsed byte",
            });
        }
        let source = std::str::from_utf8(bytes).map_err(|_| CapturedResolveError::InvalidUtf8 {
            module: key.clone(),
        })?;
        let ast = parse_source(source).map_err(|message| CapturedResolveError::Parse {
            module: key.clone(),
            message,
        })?;
        reject_nested_imports(&ast, &key)?;
        self.check_function_names(&key, &prefix, &ast)?;
        let raw_imports = collect_imports(&ast);
        self.import_count = self
            .import_count
            .checked_add(raw_imports.len())
            .ok_or(CapturedResolveError::Limit { kind: "import" })?;
        if self.import_count > self.limits.imports {
            return Err(CapturedResolveError::Limit { kind: "import" });
        }
        let mut local_enums = BTreeSet::from(["Option".to_owned(), "Result".to_owned()]);
        collect_enum_names(&ast.items, &mut local_enums);
        let mut aliases = BTreeMap::<String, ModuleKey>::new();
        let mut imports = Vec::with_capacity(raw_imports.len());
        let mut previous_end = 0;
        for (requested, span) in raw_imports {
            if span.start < previous_end
                || span.start >= span.end
                || span.end > source.len()
                || !source.is_char_boundary(span.start)
                || !source.is_char_boundary(span.end)
            {
                return Err(CapturedResolveError::InvalidSpan {
                    importer: key.clone(),
                    span,
                });
            }
            previous_end = span.end;
            if matches!(key, ModuleKey::EmbeddedStdlib(_)) && !stdlib::is_stdlib_module(&requested)
            {
                return Err(CapturedResolveError::StdlibBoundary {
                    importer: key.clone(),
                    requested,
                    span,
                });
            }
            let target = self.resolve_target(&key, &requested, span)?;
            let alias = import_alias(&requested).to_owned();
            if local_enums.contains(&alias) {
                return Err(CapturedResolveError::EnumAliasCollision {
                    importer: key.clone(),
                    alias,
                });
            }
            if let Some(first) = aliases.get(&alias) {
                if first != &target {
                    return Err(CapturedResolveError::AliasCollision {
                        importer: key.clone(),
                        alias,
                        first: first.clone(),
                        second: target,
                    });
                }
            } else {
                aliases.insert(alias, target.clone());
            }
            imports.push(CapturedImport {
                importer: key.clone(),
                requested: requested.clone(),
                span,
                target: target.clone(),
            });
            self.visit(target, requested, depth + 1)?;
        }
        self.colors.insert(key.clone(), Color::Black);
        self.modules.push(CapturedModule {
            key,
            namespace,
            bytes,
            ast,
            imports,
        });
        Ok(())
    }

    fn check_function_names(
        &mut self,
        key: &ModuleKey,
        prefix: &str,
        ast: &AST,
    ) -> Result<(), CapturedResolveError> {
        let mut names = BTreeSet::new();
        collect_fn_names(&ast.items, &mut names);
        for name in names {
            let lowered = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}__{name}")
            };
            if let Some(first) = self.lowered_functions.get(&lowered) {
                if first != key {
                    return Err(CapturedResolveError::FunctionCollision {
                        name: lowered,
                        first: first.clone(),
                        second: key.clone(),
                    });
                }
            } else {
                self.lowered_functions.insert(lowered, key.clone());
            }
        }
        Ok(())
    }

    fn resolve_target(
        &self,
        importer: &ModuleKey,
        requested: &str,
        span: Span,
    ) -> Result<ModuleKey, CapturedResolveError> {
        if !valid_dotted(requested) {
            return Err(CapturedResolveError::InvalidSpelling {
                importer: importer.clone(),
                requested: requested.into(),
                span,
            });
        }
        if stdlib::is_stdlib_module(requested) {
            return Ok(ModuleKey::EmbeddedStdlib(requested.into()));
        }
        if requested == "std" || requested.starts_with("std.") {
            return Err(CapturedResolveError::UnknownStdlib {
                importer: importer.clone(),
                requested: requested.into(),
                span,
            });
        }

        let rel = requested.replace('.', "/");
        for extension in super::MODULE_EXTENSIONS {
            let candidate = project_path(&format!("{rel}.{extension}"), requested)?;
            if self.tree.get(&candidate).is_some() {
                return Ok(ModuleKey::Project(candidate));
            }
        }
        for extension in super::MODULE_EXTENSIONS {
            let candidate = project_path(&format!("{rel}/mod.{extension}"), requested)?;
            if self.tree.get(&candidate).is_some() {
                return Ok(ModuleKey::Project(candidate));
            }
        }
        Err(CapturedResolveError::Unresolved {
            importer: importer.clone(),
            requested: requested.into(),
            span,
        })
    }
}

/// The legacy import collector sees top-level items only. Until nested import
/// semantics are defined for this captured graph, refuse every import under
/// another item instead of silently omitting it from the closure.
fn reject_nested_imports(ast: &AST, importer: &ModuleKey) -> Result<(), CapturedResolveError> {
    let mut stack = vec![ast.items.iter()];
    while let Some(items) = stack.last_mut() {
        let Some(item) = items.next() else {
            stack.pop();
            continue;
        };
        match item {
            Item::Import { path, span } if stack.len() > 1 => {
                return Err(CapturedResolveError::NestedImport {
                    importer: importer.clone(),
                    requested: path.clone(),
                    span: *span,
                });
            }
            Item::Module { items, .. } => stack.push(items.iter()),
            Item::Impl { methods, .. } | Item::Trait { methods, .. } => {
                stack.push(methods.iter());
            }
            _ => {}
        }
    }
    Ok(())
}

fn valid_dotted(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

fn project_path(value: &str, requested: &str) -> Result<PortablePath, CapturedResolveError> {
    PortablePath::parse(value).map_err(|_| CapturedResolveError::InvalidPortablePath {
        requested: requested.into(),
    })
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::*;
    use crate::frontend::{Expr, Stmt};
    use crate::package::source_graph_reader::CaptureLimits;
    use std::fs;

    fn capture(files: &[(&str, &[u8])]) -> (tempfile::TempDir, CapturedTree) {
        let temp = tempfile::tempdir().unwrap();
        for &(name, bytes) in files {
            let file = temp.path().join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        let tree = CapturedTree::capture(temp.path(), CaptureLimits::default()).unwrap();
        (temp, tree)
    }

    fn entry() -> PortablePath {
        PortablePath::parse("main.anb").unwrap()
    }

    #[test]
    fn captured_combine_uses_saved_module_bytes_and_existing_call_rewrite() {
        let (temp, tree) = capture(&[
            (
                "main.anb",
                b"import util;\nimport util;\nfn main() { print(util::value()); }",
            ),
            ("util.anb", b"pub fn value() { return 1; }"),
        ]);
        let legacy = super::super::combine_from_entry(&temp.path().join("main.anb")).unwrap();
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        fs::write(
            temp.path().join("util.anb"),
            b"pub fn replacement() { return 2; }",
        )
        .unwrap();

        let combined = super::super::combine_captured_project(&graph).unwrap();
        assert_eq!(format!("{:?}", combined.items), format!("{legacy:?}"));
        assert!(combined
            .items
            .iter()
            .any(|item| { matches!(item, Item::Fn { name, .. } if name == "util__value") }));
        assert!(!combined
            .items
            .iter()
            .any(|item| { matches!(item, Item::Fn { name, .. } if name == "util__replacement") }));
        let main_rewritten = combined.items.iter().any(|item| match item {
            Item::Fn { name, body, .. } if name == "main" => matches!(
                body.first(),
                Some(Stmt::ExprStmt(Expr::Call { callee, args }))
                    if callee == "print"
                        && matches!(
                            args.first(),
                            Some(Expr::Call { callee, .. }) if callee == "util__value"
                        )
            ),
            _ => false,
        });
        assert!(main_rewritten);
        let mut imports = graph.modules.last().unwrap().imports.iter();
        assert_eq!(imports.next().unwrap().requested, "util");
        assert_eq!(imports.next().unwrap().requested, "util");
        assert!(imports.next().is_none());
    }

    #[test]
    fn captured_combine_keeps_imported_trait_sidecar() {
        let (_temp, tree) = capture(&[
            ("main.anb", b"import api;\nfn main() { return 0; }"),
            (
                "api.anb",
                b"struct Circle { r: u32 }\ntrait Shape { fn area(self); }\nimpl Shape for Circle { fn area(self) { return 1; } }\npub fn value() { return 1; }",
            ),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        let combined = super::super::combine_captured_project(&graph).unwrap();
        assert!(combined.trait_env.traits.contains_key("Shape"));
        assert!(combined
            .trait_env
            .impls
            .iter()
            .any(|imp| { imp.trait_name == "Shape" && imp.type_name == "Circle" }));
    }

    #[test]
    fn captured_combine_refuses_ambiguous_trait_sidecars() {
        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import api;\ntrait Shape { fn area(self); }\nfn main() { return 0; }",
            ),
            ("api.anb", b"trait Shape { fn area(self); }"),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        let error = super::super::combine_captured_project(&graph).unwrap_err();
        assert!(matches!(
            error,
            CapturedResolveError::TraitNameCollision {
                name,
                first: ModuleKey::Project(_),
                second: ModuleKey::Project(_),
            } if name == "Shape"
        ));
    }

    #[test]
    fn chooses_existing_project_candidates_in_legacy_order_and_preserves_import_occurrences() {
        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import util;\nimport util;\nfn main() { return 0; }",
            ),
            ("util.anb", b"pub fn chosen() { return 1; }"),
            ("util.anub", b"pub fn wrong() { return 2; }"),
            ("util/mod.anb", b"pub fn also_wrong() { return 3; }"),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        let main = graph.modules.last().unwrap();
        assert_eq!(main.imports.len(), 2);
        assert!(main.imports.iter().all(|edge| {
            edge.importer == graph.entry
                && edge.requested == "util"
                && edge.target == ModuleKey::Project(PortablePath::parse("util.anb").unwrap())
                && std::str::from_utf8(main.bytes)
                    .unwrap()
                    .get(edge.span.start..edge.span.end)
                    .unwrap()
                    .starts_with("import util;")
        }));
        assert_eq!(graph.modules.len(), 2);
        assert_eq!(graph.modules[0].namespace, "util");
    }

    #[test]
    fn falls_back_to_mod_and_never_reopens_mutated_source() {
        let (temp, tree) = capture(&[
            ("main.anb", b"import util;\nfn main() { return 0; }"),
            ("util/mod.anb", b"pub fn before() { return 1; }"),
        ]);
        fs::write(
            temp.path().join("util/mod.anb"),
            b"pub fn after() { return 2; }",
        )
        .unwrap();
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        assert_eq!(
            graph.modules[0].key,
            ModuleKey::Project(PortablePath::parse("util/mod.anb").unwrap())
        );
        let mut names = BTreeSet::new();
        collect_fn_names(&graph.modules[0].ast.items, &mut names);
        assert!(names.contains("before"));
        assert!(!names.contains("after"));
    }

    #[test]
    fn later_extensions_and_output_named_directories_are_resolved_from_capture() {
        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import util;\nimport out.tools;\nfn main() { return 0; }",
            ),
            ("util.anub", b"pub fn selected() { return 1; }"),
            ("util.anubis", b"pub fn other() { return 2; }"),
            ("util/mod.anb", b"pub fn lower_priority() { return 3; }"),
            ("out/tools.anb", b"pub fn tool() { return 4; }"),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        let main = graph.modules.last().unwrap();
        assert_eq!(
            main.imports[0].target,
            ModuleKey::Project(PortablePath::parse("util.anub").unwrap())
        );
        assert_eq!(
            main.imports[1].target,
            ModuleKey::Project(PortablePath::parse("out/tools.anb").unwrap())
        );
    }

    #[test]
    fn embedded_stdlib_ignores_project_shadow_file() {
        let (_temp, tree) = capture(&[
            ("main.anb", b"import std.math;\nfn main() { return 0; }"),
            ("std/math.anb", b"this is not Anubis source"),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        assert!(
            matches!(graph.modules[0].key, ModuleKey::EmbeddedStdlib(ref name) if name == "std.math")
        );
        assert_eq!(
            graph.modules[0].bytes,
            stdlib::source("std.math").unwrap().as_bytes()
        );
    }

    #[test]
    fn retains_imported_trait_sidecar() {
        let (_temp, tree) = capture(&[
            ("main.anb", b"import api;\nfn main() { return 0; }"),
            (
                "api.anb",
                b"trait Shape { fn area(self); }\npub fn value() { return 1; }",
            ),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        let api = &graph.modules[0];
        assert!(api.ast.trait_env.traits.contains_key("Shape"));
        assert_eq!(api.namespace, "api");
    }

    #[test]
    fn same_byte_span_in_different_modules_has_distinct_source_identity() {
        let (_temp, tree) = capture(&[
            ("main.anb", b"import a;\nfn main() { return 0; }"),
            ("a.anb", b"import b;\npub fn a() { return 0; }"),
            ("b.anb", b"pub fn b() { return 0; }"),
        ]);
        let graph = load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap();
        let a = &graph.modules[1];
        let main = &graph.modules[2];
        assert_eq!(
            a.imports[0].importer,
            ModuleKey::Project(PortablePath::parse("a.anb").unwrap())
        );
        assert_eq!(main.imports[0].importer, graph.entry);
        assert_eq!(a.imports[0].span.start, main.imports[0].span.start);
        assert_eq!(a.imports[0].requested, "b");
        assert_eq!(main.imports[0].requested, "a");
    }

    #[test]
    fn refuses_nested_imports_with_source_qualified_spans() {
        let sources: &[(&str, &[u8], &str)] = &[
            (
                "main.anb",
                b"module inner { import util; }\nfn main() { return 0; }",
                "main.anb",
            ),
            (
                "api.anb",
                b"module outer { module inner { import util; } }",
                "api.anb",
            ),
        ];
        for &(nested_file, nested_source, expected_importer) in sources {
            let files: Vec<(&str, &[u8])> = if nested_file == "main.anb" {
                vec![
                    ("main.anb", nested_source),
                    ("util.anb", b"pub fn f() { return 0; }"),
                ]
            } else {
                vec![
                    ("main.anb", b"import api;\nfn main() { return 0; }"),
                    (nested_file, nested_source),
                    ("util.anb", b"pub fn f() { return 0; }"),
                ]
            };
            let (_temp, tree) = capture(&files);
            let error =
                load_captured_project(&tree, entry(), ResolveLimits::default()).unwrap_err();
            match error {
                CapturedResolveError::NestedImport {
                    importer,
                    requested,
                    span,
                } => {
                    assert_eq!(
                        importer,
                        ModuleKey::Project(PortablePath::parse(expected_importer).unwrap())
                    );
                    assert_eq!(requested, "util");
                    assert_eq!(
                        std::str::from_utf8(nested_source)
                            .unwrap()
                            .get(span.start..span.end),
                        Some("import util;")
                    );
                }
                other => panic!("expected source-qualified nested import error, got {other:?}"),
            }
        }
    }

    #[test]
    fn refuses_cycle_missing_module_and_invalid_utf8() {
        let (_temp, tree) = capture(&[
            ("main.anb", b"import a;\nfn main() { return 0; }"),
            ("a.anb", b"import b;\nfn a() { return 0; }"),
            ("b.anb", b"import a;\nfn b() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::Cycle { .. })
        ));
        let (_temp, tree) = capture(&[("main.anb", b"import missing;\nfn main() { return 0; }")]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::Unresolved { .. })
        ));
        let (_temp, tree) = capture(&[
            ("main.anb", b"import bad;\nfn main() { return 0; }"),
            ("bad.anb", b"\xff"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::InvalidUtf8 { .. })
        ));
    }

    #[test]
    fn refuses_alias_prefix_namespace_and_flat_function_collisions() {
        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import a.foo;\nimport b.foo;\nfn main() { return 0; }",
            ),
            ("a/foo.anb", b"pub fn a() { return 0; }"),
            ("b/foo.anb", b"pub fn b() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::AliasCollision { .. })
        ));

        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import a.b;\nimport a_b;\nfn main() { return 0; }",
            ),
            ("a/b.anb", b"pub fn a() { return 0; }"),
            ("a_b.anb", b"pub fn b() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::PrefixCollision { .. })
        ));

        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import a;\nimport a.mod;\nfn main() { return 0; }",
            ),
            ("a/mod.anb", b"pub fn a() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::ConflictingNamespace { .. })
        ));

        let (_temp, tree) = capture(&[
            ("main.anb", b"import util;\nfn util__f() { return 0; }"),
            ("util.anb", b"pub fn f() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::FunctionCollision { .. })
        ));

        let (_temp, tree) = capture(&[
            ("main.anb", b"import a;\nimport A;\nfn main() { return 0; }"),
            ("a.anb", b"pub fn small() { return 0; }"),
            ("A.anb", b"pub fn large() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::CaseCollision { .. })
        ));
    }

    #[test]
    fn refuses_local_enum_alias_unknown_std_and_small_limits() {
        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import util;\nenum util { X }\nfn main() { return 0; }",
            ),
            ("util.anb", b"pub fn f() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::EnumAliasCollision { .. })
        ));

        let (_temp, tree) = capture(&[
            ("main.anb", b"import std.unknown;\nfn main() { return 0; }"),
            ("std/unknown.anb", b"pub fn f() { return 0; }"),
        ]);
        assert!(matches!(
            load_captured_project(&tree, entry(), ResolveLimits::default()),
            Err(CapturedResolveError::UnknownStdlib { .. })
        ));

        let (_temp, tree) = capture(&[
            ("main.anb", b"import util;\nfn main() { return 0; }"),
            ("util.anb", b"pub fn f() { return 0; }"),
        ]);
        let limits = ResolveLimits {
            modules: 1,
            ..ResolveLimits::default()
        };
        assert!(matches!(
            load_captured_project(&tree, entry(), limits),
            Err(CapturedResolveError::Limit { kind: "module" })
        ));
        let limits = ResolveLimits {
            depth: 1,
            ..ResolveLimits::default()
        };
        assert!(matches!(
            load_captured_project(&tree, entry(), limits),
            Err(CapturedResolveError::Limit { kind: "depth" })
        ));
        let limits = ResolveLimits {
            parsed_bytes: 1,
            ..ResolveLimits::default()
        };
        assert!(matches!(
            load_captured_project(&tree, entry(), limits),
            Err(CapturedResolveError::Limit {
                kind: "parsed byte"
            })
        ));

        let (_temp, tree) = capture(&[
            (
                "main.anb",
                b"import util;\nimport util;\nfn main() { return 0; }",
            ),
            ("util.anb", b"pub fn f() { return 0; }"),
        ]);
        let limits = ResolveLimits {
            imports: 1,
            ..ResolveLimits::default()
        };
        assert!(matches!(
            load_captured_project(&tree, entry(), limits),
            Err(CapturedResolveError::Limit { kind: "import" })
        ));
    }
}
