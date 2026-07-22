//! Shared analysis for `anubis lsp` — diagnostics + contract hover (no JSON-RPC here).

use crate::doc::expr_to_src;
use crate::frontend::{line_col, parse_source, parse_source_detailed, Item, Mode, AST};
use crate::middle::{typecheck, SemanticDiagnostic, SolverCheck, SymbolicEngine, TypedIR};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LspDiagnostic {
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
    pub severity: u32, // 1=error 2=warning
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HoverInfo {
    pub contents: String,
}

/// Run parse + typecheck + obligations; map to LSP-shaped diagnostics.
pub fn analyze_source(source: &str) -> (Vec<LspDiagnostic>, Option<TypedIR>, Option<AST>) {
    let detailed = parse_source_detailed(source);
    let mut diags = Vec::new();
    for d in &detailed.diagnostics {
        let (line, col) = line_col(source, d.span.start);
        let (el, ec) = line_col(source, d.span.end);
        diags.push(LspDiagnostic {
            line: (line.saturating_sub(1)) as u32,
            character: (col.saturating_sub(1)) as u32,
            end_line: (el.saturating_sub(1)) as u32,
            end_character: (ec.saturating_sub(1)) as u32,
            severity: 1,
            code: Some("ANUBIS_PARSE".into()),
            message: d.message.clone(),
        });
    }
    if !detailed.diagnostics.is_empty() {
        return (diags, None, None);
    }
    let ast = detailed.ast;
    match typecheck(ast.clone(), Mode::Safe) {
        Ok(ir) => {
            for d in &ir.diagnostics {
                diags.push(semantic_to_lsp(source, d));
            }
            let checks = SymbolicEngine::check_obligations(&ir);
            for c in checks {
                if c.status == "FAIL" {
                    diags.push(obligation_to_lsp(&c));
                }
            }
            (diags, Some(ir), Some(ast))
        }
        Err(e) => {
            diags.push(LspDiagnostic {
                line: 0,
                character: 0,
                end_line: 0,
                end_character: 1,
                severity: 1,
                code: Some("ANUBIS_TYPECHECK".into()),
                message: e,
            });
            (diags, None, Some(ast))
        }
    }
}

fn semantic_to_lsp(source: &str, d: &SemanticDiagnostic) -> LspDiagnostic {
    let (line, col, el, ec) = if let Some((s, e)) = d.span {
        let (l, c) = line_col(source, s);
        let (el, ec) = line_col(source, e);
        (l, c, el, ec)
    } else {
        (1, 1, 1, 2)
    };
    LspDiagnostic {
        line: (line.saturating_sub(1)) as u32,
        character: (col.saturating_sub(1)) as u32,
        end_line: (el.saturating_sub(1)) as u32,
        end_character: (ec.saturating_sub(1)) as u32,
        severity: 1,
        code: d.code.clone(),
        message: d.message.clone(),
    }
}

fn obligation_to_lsp(c: &SolverCheck) -> LspDiagnostic {
    LspDiagnostic {
        line: 0,
        character: 0,
        end_line: 0,
        end_character: 1,
        severity: 1,
        code: Some("ANUBIS_OBLIGATION".into()),
        message: format!("{}: {} {}", c.name, c.status, c.detail),
    }
}

/// Hover text for the function whose name span covers `byte_offset`, or None.
pub fn hover_at(source: &str, byte_offset: usize) -> Option<HoverInfo> {
    let ast = parse_source(source).ok()?;
    let name = word_at(source, byte_offset)?;
    find_fn_hover(&ast.items, &name)
}

fn find_fn_hover(items: &[Item], name: &str) -> Option<HoverInfo> {
    for it in items {
        match it {
            Item::Fn {
                name: n,
                params,
                ret,
                requires,
                ensures,
                effects,
                ..
            } if n == name => {
                let mut s = format!("```anubis\nfn {n}(");
                let ps: Vec<_> = params
                    .iter()
                    .map(|(a, t)| {
                        if t.is_empty() {
                            a.clone()
                        } else {
                            format!("{a}: {t}")
                        }
                    })
                    .collect();
                s.push_str(&ps.join(", "));
                s.push(')');
                if let Some(r) = ret {
                    s.push_str(" -> ");
                    s.push_str(r);
                }
                if !effects.is_empty() {
                    s.push_str(&format!(" uses({})", effects.join(", ")));
                }
                s.push_str("\n```\n");
                if !requires.is_empty() || !ensures.is_empty() {
                    s.push_str("\n**Contracts**\n");
                    for r in requires {
                        s.push_str(&format!("- requires: `{}`\n", expr_to_src(r)));
                    }
                    for e in ensures {
                        s.push_str(&format!("- ensures: `{}`\n", expr_to_src(e)));
                    }
                }
                // Taint & confidentiality: surface the information-flow qualifiers explicitly (the
                // ROADMAP promises "hover shows contracts+effects+taint"). A `tainted<T>` param is
                // untrusted input that must be `declassify`'d before a sink; a `secret<T>` param is
                // confidential and must not reach net/shell egress without declassify. Detected on the
                // raw annotation (`tainted<`/`secret<` prefix) — the same qualifiers the Safe-mode
                // lethal-trifecta enforcement keys on.
                let taint_of = |t: &str| -> Option<&'static str> {
                    let tl = t.trim();
                    if tl.starts_with("tainted<") {
                        Some("tainted — untrusted input; declassify before any sink")
                    } else if tl.starts_with("secret<") {
                        Some("secret — confidential; must not reach net/shell egress without declassify")
                    } else {
                        None
                    }
                };
                let mut taint_lines: Vec<String> = Vec::new();
                for (a, t) in params {
                    if let Some(note) = taint_of(t) {
                        taint_lines.push(format!("- `{a}`: **{note}**"));
                    }
                }
                if let Some(r) = ret {
                    if let Some(note) = taint_of(r) {
                        taint_lines.push(format!("- return: **{note}**"));
                    }
                }
                if !taint_lines.is_empty() {
                    s.push_str("\n**Taint & confidentiality**\n");
                    for l in taint_lines {
                        s.push_str(&l);
                        s.push('\n');
                    }
                }
                return Some(HoverInfo { contents: s });
            }
            Item::Module { items: inner, .. } | Item::Impl { methods: inner, .. } => {
                if let Some(h) = find_fn_hover(inner, name) {
                    return Some(h);
                }
            }
            _ => {}
        }
    }
    None
}

fn word_at(source: &str, offset: usize) -> Option<String> {
    let b = source.as_bytes();
    if offset >= b.len() {
        return None;
    }
    let mut s = offset;
    let mut e = offset;
    while s > 0 && ((b[s - 1] as char).is_ascii_alphanumeric() || b[s - 1] == b'_') {
        s -= 1;
    }
    while e < b.len() && ((b[e] as char).is_ascii_alphanumeric() || b[e] == b'_') {
        e += 1;
    }
    if s >= e {
        None
    } else {
        Some(source[s..e].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_on_type_error() {
        let (d, _, _) = analyze_source("fn main() { let x: u32 = true; }\n");
        assert!(!d.is_empty(), "expected diagnostics");
    }

    #[test]
    fn hover_shows_contracts() {
        let src = r#"
fn div(a: u32, b: u32) -> u32 requires(b != 0) ensures(result == a / b) {
  return a / b;
}
fn main() { print(div(4, 2)); }
"#;
        // offset of "div" in definition
        let off = src.find("fn div").unwrap() + 3;
        let h = hover_at(src, off).expect("hover");
        assert!(h.contents.contains("Contracts") || h.contents.contains("requires"));
    }

    #[test]
    fn hover_shows_taint_and_secret() {
        // The ROADMAP promises "hover shows contracts+effects+taint" — surface the information-flow
        // qualifiers explicitly, not just buried in the type string.
        let src = r#"
fn handle(q: tainted<string>, key: secret<u64>) -> tainted<string> { return q; }
fn main() {}
"#;
        let off = src.find("fn handle").unwrap() + 3;
        let h = hover_at(src, off).expect("hover");
        assert!(
            h.contents.contains("Taint & confidentiality"),
            "hover should have a Taint section, got:\n{}",
            h.contents
        );
        assert!(
            h.contents.contains("`q`") && h.contents.contains("tainted"),
            "tainted param shown"
        );
        assert!(
            h.contents.contains("`key`") && h.contents.contains("secret"),
            "secret param shown"
        );
        assert!(
            h.contents.contains("return") && h.contents.contains("tainted"),
            "tainted return shown"
        );
        // A function with NO taint qualifiers must NOT get a spurious Taint section.
        let clean = "fn f(a: u32) -> u32 { return a; }\nfn main() {}\n";
        let hc = hover_at(clean, clean.find("fn f").unwrap() + 3).expect("hover");
        assert!(
            !hc.contents.contains("Taint & confidentiality"),
            "no taint section for a clean fn"
        );
    }
}
