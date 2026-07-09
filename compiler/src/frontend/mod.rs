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
    ColonColon,
    FatArrow,
    Semi,
    Comma,
    Dot,
    DotDot,
    Star,
    Slash,
    Percent,
    Amp,
    AmpAmp,
    Pipe,
    PipePipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    Bang,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    EqEq,
    Ne,
    Plus,
    Minus,
    /// A compound-assignment operator; the payload is the base op (`"+"`, `"<<"`, …).
    OpAssign(String),
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
        /// Declared return type (`-> T`), captured verbatim; `None` if omitted.
        ret: Option<String>,
        attributes: Vec<Attribute>,
        span: Span,
    },
    Struct {
        name: String,
        fields: Vec<(String, String)>,
        span: Span,
    },
    /// `enum Status { Ok, Err(u32), Pending }`
    Enum {
        name: String,
        variants: Vec<EnumVariant>,
        span: Span,
    },
}

/// Shape of an enum variant.
#[derive(Debug, Clone)]
pub enum EnumVariantKind {
    Unit,
    /// `Err(u32, bool)`
    Tuple(Vec<String>),
    /// `Err { code: u32, msg: u32 }`
    Struct(Vec<(String, String)>),
}

/// Enum variant declaration.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub kind: EnumVariantKind,
}

/// Pattern for `match` arms.
#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    /// `Status::Ok`, `Status::Err(n)`, or `Status::Err { code: c }`
    EnumVariant {
        enum_name: String,
        variant: String,
        /// Positional bindings for tuple variants.
        bindings: Vec<String>,
        /// Named field bindings `(field, bind)` for struct variants.
        named_bindings: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
}

/// Source of a `for` loop: numeric range or collection iteration.
#[derive(Debug, Clone)]
pub enum ForSource {
    /// `for i in a..b` — half-open range [a, b)
    Range { start: Expr, end: Expr },
    /// `for x in xs` — iterate list/string elements
    Collection { expr: Expr },
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
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Loop {
        body: Vec<Stmt>,
    },
    /// `for v in a..b { }` or `for v in collection { }`
    For {
        var: String,
        source: ForSource,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
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
    /// Numeric or boolean literal, carried as text (`"42"`, `"3.14"`, `"1e9"`, `"true"`).
    Literal(String),
    /// String (or char) literal with its decoded value (escapes already resolved).
    StrLiteral(String),
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    /// Application of an arbitrary callee expression: `expr(args)`. Produced by a postfix `(...)`
    /// after a field access, index, or call, so `obj.f(x)`, `arr[i](x)`, and `f(a)(b)` all work
    /// with first-class closure values.
    CallExpr {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    ArrayLiteral {
        elements: Vec<Expr>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
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
    /// `Status::Ok`, `Status::Err(x, y)`, or `Status::Err { code: x }`
    EnumConstruct {
        enum_name: String,
        variant: String,
        /// Positional field expressions (tuple) or values in `field_names` order (struct).
        fields: Vec<Expr>,
        /// Empty for unit/tuple; for struct, names parallel to `fields`.
        field_names: Vec<String>,
        span: Span,
    },
    /// `match scrutinee { Status::Ok => 0, Status::Err(n) => n, _ => 1 }`
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// `if c { a } else { b }` (expression; else required)
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
        span: Span,
    },
    /// `{ k: v, ... }` dictionary / map literal
    MapLiteral {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// A block expression: statements followed by an optional trailing value
    /// (`{ let t = a + b; t * 2 }`). Appears as the branches of an `if` expression.
    Block {
        stmts: Vec<Stmt>,
        tail: Option<Box<Expr>>,
    },
    /// A lambda / closure literal: `|x, y| body` or `|| body`. The body is a single expression
    /// (which may itself be a block expression).
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
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
                    // Line comment: skip to end of line.
                    while let Some(&(_, nc)) = chars.peek() {
                        if nc == '\n' {
                            break;
                        }
                        chars.next();
                    }
                    continue;
                }
                if let Some(&(_, '*')) = chars.peek() {
                    chars.next(); // consume '*'
                    // Block comment, nesting-aware: `/* ... /* ... */ ... */`.
                    let mut depth = 1usize;
                    while depth > 0 {
                        match chars.next() {
                            Some((_, '/')) if matches!(chars.peek(), Some(&(_, '*'))) => {
                                chars.next();
                                depth += 1;
                            }
                            Some((_, '*')) if matches!(chars.peek(), Some(&(_, '/'))) => {
                                chars.next();
                                depth -= 1;
                            }
                            Some(_) => {}
                            None => break,
                        }
                    }
                    continue;
                }
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("/".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Slash, span: Span { start, end: start + 1 } });
                }
            }
            '%' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("%".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Percent, span: Span { start, end: start + 1 } });
                }
            }
            '!' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::Ne,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Bang,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '|' => {
                if let Some(&(idx, '|')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::PipePipe, span: Span { start, end: idx + 1 } });
                } else if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("|".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Pipe, span: Span { start, end: start + 1 } });
                }
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
            ':' => {
                if let Some(&(idx, ':')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::ColonColon,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Colon,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
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
            '.' => {
                if let Some(&(idx, '.')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::DotDot,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Dot,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '*' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("*".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Star, span: Span { start, end: start + 1 } });
                }
            }
            '&' => {
                if let Some(&(idx, '&')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::AmpAmp, span: Span { start, end: idx + 1 } });
                } else if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("&".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Amp, span: Span { start, end: start + 1 } });
                }
            }
            '<' => {
                if let Some(&(_, '<')) = chars.peek() {
                    let (lidx, _) = chars.next().unwrap();
                    if let Some(&(idx, '=')) = chars.peek() {
                        chars.next();
                        tokens.push(SpannedToken { token: Token::OpAssign("<<".into()), span: Span { start, end: idx + 1 } });
                    } else {
                        tokens.push(SpannedToken { token: Token::Shl, span: Span { start, end: lidx + 1 } });
                    }
                } else if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::Le, span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Lt, span: Span { start, end: start + 1 } });
                }
            }
            '>' => {
                if let Some(&(_, '>')) = chars.peek() {
                    let (ridx, _) = chars.next().unwrap();
                    if let Some(&(idx, '=')) = chars.peek() {
                        chars.next();
                        tokens.push(SpannedToken { token: Token::OpAssign(">>".into()), span: Span { start, end: idx + 1 } });
                    } else {
                        tokens.push(SpannedToken { token: Token::Shr, span: Span { start, end: ridx + 1 } });
                    }
                } else if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::Ge, span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Gt, span: Span { start, end: start + 1 } });
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
                } else if let Some(&(idx, '>')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::FatArrow,
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
            '+' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("+".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Plus, span: Span { start, end: start + 1 } });
                }
            }
            '-' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("-".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Minus, span: Span { start, end: start + 1 } });
                }
            }
            '^' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken { token: Token::OpAssign("^".into()), span: Span { start, end: idx + 1 } });
                } else {
                    tokens.push(SpannedToken { token: Token::Caret, span: Span { start, end: start + 1 } });
                }
            }
            '~' => tokens.push(SpannedToken { token: Token::Tilde, span: Span { start, end: start + 1 } }),
            '"' => {
                let mut s = String::new();
                let mut end = start + 1;
                while let Some(&(idx, nc)) = chars.peek() {
                    chars.next();
                    end = idx + nc.len_utf8();
                    if nc == '"' {
                        break;
                    }
                    if nc == '\\' {
                        if let Some(&(eidx, ec)) = chars.peek() {
                            chars.next();
                            end = eidx + ec.len_utf8();
                            lex_escape(ec, &mut s, &mut chars, &mut end);
                            continue;
                        }
                    }
                    s.push(nc);
                }
                tokens.push(SpannedToken {
                    token: Token::StringLit(s),
                    span: Span { start, end },
                });
            }
            '\'' => {
                // Character literal → one-character string (with escape support).
                let mut s = String::new();
                let mut end = start + 1;
                if let Some(&(idx, nc)) = chars.peek() {
                    if nc != '\'' {
                        chars.next();
                        end = idx + nc.len_utf8();
                        if nc == '\\' {
                            if let Some(&(eidx, ec)) = chars.peek() {
                                chars.next();
                                end = eidx + ec.len_utf8();
                                lex_escape(ec, &mut s, &mut chars, &mut end);
                            }
                        } else {
                            s.push(nc);
                        }
                    }
                }
                if let Some(&(idx, '\'')) = chars.peek() {
                    chars.next();
                    end = idx + 1;
                }
                tokens.push(SpannedToken {
                    token: Token::StringLit(s),
                    span: Span { start, end },
                });
            }
            c if c.is_ascii_digit() => {
                let mut end = start + c.len_utf8();
                // Radix prefixes `0x`/`0b`/`0o` decode to a decimal integer string, so downstream
                // integer parsing is uniform.
                if c == '0' {
                    let radix = match chars.peek() {
                        Some(&(_, 'x')) | Some(&(_, 'X')) => Some(16u32),
                        Some(&(_, 'b')) | Some(&(_, 'B')) => Some(2u32),
                        Some(&(_, 'o')) | Some(&(_, 'O')) => Some(8u32),
                        _ => None,
                    };
                    if let Some(radix) = radix {
                        let (pidx, pch) = chars.next().unwrap();
                        end = pidx + pch.len_utf8();
                        let mut digits = String::new();
                        while let Some(&(idx, nc)) = chars.peek() {
                            if nc == '_' {
                                chars.next();
                                end = idx + 1;
                            } else if nc.is_digit(radix) {
                                chars.next();
                                end = idx + nc.len_utf8();
                                digits.push(nc);
                            } else {
                                break;
                            }
                        }
                        let decimal = i64::from_str_radix(&digits, radix)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "0".to_string());
                        tokens.push(SpannedToken {
                            token: Token::Number(decimal),
                            span: Span { start, end },
                        });
                        continue;
                    }
                }
                let mut num = c.to_string();
                // Integer part (digits + `_` separators).
                while let Some(&(idx, nc)) = chars.peek() {
                    if nc.is_ascii_digit() || nc == '_' {
                        chars.next();
                        if nc != '_' {
                            num.push(nc);
                        }
                        end = idx + nc.len_utf8();
                    } else {
                        break;
                    }
                }
                // Fractional part: a `.` followed by a digit (so `1.5` is a float but `1..5`
                // stays a range and `1.foo` stays a field access).
                if matches!(chars.peek(), Some(&(_, '.'))) {
                    let mut la = chars.clone();
                    la.next();
                    if matches!(la.peek(), Some(&(_, d)) if d.is_ascii_digit()) {
                        let (didx, _) = chars.next().unwrap();
                        num.push('.');
                        end = didx + 1;
                        while let Some(&(idx, nc)) = chars.peek() {
                            if nc.is_ascii_digit() || nc == '_' {
                                chars.next();
                                if nc != '_' {
                                    num.push(nc);
                                }
                                end = idx + nc.len_utf8();
                            } else {
                                break;
                            }
                        }
                    }
                }
                // Exponent part: `e`/`E` with optional sign then digits (`1e9`, `1.5e-3`).
                if matches!(chars.peek(), Some(&(_, 'e')) | Some(&(_, 'E'))) {
                    let mut la = chars.clone();
                    la.next();
                    let has_exp = match la.peek() {
                        Some(&(_, d)) if d.is_ascii_digit() => true,
                        Some(&(_, '+')) | Some(&(_, '-')) => {
                            la.next();
                            matches!(la.peek(), Some(&(_, d)) if d.is_ascii_digit())
                        }
                        _ => false,
                    };
                    if has_exp {
                        let (eidx, ec) = chars.next().unwrap();
                        num.push(ec);
                        end = eidx + ec.len_utf8();
                        if matches!(chars.peek(), Some(&(_, '+')) | Some(&(_, '-'))) {
                            let (sidx, sc) = chars.next().unwrap();
                            num.push(sc);
                            end = sidx + sc.len_utf8();
                        }
                        while let Some(&(idx, nc)) = chars.peek() {
                            if nc.is_ascii_digit() || nc == '_' {
                                chars.next();
                                if nc != '_' {
                                    num.push(nc);
                                }
                                end = idx + nc.len_utf8();
                            } else {
                                break;
                            }
                        }
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
                    | "enum" | "match"
                    | "return" | "as" | "while" | "loop" | "break" | "continue" | "mut" | "for"
                    | "in" => Token::Keyword(id),
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

/// Decode one escape sequence (the char after `\`) into `out`. Consumes extra chars for
/// `\xNN` and `\u{...}`. Unknown escapes pass the char through verbatim.
fn lex_escape<I>(ec: char, out: &mut String, chars: &mut std::iter::Peekable<I>, end: &mut usize)
where
    I: Iterator<Item = (usize, char)>,
{
    match ec {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        '0' => out.push('\0'),
        '\\' => out.push('\\'),
        '"' => out.push('"'),
        '\'' => out.push('\''),
        'x' => {
            let mut hex = String::new();
            for _ in 0..2 {
                if let Some(&(hidx, hc)) = chars.peek() {
                    if hc.is_ascii_hexdigit() {
                        chars.next();
                        *end = hidx + hc.len_utf8();
                        hex.push(hc);
                    }
                }
            }
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push(byte as char);
            }
        }
        'u' => {
            if let Some(&(_, '{')) = chars.peek() {
                chars.next();
                let mut hex = String::new();
                while let Some(&(hidx, hc)) = chars.peek() {
                    chars.next();
                    *end = hidx + hc.len_utf8();
                    if hc == '}' {
                        break;
                    }
                    hex.push(hc);
                }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if let Some(uc) = char::from_u32(cp) {
                        out.push(uc);
                    }
                }
            }
        }
        other => out.push(other),
    }
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
    /// When true, a bare `Name {` is NOT a struct literal — the `{` starts a block.
    /// Set while parsing `if`/`while`/`for` header expressions to resolve the classic
    /// `for i in 0..n {` ambiguity (Rust does the same). Reset inside `()`/`[]`/call args.
    no_struct: bool,
}

impl Parser {
    fn new(tokens: Vec<SpannedToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: vec![],
            no_struct: false,
        }
    }

    /// Parse an expression in header position (loop/if condition, for-range bound), where a
    /// trailing `{` must be read as a block, not a struct literal.
    fn parse_header_expr(&mut self) -> Expr {
        let prev = self.no_struct;
        self.no_struct = true;
        let e = self.parse_expr(0);
        self.no_struct = prev;
        e
    }

    /// Run a parse closure with struct literals re-enabled (inside a delimited `()`/`[]`/args).
    fn with_struct_allowed<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let prev = self.no_struct;
        self.no_struct = false;
        let r = f(self);
        self.no_struct = prev;
        r
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
            } else if self.check_keyword("enum") {
                if let Some(item) = self.parse_enum() {
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

    fn parse_match_expr(&mut self, start: Span) -> Expr {
        let scrutinee = self.with_struct_allowed(|p| p.parse_header_expr());
        let _ = self.expect_token(Token::LBrace, "expected `{` after match scrutinee");
        let mut arms = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            let pattern = self.parse_pattern();
            let _ = self.expect_token(Token::FatArrow, "expected `=>` after match pattern");
            let body = self.with_struct_allowed(|p| p.parse_expr(0));
            arms.push(MatchArm { pattern, body });
            if self.check_token(&Token::Comma) {
                self.bump();
            }
        }
        let end = if self.check_token(&Token::RBrace) {
            self.bump().map(|t| t.span.end).unwrap_or(start.end)
        } else {
            self.diagnostic("expected `}` after match arms", self.current_span());
            self.current_span().end
        };
        Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: Span {
                start: start.start,
                end,
            },
        }
    }

    fn parse_pattern(&mut self) -> Pattern {
        if let Token::Ident(name) = &self.current().token {
            if name == "_" {
                self.bump();
                return Pattern::Wildcard;
            }
        }
        // Enum::Variant / Enum::Variant(a) / Enum::Variant { f: b }
        if matches!(self.current().token, Token::Ident(_)) {
            let (enum_name, _) = self.expect_ident("expected enum name in pattern").unwrap();
            if self.check_token(&Token::ColonColon) {
                self.bump();
                let (variant, _) = self
                    .expect_ident("expected variant in pattern")
                    .unwrap_or_else(|| ("_".into(), Span::default()));
                let mut bindings = vec![];
                let mut named_bindings = vec![];
                if self.check_token(&Token::LParen) {
                    self.bump();
                    while !self.at_eof() && !self.check_token(&Token::RParen) {
                        if let Some((b, _)) = self.expect_ident("expected binding") {
                            bindings.push(b);
                        } else {
                            self.bump();
                        }
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RParen, "expected `)` in pattern");
                } else if self.check_token(&Token::LBrace) {
                    self.bump();
                    while !self.at_eof() && !self.check_token(&Token::RBrace) {
                        if let Some((fname, _)) = self.expect_ident("expected field in pattern") {
                            let _ = self.expect_token(Token::Colon, "expected `:` in struct pattern");
                            let (bname, _) = self
                                .expect_ident("expected binding after field")
                                .unwrap_or_else(|| (fname.clone(), Span::default()));
                            named_bindings.push((fname, bname));
                        } else {
                            self.bump();
                        }
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RBrace, "expected `}` in struct pattern");
                }
                return Pattern::EnumVariant {
                    enum_name,
                    variant,
                    bindings,
                    named_bindings,
                };
            }
            self.diagnostic(
                "match pattern must be `Enum::Variant` or `_`",
                self.current_span(),
            );
            return Pattern::Wildcard;
        }
        self.diagnostic("expected match pattern", self.current_span());
        self.bump();
        Pattern::Wildcard
    }

    fn parse_enum(&mut self) -> Option<Item> {
        let start = self.expect_keyword("enum")?.span;
        let (name, _) = self.expect_ident("expected enum name")?;
        let _ = self.expect_token(Token::LBrace, "expected `{` after enum name");
        let mut variants = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            if let Some((vname, _)) = self.expect_ident("expected variant name") {
                let kind = if self.check_token(&Token::LParen) {
                    self.bump();
                    let mut fields = vec![];
                    while !self.at_eof() && !self.check_token(&Token::RParen) {
                        let fty =
                            self.collect_type_until(&[Token::Comma, Token::RParen, Token::Semi]);
                        if !fty.is_empty() {
                            fields.push(fty);
                        }
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RParen, "expected `)` after variant fields");
                    EnumVariantKind::Tuple(fields)
                } else if self.check_token(&Token::LBrace) {
                    self.bump();
                    let mut fields = vec![];
                    while !self.at_eof() && !self.check_token(&Token::RBrace) {
                        if let Some((fname, _)) = self.expect_ident("expected field name") {
                            let _ = self.expect_token(Token::Colon, "expected `:` after field");
                            let fty = self
                                .collect_type_until(&[Token::Comma, Token::RBrace, Token::Semi]);
                            fields.push((fname, fty));
                        } else {
                            self.bump();
                        }
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RBrace, "expected `}` after struct variant");
                    EnumVariantKind::Struct(fields)
                } else {
                    EnumVariantKind::Unit
                };
                variants.push(EnumVariant {
                    name: vname,
                    kind,
                });
            } else {
                self.bump();
            }
            if self.check_token(&Token::Comma) || self.check_token(&Token::Semi) {
                self.bump();
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}` after enum");
        let end = self.previous_end();
        Some(Item::Enum {
            name,
            variants,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    /// Expression block body: `{ stmt* tail? }`. Statements execute for effect; a trailing
    /// expression with no semicolon (if present) is the block's value. Used for the branches of
    /// an `if` expression, so `if c { let t = a + b; t * 2 } else { 0 }` works.
    fn parse_expr_block(&mut self) -> Expr {
        let _ = self.expect_token(Token::LBrace, "expected `{`");
        let mut stmts: Vec<Stmt> = Vec::new();
        let mut tail: Option<Box<Expr>> = None;
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            // Statement-introducing keywords are always statements.
            if self.check_keyword("let")
                || self.check_keyword("while")
                || self.check_keyword("loop")
                || self.check_keyword("for")
                || self.check_keyword("return")
                || self.check_keyword("break")
                || self.check_keyword("continue")
                || self.check_keyword("research")
                || self.check_keyword("exploit")
                || self.check_keyword("hybrid")
                || self.check_keyword("spec")
                || self.check_keyword("assume")
                || self.check_keyword("assert")
            {
                if let Some(s) = self.parse_stmt() {
                    stmts.push(s);
                } else {
                    self.bump();
                }
                continue;
            }
            // Otherwise parse an expression (covers `if`/`match`/calls/values).
            let e = self.with_struct_allowed(|p| p.parse_expr(0));
            if self.check_token(&Token::Eq) {
                self.bump();
                let value = self.with_struct_allowed(|p| p.parse_expr(0));
                self.consume_optional_semi();
                stmts.push(Stmt::Assign { target: e, value });
            } else if let Token::OpAssign(op) = &self.current().token {
                let op = op.clone();
                self.bump();
                let rhs = self.with_struct_allowed(|p| p.parse_expr(0));
                self.consume_optional_semi();
                stmts.push(Stmt::Assign {
                    target: e.clone(),
                    value: Expr::Binary {
                        op,
                        lhs: Box::new(e),
                        rhs: Box::new(rhs),
                    },
                });
            } else if self.check_token(&Token::Semi) {
                self.bump();
                stmts.push(Stmt::ExprStmt(e));
            } else {
                // No trailing separator → this expression is the block's value.
                tail = Some(Box::new(e));
                break;
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}`");
        if stmts.is_empty() {
            match tail {
                Some(t) => *t,
                None => Expr::Literal("0".into()),
            }
        } else {
            Expr::Block { stmts, tail }
        }
    }

    fn parse_if_expr(&mut self, start: Span) -> Expr {
        let cond = self.parse_header_expr();
        let then = self.parse_expr_block();
        let else_ = if self.check_keyword("else") {
            self.bump();
            if self.check_keyword("if") {
                let tok = self.bump().unwrap();
                self.parse_if_expr(tok.span)
            } else {
                self.parse_expr_block()
            }
        } else {
            self.diagnostic("if-expression requires `else`", self.current_span());
            Expr::Literal("0".into())
        };
        Expr::If {
            cond: Box::new(cond),
            then: Box::new(then),
            else_: Box::new(else_),
            span: Span {
                start: start.start,
                end: self.previous_end().max(start.end),
            },
        }
    }

    fn parse_map_literal(&mut self, start: Span) -> Expr {
        // `{` already consumed by caller
        let mut entries = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            let key = self.with_struct_allowed(|p| p.parse_expr(0));
            let _ = self.expect_token(Token::Colon, "expected `:` in map entry");
            let val = self.with_struct_allowed(|p| p.parse_expr(0));
            entries.push((key, val));
            if self.check_token(&Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        let end = if self.check_token(&Token::RBrace) {
            self.bump().map(|t| t.span.end).unwrap_or(start.end)
        } else {
            self.diagnostic("expected `}` after map literal", self.current_span());
            self.current_span().end
        };
        Expr::MapLiteral {
            entries,
            span: Span {
                start: start.start,
                end,
            },
        }
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
            } else if self.check_keyword("struct") {
                if let Some(item) = self.parse_struct() {
                    items.push(item);
                }
            } else if self.check_keyword("enum") {
                if let Some(item) = self.parse_enum() {
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
        // Optional return type: `-> Type` (lexed as Minus Gt then the type). Captured in the AST
        // for tooling/typecheck even though the runtime is dynamically typed.
        let mut ret: Option<String> = None;
        if self.check_token(&Token::Minus) {
            let _ = self.bump();
            if self.check_token(&Token::Gt) {
                let _ = self.bump();
            }
            let ty = self.collect_type_until(&[Token::LBrace, Token::Semi]);
            if !ty.is_empty() {
                ret = Some(ty);
            }
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
            ret,
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
            // Parameter type annotations are optional — Anubis values are dynamically typed at
            // runtime, so `fn f(n)` and `fn f(n: u32)` are both accepted.
            let mut ty = String::new();
            if self.check_token(&Token::Colon) {
                self.bump();
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
        if self.check_keyword("while") {
            self.bump();
            let cond = self.parse_header_expr();
            let body = if self.check_token(&Token::LBrace) {
                self.parse_block()
            } else {
                self.diagnostic("expected `{` after while cond", self.current_span());
                vec![]
            };
            return Some(Stmt::While { cond, body });
        }
        if self.check_keyword("loop") {
            self.bump();
            let body = if self.check_token(&Token::LBrace) {
                self.parse_block()
            } else {
                self.diagnostic("expected `{` after loop", self.current_span());
                vec![]
            };
            return Some(Stmt::Loop { body });
        }
        if self.check_keyword("for") {
            self.bump();
            let (var, _) = self.expect_ident("expected loop variable after `for`")?;
            let _ = self.expect_keyword("in");
            // Range: `a..b`  |  Collection: any other expression
            let first = self.parse_header_expr();
            let source = if self.check_token(&Token::DotDot) {
                self.bump();
                let end = self.parse_header_expr();
                ForSource::Range {
                    start: first,
                    end,
                }
            } else {
                ForSource::Collection { expr: first }
            };
            let body = if self.check_token(&Token::LBrace) {
                self.parse_block()
            } else {
                self.diagnostic("expected `{` after for-loop header", self.current_span());
                vec![]
            };
            return Some(Stmt::For {
                var,
                source,
                body,
            });
        }
        if self.check_keyword("break") {
            self.bump();
            self.consume_optional_semi();
            return Some(Stmt::Break);
        }
        if self.check_keyword("continue") {
            self.bump();
            self.consume_optional_semi();
            return Some(Stmt::Continue);
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
            // Assignment: `lvalue = expr;` (mutation of an existing binding or place).
            if self.check_token(&Token::Eq) {
                self.bump();
                let value = self.parse_expr(0);
                self.consume_optional_semi();
                return Some(Stmt::Assign {
                    target: expr,
                    value,
                });
            }
            // Compound assignment `place op= expr` desugars to `place = place op expr`.
            if let Token::OpAssign(op) = &self.current().token {
                let op = op.clone();
                self.bump();
                let rhs = self.parse_expr(0);
                self.consume_optional_semi();
                let value = Expr::Binary {
                    op,
                    lhs: Box::new(expr.clone()),
                    rhs: Box::new(rhs),
                };
                return Some(Stmt::Assign {
                    target: expr,
                    value,
                });
            }
            self.consume_optional_semi();
            return Some(Stmt::ExprStmt(expr));
        }
        self.diagnostic("expected statement", self.current_span());
        None
    }

    fn parse_let(&mut self) -> Option<Stmt> {
        let start = self.expect_keyword("let")?.span;
        // Optional `mut` — all Anubis bindings are reassignable, so `mut` is accepted but not required.
        if self.check_keyword("mut") {
            self.bump();
        }
        let (name, _) = self.expect_ident("expected binding name after `let`")?;
        let ty = if self.check_token(&Token::Colon) {
            self.bump();
            let ty = self.collect_type_until(&[Token::Eq, Token::Semi, Token::RBrace]);
            if ty.is_empty() {
                None
            } else {
                // Preserve the annotation verbatim, including the inner type of `tainted<T>`
                // (e.g. `tainted<u8>` keeps its 8-bit width for the solver).
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
        let cond = self.parse_header_expr();
        let then = if self.check_token(&Token::LBrace) {
            self.parse_block()
        } else {
            self.diagnostic("expected `{` after if cond", self.current_span());
            vec![]
        };
        let mut else_ = None;
        if self.check_keyword("else") {
            let _ = self.bump();
            if self.check_keyword("if") {
                // `else if ...` desugars to an else-block containing a single nested if.
                else_ = self.parse_if_stmt().map(|nested| vec![nested]);
            } else {
                else_ = Some(if self.check_token(&Token::LBrace) {
                    self.parse_block()
                } else {
                    self.diagnostic("expected `{` after else", self.current_span());
                    vec![]
                });
            }
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

    /// Parse a lambda literal: `|p1, p2| body` or `|| body`. `body` is a single expression, which
    /// may be a block `{ ... }`.
    fn parse_lambda(&mut self) -> Expr {
        let mut params = Vec::new();
        if self.check_token(&Token::PipePipe) {
            self.bump(); // `||` — no parameters
        } else {
            self.bump(); // opening `|`
            while !self.at_eof() && !self.check_token(&Token::Pipe) {
                let Some((p, _)) = self.expect_ident("expected lambda parameter") else {
                    break;
                };
                params.push(p);
                // Optional `: type` annotation (ignored — dynamically typed).
                if self.check_token(&Token::Colon) {
                    self.bump();
                    let _ = self.collect_type_until(&[Token::Comma, Token::Pipe]);
                }
                if self.check_token(&Token::Comma) {
                    self.bump();
                }
            }
            let _ = self.expect_token(Token::Pipe, "expected `|` after lambda parameters");
        }
        let body = if self.check_token(&Token::LBrace) {
            self.parse_expr_block()
        } else {
            self.with_struct_allowed(|p| p.parse_expr(0))
        };
        Expr::Lambda {
            params,
            body: Box::new(body),
        }
    }

    fn parse_primary(&mut self) -> Expr {
        // Prefix unary operators: `-expr` (negation) and `!expr` (logical not).
        if self.check_token(&Token::Minus) {
            self.bump();
            let inner = self.parse_primary();
            return Expr::Unary {
                op: "-".into(),
                expr: Box::new(inner),
            };
        }
        if self.check_token(&Token::Bang) {
            self.bump();
            let inner = self.parse_primary();
            return Expr::Unary {
                op: "!".into(),
                expr: Box::new(inner),
            };
        }
        if self.check_token(&Token::Tilde) {
            self.bump();
            let inner = self.parse_primary();
            return Expr::Unary {
                op: "~".into(),
                expr: Box::new(inner),
            };
        }
        // Lambda literal in primary position: `|params| body` or `|| body`.
        if self.check_token(&Token::Pipe) || self.check_token(&Token::PipePipe) {
            return self.parse_lambda();
        }
        let Some(tok) = self.bump() else {
            return Expr::Other("eof".into());
        };
        let primary = match tok.token {
            Token::Number(n) => Expr::Literal(n),
            Token::StringLit(s) => Expr::StrLiteral(s),
            Token::Ident(name) => {
                // Enum construct: Status::Ok / Status::Err(a) / Status::Err { code: a }
                if self.check_token(&Token::ColonColon) {
                    self.bump();
                    let (variant, _) = self
                        .expect_ident("expected variant after `::`")
                        .unwrap_or_else(|| ("_".into(), tok.span));
                    let (fields, field_names) = if self.check_token(&Token::LParen) {
                        (self.parse_call_args(), vec![])
                    } else if self.check_token(&Token::LBrace) {
                        self.bump();
                        let mut names = vec![];
                        let mut vals = vec![];
                        while !self.at_eof() && !self.check_token(&Token::RBrace) {
                            if let Some((fname, _)) =
                                self.expect_ident("expected field in enum construct")
                            {
                                let _ = self
                                    .expect_token(Token::Colon, "expected `:` in enum construct");
                                let val = self.with_struct_allowed(|p| p.parse_expr(0));
                                names.push(fname);
                                vals.push(val);
                            } else {
                                self.bump();
                            }
                            if self.check_token(&Token::Comma) {
                                self.bump();
                            }
                        }
                        let _ = self.expect_token(Token::RBrace, "expected `}` in enum construct");
                        (vals, names)
                    } else {
                        (vec![], vec![])
                    };
                    Expr::EnumConstruct {
                        enum_name: name.clone(),
                        variant,
                        fields,
                        field_names,
                        span: tok.span,
                    }
                } else if self.check_token(&Token::LParen) {
                    let args = self.parse_call_args();
                    Expr::Call {
                        callee: name.clone(),
                        args,
                    }
                } else if !self.no_struct && self.check_token(&Token::LBrace) {
                    // struct literal: Name { f: e, ... }
                    self.bump();
                    let mut fields = vec![];
                    while !self.at_eof() && !self.check_token(&Token::RBrace) {
                        if let Some((fname, _)) = self.expect_ident("expected field in struct lit")
                        {
                            let _ = self.expect_token(Token::Colon, "expected : in struct lit");
                            let val = self.with_struct_allowed(|p| p.parse_expr(0));
                            fields.push((fname, Box::new(val)));
                        } else {
                            // Do not spin on an unexpected token; advance to make progress.
                            self.bump();
                        }
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RBrace, "expected } in struct lit");
                    Expr::StructLiteral {
                        name: name.clone(),
                        fields,
                        span: tok.span,
                    }
                } else {
                    Expr::Var(name.clone())
                }
            }
            Token::Keyword(k) if k == "true" || k == "false" => Expr::Literal(k),
            Token::Keyword(k) if k == "match" => self.parse_match_expr(tok.span),
            Token::Keyword(k) if k == "if" => self.parse_if_expr(tok.span),
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
                        Expr::Literal(s) | Expr::StrLiteral(s) | Expr::Var(s) => s.clone(),
                        _ => "unknown".to_string(),
                    }
                };
                Expr::TaintSource { label }
            }
            // (taint_source handled via Keyword above; duplicate Ident arm removed to satisfy -D warnings)
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
                                    if let Expr::Literal(s) | Expr::StrLiteral(s) | Expr::Var(s) =
                                        &args[i]
                                    {
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
                        } else if let Expr::Literal(s) | Expr::StrLiteral(s) | Expr::Var(s) =
                            &args[i]
                        {
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
                let expr = self.with_struct_allowed(|p| p.parse_expr(0));
                let _ = self.expect_token(Token::RParen, "expected `)` after expression");
                expr
            }
            Token::LBracket => {
                let mut elements = vec![];
                while !self.at_eof() && !self.check_token(&Token::RBracket) {
                    elements.push(self.with_struct_allowed(|p| p.parse_expr(0)));
                    if self.check_token(&Token::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let _ = self.expect_token(Token::RBracket, "expected `]` after array literal");
                Expr::ArrayLiteral { elements }
            }
            Token::LBrace => self.parse_map_literal(tok.span),
            other => Expr::Other(format!("{:?}", other)),
        };
        // Unified postfix chain: `.field` and `[index]` interleaved and repeated, so
        // `a[i].b`, `a.b[i]`, `a.b.c[i].d`, and `foo().bar[0]` all parse.
        let mut e = primary;
        loop {
            if self.check_token(&Token::Dot) {
                self.bump();
                if let Some((field, fspan)) = self.expect_ident("expected field name after `.`") {
                    e = Expr::FieldAccess {
                        base: Box::new(e),
                        field,
                        span: tok.span.merge(fspan),
                    };
                } else {
                    break;
                }
            } else if self.check_token(&Token::LBracket) {
                self.bump();
                let index = self.with_struct_allowed(|p| p.parse_expr(0));
                let _ = self.expect_token(Token::RBracket, "expected `]` after index");
                e = Expr::Index {
                    base: Box::new(e),
                    index: Box::new(index),
                };
            } else if self.check_token(&Token::LParen) {
                // Application of a callee expression: `expr(args)` — e.g. `obj.f(x)`, `arr[i](x)`,
                // `f(a)(b)`.
                let args = self.parse_call_args();
                e = Expr::CallExpr {
                    callee: Box::new(e),
                    args,
                };
            } else {
                break;
            }
        }
        e
    }

    fn parse_call_args(&mut self) -> Vec<Expr> {
        let mut args = vec![];
        let _ = self.expect_token(Token::LParen, "expected `(` for call");
        while !self.at_eof() && !self.check_token(&Token::RParen) {
            args.push(self.with_struct_allowed(|p| p.parse_expr(0)));
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
        // Turbofish: `::<T>` — either ColonColon or legacy Colon+Colon.
        if self.check_token(&Token::ColonColon) {
            self.bump();
        } else if self.check_token(&Token::Colon)
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
        // Precedence ladder (higher binds tighter), C-like:
        // || < && < | < ^ < & < (== !=  < <= > >=) < (<< >>) < (+ -) < (* / %)
        match &self.current().token {
            Token::PipePipe => Some(("||".into(), 4)),
            Token::AmpAmp => Some(("&&".into(), 5)),
            Token::Pipe => Some(("|".into(), 6)),
            Token::Caret => Some(("^".into(), 7)),
            Token::Amp => Some(("&".into(), 8)),
            Token::Lt => Some(("<".into(), 10)),
            Token::Gt => Some((">".into(), 10)),
            Token::Le => Some(("<=".into(), 10)),
            Token::Ge => Some((">=".into(), 10)),
            Token::EqEq => Some(("==".into(), 10)),
            Token::Ne => Some(("!=".into(), 10)),
            Token::Shl => Some(("<<".into(), 18)),
            Token::Shr => Some((">>".into(), 18)),
            Token::Plus => Some(("+".into(), 20)),
            Token::Minus => Some(("-".into(), 20)),
            Token::Star => Some(("*".into(), 30)),
            Token::Slash => Some(("/".into(), 30)),
            Token::Percent => Some(("%".into(), 30)),
            _ => None,
        }
    }

    fn starts_expr(&self) -> bool {
        match &self.current().token {
            Token::Ident(_)
            | Token::Number(_)
            | Token::StringLit(_)
            | Token::LParen
            | Token::LBracket
            | Token::Minus
            | Token::Tilde
            | Token::Pipe
            | Token::PipePipe
            | Token::Bang => true,
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
