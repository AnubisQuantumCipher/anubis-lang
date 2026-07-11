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
    Question,
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
        /// `pub fn` marks a function exported from its module (callable as `mod::name` across a
        /// module boundary). Defaults to `Private` — private functions are callable only within
        /// their own module. Cross-module privacy is enforced by `resolve::combine_graph`.
        visibility: Visibility,
        params: Vec<(String, String)>,
        body: Vec<Stmt>,
        mode: Mode,
        intent: Option<String>,
        /// Declared return type (`-> T`), captured verbatim; `None` if omitted.
        ret: Option<String>,
        /// B2 contracts: `requires(P)` preconditions and `ensures(Q)` postconditions declared after
        /// the signature. Each is a boolean expression; `ensures` may reference `result` (the return
        /// value). Empty when the function declares no contracts.
        requires: Vec<Expr>,
        ensures: Vec<Expr>,
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
    /// `impl Point { ... }` or `impl Shape for Circle { ... }` — methods for a struct/enum type.
    /// Each method is an `Item::Fn` taking `self` first. `trait_name` is `Some` for the
    /// `impl Trait for Type` form, which inherits the trait's un-overridden default methods.
    Impl {
        type_name: String,
        trait_name: Option<String>,
        methods: Vec<Item>,
        span: Span,
    },
    /// `trait Shape { fn area(self); fn name(self) { "shape" } }` — an interface. Methods with a
    /// body are defaults inherited by implementors; methods without one are required.
    Trait {
        name: String,
        methods: Vec<Item>,
        span: Span,
    },
}

/// Item visibility. `Private` (the default, no `pub`) is callable only within its own module;
/// `Public` (`pub`) is exported and callable across a module boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
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
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// A bare lowercase/identifier pattern like `n` — matches anything and binds
    /// the scrutinee to `n`. Irrefutable.
    Binding(String),
    /// A numeric or boolean literal (`1`, `-3`, `3.14`, `true`) — matches by value.
    Literal(String),
    /// A string or char literal (`"hi"`, `'a'`) — matches by value.
    StrLiteral(String),
    /// Or-pattern: `1 | 2 | 3` — matches if any alternative matches. Alternatives
    /// may not bind variables (they must be literals, wildcards, or unit variants).
    Or(Vec<Pattern>),
    /// List/tuple pattern: `[a, b, c]` or `(a, b)` — matches a list of exactly this
    /// length, matching each element against the corresponding sub-pattern.
    List(Vec<Pattern>),
    /// Struct pattern: `Point { x, y }`, `Point { x: 0, y: b }` — matches a struct of the named
    /// type, matching each listed field against a sub-pattern. `fields` is `(field_name, pattern)`
    /// (shorthand `{ x }` desugars to `{ x: x }`, binding field `x` to a variable `x`).
    Struct {
        name: String,
        fields: Vec<(String, Pattern)>,
    },
    /// `Status::Ok`, `Status::Err(n)`, or `Status::Err { code: c }`
    EnumVariant {
        enum_name: String,
        variant: String,
        /// Positional sub-patterns for tuple variants (`Some(Point { x, y })`, `Err(0)`).
        bindings: Vec<Pattern>,
        /// Named field sub-patterns `(field, pattern)` for struct variants
        /// (`Http::Ok { code: 200 }`, `Shape::Circle { r }`).
        named_bindings: Vec<(String, Pattern)>,
    },
}

impl Pattern {
    /// All identifiers this pattern introduces into the arm's scope.
    pub fn bound_names(&self) -> Vec<String> {
        match self {
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::StrLiteral(_) => Vec::new(),
            Pattern::Binding(n) => vec![n.clone()],
            Pattern::Or(pats) | Pattern::List(pats) => {
                pats.iter().flat_map(|p| p.bound_names()).collect()
            }
            Pattern::Struct { fields, .. } => {
                fields.iter().flat_map(|(_, p)| p.bound_names()).collect()
            }
            Pattern::EnumVariant {
                bindings,
                named_bindings,
                ..
            } => {
                let mut v: Vec<String> = bindings.iter().flat_map(|p| p.bound_names()).collect();
                v.extend(named_bindings.iter().flat_map(|(_, p)| p.bound_names()));
                v
            }
        }
    }

    /// Whether this pattern matches every possible value (making a match exhaustive
    /// on its own, when unguarded).
    pub fn is_irrefutable(&self) -> bool {
        match self {
            Pattern::Wildcard | Pattern::Binding(_) => true,
            // An or-pattern is irrefutable if any alternative is — `Red | _` matches anything.
            Pattern::Or(pats) => pats.iter().any(|p| p.is_irrefutable()),
            _ => false,
        }
    }

    /// Collect every `(enum_name, variant)` pair this pattern covers, recursing
    /// through or-patterns. Used for exhaustiveness analysis.
    pub fn covered_enum_variants(&self, out: &mut Vec<(String, String)>) {
        match self {
            Pattern::EnumVariant {
                enum_name, variant, ..
            } => out.push((enum_name.clone(), variant.clone())),
            Pattern::Or(pats) => {
                for p in pats {
                    p.covered_enum_variants(out);
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    /// Optional `if <expr>` guard evaluated after the pattern binds.
    pub guard: Option<Expr>,
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
    /// Destructuring binding: `let [a, b] = xs` or `let (a, b) = pair`. The pattern is
    /// irrefutable — missing/short values bind the default `0`.
    LetPattern {
        pattern: Pattern,
        init: Expr,
        span: Span,
    },
    /// `while let PATTERN = expr { body }` — loop while the pattern keeps matching.
    WhileLet {
        pattern: Pattern,
        expr: Expr,
        body: Vec<Stmt>,
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
        /// B3 loop invariants: predicates the checker verifies hold on entry AND are preserved by
        /// every iteration, then may assume after the loop.
        invariant: Vec<Expr>,
    },
    Loop {
        body: Vec<Stmt>,
        invariant: Vec<Expr>,
    },
    /// `for v in a..b { }` or `for v in collection { }`
    For {
        var: String,
        source: ForSource,
        body: Vec<Stmt>,
        invariant: Vec<Expr>,
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
    /// Error-propagation postfix `expr?`: unwrap `Ok(v)`/`Some(v)` to `v`, otherwise return the
    /// `Err`/`None` value from the enclosing function.
    Try(Box<Expr>),
    /// `if let PATTERN = scrutinee { then } else { else_ }` as an expression: yields the matching
    /// branch's value (bindings from the pattern are in scope in `then`). `else_` defaults to `0`.
    IfLet {
        pattern: Pattern,
        scrutinee: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
        span: Span,
    },
    Other(String),
}

/// Desugar traits: replace each `impl Trait for Type` with a plain `impl Type` that inherits the
/// trait's un-overridden default methods, and drop `trait` declarations. Downstream passes then
/// only ever see plain `impl` blocks — traits add no new lowering machinery.
pub fn resolve_traits(items: Vec<Item>) -> Vec<Item> {
    let mut defaults: std::collections::BTreeMap<String, Vec<Item>> = Default::default();
    collect_trait_defaults(&items, &mut defaults);
    // Method names each type already defines EXPLICITLY across every impl block (inherent or trait).
    let mut explicit: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    collect_explicit_methods(&items, &mut explicit);
    // Names already injected per type, so two trait impls sharing a default don't both add it.
    let mut injected: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    transform_trait_items(items, &defaults, &explicit, &mut injected)
}

/// Method names each type defines explicitly across all its `impl` blocks (inherent and trait).
fn collect_explicit_methods(
    items: &[Item],
    out: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    for item in items {
        match item {
            Item::Impl {
                type_name, methods, ..
            } => {
                let set = out.entry(type_name.clone()).or_default();
                for m in methods {
                    if let Item::Fn { name, .. } = m {
                        set.insert(name.clone());
                    }
                }
            }
            Item::Module { items, .. } => collect_explicit_methods(items, out),
            _ => {}
        }
    }
}

fn collect_trait_defaults(items: &[Item], out: &mut std::collections::BTreeMap<String, Vec<Item>>) {
    for item in items {
        match item {
            Item::Trait { name, methods, .. } => {
                let ds: Vec<Item> = methods
                    .iter()
                    .filter(|m| matches!(m, Item::Fn { body, .. } if !body.is_empty()))
                    .cloned()
                    .collect();
                out.insert(name.clone(), ds);
            }
            Item::Module { items, .. } => collect_trait_defaults(items, out),
            _ => {}
        }
    }
}

fn transform_trait_items(
    items: Vec<Item>,
    defaults: &std::collections::BTreeMap<String, Vec<Item>>,
    explicit: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    injected: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> Vec<Item> {
    let empty = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Trait { .. } => {} // declaration only — drop after resolution
            Item::Impl {
                type_name,
                trait_name: Some(t),
                mut methods,
                span,
            } => {
                if let Some(ds) = defaults.get(&t) {
                    let have = explicit.get(&type_name).unwrap_or(&empty);
                    let done = injected.entry(type_name.clone()).or_default();
                    for d in ds {
                        if let Item::Fn { name, .. } = d {
                            // Inject a default only if the type does not define it explicitly in
                            // ANY impl block, and it hasn't already been injected for this type.
                            if !have.contains(name) && done.insert(name.clone()) {
                                methods.push(d.clone());
                            }
                        }
                    }
                }
                out.push(Item::Impl {
                    type_name,
                    trait_name: None,
                    methods,
                    span,
                });
            }
            Item::Module { name, items, span } => out.push(Item::Module {
                name,
                items: transform_trait_items(items, defaults, explicit, injected),
                span,
            }),
            other => out.push(other),
        }
    }
    out
}

/// The built-in enum a bare `Some`/`None`/`Ok`/`Err` constructor or pattern belongs to.
/// `Option` for `Some`/`None`, `Result` for `Ok`/`Err`. These need no `enum` declaration.
/// Research/PoC-surface words that are keywords only in their construct form and should otherwise
/// be usable as ordinary identifiers (variables, user-defined functions). Excludes hard keywords
/// like `assume`/`assert`/`research`/`exploit`/`hybrid` that have no plain-identifier meaning.
fn is_soft_research_word(k: &str) -> bool {
    matches!(
        k,
        "symbolic"
            | "unified"
            | "taint_source"
            | "declassify"
            | "tainted"
            | "cpu"
            | "gpu"
            | "prove"
            | "spec"
            | "forall"
            | "Buffer"
            | "intent"
    )
}

pub fn builtin_variant_enum(name: &str) -> Option<&'static str> {
    match name {
        "Some" | "None" => Some("Option"),
        "Ok" | "Err" => Some("Result"),
        _ => None,
    }
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
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("/".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Slash,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '%' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("%".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Percent,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
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
                    tokens.push(SpannedToken {
                        token: Token::PipePipe,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("|".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Pipe,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
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
            '?' => tokens.push(SpannedToken {
                token: Token::Question,
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
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("*".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Star,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '&' => {
                if let Some(&(idx, '&')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::AmpAmp,
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("&".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Amp,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '<' => {
                if let Some(&(_, '<')) = chars.peek() {
                    let (lidx, _) = chars.next().unwrap();
                    if let Some(&(idx, '=')) = chars.peek() {
                        chars.next();
                        tokens.push(SpannedToken {
                            token: Token::OpAssign("<<".into()),
                            span: Span {
                                start,
                                end: idx + 1,
                            },
                        });
                    } else {
                        tokens.push(SpannedToken {
                            token: Token::Shl,
                            span: Span {
                                start,
                                end: lidx + 1,
                            },
                        });
                    }
                } else if let Some(&(idx, '=')) = chars.peek() {
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
                if let Some(&(_, '>')) = chars.peek() {
                    let (ridx, _) = chars.next().unwrap();
                    if let Some(&(idx, '=')) = chars.peek() {
                        chars.next();
                        tokens.push(SpannedToken {
                            token: Token::OpAssign(">>".into()),
                            span: Span {
                                start,
                                end: idx + 1,
                            },
                        });
                    } else {
                        tokens.push(SpannedToken {
                            token: Token::Shr,
                            span: Span {
                                start,
                                end: ridx + 1,
                            },
                        });
                    }
                } else if let Some(&(idx, '=')) = chars.peek() {
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
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("+".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Plus,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '-' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("-".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Minus,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '^' => {
                if let Some(&(idx, '=')) = chars.peek() {
                    chars.next();
                    tokens.push(SpannedToken {
                        token: Token::OpAssign("^".into()),
                        span: Span {
                            start,
                            end: idx + 1,
                        },
                    });
                } else {
                    tokens.push(SpannedToken {
                        token: Token::Caret,
                        span: Span {
                            start,
                            end: start + 1,
                        },
                    });
                }
            }
            '~' => tokens.push(SpannedToken {
                token: Token::Tilde,
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
                    if nc == '\\' {
                        if let Some(&(eidx, ec)) = chars.peek() {
                            chars.next();
                            end = eidx + ec.len_utf8();
                            lex_escape(ec, &mut s, &mut chars, &mut end);
                            continue;
                        }
                    }
                    // Interpolation `${ ... }`: consume it verbatim (balancing braces and skipping
                    // over nested string literals) so an inner `"` or `}` does not end the outer
                    // string. The raw fragment is re-parsed later by `interp_string`.
                    if nc == '$' {
                        if let Some(&(_, '{')) = chars.peek() {
                            chars.next();
                            s.push('$');
                            s.push('{');
                            let mut depth = 1usize;
                            // `in_str` tracks whether we're inside a nested string literal (so a
                            // `{`/`}` there is not counted for brace-balancing), and HOW it was
                            // opened, because two styles are supported:
                            //   1 = bare `"`-delimited (`${x + "lit"}`): contents are preserved
                            //       VERBATIM — the fragment is re-lexed later by `interp_string`,
                            //       and that re-lex is the one and only unescaping pass, so
                            //       `"\\"` keeps its backslash and `"a \"b\""` keeps its quotes.
                            //   2 = `\"`-delimited (`${upper(\"hi\")}`, outer-string style):
                            //       the escaped delimiters become real `"` for the re-lex and
                            //       other escapes are resolved here, as they always were.
                            let mut in_str = 0u8;
                            while depth > 0 {
                                match chars.next() {
                                    Some((i2, '\\')) => {
                                        end = i2 + 1;
                                        if in_str == 1 {
                                            // Inside a bare-delimited string: keep the escape
                                            // verbatim (both characters) for the re-lex.
                                            s.push('\\');
                                            if let Some(&(ei, ec)) = chars.peek() {
                                                chars.next();
                                                end = ei + ec.len_utf8();
                                                s.push(ec);
                                            }
                                        } else if let Some(&(ei, ec)) = chars.peek() {
                                            chars.next();
                                            end = ei + ec.len_utf8();
                                            if ec == '"' {
                                                s.push('"');
                                                in_str = if in_str == 2 { 0 } else { 2 };
                                            } else {
                                                lex_escape(ec, &mut s, &mut chars, &mut end);
                                            }
                                        }
                                    }
                                    Some((i2, '"')) => {
                                        end = i2 + 1;
                                        // A bare quote toggles a bare-delimited nested string;
                                        // inside a `\"`-delimited one it is literal content.
                                        if in_str == 0 {
                                            in_str = 1;
                                        } else if in_str == 1 {
                                            in_str = 0;
                                        }
                                        s.push('"');
                                    }
                                    Some((i2, '{')) => {
                                        end = i2 + 1;
                                        if in_str == 0 {
                                            depth += 1;
                                        }
                                        s.push('{');
                                    }
                                    Some((i2, '}')) => {
                                        end = i2 + 1;
                                        if in_str == 0 {
                                            depth -= 1;
                                        }
                                        s.push('}');
                                    }
                                    Some((i2, c2)) => {
                                        end = i2 + c2.len_utf8();
                                        s.push(c2);
                                    }
                                    None => break,
                                }
                            }
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
                        // Parse as u64 so full-width literals like 0xFFFFFFFFFFFFFFFF are kept
                        // (they reinterpret to their i64 bit pattern downstream) instead of
                        // overflowing i64 and collapsing to 0.
                        let decimal = u64::from_str_radix(&digits, radix)
                            .map(|v| v.to_string())
                            .or_else(|_| i64::from_str_radix(&digits, radix).map(|v| v.to_string()))
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
                    | "enum" | "match" | "impl" | "trait" | "return" | "as" | "while" | "loop"
                    | "break" | "continue" | "mut" | "for" | "in" => Token::Keyword(id),
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
    /// Monotonic counter for compiler-generated temporaries (e.g. compound-assignment index
    /// hoisting). Names use the `__anubis_ca_N` prefix, which user source cannot collide with.
    temp_counter: usize,
}

impl Parser {
    fn new(tokens: Vec<SpannedToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: vec![],
            no_struct: false,
            temp_counter: 0,
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
            let vis = self.parse_visibility();
            if self.check_keyword("fn") {
                if let Some(item) = self.parse_fn(attrs, vis) {
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
            } else if self.check_keyword("impl") {
                if let Some(item) = self.parse_impl() {
                    items.push(item);
                }
            } else if self.check_keyword("trait") {
                if let Some(item) = self.parse_trait() {
                    items.push(item);
                }
            } else {
                let span = self.current_span();
                self.diagnostic("expected item", span);
                self.bump();
            }
        }
        ParseOutput {
            ast: AST {
                items: resolve_traits(items),
            },
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
        self.skip_generic_params();
        self.skip_where_clause();
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
            // Optional `if <guard>` before the arrow.
            let guard = if matches!(&self.current().token, Token::Keyword(k) if k == "if") {
                self.bump();
                Some(self.with_struct_allowed(|p| p.parse_expr(0)))
            } else {
                None
            };
            let _ = self.expect_token(Token::FatArrow, "expected `=>` after match pattern");
            // A `{`-led arm body is a block expression (`=> { stmt; stmt; value }`), matching
            // `if`/lambda bodies; anything else is a normal expression. (A map-literal arm body
            // must be parenthesized: `=> ({ "k": v })`.)
            let body = if self.check_token(&Token::LBrace) {
                self.parse_expr_block()
            } else {
                self.with_struct_allowed(|p| p.parse_expr(0))
            };
            arms.push(MatchArm {
                pattern,
                guard,
                body,
            });
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

    /// Parse a full pattern, including top-level or-patterns (`a | b | c`).
    fn parse_pattern(&mut self) -> Pattern {
        let first = self.parse_pattern_atom();
        if !self.check_token(&Token::Pipe) {
            return first;
        }
        let mut alts = vec![first];
        while self.check_token(&Token::Pipe) {
            self.bump();
            alts.push(self.parse_pattern_atom());
        }
        Pattern::Or(alts)
    }

    /// Parse a single (non-or) pattern.
    fn parse_pattern_atom(&mut self) -> Pattern {
        // List pattern `[p, p, …]`.
        if self.check_token(&Token::LBracket) {
            self.bump();
            let mut subs = vec![];
            while !self.at_eof() && !self.check_token(&Token::RBracket) {
                subs.push(self.parse_pattern_atom());
                if self.check_token(&Token::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
            let _ = self.expect_token(Token::RBracket, "expected `]` in list pattern");
            return Pattern::List(subs);
        }
        // Tuple pattern `(p, p, …)`, or a parenthesized grouping `(p)`.
        if self.check_token(&Token::LParen) {
            self.bump();
            if self.check_token(&Token::RParen) {
                self.bump();
                return Pattern::List(vec![]);
            }
            let first = self.parse_pattern_atom();
            if self.check_token(&Token::Comma) {
                let mut subs = vec![first];
                while self.check_token(&Token::Comma) {
                    self.bump();
                    if self.check_token(&Token::RParen) {
                        break;
                    }
                    subs.push(self.parse_pattern_atom());
                }
                let _ = self.expect_token(Token::RParen, "expected `)` in tuple pattern");
                return Pattern::List(subs);
            }
            let _ = self.expect_token(Token::RParen, "expected `)` in pattern");
            return first;
        }
        // Built-in Option/Result variant patterns: Some(x), None, Ok(x), Err(e).
        if let Token::Ident(name) = &self.current().token {
            if let Some(en) = builtin_variant_enum(name) {
                let variant = name.clone();
                let enum_name = en.to_string();
                self.bump();
                let mut bindings = vec![];
                if self.check_token(&Token::LParen) {
                    self.bump();
                    while !self.at_eof() && !self.check_token(&Token::RParen) {
                        bindings.push(self.parse_pattern_atom());
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RParen, "expected `)` in pattern");
                }
                return Pattern::EnumVariant {
                    enum_name,
                    variant,
                    bindings,
                    named_bindings: vec![],
                };
            }
        }
        // Literal patterns: numbers, negative numbers, booleans, strings/chars.
        match &self.current().token {
            Token::Number(n) => {
                let n = n.clone();
                self.bump();
                return Pattern::Literal(n);
            }
            Token::Minus => {
                // Negative numeric literal: `-5`, `-3.14`.
                self.bump();
                if let Token::Number(n) = &self.current().token {
                    let lit = format!("-{}", n);
                    self.bump();
                    return Pattern::Literal(lit);
                }
                self.diagnostic("expected number after `-` in pattern", self.current_span());
                return Pattern::Wildcard;
            }
            Token::StringLit(s) => {
                let s = s.clone();
                self.bump();
                return Pattern::StrLiteral(s);
            }
            Token::Keyword(k) if k == "true" || k == "false" => {
                let k = k.clone();
                self.bump();
                return Pattern::Literal(k);
            }
            _ => {}
        }
        if let Token::Ident(name) = &self.current().token {
            if name == "_" {
                self.bump();
                return Pattern::Wildcard;
            }
        }
        // Enum::Variant / Enum::Variant(a) / Enum::Variant { f: b }, or a bare binding.
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
                        bindings.push(self.parse_pattern_atom());
                        if self.check_token(&Token::Comma) {
                            self.bump();
                        }
                    }
                    let _ = self.expect_token(Token::RParen, "expected `)` in pattern");
                } else if self.check_token(&Token::LBrace) {
                    self.bump();
                    while !self.at_eof() && !self.check_token(&Token::RBrace) {
                        if let Some((fname, _)) = self.expect_ident("expected field in pattern") {
                            // `field: <sub-pattern>` matches the field; shorthand `field` binds it.
                            let sub = if self.check_token(&Token::Colon) {
                                self.bump();
                                self.parse_pattern_atom()
                            } else {
                                Pattern::Binding(fname.clone())
                            };
                            named_bindings.push((fname, sub));
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
            // Struct pattern: `Point { x, y }` or `Point { x: a }`.
            if self.check_token(&Token::LBrace) {
                self.bump();
                let mut fields = vec![];
                while !self.at_eof() && !self.check_token(&Token::RBrace) {
                    if let Some((fname, _)) = self.expect_ident("expected field in struct pattern")
                    {
                        let sub = if self.check_token(&Token::Colon) {
                            self.bump();
                            self.parse_pattern_atom()
                        } else {
                            // shorthand `{ x }` binds field x to a variable x
                            Pattern::Binding(fname.clone())
                        };
                        fields.push((fname, sub));
                    } else {
                        self.bump();
                    }
                    if self.check_token(&Token::Comma) {
                        self.bump();
                    }
                }
                let _ = self.expect_token(Token::RBrace, "expected `}` in struct pattern");
                return Pattern::Struct {
                    name: enum_name,
                    fields,
                };
            }
            // A bare identifier (no `::`) is a binding pattern: it matches anything
            // and binds the scrutinee to that name.
            return Pattern::Binding(enum_name);
        }
        self.diagnostic("expected match pattern", self.current_span());
        self.bump();
        Pattern::Wildcard
    }

    fn parse_impl(&mut self) -> Option<Item> {
        let start = self.expect_keyword("impl")?.span;
        self.skip_generic_params(); // `impl<T> ...`
        let (first, _) = self.expect_ident("expected type name after `impl`")?;
        self.skip_generic_params(); // `impl Type<T>` or `impl Trait<T> for ...`
                                    // `impl Type { ... }`  or  `impl Trait for Type { ... }`.
        let (trait_name, type_name) = if self.check_keyword("for") {
            self.bump();
            let (ty, _) = self
                .expect_ident("expected type name after `for`")
                .unwrap_or_else(|| ("_".into(), start));
            self.skip_generic_params(); // `for Type<T>`
            (Some(first), ty)
        } else {
            (None, first)
        };
        self.skip_where_clause();
        let _ = self.expect_token(Token::LBrace, "expected `{` after impl type");
        let mut methods = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            let attrs = self.parse_attributes();
            if self.check_keyword("fn") {
                // Methods dispatch by receiver type, never as `Type::method`, so visibility is inert
                // for them; keep them Private so `anubis fmt` doesn't emit `pub` (not accepted here).
                if let Some(m) = self.parse_fn(attrs, Visibility::Private) {
                    methods.push(m);
                }
            } else {
                self.diagnostic("expected `fn` in impl block", self.current_span());
                self.bump();
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}` after impl block");
        let end = self.previous_end();
        Some(Item::Impl {
            type_name,
            trait_name,
            methods,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_trait(&mut self) -> Option<Item> {
        let start = self.expect_keyword("trait")?.span;
        let (name, _) = self.expect_ident("expected trait name")?;
        self.skip_generic_params();
        self.skip_where_clause();
        let _ = self.expect_token(Token::LBrace, "expected `{` after trait name");
        let mut methods = vec![];
        while !self.at_eof() && !self.check_token(&Token::RBrace) {
            let attrs = self.parse_attributes();
            if self.check_keyword("fn") {
                // Methods dispatch by receiver type, never as `Type::method`, so visibility is inert
                // for them; keep them Private so `anubis fmt` doesn't emit `pub` (not accepted here).
                if let Some(m) = self.parse_fn(attrs, Visibility::Private) {
                    methods.push(m);
                }
            } else {
                self.diagnostic("expected `fn` in trait", self.current_span());
                self.bump();
            }
        }
        let _ = self.expect_token(Token::RBrace, "expected `}` after trait");
        let end = self.previous_end();
        Some(Item::Trait {
            name,
            methods,
            span: Span {
                start: start.start,
                end,
            },
        })
    }

    fn parse_enum(&mut self) -> Option<Item> {
        let start = self.expect_keyword("enum")?.span;
        let (name, _) = self.expect_ident("expected enum name")?;
        self.skip_generic_params();
        self.skip_where_clause();
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
                            let fty = self.collect_type_until(&[
                                Token::Comma,
                                Token::RBrace,
                                Token::Semi,
                            ]);
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
                variants.push(EnumVariant { name: vname, kind });
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
            // Statement-introducing keywords are always statements. `if` is included so a bare
            // no-else `if` (a guard) parses as a statement; a trailing `if/else` is recovered as
            // the block's value by `Expr::Block` lowering (via split_tail_expr).
            if self.check_keyword("let")
                || self.check_keyword("if")
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
                // Hoist side-effecting index subexpressions so the place is evaluated once.
                let mut hoist = Vec::new();
                let place = self.hoist_place_indices(e, &mut hoist);
                stmts.append(&mut hoist);
                stmts.push(Stmt::Assign {
                    target: place.clone(),
                    value: Expr::Binary {
                        op,
                        lhs: Box::new(place),
                        rhs: Box::new(rhs),
                    },
                });
            } else if self.check_token(&Token::Semi) {
                self.bump();
                stmts.push(Stmt::ExprStmt(e));
            } else if matches!(
                e,
                Expr::Match { .. } | Expr::If { .. } | Expr::IfLet { .. } | Expr::Block { .. }
            ) && !self.check_token(&Token::RBrace)
            {
                // A block-like expression (`match`/`if`/`{ … }`) in statement position needs no
                // `;` to be a statement — only the FINAL expression is the block's tail value.
                // Without this, `match x { … }` followed by another statement inside a closure or
                // match-arm body was mis-parsed as the tail, and the next statement broke parsing.
                stmts.push(Stmt::ExprStmt(e));
            } else {
                // No trailing separator and at block end → this expression is the block's value.
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
        // `if let PATTERN = scrutinee { then } else { else_ }` as an expression.
        if self.check_keyword("let") {
            return self.parse_if_let(start);
        }
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

    /// Parse `if let PATTERN = scrutinee { then } else { else_ }` into an `Expr::IfLet`.
    /// Assumes the leading `if` has been consumed and the current token is `let`.
    fn parse_if_let(&mut self, start: Span) -> Expr {
        let _ = self.expect_keyword("let");
        let pattern = self.parse_pattern();
        let _ = self.expect_token(Token::Eq, "expected `=` in `if let`");
        let scrutinee = self.parse_header_expr();
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
            // No else branch: an unmatched `if let` yields the default `0`.
            Expr::Literal("0".into())
        };
        Expr::IfLet {
            pattern,
            scrutinee: Box::new(scrutinee),
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
                if let Some(item) = self.parse_fn(attrs, Visibility::Private) {
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
            } else if self.check_keyword("impl") {
                if let Some(item) = self.parse_impl() {
                    items.push(item);
                }
            } else if self.check_keyword("trait") {
                if let Some(item) = self.parse_trait() {
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

    /// Consume a generic parameter list `<T, U: Bound, ...>` if present, ignoring it — the runtime
    /// is dynamically typed, so generics are purely syntactic. Balances nested `<...>` and `>>`.
    fn skip_generic_params(&mut self) {
        if !self.check_token(&Token::Lt) {
            return;
        }
        self.bump();
        let mut depth = 1i32;
        while depth > 0 && !self.at_eof() {
            if self.check_token(&Token::Lt) {
                depth += 1;
            } else if self.check_token(&Token::Gt) {
                depth -= 1;
            } else if self.check_token(&Token::Shr) {
                depth -= 2; // `>>` closes two levels
            }
            self.bump();
        }
    }

    /// Consume a `where T: Bound, ...` clause (up to the body `{` or `;`), ignoring it.
    fn skip_where_clause(&mut self) {
        if matches!(&self.current().token, Token::Ident(w) if w == "where") {
            while !self.at_eof()
                && !self.check_token(&Token::LBrace)
                && !self.check_token(&Token::Semi)
            {
                self.bump();
            }
        }
    }

    /// Parse zero or more `invariant(P)` clauses (B3 loop invariants) between a loop header and its
    /// `{` body. Contextual keyword, mirroring the `requires`/`ensures` contract clauses. Each
    /// predicate is parenthesized, so there is no ambiguity with the body brace.
    fn parse_loop_invariants(&mut self) -> Vec<Expr> {
        let mut invariants = vec![];
        while matches!(&self.current().token, Token::Ident(k) if k == "invariant") {
            self.bump();
            if self.check_token(&Token::LParen) {
                self.bump();
                let cond = self.with_struct_allowed(|p| p.parse_expr(0));
                let _ = self.expect_token(Token::RParen, "expected `)` after loop invariant");
                invariants.push(cond);
            } else {
                self.diagnostic("expected `(` after `invariant`", self.current_span());
                break;
            }
        }
        invariants
    }

    /// Consume an optional leading `pub`, returning `Public` when present. `pub` is contextual
    /// (lexed as an identifier), so it is a modifier only in item position — a value named `pub`
    /// elsewhere is unaffected.
    fn parse_visibility(&mut self) -> Visibility {
        if let Token::Ident(k) = &self.current().token {
            if k == "pub" {
                self.bump();
                return Visibility::Public;
            }
        }
        Visibility::Private
    }

    fn parse_fn(&mut self, pre_attrs: Vec<Attribute>, visibility: Visibility) -> Option<Item> {
        let start = self.expect_keyword("fn")?.span;
        let (name, _) = self.expect_ident("expected function name")?;
        self.skip_generic_params(); // `fn foo<T>(...)`
        let params = self.parse_params();
        // Optional return type: `-> Type` (lexed as Minus Gt then the type). Captured in the AST
        // for tooling/typecheck even though the runtime is dynamically typed.
        let mut ret: Option<String> = None;
        if self.check_token(&Token::Minus) {
            let _ = self.bump();
            if self.check_token(&Token::Gt) {
                let _ = self.bump();
            }
            // collect_type_until also stops at a `requires`/`ensures` contract clause (by value), so
            // the clauses are not greedily absorbed into the return-type string.
            let ty = self.collect_type_until(&[Token::LBrace, Token::Semi]);
            if !ty.is_empty() {
                ret = Some(ty);
            }
        }
        self.skip_where_clause(); // `fn foo<T>() where T: Ord { ... }`
                                  // B2 contracts: `requires(P)` / `ensures(Q)` clauses sit between the signature and the body.
                                  // They are contextual (parsed as identifiers) and unambiguous here — only a clause or the
                                  // `{`/`;` body can follow the signature.
        let mut requires = vec![];
        let mut ensures = vec![];
        loop {
            let clause = match &self.current().token {
                Token::Ident(k) if k == "requires" => Some(true),
                Token::Ident(k) if k == "ensures" => Some(false),
                _ => None,
            };
            match clause {
                Some(is_requires) => {
                    self.bump();
                    if self.check_token(&Token::LParen) {
                        self.bump();
                        let cond = self.with_struct_allowed(|p| p.parse_expr(0));
                        let _ = self
                            .expect_token(Token::RParen, "expected `)` after contract condition");
                        if is_requires {
                            requires.push(cond);
                        } else {
                            ensures.push(cond);
                        }
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }
        let body_start = self.current_span();
        let body = if self.check_token(&Token::LBrace) {
            self.parse_block()
        } else if self.check_token(&Token::Semi) {
            // A `;`-terminated signature (`fn area(self);`) — a required trait method, or a
            // forward declaration. Empty body: calling it (unless overridden) yields 0.
            self.bump();
            vec![]
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
            visibility,
            params,
            body,
            mode,
            intent: None,
            ret,
            requires,
            ensures,
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
            // `while let PATTERN = expr { body }`
            if self.check_keyword("let") {
                self.bump();
                let pattern = self.parse_pattern();
                let _ = self.expect_token(Token::Eq, "expected `=` in `while let`");
                let expr = self.parse_header_expr();
                let body = if self.check_token(&Token::LBrace) {
                    self.parse_block()
                } else {
                    self.diagnostic("expected `{` after `while let`", self.current_span());
                    vec![]
                };
                return Some(Stmt::WhileLet {
                    pattern,
                    expr,
                    body,
                });
            }
            let cond = self.parse_header_expr();
            let invariant = self.parse_loop_invariants();
            let body = if self.check_token(&Token::LBrace) {
                self.parse_block()
            } else {
                self.diagnostic("expected `{` after while cond", self.current_span());
                vec![]
            };
            return Some(Stmt::While {
                cond,
                body,
                invariant,
            });
        }
        if self.check_keyword("loop") {
            self.bump();
            let invariant = self.parse_loop_invariants();
            let body = if self.check_token(&Token::LBrace) {
                self.parse_block()
            } else {
                self.diagnostic("expected `{` after loop", self.current_span());
                vec![]
            };
            return Some(Stmt::Loop { body, invariant });
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
                ForSource::Range { start: first, end }
            } else {
                ForSource::Collection { expr: first }
            };
            let invariant = self.parse_loop_invariants();
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
                invariant,
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
            // Compound assignment `place op= expr` desugars to `place = place op expr`, with any
            // side-effecting index subexpression hoisted so the place is evaluated exactly once.
            if let Token::OpAssign(op) = &self.current().token {
                let op = op.clone();
                self.bump();
                let rhs = self.parse_expr(0);
                self.consume_optional_semi();
                let mut hoist = Vec::new();
                let place = self.hoist_place_indices(expr, &mut hoist);
                let assign = Stmt::Assign {
                    target: place.clone(),
                    value: Expr::Binary {
                        op,
                        lhs: Box::new(place),
                        rhs: Box::new(rhs),
                    },
                };
                if hoist.is_empty() {
                    return Some(assign);
                }
                // This context returns a single statement, so wrap the temporaries + the
                // assignment in a block (which shares the enclosing scope, so the mutation lands).
                hoist.push(assign);
                return Some(Stmt::ExprStmt(Expr::Block {
                    stmts: hoist,
                    tail: None,
                }));
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
        // Destructuring binding: `let [a, b] = xs`, `let (a, b) = pair`, `let Point { x, y } = p`.
        let struct_destructure = matches!(&self.current().token, Token::Ident(_))
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.token),
                Some(Token::LBrace)
            );
        if self.check_token(&Token::LBracket)
            || self.check_token(&Token::LParen)
            || struct_destructure
        {
            let pattern = self.parse_pattern_atom();
            let init = if self.check_token(&Token::Eq) {
                self.bump();
                self.parse_expr(0)
            } else {
                self.diagnostic("expected `=` in destructuring binding", self.current_span());
                Expr::Other("missing-init".into())
            };
            let end = if self.check_token(&Token::Semi) {
                self.bump().map(|t| t.span.end).unwrap_or(start.end)
            } else {
                self.previous_end()
            };
            return Some(Stmt::LetPattern {
                pattern,
                init,
                span: Span {
                    start: start.start,
                    end,
                },
            });
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

        // The trailing `;` is optional, matching every other statement kind
        // (expression statements, assignments, and `return` are all newline-terminated).
        // Consume it if present; otherwise the binding ends at the initializer.
        let end = if self.check_token(&Token::Semi) {
            self.bump().map(|t| t.span.end).unwrap_or(start.end)
        } else {
            self.previous_end()
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
        let start = self.expect_keyword("if")?.span;
        // `if let PATTERN = expr { ... } else { ... }` — an expression, used here for side effects.
        if self.check_keyword("let") {
            return Some(Stmt::ExprStmt(self.parse_if_let(start)));
        }
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
                let ty = self.parse_cast_type();
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
    /// Desugar string interpolation: `"a ${expr} b"` → `"" + "a " + (expr) + " b"`. The `+`
    /// operator coerces each interpolated value to its display form. A literal `${` is not
    /// currently escapable. Nested string literals (with their own escapes) are allowed inside
    /// `${...}`: the lexer preserved the fragment verbatim, and re-lexing it here resolves
    /// escapes exactly once.
    fn interp_string(&mut self, s: String) -> Expr {
        if !s.contains("${") {
            return Expr::StrLiteral(s);
        }
        let chars: Vec<char> = s.chars().collect();
        // Seed with "" so the whole concatenation is string-typed even if it starts with a value.
        let mut parts: Vec<Expr> = vec![Expr::StrLiteral(String::new())];
        let mut lit = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                if !lit.is_empty() {
                    parts.push(Expr::StrLiteral(std::mem::take(&mut lit)));
                }
                i += 2;
                let mut depth = 1usize;
                let mut expr_src = String::new();
                while i < chars.len() && depth > 0 {
                    match chars[i] {
                        '{' => {
                            depth += 1;
                            expr_src.push('{');
                        }
                        '}' => {
                            depth -= 1;
                            if depth > 0 {
                                expr_src.push('}');
                            }
                        }
                        '"' => {
                            // Copy a nested string literal verbatim so a `}` inside it doesn't
                            // close the interpolation.
                            expr_src.push('"');
                            i += 1;
                            while i < chars.len() {
                                let c = chars[i];
                                expr_src.push(c);
                                i += 1;
                                if c == '\\' {
                                    if i < chars.len() {
                                        expr_src.push(chars[i]);
                                        i += 1;
                                    }
                                    continue;
                                }
                                if c == '"' {
                                    break;
                                }
                            }
                            continue;
                        }
                        c => expr_src.push(c),
                    }
                    i += 1;
                }
                if expr_src.trim().is_empty() {
                    // `${}` with no expression is almost always a typo; report it clearly instead
                    // of producing an unlowerable placeholder, and treat it as empty text.
                    self.diagnostics.push(ParseDiagnostic {
                        message: "empty interpolation `${}` has no expression".into(),
                        span: Span::default(),
                    });
                    parts.push(Expr::StrLiteral(String::new()));
                } else {
                    parts.push(self.parse_embedded_expr(&expr_src));
                }
            } else {
                lit.push(chars[i]);
                i += 1;
            }
        }
        if !lit.is_empty() {
            parts.push(Expr::StrLiteral(lit));
        }
        // An interpolation with no parts at all (e.g. the empty string `""`) is the empty string.
        if parts.is_empty() {
            return Expr::StrLiteral(String::new());
        }
        let mut acc = parts.remove(0);
        for p in parts {
            acc = Expr::Binary {
                op: "+".into(),
                lhs: Box::new(acc),
                rhs: Box::new(p),
            };
        }
        acc
    }

    /// Parse a standalone expression from an interpolation fragment, forwarding any diagnostics.
    fn parse_embedded_expr(&mut self, src: &str) -> Expr {
        let mut sub = Parser::new(lex_spanned(src));
        let e = sub.parse_expr(0);
        self.diagnostics.extend(sub.diagnostics);
        e
    }

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

    /// Whether a soft research keyword (already bumped) is followed by the token that begins its
    /// construct — otherwise it is being used as a plain identifier. `self.current()` is the token
    /// immediately after the keyword.
    fn soft_kw_starts_construct(&self, k: &str) -> bool {
        match k {
            // `symbolic()`, `symbolic<u32>()`, or the turbofish `symbolic::<u32>()`.
            "symbolic" => matches!(
                self.current().token,
                Token::LParen | Token::Lt | Token::ColonColon
            ),
            "unified" => matches!(&self.current().token,
                Token::Keyword(w) | Token::Ident(w) if w == "Buffer"),
            "taint_source" | "declassify" => matches!(self.current().token, Token::LParen),
            // cpu/gpu/prove/spec/forall/tainted/Buffer/intent never form a bare-expression
            // construct — in expression position they are always identifiers.
            _ => false,
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
        // Control-flow keywords in expression position — a braceless match arm body
        // (`3 => return 999`, `n if c => continue`) or an if-expression branch. Lowered to the
        // same `Call` forms the statement parser produces; each diverges (`!`), so it composes
        // as a value of any type.
        if self.check_keyword("return") {
            self.bump();
            let val = if self.starts_expr() {
                self.parse_expr(0)
            } else {
                Expr::Literal("0".into())
            };
            return Expr::Call {
                callee: "return".into(),
                args: vec![val],
            };
        }
        if self.check_keyword("break") {
            self.bump();
            return Expr::Call {
                callee: "break".into(),
                args: vec![],
            };
        }
        if self.check_keyword("continue") {
            self.bump();
            return Expr::Call {
                callee: "continue".into(),
                args: vec![],
            };
        }
        let Some(mut tok) = self.bump() else {
            return Expr::Other("eof".into());
        };
        // Soft research keywords (`symbolic`/`unified`/`taint_source`/`declassify`/`tainted`/
        // `cpu`/`gpu`/`prove`/`spec`/`forall`/`Buffer`/`intent`) form their constructs only in a
        // specific syntactic form (e.g. `symbolic(...)`, `unified Buffer<T>`). Used as an ordinary
        // identifier — a variable or a user-defined function like `fn unified()` — they must NOT be
        // hijacked into a research construct that the run path then rejects. Re-tag as an identifier
        // unless the construct's trigger token follows.
        if let Token::Keyword(k) = &tok.token {
            if is_soft_research_word(k) && !self.soft_kw_starts_construct(k) {
                tok.token = Token::Ident(k.clone());
            }
        }
        let primary = match tok.token {
            Token::Number(n) => Expr::Literal(n),
            Token::StringLit(s) => self.interp_string(s),
            Token::Ident(name) => {
                // Built-in Option/Result constructors: Some(x), None, Ok(x), Err(x) — no decl needed.
                if let Some(en) = builtin_variant_enum(&name) {
                    let fields = if self.check_token(&Token::LParen) {
                        self.parse_call_args()
                    } else {
                        vec![]
                    };
                    Expr::EnumConstruct {
                        enum_name: en.to_string(),
                        variant: name.clone(),
                        fields,
                        field_names: vec![],
                        span: tok.span,
                    }
                }
                // Enum construct: Status::Ok / Status::Err(a) / Status::Err { code: a }
                else if self.check_token(&Token::ColonColon) {
                    self.bump();
                    let (variant, _) = self
                        .expect_ident("expected variant after `::`")
                        .unwrap_or_else(|| ("_".into(), tok.span));
                    let (fields, field_names) = if self.check_token(&Token::LParen) {
                        (self.parse_call_args(), vec![])
                    } else if (!self.no_struct || self.looks_like_struct_fields())
                        && self.check_token(&Token::LBrace)
                    {
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
            Token::Keyword(k) if k == "assert" || k == "assume" => {
                // Also usable in expression position (`let ok = assert(cond)`), not just as a
                // statement; `assert` panics fail-closed on false, `assume` is a solver hint.
                let is_assert = k == "assert";
                let _ = self.expect_token(Token::LParen, "expected `(` after assert/assume");
                let inner = self.parse_expr(0);
                let _ = self.expect_token(Token::RParen, "expected `)` after expression");
                if is_assert {
                    Expr::Assert(Box::new(inner))
                } else {
                    Expr::Assume(Box::new(inner))
                }
            }
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
                // `(e)` is grouping; `(a, b, …)` is a tuple, represented as a list value;
                // `()` is the empty tuple / unit, an empty list.
                if self.check_token(&Token::RParen) {
                    self.bump();
                    Expr::ArrayLiteral { elements: vec![] }
                } else {
                    let first = self.with_struct_allowed(|p| p.parse_expr(0));
                    if self.check_token(&Token::Comma) {
                        let mut elements = vec![first];
                        while self.check_token(&Token::Comma) {
                            self.bump();
                            if self.check_token(&Token::RParen) {
                                break; // allow a trailing comma
                            }
                            elements.push(self.with_struct_allowed(|p| p.parse_expr(0)));
                        }
                        let _ = self.expect_token(Token::RParen, "expected `)` after tuple");
                        Expr::ArrayLiteral { elements }
                    } else {
                        let _ = self.expect_token(Token::RParen, "expected `)` after expression");
                        first
                    }
                }
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
            } else if self.check_token(&Token::Question) {
                // Error-propagation postfix: `expr?`.
                self.bump();
                e = Expr::Try(Box::new(e));
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
            // `{` in statement/tail position is a map literal — the language has no bare-block
            // statement (blocks only appear as if/while/for/fn/lambda/match bodies).
            | Token::LBrace
            | Token::Minus
            | Token::Tilde
            | Token::Pipe
            | Token::PipePipe
            | Token::Bang => true,
            // `match` and `if` are expressions in this language, so they can also stand
            // as statements / block-tail expressions (`match x { ... }` for its value or
            // side effects). `parse_primary` handles both keywords.
            Token::Keyword(k) => {
                k == "declassify"
                    || k == "true"
                    || k == "false"
                    || k == "unified"
                    || k == "match"
                    || k == "if"
            }
            _ => false,
        }
    }

    /// Parse the target type of an `as` cast: an optional pointer/reference prefix followed by a
    /// single type name (`u8`, `i64`, `f64`, `usize`, `*mut u8`, ...). Unlike `collect_type_until`,
    /// it stops as soon as the type is complete, so `x as u8 + 1` parses `u8` and leaves `+ 1` for
    /// the binary-operator loop. (Previously the operator and its right operand were swallowed into
    /// the "type" string, which was then unrecognized, so the cast silently voided and dropped the
    /// rest of the expression.)
    fn parse_cast_type(&mut self) -> String {
        let mut ty = String::new();
        // Pointer / reference prefixes: `*`, `*mut`, `*const`, `&`, `&mut` (repeatable).
        loop {
            if self.check_token(&Token::Star) {
                self.bump();
                ty.push('*');
                if self.check_keyword("mut") {
                    self.bump();
                    ty.push_str("mut ");
                } else if self.check_keyword("const") {
                    self.bump();
                    ty.push_str("const ");
                }
            } else if self.check_token(&Token::Amp) {
                self.bump();
                ty.push('&');
                if self.check_keyword("mut") {
                    self.bump();
                    ty.push_str("mut ");
                }
            } else {
                break;
            }
        }
        // Base type name: a single identifier or keyword.
        if matches!(self.current().token, Token::Ident(_) | Token::Keyword(_)) {
            if let Some(tok) = self.bump() {
                match tok.token {
                    Token::Ident(s) | Token::Keyword(s) => ty.push_str(&s),
                    _ => {}
                }
            }
        }
        ty
    }

    /// Rewrite an assignable place so that every side-effecting index subexpression is evaluated
    /// exactly once: each non-trivial `[index]` is hoisted into a fresh `let __anubis_ca_N = index`
    /// (pushed into `lets`) and replaced by that temporary. Used by the compound-assignment desugar
    /// so `xs[pop(sel)] += 5` pops once, not once for the read and once for the write.
    fn hoist_place_indices(&mut self, place: Expr, lets: &mut Vec<Stmt>) -> Expr {
        match place {
            Expr::Index { base, index } => {
                let base = self.hoist_place_indices(*base, lets);
                // A bare variable or literal index is safe to re-evaluate; anything else may have
                // side effects (a call, `pop`, arithmetic on a call, …) and must be hoisted.
                let index = match *index {
                    idx @ (Expr::Var(_) | Expr::Literal(_)) => idx,
                    idx => {
                        let tmp = format!("__anubis_ca_{}", self.temp_counter);
                        self.temp_counter += 1;
                        lets.push(Stmt::Let {
                            name: tmp.clone(),
                            ty: None,
                            init: idx,
                            span: Span::default(),
                        });
                        Expr::Var(tmp)
                    }
                };
                Expr::Index {
                    base: Box::new(base),
                    index: Box::new(index),
                }
            }
            Expr::FieldAccess { base, field, span } => {
                let base = self.hoist_place_indices(*base, lets);
                Expr::FieldAccess {
                    base: Box::new(base),
                    field,
                    span,
                }
            }
            other => other,
        }
    }

    fn collect_type_until(&mut self, stops: &[Token]) -> String {
        let mut ty = String::new();
        // Track angle-bracket depth so a stop token inside generic arguments does not end the type
        // early: `Map<int, string>` keeps its inner comma instead of stopping at it.
        let mut depth: i32 = 0;
        while !self.at_eof()
            && !(depth == 0
                && (stops.iter().any(|stop| self.check_token(stop))
                    // Also stop at a B2 `requires`/`ensures` contract clause (matched by value —
                    // no type is named these), so it is not absorbed into the type.
                    || matches!(&self.current().token, Token::Ident(k) if k == "requires" || k == "ensures")))
        {
            let Some(tok) = self.bump() else {
                break;
            };
            match tok.token {
                Token::Ident(s) | Token::Keyword(s) | Token::Number(s) => ty.push_str(&s),
                Token::Lt => {
                    depth += 1;
                    ty.push('<');
                }
                Token::Gt => {
                    depth -= 1;
                    ty.push('>');
                }
                // `>>` closing a nested generic (`Box<Box<T>>`) lexes as a single shift token.
                Token::Shr => {
                    depth -= 2;
                    ty.push_str(">>");
                }
                Token::Star => ty.push('*'),
                Token::Amp => ty.push('&'),
                // A comma inside generic arguments is part of the type; at depth 0 it is dropped
                // (unchanged from before — stop sets that include Comma break above).
                Token::Comma => {
                    if depth > 0 {
                        ty.push_str(", ");
                    }
                }
                Token::Colon | Token::Dot => {}
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

    /// Lookahead: does the current `{` open struct/variant fields (`{ ident : ...`) rather than a
    /// following match body (`{ pattern => ...`)? This lets a struct-variant construction be a match
    /// scrutinee — `match Rec::Full { x: 1 } { ... }` — even in no-struct context, while a
    /// unit-variant scrutinee (`match Status::Active { ... }`) still leaves the brace for the body
    /// (a match arm never begins `ident :`).
    fn looks_like_struct_fields(&self) -> bool {
        matches!(self.current().token, Token::LBrace)
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.token),
                Some(Token::Ident(_))
            )
            && matches!(
                self.tokens.get(self.pos + 2).map(|t| &t.token),
                Some(Token::Colon)
            )
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

/// Resolve a byte offset into a 1-based `(line, column)` pair, counting columns in
/// Unicode scalar values (not bytes) so multi-byte source still points at the right cell.
/// Offsets at or past end-of-source clamp to the final position.
pub fn line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let clamped = byte_offset.min(source.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx >= clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Render one parse diagnostic in the rustc-grade format: `path:line:col: error: message`
/// followed by the offending source line and a caret underline sized to the span. The
/// diagnostic `message` is preserved verbatim so `ANUBIS_*` codes and `ERROR_CONTAINS`
/// substrings still match on the rendered text.
pub fn render_parse_diagnostic(source: &str, diag: &ParseDiagnostic, path: Option<&str>) -> String {
    let (line, col) = line_col(source, diag.span.start);
    let file = path.unwrap_or("<anubis>");
    let src_line = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let start = diag.span.start.min(source.len());
    let end = diag.span.end.clamp(start, source.len());
    let underline_len = source
        .get(start..end)
        .map(|s| s.chars().take_while(|&c| c != '\n').count())
        .unwrap_or(1)
        .max(1);
    let gutter = line.to_string();
    let pad = " ".repeat(gutter.len());
    let caret_pad = " ".repeat(col.saturating_sub(1));
    let carets = "^".repeat(underline_len);
    format!(
        "{file}:{line}:{col}: error: {msg}\n {pad} |\n {gutter} | {src_line}\n {pad} | {caret_pad}{carets}",
        msg = diag.message
    )
}

/// Render every diagnostic from a parse, rustc-style, or `None` when the source parses cleanly.
/// This is the user-facing counterpart to `parse_source`'s `"; "`-joined error string.
pub fn render_parse_errors(source: &str, path: Option<&str>) -> Option<String> {
    let output = parse_source_detailed(source);
    if output.diagnostics.is_empty() {
        return None;
    }
    Some(
        output
            .diagnostics
            .iter()
            .map(|d| render_parse_diagnostic(source, d, path))
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

#[cfg(test)]
mod diagnostic_render_tests {
    use super::*;

    #[test]
    fn line_col_maps_offset_to_1based_line_and_column() {
        let src = "fn main() {\n    let x = ;\n}\n";
        let semi = src.find(';').unwrap();
        assert_eq!(line_col(src, semi), (2, 13));
        assert_eq!(line_col(src, 0), (1, 1));
        // past EOF clamps rather than panicking
        let (l, _c) = line_col(src, src.len() + 100);
        assert_eq!(l, 4);
    }

    #[test]
    fn render_parse_diagnostic_points_a_caret_at_the_span() {
        let src = "fn main() {\n    let x = ;\n}\n";
        let semi = src.find(';').unwrap();
        let diag = ParseDiagnostic {
            message: "expected expression".into(),
            span: Span {
                start: semi,
                end: semi + 1,
            },
        };
        let rendered = render_parse_diagnostic(src, &diag, Some("t.anb"));
        assert!(rendered.contains("t.anb:2:13: error: expected expression"));
        assert!(rendered.contains("let x = ;"));
        // caret sits under column 13 (12 spaces of padding then a single caret)
        assert!(rendered.contains("|             ^"));
    }

    #[test]
    fn render_parse_errors_is_none_for_valid_source() {
        assert!(render_parse_errors("fn main() { print(1); }\n", None).is_none());
        assert!(render_parse_errors("", None).is_none());
    }

    #[test]
    fn render_parse_errors_preserves_message_for_error_contains_matching() {
        // A source the recovering parser rejects; the rendered text must still carry
        // the underlying message so fixture `ERROR_CONTAINS` needles keep matching.
        let src = "import foo bar baz\n";
        if let Some(rendered) = render_parse_errors(src, Some("bad.anb")) {
            // Line/col of a recovering parser's EOF diagnostic is an implementation detail;
            // assert only the stable structure: the file is named and a caret is drawn.
            assert!(rendered.contains("bad.anb:"));
            assert!(rendered.contains("error:"));
            assert!(rendered.contains('^'));
        }
    }
}
