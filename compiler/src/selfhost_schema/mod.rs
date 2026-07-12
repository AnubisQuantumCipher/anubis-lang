//! Phase-8 self-host schema: deterministic token/AST dumps shared by host goldens and Anubis-SH.
//!
//! JSON rules: no whitespace pretty-print for compare path (`to_string`); tests may pretty-print.
//! Spans are UTF-8 byte offsets. Comments are **omitted** from token dumps (SH lexer skips them
//! for the parse stream, matching host parse path).

use crate::frontend::{
    lex_spanned, parse_source, Expr, Item, Stmt, Token, Visibility, AST,
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ShToken {
    pub kind: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Lex source and project to SH tokens (comments stripped — parse-relevant stream).
pub fn dump_tokens(source: &str) -> Vec<ShToken> {
    lex_spanned(source)
        .into_iter()
        .filter(|st| {
            !matches!(
                st.token,
                Token::LineComment(_) | Token::BlockComment(_) | Token::Eof
            )
        })
        .map(|st| {
            let (kind, text) = token_kind_text(&st.token);
            ShToken {
                kind,
                text,
                start: st.span.start,
                end: st.span.end,
            }
        })
        .collect()
}

fn token_kind_text(t: &Token) -> (String, String) {
    match t {
        Token::Ident(s) => ("Ident".into(), s.clone()),
        Token::Keyword(s) => ("Keyword".into(), s.clone()),
        Token::Number(s) => ("Number".into(), s.clone()),
        Token::StringLit(s) => ("String".into(), s.clone()),
        Token::LParen => ("LParen".into(), "(".into()),
        Token::RParen => ("RParen".into(), ")".into()),
        Token::LBrace => ("LBrace".into(), "{".into()),
        Token::RBrace => ("RBrace".into(), "}".into()),
        Token::LBracket => ("LBracket".into(), "[".into()),
        Token::RBracket => ("RBracket".into(), "]".into()),
        Token::Colon => ("Colon".into(), ":".into()),
        Token::ColonColon => ("ColonColon".into(), "::".into()),
        Token::FatArrow => ("FatArrow".into(), "=>".into()),
        Token::Semi => ("Semi".into(), ";".into()),
        Token::Comma => ("Comma".into(), ",".into()),
        Token::Question => ("Question".into(), "?".into()),
        Token::Dot => ("Dot".into(), ".".into()),
        Token::DotDot => ("DotDot".into(), "..".into()),
        Token::Star => ("Star".into(), "*".into()),
        Token::Slash => ("Slash".into(), "/".into()),
        Token::Percent => ("Percent".into(), "%".into()),
        Token::Amp => ("Amp".into(), "&".into()),
        Token::AmpAmp => ("AmpAmp".into(), "&&".into()),
        Token::Pipe => ("Pipe".into(), "|".into()),
        Token::PipePipe => ("PipePipe".into(), "||".into()),
        Token::Caret => ("Caret".into(), "^".into()),
        Token::Tilde => ("Tilde".into(), "~".into()),
        Token::Shl => ("Shl".into(), "<<".into()),
        Token::Shr => ("Shr".into(), ">>".into()),
        Token::Bang => ("Bang".into(), "!".into()),
        Token::Lt => ("Lt".into(), "<".into()),
        Token::Gt => ("Gt".into(), ">".into()),
        Token::Le => ("Le".into(), "<=".into()),
        Token::Ge => ("Ge".into(), ">=".into()),
        Token::Eq => ("Eq".into(), "=".into()),
        Token::EqEq => ("EqEq".into(), "==".into()),
        Token::Ne => ("Ne".into(), "!=".into()),
        Token::Plus => ("Plus".into(), "+".into()),
        Token::Minus => ("Minus".into(), "-".into()),
        Token::OpAssign(op) => ("OpAssign".into(), format!("{op}=")),
        Token::LineComment(s) => ("LineComment".into(), s.clone()),
        Token::BlockComment(s) => ("BlockComment".into(), s.clone()),
        Token::Eof => ("Eof".into(), "".into()),
        Token::Other(s) => ("Other".into(), s.clone()),
    }
}

/// Compact JSON (no spaces) for golden comparison.
pub fn tokens_json(source: &str) -> String {
    serde_json::to_string(&dump_tokens(source)).expect("serialize tokens")
}

/// Project host AST into SH schema (subset of items/stmts/exprs used by self-host).
pub fn dump_ast(source: &str) -> Result<serde_json::Value, String> {
    let ast = parse_source(source)?;
    Ok(project_ast(&ast))
}

pub fn ast_json(source: &str) -> Result<String, String> {
    let v = dump_ast(source)?;
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

pub fn dump_tokens_path(path: &Path) -> Result<String, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(tokens_json(&source))
}

pub fn dump_ast_path(path: &Path) -> Result<String, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    ast_json(&source)
}

fn project_ast(ast: &AST) -> serde_json::Value {
    let items: Vec<_> = ast.items.iter().filter_map(project_item).collect();
    serde_json::json!({ "kind": "Program", "items": items })
}

fn project_item(it: &Item) -> Option<serde_json::Value> {
    match it {
        Item::Fn {
            name,
            visibility,
            params,
            ret,
            requires,
            ensures,
            effects,
            body,
            ..
        } => {
            let params: Vec<_> = params
                .iter()
                .map(|(n, t)| {
                    serde_json::json!({
                        "name": n,
                        "ty": if t.is_empty() { serde_json::Value::Null } else { t.clone().into() }
                    })
                })
                .collect();
            let req: Vec<_> = requires.iter().map(project_expr).collect();
            let ens: Vec<_> = ensures.iter().map(project_expr).collect();
            let body: Vec<_> = body.iter().map(project_stmt).collect();
            Some(serde_json::json!({
                "kind": "Fn",
                "name": name,
                "pub": matches!(visibility, Visibility::Public),
                "params": params,
                "ret": ret,
                "requires": req,
                "ensures": ens,
                "effects": effects,
                "body": body,
            }))
        }
        Item::Import { path, .. } => Some(serde_json::json!({
            "kind": "Import",
            "path": path,
        })),
        // SH v1 ignores modules/structs/enums/traits at dump (not in subset for self-host sources)
        _ => None,
    }
}

fn project_stmt(st: &Stmt) -> serde_json::Value {
    match st {
        Stmt::Let { name, ty, init, .. } => serde_json::json!({
            "kind": "Let",
            "name": name,
            "ty": ty,
            "init": project_expr(init),
        }),
        Stmt::Assign { target, value } => serde_json::json!({
            "kind": "Assign",
            "target": project_expr(target),
            "value": project_expr(value),
        }),
        Stmt::ExprStmt(e) => {
            // Host represents `return x` as Call { callee: "return", args: [x] } sometimes
            if let Expr::Call { callee, args } = e {
                if callee == "return" {
                    let val = args.first().map(project_expr);
                    return serde_json::json!({ "kind": "Return", "value": val });
                }
            }
            serde_json::json!({ "kind": "ExprStmt", "expr": project_expr(e) })
        }
        Stmt::If { cond, then, else_ } => serde_json::json!({
            "kind": "If",
            "cond": project_expr(cond),
            "then": then.iter().map(project_stmt).collect::<Vec<_>>(),
            "else": else_.as_ref().map(|b| b.iter().map(project_stmt).collect::<Vec<_>>()),
        }),
        Stmt::While { cond, body, .. } => serde_json::json!({
            "kind": "While",
            "cond": project_expr(cond),
            "body": body.iter().map(project_stmt).collect::<Vec<_>>(),
        }),
        Stmt::For {
            var, source, body, ..
        } => {
            use crate::frontend::ForSource;
            let iter = match source {
                ForSource::Range { start, end } => serde_json::json!({
                    "kind": "Range",
                    "start": project_expr(start),
                    "end": project_expr(end),
                }),
                ForSource::Collection { expr } => project_expr(expr),
            };
            serde_json::json!({
                "kind": "For",
                "var": var,
                "iter": iter,
                "body": body.iter().map(project_stmt).collect::<Vec<_>>(),
            })
        }
        _ => serde_json::json!({ "kind": "UnsupportedStmt" }),
    }
}

fn project_expr(e: &Expr) -> serde_json::Value {
    match e {
        Expr::Var(s) => serde_json::json!({ "kind": "Var", "name": s }),
        Expr::Literal(s) => {
            if s == "true" || s == "false" {
                serde_json::json!({ "kind": "Bool", "value": s == "true" })
            } else {
                serde_json::json!({ "kind": "Int", "value": s })
            }
        }
        Expr::StrLiteral(s) => serde_json::json!({ "kind": "Str", "value": s }),
        Expr::Call { callee, args } => serde_json::json!({
            "kind": "Call",
            "callee": callee,
            "args": args.iter().map(project_expr).collect::<Vec<_>>(),
        }),
        Expr::Binary { op, lhs, rhs } => serde_json::json!({
            "kind": "Binary",
            "op": op,
            "lhs": project_expr(lhs),
            "rhs": project_expr(rhs),
        }),
        Expr::Unary { op, expr } => serde_json::json!({
            "kind": "Unary",
            "op": op,
            "expr": project_expr(expr),
        }),
        Expr::Index { base, index } => serde_json::json!({
            "kind": "Index",
            "base": project_expr(base),
            "index": project_expr(index),
        }),
        Expr::FieldAccess { base, field, .. } => serde_json::json!({
            "kind": "Field",
            "base": project_expr(base),
            "field": field,
        }),
        Expr::ArrayLiteral { elements } => serde_json::json!({
            "kind": "List",
            "elements": elements.iter().map(project_expr).collect::<Vec<_>>(),
        }),
        _ => serde_json::json!({ "kind": "UnsupportedExpr" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_hello_stable() {
        let src = "fn main() {\n    print(\"hello\");\n}\n";
        let j = tokens_json(src);
        assert!(j.contains("\"kind\":\"Keyword\""), "{j}");
        assert!(j.contains("\"text\":\"fn\""), "{j}");
        assert!(j.contains("main"), "{j}");
        // deterministic: second call identical
        assert_eq!(j, tokens_json(src));
    }

    #[test]
    fn ast_hello_has_fn() {
        let src = "fn main() { print(1); }\n";
        let j = ast_json(src).unwrap();
        assert!(j.contains("\"kind\":\"Program\""), "{j}");
        assert!(j.contains("\"name\":\"main\""), "{j}");
    }

    #[test]
    fn args_not_required_for_schema() {
        // schema module is pure string in/out
        let _ = dump_tokens("let x = 1;");
    }
}
