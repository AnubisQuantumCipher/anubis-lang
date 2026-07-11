//! `anubis fmt` — a canonical, self-verifying source formatter.
//!
//! The formatter pretty-prints the parsed AST back to Anubis source. Its safety guarantee is
//! **fail-closed, verification-first**: [`format_source`] reparses its own output and refuses to
//! emit anything whose structure (the AST, compared span-insensitively) differs from the input. So
//! `anubis fmt` can never silently mangle a program — on any construct the printer gets wrong it
//! declines the file (`ANUBIS_FMT_ROUNDTRIP`) rather than corrupting it.
//!
//! Two known, explicit limitations, both surfaced honestly rather than mishandled:
//! - Files that declare a `trait` are skipped (`ANUBIS_FMT_TRAIT_UNSUPPORTED`): the parser desugars
//!   traits into their implementing `impl`s before the formatter can see them, so re-emitting would
//!   drop the `trait` declaration.
//! - String interpolation (`"a${x}"`) is parsed into a `+`-concatenation, so the formatter prints
//!   the concatenation form. The result is behavior-identical (and round-trip-verified), just not
//!   the original `${...}` surface.

use crate::frontend::{parse_source, Expr, ForSource, Item, MatchArm, Pattern, Stmt, AST};

const INDENT: &str = "    ";

/// Format Anubis source, fail-closed. Returns the formatted text, or an `ANUBIS_FMT_*` error when
/// the file cannot be safely formatted (declares a trait, fails to parse, or would round-trip to a
/// different AST).
pub fn format_source(src: &str) -> Result<String, String> {
    if declares_trait(src) {
        return Err(
            "ANUBIS_FMT_TRAIT_UNSUPPORTED: file declares a `trait` (desugared during parsing; \
             reformatting would drop it) — skipped"
                .to_string(),
        );
    }
    let ast = parse_source(src).map_err(|e| format!("ANUBIS_FMT_PARSE: {e}"))?;
    let output = format_ast(&ast.items);
    // Self-verification: the emitted source must parse back to the same AST (ignoring spans).
    let reparsed = parse_source(&output).map_err(|e| format!("ANUBIS_FMT_REPARSE: {e}"))?;
    if strip_span_debug(&ast) != strip_span_debug(&reparsed) {
        return Err(
            "ANUBIS_FMT_ROUNDTRIP: formatting would change the program structure — refusing to \
             emit (fail-closed)"
                .to_string(),
        );
    }
    Ok(output)
}

/// True if `formatted == format_source(src)` — i.e. the source is already canonically formatted.
/// Errors propagate (an unformattable file is neither formatted nor "already formatted").
pub fn is_formatted(src: &str) -> Result<bool, String> {
    Ok(format_source(src)? == src)
}

/// A source-level check for a `trait` declaration (the AST no longer carries one — see module docs).
fn declares_trait(src: &str) -> bool {
    src.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with("trait ") || t == "trait"
    })
}

/// Compare ASTs ignoring spans: the `Debug` string with every `Span { start: .., end: .. }` masked.
fn strip_span_debug(ast: &AST) -> String {
    let s = format!("{ast:?}");
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(idx) = rest.find("Span { start:") {
        out.push_str(&rest[..idx]);
        out.push_str("Span");
        rest = &rest[idx..];
        match rest.find('}') {
            Some(close) => rest = &rest[close + 1..],
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// Pretty-print a whole program.
pub fn format_ast(items: &[Item]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&fmt_item(item, 0));
        out.push('\n');
    }
    out
}

fn pad(indent: usize) -> String {
    INDENT.repeat(indent)
}

fn fmt_item(item: &Item, indent: usize) -> String {
    let p = pad(indent);
    match item {
        Item::Import { path, .. } => format!("{p}import {path};"),
        Item::Module { name, items, .. } => {
            let mut s = format!("{p}module {name} {{\n");
            for it in items {
                s.push_str(&fmt_item(it, indent + 1));
                s.push('\n');
            }
            s.push_str(&format!("{p}}}"));
            s
        }
        Item::Fn {
            name,
            visibility,
            params,
            body,
            ret,
            requires,
            ensures,
            attributes,
            ..
        } => {
            let mut s = String::new();
            for attr in attributes {
                s.push_str(&format!("{p}@{}\n", attr.name));
            }
            let vis = if matches!(visibility, crate::frontend::Visibility::Public) {
                "pub "
            } else {
                ""
            };
            let params_s = params
                .iter()
                .map(|(n, ty)| {
                    if ty.is_empty() {
                        n.clone()
                    } else {
                        format!("{n}: {ty}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            s.push_str(&format!("{p}{vis}fn {name}({params_s})"));
            if let Some(rty) = ret {
                s.push_str(&format!(" -> {rty}"));
            }
            for r in requires {
                s.push_str(&format!(" requires({})", fmt_expr(r)));
            }
            for e in ensures {
                s.push_str(&format!(" ensures({})", fmt_expr(e)));
            }
            s.push_str(" {\n");
            s.push_str(&fmt_block(body, indent + 1));
            s.push_str(&format!("{p}}}"));
            s
        }
        Item::Struct { name, fields, .. } => {
            let mut s = format!("{p}struct {name} {{\n");
            for (fname, fty) in fields {
                s.push_str(&format!("{}{fname}: {fty},\n", pad(indent + 1)));
            }
            s.push_str(&format!("{p}}}"));
            s
        }
        Item::Enum { name, variants, .. } => {
            let mut s = format!("{p}enum {name} {{\n");
            for v in variants {
                s.push_str(&format!("{}{},\n", pad(indent + 1), fmt_variant(v)));
            }
            s.push_str(&format!("{p}}}"));
            s
        }
        Item::Impl {
            type_name,
            trait_name,
            methods,
            ..
        } => {
            let head = match trait_name {
                Some(tr) => format!("{p}impl {tr} for {type_name} {{\n"),
                None => format!("{p}impl {type_name} {{\n"),
            };
            let mut s = head;
            for (i, m) in methods.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                s.push_str(&fmt_item(m, indent + 1));
                s.push('\n');
            }
            s.push_str(&format!("{p}}}"));
            s
        }
        Item::Trait { name, .. } => format!("{p}trait {name} {{ /* unsupported by fmt */ }}"),
    }
}

fn fmt_variant(v: &crate::frontend::EnumVariant) -> String {
    use crate::frontend::EnumVariantKind;
    match &v.kind {
        EnumVariantKind::Unit => v.name.clone(),
        EnumVariantKind::Tuple(tys) => format!("{}({})", v.name, tys.join(", ")),
        EnumVariantKind::Struct(fields) => {
            let inner = fields
                .iter()
                .map(|(n, t)| format!("{n}: {t}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{ {inner} }}", v.name)
        }
    }
}

fn fmt_block(stmts: &[Stmt], indent: usize) -> String {
    let mut s = String::new();
    for st in stmts {
        s.push_str(&fmt_stmt(st, indent));
    }
    s
}

fn fmt_stmt(stmt: &Stmt, indent: usize) -> String {
    let p = pad(indent);
    match stmt {
        Stmt::Let { name, ty, init, .. } => {
            let ann = ty.as_ref().map(|t| format!(": {t}")).unwrap_or_default();
            format!("{p}let {name}{ann} = {};\n", fmt_expr(init))
        }
        Stmt::LetPattern { pattern, init, .. } => {
            format!("{p}let {} = {};\n", fmt_pattern(pattern), fmt_expr(init))
        }
        Stmt::Assign { target, value } => {
            format!("{p}{} = {};\n", fmt_expr(target), fmt_expr(value))
        }
        Stmt::If { cond, then, else_ } => {
            let mut s = format!(
                "{p}if {} {{\n{}",
                fmt_expr(cond),
                fmt_block(then, indent + 1)
            );
            match else_ {
                Some(e) => {
                    s.push_str(&format!(
                        "{p}}} else {{\n{}{p}}}\n",
                        fmt_block(e, indent + 1)
                    ));
                }
                None => s.push_str(&format!("{p}}}\n")),
            }
            s
        }
        Stmt::While {
            cond,
            body,
            invariant,
        } => {
            let inv = invariant
                .iter()
                .map(|i| format!(" invariant({})", fmt_expr(i)))
                .collect::<String>();
            format!(
                "{p}while {}{inv} {{\n{}{p}}}\n",
                fmt_expr(cond),
                fmt_block(body, indent + 1)
            )
        }
        Stmt::Loop { body, invariant } => {
            let inv = invariant
                .iter()
                .map(|i| format!(" invariant({})", fmt_expr(i)))
                .collect::<String>();
            format!("{p}loop{inv} {{\n{}{p}}}\n", fmt_block(body, indent + 1))
        }
        Stmt::For {
            var,
            source,
            body,
            invariant,
        } => {
            let src = match source {
                ForSource::Range { start, end } => {
                    format!("{}..{}", fmt_expr(start), fmt_expr(end))
                }
                ForSource::Collection { expr } => fmt_expr(expr),
            };
            let inv = invariant
                .iter()
                .map(|i| format!(" invariant({})", fmt_expr(i)))
                .collect::<String>();
            format!(
                "{p}for {var} in {src}{inv} {{\n{}{p}}}\n",
                fmt_block(body, indent + 1)
            )
        }
        Stmt::WhileLet {
            pattern,
            expr,
            body,
        } => format!(
            "{p}while let {} = {} {{\n{}{p}}}\n",
            fmt_pattern(pattern),
            fmt_expr(expr),
            fmt_block(body, indent + 1)
        ),
        Stmt::Break => format!("{p}break;\n"),
        Stmt::Continue => format!("{p}continue;\n"),
        // Control-flow expressions used as statements take no trailing `;` (an if-let/match
        // statement with a `;` is a parse error); ordinary expression statements do.
        Stmt::ExprStmt(e) => match e {
            Expr::If { .. } | Expr::IfLet { .. } | Expr::Match { .. } | Expr::Block { .. } => {
                format!("{p}{}\n", fmt_expr(e))
            }
            _ => format!("{p}{};\n", fmt_expr(e)),
        },
        Stmt::ResearchBlock { body, .. } => {
            format!("{p}research {{\n{}{p}}}\n", fmt_block(body, indent + 1))
        }
        Stmt::ExploitBlock { body, .. } => {
            format!("{p}exploit {{\n{}{p}}}\n", fmt_block(body, indent + 1))
        }
        Stmt::HybridBlock { .. } | Stmt::SpecBlock { .. } => {
            // Rare research constructs — printed as a marker; self-verification will refuse the file
            // if this does not round-trip, so it is never emitted incorrectly.
            format!("{p}/* fmt: unsupported block */\n")
        }
    }
}

/// Binary-operator precedence, mirroring the parser's ladder (higher binds tighter).
fn bin_prec(op: &str) -> u8 {
    match op {
        "||" => 4,
        "&&" => 5,
        "|" => 6,
        "^" => 7,
        "&" => 8,
        "==" | "!=" | "<" | "<=" | ">" | ">=" => 10,
        "<<" | ">>" => 18,
        "+" | "-" => 20,
        "*" | "/" | "%" => 30,
        _ => 0,
    }
}

fn fmt_expr(e: &Expr) -> String {
    fmt_expr_prec(e, 0)
}

/// Format an expression, wrapping in parens when its binary precedence is lower than `min` (the
/// precedence required by the surrounding context) — so the reparsed tree is identical.
fn fmt_expr_prec(e: &Expr, min: u8) -> String {
    match e {
        Expr::Binary { op, lhs, rhs } => {
            let p = bin_prec(op);
            // Left-associative: left child allows equal precedence, right child requires higher.
            let inner = format!(
                "{} {op} {}",
                fmt_expr_prec(lhs, p),
                fmt_expr_prec(rhs, p + 1)
            );
            if p < min {
                format!("({inner})")
            } else {
                inner
            }
        }
        _ => fmt_atom(e),
    }
}

/// Format a non-binary (atomic or postfix/prefix) expression.
fn fmt_atom(e: &Expr) -> String {
    match e {
        Expr::Var(n) => n.clone(),
        Expr::Literal(s) => s.clone(),
        Expr::StrLiteral(s) => format!("\"{}\"", escape_str(s)),
        // `return`/`throw` parse as a keyword-prefixed expression, not a call — print `return X`.
        Expr::Call { callee, args } if callee == "return" => match args.first() {
            Some(a) => format!("return {}", fmt_expr(a)),
            None => "return".to_string(),
        },
        Expr::Call { callee, args } => format!("{callee}({})", fmt_args(args)),
        Expr::CallExpr { callee, args } => {
            format!("{}({})", fmt_callee(callee), fmt_args(args))
        }
        Expr::Unary { op, expr } => format!("{op}{}", fmt_unary_operand(expr)),
        Expr::ArrayLiteral { elements } => format!("[{}]", fmt_args(elements)),
        Expr::Index { base, index } => {
            format!("{}[{}]", fmt_callee(base), fmt_expr(index))
        }
        Expr::Cast { expr, ty } => format!("{} as {ty}", fmt_expr_prec(expr, 100)),
        Expr::FieldAccess { base, field, .. } => format!("{}.{field}", fmt_callee(base)),
        Expr::StructLiteral { name, fields, .. } => {
            let inner = fields
                .iter()
                .map(|(n, v)| format!("{n}: {}", fmt_expr(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{ {inner} }}")
        }
        Expr::EnumConstruct {
            enum_name,
            variant,
            fields,
            field_names,
            ..
        } => {
            if field_names.is_empty() {
                if fields.is_empty() {
                    format!("{enum_name}::{variant}")
                } else {
                    format!("{enum_name}::{variant}({})", fmt_args(fields))
                }
            } else {
                let inner = field_names
                    .iter()
                    .zip(fields)
                    .map(|(n, v)| format!("{n}: {}", fmt_expr(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{enum_name}::{variant} {{ {inner} }}")
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => fmt_match(scrutinee, arms),
        Expr::If {
            cond, then, else_, ..
        } => {
            // An `else` branch that is itself an `if` is an else-if chain — print `else if ...`
            // without wrapping braces, which would otherwise turn `else_: If` into `else_: Block(If)`.
            let else_part = if matches!(else_.as_ref(), Expr::If { .. }) {
                format!("else {}", fmt_expr(else_))
            } else {
                format!("else {}", fmt_branch(else_))
            };
            format!("if {} {} {}", fmt_expr(cond), fmt_branch(then), else_part)
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => format!(
            "if let {} = {} {} else {}",
            fmt_pattern(pattern),
            fmt_expr(scrutinee),
            fmt_branch(then),
            fmt_branch(else_)
        ),
        Expr::Block { stmts, tail } => {
            let mut inner = stmts
                .iter()
                .map(|s| fmt_stmt(s, 0).trim().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(t) = tail {
                if !inner.is_empty() {
                    inner.push(' ');
                }
                inner.push_str(&fmt_expr(t));
            }
            format!("{{ {inner} }}")
        }
        Expr::MapLiteral { entries, .. } => {
            if entries.is_empty() {
                "{}".to_string()
            } else {
                let inner = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", fmt_expr(k), fmt_expr(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {inner} }}")
            }
        }
        Expr::Lambda { params, body } => {
            format!("|{}| {}", params.join(", "), fmt_expr(body))
        }
        Expr::Try(inner) => format!("{}?", fmt_callee(inner)),
        Expr::Assume(inner) => format!("assume({})", fmt_expr(inner)),
        Expr::Assert(inner) => format!("assert({})", fmt_expr(inner)),
        Expr::Tainted { ty, inner } => format!("tainted<{ty}>({})", fmt_expr(inner)),
        Expr::Symbolic { ty } => format!("symbolic({ty})"),
        Expr::TaintSource { label } => format!("taint_source({label})"),
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => {
            let mut s = format!("declassify({}", fmt_expr(inner));
            if let Some(p) = policy {
                s.push_str(&format!(", policy: \"{}\"", escape_str(p)));
            }
            if let Some(r) = reason {
                s.push_str(&format!(", reason: \"{}\"", escape_str(r)));
            }
            s.push(')');
            s
        }
        // Binary handled by fmt_expr_prec; here only as a fallback (fully parenthesized).
        Expr::Binary { .. } => format!("({})", fmt_expr_prec(e, 0)),
        // Rare/opaque constructs — printed best-effort; self-verification refuses the file if wrong.
        Expr::UnifiedBuffer { ty } => format!("unified Buffer<{ty}>"),
        Expr::RawPtr { mutable } => {
            if *mutable {
                "*mut unknown".to_string()
            } else {
                "*const unknown".to_string()
            }
        }
        Expr::Other(s) => s.clone(),
    }
}

/// A callee/base position needs parentheses around anything that isn't a simple postfix chain.
fn fmt_callee(e: &Expr) -> String {
    match e {
        Expr::Var(_)
        | Expr::Call { .. }
        | Expr::CallExpr { .. }
        | Expr::Index { .. }
        | Expr::FieldAccess { .. }
        | Expr::Literal(_)
        | Expr::StrLiteral(_) => fmt_atom(e),
        _ => format!("({})", fmt_expr(e)),
    }
}

/// Format an `if`/`if let` branch. The branch is normally already an `Expr::Block` (which prints
/// its own `{ }`); a non-block branch is wrapped in braces so the surrounding `if` still parses.
fn fmt_branch(e: &Expr) -> String {
    match e {
        Expr::Block { .. } => fmt_atom(e),
        _ => format!("{{ {} }}", fmt_expr(e)),
    }
}

/// A unary operand binds tightly; parenthesize a binary/complex operand.
fn fmt_unary_operand(e: &Expr) -> String {
    match e {
        Expr::Binary { .. } => format!("({})", fmt_expr(e)),
        _ => fmt_atom(e),
    }
}

fn fmt_args(args: &[Expr]) -> String {
    args.iter().map(fmt_expr).collect::<Vec<_>>().join(", ")
}

fn fmt_match(scrutinee: &Expr, arms: &[MatchArm]) -> String {
    let arms_s = arms
        .iter()
        .map(|a| {
            let guard = a
                .guard
                .as_ref()
                .map(|g| format!(" if {}", fmt_expr(g)))
                .unwrap_or_default();
            format!(
                "{}{guard} => {}",
                fmt_pattern(&a.pattern),
                fmt_expr(&a.body)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("match {} {{ {arms_s} }}", fmt_expr(scrutinee))
}

fn fmt_pattern(p: &Pattern) -> String {
    match p {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Binding(n) => n.clone(),
        Pattern::Literal(s) => s.clone(),
        Pattern::StrLiteral(s) => format!("\"{}\"", escape_str(s)),
        Pattern::Or(pats) => pats.iter().map(fmt_pattern).collect::<Vec<_>>().join(" | "),
        Pattern::List(pats) => {
            format!(
                "[{}]",
                pats.iter().map(fmt_pattern).collect::<Vec<_>>().join(", ")
            )
        }
        Pattern::Struct { name, fields } => {
            let inner = fields
                .iter()
                .map(|(n, pat)| format!("{n}: {}", fmt_pattern(pat)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{ {inner} }}")
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            named_bindings,
        } => {
            let en = if enum_name.is_empty() {
                String::new()
            } else {
                format!("{enum_name}::")
            };
            if !named_bindings.is_empty() {
                let inner = named_bindings
                    .iter()
                    .map(|(n, p)| format!("{n}: {}", fmt_pattern(p)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{en}{variant} {{ {inner} }}")
            } else if bindings.is_empty() {
                format!("{en}{variant}")
            } else {
                let inner = bindings
                    .iter()
                    .map(fmt_pattern)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{en}{variant}({inner})")
            }
        }
    }
}

/// Re-escape a decoded string value into a valid double-quoted literal body.
fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn corpus_files() -> Vec<PathBuf> {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = vec![];
        for dir in ["examples/tour", "examples/programs"] {
            if let Ok(rd) = std::fs::read_dir(base.join(dir)) {
                for e in rd.flatten() {
                    let p = e.path();
                    let is_anb = p
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|x| matches!(x, "anb" | "anub" | "anubis"))
                        .unwrap_or(false);
                    if is_anb {
                        files.push(p);
                    }
                }
            }
        }
        files.sort();
        files
    }

    #[test]
    fn fmt_is_safe_and_idempotent_over_the_corpus() {
        let mut formatted = 0usize;
        let mut skipped = 0usize;
        let mut refused: Vec<(PathBuf, String)> = vec![];
        for f in corpus_files() {
            let src = std::fs::read_to_string(&f).unwrap();
            match format_source(&src) {
                Ok(out) => {
                    // Idempotence: re-formatting the output yields byte-identical text.
                    let out2 = format_source(&out).expect("re-format formatted source");
                    assert_eq!(out, out2, "fmt not idempotent on {}", f.display());
                    formatted += 1;
                }
                Err(e) if e.starts_with("ANUBIS_FMT_TRAIT") => skipped += 1,
                Err(e) => refused.push((f, e)),
            }
        }
        eprintln!(
            "fmt corpus: {formatted} formatted+idempotent, {skipped} trait-skipped, {} refused",
            refused.len()
        );
        for (f, e) in &refused {
            eprintln!("  REFUSED {} — {}", f.display(), e);
        }
        // SAFETY (unconditional): every file is either formatted+idempotent, honestly trait-skipped,
        // or refused with a proper ANUBIS_FMT_* diagnostic — never a panic, never mangled output.
        for (f, e) in &refused {
            assert!(
                e.starts_with("ANUBIS_FMT_"),
                "refusal must be a fail-closed ANUBIS_FMT_* error, got `{e}` on {}",
                f.display()
            );
        }
        // COVERAGE: a strong majority formats. Floor set well below the current 31 so it never flakes.
        let total = formatted + skipped + refused.len();
        assert!(total > 20, "corpus not found? total={total}");
        assert!(
            formatted >= 28,
            "fmt coverage regressed: {formatted}/{total} formatted"
        );
    }

    #[test]
    fn fmt_round_trips_a_representative_program() {
        // A hand-written program exercising the constructs fmt must get exactly right: precedence
        // (parens), else-if chains, match, closures, structs/enums, `?`/return, string escapes.
        let src = r#"
struct P { x: int, y: int }
enum E { A, B(int) }
pub fn calc(a, b) -> int requires(b != 0) {
    let r = if a % 15 == 0 { 1 } else if a > b { a - b * 2 } else { 0 };
    let f = |z| z * z + 1;
    let p = P { x: a + 1, y: f(b) };
    match E::B(r) {
        E::A => 0,
        E::B(n) => n + p.x,
    }
}
fn main() {
    let s = "line\n\tand \"quote\" ${calc(30, 5)}";
    print(s);
}
"#;
        let out = format_source(src).expect("representative program must format");
        // Idempotent, and formatting is stable.
        assert_eq!(out, format_source(&out).unwrap(), "not idempotent");
        // The precedence-sensitive expression round-trips with correct structure (already checked by
        // the self-verification inside format_source; this pins a couple of surface expectations).
        assert!(
            out.contains("else if a > b"),
            "else-if chain preserved:\n{out}"
        );
        assert!(out.contains("|z| z * z + 1"), "closure preserved:\n{out}");
    }
}
