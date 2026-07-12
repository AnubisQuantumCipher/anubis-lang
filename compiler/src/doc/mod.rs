//! `anubis doc` — verification-first documentation renderer.
//!
//! Surfaces signatures, `uses(...)`, and a **Contracts** section from `requires`/`ensures`.

use crate::frontend::{associate_docs, parse_source, Expr, Item, Mode, Visibility, AST};
use crate::middle::{typecheck, TypedIR};
use crate::resolve::combine_from_entry_opts;
use crate::package::ResolveOptions;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DocOptions {
    pub include_private: bool,
    pub format: DocFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    Markdown,
    Json,
}

impl Default for DocOptions {
    fn default() -> Self {
        Self {
            include_private: false,
            format: DocFormat::Markdown,
        }
    }
}

/// Render documentation for an entry file (multi-file + package deps supported).
pub fn render_path(path: &Path, opts: &DocOptions) -> Result<String, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let items = if path.is_file() {
        combine_from_entry_opts(path, &ResolveOptions::default()).unwrap_or_else(|_| {
            parse_source(&source)
                .map(|a| a.items)
                .unwrap_or_default()
        })
    } else {
        parse_source(&source)?.items
    };
    // Re-parse entry source so doc-comment spans match associate_docs keys.
    // Typecheck the full multi-file/package graph; render the entry file's items
    // (contracts still come from AST requires/ensures on those items).
    let entry_ast = parse_source(&source)?;
    let docs = associate_docs(&source, &entry_ast.items);
    let _typed: TypedIR = typecheck(AST { items }, Mode::Safe)?;
    render_items(&entry_ast.items, &docs, opts)
}

/// Render from in-memory source (tests).
pub fn render_source(source: &str, opts: &DocOptions) -> Result<String, String> {
    let ast = parse_source(source)?;
    let docs = associate_docs(source, &ast.items);
    let _typed = typecheck(ast.clone(), Mode::Safe)?;
    render_items(&ast.items, &docs, opts)
}

fn render_items(
    items: &[Item],
    docs: &std::collections::BTreeMap<usize, String>,
    opts: &DocOptions,
) -> Result<String, String> {
    let mut pages = Vec::new();
    collect_fn_pages(items, docs, opts, &mut pages);
    match opts.format {
        DocFormat::Markdown => Ok(render_markdown(&pages)),
        DocFormat::Json => serde_json::to_string_pretty(&pages).map_err(|e| e.to_string()),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FnDocPage {
    pub name: String,
    pub visibility: String,
    pub params: Vec<String>,
    pub ret: Option<String>,
    pub effects: Vec<String>,
    pub requires: Vec<String>,
    pub ensures: Vec<String>,
    pub doc: String,
    pub mode: String,
}

fn collect_fn_pages(
    items: &[Item],
    docs: &std::collections::BTreeMap<usize, String>,
    opts: &DocOptions,
    out: &mut Vec<FnDocPage>,
) {
    for it in items {
        match it {
            Item::Fn {
                name,
                visibility,
                params,
                ret,
                requires,
                ensures,
                effects,
                mode,
                span,
                ..
            } => {
                if !opts.include_private && matches!(visibility, Visibility::Private) {
                    // still document main
                    if name != "main" {
                        continue;
                    }
                }
                out.push(FnDocPage {
                    name: name.clone(),
                    visibility: match visibility {
                        Visibility::Public => "pub".into(),
                        Visibility::Private => "private".into(),
                    },
                    params: params
                        .iter()
                        .map(|(n, t)| {
                            if t.is_empty() {
                                n.clone()
                            } else {
                                format!("{n}: {t}")
                            }
                        })
                        .collect(),
                    ret: ret.clone(),
                    effects: effects.clone(),
                    requires: requires.iter().map(expr_to_src).collect(),
                    ensures: ensures.iter().map(expr_to_src).collect(),
                    doc: docs.get(&span.start).cloned().unwrap_or_default(),
                    mode: format!("{mode:?}").to_ascii_lowercase(),
                });
            }
            Item::Module { items: inner, .. } | Item::Impl { methods: inner, .. } => {
                collect_fn_pages(inner, docs, opts, out);
            }
            Item::Trait { methods: inner, .. } => collect_fn_pages(inner, docs, opts, out),
            _ => {}
        }
    }
}

fn render_markdown(pages: &[FnDocPage]) -> String {
    let mut s = String::from("# Anubis API documentation\n\n");
    s.push_str("_Verification-first: Contracts are source `requires`/`ensures`, not prose claims._\n\n");
    for p in pages {
        s.push_str(&format!("## `{}`\n\n", p.name));
        if !p.doc.is_empty() {
            s.push_str(&p.doc);
            s.push_str("\n\n");
        }
        s.push_str("### Signature\n\n```anubis\n");
        if p.visibility == "pub" {
            s.push_str("pub ");
        }
        s.push_str("fn ");
        s.push_str(&p.name);
        s.push('(');
        s.push_str(&p.params.join(", "));
        s.push(')');
        if let Some(r) = &p.ret {
            s.push_str(" -> ");
            s.push_str(r);
        }
        if !p.effects.is_empty() {
            s.push_str(" uses(");
            s.push_str(&p.effects.join(", "));
            s.push(')');
        }
        s.push_str("\n```\n\n");
        if !p.requires.is_empty() || !p.ensures.is_empty() {
            s.push_str("### Contracts\n\n");
            for r in &p.requires {
                s.push_str(&format!("- **requires:** `{r}`\n"));
            }
            for e in &p.ensures {
                s.push_str(&format!("- **ensures:** `{e}`\n"));
            }
            s.push('\n');
        }
        if !p.effects.is_empty() {
            s.push_str(&format!(
                "### Effects\n\n`uses({})`\n\n",
                p.effects.join(", ")
            ));
        }
    }
    s
}

/// Minimal expression pretty-printer for contract display.
pub fn expr_to_src(e: &Expr) -> String {
    match e {
        Expr::Var(s) => s.clone(),
        Expr::Literal(s) => s.clone(),
        Expr::StrLiteral(s) => format!("\"{s}\""),
        Expr::Call { callee, args } => {
            let a: Vec<_> = args.iter().map(expr_to_src).collect();
            format!("{callee}({})", a.join(", "))
        }
        Expr::Binary { op, lhs, rhs } => {
            format!("({} {} {})", expr_to_src(lhs), op, expr_to_src(rhs))
        }
        Expr::Unary { op, expr } => format!("({op}{})", expr_to_src(expr)),
        Expr::FieldAccess { base, field, .. } => format!("{}.{}", expr_to_src(base), field),
        Expr::Index { base, index } => {
            format!("{}[{}]", expr_to_src(base), expr_to_src(index))
        }
        _ => "<expr>".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contracts_section_from_requires_ensures() {
        let src = r#"
// Integer division
pub fn div(a: u32, b: u32) -> u32 requires(b != 0) ensures(result == a / b) {
    return a / b;
}
fn main() { print(div(10, 2)); }
"#;
        let md = render_source(src, &DocOptions::default()).expect("doc");
        assert!(md.contains("### Contracts"), "{md}");
        assert!(md.contains("requires"), "{md}");
        assert!(md.contains("ensures"), "{md}");
        assert!(md.contains("div"), "{md}");
        // Leading // comments must attach as function prose (lexer preservation).
        assert!(
            md.contains("Integer division"),
            "expected doc comment body in markdown:\n{md}"
        );
    }

    #[test]
    fn associate_docs_from_lexer_comments() {
        let src = "// hello docs\nfn main() { print(1); }\n";
        let ast = parse_source(src).unwrap();
        let docs = associate_docs(src, &ast.items);
        assert!(!docs.is_empty(), "expected attached docs");
        let any = docs.values().any(|v| v.contains("hello docs"));
        assert!(any, "expected 'hello docs' in {docs:?}");
    }
}
