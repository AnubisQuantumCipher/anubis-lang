//! Real MVP lexer + parser for Anubis v0.1
//! Recognizes the critical syntax from the spec for dual modes, tainted, symbolic, hybrid, spec.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Keyword(String),
    Number(String),
    StringLit(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Semi,
    Comma,
    Dot,
    Star,
    Amp,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    EqEq,
    Plus,
    Minus,
    Eof,
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParseOutput {
    pub ast: AST,
    pub diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Safe,
    Research,
    Exploit,
}

#[derive(Debug, Clone)]
pub struct AST {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AttrArg {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<AttrArg>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Import {
        path: String,
        span: Span,
    },
    Module {
        name: String,
        items: Vec<Item>,
        span: Span,
    },
    Fn {
        name: String,
        params: Vec<(String, String)>,
        body: Vec<Stmt>,
        mode: Mode,
        intent: Option<String>,
        attributes: Vec<Attribute>,
        span: Span,
    },
    Struct {
        name: String,
        fields: Vec<(String, String)>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<String>,
        init: Expr,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        else_: Option<Vec<Stmt>>,
    },
    ResearchBlock {
        intent: Option<String>,
        body: Vec<Stmt>,
    },
    ExploitBlock {
        intent: Option<String>,
        body: Vec<Stmt>,
    },
    HybridBlock {
        gpu: Option<Vec<Stmt>>,
        cpu: Option<Vec<Stmt>>,
        prove: Option<Vec<Stmt>>,
    },
    SpecBlock {
        forall: String,
    },
    ExprStmt(Expr),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Var(String),
    Literal(String),
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        ty: String,
    },
    Tainted {
        ty: String,
        inner: Box<Expr>,
    },
    Symbolic {
        ty: String,
    },
    Assume(Box<Expr>),
    Assert(Box<Expr>),
    Declassify {
        inner: Box<Expr>,
        policy: Option<String>,
        reason: Option<String>,
    },
    TaintSource {
        label: String,
    },
    UnifiedBuffer {
        ty: String,
    },
    RawPtr {
        mutable: bool,
    },
    StructLiteral {
        name: String,
        fields: Vec<(String, Box<Expr>)>,
        span: Span,
    },
    FieldAccess {
        base: Box<Expr>,
        field: String,
        span: Span,
    },
    Other(String),
}

pub fn lex_spanned(source: &str) -> Vec<SpannedToken> {
    let mut tokens = vec![];
    let mut chars = source.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        match c {
            ' ' | '\n' | '\t' | '\r' => continue,
            '/' => {
                if let Some(&(_, '/')) = chars.peek() {
                    chars.next();
                    // skip to end of line (comments)
                    while let Some(&(_, nc)) = chars.peek() {
                        if nc == '\n' {
                            break;
                        }
                        chars.next();
                    }
                    continue;
                }
                // otherwise fallthrough (rare in our examples)
            }
            '{' => tokens.push(SpannedToken {
                token: Token::LBrace,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '}' => tokens.push(SpannedToken {
                token: Token::RBrace,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '(' => tokens.push(SpannedToken {
                token: Token::LParen,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            ')' => tokens.push(SpannedToken {
                token: Token::RParen,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '[' => tokens.push(SpannedToken {
                token: Token::LBracket,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            ']' => tokens.push(SpannedToken {
                token: Token::RBracket,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            ':' => tokens.push(SpannedToken {
                token: Token::Colon,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            ';' => tokens.push(SpannedToken {
                token: Token::Semi,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            ',' => tokens.push(SpannedToken {
                token: Token::Comma,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '.' => tokens.push(SpannedToken {
                token: Token::Dot,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '*' => tokens.push(SpannedToken {
                token: Token::Star,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '&' => tokens.push(SpannedToken {
                token: Token::Amp,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '<' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::Le,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Lt,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '>' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::Ge,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Gt,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '=' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::EqEq,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Eq,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '+' => tokens.push(SpannedToken {
                token: Token::Plus,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '-' => tokens.push(SpannedToken {
                token: Token::Minus,
                span: Span {
                    start,
                    end: start + 1,
                },
            }),
            '"' => {
                let mut s = String::new();
                let mut end = start + 1;
                while let Some(&(idx, nc)) = chars.peek() {
                    chars.next();
                    end = idx + nc.len_utf8();
                    if nc == '"' {
                        break;
                    }
                    s.push(nc);
                }
                tokens.push(SpannedToken {
                    token: Token::StringLit(s),
                    span: Span { start, end },
                });
            }
            c if c.is_ascii_digit() => {
                let mut num = c.to_string();
                let mut end = start + c.len_utf8();
                while let Some(&(idx, nc)) = chars.peek() {
                    if nc.is_ascii_digit() || nc == '.' {
                        chars.next();
                        num.push(nc);
                        end = idx + nc.len_utf8();
                    } else {
                        break;
                    }
                }
                tokens.push(SpannedToken {
                    token: Token::Number(num),
                    span: Span { start, end },
                });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut id = c.to_string();
                let mut end = start + c.len_utf8();
                while let Some(&(idx, nc)) = chars.peek() {
                    if nc.is_ascii_alphanumeric() || nc == '_' {
                        chars.next();
                        id.push(nc);
                        end = idx + nc.len_utf8();
                    } else {
                        break;
                    }
                }
                let token = match id.as_str() {
                    "fn" | "let" | "if" | "else" | "research" | "exploit" | "hybrid" | "gpu"
                    | "cpu" | "prove" | "spec" | "forall" | "tainted" | "symbolic" | "assume"
                    | "taint_source" | "assert" | "declassify" | "unified" | "Buffer"
                    | "intent" | "true" | "false" | "import" | "module" | "mod" | "struct"
                    | "return" | "as" => Token::Keyword(id),
                    _ => Token::Ident(id),
                };
                tokens.push(SpannedToken {
                    token,
                    span: Span { start, end },
                });
            }
            _ => {}
        }
    }
    tokens.push(SpannedToken {
        token: Token::Eof,
        span: Span {
            start: source.len(),
            end: source.len(),
        },
    });
    tokens
}

pub fn lex(source: &str) -> Vec<Token> {
    lex_spanned(source)
        .into_iter()
        .map(|spanned| spanned.token)
        .collect()
}

struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    diagnostics: Vec<ParseDiagnostic>,
}

impl Parser {
    fn new(tokens: Vec<SpannedToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: vec![],
        }
    }

    fn parse_output(mut self) -> ParseOutput {
        let mut items = vec![];
        while !self.at_eof() {
            let attrs = self.parse_attributes();
            if self.check_keyword("fn") {
                if let Some(item) = self.parse_fn(attrs) {
                    items.push(item);
                }
            } else if self.check_keyword("import") {
                if let Some(item) = self.parse_import() {
                    items.push(item);
                }
            } else if self.check_keyword("module") || self.check_keyword("mod") {
                if let Some(item) = self.parse_module() {
                    items.push(item);
                }
            } else if self.check_keyword("struct") {
                if let Some(item) = self.parse_struct() {
                    items.push(item);
                }
            } else {
                let span = self.current_span();
                self.diagnostic("expected item", span);
                self.bump();
            }
        }
        ParseOutput {
            ast: AST { items },
            diagnostics: self.diagnostics,
        }
    }

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = vec![];
        loop {
            if self.at_eof() {
                break;
            }
            let tok = match self.tokens.get(self.pos) {
                Some(t) => &t.token,
                None => break,
            };
            let s = match tok {
                Token::Other(s) | Token::Keyword(s) | Token::Ident(s) => s.clone(),
                _ => break,
            };
            if !(s == "@"
                || s.starts_with('@')
                || matches!(
                    s.as_str(),
                    "safe" | "research" | "proof" | "audit" | "poc" | "fuzz" | "defensive"
                ))
            {
                break;
            }
            self.bump();
            let name = if s.starts_with('@') {
                s.trim_start_matches('@').to_string()
            } else {
                s
            };
            let mut args = vec![];
            if self.check_token(&Token::LParen) {
                self.bump(); // (
                while !self.at_eof() && !self.check_token(&Token::RParen) {
                    if let Some((k, _)) = self.expect_ident("key") {
                        if self.check_token(&Token::Colon) {
                            let _ = self.bump();
                        }
                        // Gate15: support simple val or [list] for effects etc.
                        let mut val = String::new();
                        if self.check_token(&Token::LBracket) {
                            // consume [ ... ] as single value string e.g. "[file_read]"
                            let mut depth = 0;
                            let mut buf = String::new();
                            while !self.at_eof() {
                                let t = self.bump();
                                if t.is_none() {
                                    break;
                                }
                                let tt = t.unwrap().token;
                                if matches!(tt, Token::LBracket) {
                                    depth += 1;
                                    buf.push('[');
                                } else if matches!(tt, Token::RBracket) {
                                    depth -= 1;
                                    buf.push(']');
                                    if depth == 0 {
                                        break;
                                    }
                                } else if let Token::StringLit(s)
                                | Token::Ident(s)
                                | Token::Keyword(s)
                                | Token::Other(s)
                                | Token::Number(s) = tt
                                {
                                    buf.push_str(&s);
                                } else {
                                    buf.push_str(&format!("{:?}", tt));
                                }
                            }
                            val = buf;
                        } else if let Some(val_st) = self.bump() {
                            let val_tok = val_st.token;
                            val = match val_tok {
                                Token::StringLit(v) => v.trim_matches('"').to_string(),
                                Token::Ident(v)
                                | Token::Keyword(v)
                                | Token::Other(v)
                                | Token::Number(v) => v,
                                _ => "".into(),
                            };
                        }
                        args.push(AttrArg { key: k, value: val });
                    }
                    if self.check_token(&Token::Comma) {
                        let _ = self.bump();
                    }
                }
                if self.check_token(&Token::RParen) {
                    let _ = self.bump();
                }
            }
            attrs.push(Attribute { name, args });
        }
        attrs
    }

    fn parse_import(&mut self) -> Option<Item> {
        let start = self.expect_keyword("import")?.span;
        let mut path = String::new();
        while !self.at_eof() && !self.check_token(&Token::Semi) {
            let Some(tok) = self.bump() else {
                break;
            };
            match tok.token {
                Token::Ident(s) | Token::Keyword(s) => path.push_str(&s),
                Token::Dot => path.push('.'),
                other => {
                    self.diagnostic(
                        format!("unexpected token in import path: {:?}", other),
                        tok.span,
                    );
                }
            }
        }
        let end = if self.check_token(&Token::Semi) {
            self.bump().map(|tok| tok.span.end).unwrap_or(start.end)
        } else {
            self.diagnostic("expected `;` after import", self.current_span());
            self.current_span().end
        };
        Some(Item::Import {
            path,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_struct(&mut self) -> Option<Item> {
        let start = self.expect_keyword("struct")?.span;
        let (name, _) = self.expect_ident("expected struct name")?;
        let _ = self.expect_token(Token::LBrace, "expected `{` after struct name");
        let mut fields = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            if let Some((fname, _)) = self.expect_ident("expected field name") {
                let _ = self.expect_token(Token::Colon, "expected `:` after field");
                let fty = self.collect_type_until(&[Token::Comma, Token::RBrace, Token::Semi]);
                fields.push((fname, fty));
            }
            if self.check_token(&Token::Comma) || self.check_token(&Token::Semi) {
                self.bump();
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}` after struct");
        let end = self.previous_end();
        Some(Item::Struct {
            name,
            fields,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_module(&mut self) -> Option<Item> {
        let start = self.bump()?.span;
        let (name, _) = self.expect_ident("expected module name")?;
        let _ = self.expect_token(Token::LBrace, "expected `{` after module name");
        let mut items = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            if self.check_keyword("fn") {
                let attrs = self.parse_attributes();
                if let Some(item) = self.parse_fn(attrs) {
                    items.push(item);
                }
            } else if self.check_keyword("import") {
                if let Some(item) = self.parse_import() {
                    items.push(item);
                }
            } else if self.check_keyword("module") || self.check_keyword("mod") {
                if let Some(item) = self.parse_module() {
                    items.push(item);
                }
            } else {
                self.diagnostic("expected item in module", self.current_span());
                self.bump();
            }
        }
        let end = if self.check_token(&Token::RBrace) {
            self.bump().map(|tok| tok.span.end).unwrap_or(start.end)
        } else {
            self.diagnostic("expected `}` after module", self.current_span());
            self.current_span().end
        };
        Some(Item::Module {
            name,
            items,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_fn(&mut self, pre_attrs: Vec<Attribute>) -> Option<Item> {
        let start = self.expect_keyword("fn")?.span;
        let (name, _) = self.expect_ident("expected function name")?;
        let params = self.parse_params();
        // Optional return type: -> Type
        if self.check_token(&Token::Minus) {
            // for ->
            // consume - >
            let _ = self.bump();
            if self.check_token(&Token::Gt) {
                let _ = self.bump();
            }
            // consume the type name token (simple ident)
            let _ = self.bump();
            // ignore for now; not stored in AST for this slice
        }
        let body_start = self.current_span();
        let body = if self.check_token(&Token::LBrace) {
            self.parse_block()
        } else {
            self.diagnostic("expected `{` before function body", body_start);
            vec![]
        };
        let span = Span {
            start: start.start,
            end: self.previous_end().max(start.end),
        };
        let mut mode = infer_mode(&body);
        // Gate 15: attributes can set/override mode for security capabilities
        for attr in &pre_attrs {
            match attr.name.as_str() {
                "safe" => mode = Mode::Safe,
                "research" | "poc" | "fuzz" | "proof" | "defensive" | "audit" => {
                    mode = Mode::Research
                }
                _ => {}
            }
        }
        Some(Item::Fn {
            name,
            params,
            body,
            mode,
            intent: None,
            attributes: pre_attrs,
            span,
        })
    }

    fn parse_params(&mut self) -> Vec<(String, String)> {
        let mut params = vec![];
        if self
            .expect_token(Token::LParen, "expected `(` before parameters")
            .is_none()
        {
            return params;
        }
        while !self.at_eof() && !self.check_token(&Token::RParen) {
            let Some((name, _)) = self.expect_ident("expected parameter name") else {
                self.synchronize_param();
                continue;
            };
            let mut ty = String::new();
            if self
                .expect_token(Token::Colon, "expected `:` after parameter name")
                .is_some()
            {
                ty = self.collect_type_until(&[Token::Comma, Token::RParen]);
            }
            params.push((name, ty));
            if self.check_token(&Token::Comma) {
                self.bump();
            } else if !self.check_token(&Token::RParen) {
                self.diagnostic("expected `,` or `)` after parameter", self.current_span());
                self.synchronize_param();
            }
        }
        let _ = self.expect_token(Token::RParen, "expected `)` after parameters");
        params
    }

    fn parse_block(&mut self) -> Vec<Stmt> {
        let _ = self.expect_token(Token::LBrace, "expected `{`");
        let mut body = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            if let Some(stmt) = self.parse_stmt() {
                body.push(stmt);
            } else {
                self.bump();
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}` after block");
        body
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        if self.check_keyword("let") {
            return self.parse_let();
        }
        if self.check_keyword("if") {
            return self.parse_if_stmt();
        }
        if self.check_keyword("research") {
            let _start = self.bump()?;
            let body = self.parse_block();
            return Some(Stmt::ResearchBlock { intent: None, body });
        }
        if self.check_keyword("exploit") {
            let _start = self.bump()?;
            let body = self.parse_block();
            return Some(Stmt::ExploitBlock { intent: None, body });
        }
        if self.check_keyword("hybrid") {
            return self.parse_hybrid();
        }
        if self.check_keyword("spec") {
            return Some(self.parse_spec());
        }
        if self.check_keyword("assume") || self.check_keyword("assert") {
            return self.parse_assume_or_assert();
        }
        if self.check_keyword("symbolic") {
            let expr = self.parse_primary();
            self.consume_optional_semi();
            return Some(Stmt::ExprStmt(expr));
        }
        if self.check_keyword("return") {
            self.bump();
            let val = if self.check_token(&Token::Semi) {
                Expr::Literal("0".into())
            } else {
                self.parse_expr(0)
            };
            self.consume_optional_semi();
            // Represent return as a call to a builtin for lowering compatibility in this slice
            return Some(Stmt::ExprStmt(Expr::Call {
                callee: "return".into(),
                args: vec![val],
            }));
        }
        if self.starts_expr() {
            let expr = self.parse_expr(0);
            self.consume_optional_semi();
            return Some(Stmt::ExprStmt(expr));
        }
        self.diagnostic("expected statement", self.current_span());
        None
    }

    fn parse_let(&mut self) -> Option<Stmt> {
        let start = self.expect_keyword("let")?.span;
        let (name, _) = self.expect_ident("expected binding name after `let`")?;
        let ty = if self.check_token(&Token::Colon) {
            self.bump();
            let ty = self.collect_type_until(&[Token::Eq, Token::Semi, Token::RBrace]);
            if ty.is_empty() {
                None
            } else if ty.to_lowercase().contains("tainted") {
                Some("tainted<u32>".into())
            } else {
                Some(ty)
            }
        } else {
            None
        };
        let init = if self.check_token(&Token::Eq) {
            self.bump();
            self.parse_expr(0)
        } else {
            self.diagnostic("expected `=` in let binding", self.current_span());
            Expr::Other("missing-init".into())
        };

        let end = if self.check_token(&Token::Semi) {
            self.bump().map(|t| t.span.end).unwrap_or(start.end)
        } else {
            self.diagnostic("expected `;` after let binding", self.current_span());
            start.merge(self.current_span()).end
        };

        Some(Stmt::Let {
            name,
            ty,
            init,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_if_stmt(&mut self) -> Option<Stmt> {
        let _ = self.expect_keyword("if");
        let cond = self.parse_expr(0);
        let then = if self.check_token(&Token::LBrace) {
            self.parse_block()
        } else {
            self.diagnostic("expected `{` after if cond", self.current_span());
            vec![]
        };
        let mut else_ = None;
        if self.check_keyword("else") {
            let _ = self.bump();
            else_ = Some(if self.check_token(&Token::LBrace) {
                self.parse_block()
            } else {
                self.diagnostic("expected `{` after else", self.current_span());
                vec![]
            });
        }
        Some(Stmt::If { cond, then, else_ })
    }

    fn parse_assume_or_assert(&mut self) -> Option<Stmt> {
        let is_assert = self.check_keyword("assert");
        self.bump();
        let _ = self.expect_token(Token::LParen, "expected `(` after assume/assert");
        let expr = self.parse_expr(0);
        let _ = self.expect_token(Token::RParen, "expected `)` after expression");
        self.consume_optional_semi();
        Some(Stmt::ExprStmt(if is_assert {
            Expr::Assert(Box::new(expr))
        } else {
            Expr::Assume(Box::new(expr))
        }))
    }

    fn parse_hybrid(&mut self) -> Option<Stmt> {
        self.expect_keyword("hybrid")?;
        let _ = self.expect_token(Token::LBrace, "expected `{` after hybrid");
        let mut gpu = None;
        let mut cpu = None;
        let mut prove = None;
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            if self.check_keyword("gpu") {
                self.bump();
                self.skip_parens_if_present();
                gpu = Some(self.parse_block());
            } else if self.check_keyword("cpu") {
                self.bump();
                cpu = Some(self.parse_block());
            } else if self.check_keyword("prove") {
                self.bump();
                self.skip_parens_if_present();
                prove = Some(self.parse_block());
            } else {
                self.diagnostic(
                    "expected gpu/cpu/prove section in hybrid block",
                    self.current_span(),
                );
                self.bump();
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}` after hybrid block");
        Some(Stmt::HybridBlock { gpu, cpu, prove })
    }

    fn parse_spec(&mut self) -> Stmt {
        self.bump();
        let _ = self.expect_token(Token::LBrace, "expected `{` after spec");
        let mut forall = "x".to_string();
        let mut depth = 1;
        while !self.at_eof() && depth > 0 {
            let Some(tok) = self.bump() else {
                break;
            };
            match tok.token {
                Token::LBrace => depth += 1,
                Token::RBrace => depth -= 1,
                Token::Ident(v) | Token::Number(v) if v != "forall" => forall = v,
                _ => {}
            }
        }
        Stmt::SpecBlock { forall }
    }

    fn parse_expr(&mut self, min_prec: u8) -> Expr {
        let mut lhs = self.parse_primary();
        loop {
            if self.check_keyword("as") {
                self.bump();
                let ty = self.collect_type_until(&[
                    Token::Semi,
                    Token::Comma,
                    Token::RParen,
                    Token::RBrace,
                ]);
                lhs = Expr::Cast {
                    expr: Box::new(lhs),
                    ty,
                };
                continue;
            }
            let Some((op, prec)) = self.current_binary_op() else {
                break;
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(prec + 1);
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        lhs
    }

    fn parse_primary(&mut self) -> Expr {
        let Some(tok) = self.bump() else {
            return Expr::Other("eof".into());
        };
        match tok.token {
            Token::Number(n) | Token::StringLit(n) => Expr::Literal(n),
            Token::Ident(name) => {
                if self.check_token(&Token::LParen) {
                    let args = self.parse_call_args();
                    let mut e = Expr::Call {
                        callee: name.clone(),
                        args,
                    };
                    // allow field on call result: foo().bar
                    while self.check_token(&Token::Dot) {
                        self.bump();
                        if let Some((f, _)) = self.expect_ident("expected field after .") {
                            e = Expr::FieldAccess {
                                base: Box::new(e),
                                field: f,
                                span: tok.span,
                            };
                        }
                    }
                    e
                } else if self.check_token(&Token::LBrace) {
                    // struct literal: Name { f: e, ... }
                    self.bump();
                    let mut fields = vec![];
                    while !self.at_eof() && !self.check_token(&Token::RBrace) {
                        if let Some((fname, _)) = self.expect_ident("expected field in struct lit")
                        {
                            let _ = self.expect_token(Token::Colon, "expected : in struct lit");
                            let val = self.parse_expr(0);
                            fields.push((fname, Box::new(val)));
                        }
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RBrace, "expected } in struct lit");
                    let mut e = Expr::StructLiteral {
                        name: name.clone(),
                        fields,
                        span: tok.span,
                    };
                    while self.check_token(&Token::Dot) {
                        self.bump();
                        if let Some((f, _)) = self.expect_ident("expected field") {
                            e = Expr::FieldAccess {
                                base: Box::new(e),
                                field: f,
                                span: tok.span,
                            };
                        }
                    }
                    e
                } else {
                    let mut e = Expr::Var(name.clone());
                    // field access: p.x or chained
                    while self.check_token(&Token::Dot) {
                        self.bump();
                        if let Some((f, _)) = self.expect_ident("expected field after .") {
                            e = Expr::FieldAccess {
                                base: Box::new(e),
                                field: f,
                                span: tok.span,
                            };
                        }
                    }
                    e
                }
            }
            Token::Keyword(k) if k == "true" || k == "false" => Expr::Literal(k),
            Token::Keyword(k) if k == "symbolic" => {
                let ty = self
                    .parse_optional_generic_ty()
                    .unwrap_or_else(|| "u32".into());
                self.skip_parens_if_present();
                Expr::Symbolic { ty }
            }
            Token::Keyword(k) if k == "taint_source" => {
                let args = self.parse_call_args();
                let label = if args.is_empty() {
                    "unknown".to_string()
                } else {
                    match &args[0] {
                        Expr::Literal(s) | Expr::Var(s) => s.clone(),
                        _ => "unknown".to_string(),
                    }
                };
                Expr::TaintSource { label }
            }
            Token::Ident(k) if k == "taint_source" => {
                let args = self.parse_call_args();
                let label = if args.is_empty() {
                    "unknown".to_string()
                } else {
                    match &args[0] {
                        Expr::Literal(s) | Expr::Var(s) => s.clone(),
                        _ => "unknown".to_string(),
                    }
                };
                Expr::TaintSource { label }
            }
            Token::Keyword(k) if k == "declassify" => {
                let mut args = self.parse_call_args();
                if args.is_empty() {
                    Expr::Declassify {
                        inner: Box::new(Expr::Other("missing-declassify-arg".into())),
                        policy: None,
                        reason: None,
                    }
                } else {
                    let inner = args.remove(0);
                    // Support declassify(v), declassify(v, p, r), declassify(v, policy: "p", reason: "r")
                    // Consume any leading "policy" / "reason" idents and their : if present
                    let mut policy: Option<String> = None;
                    let mut reason: Option<String> = None;
                    let mut i = 0;
                    while i < args.len() {
                        if let Expr::Var(name) = &args[i] {
                            if name == "policy" || name == "reason" {
                                // skip the ident, next should be the value or :
                                i += 1;
                                // if next is literal use it, else take next
                                if i < args.len() {
                                    if let Expr::Literal(s) | Expr::Var(s) = &args[i] {
                                        if name == "policy" {
                                            policy = Some(s.clone());
                                        } else {
                                            reason = Some(s.clone());
                                        }
                                        i += 1;
                                        continue;
                                    }
                                }
                            }
                        } else if let Expr::Literal(s) | Expr::Var(s) = &args[i] {
                            // positional: first extra = policy, second = reason
                            if policy.is_none() {
                                policy = Some(s.clone());
                            } else if reason.is_none() {
                                reason = Some(s.clone());
                            }
                            i += 1;
                            continue;
                        }
                        i += 1;
                    }
                    Expr::Declassify {
                        inner: Box::new(inner),
                        policy,
                        reason,
                    }
                }
            }
            Token::Keyword(k) if k == "unified" => {
                if self.check_keyword("Buffer") {
                    self.bump();
                }
                let ty = self
                    .parse_optional_generic_ty()
                    .unwrap_or_else(|| "u8".into());
                Expr::UnifiedBuffer { ty }
            }
            Token::LParen => {
                let expr = self.parse_expr(0);
                let _ = self.expect_token(Token::RParen, "expected `)` after expression");
                expr
            }
            other => Expr::Other(format!("{:?}", other)),
        }
    }

    fn parse_call_args(&mut self) -> Vec<Expr> {
        let mut args = vec![];
        let _ = self.expect_token(Token::LParen, "expected `(` for call");
        while !self.at_eof() && !self.check_token(&Token::RParen) {
            args.push(self.parse_expr(0));
            if self.check_token(&Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        let _ = self.expect_token(Token::RParen, "expected `)` after call arguments");
        args
    }

    fn parse_optional_generic_ty(&mut self) -> Option<String> {
        if self.check_token(&Token::Colon)
            && matches!(
                self.tokens.get(self.pos + 1).map(|tok| &tok.token),
                Some(Token::Colon)
            )
        {
            self.bump();
            self.bump();
        }
        if self.check_token(&Token::Lt) {
            self.bump();
            let ty = self.collect_type_until(&[Token::Gt]);
            let _ = self.expect_token(Token::Gt, "expected `>` after generic type");
            Some(ty)
        } else {
            None
        }
    }

    fn current_binary_op(&self) -> Option<(String, u8)> {
        match &self.current().token {
            Token::Plus => Some(("+".into(), 20)),
            Token::Minus => Some(("-".into(), 20)),
            Token::Star => Some(("*".into(), 30)),
            Token::Lt => Some(("<".into(), 10)),
            Token::Gt => Some((">".into(), 10)),
            Token::Le => Some(("<=".into(), 10)),
            Token::Ge => Some((">=".into(), 10)),
            Token::EqEq => Some(("==".into(), 10)),
            _ => None,
        }
    }

    fn starts_expr(&self) -> bool {
        match &self.current().token {
            Token::Ident(_) | Token::Number(_) | Token::StringLit(_) | Token::LParen => true,
            Token::Keyword(k) => k == "declassify" || k == "true" || k == "false" || k == "unified",
            _ => false,
        }
    }

    fn collect_type_until(&mut self, stops: &[Token]) -> String {
        let mut ty = String::new();
        while !self.at_eof() && !stops.iter().any(|stop| self.check_token(stop)) {
            let Some(tok) = self.bump() else {
                break;
            };
            match tok.token {
                Token::Ident(s) | Token::Keyword(s) | Token::Number(s) => ty.push_str(&s),
                Token::Lt => ty.push('<'),
                Token::Gt => ty.push('>'),
                Token::Star => ty.push('*'),
                Token::Amp => ty.push('&'),
                Token::Comma | Token::Colon | Token::Dot => {}
                _ => {}
            }
        }
        ty
    }

    fn synchronize_param(&mut self) {
        while !self.at_eof()
            && !self.check_token(&Token::Comma)
            && !self.check_token(&Token::RParen)
        {
            self.bump();
        }
        if self.check_token(&Token::Comma) {
            self.bump();
        }
    }

    fn skip_parens_if_present(&mut self) {
        if !self.check_token(&Token::LParen) {
            return;
        }
        let mut depth = 0;
        while !self.at_eof() {
            let Some(tok) = self.bump() else {
                break;
            };
            match tok.token {
                Token::LParen => depth += 1,
                Token::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn consume_optional_semi(&mut self) {
        if self.check_token(&Token::Semi) {
            self.bump();
        }
    }

    fn expect_ident(&mut self, message: &str) -> Option<(String, Span)> {
        let tok = self.current().clone();
        match tok.token {
            Token::Ident(name) | Token::Keyword(name) if name != "fn" && name != "let" => {
                self.bump();
                Some((name, tok.span))
            }
            _ => {
                self.diagnostic(message, tok.span);
                None
            }
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Option<SpannedToken> {
        if self.check_keyword(keyword) {
            self.bump()
        } else {
            self.diagnostic(format!("expected `{}`", keyword), self.current_span());
            None
        }
    }

    fn expect_token(&mut self, token: Token, message: &str) -> Option<SpannedToken> {
        if self.check_token(&token) {
            self.bump()
        } else {
            self.diagnostic(message, self.current_span());
            None
        }
    }

    fn diagnostic(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics.push(ParseDiagnostic {
            message: message.into(),
            span,
        });
    }

    fn current(&self) -> &SpannedToken {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("parser has eof token"))
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn previous_end(&self) -> usize {
        self.pos
            .checked_sub(1)
            .and_then(|idx| self.tokens.get(idx))
            .map(|tok| tok.span.end)
            .unwrap_or(0)
    }

    fn bump(&mut self) -> Option<SpannedToken> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn check_keyword(&self, keyword: &str) -> bool {
        matches!(&self.current().token, Token::Keyword(k) if k == keyword)
    }

    fn check_token(&self, token: &Token) -> bool {
        std::mem::discriminant(&self.current().token) == std::mem::discriminant(token)
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().token, Token::Eof)
    }
}

fn infer_mode(body: &[Stmt]) -> Mode {
    if body
        .iter()
        .any(|stmt| matches!(stmt, Stmt::ExploitBlock { .. }))
    {
        Mode::Exploit
    } else if body
        .iter()
        .any(|stmt| matches!(stmt, Stmt::ResearchBlock { .. }))
    {
        Mode::Research
    } else {
        Mode::Safe
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<AST, String> {
    let spanned = tokens
        .into_iter()
        .enumerate()
        .map(|(idx, token)| SpannedToken {
            token,
            span: Span {
                start: idx,
                end: idx,
            },
        })
        .collect();
    let output = Parser::new(spanned).parse_output();
    if !output.diagnostics.is_empty() {
        Err(output
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; "))
    } else {
        Ok(output.ast)
    }
}

pub fn parse_source_detailed(source: &str) -> ParseOutput {
    Parser::new(lex_spanned(source)).parse_output()
}

pub fn parse_source(source: &str) -> Result<AST, String> {
    let output = parse_source_detailed(source);
    if !output.diagnostics.is_empty() {
        Err(output
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; "))
    } else {
        Ok(output.ast)
    }
}
