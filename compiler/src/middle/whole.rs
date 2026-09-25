//! The whole-struct lane.
//!
//! A struct whose declaration has a `secret` field is not itself secret: reading one of its public
//! fields (`p.pub_n`), taking the length of a list of them, or testing a map key reveals nothing of
//! the secret field. But the runtime renders EVERY field when such a struct leaves the program whole
//! (`print(p)`), and a value computed from its full contents (`str(p)`, `p == q`, `sha256(p)`, the
//! order `sort(xs)` puts them in) depends on the secret field. The field lane labels fields; this lane
//! tracks what a value holds of such a struct ([`WholeSrc`]), and the egress checks refuse any value
//! that holds anything.
//!
//! [`src`] gives what an expression holds. Function bodies, value blocks and closure bodies are run by
//! a small abstract interpreter ([`interp`]) in program order. A `let` or plain assignment replaces
//! what a name holds, and a write into a field, key or position adds to it. A `let` is scoped to its
//! block. Branches join. A loop repeats to a fixpoint, leaving with the names at every point a
//! `break` or the loop condition may leave from, and is widened if it keeps growing. Expressions carry
//! the names through too (a write inside a `match` arm, a `push` in an argument). The same pass
//! collects what a body returns, joined with what the conditions selecting each `return` decide, and
//! where it makes such a struct reach an egress sink.
//!
//! A call of a user function or method is SPECIALIZED: the callee's body is interpreted with its
//! formals bound to what the actual arguments hold, and its callbacks to the actual closures and
//! functions. A helper is thus judged by what it does with what it is given. Recursion iterates to a
//! fixpoint, widening what keeps growing. Every limit fails CLOSED — call depth, a work budget,
//! closure nesting, a fixpoint that does not settle. The callee is then assumed to compute from, and
//! release, everything it was given, everything a closure given to it can reach, and every struct it
//! may build.
//!
//! Declared types are not enforced by the checker (`let s: list<i64> = "ab"` checks). The lane
//! therefore uses them only to ADD what a value may hold, never to decide that it holds less.
//! Builtins are classified by [`builtin_src`] against the runtime (`backends/run.rs`). A builtin it
//! does not name is COMPUTED from all its arguments, so a new builtin fails closed.

use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Prefix of a source COMPUTED from a whole struct. It does not start with `WHOLE_STRUCT_SOURCE`, so
/// no field-lane consumer that discards a whole VALUE discards it.
const WHOLE_COMPUTED_SOURCE: &str = "a value computed from ";

/// The entry under which a [`WholeSrc::Record`] notes that the record may itself be such a struct.
/// Entries the lane names itself start with `\u{1}`; a key from the program that does is escaped
/// ([`user_key`]), so no map key can be taken for one.
const WHOLE_SELF: &str = "\u{1}self";

/// The entry under which a [`WholeSrc::Record`] notes what an entry at a key or position the lane
/// cannot name holds (`{ k: p }`, `xs[i] = p`): every read may yield it.
const WHOLE_ANY: &str = "\u{1}*";

/// The entry under which a [`WholeSrc::Record`] holds a `Result`'s `Err` payload (what `?` returns
/// early), apart from an `Ok` payload.
const WHOLE_ERR: &str = "\u{1}err";

/// Separates the sources a joined [`WholeSrc::Value`] may be (structs of several types).
const TEXT_SEP: &str = "\u{2}";

/// Nested closure applications and nested calls followed, specializations in one query, and loop,
/// fold and fixpoint iterations, before the lane assumes the worst.
const MAX_APPLY_DEPTH: u32 = 24;
const MAX_CALL_DEPTH: usize = 24;
const MAX_WORK: usize = 4000;
const MAX_ITER: usize = 6;

/// Statement lists and closure bodies run in one query, before every further call is assumed the
/// worst (a bound on time and memory whatever the program).
const MAX_STEPS: usize = 60_000;

/// How deep a value a call or closure application may return before it is widened (applications
/// wrapping their argument can nest it without bound).
const MAX_DEPTH: usize = 24;

/// How deeply closures may close over closures (a chain of wrappers) before the captured ones are
/// taken as opaque functions.
const MAX_CLOSURE_NEST: usize = 12;
const MAX_SIZE: usize = 1024;

/// Distinct specializations of one function in one query before its further calls share one, with
/// their bindings joined (and widened as they grow).
const SUMMARY_AFTER: usize = 32;

/// Specializations of one function with closures of the same lambdas before its further calls with
/// them share one.
const SAME_LAMBDAS_AFTER: usize = 12;

/// Times a shared specialization's bindings may still change by merging closures per lambda before
/// its closures are widened to opaque functions (each change re-runs everything downstream).
const SHARED_CHANGES: usize = 64;

/// How many closures deep a shared specialization keeps closures exact.
const MERGE_NEST: usize = 3;

/// How many parameters a callback is bound when the lane spreads a value into it (`apply(f, xs)`,
/// an unknown builtin's callback), at least.
const SPREAD: usize = 64;
/// Block runs remembered per query. Every run from a new state is remembered, and nested loops
/// produce one per iteration of every enclosing loop: past this, runs are not remembered (they
/// still count as steps), so the memory a query holds is bounded.
const MAX_BLOCKS: usize = 256;

/// The builtins that write their first argument (a variable) in place.
const IN_PLACE: [&str; 6] = ["push", "append", "unshift", "insert", "remove", "pop"];

/// A function's formals and body, as the whole-struct lane specializes it.
pub(super) type WholeBody = (Vec<String>, Vec<Stmt>);

/// Specializations shared by every query of a program (see `SemanticContext::whole_memo`): what
/// each returns and where it releases a whole struct.
pub(super) type WholeMemo =
    std::cell::RefCell<BTreeMap<String, (Option<WholeSrc>, Option<WholeSrc>)>>;

/// What a value holds of a struct with a `secret` field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WholeSrc {
    /// Such a struct (of any of the types its sources name). Its declared public fields reveal
    /// nothing; leaving whole releases the secret field.
    Value(String),
    /// Computed from such a struct's full contents: a rendering, a digest, a comparison, a length,
    /// count, order or choice decided by one. Everything about it depends on the secret field.
    Computed(String),
    /// A container (list, map, option) whose shape is public and whose elements hold `.0`.
    Holds(Box<WholeSrc>),
    /// A record, map or list by field name, key or position (`T { a: str(p) }`, `{ "a": p }`,
    /// `[x.id, x]`, an `enumerate` pair): reading one entry yields that entry. `WHOLE_SELF`: the
    /// record may itself be such a struct; `WHOLE_ANY`: what an entry the lane cannot name holds.
    Record(BTreeMap<String, WholeSrc>),
    /// A container of unknown depth (what kept growing), any part of which — at any depth — may be
    /// such a struct, or another such container. Its own shape is public.
    Deep(String),
}

impl WholeSrc {
    /// The diagnostic text.
    pub(super) fn text(&self) -> String {
        match self {
            Self::Value(t) | Self::Deep(t) => first_text(t),
            Self::Computed(t) => t.clone(),
            Self::Holds(x) => x.text(),
            Self::Record(fs) => fs
                .iter()
                .next()
                .map(|(f, s)| {
                    if f == WHOLE_SELF || f == WHOLE_ANY || f == WHOLE_ERR {
                        s.text()
                    } else {
                        format!("{} (in `{f}`)", s.text())
                    }
                })
                .unwrap_or_default(),
        }
    }

    fn is_computed(&self) -> bool {
        matches!(self, Self::Computed(_))
    }

    /// This value, computed from.
    fn computed(self) -> Self {
        match self {
            c @ Self::Computed(_) => c,
            other => Self::Computed(format!("{WHOLE_COMPUTED_SOURCE}{}", other.text())),
        }
    }

    fn holds(inner: Self) -> Self {
        Self::Holds(Box::new(inner))
    }

    /// An element as the runtime ITERATES the value (`for`, `map`, `flatten`, `first`): a list's
    /// elements, a map's keys (the entries stand in for them), a string's characters. A struct is
    /// not iterable (the runtime stops).
    fn elem(&self) -> Option<Self> {
        match self {
            Self::Value(_) => None,
            Self::Computed(_) | Self::Deep(_) => Some(self.clone()),
            Self::Holds(x) => Some((**x).clone()),
            Self::Record(fs) => join_all(
                fs.iter()
                    .filter(|(k, _)| k.as_str() != WHOLE_SELF)
                    .map(|(_, v)| Some(v.clone())),
            ),
        }
    }

    /// What a read at a key or position the lane cannot name yields: any element or entry — and, of
    /// a struct, any field by position, the secret one among them.
    fn index_elem(&self) -> Option<Self> {
        match self {
            Self::Value(_) | Self::Deep(_) => Some(self.clone().computed()),
            Self::Computed(_) => Some(self.clone()),
            Self::Holds(x) => Some((**x).clone()),
            Self::Record(fs) => join_all(fs.iter().map(|(k, v)| {
                if k == WHOLE_SELF {
                    v.index_elem()
                } else {
                    Some(v.clone())
                }
            })),
        }
    }

    /// A read at clean literal position `i` (`pair[1]`, `xs[0]`).
    fn at_pos(&self, i: &str) -> Option<Self> {
        match self {
            Self::Record(fs) => {
                let named = fs
                    .keys()
                    .any(|k| k != WHOLE_SELF && k != WHOLE_ANY && !is_position(k));
                join_all([
                    fs.get(i).cloned(),
                    fs.get(WHOLE_ANY).cloned(),
                    fs.get(WHOLE_SELF).and_then(Self::index_elem),
                    // A struct with named fields read by position reads one of them.
                    if named {
                        join_all(
                            fs.iter()
                                .filter(|(k, _)| k.as_str() != WHOLE_SELF)
                                .map(|(_, v)| Some(v.clone())),
                        )
                    } else {
                        None
                    },
                ])
            }
            other => other.index_elem(),
        }
    }

    /// A read at string literal key `k`: a map's entry, a struct's field of that name, or a list's
    /// element at the position the string converts to.
    fn at_key(&self, k: &str, ctx: &SemanticContext) -> Option<Self> {
        match self {
            Self::Record(fs) => {
                let positional = || {
                    join_all(
                        fs.iter()
                            .filter(|(x, _)| is_position(x))
                            .map(|(_, v)| Some(v.clone())),
                    )
                };
                join_all([
                    fs.get(&user_key(k)).cloned(),
                    fs.get(WHOLE_ANY).cloned(),
                    fs.get(WHOLE_SELF).and_then(|s| s.field(k, ctx)),
                    match list_pos(k) {
                        Some(p) => fs.get(&p).cloned(),
                        None => positional(),
                    },
                ])
            }
            Self::Value(_) => self.field(k, ctx),
            Self::Deep(t) => join(Some(self.clone()), Self::Value(t.clone()).field(k, ctx)),
            Self::Holds(x) => Some((**x).clone()),
            Self::Computed(_) => Some(self.clone()),
        }
    }

    /// A dot read `.f`: a struct's declared field (a nested struct with a secret field is one), a
    /// map's entry, a struct-variant enum's field. Anything else reads the default `0`.
    fn field(&self, f: &str, ctx: &SemanticContext) -> Option<Self> {
        match self {
            Self::Value(_) => match self.struct_types() {
                Some(tys) if !tys.is_empty() => {
                    join_all(tys.iter().map(|t| declared_field_whole(t, f, ctx)))
                }
                // A struct of a type the lane cannot name: any field.
                _ => Some(self.clone().computed()),
            },
            Self::Deep(t) => join(Some(self.clone()), Self::Value(t.clone()).field(f, ctx)),
            Self::Computed(_) => Some(self.clone()),
            Self::Holds(x) => Some((**x).clone()),
            Self::Record(fs) => join_all([
                fs.get(f).cloned(),
                fs.get(WHOLE_ANY).cloned(),
                fs.get(WHOLE_SELF).and_then(|s| s.field(f, ctx)),
            ]),
        }
    }

    /// What a shape-only read (`len`, `is_empty`, `keys`, `type`) reveals: only a computed value's.
    fn shape(&self) -> Option<Self> {
        self.is_computed().then(|| self.clone())
    }

    /// What `x?` yields when it does not return: an `Ok` / `Some` payload, or a user enum unchanged.
    fn try_value(&self) -> Option<Self> {
        match self {
            Self::Holds(x) => Some((**x).clone()),
            Self::Record(fs) => {
                let rest: BTreeMap<String, Self> = fs
                    .iter()
                    .filter(|(k, _)| k.as_str() != WHOLE_ERR && k.as_str() != WHOLE_SELF)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let named = rest.keys().any(|k| k != WHOLE_ANY && !is_position(k));
                join(
                    join_all(rest.values().map(|v| Some(v.clone()))),
                    (named && !rest.is_empty()).then(|| Self::Record(rest.clone())),
                )
            }
            Self::Value(_) => None,
            Self::Computed(_) | Self::Deep(_) => Some(self.clone()),
        }
    }

    /// What `x?` returns early: an `Err` (or `None`) as it is.
    fn err_part(&self) -> Option<Self> {
        match self {
            // With the names an `Err` in struct form carries (a name that is not a position).
            Self::Record(fs) => {
                join(fs.get(WHOLE_ERR).cloned(), fs.get(WHOLE_ANY).cloned()).map(|e| {
                    let mut out: BTreeMap<String, Self> = fs
                        .iter()
                        .filter(|(k, _)| {
                            k.as_str() != WHOLE_SELF
                                && k.as_str() != WHOLE_ANY
                                && k.as_str() != WHOLE_ERR
                                && !is_position(k)
                        })
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    out.insert(WHOLE_ERR.to_string(), e);
                    Self::Record(out)
                })
            }
            Self::Holds(x) if x.is_computed() => Some((**x).clone()),
            Self::Holds(_) | Self::Value(_) => None,
            Self::Computed(_) | Self::Deep(_) => Some(self.clone()),
        }
    }

    /// The same contents with positions forgotten (a builtin moved the elements).
    fn unpos(self) -> Self {
        match self {
            Self::Record(fs) if fs.keys().any(|k| is_position(k)) => {
                match join_all(fs.values().map(|v| Some(v.clone()))) {
                    Some(s) => Self::holds(s),
                    None => Self::Record(fs),
                }
            }
            other => other,
        }
    }

    /// The value, less the possibility that it is itself a struct (for a builtin that reads only
    /// lists and maps and returns its default for anything else).
    fn strip_struct(self) -> Option<Self> {
        match self {
            Self::Value(_) => None,
            Self::Record(mut fs) => {
                fs.remove(WHOLE_SELF);
                (!fs.is_empty()).then_some(Self::Record(fs))
            }
            other => Some(other),
        }
    }

    /// What keeps growing (a loop, fold or recursion wrapping a value each time): the top two levels
    /// kept, anything below them any depth deep ([`Self::Deep`]).
    fn widen(self) -> Self {
        self.widen_keep(2)
    }

    fn widen_keep(self, keep: u32) -> Self {
        match self {
            Self::Holds(x) if keep > 0 => Self::holds(x.widen_keep(keep - 1)),
            Self::Record(fs) if keep > 0 => Self::Record(
                fs.into_iter()
                    .map(|(k, v)| (k, v.widen_keep(keep - 1)))
                    .collect(),
            ),
            s @ (Self::Holds(_) | Self::Record(_)) => s.deep(),
            leaf => leaf,
        }
    }

    /// How many containers deep the value is.
    fn depth(&self) -> usize {
        match self {
            Self::Holds(x) => 1 + x.depth(),
            Self::Record(fs) => 1 + fs.values().map(Self::depth).max().unwrap_or(0),
            Self::Value(_) | Self::Computed(_) | Self::Deep(_) => 0,
        }
    }

    /// How many parts the value has.
    fn size(&self) -> usize {
        match self {
            Self::Holds(x) => 1 + x.size(),
            Self::Record(fs) => 1 + fs.values().map(Self::size).sum::<usize>(),
            Self::Value(_) | Self::Computed(_) | Self::Deep(_) => 1,
        }
    }

    /// The value, widened when it is deeper than `MAX_DEPTH` or larger than `MAX_SIZE`.
    fn capped(self) -> Self {
        if self.depth() > MAX_DEPTH || self.size() > MAX_SIZE {
            self.widen().deep_if_large()
        } else {
            self
        }
    }

    /// Still larger than `MAX_SIZE` after widening (a record with very many entries): as one of
    /// unknown depth.
    fn deep_if_large(self) -> Self {
        if self.size() > MAX_SIZE {
            self.deep()
        } else {
            self
        }
    }

    /// A container, as one of unknown depth holding what it holds anywhere. One that may itself be
    /// such a struct stays one (a method call on it runs that struct's impl), whatever it holds.
    fn deep(self) -> Self {
        match self.leaves() {
            Some(c @ Self::Computed(_)) => Self::maybe_self(self.self_part(), c),
            Some(Self::Value(t)) => Self::Deep(t),
            _ => self,
        }
    }

    /// The possibility that the value is itself such a struct.
    fn self_part(&self) -> Option<Self> {
        match self {
            Self::Value(_) => Some(self.clone()),
            Self::Deep(t) => Some(Self::Value(t.clone())),
            Self::Record(fs) => fs.get(WHOLE_SELF).cloned(),
            Self::Holds(_) | Self::Computed(_) => None,
        }
    }

    /// A value that may be the struct `me`, or a container whose entries hold `entries`.
    fn maybe_self(me: Option<Self>, entries: Self) -> Self {
        match me {
            Some(s) => Self::Record(
                [
                    (WHOLE_SELF.to_string(), s),
                    (WHOLE_ANY.to_string(), entries),
                ]
                .into_iter()
                .collect(),
            ),
            None => Self::holds(entries),
        }
    }

    /// Every struct or computed value inside, joined (a `Deep` contributes its structs).
    fn leaves(&self) -> Option<Self> {
        match self {
            Self::Value(_) | Self::Computed(_) => Some(self.clone()),
            Self::Deep(t) => Some(Self::Value(t.clone())),
            Self::Holds(x) => x.leaves(),
            Self::Record(fs) => join_all(fs.values().map(Self::leaves)),
        }
    }

    /// The struct types the value may be a struct of (`… (type `S`)`): empty when it is no struct,
    /// `None` when some source names no type.
    fn struct_types(&self) -> Option<BTreeSet<String>> {
        let t = match self {
            Self::Value(t) | Self::Deep(t) => t,
            Self::Record(fs) => {
                return fs
                    .get(WHOLE_SELF)
                    .map_or(Some(BTreeSet::new()), Self::struct_types)
            }
            _ => return Some(BTreeSet::new()),
        };
        t.split(TEXT_SEP)
            .map(|part| {
                let i = part.rfind("(type `")? + "(type `".len();
                let j = part[i..].find('`')? + i;
                let ty = part[i..j]
                    .split('<')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                (!ty.is_empty()).then_some(ty)
            })
            .collect()
    }
}

/// The first of a joined value's sources (the diagnostic names one).
fn first_text(t: &str) -> String {
    t.split(TEXT_SEP).next().unwrap_or("").to_string()
}

/// The sources of two joined values, each once, in a fixed order (so joins commute).
fn union_text(a: &str, b: &str) -> String {
    let parts: BTreeSet<&str> = a.split(TEXT_SEP).chain(b.split(TEXT_SEP)).collect();
    parts.into_iter().collect::<Vec<_>>().join(TEXT_SEP)
}

/// A key from the program, as a record's entry (escaped when it could be taken for one the lane
/// names itself).
fn user_key(k: &str) -> String {
    if k.starts_with('\u{1}') {
        format!("\u{1}{k}")
    } else {
        k.to_string()
    }
}

/// A clean non-negative integer position (`0`, `12`), as a literal index or a record's key.
fn is_position(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 9
        && k.bytes().all(|b| b.is_ascii_digit())
        && (k == "0" || !k.starts_with('0'))
}

/// The list position the runtime reads for string key `k` (`as_i64`: trimmed, an integer or a
/// truncated float, else 0), when it is a position the lane can name.
fn list_pos(k: &str) -> Option<String> {
    let t = k.trim();
    let i = t
        .parse::<i64>()
        .ok()
        .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
        .unwrap_or(0);
    let p = i.to_string();
    (i >= 0 && is_position(&p)).then_some(p)
}

fn join2(a: WholeSrc, b: WholeSrc) -> WholeSrc {
    use WholeSrc::*;
    match (a, b) {
        (c @ Computed(_), _) | (_, c @ Computed(_)) => c,
        (Value(a), Value(b)) => Value(union_text(&a, &b)),
        (Deep(a), Deep(b)) | (Deep(a), Value(b)) | (Value(b), Deep(a)) => Deep(union_text(&a, &b)),
        (Deep(a), other) | (other, Deep(a)) => match other.leaves() {
            // Either may itself be such a struct.
            Some(c @ Computed(_)) => {
                WholeSrc::maybe_self(join(Some(Value(a)), other.self_part()), c)
            }
            Some(Value(b)) => Deep(union_text(&a, &b)),
            _ => Deep(a),
        },
        (Holds(x), Holds(y)) => WholeSrc::holds(join2(*x, *y)),
        (Record(mut f), Record(g)) => {
            for (k, v) in g {
                let merged = match f.remove(&k) {
                    Some(o) => join2(o, v),
                    None => v,
                };
                f.insert(k, merged);
            }
            Record(f)
        }
        // A struct, or a container of `x`.
        (Value(t), Holds(x)) | (Holds(x), Value(t)) => Record(
            [
                (WHOLE_SELF.to_string(), Value(t)),
                (WHOLE_ANY.to_string(), *x),
            ]
            .into_iter()
            .collect(),
        ),
        (Value(t), Record(mut fs)) | (Record(mut fs), Value(t)) => {
            let merged = match fs.remove(WHOLE_SELF) {
                Some(o) => join2(o, Value(t)),
                None => Value(t),
            };
            fs.insert(WHOLE_SELF.to_string(), merged);
            Record(fs)
        }
        // A record, or a container: any entry may be an element.
        (Holds(x), Record(mut fs)) | (Record(mut fs), Holds(x)) => {
            let merged = match fs.remove(WHOLE_ANY) {
                Some(o) => join2(o, *x),
                None => *x,
            };
            fs.insert(WHOLE_ANY.to_string(), merged);
            Record(fs)
        }
    }
}

fn join(a: Option<WholeSrc>, b: Option<WholeSrc>) -> Option<WholeSrc> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(join2(a, b)),
    }
}

fn join_all(xs: impl IntoIterator<Item = Option<WholeSrc>>) -> Option<WholeSrc> {
    xs.into_iter().fold(None, join)
}

fn computed(s: Option<WholeSrc>) -> Option<WholeSrc> {
    s.map(WholeSrc::computed)
}

fn shape(s: Option<WholeSrc>) -> Option<WholeSrc> {
    s.and_then(|s| s.shape())
}

fn holds(s: Option<WholeSrc>) -> Option<WholeSrc> {
    s.map(WholeSrc::holds)
}

fn unpos(s: Option<WholeSrc>) -> Option<WholeSrc> {
    s.map(WholeSrc::unpos)
}

/// Record `src` on a binding's mark: the join, so marks only grow.
pub(super) fn whole_mark(mark: &mut Option<WholeSrc>, src: WholeSrc) {
    *mark = join(mark.take(), Some(src));
}

/// Forget the positions a binding's mark records (an in-place `remove` / `insert` shifted them).
pub(super) fn whole_unpos(mark: &mut Option<WholeSrc>) {
    *mark = mark.take().map(WholeSrc::unpos);
}

/// The join of two bindings' marks at a control-flow merge.
pub(super) fn whole_merge(a: &Option<WholeSrc>, b: &Option<WholeSrc>) -> Option<WholeSrc> {
    join(a.clone(), b.clone())
}

// ------------------------------------------------------------------------------------------------
// Names

/// A closure: its lambda (its identity is the address of the copy it holds, alive as long as the
/// closure is), what the free names it closes over hold, and whether its other free names fall back
/// to the caller's scope (a closure written in the caller) or not (one written in a callee).
#[derive(Clone, Debug)]
struct Closure {
    lam: Rc<Expr>,
    id: usize,
    captured: Rc<Locals>,
    use_scope: bool,
}

impl Closure {
    fn same(&self, other: &Closure) -> bool {
        self.id == other.id
            && self.use_scope == other.use_scope
            && (Rc::ptr_eq(&self.captured, &other.captured) || self.captured == other.captured)
    }
}

/// What a value is known to be from the shape of what was bound — never from a declared type,
/// which the checker does not enforce: a list, a map, and the struct type of a method's receiver.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Kind {
    list: bool,
    map: bool,
    ty: Option<Rc<str>>,
    /// `ty` is certain: the value is a struct literal of it, a method's own receiver, or a scope
    /// binding whose every `let` and assignment is known to keep it. Otherwise `ty` is only
    /// declared or inferred (not enforced: TY-UNENFORCED), and the value may still be anything a
    /// `declassify` released.
    sure: bool,
}

impl Kind {
    const NONE: Kind = Kind {
        list: false,
        map: false,
        ty: None,
        sure: false,
    };
    const LIST: Kind = Kind {
        list: true,
        map: false,
        ty: None,
        sure: false,
    };

    /// What is known on both of two paths.
    fn meet(&self, o: &Kind) -> Kind {
        Kind {
            list: self.list && o.list,
            map: self.map && o.map,
            ty: if self.ty == o.ty {
                self.ty.clone()
            } else {
                None
            },
            sure: self.sure && o.sure && self.ty == o.ty,
        }
    }
}

/// What a name bound inside interpreted code (a formal, a lambda parameter, a `let`, a binder)
/// stands for. Such names shadow the caller's scope.
#[derive(Clone, Debug)]
enum Local {
    /// A value holding this (`None`: nothing), and what it is known to be.
    Src(Option<WholeSrc>, Kind),
    Closure(Closure),
    /// A user function or builtin passed as a value.
    Named(String),
    /// Some function the lane stopped following (a closure chain that never settled): calling it
    /// computes from, and releases, what it can reach and its arguments.
    Opaque(Option<WholeSrc>),
    /// Any of these (a name bound differently on two paths).
    Any(Vec<Local>),
}

/// What a closure written in the caller's scope closes over, as it was when the closure was made
/// (the runtime copies every free local into the closure then).
#[derive(Clone, Debug)]
pub(super) struct Captures(Rc<Locals>);

type Locals = BTreeMap<String, Local>;

impl PartialEq for Local {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Local::Src(a, la), Local::Src(b, lb)) => a == b && la == lb,
            (Local::Closure(x), Local::Closure(y)) => x.same(y),
            (Local::Named(a), Local::Named(b)) => a == b,
            (Local::Opaque(a), Local::Opaque(b)) => a == b,
            (Local::Any(a), Local::Any(b)) => a == b,
            _ => false,
        }
    }
}

fn local_src(l: &Local) -> Option<WholeSrc> {
    match l {
        Local::Src(s, _) => s.clone(),
        Local::Any(ls) => join_all(ls.iter().map(local_src)),
        _ => None,
    }
}

fn local_kind(l: &Local) -> Kind {
    match l {
        Local::Src(_, k) => k.clone(),
        Local::Any(ls) if !ls.is_empty() => {
            let mut ks = ls.iter().map(local_kind);
            let first = ks.next().unwrap_or_default();
            ks.fold(first, |a, b| a.meet(&b))
        }
        _ => Kind::NONE,
    }
}

fn local_is_list(l: &Local) -> bool {
    local_kind(l).list
}

/// What a name may stand for after two paths join.
fn join_local(a: &Local, b: &Local) -> Local {
    match (a, b) {
        (Local::Src(x, la), Local::Src(y, lb)) => {
            Local::Src(join(x.clone(), y.clone()), la.meet(lb))
        }
        (Local::Opaque(x), Local::Opaque(y)) => Local::Opaque(join(x.clone(), y.clone())),
        _ if a == b => a.clone(),
        (Local::Any(xs), other) | (other, Local::Any(xs)) => {
            let mut v = xs.clone();
            match other {
                Local::Any(ys) => {
                    for y in ys {
                        if !v.contains(y) {
                            v.push(y.clone());
                        }
                    }
                }
                _ => {
                    if !v.contains(other) {
                        v.push(other.clone());
                    }
                }
            }
            Local::Any(v)
        }
        _ => Local::Any(vec![a.clone(), b.clone()]),
    }
}

fn join_locals(a: &Locals, b: &Locals) -> Locals {
    let mut out = a.clone();
    for (n, lb) in b {
        let v = match a.get(n) {
            Some(la) => join_local(la, lb),
            None => lb.clone(),
        };
        out.insert(n.clone(), v);
    }
    out
}

fn join_opt_locals(a: Option<Locals>, b: Option<Locals>) -> Option<Locals> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(join_locals(&a, &b)),
    }
}

/// How deeply a binding's closures close over closures.
fn closure_nest(l: &Local) -> usize {
    match l {
        Local::Closure(c) => 1 + c.captured.values().map(closure_nest).max().unwrap_or(0),
        Local::Any(ls) => ls.iter().map(closure_nest).max().unwrap_or(0),
        Local::Src(..) | Local::Named(_) | Local::Opaque(_) => 0,
    }
}

/// A binding that kept growing, widened: what it holds widened, and a closure (a chain of closures
/// wrapping each other never settles) replaced by an opaque function computing from what it can
/// reach.
fn widen_with(l: &Local, env: &Env) -> Local {
    match l {
        Local::Closure(_) => {
            let mut seen = BTreeSet::new();
            Local::Opaque(reach(l, env, &mut seen))
        }
        Local::Any(ls) => {
            let mut vals: Option<Local> = None;
            let mut fns: Option<Option<WholeSrc>> = None;
            let mut named: Vec<Local> = Vec::new();
            for x in ls.iter().map(|x| widen_with(x, env)) {
                match x {
                    Local::Opaque(r) => fns = Some(join(fns.flatten(), r)),
                    n @ Local::Named(_) => {
                        if !named.contains(&n) {
                            named.push(n);
                        }
                    }
                    other => {
                        vals = Some(match vals {
                            Some(v) => join_local(&v, &other),
                            None => other,
                        })
                    }
                }
            }
            let mut out: Vec<Local> = vals.into_iter().collect();
            out.extend(named);
            out.extend(fns.map(Local::Opaque));
            if out.len() == 1 {
                out.remove(0)
            } else {
                Local::Any(out)
            }
        }
        other => widen_local(other),
    }
}

/// A name's binding with what it holds widened (a loop or a recursive call kept growing it).
fn widen_local(l: &Local) -> Local {
    match l {
        Local::Src(s, k) => Local::Src(s.clone().map(WholeSrc::widen), k.clone()),
        Local::Any(ls) => Local::Any(ls.iter().map(widen_local).collect()),
        other => other.clone(),
    }
}

/// What a call returns, and where it makes a whole struct reach an egress.
#[derive(Clone, Debug, Default, PartialEq)]
struct FnFx {
    ret: Option<WholeSrc>,
    egress: Option<WholeSrc>,
}

/// Which half of a call's effect a caller needs (a builtin's two halves are computed apart, so a
/// nested argument is not evaluated twice per level).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    Ret,
    Egress,
}

/// Per function in one query: specializations begun (in all, and per the lambdas its closure
/// arguments are made of), the bindings its shared specialization stands for, and how many times
/// they changed.
#[derive(Default)]
struct RouteState {
    count: usize,
    by_lambdas: BTreeMap<Vec<usize>, usize>,
    shared: Vec<Local>,
    changes: usize,
}

/// The lambdas a call's closure arguments are made of (their captures aside).
fn lambdas_of(binds: &[Local]) -> Vec<usize> {
    fn ids(l: &Local, out: &mut Vec<usize>) {
        match l {
            Local::Closure(c) => out.push(c.id),
            Local::Any(ls) => ls.iter().for_each(|x| ids(x, out)),
            Local::Src(..) | Local::Named(_) | Local::Opaque(_) => {}
        }
    }
    let mut out = Vec::new();
    binds.iter().for_each(|b| ids(b, &mut out));
    out.sort_unstable();
    out
}

/// A call being specialized: its route (the function, or a method with the impls it runs), its key,
/// the running bindings of its formals (a recursive call joins its own into them), and how many
/// times they grew.
struct Active {
    route: String,
    key: String,
    binds: Vec<Local>,
    grew: usize,
}

/// A statement list (its address and length), the names it runs from (their address; the entry
/// keeps them alive), and the rest of the position.
type BlockKey = (usize, usize, usize, usize, bool, u32, u8);

/// A position made by binding a pattern: the base names (kept alive), what the scrutinee held, and
/// the position.
type BinderAt = (Rc<Locals>, Option<WholeSrc>, At);

/// A result computed from running approximations: the epoch, the result, and the calls it read.
type Tentative = (u64, FnFx, Vec<String>);

/// A remembered closure application: the arguments, the epoch, the calls it read, the application
/// depth when a limit shaped it, and the result.
struct Applied {
    binds: Vec<Local>,
    epoch: u64,
    deps: Vec<String>,
    limited_at: Option<u32>,
    fx: FnFx,
}

/// The environment of one query: the caller's scope, the context, and the query's caches.
struct Env<'a> {
    ctx: &'a SemanticContext,
    scope: &'a BTreeMap<String, ScopeBinding>,
    /// Finished specializations (by key, or by key and call depth when a limit shaped them), and
    /// those in progress with their running approximation.
    done: RefCell<BTreeMap<String, FnFx>>,
    active: RefCell<BTreeMap<String, FnFx>>,
    /// The keys whose running approximation was read, in order.
    reads: RefCell<Vec<String>>,
    calls: RefCell<Vec<Active>>,
    /// Per function: see [`RouteState`].
    per_route: RefCell<BTreeMap<String, RouteState>>,
    work: Cell<usize>,
    steps: Cell<usize>,
    /// How many times a limit made the lane assume the worst.
    fallbacks: Cell<usize>,
    /// Bumped whenever the running approximation of a call in progress grows. A result computed from
    /// such an approximation is kept only for the epoch it was computed in, with the calls it read.
    epoch: Cell<u64>,
    tentative: RefCell<BTreeMap<String, Tentative>>,
    blocks: RefCell<BTreeMap<BlockKey, (Rc<Locals>, Ran)>>,
    binder_ats: RefCell<BTreeMap<(usize, usize), Vec<BinderAt>>>,
    /// Closure applications made, by the closure's number.
    applies: RefCell<BTreeMap<usize, Vec<Applied>>>,
    /// Every lambda a closure was made of, by its node's address (so no address is reused while the
    /// query runs); closures numbered by identity and captures; lambdas' free names.
    lams: RefCell<BTreeMap<usize, Rc<Expr>>>,
    interned: RefCell<BTreeMap<usize, Vec<(usize, Closure)>>>,
    next_closure: Cell<usize>,
    free: RefCell<BTreeMap<usize, Rc<BTreeSet<String>>>>,
    /// When recording (`loop_carried`): what every name held at the head of any iteration of the
    /// outermost loop; and how many loops deep the interpretation is.
    heads: RefCell<Option<Locals>>,
    loops: Cell<usize>,
}

impl<'a> Env<'a> {
    fn new(ctx: &'a SemanticContext, scope: &'a BTreeMap<String, ScopeBinding>) -> Self {
        Self {
            ctx,
            scope,
            done: RefCell::default(),
            active: RefCell::default(),
            reads: RefCell::default(),
            calls: RefCell::default(),
            per_route: RefCell::default(),
            work: Cell::new(0),
            steps: Cell::new(0),
            fallbacks: Cell::new(0),
            epoch: Cell::new(0),
            tentative: RefCell::default(),
            blocks: RefCell::default(),
            binder_ats: RefCell::default(),
            applies: RefCell::default(),
            lams: RefCell::default(),
            interned: RefCell::default(),
            next_closure: Cell::new(0),
            free: RefCell::default(),
            heads: RefCell::default(),
            loops: Cell::new(0),
        }
    }

    fn is_user_fn(&self, n: &str) -> bool {
        self.ctx.whole_fns.contains_key(n)
    }

    /// Whether the query has used up its steps (and counts one more).
    fn exhausted(&self) -> bool {
        self.steps.set(self.steps.get() + 1);
        self.steps.get() > MAX_STEPS
    }

    /// Record the names at the head of a loop iteration (`loop_carried`): only a loop of the scope
    /// itself, not one inside a function or closure it calls (whose names are its own).
    fn note_head(&self, at: &At) {
        if at.depth != 0 || !at.use_scope || self.loops.get() != 1 {
            return;
        }
        if let Some(h) = self.heads.borrow_mut().as_mut() {
            *h = join_locals(h, &at.locals);
        }
    }

    fn fell_back(&self) {
        self.fallbacks.set(self.fallbacks.get() + 1);
    }

    /// The calls in progress whose approximation was read since `start` (other than `own`).
    fn deps_since(&self, start: usize, own: Option<&str>) -> Vec<String> {
        let active = self.active.borrow();
        let mut v: Vec<String> = self.reads.borrow()[start..]
            .iter()
            .filter(|k| Some(k.as_str()) != own && active.contains_key(*k))
            .cloned()
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// A closure over `lam` (a node of the program, or of a lambda this query holds), closing over
    /// what its free names hold in `locals`.
    fn closure(&self, lam: &Expr, locals: &Locals, use_scope: bool) -> Local {
        let rc = self
            .lams
            .borrow_mut()
            .entry(lam as *const Expr as usize)
            .or_insert_with(|| Rc::new(lam.clone()))
            .clone();
        let id = Rc::as_ptr(&rc) as usize;
        let free = self.free_names(id, &rc);
        let captured: Locals = locals
            .iter()
            .filter(|(n, _)| free.contains(*n))
            .map(|(n, l)| {
                let l = if closure_nest(l) >= MAX_CLOSURE_NEST {
                    widen_with(l, self)
                } else {
                    l.clone()
                };
                (n.clone(), l)
            })
            .collect();
        Local::Closure(Closure {
            lam: rc,
            id,
            captured: Rc::new(captured),
            use_scope,
        })
    }

    fn free_names(&self, id: usize, lam: &Expr) -> Rc<BTreeSet<String>> {
        if let Some(f) = self.free.borrow().get(&id) {
            return f.clone();
        }
        let f = Rc::new(free_names(lam));
        self.free.borrow_mut().insert(id, f.clone());
        f
    }

    /// A closure's number: the same lambda closing over the same values gets the same one.
    fn intern(&self, c: &Closure) -> usize {
        if let Some(n) = self
            .interned
            .borrow()
            .get(&c.id)
            .and_then(|v| v.iter().find(|(_, x)| x.same(c)).map(|(n, _)| *n))
        {
            return n;
        }
        let n = self.next_closure.get();
        self.next_closure.set(n + 1);
        self.interned
            .borrow_mut()
            .entry(c.id)
            .or_default()
            .push((n, c.clone()));
        n
    }
}

/// The interpreter's position: the names bound here, whether a name not among them falls back to the
/// caller's scope, and how many closure applications deep it is.
#[derive(Clone)]
struct At {
    locals: Rc<Locals>,
    use_scope: bool,
    depth: u32,
}

impl At {
    fn with(&self, extra: impl IntoIterator<Item = (String, Local)>) -> Self {
        let mut l = (*self.locals).clone();
        l.extend(extra);
        self.with_locals(l)
    }

    /// These names here (the same position when they are what it already has).
    fn with_locals(&self, locals: Locals) -> Self {
        if *self.locals == locals {
            return self.clone();
        }
        self.with_locals_rc(Rc::new(locals))
    }

    fn with_locals_rc(&self, locals: Rc<Locals>) -> Self {
        Self {
            locals,
            ..self.clone()
        }
    }
}

fn top() -> At {
    At {
        locals: Rc::new(Locals::new()),
        use_scope: true,
        depth: 0,
    }
}

fn binders_at(
    env: &Env,
    at: &At,
    p: &crate::frontend::Pattern,
    s: &Option<WholeSrc>,
    whole: &Local,
) -> At {
    // A binder that takes the whole scrutinee (`match f { g => g() }`, `let (g) = f`) is what the
    // scrutinee is, a function value included.
    if let Some(b) = whole_binder(p) {
        return at.with([(b.to_string(), whole.clone())]);
    }
    let key = (p as *const _ as usize, Rc::as_ptr(&at.locals) as usize);
    if let Some(hit) = env.binder_ats.borrow().get(&key).and_then(|v| {
        v.iter()
            .find(|(base, src, a)| {
                Rc::ptr_eq(base, &at.locals)
                    && src == s
                    && a.use_scope == at.use_scope
                    && a.depth == at.depth
            })
            .map(|(_, _, a)| a.clone())
    }) {
        return hit;
    }
    let made = at.with(
        pattern_binders(p, s, env.ctx)
            .into_iter()
            .map(|(n, b)| (n, Local::Src(b, Kind::NONE))),
    );
    env.binder_ats.borrow_mut().entry(key).or_default().push((
        at.locals.clone(),
        s.clone(),
        made.clone(),
    ));
    made
}

/// The name a pattern binds to the whole value it matches: a bare binder (`g`; the parser reads
/// `(g)` as one). `[g]` and `(g,)` bind the element of a one-element list.
fn whole_binder(p: &crate::frontend::Pattern) -> Option<&str> {
    match p {
        crate::frontend::Pattern::Binding(b) => Some(b),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------
// Entry points

/// What `expr` holds in `scope` (see `ScopeBinding::whole_struct`). Consulted where a value leaves the
/// program and for bindings; ordinary secret propagation never sees it.
pub(super) fn whole_value_source(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<WholeSrc> {
    let env = Env::new(ctx, scope);
    let v = if has_writes(expr) {
        // A read after a write inside the expression (`[push(t, p), t]`) sees it.
        let at = seeded(expr, &env);
        let w = walk(expr, &env, &at, &None);
        src(expr, &env, &w.view())
    } else {
        src(expr, &env, &top())
    };
    v.map(WholeSrc::capped)
}

/// The caller's position with every variable `e` may write bound to what it holds now, so a state
/// where the write has not happened joins with its value rather than with nothing.
/// The names a statement list writes: assignments' roots, anywhere in it.
fn stmt_roots(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            Stmt::Assign { target, .. } => {
                if let Some(r) = place_root(target) {
                    out.insert(r.to_string());
                }
            }
            Stmt::If { then, else_, .. } => {
                stmt_roots(then, out);
                if let Some(e) = else_ {
                    stmt_roots(e, out);
                }
            }
            Stmt::While { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => stmt_roots(body, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    stmt_roots(b, out);
                }
            }
            Stmt::Let { .. }
            | Stmt::LetPattern { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Break
            | Stmt::Continue
            | Stmt::SpecBlock { .. } => {}
        }
    }
}

fn seeded(e: &Expr, env: &Env) -> At {
    let mut names = BTreeSet::new();
    visit::each_expr(e, &mut |x| match x {
        Expr::Block { stmts, .. } => stmt_roots(stmts, &mut names),
        Expr::Call { callee, args } if IN_PLACE.contains(&callee.as_str()) => {
            if let Some(Expr::Var(n)) = args.first() {
                names.insert(n.clone());
            }
        }
        _ => {}
    });
    let base = top();
    let bound: Vec<(String, Local)> = names
        .into_iter()
        .filter_map(|n| resolve_local(&n, env, &base).map(|l| (n, l)))
        .collect();
    base.with(bound)
}

/// What each name of `scope` written by the loop statement `stmt` may hold at the head of any of its
/// iterations. The statement walk in `mod.rs` analyzes a loop body once, so a value that reaches a
/// name late in one iteration and is read early in the next is seeded from here.
pub(super) fn loop_carried(
    stmt: &Stmt,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Vec<(String, Option<WholeSrc>)> {
    let env = Env::new(ctx, scope);
    let stmts = std::slice::from_ref(stmt);
    let mut names = BTreeSet::new();
    stmt_roots(stmts, &mut names);
    visit::each_expr_in_stmts(stmts, &mut |x| match x {
        Expr::Block { stmts, .. } => stmt_roots(stmts, &mut names),
        Expr::Call { callee, args } if IN_PLACE.contains(&callee.as_str()) => {
            if let Some(Expr::Var(n)) = args.first() {
                names.insert(n.clone());
            }
        }
        _ => {}
    });
    let base = top();
    let bound: Vec<(String, Local)> = names
        .into_iter()
        .filter_map(|n| resolve_local(&n, &env, &base).map(|l| (n, l)))
        .collect();
    let at = base.with(bound);
    *env.heads.borrow_mut() = Some(Locals::new());
    let _ = interp_uncached(stmts, None, &env, &at, Run::LOOP_BODY);
    let heads = env.heads.borrow_mut().take().unwrap_or_default();
    heads
        .into_iter()
        .filter(|(n, _)| scope.contains_key(n))
        .map(|(n, l)| (n, local_src(&l)))
        .collect()
}

/// Whether `expr` is known to be a list, from its shape (never from a declared type).
pub(super) fn is_list_expr(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> bool {
    let env = Env::new(ctx, scope);
    is_listish(expr, &env, &top())
}

/// Whether `expr` is known to be a map, from its shape.
pub(super) fn is_map_expr(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> bool {
    let env = Env::new(ctx, scope);
    is_mapish(expr, &env, &top())
}

/// The names a function body ever assigns a value not known, from its text, to stay a list (resp. a
/// map) — `t = "ab"`, `a = b` — anywhere, in a loop or a branch, under any shadowing. A binding of
/// such a name is never taken to be a list (map): the caller's statement-level analysis neither
/// iterates loops nor keeps a branch's write to a name it then shadows.
/// What may make a name's declared or inferred struct type stale (`assigned_shapes`), read by
/// `stale_type` when that type is about to decide a method call.
#[derive(Debug, Default)]
pub(super) struct Retype {
    /// The values plainly assigned to each name.
    assigned: BTreeMap<String, Vec<Expr>>,
    /// The parts written into, per name: a field's name, or `WHOLE_ANY` for a position or an
    /// in-place write (any part). A written part's declared or inferred type may be stale; the
    /// name's own type is not.
    written: BTreeMap<String, BTreeSet<String>>,
    /// What each name is bound from (a `let`, a pattern, a loop variable, an assignment): the names
    /// its type or its parts' types are read from (`type_roots`), and how.
    bound: BTreeMap<String, Vec<(String, Via)>>,
    /// Names whose parts' types may change, whatever their own type: written into, assigned, or
    /// bound from one of these.
    changes: BTreeSet<String>,
    /// The initial value of every `let` of each name.
    inits: BTreeMap<String, Vec<Expr>>,
    /// Which functions and methods return their own type (asked again at every read of a name
    /// assigned by one): per function and type, the formals whose arguments must keep it (`None`:
    /// it does not return one); per method and type, whether it does.
    fn_own: RefCell<BTreeMap<(String, String), OwnFormals>>,
    method_own: RefCell<BTreeMap<(String, String), bool>>,
}

/// Whether a function returns its own type for a given type: the formals whose arguments must keep
/// it, or `None` when it does not return one.
type OwnFormals = Option<Vec<usize>>;

/// How a bound name's type is read from another name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Via {
    /// It is that name's value (`let y = x`, a branch of one).
    Whole,
    /// It is a part of that name's value (`x.f`, `x[i]`, a method's or a builtin's result on it, a
    /// pattern binder or loop variable over it).
    Part,
    /// Its parts are built from that name's value (`[x]`, `W { f: x }`); its own type is not.
    Element,
}

/// Whether `n`'s declared or inferred struct type `ty` may not be what it holds: it is assigned a
/// value not known to be a `ty` (`keeps_type`), or bound from a name whose type may be stale.
fn stale_type(n: &str, ty: &str, env: &Env) -> bool {
    stale_type_in(n, ty, env, &mut Vec::new())
}

/// Whether `n` surely holds a `ty`: every `let` of it and every assignment to it is known to keep
/// the type (a literal of it, itself, a method or function returning one), and nothing it is bound
/// from may be stale.
fn sure_type(n: &str, ty: &str, env: &Env) -> bool {
    env.ctx.whole_retype.inits.get(n).is_some_and(|inits| {
        !inits.is_empty()
            && inits
                .iter()
                .all(|i| keeps_type(i, n, ty, env.ctx, 0) && !matches!(i, Expr::Var(x) if x == n))
    }) && !stale_type(n, ty, env)
}

/// The declared or inferred struct type of a scope binding.
fn scope_type(n: &str, env: &Env) -> Option<String> {
    env.scope
        .get(n)
        .and_then(|b| b.info.ty.as_deref())
        .map(|t| t.split('<').next().unwrap_or("").trim().to_string())
        .filter(|t| env.ctx.struct_fields.contains_key(t))
}

/// Whether the parts of `root` may not be what their declared types say: it is written into, or its
/// own type may be stale (when it has none known, whether it changes at all).
fn parts_stale(root: &str, env: &Env, visiting: &mut Vec<String>) -> bool {
    let r = &env.ctx.whole_retype;
    r.written.contains_key(root)
        || match scope_type(root, env) {
            Some(t) => stale_type_in(root, &t, env, visiting),
            None => r.changes.contains(root),
        }
}

fn stale_type_in(n: &str, ty: &str, env: &Env, visiting: &mut Vec<String>) -> bool {
    let ctx = env.ctx;
    // A cycle of bindings reads no type from outside it; one too long to follow may be stale.
    if visiting.iter().any(|v| v == n) {
        return false;
    }
    if visiting.len() > 64 {
        return true;
    }
    visiting.push(n.to_string());
    let r = &ctx.whole_retype;
    let stale = r
        .assigned
        .get(n)
        .is_some_and(|vs| vs.iter().any(|v| !keeps_type(v, n, ty, ctx, 0)))
        || r.bound.get(n).is_some_and(|roots| {
            roots.iter().any(|(root, via)| match via {
                Via::Whole => stale_type_in(root, ty, env, visiting),
                Via::Part => parts_stale(root, env, visiting),
                Via::Element => false,
            })
        });
    visiting.pop();
    stale
}

/// Whether the declared type of the part `seg` (a field's name, `WHOLE_ANY` for a position) of `n`
/// may be stale: that part, or any, is written into; `n`'s own type (`own`, when known) may be; or
/// its parts were built from a name that changes.
fn stale_part(n: &str, seg: &str, own: Option<&str>, env: &Env) -> bool {
    let r = &env.ctx.whole_retype;
    r.written.get(n).is_some_and(|ws| {
        ws.contains(WHOLE_ANY) || ws.contains(seg) || (seg == WHOLE_ANY && !ws.is_empty())
    }) || match own {
        Some(t) => stale_type(n, t, env),
        None => r.changes.contains(n),
    } || r.bound.get(n).is_some_and(|roots| {
        roots
            .iter()
            .any(|(root, via)| *via == Via::Element && parts_stale(root, env, &mut Vec::new()))
    })
}

/// The part of its root a place reads first (`x.f.g`: `f`; `x[i]`: `WHOLE_ANY`).
fn first_segment(place: &Expr) -> Option<String> {
    match place {
        Expr::FieldAccess { base, field, .. } => match base.as_ref() {
            Expr::Var(_) => Some(field.clone()),
            b => first_segment(b),
        },
        Expr::Index { base, .. } => match base.as_ref() {
            Expr::Var(_) => Some(WHOLE_ANY.to_string()),
            b => first_segment(b),
        },
        _ => None,
    }
}

/// The names a value's struct type (or its parts' types) is read from, and how (`Via`). A struct
/// literal's own type, or a declared function result's, is read from no name.
fn type_roots(v: &Expr, ctx: &SemanticContext, via: Via, out: &mut Vec<(String, Via)>) {
    let part = via.max(Via::Part);
    match v {
        Expr::Var(x) => out.push((x.clone(), via)),
        Expr::FieldAccess { base, .. } | Expr::Index { base, .. } => {
            type_roots(base, ctx, part, out)
        }
        Expr::CallExpr { callee, .. } => {
            if let Expr::FieldAccess { base, .. } = callee.as_ref() {
                type_roots(base, ctx, part, out);
            }
        }
        // A builtin may return an element; a user function without a declared result may return
        // what it is given.
        Expr::Call { callee, args } => {
            let user = ctx.whole_fns.contains_key(callee);
            if !user || !ctx.fn_ret_types.contains_key(callee) {
                for a in args {
                    type_roots(a, ctx, part, out);
                }
            }
        }
        // A literal's parts are what it is built from.
        Expr::ArrayLiteral { elements: xs } | Expr::EnumConstruct { fields: xs, .. } => {
            for x in xs {
                type_roots(x, ctx, Via::Element, out);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                type_roots(x, ctx, Via::Element, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (_, x) in entries {
                type_roots(x, ctx, Via::Element, out);
            }
        }
        Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
            type_roots(then, ctx, via, out);
            type_roots(else_, ctx, via, out);
        }
        Expr::Match { arms, .. } => {
            for a in arms {
                type_roots(&a.body, ctx, via, out);
            }
        }
        Expr::Block { tail: Some(t), .. } => type_roots(t, ctx, via, out),
        Expr::Block { stmts, tail: None } => {
            let mut tails = Vec::new();
            tail_values(stmts, &mut tails);
            for t in tails.into_iter().flatten() {
                type_roots(&t, ctx, via, out);
            }
        }
        // A cast to a type the runtime does not convert to leaves the value as it is.
        Expr::Cast { expr: inner, .. }
        | Expr::Declassify { inner, .. }
        | Expr::Try(inner)
        | Expr::Tainted { inner, .. } => type_roots(inner, ctx, via, out),
        _ => {}
    }
}

/// Whether `v`, assigned to `n`, is known to be a `ty`: a literal of it, `n` itself, a method of
/// `ty` on `n` that returns one, or a function whose every return is one (or a formal given one).
fn keeps_type(v: &Expr, n: &str, ty: &str, ctx: &SemanticContext, depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    let binds_n = |p: &crate::frontend::Pattern| p.bound_names().iter().any(|b| b == n);
    match v {
        Expr::StructLiteral { name, .. } => name.split('<').next().unwrap_or("").trim() == ty,
        Expr::Var(x) => x == n,
        Expr::If { then, else_, .. } => {
            keeps_type(then, n, ty, ctx, depth) && keeps_type(else_, n, ty, ctx, depth)
        }
        Expr::IfLet {
            pattern,
            then,
            else_,
            ..
        } => {
            !binds_n(pattern)
                && keeps_type(then, n, ty, ctx, depth)
                && keeps_type(else_, n, ty, ctx, depth)
        }
        Expr::Match { arms, .. } => arms
            .iter()
            .all(|a| !binds_n(&a.pattern) && keeps_type(&a.body, n, ty, ctx, depth)),
        // A block that binds `n` again yields that `n`, not the outer one.
        Expr::Block { stmts, tail } => {
            if rebinds(stmts, n) {
                return false;
            }
            match tail {
                Some(t) => keeps_type(t, n, ty, ctx, depth),
                None => {
                    let mut tails = Vec::new();
                    tail_values(stmts, &mut tails);
                    !tails.is_empty()
                        && tails.iter().all(|t| {
                            t.as_ref()
                                .is_some_and(|t| keeps_type(t, n, ty, ctx, depth + 1))
                        })
                }
            }
        }
        Expr::Declassify { inner, .. } => keeps_type(inner, n, ty, ctx, depth),
        Expr::CallExpr { callee, .. } => match callee.as_ref() {
            Expr::FieldAccess { base, field, .. } if matches!(base.as_ref(), Expr::Var(x) if x == n) => {
                method_returns_own(ty, field, ctx)
            }
            _ => false,
        },
        // `state = step(state)`: a function every return of which is a `ty` (a literal, or a
        // formal, never bound again in its body, given one).
        Expr::Call { callee, args } => match fn_returns_own(callee, ty, ctx) {
            Some(formals) => formals.iter().all(|i| {
                args.get(*i)
                    .is_some_and(|a| keeps_type(a, n, ty, ctx, depth + 1))
            }),
            None => false,
        },
        _ => false,
    }
}

/// Per function and type (memoized): `Some(formals)` when every value the function returns is a
/// `ty` literal, or one of those formals as given (never bound again in the body), or a call of
/// such a function on them; `None` otherwise.
fn fn_returns_own(f: &str, ty: &str, ctx: &SemanticContext) -> Option<Vec<usize>> {
    let key = (f.to_string(), ty.to_string());
    if let Some(v) = ctx.whole_retype.fn_own.borrow().get(&key) {
        return v.clone();
    }
    // Recursion: while this is being decided, it does not return its own type.
    ctx.whole_retype
        .fn_own
        .borrow_mut()
        .insert(key.clone(), None);
    let result = ctx.whole_fns.get(f).and_then(|(params, body)| {
        let rets = body_returns(body);
        if rets.is_empty() {
            return None;
        }
        let mut need: BTreeSet<usize> = BTreeSet::new();
        for r in &rets {
            own_value(r.as_ref()?, params, body, ty, ctx, &mut need, 0)?;
        }
        Some(need.into_iter().collect())
    });
    ctx.whole_retype
        .fn_own
        .borrow_mut()
        .insert(key, result.clone());
    result
}

/// Whether a value a function returns is a `ty` (adding to `need` the formals whose arguments must
/// be one); `None`: not known to be.
fn own_value(
    e: &Expr,
    params: &[String],
    body: &[Stmt],
    ty: &str,
    ctx: &SemanticContext,
    need: &mut BTreeSet<usize>,
    depth: u32,
) -> Option<()> {
    if depth > 8 {
        return None;
    }
    let formal = |x: &str| {
        params
            .iter()
            .position(|p| p == x)
            .filter(|_| !rebinds(body, x))
    };
    match e {
        Expr::StructLiteral { name, .. } => {
            (name.split('<').next().unwrap_or("").trim() == ty).then_some(())
        }
        Expr::Var(x) => {
            need.insert(formal(x)?);
            Some(())
        }
        Expr::If { then, else_, .. } => {
            own_value(then, params, body, ty, ctx, need, depth + 1)?;
            own_value(else_, params, body, ty, ctx, need, depth + 1)
        }
        Expr::Block {
            stmts,
            tail: Some(t),
        } if stmts.is_empty() => own_value(t, params, body, ty, ctx, need, depth + 1),
        Expr::Declassify { inner, .. } => own_value(inner, params, body, ty, ctx, need, depth + 1),
        // A `match` / `if let` whose every value is one, binding no formal again.
        Expr::Match { arms, .. } => {
            for a in arms {
                if a.pattern.bound_names().iter().any(|b| params.contains(b)) {
                    return None;
                }
                own_value(&a.body, params, body, ty, ctx, need, depth + 1)?;
            }
            Some(())
        }
        Expr::IfLet {
            pattern,
            then,
            else_,
            ..
        } => {
            if pattern.bound_names().iter().any(|b| params.contains(b)) {
                return None;
            }
            own_value(then, params, body, ty, ctx, need, depth + 1)?;
            own_value(else_, params, body, ty, ctx, need, depth + 1)
        }
        // A call of such a function: each formal it needs is given a literal of `ty` or a formal
        // of this one.
        Expr::Call { callee, args } => {
            for i in fn_returns_own(callee, ty, ctx)? {
                match args.get(i)? {
                    Expr::StructLiteral { name, .. }
                        if name.split('<').next().unwrap_or("").trim() == ty => {}
                    Expr::Var(x) => {
                        need.insert(formal(x)?);
                    }
                    _ => return None,
                }
            }
            Some(())
        }
        _ => None,
    }
}

/// Whether every value `ty`'s method `m` returns is a `ty` (a literal, `self` never bound again in
/// the body, or another such method of `self`). Memoized.
fn method_returns_own(ty: &str, m: &str, ctx: &SemanticContext) -> bool {
    let key = (m.to_string(), ty.to_string());
    if let Some(v) = ctx.whole_retype.method_own.borrow().get(&key) {
        return *v;
    }
    ctx.whole_retype
        .method_own
        .borrow_mut()
        .insert(key.clone(), false);
    let result = ctx.whole_methods.get(m).is_some_and(|impls| {
        let bodies: Vec<&Vec<Stmt>> = impls
            .iter()
            .filter(|(t, _)| t == ty)
            .map(|(_, (_, body))| body)
            .collect();
        !bodies.is_empty()
            && bodies.iter().all(|body| {
                let out = body_returns(body);
                let self_ok = !rebinds(body, "self");
                !out.is_empty()
                    && out.iter().all(|r| {
                        r.as_ref()
                            .is_some_and(|e| own_method_value(e, ty, self_ok, ctx, 0))
                    })
            })
    });
    ctx.whole_retype.method_own.borrow_mut().insert(key, result);
    result
}

fn own_method_value(e: &Expr, ty: &str, self_ok: bool, ctx: &SemanticContext, depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    match e {
        Expr::StructLiteral { name, .. } => name.split('<').next().unwrap_or("").trim() == ty,
        Expr::Var(x) => self_ok && x == "self",
        Expr::If { then, else_, .. } => {
            own_method_value(then, ty, self_ok, ctx, depth + 1)
                && own_method_value(else_, ty, self_ok, ctx, depth + 1)
        }
        Expr::Match { arms, .. } => arms.iter().all(|a| {
            !a.pattern.bound_names().iter().any(|b| b == "self")
                && own_method_value(&a.body, ty, self_ok, ctx, depth + 1)
        }),
        Expr::IfLet {
            pattern,
            then,
            else_,
            ..
        } => {
            !pattern.bound_names().iter().any(|b| b == "self")
                && own_method_value(then, ty, self_ok, ctx, depth + 1)
                && own_method_value(else_, ty, self_ok, ctx, depth + 1)
        }
        // A free function every return of which is one, given literals of the type or `self`.
        Expr::Call { callee, args } => fn_returns_own(callee, ty, ctx).is_some_and(|need| {
            need.iter().all(|i| match args.get(*i) {
                Some(Expr::StructLiteral { name, .. }) => {
                    name.split('<').next().unwrap_or("").trim() == ty
                }
                Some(Expr::Var(x)) => self_ok && x == "self",
                _ => false,
            })
        }),
        Expr::Block { stmts, tail } => {
            if rebinds(stmts, "self") {
                return false;
            }
            match tail {
                Some(t) => own_method_value(t, ty, self_ok, ctx, depth + 1),
                None => {
                    let mut tails = Vec::new();
                    tail_values(stmts, &mut tails);
                    !tails.is_empty()
                        && tails.iter().all(|t| {
                            t.as_ref()
                                .is_some_and(|t| own_method_value(t, ty, self_ok, ctx, depth + 1))
                        })
                }
            }
        }
        Expr::Declassify { inner, .. } => own_method_value(inner, ty, self_ok, ctx, depth + 1),
        Expr::CallExpr { callee, .. } => match callee.as_ref() {
            Expr::FieldAccess { base, field, .. }
                if self_ok && matches!(base.as_ref(), Expr::Var(x) if x == "self") =>
            {
                method_returns_own(ty, field, ctx)
            }
            _ => false,
        },
        _ => false,
    }
}

/// What a body may return: every `return`'s value, wherever it sits (a `match` arm, an `if`
/// expression, a value block) except inside a closure, and what it yields when it falls off its
/// end (`None`: the default, or a value this scan does not follow).
fn body_returns(body: &[Stmt]) -> Vec<Option<Expr>> {
    // A `return` inside a closure returns from the closure.
    let mut in_closures: BTreeSet<usize> = BTreeSet::new();
    visit::each_expr_in_stmts(body, &mut |e| {
        if let Expr::Lambda { body: lb, .. } = e {
            visit::each_expr(lb, &mut |x| {
                if matches!(x, Expr::Call { callee, .. } if callee == "return") {
                    in_closures.insert(x as *const Expr as usize);
                }
            });
        }
    });
    let mut out = Vec::new();
    visit::each_expr_in_stmts(body, &mut |e| {
        if let Expr::Call { callee, args } = e {
            if callee == "return" && !in_closures.contains(&(e as *const Expr as usize)) {
                out.push(args.first().cloned());
            }
        }
    });
    tail_values(body, &mut out);
    out
}

/// What a statement list yields when it falls off its end (`split_tail_expr`): its last bare
/// expression, or both branches of a last `if … else`; `None` for anything else (the default).
fn tail_values(stmts: &[Stmt], out: &mut Vec<Option<Expr>>) {
    match stmts.last() {
        Some(Stmt::ExprStmt(Expr::Call { callee, .. })) if callee == "return" => {}
        Some(Stmt::ExprStmt(Expr::Call { callee, .. })) if is_statement_sink(callee) => {
            out.push(None)
        }
        Some(Stmt::ExprStmt(e)) => out.push(Some(e.clone())),
        Some(Stmt::If {
            then,
            else_: Some(e),
            ..
        }) => {
            tail_values(then, out);
            tail_values(e, out);
        }
        _ => out.push(None),
    }
}

/// Whether `stmts` bind `n` again: an assignment, a `let`, a pattern, a loop variable, a closure
/// parameter, anywhere in them.
fn rebinds(stmts: &[Stmt], n: &str) -> bool {
    fn in_stmts(stmts: &[Stmt], n: &str) -> bool {
        stmts.iter().any(|s| match s {
            Stmt::Let { name, .. } => name == n,
            Stmt::LetPattern { pattern, .. } => pattern.bound_names().iter().any(|b| b == n),
            Stmt::Assign { target, .. } => matches!(target, Expr::Var(t) if t == n),
            Stmt::For { var, body, .. } => var == n || in_stmts(body, n),
            Stmt::WhileLet { pattern, body, .. } => {
                pattern.bound_names().iter().any(|b| b == n) || in_stmts(body, n)
            }
            Stmt::If { then, else_, .. } => {
                in_stmts(then, n) || else_.as_ref().is_some_and(|e| in_stmts(e, n))
            }
            Stmt::While { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => in_stmts(body, n),
            Stmt::HybridBlock { gpu, cpu, prove } => [gpu, cpu, prove]
                .into_iter()
                .flatten()
                .any(|b| in_stmts(b, n)),
            Stmt::ExprStmt(_) | Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => false,
        })
    }
    let mut hit = in_stmts(stmts, n);
    visit::each_expr_in_stmts(stmts, &mut |e| {
        hit |= match e {
            Expr::Block { stmts, .. } => in_stmts(stmts, n),
            Expr::IfLet { pattern, .. } => pattern.bound_names().iter().any(|b| b == n),
            Expr::Match { arms, .. } => arms
                .iter()
                .any(|a| a.pattern.bound_names().iter().any(|b| b == n)),
            Expr::Lambda { params, .. } => params.iter().any(|p| p == n),
            _ => false,
        };
    });
    hit
}

pub(super) fn assigned_shapes(
    body: &[Stmt],
    ctx: &SemanticContext,
) -> (BTreeSet<String>, BTreeSet<String>, Retype) {
    // A builtin's name bound by a `let` anywhere in the body (a closure, say) is not the builtin.
    fn let_names(stmts: &[Stmt], out: &mut BTreeSet<String>) {
        for s in stmts {
            match s {
                Stmt::Let { name, .. } => {
                    out.insert(name.clone());
                }
                Stmt::LetPattern { pattern, .. } => out.extend(pattern.bound_names()),
                Stmt::If { then, else_, .. } => {
                    let_names(then, out);
                    if let Some(e) = else_ {
                        let_names(e, out);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::Loop { body, .. }
                | Stmt::For { body, .. }
                | Stmt::WhileLet { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => let_names(body, out),
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        let_names(b, out);
                    }
                }
                Stmt::Assign { .. }
                | Stmt::ExprStmt(_)
                | Stmt::Break
                | Stmt::Continue
                | Stmt::SpecBlock { .. } => {}
            }
        }
    }
    let mut lets = BTreeSet::new();
    let_names(body, &mut lets);
    visit::each_expr_in_stmts(body, &mut |e| {
        if let Expr::Block { stmts, .. } = e {
            let_names(stmts, &mut lets);
        }
    });
    let lets = &lets;
    let stays_list = |v: &Expr, n: &str| stays_list_in(v, n, ctx, lets);
    let stays_map = |v: &Expr, n: &str| stays_map_in(v, n, ctx, lets);
    fn stays_list_in(v: &Expr, n: &str, ctx: &SemanticContext, lets: &BTreeSet<String>) -> bool {
        let stays_list = |v: &Expr, n: &str| stays_list_in(v, n, ctx, lets);
        match v {
            Expr::ArrayLiteral { .. } => true,
            Expr::Var(x) => x == n,
            Expr::Binary { op, lhs, .. } if op == "+" => stays_list(lhs, n),
            Expr::If { then, else_, .. } => stays_list(then, n) && stays_list(else_, n),
            Expr::Block { tail: Some(t), .. } => stays_list(t, n),
            Expr::Call { callee, .. } => {
                !ctx.whole_fns.contains_key(callee)
                    && !lets.contains(callee)
                    && matches!(
                        callee.as_str(),
                        "map"
                            | "filter"
                            | "flat_map"
                            | "take"
                            | "drop"
                            | "skip"
                            | "concat"
                            | "flatten"
                            | "keys"
                            | "values"
                            | "entries"
                            | "enumerate"
                            | "zip"
                            | "chunk"
                            | "window"
                            | "partition"
                            | "unique"
                            | "dedup"
                            | "range"
                            | "times"
                            | "take_while"
                            | "drop_while"
                            | "to_list"
                            | "push"
                    )
            }
            _ => false,
        }
    }
    fn stays_map_in(v: &Expr, n: &str, ctx: &SemanticContext, lets: &BTreeSet<String>) -> bool {
        let stays_map = |v: &Expr, n: &str| stays_map_in(v, n, ctx, lets);
        match v {
            Expr::MapLiteral { .. } => true,
            Expr::Var(x) => x == n,
            Expr::If { then, else_, .. } => stays_map(then, n) && stays_map(else_, n),
            Expr::Block { tail: Some(t), .. } => stays_map(t, n),
            Expr::Call { callee, .. } => {
                callee == "merge" && !ctx.whole_fns.contains_key(callee) && !lets.contains(callee)
            }
            _ => false,
        }
    }
    let mut not_list = BTreeSet::new();
    let mut not_map = BTreeSet::new();
    let mut retype = Retype::default();
    let mut note = |target: &Expr, value: &Expr| {
        if let Expr::Var(n) = target {
            if !stays_list(value, n) {
                not_list.insert(n.clone());
            }
            if !stays_map(value, n) {
                not_map.insert(n.clone());
            }
            retype
                .assigned
                .entry(n.clone())
                .or_default()
                .push(value.clone());
        } else if let (Some(r), Some(seg)) = (place_root(target), first_segment(target)) {
            // A field or position write: that part's declared or inferred type may be stale.
            retype.written.entry(r.to_string()).or_default().insert(seg);
        }
    };
    fn stmts_assigns<'a>(stmts: &'a [Stmt], out: &mut Vec<(&'a Expr, &'a Expr)>) {
        for s in stmts {
            match s {
                Stmt::Assign { target, value } => out.push((target, value)),
                Stmt::If { then, else_, .. } => {
                    stmts_assigns(then, out);
                    if let Some(e) = else_ {
                        stmts_assigns(e, out);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::Loop { body, .. }
                | Stmt::For { body, .. }
                | Stmt::WhileLet { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => stmts_assigns(body, out),
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        stmts_assigns(b, out);
                    }
                }
                Stmt::Let { .. }
                | Stmt::LetPattern { .. }
                | Stmt::ExprStmt(_)
                | Stmt::Break
                | Stmt::Continue
                | Stmt::SpecBlock { .. } => {}
            }
        }
    }
    let mut pairs = Vec::new();
    stmts_assigns(body, &mut pairs);
    // Assignments inside value blocks anywhere in the body's expressions.
    visit::each_expr_in_stmts(body, &mut |e| {
        if let Expr::Block { stmts, .. } = e {
            stmts_assigns(stmts, &mut pairs);
        }
    });
    for (t, v) in pairs {
        note(t, v);
    }
    // In-place writes into a container: any part.
    visit::each_expr_in_stmts(body, &mut |e| {
        if let Expr::Call { callee, args } = e {
            if IN_PLACE.contains(&callee.as_str()) && !lets.contains(callee) {
                if let Some(Expr::Var(n)) = args.first() {
                    retype
                        .written
                        .entry(n.clone())
                        .or_default()
                        .insert(WHOLE_ANY.to_string());
                }
            }
        }
    });
    // What each name is bound from (a `let`, a pattern, a loop variable, an assignment).
    fn binds<'a>(stmts: &'a [Stmt], out: &mut Vec<(Vec<String>, &'a Expr, Via)>) {
        for s in stmts {
            match s {
                Stmt::Let { name, init, .. } => out.push((vec![name.clone()], init, Via::Whole)),
                Stmt::LetPattern { pattern, init, .. } => {
                    out.push((pattern.bound_names(), init, Via::Part))
                }
                Stmt::Assign { target, value } => {
                    if let Expr::Var(n) = target {
                        out.push((vec![n.clone()], value, Via::Whole));
                    }
                }
                Stmt::For {
                    var, source, body, ..
                } => {
                    if let crate::frontend::ForSource::Collection { expr } = source {
                        out.push((vec![var.clone()], expr, Via::Part));
                    }
                    binds(body, out);
                }
                Stmt::WhileLet {
                    pattern,
                    expr,
                    body,
                    ..
                } => {
                    out.push((pattern.bound_names(), expr, Via::Part));
                    binds(body, out);
                }
                Stmt::If { then, else_, .. } => {
                    binds(then, out);
                    if let Some(e) = else_ {
                        binds(e, out);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::Loop { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => binds(body, out),
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        binds(b, out);
                    }
                }
                Stmt::ExprStmt(_) | Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
            }
        }
    }
    let mut bound = Vec::new();
    binds(body, &mut bound);
    // The initial values of `let`s (a name's own type is certain only if every one keeps it).
    fn let_inits_in<'a>(stmts: &'a [Stmt], out: &mut Vec<(String, &'a Expr)>) {
        for s in stmts {
            match s {
                Stmt::Let { name, init, .. } => out.push((name.clone(), init)),
                Stmt::If { then, else_, .. } => {
                    let_inits_in(then, out);
                    if let Some(e) = else_ {
                        let_inits_in(e, out);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::Loop { body, .. }
                | Stmt::For { body, .. }
                | Stmt::WhileLet { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => let_inits_in(body, out),
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        let_inits_in(b, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut let_inits = Vec::new();
    let_inits_in(body, &mut let_inits);
    visit::each_expr_in_stmts(body, &mut |e| {
        if let Expr::Block { stmts, .. } = e {
            let_inits_in(stmts, &mut let_inits);
        }
    });
    for (n, init) in let_inits {
        retype.inits.entry(n).or_default().push(init.clone());
    }
    // And inside expressions: value blocks, and the binders of `if let` and `match`.
    visit::each_expr_in_stmts(body, &mut |e| match e {
        Expr::Block { stmts, .. } => binds(stmts, &mut bound),
        Expr::IfLet {
            pattern, scrutinee, ..
        } => bound.push((pattern.bound_names(), scrutinee, Via::Part)),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            for a in arms {
                bound.push((a.pattern.bound_names(), scrutinee, Via::Part));
            }
        }
        _ => {}
    });
    for (names, value, via) in &bound {
        let mut roots = Vec::new();
        // A pattern or a loop variable reads a part of what it is bound from.
        type_roots(value, ctx, *via, &mut roots);
        for n in names {
            // Reading itself (`x = x.step(y)`) is judged by `keeps_type`.
            let rs: Vec<(String, Via)> = roots.iter().filter(|(r, _)| r != n).cloned().collect();
            if !rs.is_empty() {
                retype.bound.entry(n.clone()).or_default().extend(rs);
            }
        }
    }
    // Whose parts may change: written, assigned, or bound from one of these.
    retype.changes = retype
        .written
        .keys()
        .chain(retype.assigned.keys())
        .cloned()
        .collect();
    loop {
        let before = retype.changes.len();
        for (n, roots) in &retype.bound {
            if roots.iter().any(|(r, _)| retype.changes.contains(r)) {
                retype.changes.insert(n.clone());
            }
        }
        if retype.changes.len() == before {
            break;
        }
    }
    (not_list, not_map, retype)
}

/// The writes evaluating `expr` makes to the caller's variables (an in-place `push` in an argument,
/// an assignment inside a value block or a `match` arm): each written variable, what it may hold at
/// any point of the evaluation, and whether it is still known to be a list, and a map.
pub(super) fn expr_writes(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Vec<(String, Option<WholeSrc>, bool, bool)> {
    if !has_writes(expr) {
        return Vec::new();
    }
    let env = Env::new(ctx, scope);
    let at = seeded(expr, &env);
    let w = walk(expr, &env, &at, &None);
    let view = w.view();
    view.locals
        .iter()
        .filter(|(n, _)| scope.contains_key(*n))
        .map(|(n, l)| {
            let k = local_kind(l);
            (n.clone(), local_src(l), k.list, k.map)
        })
        .collect()
}

/// Whether evaluating `e` may write a variable: an assignment in a value block, or an in-place list
/// builtin on a variable.
fn has_writes(e: &Expr) -> bool {
    fn stmts_assign(stmts: &[Stmt]) -> bool {
        stmts.iter().any(|s| match s {
            Stmt::Assign { .. } => true,
            Stmt::If { then, else_, .. } => {
                stmts_assign(then) || else_.as_deref().is_some_and(stmts_assign)
            }
            Stmt::While { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => stmts_assign(body),
            Stmt::HybridBlock { gpu, cpu, prove } => [gpu, cpu, prove]
                .into_iter()
                .flatten()
                .any(|b| stmts_assign(b)),
            Stmt::Let { .. }
            | Stmt::LetPattern { .. }
            | Stmt::ExprStmt(_)
            | Stmt::Break
            | Stmt::Continue
            | Stmt::SpecBlock { .. } => false,
        })
    }
    let mut hit = false;
    visit::each_expr(e, &mut |x| match x {
        Expr::Block { stmts, .. } => hit |= stmts_assign(stmts),
        Expr::Call { callee, args } => {
            hit |= IN_PLACE.contains(&callee.as_str()) && matches!(args.first(), Some(Expr::Var(_)))
        }
        _ => {}
    });
    hit
}

/// What a `for` variable holds: an element of the collection, or a counter computed from range bounds
/// computed from a whole struct.
pub(super) fn for_var_source(
    source: &crate::frontend::ForSource,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<WholeSrc> {
    let env = Env::new(ctx, scope);
    for_var(source, &env, &top())
}

fn for_var(source: &crate::frontend::ForSource, env: &Env, at: &At) -> Option<WholeSrc> {
    match source {
        crate::frontend::ForSource::Collection { expr } => {
            src(expr, env, at).and_then(|s| s.elem())
        }
        crate::frontend::ForSource::Range { start, end } => {
            computed(join(src(start, env, at), src(end, env, at)))
        }
    }
}

/// What a variant pattern's payload holds, given that the scrutinee holds `s`.
fn payload(s: &Option<WholeSrc>) -> Option<WholeSrc> {
    s.as_ref().and_then(WholeSrc::index_elem)
}

/// The names a pattern binds and what each holds, given that the scrutinee holds `s`.
pub(super) fn pattern_binders(
    p: &crate::frontend::Pattern,
    s: &Option<WholeSrc>,
    ctx: &SemanticContext,
) -> Vec<(String, Option<WholeSrc>)> {
    use crate::frontend::Pattern;
    let mut out = Vec::new();
    match p {
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::StrLiteral(_) => {}
        Pattern::Binding(n) => out.push((n.clone(), s.clone())),
        // A name bound by several alternatives holds what any of them binds it to.
        Pattern::Or(alts) => {
            let mut joined: Vec<(String, Option<WholeSrc>)> = Vec::new();
            for (n, b) in alts.iter().flat_map(|a| pattern_binders(a, s, ctx)) {
                match joined.iter_mut().find(|(m, _)| *m == n) {
                    Some((_, prev)) => *prev = join(prev.take(), b),
                    None => joined.push((n, b)),
                }
            }
            out.extend(joined);
        }
        Pattern::List(ps) => {
            for (i, q) in ps.iter().enumerate() {
                let e = s.as_ref().and_then(|s| s.at_pos(&i.to_string()));
                out.extend(pattern_binders(q, &e, ctx));
            }
        }
        Pattern::Struct { name, fields } => {
            for (f, q) in fields {
                let e = join(
                    s.as_ref().and_then(|s| s.field(f, ctx)),
                    declared_field_whole(name, f, ctx),
                );
                out.extend(pattern_binders(q, &e, ctx));
            }
        }
        Pattern::EnumVariant {
            bindings,
            named_bindings,
            ..
        } => {
            let e = payload(s);
            for q in bindings {
                out.extend(pattern_binders(q, &e, ctx));
            }
            for (f, q) in named_bindings {
                let fe = s.as_ref().and_then(|s| s.field(f, ctx));
                out.extend(pattern_binders(q, &fe, ctx));
            }
        }
    }
    out
}

/// What a pattern TESTS of the scrutinee (holding `s`) — which arm runs depends on it: a literal
/// compared against a part, a list's length, an enum's variant, a field declared `secret`. A binder
/// or wildcard tests nothing.
fn tested(
    p: &crate::frontend::Pattern,
    s: &Option<WholeSrc>,
    ctx: &SemanticContext,
) -> Option<WholeSrc> {
    use crate::frontend::Pattern;
    match p {
        Pattern::Literal(_) | Pattern::StrLiteral(_) => computed(s.clone()),
        Pattern::Wildcard | Pattern::Binding(_) => None,
        Pattern::Or(ps) => join_all(ps.iter().map(|q| tested(q, s, ctx))),
        Pattern::List(ps) => {
            join_all(std::iter::once(shape(s.clone())).chain(
                ps.iter().enumerate().map(|(i, q)| {
                    tested(q, &s.as_ref().and_then(|s| s.at_pos(&i.to_string())), ctx)
                }),
            ))
        }
        // Which struct type the scrutinee is, and what its listed fields hold.
        Pattern::Struct { name, fields } => join(
            shape(s.clone()),
            join_all(fields.iter().map(|(f, q)| {
                // A test of a field declared `secret` tests the secret (the field lane does not see a
                // pattern's literal).
                let secret_field = ctx
                    .struct_fields
                    .get(name.trim())
                    .and_then(|fs| fs.get(f))
                    .filter(|t| is_secret_type(Some(t.as_str())))
                    .map(|t| WholeSrc::Computed(format!("declared field `{f}` of type `{t}`")));
                let e = join_all([
                    s.as_ref().and_then(|s| s.field(f, ctx)),
                    declared_field_whole(name, f, ctx),
                    secret_field,
                ]);
                tested(q, &e, ctx)
            })),
        ),
        Pattern::EnumVariant {
            bindings,
            named_bindings,
            ..
        } => {
            let e = payload(s);
            join_all(
                std::iter::once(shape(s.clone()))
                    .chain(bindings.iter().map(|q| tested(q, &e, ctx)))
                    .chain(
                        named_bindings.iter().map(|(f, q)| {
                            tested(q, &s.as_ref().and_then(|s| s.field(f, ctx)), ctx)
                        }),
                    ),
            )
        }
    }
}

/// What a struct type's declared field `f` may hold of such a struct, by its declared type.
fn declared_field_whole(ty: &str, f: &str, ctx: &SemanticContext) -> Option<WholeSrc> {
    declared_type_whole(ctx.struct_fields.get(ty.trim())?.get(f)?, ctx, 0)
}

/// What a value of declared type `t` may hold of such a struct: one, a container of what its
/// element type may hold (`list<list<S>>`, `map<string, S>`), or — a type naming one in a shape
/// the lane does not read — one anywhere inside.
fn declared_type_whole(t: &str, ctx: &SemanticContext, depth: u32) -> Option<WholeSrc> {
    let pt = ctx.place_types();
    if let Some(v) = whole_struct_source(t, &pt, SourceLane::Secret) {
        return Some(WholeSrc::Value(v));
    }
    if depth < 8 {
        if let Some(e) = ty::container_element_type(t) {
            return declared_type_whole(&e, ctx, depth + 1).map(WholeSrc::holds);
        }
    }
    t.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .filter_map(|w| whole_struct_source(w, &pt, SourceLane::Secret))
        .reduce(|a, b| union_text(&a, &b))
        .map(WholeSrc::Deep)
}

/// The mark an assignment leaves on the root it writes (`q = p`, `t.a = str(p)`, `m["a"] = p`,
/// `xs[i] = p`): the value at that place, a key or position computed from a whole struct deciding
/// where.
pub(super) fn assign_mark_in(
    target: &Expr,
    value: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<(String, WholeSrc)> {
    let env = Env::new(ctx, scope);
    let at = top();
    let (root, s) = assign_mark(target, value, &env, &at)?;
    if !free_names(value).contains(&root) {
        return Some((root, s));
    }
    // A value built from the root itself: what the root holds once the assignment has run any number
    // of times (in a loop), widened as it grows.
    let kind = Kind {
        list: scope.get(&root).is_some_and(|b| b.whole_list),
        ..Kind::NONE
    };
    let mut cur = join(src(&Expr::Var(root.clone()), &env, &at), Some(s));
    for k in 0..MAX_ITER {
        let again = at.with([(root.clone(), Local::Src(cur.clone(), kind.clone()))]);
        let mut next = join(
            cur.clone(),
            assign_mark(target, value, &env, &again).map(|(_, t)| t),
        );
        if k >= 1 {
            next = next.map(WholeSrc::widen);
        }
        if next == cur {
            return cur.map(|c| (root, c));
        }
        cur = next;
    }
    cur.map(|c| (root, c.computed()))
}

fn assign_mark(target: &Expr, value: &Expr, env: &Env, at: &At) -> Option<(String, WholeSrc)> {
    assign_mark_held(target, src(value, env, at), env, at)
}

/// `assign_mark`, the value holding `place`.
fn assign_mark_held(
    target: &Expr,
    mut place: Option<WholeSrc>,
    env: &Env,
    at: &At,
) -> Option<(String, WholeSrc)> {
    let mut t = target;
    loop {
        match t {
            Expr::Var(r) => return place.map(|s| (r.clone(), s)),
            Expr::FieldAccess { base, field, .. } => {
                place = place.map(|s| WholeSrc::Record([(field.clone(), s)].into_iter().collect()));
                t = base;
            }
            Expr::Index { base, index } => {
                let key = src(index, env, at);
                // The entries a write lands in: a clean position; a string key both as a map key and
                // as the list position it converts to; anything else, any entry.
                let slots: Vec<String> = match index.as_ref() {
                    Expr::Literal(k) if is_position(k) => vec![k.clone()],
                    Expr::StrLiteral(k) => match (list_pos(k), kind_of(base, env, at)) {
                        (_, Kind { map: true, .. }) => vec![user_key(k)],
                        (Some(p), Kind { list: true, .. }) => vec![p],
                        (Some(p), _) => vec![user_key(k), p],
                        (None, _) => vec![user_key(k), WHOLE_ANY.to_string()],
                    },
                    _ => vec![WHOLE_ANY.to_string()],
                };
                place = place
                    .map(|s| WholeSrc::Record(slots.into_iter().map(|k| (k, s.clone())).collect()));
                // Where the write lands depends on a key or position computed from one.
                if key.is_some() {
                    place = join(place, computed(key));
                }
                t = base;
            }
            _ => return None,
        }
    }
}

/// The mark an in-place list builtin (`push(ys, v)`, `insert(ys, i, v)`, `remove(ys, i)`) leaves on
/// its list, and whether it shifts the positions already there.
/// `statement`: the call is a statement of its own, where the runtime pushes in place whatever
/// else `push` names.
pub(super) fn in_place_mark_in(
    callee: &str,
    args: &[Expr],
    statement: bool,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<(String, Option<WholeSrc>, bool)> {
    let env = Env::new(ctx, scope);
    in_place_mark(callee, args, statement, &env, &top())
}

fn in_place_mark(
    callee: &str,
    args: &[Expr],
    statement: bool,
    env: &Env,
    at: &At,
) -> Option<(String, Option<WholeSrc>, bool)> {
    let statement_push = statement && callee == "push" && args.len() == 2;
    if !statement_push && (env.is_user_fn(callee) || resolve_local(callee, env, at).is_some()) {
        return None;
    }
    let Some(Expr::Var(root)) = args.first() else {
        return None;
    };
    let a = |i: usize| args.get(i).and_then(|x| src(x, env, at));
    let rest = || holds(join_all(args[1..].iter().map(|x| src(x, env, at))));
    let (s, shifts) = match callee {
        "push" | "append" => (rest(), false),
        "unshift" => (rest(), true),
        // A position or key computed from such a struct decides where every later element sits, or
        // which one is gone.
        "insert" => (join(holds(a(2)), computed(a(1))), true),
        "remove" => (computed(a(1)), true),
        "pop" => (None, false),
        _ => return None,
    };
    Some((root.clone(), s, shifts))
}

/// Where a call (a user function, method, closure, or builtin applying callbacks) makes a whole
/// struct, or a value computed from one, reach an egress sink: the source that does. A direct sink
/// call is the caller's own check.
pub(super) fn call_egress(
    call: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<String> {
    let env = Env::new(ctx, scope);
    let at = top();
    let fx = match call {
        Expr::Call { callee, args } if !is_direct_sink(callee, &env) => {
            call_fx(callee, args, &env, &at, Want::Egress)
        }
        Expr::CallExpr { callee, args } => callexpr_fx(callee, args, &env, &at, Want::Egress),
        _ => FnFx::default(),
    };
    fx.egress.map(|s| s.text())
}

/// What the names a closure written in the caller's scope closes over stand for now — values,
/// closures (with what they close over), functions — kept on the binding, since a later `let` or
/// assignment of the same name must not change what the closure sees.
pub(super) fn closure_captures(
    lambda: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Captures {
    let env = Env::new(ctx, scope);
    let at = top();
    Captures(Rc::new(
        free_names(lambda)
            .into_iter()
            .filter(|n| scope.contains_key(n))
            .filter_map(|n| {
                let l = match resolve_local(&n, &env, &at)? {
                    l @ Local::Src(..) => {
                        Local::Src(src(&Expr::Var(n.clone()), &env, &at), local_kind(&l))
                    }
                    // A closure or a value (joined paths): both halves are captured.
                    other => other,
                };
                Some((n, l))
            })
            .collect(),
    ))
}

/// The functions and (`impl`-prefixed) methods that may build a struct with a `secret` field —
/// directly, through a call, or by passing a builder along — with the sources of what they build.
/// A call past the lane's limits may return one without being given it.
pub(super) fn compute_builders(ctx: &SemanticContext) -> BTreeMap<String, String> {
    let pt = ctx.place_types();
    let scan = |body: &[Stmt], known: &BTreeMap<String, String>| {
        let mut acc: Option<String> = None;
        visit::each_expr_in_stmts(body, &mut |e| {
            let hit = match e {
                Expr::StructLiteral { name, .. } => {
                    whole_struct_source(name, &pt, SourceLane::Secret)
                }
                Expr::Call { callee, .. } => known.get(callee).cloned(),
                Expr::Var(n) => known.get(n).cloned(),
                Expr::CallExpr { callee, .. } => match callee.as_ref() {
                    Expr::FieldAccess { field, .. } => known.get(&format!("impl {field}")).cloned(),
                    _ => None,
                },
                _ => None,
            };
            if let Some(t) = hit {
                acc = Some(match acc.take() {
                    Some(a) => union_text(&a, &t),
                    None => t,
                });
            }
        });
        acc
    };
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    loop {
        let mut next = out.clone();
        let mut add = |name: String, t: String| {
            let merged = match next.get(&name) {
                Some(a) => union_text(a, &t),
                None => t,
            };
            next.insert(name, merged);
        };
        for (n, (_, body)) in &ctx.whole_fns {
            if let Some(t) = scan(body, &out) {
                add(n.clone(), t);
            }
        }
        for (m, impls) in &ctx.whole_methods {
            for (_, (_, body)) in impls {
                if let Some(t) = scan(body, &out) {
                    add(format!("impl {m}"), t);
                }
            }
        }
        if next == out {
            return out;
        }
        out = next;
    }
}

/// The secret struct types a well-formed `declassify` anywhere in the program may release whole. A
/// released struct holds nothing the lane tracks, yet the runtime dispatches a method call on it by
/// its type: a receiver may always be one of these (`call_method`).
pub(super) fn compute_released(ctx: &SemanticContext) -> BTreeSet<String> {
    let pt = ctx.place_types();
    let secret: BTreeSet<String> = ctx
        .struct_fields
        .keys()
        .filter(|t| whole_struct_source(t, &pt, SourceLane::Secret).is_some())
        .cloned()
        .collect();
    let mut out = BTreeSet::new();
    let mut scan = |body: &[Stmt]| {
        visit::each_expr_in_stmts(body, &mut |e| {
            if let Expr::Declassify {
                inner,
                policy,
                reason,
            } = e
            {
                if !declassify_wellformed(policy, reason) {
                    return;
                }
                released_by(inner, ctx, &secret, &mut out);
            }
        });
    };
    for (_, body) in ctx.whole_fns.values() {
        scan(body);
    }
    for impls in ctx.whole_methods.values() {
        for (_, (_, body)) in impls {
            scan(body);
        }
    }
    out
}

/// The secret struct types a declassified value may release whole: a struct literal's own type and
/// what its fields may release; for anything else that may hold a struct, every secret type.
fn released_by(
    e: &Expr,
    ctx: &SemanticContext,
    secret: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match e {
        Expr::StructLiteral { name, fields, .. } => {
            let t = name.split('<').next().unwrap_or("").trim();
            if secret.contains(t) {
                out.insert(t.to_string());
            }
            for (_, v) in fields {
                released_by(v, ctx, secret, out);
            }
        }
        other if may_hold_struct(other, ctx) => out.extend(secret.iter().cloned()),
        _ => {}
    }
}

/// Whether a type is one the runtime enforces as a number (a field declared so holds one; a cast to
/// it converts). Any other declared type may hold anything (TY-UNENFORCED).
fn numeric_type(t: &str) -> bool {
    let mut t = t.trim();
    for wrap in ["secret<", "tainted<"] {
        if let Some(rest) = t.strip_prefix(wrap) {
            t = rest.trim_end_matches('>').trim();
        }
    }
    matches!(
        t,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "int"
            | "float"
    )
}

/// Whether a value may be (or hold) a struct: anything but a number's or a scalar builtin's shape.
fn may_hold_struct(e: &Expr, ctx: &SemanticContext) -> bool {
    match e {
        Expr::Literal(_) | Expr::StrLiteral(_) | Expr::Unary { .. } => false,
        // The runtime converts only to a numeric type; any other cast leaves the value as it is.
        Expr::Cast { expr, ty } => !numeric_type(ty) && may_hold_struct(expr, ctx),
        // `+` concatenates lists.
        Expr::Binary { op, lhs, rhs } => {
            op == "+" && (may_hold_struct(lhs, ctx) || may_hold_struct(rhs, ctx))
        }
        // Any field: the runtime lets a struct through a field declared numeric (it rejects only a
        // float in an integer field), and the checker does not enforce declared types.
        Expr::FieldAccess { .. } => true,
        // A runtime builtin returning a scalar, not taken by a user function of that name.
        Expr::Call { callee, .. } => {
            ctx.whole_fns.contains_key(callee)
                || !crate::backends::run::is_builtin_name(callee)
                || !matches!(
                    callee.as_str(),
                    "str" | "len" | "int" | "float" | "abs" | "sha256"
                )
        }
        _ => true,
    }
}

/// A sink name is a sink in call position (the runtime resolves the builtin before a parameter of
/// that name), unless a user function takes it.
fn is_direct_sink(callee: &str, env: &Env) -> bool {
    is_egress_sink(callee) && !env.is_user_fn(callee)
}

/// The sinks a statement call always reaches, whatever else the name means (the runtime's
/// `stmt_as_tail_expr`).
pub(super) fn is_statement_sink(callee: &str) -> bool {
    matches!(callee, "print" | "println" | "eprint" | "eprintln")
}

// ------------------------------------------------------------------------------------------------
// The walker

/// A name bound here (locals first, then the caller's scope when allowed).
fn resolve_local(n: &str, env: &Env, at: &At) -> Option<Local> {
    if let Some(l) = at.locals.get(n) {
        return Some(l.clone());
    }
    if !at.use_scope {
        return None;
    }
    let b = env.scope.get(n)?;
    Some(
        match (b.closure_lambda.as_deref(), &b.fn_identities, &b.fn_alias) {
            (Some(lam @ Expr::Lambda { .. }), _, _) => {
                let caps: Locals = b
                    .whole_captures
                    .as_ref()
                    .map(|c| (*c.0).clone())
                    .unwrap_or_default();
                let f = env.closure(lam, &caps, true);
                // A binding that may be the closure or a value (joined paths) is either.
                match &b.whole_struct {
                    Some(w) => Local::Any(vec![f, Local::Src(Some(w.clone()), Kind::NONE)]),
                    None => f,
                }
            }
            (_, FnIdentitySet::Known(ns), _) if ns.len() == 1 && b.whole_struct.is_none() => {
                Local::Named(ns.iter().next().cloned().unwrap_or_default())
            }
            (_, _, Some(f)) if b.whole_struct.is_none() => Local::Named(f.clone()),
            _ => Local::Src(
                b.whole_struct.clone(),
                Kind {
                    list: b.whole_list,
                    map: b.whole_map,
                    // The struct type declared (or inferred) for it: a method call on it runs that
                    // impl (declared types are not enforced: TY-UNENFORCED) — unless the name is
                    // assigned again or written into, when the inference may be stale.
                    ty: b
                        .info
                        .ty
                        .as_deref()
                        .map(|t| t.split('<').next().unwrap_or("").trim())
                        .filter(|t| env.ctx.struct_fields.contains_key(*t))
                        .filter(|t| !stale_type(n, t, env))
                        .map(Rc::from),
                    sure: scope_type(n, env).is_some_and(|t| sure_type(n, &t, env)),
                },
            ),
        },
    )
}

/// What a name stands for before a write adds to it: bound here, in the caller's scope, or nothing.
fn current_local(n: &str, env: &Env, at: &At) -> Local {
    match resolve_local(n, env, at) {
        Some(l @ (Local::Src(..) | Local::Any(_))) => {
            Local::Src(src(&Expr::Var(n.to_string()), env, at), local_kind(&l))
        }
        _ => Local::Src(src(&Expr::Var(n.to_string()), env, at), Kind::NONE),
    }
}

/// The variable a place expression (`x`, `x.f`, `x[i]`, `x.f()`) is rooted in.
fn place_root(e: &Expr) -> Option<&str> {
    match e {
        Expr::Var(n) => Some(n),
        Expr::FieldAccess { base, .. } | Expr::Index { base, .. } => place_root(base),
        Expr::CallExpr { callee, .. } => place_root(callee),
        _ => None,
    }
}

/// What `e` is known to be, from its shape alone (a struct literal's type among it).
fn kind_of(e: &Expr, env: &Env, at: &At) -> Kind {
    let (ty, sure) = match e {
        Expr::StructLiteral { name, .. } => (
            Some(Rc::from(name.split('<').next().unwrap_or("").trim())),
            true,
        ),
        Expr::Var(n) => match resolve_local(n, env, at) {
            Some(l) => {
                let k = local_kind(&l);
                (k.ty, k.sure)
            }
            None => (None, false),
        },
        // Released, but still what it was (a method call on it runs that impl).
        Expr::Declassify { inner, .. } => return kind_of(inner, env, at),
        _ => (None, false),
    };
    Kind {
        list: is_listish(e, env, at),
        map: is_mapish(e, env, at),
        ty,
        sure,
    }
}

/// Whether `e` is known to be a map, from its shape alone.
fn is_mapish(e: &Expr, env: &Env, at: &At) -> bool {
    match e {
        Expr::MapLiteral { .. } => true,
        Expr::Var(n) => resolve_local(n, env, at).is_some_and(|l| local_kind(&l).map),
        Expr::If { then, else_, .. } => is_mapish(then, env, at) && is_mapish(else_, env, at),
        // `merge` requires a map and returns one.
        Expr::Call { callee, .. } if callee == "merge" && !env.is_user_fn(callee) => {
            resolve_local(callee, env, at).is_none()
        }
        _ => false,
    }
}

/// Whether `e` is known to be a list, from its shape alone.
fn is_listish(e: &Expr, env: &Env, at: &At) -> bool {
    match e {
        Expr::ArrayLiteral { .. } => true,
        Expr::Var(n) => resolve_local(n, env, at).is_some_and(|l| local_is_list(&l)),
        // A list plus anything is a list (the runtime appends or concatenates).
        Expr::Binary { op, lhs, .. } if op == "+" => is_listish(lhs, env, at),
        Expr::If { then, else_, .. } => is_listish(then, env, at) && is_listish(else_, env, at),
        // A string stays a string (`reverse("ab")`, `slice(s, 0, 2)`); only a list sorts.
        Expr::Call { callee, args }
            if matches!(callee.as_str(), "reverse" | "slice" | "sort" | "sort_by")
                && !env.is_user_fn(callee) =>
        {
            resolve_local(callee, env, at).is_none()
                && args.first().is_some_and(|a| is_listish(a, env, at))
        }
        Expr::Call { callee, .. } if !env.is_user_fn(callee) => {
            resolve_local(callee, env, at).is_none()
                && matches!(
                    callee.as_str(),
                    "map"
                        | "filter"
                        | "flat_map"
                        | "take"
                        | "drop"
                        | "skip"
                        | "concat"
                        | "flatten"
                        | "keys"
                        | "values"
                        | "entries"
                        | "enumerate"
                        | "zip"
                        | "chunk"
                        | "window"
                        | "partition"
                        | "unique"
                        | "dedup"
                        | "range"
                        | "times"
                        | "take_while"
                        | "drop_while"
                        | "to_list"
                        | "push"
                )
        }
        _ => false,
    }
}

/// What `e` holds.
fn src(e: &Expr, env: &Env, at: &At) -> Option<WholeSrc> {
    let ctx = env.ctx;
    let rec = |x: &Expr| src(x, env, at);
    let pt = ctx.place_types();
    // Declared types come from the caller's scope, so they apply only to a place rooted in a name of
    // that scope — not one a lambda parameter or local shadows. They only ADD.
    let typed = |x: &Expr| {
        if !at.use_scope || place_root(x).is_some_and(|r| at.locals.contains_key(r)) {
            return None;
        }
        place_struct_type(x, env.scope, &pt)
            .and_then(|t| whole_struct_source(&t, &pt, SourceLane::Secret))
            .map(WholeSrc::Value)
    };
    match e {
        Expr::Var(n) => match at.locals.get(n) {
            Some(l) => local_src(l),
            None => match resolve_local(n, env, at) {
                Some(l @ (Local::Src(..) | Local::Any(_))) => join(local_src(&l), typed(e)),
                Some(_) => None,
                None => typed(e),
            },
        },
        Expr::StructLiteral { name, fields, .. } => {
            let own = whole_struct_source(name, &pt, SourceLane::Secret).map(WholeSrc::Value);
            let carried: BTreeMap<String, WholeSrc> = fields
                .iter()
                .filter_map(|(f, v)| rec(v).map(|s| (f.clone(), s)))
                .collect();
            if carried.is_empty() {
                own
            } else {
                join(own, Some(WholeSrc::Record(carried)))
            }
        }
        // A list by position when its elements differ (`[x.id, x]`), else a container of them.
        Expr::ArrayLiteral { elements } => {
            let parts: Vec<Option<WholeSrc>> = elements.iter().map(rec).collect();
            if parts.iter().all(Option::is_none) {
                return None;
            }
            if parts.iter().all(|p| p.is_some() && p == &parts[0]) {
                return holds(parts[0].clone());
            }
            Some(WholeSrc::Record(
                parts
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, p)| p.map(|p| (i.to_string(), p)))
                    .collect(),
            ))
        }
        // A key computed from such a struct is observable (`keys`, `has_key`); a value may be one. A
        // string literal key is the entry's exact key; any other key, any entry.
        Expr::MapLiteral { entries, .. } => {
            let keys = computed(join_all(entries.iter().map(|(k, _)| rec(k))));
            let mut fs: BTreeMap<String, WholeSrc> = BTreeMap::new();
            for (k, v) in entries {
                if let Some(s) = rec(v) {
                    let slot = match k {
                        Expr::StrLiteral(k) => user_key(k),
                        _ => WHOLE_ANY.to_string(),
                    };
                    let merged = match fs.remove(&slot) {
                        Some(o) => join2(o, s),
                        None => s,
                    };
                    fs.insert(slot, merged);
                }
            }
            join(keys, (!fs.is_empty()).then_some(WholeSrc::Record(fs)))
        }
        // A struct variant by field name; a tuple variant's payload.
        Expr::EnumConstruct {
            enum_name,
            variant,
            fields,
            field_names,
            ..
        } => {
            // A `Result::Err` / `Option::None` (with a payload) is what `?` returns early.
            if (variant == "Err" && (enum_name == "Result" || enum_name.is_empty()))
                || (variant == "None"
                    && (enum_name == "Option" || enum_name.is_empty())
                    && !fields.is_empty())
            {
                // In struct form (`Result::Err { e: x }`) its fields are read by name too.
                join_all(fields.iter().map(&rec)).map(|e| {
                    let mut fs: BTreeMap<String, WholeSrc> =
                        [(WHOLE_ERR.to_string(), e)].into_iter().collect();
                    if field_names.len() == fields.len() {
                        for (n, v) in field_names.iter().zip(fields) {
                            if let Some(s) = rec(v) {
                                let merged = match fs.remove(&user_key(n)) {
                                    Some(o) => join2(o, s),
                                    None => s,
                                };
                                fs.insert(user_key(n), merged);
                            }
                        }
                    }
                    WholeSrc::Record(fs)
                })
            } else if !field_names.is_empty() && field_names.len() == fields.len() {
                let mut fs: BTreeMap<String, WholeSrc> = BTreeMap::new();
                for (n, v) in field_names.iter().zip(fields) {
                    if let Some(s) = rec(v) {
                        // A field named twice: either value.
                        let merged = match fs.remove(n) {
                            Some(o) => join2(o, s),
                            None => s,
                        };
                        fs.insert(n.clone(), merged);
                    }
                }
                (!fs.is_empty()).then_some(WholeSrc::Record(fs))
            } else {
                holds(join_all(fields.iter().map(&rec)))
            }
        }
        Expr::Index { base, index } => {
            let b = rec(base);
            let read = match index.as_ref() {
                Expr::StrLiteral(k) => b.and_then(|b| b.at_key(k, ctx)),
                Expr::Literal(i) if is_position(i) => b.and_then(|b| b.at_pos(i)),
                // Any other index (`1.5`, `true`, `-1`, `i`) converts to some position, or names some
                // key: any entry, chosen by it.
                _ => join(b.and_then(|b| b.index_elem()), computed(rec(index))),
            };
            join(read, typed(e))
        }
        Expr::FieldAccess { base, field, .. } => {
            join(rec(base).and_then(|b| b.field(field, ctx)), typed(e))
        }
        // A condition computed from such a struct chooses the value by it.
        Expr::If {
            cond, then, else_, ..
        } => join_all([computed(rec(cond)), rec(then), rec(else_)]),
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let s = rec(scrutinee);
            let whole = value_local(scrutinee, &s, env, at);
            let inner = binders_at(env, at, pattern, &s, &whole);
            join_all([tested(pattern, &s, ctx), src(then, env, &inner), rec(else_)])
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let s = rec(scrutinee);
            let whole = value_local(scrutinee, &s, env, at);
            let mut out = None;
            for a in arms {
                let inner = binders_at(env, at, &a.pattern, &s, &whole);
                out = join(out, tested(&a.pattern, &s, ctx));
                if let Some(g) = &a.guard {
                    out = join(out, computed(src(g, env, &inner)));
                }
                out = join(out, src(&a.body, env, &inner));
            }
            out
        }
        // A block's value: its tail, or — as the runtime reads a block — its last statement's value.
        Expr::Block { stmts, tail } => {
            interp(stmts, tail.as_deref(), env, at, &None, Run::BLOCK_VALUE).val
        }
        Expr::Binary { op, lhs, rhs } => {
            let (l, r) = (rec(lhs), rec(rhs));
            let empty = |x: &Expr| match x {
                Expr::ArrayLiteral { elements } => elements.is_empty(),
                Expr::MapLiteral { entries, .. } => entries.is_empty(),
                Expr::StrLiteral(s) => s.is_empty(),
                Expr::Var(n) => n == "None",
                Expr::EnumConstruct {
                    variant, fields, ..
                } => variant == "None" && fields.is_empty(),
                _ => false,
            };
            // Emptiness and presence tests are shape reads.
            if (op == "==" || op == "!=") && (empty(lhs) || empty(rhs)) {
                return shape(join(l, r));
            }
            if op == "+" {
                // A list plus a list concatenates; a list plus anything else appends it. A string on
                // either side renders the other, and numbers add.
                let (ll, rl) = (is_listish(lhs, env, at), is_listish(rhs, env, at));
                if ll && rl {
                    return join(l, unpos(r));
                }
                if ll {
                    let spread = r
                        .as_ref()
                        .filter(|s| !matches!(s, WholeSrc::Value(_)))
                        .cloned()
                        .map(WholeSrc::unpos);
                    return join_all([l, holds(r), spread]);
                }
            }
            computed(join(l, r))
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => computed(rec(expr)),
        Expr::Tainted { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => rec(inner),
        Expr::Try(inner) => rec(inner).and_then(|s| s.try_value()),
        // A well-formed `declassify` releases the value; a malformed one releases nothing.
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => {
            if declassify_wellformed(policy, reason) {
                None
            } else {
                rec(inner)
            }
        }
        Expr::Call { callee, args } => {
            join(call_fx(callee, args, env, at, Want::Ret).ret, typed(e))
        }
        Expr::CallExpr { callee, args } => {
            join(callexpr_fx(callee, args, env, at, Want::Ret).ret, typed(e))
        }
        Expr::Lambda { .. }
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. } => None,
        Expr::Other(_) => typed(e),
    }
}

// ------------------------------------------------------------------------------------------------
// The interpreter

/// How a statement list runs: whether its last statement's value is a result (a function body, a
/// value block), and whether its `let`s are scoped to it (every block but a function body).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Run {
    tail: bool,
    scoped: bool,
}

impl Run {
    const BODY: Run = Run {
        tail: true,
        scoped: false,
    };
    const BLOCK_VALUE: Run = Run {
        tail: true,
        scoped: true,
    };
    const LOOP_BODY: Run = Run {
        tail: false,
        scoped: true,
    };

    /// A branch of this run (the last statement's branches carry its value).
    fn branch(self, last: bool) -> Run {
        Run {
            tail: self.tail && last,
            scoped: true,
        }
    }

    fn bits(self) -> u8 {
        u8::from(self.tail) | (u8::from(self.scoped) << 1)
    }
}

/// What running statements or walking an expression did: the names after it; what it returned
/// (joined with what the conditions selecting each `return` decide) and whether anything is
/// returned at all (`return 1` under a computed condition returns what the condition decides); its
/// value (a block's last statement or tail, chosen by the conditions around it) and whether it has
/// one; where a whole struct reached an egress; the names where a `break` (or the loop's own exit)
/// leaves and where a `continue` goes back; and, walking an expression, the join of every state it
/// passed through (a read inside it may see any of them).
struct Fx {
    at: At,
    ret: Option<WholeSrc>,
    returned: bool,
    val: Option<WholeSrc>,
    valued: bool,
    /// Walking an expression: `val` is its whole value (a block, `if`, `if let` or `match` computed
    /// as it ran), so it need not be recomputed from the state the walk ended in.
    known: bool,
    egress: Option<WholeSrc>,
    brk: Option<Locals>,
    cont: Option<Locals>,
    track: bool,
    seen: Option<Rc<Locals>>,
}

/// A remembered block run.
#[derive(Clone)]
struct Ran {
    /// `None`: the names it started from.
    after: Option<Rc<Locals>>,
    ret: Option<WholeSrc>,
    returned: bool,
    val: Option<WholeSrc>,
    valued: bool,
    egress: Option<WholeSrc>,
    brk: Option<Locals>,
    cont: Option<Locals>,
}

impl Fx {
    fn at(at: &At) -> Fx {
        Fx {
            at: at.clone(),
            ret: None,
            returned: false,
            val: None,
            valued: false,
            known: false,
            egress: None,
            brk: None,
            cont: None,
            track: false,
            seen: None,
        }
    }

    fn tracking(at: &At) -> Fx {
        Fx {
            track: true,
            ..Fx::at(at)
        }
    }

    /// The names now stand at `at` (the same position, kept, when they are what they were: a block's
    /// run is remembered by the position it starts from).
    fn move_to(&mut self, at: At) {
        if Rc::ptr_eq(&at.locals, &self.at.locals) || *at.locals == *self.at.locals {
            return;
        }
        if self.track {
            let base = match self.seen.take() {
                Some(s) => s,
                None => self.at.locals.clone(),
            };
            self.seen = Some(Rc::new(join_locals(&base, &at.locals)));
        }
        self.at = at;
    }

    /// Continue with what `next` (run from the names as they stand) did.
    fn then(&mut self, next: Fx) {
        self.ret = join(self.ret.take(), next.ret);
        self.returned |= next.returned;
        self.egress = join(self.egress.take(), next.egress);
        self.brk = join_opt_locals(self.brk.take(), next.brk);
        self.cont = join_opt_locals(self.cont.take(), next.cont);
        if self.track {
            if let Some(s) = next.seen {
                let base = match self.seen.take() {
                    Some(b) => b,
                    None => self.at.locals.clone(),
                };
                self.seen = Some(Rc::new(join_locals(&base, &s)));
            }
        }
        self.move_to(next.at);
    }

    /// Continue after one of `arms` ran from `here` (each arm's state joins).
    fn branches(&mut self, here: &At, arms: Vec<Fx>) {
        let mut after: Option<Locals> = None;
        for a in arms {
            self.ret = join(self.ret.take(), a.ret);
            self.returned |= a.returned;
            self.val = join(self.val.take(), a.val);
            self.valued |= a.valued;
            self.egress = join(self.egress.take(), a.egress);
            self.brk = join_opt_locals(self.brk.take(), a.brk);
            self.cont = join_opt_locals(self.cont.take(), a.cont);
            if self.track {
                if let Some(s) = a.seen {
                    let base = match self.seen.take() {
                        Some(b) => b,
                        None => self.at.locals.clone(),
                    };
                    self.seen = Some(Rc::new(join_locals(&base, &s)));
                }
            }
            after = Some(match after {
                Some(l) => join_locals(&l, &a.at.locals),
                None => (*a.at.locals).clone(),
            });
        }
        let at = match after {
            Some(l) => here.with_locals(l),
            None => here.clone(),
        };
        self.move_to(at);
    }

    /// A `return v` under `cond`.
    fn returns(&mut self, v: Option<WholeSrc>, cond: &Option<WholeSrc>) {
        self.ret = join_all([self.ret.take(), v, cond.clone()]);
        self.returned = true;
    }

    /// The names any read so far may have seen.
    fn view(&self) -> At {
        match &self.seen {
            Some(s) => self.at.with_locals_rc(s.clone()),
            None => self.at.clone(),
        }
    }

    /// Names bound inside (a block's `let`s, a pattern's binders) no longer bound: each takes back
    /// what it stood for in `outer`, or is gone. A jump or an earlier point may have come before the
    /// binding, so there it is joined with that instead.
    fn restore(&mut self, names: impl IntoIterator<Item = (String, Option<Local>)>) {
        let names: Vec<(String, Option<Local>)> = names.into_iter().collect();
        if names.is_empty() {
            return;
        }
        let mut end = (*self.at.locals).clone();
        let fix_join = |m: &mut Locals, n: &str, prev: &Option<Local>| match prev {
            Some(p) => {
                let v = match m.get(n) {
                    Some(x) => join_local(x, p),
                    None => p.clone(),
                };
                m.insert(n.to_string(), v);
            }
            None => {
                m.remove(n);
            }
        };
        let names_map: BTreeMap<String, Option<Local>> = names.iter().cloned().collect();
        restore_names(&mut end, &names_map);
        // A jump out of the block was recorded while the names were bound: the outer names are
        // exactly what they were.
        for j in [&mut self.brk, &mut self.cont].into_iter().flatten() {
            restore_names(j, &names_map);
        }
        if let Some(s) = self.seen.take() {
            let mut s = (*s).clone();
            for (n, prev) in &names {
                fix_join(&mut s, n, prev);
            }
            self.seen = Some(Rc::new(s));
        }
        self.at = self.at.with_locals(end);
    }
}

/// What `names` stood for at `at` (to restore them later).
fn saved(names: impl IntoIterator<Item = String>, at: &At) -> Vec<(String, Option<Local>)> {
    names
        .into_iter()
        .map(|n| {
            let prev = at.locals.get(&n).cloned();
            (n, prev)
        })
        .collect()
}

/// What an expression just walked holds: the value the walk computed as it ran, when it did (a
/// block, `if`, `if let`, `match`), else its source at every state the walk passed through.
fn walked_value(e: &Expr, x: &Fx, env: &Env) -> Option<WholeSrc> {
    if x.known {
        x.val.clone()
    } else {
        src(e, env, &x.view())
    }
}

/// `let_local` for an expression just walked.
fn walked_local(e: &Expr, x: &Fx, env: &Env) -> Local {
    let base = match e {
        Expr::Lambda { .. } | Expr::Var(_) => return let_local(e, env, &x.view()),
        _ if x.known => Local::Src(
            x.val.clone().map(WholeSrc::capped),
            kind_of(e, env, &x.view()),
        ),
        _ => match let_local(e, env, &x.view()) {
            Local::Src(v, k) => Local::Src(v.map(WholeSrc::capped), k),
            other => other,
        },
    };
    // A builtin may hand back a function value it was given (`identity(f)`, `max(f)`, a `get`
    // default, `Some(f)?`).
    let mut fns = Vec::new();
    passed_fns(e, env, &x.view(), &mut fns);
    if fns.is_empty() {
        base
    } else {
        fns.insert(0, base);
        Local::Any(fns)
    }
}

/// What an expression evaluated at `at` is, as a binding (its source `s` already computed): a
/// function value stays one, and one a builtin hands back comes with it (`walked_local`, without a
/// walk).
fn value_local(e: &Expr, s: &Option<WholeSrc>, env: &Env, at: &At) -> Local {
    let base = match e {
        Expr::Lambda { .. } | Expr::Var(_) => return let_local(e, env, at),
        _ => Local::Src(s.clone(), kind_of(e, env, at)),
    };
    let mut fns = Vec::new();
    passed_fns(e, env, at, &mut fns);
    if fns.is_empty() {
        base
    } else {
        fns.insert(0, base);
        Local::Any(fns)
    }
}

/// The function values an expression may yield as they are: given to a builtin, wrapped in a
/// variant, taken by `?`, released, or chosen by a branch.
fn passed_fns(e: &Expr, env: &Env, at: &At, out: &mut Vec<Local>) {
    match e {
        Expr::Lambda { .. } => out.push(env.closure(e, &at.locals, at.use_scope)),
        Expr::Var(n) => match resolve_local(n, env, at) {
            Some(l @ (Local::Closure(_) | Local::Named(_) | Local::Opaque(_))) => out.push(l),
            Some(Local::Any(ls)) => {
                out.extend(ls.into_iter().filter(|l| !matches!(l, Local::Src(..))))
            }
            Some(_) => {}
            None if env.is_user_fn(n) => out.push(Local::Named(n.clone())),
            None => {}
        },
        // `identity(f)`, `max(f)` / `min(f)` of one value, a `get` default.
        Expr::Call { callee, args }
            if matches!(callee.as_str(), "identity" | "max" | "min" | "get")
                && !env.is_user_fn(callee)
                && resolve_local(callee, env, at).is_none() =>
        {
            for a in args {
                passed_fns(a, env, at, out);
            }
        }
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                passed_fns(f, env, at, out);
            }
        }
        Expr::Try(inner) | Expr::Declassify { inner, .. } | Expr::Cast { expr: inner, .. } => {
            passed_fns(inner, env, at, out)
        }
        Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
            passed_fns(then, env, at, out);
            passed_fns(else_, env, at, out);
        }
        Expr::Block { tail: Some(t), .. } => passed_fns(t, env, at, out),
        Expr::Match { arms, .. } => {
            for a in arms {
                passed_fns(&a.body, env, at, out);
            }
        }
        _ => {}
    }
}

/// A name's binding for `let name = init` (evaluated at `at`): a closure stays callable (closing over
/// what its free names hold now), a callable alias stays one, anything else is what it holds.
fn let_local(init: &Expr, env: &Env, at: &At) -> Local {
    match init {
        Expr::Lambda { .. } => env.closure(init, &at.locals, at.use_scope),
        Expr::Var(_) => arg_local(init, env, at),
        _ => Local::Src(src(init, env, at), kind_of(init, env, at)),
    }
}

/// Run `stmts` (and a block's `tail`) from `at`. `cond` is what the conditions around them decide
/// (joined into any `return`).
fn interp(
    stmts: &[Stmt],
    tail: Option<&Expr>,
    env: &Env,
    at: &At,
    cond: &Option<WholeSrc>,
    run: Run,
) -> Fx {
    let key: BlockKey = (
        stmts.as_ptr() as usize,
        stmts.len(),
        tail.map_or(0, |t| t as *const Expr as usize),
        Rc::as_ptr(&at.locals) as usize,
        at.use_scope,
        at.depth,
        run.bits(),
    );
    let hit = env.blocks.borrow().get(&key).map(|(_, r)| r.clone());
    let r = match hit {
        Some(r) => r,
        None => {
            // Run without the surrounding condition (joined in below: joins commute), so every use
            // shares the run.
            let f = interp_uncached(stmts, tail, env, at, run);
            let r = Ran {
                after: (*f.at.locals != *at.locals).then(|| f.at.locals.clone()),
                ret: f.ret,
                returned: f.returned,
                val: f.val,
                valued: f.valued,
                egress: f.egress,
                brk: f.brk,
                cont: f.cont,
            };
            let mut blocks = env.blocks.borrow_mut();
            if blocks.len() < MAX_BLOCKS {
                blocks.insert(key, (at.locals.clone(), r.clone()));
            }
            r
        }
    };
    let mut f = Fx::at(&match r.after {
        Some(after) => at.with_locals_rc(after),
        None => at.clone(),
    });
    f.ret = if r.returned {
        join(r.ret, cond.clone())
    } else {
        r.ret
    };
    f.returned = r.returned;
    f.val = if r.valued {
        join(r.val, cond.clone())
    } else {
        r.val
    };
    f.valued = r.valued;
    f.egress = r.egress;
    f.brk = r.brk;
    f.cont = r.cont;
    f
}

fn interp_uncached(stmts: &[Stmt], tail: Option<&Expr>, env: &Env, start: &At, run: Run) -> Fx {
    let ctx = env.ctx;
    let _ = env.exhausted();
    let none = None;
    let mut w = Fx::at(start);
    // What the names a `let` here shadows stood for (restored when the block ends). A jump state
    // recorded before the `let` holds the outer name: it is set aside as it is (the names shadowed
    // so far restored in it), so only states recorded after the `let` are restored at the end.
    let mut shadowed: BTreeMap<String, Option<Local>> = BTreeMap::new();
    let mut early_brk: Option<Locals> = None;
    let mut early_cont: Option<Locals> = None;
    let mut shadow = |names: Vec<String>, w: &mut Fx| {
        if run.scoped {
            if !shadowed.is_empty() {
                for j in [&mut w.brk, &mut w.cont].into_iter().flatten() {
                    restore_names(j, &shadowed);
                }
            }
            early_brk = join_opt_locals(early_brk.take(), w.brk.take());
            early_cont = join_opt_locals(early_cont.take(), w.cont.take());
            for n in names {
                let prev = w.at.locals.get(&n).cloned();
                shadowed.entry(n).or_insert(prev);
            }
        }
    };
    for (i, st) in stmts.iter().enumerate() {
        let last = run.tail && tail.is_none() && i + 1 == stmts.len();
        match st {
            Stmt::Let { name, init, .. } => {
                let x = walk(init, env, &w.at, &none);
                let v = walked_local(init, &x, env);
                w.then(x);
                shadow(vec![name.clone()], &mut w);
                let at = w.at.with([(name.clone(), v)]);
                w.move_to(at);
            }
            Stmt::LetPattern { pattern, init, .. } => {
                let x = walk(init, env, &w.at, &none);
                let s = walked_value(init, &x, env);
                let whole = walked_local(init, &x, env);
                w.then(x);
                shadow(pattern.bound_names(), &mut w);
                let at = match whole_binder(pattern) {
                    Some(b) => w.at.with([(b.to_string(), whole)]),
                    None => w.at.with(
                        pattern_binders(pattern, &s, ctx)
                            .into_iter()
                            .map(|(n, b)| (n, Local::Src(b, Kind::NONE))),
                    ),
                };
                w.move_to(at);
            }
            Stmt::Assign { target, value } => {
                let mut x = walk(target, env, &w.at, &none);
                let after_target = x.at.clone();
                let vx = walk(value, env, &after_target, &none);
                let v = walked_local(value, &vx, env);
                let held = walked_value(value, &vx, env);
                x.then(vx);
                let view = x.view();
                let mark = assign_mark_held(target, held, env, &view);
                w.then(x);
                match target {
                    // A plain assignment replaces what the name holds.
                    Expr::Var(r) => {
                        let at = w.at.with([(r.clone(), v)]);
                        w.move_to(at);
                    }
                    _ => {
                        if let Some((root, s)) = mark {
                            let old = match w.at.locals.get(&root) {
                                Some(l) => l.clone(),
                                None => current_local(&root, env, &w.at),
                            };
                            let l = join_local(&old, &Local::Src(Some(s), local_kind(&old)));
                            let at = w.at.with([(root, l)]);
                            w.move_to(at);
                        }
                    }
                }
            }
            Stmt::ExprStmt(e @ Expr::Call { callee, .. }) if callee == "return" => {
                let x = walk(e, env, &w.at, &none);
                w.then(x);
            }
            // A statement `push(xs, v)` pushes in place, whatever else `push` names.
            // (As a block's last statement it is the block's value: an ordinary call, below.)
            Stmt::ExprStmt(Expr::Call { callee, args })
                if !last
                    && callee == "push"
                    && args.len() == 2
                    && matches!(args[0], Expr::Var(_)) =>
            {
                let x = walk(&args[1], env, &w.at, &none);
                let view = x.view();
                w.then(x);
                if let Some((root, s, _)) = in_place_mark(callee, args, true, env, &view) {
                    let old = match w.at.locals.get(&root) {
                        Some(l) => local_src(l),
                        None => src(&Expr::Var(root.clone()), env, &w.at),
                    };
                    let at = w.at.with([(root, Local::Src(join(old, s), Kind::LIST))]);
                    w.move_to(at);
                }
            }
            // A statement `print` / `println` / `eprint` / `eprintln` is the builtin, whatever
            // else the name means (the runtime lowers it before looking any function up, and it
            // stays a statement as a block's last: `stmt_as_tail_expr`).
            Stmt::ExprStmt(e @ Expr::Call { callee, args })
                if is_statement_sink(callee) && env.is_user_fn(callee) =>
            {
                let x = walk(e, env, &w.at, &none);
                let view = x.view();
                let egress = join_all(args.iter().map(|a| src(a, env, &view)));
                w.then(x);
                w.egress = join(w.egress.take(), egress);
                if last {
                    w.val = None;
                    w.valued = true;
                }
            }
            Stmt::ExprStmt(e) => {
                let x = walk(e, env, &w.at, &none);
                let v = if last { walked_value(e, &x, env) } else { None };
                w.then(x);
                if last {
                    w.val = v;
                    w.valued = true;
                }
            }
            Stmt::If {
                cond: c,
                then,
                else_,
            } => {
                let x = walk(c, env, &w.at, &none);
                let cc = computed(src(c, env, &x.view()));
                w.then(x);
                let here = w.at.clone();
                // Only an `if` with an `else` is a value (the runtime's `stmt_as_tail_expr`).
                let arms = run.branch(last && else_.is_some());
                let a = interp(then, None, env, &here, &cc, arms);
                let b = match else_ {
                    Some(e) => interp(e, None, env, &here, &cc, arms),
                    None => Fx::at(&here),
                };
                w.branches(&here, vec![a, b]);
            }
            Stmt::While { cond: c, body, .. } => {
                let f = run_loop(env, &w.at, |at_k| {
                    let mut x = walk(c, env, at_k, &none);
                    let cc = computed(src(c, env, &x.view()));
                    // The condition fails: the loop leaves here.
                    x.brk = join_opt_locals(x.brk.take(), Some((*x.at.locals).clone()));
                    let head = x.at.clone();
                    x.then(interp(body, None, env, &head, &cc, Run::LOOP_BODY));
                    x
                });
                w.then(f);
            }
            Stmt::Loop { body, .. } => {
                let f = run_loop(env, &w.at, |at_k| {
                    interp(body, None, env, at_k, &none, Run::LOOP_BODY)
                });
                w.then(f);
            }
            Stmt::For {
                var, source, body, ..
            } => {
                let (x, decides) = match source {
                    crate::frontend::ForSource::Collection { expr } => {
                        let x = walk(expr, env, &w.at, &none);
                        // How many times the body runs: only a computed collection's shape.
                        let d = shape(src(expr, env, &x.view()));
                        (x, d)
                    }
                    crate::frontend::ForSource::Range { start: s, end: e } => {
                        let mut x = walk(s, env, &w.at, &none);
                        let after_start = x.at.clone();
                        x.then(walk(e, env, &after_start, &none));
                        let view = x.view();
                        let d = computed(join(src(s, env, &view), src(e, env, &view)));
                        (x, d)
                    }
                };
                // The collection is evaluated once, before the loop.
                let v = for_var(source, env, &x.view());
                w.then(x);
                let f = run_loop(env, &w.at, |at_k| {
                    let inner = at_k.with([(var.clone(), Local::Src(v.clone(), Kind::NONE))]);
                    let mut b = interp(body, None, env, &inner, &decides, Run::LOOP_BODY);
                    // The loop variable does not outlive the loop.
                    b.restore(saved([var.clone()], at_k));
                    // The collection runs out: the loop leaves from the head.
                    b.brk = join_opt_locals(b.brk.take(), Some((*at_k.locals).clone()));
                    b
                });
                w.then(f);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
                ..
            } => {
                let f = run_loop(env, &w.at, |at_k| {
                    let mut x = walk(expr, env, at_k, &none);
                    let s = src(expr, env, &x.view());
                    let whole = walked_local(expr, &x, env);
                    let cc = tested(pattern, &s, ctx);
                    // The pattern does not match: the loop leaves here.
                    x.brk = join_opt_locals(x.brk.take(), Some((*x.at.locals).clone()));
                    let head = x.at.clone();
                    let inner = binders_at(env, &head, pattern, &s, &whole);
                    let mut b = interp(body, None, env, &inner, &cc, Run::LOOP_BODY);
                    b.restore(saved(pattern.bound_names(), &head));
                    x.then(b);
                    x
                });
                w.then(f);
            }
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                let here = w.at.clone();
                w.then(interp(body, None, env, &here, &none, run.branch(false)));
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for blk in [gpu, cpu, prove].into_iter().flatten() {
                    let here = w.at.clone();
                    w.then(interp(blk, None, env, &here, &none, run.branch(false)));
                }
            }
            Stmt::Break => {
                w.brk = join_opt_locals(w.brk.take(), Some((*w.at.locals).clone()));
            }
            Stmt::Continue => {
                w.cont = join_opt_locals(w.cont.take(), Some((*w.at.locals).clone()));
            }
            Stmt::SpecBlock { .. } => {}
        }
    }
    if let Some(t) = tail {
        let x = walk(t, env, &w.at, &none);
        let v = if run.tail {
            walked_value(t, &x, env)
        } else {
            None
        };
        w.then(x);
        if run.tail {
            w.val = v;
            w.valued = true;
        }
    }
    // A `let` does not outlive its block.
    w.restore(shadowed);
    w.brk = join_opt_locals(w.brk.take(), early_brk);
    w.cont = join_opt_locals(w.cont.take(), early_cont);
    w
}

/// Put back what `names` stood for before they were bound in a state that jumps out of the block
/// (every such state was recorded while they were bound).
fn restore_names(m: &mut Locals, names: &BTreeMap<String, Option<Local>>) {
    for (n, prev) in names {
        match prev {
            Some(p) => {
                m.insert(n.clone(), p.clone());
            }
            None => {
                m.remove(n);
            }
        }
    }
}

/// A loop: run the body from the names as they stand, join, and repeat until nothing changes —
/// widening what still grows, and past that assuming the worst of it. The loop leaves with the names
/// where the body's `break`s (and the loop's own exit) left.
fn run_loop(env: &Env, at: &At, body: impl Fn(&At) -> Fx) -> Fx {
    let cur = at.clone();
    let acc = Fx::at(at);
    // A value moves one name per iteration along a chain of assignments: allow one per name.
    let limit = MAX_ITER.max(at.locals.len() + 2).min(4 * MAX_ITER);
    env.loops.set(env.loops.get() + 1);
    let r = run_loop_from(env, cur, acc, limit, &body);
    env.loops.set(env.loops.get() - 1);
    r
}

/// `run_loop` from its first state (`env.loops` counts the loops being run).
fn run_loop_from(
    env: &Env,
    mut cur: At,
    mut acc: Fx,
    limit: usize,
    body: &impl Fn(&At) -> Fx,
) -> Fx {
    for k in 0..limit {
        // Past the query's step budget, stop iterating: the fallback below covers every iteration.
        if env.steps.get() > MAX_STEPS {
            break;
        }
        env.note_head(&cur);
        let f = body(&cur);
        acc.ret = join(acc.ret.take(), f.ret);
        acc.returned |= f.returned;
        acc.egress = join(acc.egress.take(), f.egress);
        acc.brk = join_opt_locals(acc.brk.take(), f.brk);
        let mut next = join_locals(&cur.locals, &f.at.locals);
        if let Some(c) = &f.cont {
            next = join_locals(&next, c);
        }
        if k >= 2 {
            for (n, v) in next.iter_mut() {
                if cur.locals.get(n) != Some(v) {
                    *v = widen_with(v, env);
                }
            }
        }
        if *cur.locals == next {
            return leave(cur, acc);
        }
        cur = cur.with_locals(next);
    }
    // Not settled: every name may hold, computed, anything any name can reach, and a function kept
    // in one is no longer followed; one more run from there covers every later iteration.
    env.fell_back();
    let mut seen = BTreeSet::new();
    let mut all = None;
    for l in cur
        .locals
        .values()
        .chain(acc.brk.iter().flat_map(|b| b.values()))
    {
        all = join(all, reach(l, env, &mut seen));
    }
    let all = computed(all);
    let mut top_names = (*cur.locals).clone();
    for v in top_names.values_mut() {
        *v = topped(v, &all);
    }
    let cur = cur.with_locals(top_names);
    env.note_head(&cur);
    let f = body(&cur);
    acc.ret = computed(join(acc.ret.take(), f.ret));
    acc.returned |= f.returned;
    acc.egress = computed(join(acc.egress.take(), f.egress));
    acc.brk = join_opt_locals(acc.brk.take(), f.brk);
    let mut after = join_locals(&cur.locals, &f.at.locals);
    if let Some(c) = &f.cont {
        after = join_locals(&after, c);
    }
    let cur = cur.with_locals(after);
    acc.brk = acc.brk.map(|b| {
        let mut b = join_locals(&b, &cur.locals);
        for v in b.values_mut() {
            *v = topped(v, &all);
        }
        b
    });
    leave(cur, acc)
}

/// A binding past an unsettled loop: what it holds joined with `all`, its shape no longer known, and
/// a function in it opaque — keeping both halves of a value-or-function.
fn topped(l: &Local, all: &Option<WholeSrc>) -> Local {
    match l {
        Local::Src(s, _) => Local::Src(join(s.clone(), all.clone()), Kind::NONE),
        // A function moved along a chain of names may be any function any name held.
        Local::Named(_) => Local::Any(vec![l.clone(), Local::Opaque(all.clone())]),
        Local::Closure(_) | Local::Opaque(_) => Local::Opaque(all.clone()),
        Local::Any(_) => Local::Any(vec![
            Local::Src(join(local_src(l), all.clone()), Kind::NONE),
            Local::Opaque(all.clone()),
        ]),
    }
}

/// The names after a settled loop: where its `break`s and exits left (none: it never leaves).
fn leave(cur: At, mut acc: Fx) -> Fx {
    let at = match acc.brk.take() {
        Some(b) => cur.with_locals(b),
        None => cur,
    };
    acc.cont = None;
    acc.at = at;
    acc
}

/// Walk an expression in evaluation order: the `return`s inside it, where it makes a whole struct
/// reach an egress, and the names after it (a write in a value block or arm, an in-place `push`).
/// A lambda's body is not entered (it runs where the lambda is called). `cond` is what the
/// conditions around it decide.
fn walk(e: &Expr, env: &Env, at: &At, cond: &Option<WholeSrc>) -> Fx {
    let ctx = env.ctx;
    let mut w = Fx::tracking(at);
    match e {
        Expr::Call { callee, args } => {
            walk_seq(&mut w, args, env, cond);
            let view = w.view();
            if callee == "return" {
                let v = args.first().and_then(|a| src(a, env, &view));
                w.returns(v, cond);
                return w;
            }
            // `break` / `continue` in expression position (a braceless `match` arm) leave the loop
            // or go back to its head from here.
            if callee == "break" && args.is_empty() {
                w.brk = join_opt_locals(w.brk.take(), Some((*w.at.locals).clone()));
                return w;
            }
            if callee == "continue" && args.is_empty() {
                w.cont = join_opt_locals(w.cont.take(), Some((*w.at.locals).clone()));
                return w;
            }
            let egress = if is_direct_sink(callee, env) {
                join_all(args.iter().map(|a| src(a, env, &view)))
            } else {
                call_fx(callee, args, env, &view, Want::Egress).egress
            };
            w.egress = join(w.egress.take(), egress);
            // An in-place list builtin writes its variable wherever it appears.
            if let Some((root, s, shifts)) = in_place_mark(callee, args, false, env, &view) {
                let old_local =
                    w.at.locals
                        .get(&root)
                        .cloned()
                        .unwrap_or_else(|| current_local(&root, env, &w.at));
                let old = local_src(&old_local).map(|o| if shifts { o.unpos() } else { o });
                // Every in-place builtin but `remove` works only on a list.
                let kind = if callee == "remove" {
                    local_kind(&old_local)
                } else {
                    Kind::LIST
                };
                let at = w.at.with([(root, Local::Src(join(old, s), kind))]);
                w.move_to(at);
            }
        }
        Expr::CallExpr { callee, args } => {
            if !matches!(callee.as_ref(), Expr::Lambda { .. }) {
                walk_seq(&mut w, [callee.as_ref()], env, cond);
            }
            walk_seq(&mut w, args, env, cond);
            let view = w.view();
            let egress = callexpr_fx(callee, args, env, &view, Want::Egress).egress;
            w.egress = join(w.egress.take(), egress);
        }
        Expr::If {
            cond: c,
            then,
            else_,
            ..
        } => {
            let x = walk(c, env, at, cond);
            let chose = computed(src(c, env, &x.view()));
            let cc = join(cond.clone(), chose.clone());
            w.then(x);
            let here = w.at.clone();
            let a = walk(then, env, &here, &cc);
            let b = walk(else_, env, &here, &cc);
            let known = a.known && b.known;
            let val = join_all([chose, a.val.clone(), b.val.clone()]);
            w.branches(&here, vec![a, b]);
            w.val = val;
            w.known = known;
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let x = walk(scrutinee, env, at, cond);
            let s = src(scrutinee, env, &x.view());
            let whole = walked_local(scrutinee, &x, env);
            let test = tested(pattern, &s, ctx);
            let cc = join(cond.clone(), test.clone());
            w.then(x);
            let here = w.at.clone();
            let inner = binders_at(env, &here, pattern, &s, &whole);
            let mut a = walk(then, env, &inner, &cc);
            a.restore(saved(pattern.bound_names(), &here));
            let b = walk(else_, env, &here, &cc);
            let known = a.known && b.known;
            let val = join_all([test, a.val.clone(), b.val.clone()]);
            w.branches(&here, vec![a, b]);
            w.val = val;
            w.known = known;
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let x = walk(scrutinee, env, at, cond);
            let s = src(scrutinee, env, &x.view());
            let whole = walked_local(scrutinee, &x, env);
            w.then(x);
            let here = w.at.clone();
            // The runtime tries the arms in order: a guard that runs and fails leaves its writes for
            // the arms after it.
            let mut start = here.clone();
            let mut outs = Vec::new();
            let mut known = true;
            let mut val = None;
            for a in arms {
                let inner = binders_at(env, &start, &a.pattern, &s, &whole);
                let test = tested(&a.pattern, &s, ctx);
                val = join(val, test.clone());
                let mut cc = join(cond.clone(), test);
                let mut arm = Fx::tracking(&inner);
                if let Some(g) = &a.guard {
                    // The guard runs once for each alternative of the pattern that matches.
                    let runs = match &a.pattern {
                        crate::frontend::Pattern::Or(ps) => ps.len().max(1),
                        _ => 1,
                    };
                    let mut gx = walk(g, env, &inner, &cc);
                    for _ in 1..runs {
                        let once = gx.at.clone();
                        let again = walk(g, env, &once, &cc);
                        gx.branches(&once, vec![again, Fx::at(&once)]);
                    }
                    let decided = computed(src(g, env, &gx.view()));
                    val = join(val, decided.clone());
                    cc = join(cc, decided);
                    arm.then(gx);
                    let mut failed = Fx::at(&arm.at);
                    failed.restore(saved(a.pattern.bound_names(), &start));
                    start = start.with_locals(join_locals(&start.locals, &failed.at.locals));
                }
                let after_guard = arm.at.clone();
                let body = walk(&a.body, env, &after_guard, &cc);
                known &= body.known;
                val = join(val, body.val.clone());
                arm.then(body);
                arm.restore(saved(a.pattern.bound_names(), &start));
                outs.push(arm);
            }
            w.branches(&here, outs);
            w.val = val;
            w.known = known;
        }
        Expr::Block { stmts, tail } => {
            // Its `let`s end with it; its writes to outer names stay. A read after it sees where it
            // ended.
            let f = interp(stmts, tail.as_deref(), env, at, cond, Run::BLOCK_VALUE);
            w.val = f.val.clone();
            w.known = true;
            w.ret = join(w.ret.take(), f.ret);
            w.returned |= f.returned;
            w.egress = join(w.egress.take(), f.egress);
            w.brk = join_opt_locals(w.brk.take(), f.brk);
            w.cont = join_opt_locals(w.cont.take(), f.cont);
            w.move_to(f.at);
        }
        Expr::Lambda { .. } => {}
        // `x?` returns an `Err` (or `None`) as it is, chosen by the variant.
        Expr::Try(x) => {
            walk_seq(&mut w, [x.as_ref()], env, cond);
            let v = src(x, env, &w.view());
            let early = join(v.as_ref().and_then(WholeSrc::err_part), shape(v));
            w.returns(early, cond);
        }
        // The right of `&&` / `||` runs only when the left does not decide.
        Expr::Binary { op, lhs, rhs } if op == "&&" || op == "||" => {
            walk_seq(&mut w, [lhs.as_ref()], env, cond);
            let here = w.at.clone();
            let cc = join(cond.clone(), computed(src(lhs, env, &w.view())));
            let right = walk(rhs, env, &here, &cc);
            w.branches(&here, vec![right, Fx::at(&here)]);
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_seq(&mut w, [lhs.as_ref(), rhs.as_ref()], env, cond);
        }
        Expr::Index { base, index } => {
            walk_seq(&mut w, [base.as_ref(), index.as_ref()], env, cond);
        }
        Expr::Unary { expr: x, .. }
        | Expr::Cast { expr: x, .. }
        | Expr::Tainted { inner: x, .. }
        | Expr::Assume(x)
        | Expr::Assert(x)
        | Expr::Declassify { inner: x, .. }
        | Expr::FieldAccess { base: x, .. } => walk_seq(&mut w, [x.as_ref()], env, cond),
        Expr::ArrayLiteral { elements: xs } | Expr::EnumConstruct { fields: xs, .. } => {
            walk_seq(&mut w, xs, env, cond);
        }
        Expr::StructLiteral { fields, .. } => {
            walk_seq(&mut w, fields.iter().map(|(_, x)| x.as_ref()), env, cond);
        }
        Expr::MapLiteral { entries, .. } => {
            walk_seq(&mut w, entries.iter().flat_map(|(k, v)| [k, v]), env, cond);
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
    w
}

/// Walk `xs` in order, each from the names the previous one left.
fn walk_seq<'e>(
    w: &mut Fx,
    xs: impl IntoIterator<Item = &'e Expr>,
    env: &Env,
    cond: &Option<WholeSrc>,
) {
    for x in xs {
        let here = w.at.clone();
        let sub = walk(x, env, &here, cond);
        w.then(sub);
    }
}

// ------------------------------------------------------------------------------------------------
// Calls

/// A call `callee(args)`: a user function (specialized), a local closure, or a builtin.
fn call_fx(callee: &str, args: &[Expr], env: &Env, at: &At, want: Want) -> FnFx {
    if callee == "return" {
        return FnFx {
            ret: args.first().and_then(|a| src(a, env, at)),
            egress: None,
        };
    }
    // A user function wins in call position (the runtime resolves it before a local of that name).
    if env.is_user_fn(callee) {
        return call_fn(callee, args_locals(args, env, at), env);
    }
    match resolve_local(callee, env, at) {
        Some(l) => apply_fx(&l, args_locals(args, env, at), env, at),
        None => match want {
            Want::Ret => FnFx {
                ret: builtin_src(callee, args, env, at),
                egress: None,
            },
            Want::Egress => FnFx {
                ret: None,
                egress: builtin_egress(callee, args, env, at),
            },
        },
    }
}

/// A call through an expression: a method (`p.m(x)`), a parenthesized name, a lambda, or a value.
fn callexpr_fx(callee: &Expr, args: &[Expr], env: &Env, at: &At, want: Want) -> FnFx {
    match callee {
        Expr::FieldAccess { base, field, .. } if env.ctx.whole_methods.contains_key(field) => {
            let recv = arg_local(base, env, at);
            let kind = local_kind(&recv);
            if (kind.map || kind.list) && kind.ty.is_none() {
                // A map's (or list's) "method" is a function stored in it: a value the lane does
                // not resolve.
                return FnFx {
                    ret: computed(join_all(args.iter().map(|a| src(a, env, at)))),
                    egress: None,
                };
            }
            let which = receiver(base, &recv, env, at);
            let mut binds = vec![recv];
            binds.extend(args_locals(args, env, at));
            call_method(field, binds, &which, env)
        }
        Expr::Var(n) => call_fx(n, args, env, at, want),
        Expr::Lambda { .. } => {
            let l = env.closure(callee, &at.locals, at.use_scope);
            apply_fx(&l, args_locals(args, env, at), env, at)
        }
        // A value the lane cannot resolve (`fs[0](p)`, `h.f(p)`) may compute from any argument.
        _ => FnFx {
            ret: computed(join_all(args.iter().map(|a| src(a, env, at)))),
            egress: None,
        },
    }
}

/// The impls a method call may run, by the receiver.
enum Recv {
    /// A struct of the types with a `secret` field its value names, or of any type without a
    /// `secret` field (a value holding nothing of such a struct may be one of those).
    Named(BTreeSet<String>),
    /// Of the type the value is surely (a struct literal, a method's own receiver — the value,
    /// wherever it is passed or captured, not the name `self` — or a scope binding whose every
    /// binding keeps its type), or of the types with a `secret` field its value names.
    Sure(BTreeSet<String>),
    /// Of the type declared or inferred for it, of the types with a `secret` field its value names,
    /// or of a struct type a `declassify` may have released (declared types are not enforced).
    Declared(BTreeSet<String>),
    /// Anything.
    Unknown,
}

fn receiver(base: &Expr, recv: &Local, env: &Env, at: &At) -> Recv {
    let mut tys = match local_src(recv) {
        Some(s @ (WholeSrc::Value(_) | WholeSrc::Record(_) | WholeSrc::Deep(_))) => {
            match s.struct_types() {
                Some(t) => t,
                None => return Recv::Unknown,
            }
        }
        // A value computed from such a struct (`sort(xs)[0]`, a choice by one) may be a struct of any
        // type.
        Some(WholeSrc::Computed(_)) => return Recv::Unknown,
        _ => BTreeSet::new(),
    };
    // Of the type the value is known to be (a struct literal, a method's own receiver, a scope
    // binding's type), or of the types with a `secret` field its value names: a scope type may be
    // stale or unenforced (TY-UNENFORCED), and only ever adds.
    let kind = local_kind(recv);
    if let Some(t) = kind.ty {
        tys.insert(t.to_string());
        return if kind.sure {
            Recv::Sure(tys)
        } else {
            Recv::Declared(tys)
        };
    }
    let scoped = at.use_scope && !place_root(base).is_some_and(|r| at.locals.contains_key(r));
    if scoped {
        if let Some(t) = place_struct_type(base, env.scope, &env.ctx.place_types()) {
            let t = t.split('<').next().unwrap_or("").trim().to_string();
            // Of the declared type (declared types are not enforced: TY-UNENFORCED), unless it
            // may be stale, or of the types with a `secret` field its value names.
            let stale = place_root(base).is_some_and(|r| match first_segment(base) {
                None => stale_type(r, &t, env),
                Some(seg) => {
                    let own = env
                        .scope
                        .get(r)
                        .and_then(|b| b.info.ty.as_deref())
                        .map(|t| t.split('<').next().unwrap_or("").trim().to_string())
                        .filter(|t| env.ctx.struct_fields.contains_key(t));
                    stale_part(r, &seg, own.as_deref(), env)
                }
            });
            if !stale {
                tys.insert(t);
                return Recv::Declared(tys);
            }
        }
    }
    Recv::Named(tys)
}

/// An argument as a formal's binding: a function value stays callable, anything else is its source.
fn arg_local(a: &Expr, env: &Env, at: &At) -> Local {
    match a {
        Expr::Lambda { .. } => env.closure(a, &at.locals, at.use_scope),
        Expr::Var(n) => match resolve_local(n, env, at) {
            Some(l) => l,
            None if env.is_user_fn(n) || crate::backends::run::is_builtin_name(n) => {
                Local::Named(n.clone())
            }
            None => Local::Src(src(a, env, at), kind_of(a, env, at)),
        },
        _ => {
            let base = Local::Src(src(a, env, at), kind_of(a, env, at));
            let mut fns = Vec::new();
            passed_fns(a, env, at, &mut fns);
            if fns.is_empty() {
                base
            } else {
                fns.insert(0, base);
                Local::Any(fns)
            }
        }
    }
}

/// Whether an argument may be a function value (written as one, or handed back by a builtin that
/// returns its argument).
fn may_pass_fn(e: &Expr) -> bool {
    match e {
        Expr::Lambda { .. } | Expr::Var(_) => true,
        Expr::Call { callee, args } => {
            matches!(callee.as_str(), "identity" | "max" | "min" | "get")
                && args.iter().any(may_pass_fn)
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().any(may_pass_fn),
        Expr::Try(inner) | Expr::Declassify { inner, .. } | Expr::Cast { expr: inner, .. } => {
            may_pass_fn(inner)
        }
        Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
            may_pass_fn(then) || may_pass_fn(else_)
        }
        Expr::Block { tail: Some(t), .. } => may_pass_fn(t),
        Expr::Match { arms, .. } => arms.iter().any(|a| may_pass_fn(&a.body)),
        _ => false,
    }
}

fn args_locals(args: &[Expr], env: &Env, at: &At) -> Vec<Local> {
    args.iter().map(|a| arg_local(a, env, at)).collect()
}

/// How many parameters what a name stands for takes (0 when unknown).
fn arity(l: &Local, env: &Env) -> usize {
    match l {
        Local::Closure(c) => match c.lam.as_ref() {
            Expr::Lambda { params, .. } => params.len(),
            _ => 0,
        },
        Local::Named(f) => env.ctx.whole_fns.get(f).map_or(0, |(ps, _)| ps.len()),
        Local::Any(ls) => ls.iter().map(|x| arity(x, env)).max().unwrap_or(0),
        Local::Src(..) | Local::Opaque(_) => 0,
    }
}

/// Calling what a name stands for with arguments bound to `binds`.
fn apply_fx(l: &Local, binds: Vec<Local>, env: &Env, at: &At) -> FnFx {
    if at.depth >= MAX_APPLY_DEPTH || env.exhausted() {
        env.fell_back();
        let mut all = binds;
        all.push(l.clone());
        return worst(&all, None, env);
    }
    match l {
        Local::Closure(c) => {
            let Expr::Lambda { params, body } = c.lam.as_ref() else {
                return FnFx::default();
            };
            let cid = env.intern(c);
            let hit = env.applies.borrow().get(&cid).and_then(|v| {
                v.iter()
                    .find(|a| {
                        a.epoch == env.epoch.get()
                            && a.limited_at.is_none_or(|d| d == at.depth)
                            && a.binds == binds
                    })
                    .map(|a| (a.fx.clone(), a.deps.clone()))
            });
            if let Some((fx, deps)) = hit {
                env.reads.borrow_mut().extend(deps);
                return fx;
            }
            let reads_start = env.reads.borrow().len();
            let fallbacks_start = env.fallbacks.get();
            let mut locals = (*c.captured).clone();
            for (i, p) in params.iter().enumerate() {
                // The runtime pads a missing argument with `0`.
                locals.insert(
                    p.clone(),
                    binds
                        .get(i)
                        .cloned()
                        .unwrap_or(Local::Src(None, Kind::NONE)),
                );
            }
            let inner = At {
                locals: Rc::new(locals),
                use_scope: c.use_scope,
                depth: at.depth + 1,
            };
            let w = walk(body, env, &inner, &None);
            let fx = FnFx {
                // What the body evaluates to, and what a `return` inside it returns.
                ret: join(src(body, env, &w.view()), w.ret).map(WholeSrc::capped),
                egress: w.egress,
            };
            let applied = Applied {
                binds,
                epoch: env.epoch.get(),
                deps: env.deps_since(reads_start, None),
                limited_at: (env.fallbacks.get() != fallbacks_start).then_some(at.depth),
                fx: fx.clone(),
            };
            env.applies
                .borrow_mut()
                .entry(cid)
                .or_default()
                .push(applied);
            fx
        }
        Local::Named(f) if env.is_user_fn(f) => call_fn(f, binds, env),
        // A builtin passed as a value, applied to `binds`: as if called with them.
        Local::Named(f) => {
            let names: Vec<String> = (0..binds.len()).map(|i| format!("\u{0}w{i}")).collect();
            let inner = At {
                locals: Rc::new(names.iter().cloned().zip(binds.iter().cloned()).collect()),
                use_scope: false,
                depth: at.depth + 1,
            };
            let args: Vec<Expr> = names.into_iter().map(Expr::Var).collect();
            let sink = if is_egress_sink(f) {
                join_all(binds.iter().map(local_src))
            } else {
                None
            };
            FnFx {
                ret: builtin_src(f, &args, env, &inner),
                egress: join(sink, builtin_egress(f, &args, env, &inner)),
            }
        }
        Local::Any(ls) => {
            let mut out = FnFx::default();
            for x in ls {
                let f = apply_fx(x, binds.clone(), env, at);
                out.ret = join(out.ret, f.ret);
                out.egress = join(out.egress, f.egress);
            }
            out
        }
        Local::Opaque(r) => {
            let mut seen = BTreeSet::new();
            let mut all = r.clone();
            for b in &binds {
                all = join(all, reach(b, env, &mut seen));
            }
            let all = computed(all);
            FnFx {
                ret: all.clone(),
                egress: all,
            }
        }
        Local::Src(..) => FnFx {
            ret: computed(join_all(binds.iter().map(local_src))),
            egress: None,
        },
    }
}

/// The worst a call can do given `binds`: compute from, and release, everything it was given —
/// including everything a closure given to it can reach (what it closes over, the caller's names it
/// reads, closures those name in turn) — and every struct the callee, or a function or closure
/// given to it, may build.
fn worst(binds: &[Local], callee: Option<&str>, env: &Env) -> FnFx {
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let all = computed(join(
        join_all(binds.iter().map(|b| reach(b, env, &mut seen))),
        callee.and_then(|c| built_by(c, env)),
    ));
    FnFx {
        ret: all.clone(),
        egress: all,
    }
}

/// Everything a binding can reach.
fn reach(l: &Local, env: &Env, seen: &mut BTreeSet<usize>) -> Option<WholeSrc> {
    match l {
        Local::Src(s, _) => s.clone(),
        Local::Named(f) => built_by(f, env),
        Local::Opaque(r) => r.clone(),
        Local::Any(ls) => {
            let mut out = None;
            for x in ls {
                out = join(out, reach(x, env, seen));
            }
            out
        }
        Local::Closure(c) => {
            let n = env.intern(c);
            if !seen.insert(n) {
                return None;
            }
            let mut out = None;
            for v in c.captured.values() {
                out = join(out, reach(v, env, seen));
            }
            if c.use_scope {
                let free = env.free_names(c.id, &c.lam);
                for name in free.iter().filter(|n| !c.captured.contains_key(*n)) {
                    if let Some(x) = resolve_local(name, env, &top()) {
                        out = join(out, reach(&x, env, seen));
                    }
                }
            }
            join(out, builds_in(&c.lam, env))
        }
    }
}

/// What a function (or `impl`-prefixed method) may build.
fn built_by(f: &str, env: &Env) -> Option<WholeSrc> {
    env.ctx.whole_builders.get(f).cloned().map(WholeSrc::Value)
}

/// What an expression (a closure's body) may build: a struct literal, a builder's call.
fn builds_in(e: &Expr, env: &Env) -> Option<WholeSrc> {
    let pt = env.ctx.place_types();
    let mut out = None;
    visit::each_expr(e, &mut |x| {
        let hit = match x {
            Expr::StructLiteral { name, .. } => {
                whole_struct_source(name, &pt, SourceLane::Secret).map(WholeSrc::Value)
            }
            Expr::Call { callee, .. } | Expr::Var(callee) => built_by(callee, env),
            Expr::CallExpr { callee, .. } => match callee.as_ref() {
                Expr::FieldAccess { field, .. } => built_by(&format!("impl {field}"), env),
                _ => None,
            },
            _ => None,
        };
        out = join(out.take(), hit);
    });
    out
}

/// What a call binding a callee's formals to `binds` is keyed by (a closure by its number), and
/// whether the key names no closure (so it means the same in every query).
fn binds_key(binds: &[Local], env: &Env) -> (String, bool) {
    fn key(l: &Local, env: &Env, out: &mut String, portable: &mut bool) {
        match l {
            Local::Src(s, k) => out.push_str(&format!("S{}{}{:?}{s:?};", k.list, k.map, k.ty)),
            Local::Named(n) => out.push_str(&format!("N{n};")),
            Local::Opaque(r) => out.push_str(&format!("O{r:?};")),
            Local::Closure(c) => {
                *portable = false;
                out.push_str(&format!("C{};", env.intern(c)));
            }
            Local::Any(ls) => {
                out.push('A');
                for x in ls {
                    key(x, env, out, portable);
                }
                out.push(';');
            }
        }
    }
    let mut out = String::new();
    let mut portable = true;
    for b in binds {
        key(b, env, &mut out, &mut portable);
    }
    (out, portable)
}

/// The bindings a call of `route` is specialized for: its own, or — once the function has been
/// specialized `SUMMARY_AFTER` times in this query — the join of every later call's (widened as it
/// grows), which one specialization then covers.
fn shared_binds(route: &str, binds: Vec<Local>, env: &Env) -> Vec<Local> {
    let mut per = env.per_route.borrow_mut();
    let Some(RouteState {
        count,
        by_lambdas,
        shared,
        changes,
    }) = per.get_mut(route)
    else {
        return binds;
    };
    // The same lambdas over and over with different captures (a chain of wrappers) share sooner.
    let same_lambdas = lambdas_of(&binds);
    let repeated = !same_lambdas.is_empty()
        && by_lambdas.get(&same_lambdas).copied().unwrap_or(0) >= SAME_LAMBDAS_AFTER;
    if *count < SUMMARY_AFTER && !repeated {
        return binds;
    }
    if shared.is_empty() {
        *shared = binds.clone();
        return binds;
    }
    let none = Local::Src(None, Kind::NONE);
    let joined: Vec<Local> = (0..shared.len().max(binds.len()))
        .map(|i| {
            join_local(
                shared.get(i).unwrap_or(&none),
                binds.get(i).unwrap_or(&none),
            )
        })
        .collect();
    if joined != *shared {
        *changes += 1;
        *shared = if *changes > SHARED_CHANGES {
            joined.iter().map(|l| widen_with(l, env)).collect()
        } else {
            joined.iter().map(|l| merged(l, env)).collect()
        };
    }
    shared.clone()
}

/// A binding shared by many calls, kept finite: what it holds widened, and the closures it may be
/// merged per lambda (one closure of each, closing over what any of them closes over — a program's
/// lambdas are finitely many), to `MERGE_NEST` closures deep; below that a closure is opaque.
fn merged(l: &Local, env: &Env) -> Local {
    merged_at(l, 0, env)
}

fn merged_at(l: &Local, depth: usize, env: &Env) -> Local {
    match l {
        Local::Closure(_) if depth >= MERGE_NEST => widen_with(l, env),
        Local::Closure(c) => Local::Closure(merged_closure(c, depth, env)),
        Local::Any(ls) => {
            let mut vals: Option<Local> = None;
            let mut by_lambda: Vec<Closure> = Vec::new();
            let mut rest: Vec<Local> = Vec::new();
            for x in ls {
                match x {
                    Local::Closure(c) if depth < MERGE_NEST => match by_lambda
                        .iter_mut()
                        .find(|m| m.id == c.id && m.use_scope == c.use_scope)
                    {
                        Some(m) => {
                            m.captured = Rc::new(merged_locals(
                                &join_locals(&m.captured, &c.captured),
                                depth + 1,
                                env,
                            ));
                        }
                        None => by_lambda.push(merged_closure(c, depth, env)),
                    },
                    Local::Src(..) => {
                        vals = Some(match vals {
                            Some(v) => join_local(&v, x),
                            None => x.clone(),
                        })
                    }
                    other => {
                        let w = if matches!(other, Local::Closure(_)) {
                            widen_with(other, env)
                        } else {
                            other.clone()
                        };
                        if !rest.contains(&w) {
                            rest.push(w);
                        }
                    }
                }
            }
            let mut out: Vec<Local> = vals.map(|v| widen_local(&v)).into_iter().collect();
            out.extend(by_lambda.into_iter().map(Local::Closure));
            out.extend(rest);
            if out.len() == 1 {
                out.remove(0)
            } else {
                Local::Any(out)
            }
        }
        other => widen_local(other),
    }
}

/// A closure with what it closes over merged the same way, one closure level deeper.
fn merged_closure(c: &Closure, depth: usize, env: &Env) -> Closure {
    Closure {
        captured: Rc::new(merged_locals(&c.captured, depth + 1, env)),
        ..c.clone()
    }
}

fn merged_locals(ls: &Locals, depth: usize, env: &Env) -> Locals {
    ls.iter()
        .map(|(n, l)| (n.clone(), merged_at(l, depth, env)))
        .collect()
}

/// A recursive call — `route` already in progress: its bindings join the outermost such call's
/// (widened once they keep growing), and it reads that call's running result.
fn recur(route: &str, binds: &[Local], env: &Env) -> Option<FnFx> {
    let key = {
        let mut calls = env.calls.borrow_mut();
        let c = calls.iter_mut().find(|c| c.route == route)?;
        let none = Local::Src(None, Kind::NONE);
        let joined: Vec<Local> = (0..c.binds.len().max(binds.len()))
            .map(|i| {
                join_local(
                    c.binds.get(i).unwrap_or(&none),
                    binds.get(i).unwrap_or(&none),
                )
            })
            .collect();
        if joined != c.binds {
            c.binds = if c.grew >= 1 {
                joined.iter().map(|l| widen_with(l, env)).collect()
            } else {
                joined
            };
            c.grew += 1;
        }
        c.key.clone()
    };
    env.reads.borrow_mut().push(key.clone());
    Some(env.active.borrow().get(&key).cloned().unwrap_or_default())
}

/// Specialize `compute` for `route` with its formals bound to `binds`. A recursive call joins its
/// bindings in and reads the running result (starting from nothing, widened as it grows); the body
/// is rerun until both settle. Past the limits, or unsettled, the worst is assumed. A result that
/// read the running result of another call still in progress is kept only while that stands; one a
/// limit shaped only for the call depth it was computed at.
fn specialize(
    route: String,
    name: &str,
    binds: Vec<Local>,
    env: &Env,
    compute: impl Fn(&[Local]) -> FnFx,
) -> FnFx {
    if let Some(r) = recur(&route, &binds, env) {
        return r;
    }
    let binds: Vec<Local> = shared_binds(&route, binds, env)
        .into_iter()
        .map(|l| match l {
            Local::Src(s, k) => Local::Src(s.map(WholeSrc::capped), k),
            other => other,
        })
        .collect();
    let (bkey, portable) = binds_key(&binds, env);
    let key = format!("{route}|{bkey}");
    let depth = env.calls.borrow().len();
    let depth_key = format!("{key}#{depth}");
    for k in [&key, &depth_key] {
        if let Some(r) = env.done.borrow().get(k) {
            return r.clone();
        }
    }
    if let Some((ep, r, deps)) = env.tentative.borrow().get(&key) {
        if *ep == env.epoch.get() {
            env.reads.borrow_mut().extend(deps.iter().cloned());
            return r.clone();
        }
    }
    if let Some(r) = portable
        .then(|| env.ctx.whole_memo.borrow().get(&key).cloned())
        .flatten()
    {
        return FnFx {
            ret: r.0,
            egress: r.1,
        };
    }
    if depth >= MAX_CALL_DEPTH || env.work.get() >= MAX_WORK || env.exhausted() {
        env.fell_back();
        return worst(&binds, Some(name), env);
    }
    env.work.set(env.work.get() + 1);
    {
        let mut per = env.per_route.borrow_mut();
        let state = per.entry(route.clone()).or_default();
        state.count += 1;
        *state.by_lambdas.entry(lambdas_of(&binds)).or_default() += 1;
    }
    env.calls.borrow_mut().push(Active {
        route,
        key: key.clone(),
        binds,
        grew: 0,
    });
    env.active.borrow_mut().insert(key.clone(), FnFx::default());
    let reads_start = env.reads.borrow().len();
    let fallbacks_start = env.fallbacks.get();
    let mut result = FnFx::default();
    let mut settled = false;
    // A value moves one formal per rerun when a recursive call rotates them: allow one per formal.
    let limit = MAX_ITER
        .max(env.calls.borrow()[depth].binds.len() + 2)
        .min(4 * MAX_ITER);
    for iter in 0..limit {
        let (now, grew_before) = {
            let calls = env.calls.borrow();
            (calls[depth].binds.clone(), calls[depth].grew)
        };
        let mark = env.reads.borrow().len();
        let r = compute(&now);
        let assumed = env.active.borrow().get(&key).cloned().unwrap_or_default();
        let reread = env.reads.borrow()[mark..].iter().any(|k| k == &key);
        let grew = env.calls.borrow()[depth].grew != grew_before;
        let mut next = FnFx {
            ret: join(assumed.ret.clone(), r.ret.clone()),
            egress: join(assumed.egress.clone(), r.egress.clone()),
        };
        // Settled: the bindings stood, and the result computed from the running one adds nothing.
        if !grew && (!reread || next == assumed) {
            result = r;
            settled = true;
            break;
        }
        if iter >= 1 {
            next.ret = next.ret.map(WholeSrc::widen);
            next.egress = next.egress.map(WholeSrc::widen);
        }
        env.active.borrow_mut().insert(key.clone(), next);
        env.epoch.set(env.epoch.get() + 1);
        result = r;
    }
    let last = env
        .calls
        .borrow_mut()
        .pop()
        .map(|c| c.binds)
        .unwrap_or_default();
    if !settled {
        env.fell_back();
        let w = worst(&last, Some(name), env);
        result.ret = join(result.ret, w.ret);
        result.egress = join(result.egress, w.egress);
    }
    env.active.borrow_mut().remove(&key);
    result.ret = result.ret.map(WholeSrc::capped);
    let deps = env.deps_since(reads_start, Some(&key));
    if !deps.is_empty() {
        env.tentative
            .borrow_mut()
            .insert(key, (env.epoch.get(), result.clone(), deps));
    } else if env.fallbacks.get() != fallbacks_start {
        env.done.borrow_mut().insert(depth_key, result.clone());
    } else {
        // A callee's body reads only its formals: a result for bindings that name no closure (whose
        // numbers are the query's own) holds in every query of this program.
        if portable {
            env.ctx
                .whole_memo
                .borrow_mut()
                .insert(key.clone(), (result.ret.clone(), result.egress.clone()));
        }
        env.done.borrow_mut().insert(key, result.clone());
    }
    result
}

/// A call of user function `f` with its formals bound to `binds`.
fn call_fn(f: &str, binds: Vec<Local>, env: &Env) -> FnFx {
    let Some((params, body)) = env.ctx.whole_fns.get(f) else {
        return FnFx::default();
    };
    specialize(format!("fn|{f}"), f, binds, env, |b| {
        let at = callee_at(params, b, None);
        let flow = interp(body, None, env, &at, &None, Run::BODY);
        FnFx {
            ret: join(flow.ret, flow.val),
            egress: flow.egress,
        }
    })
}

/// A method call: the impls the receiver may run (`self` bound first). In the impl of a type without
/// a `secret` field, the receiver is not itself such a struct.
fn call_method(m: &str, binds: Vec<Local>, recv: &Recv, env: &Env) -> FnFx {
    let Some(impls) = env.ctx.whole_methods.get(m) else {
        return FnFx::default();
    };
    let pt = env.ctx.place_types();
    let secretless = |t: &str| whole_struct_source(t, &pt, SourceLane::Secret).is_none();
    // A struct released by `declassify` names nothing, whatever it passed through.
    let released = |t: &str| env.ctx.whole_released.contains(t);
    let mut chosen: Vec<&(String, WholeBody)> = match recv {
        Recv::Named(tys) => impls
            .iter()
            .filter(|(ty, _)| tys.contains(ty) || secretless(ty) || released(ty))
            .collect(),
        Recv::Sure(tys) => impls.iter().filter(|(ty, _)| tys.contains(ty)).collect(),
        Recv::Declared(tys) => impls
            .iter()
            .filter(|(ty, _)| tys.contains(ty) || released(ty))
            .collect(),
        Recv::Unknown => impls.iter().collect(),
    };
    if chosen.is_empty() {
        chosen = impls.iter().collect();
    }
    let which: Vec<&str> = chosen.iter().map(|(t, _)| t.as_str()).collect();
    specialize(
        format!("method|{m}@{}", which.join("|")),
        &format!("impl {m}"),
        binds,
        env,
        |b| {
            let mut out = FnFx::default();
            for (ty, (params, body)) in &chosen {
                let mut b = b.to_vec();
                if secretless(ty) {
                    if let Some(Local::Src(s, _)) = b.first_mut() {
                        *s = s.take().and_then(WholeSrc::strip_struct);
                    }
                }
                let at = callee_at(params, &b, Some(ty));
                let flow = interp(body, None, env, &at, &None, Run::BODY);
                out.ret = join_all([out.ret, flow.ret, flow.val]);
                out.egress = join(out.egress, flow.egress);
            }
            out
        },
    )
}

/// A callee body's position: its formals bound. A method's receiver (its first formal) is known to
/// be of the impl's type — the value, wherever it goes, not the name `self`.
fn callee_at(params: &[String], binds: &[Local], self_ty: Option<&str>) -> At {
    let mut locals = Locals::new();
    for (i, p) in params.iter().enumerate() {
        let mut l = binds
            .get(i)
            .cloned()
            .unwrap_or(Local::Src(None, Kind::NONE));
        if let (0, Some(t), Local::Src(_, k)) = (i, self_ty, &mut l) {
            k.ty = Some(Rc::from(t));
            k.sure = true;
        }
        locals.insert(p.clone(), l);
    }
    At {
        locals: Rc::new(locals),
        use_scope: false,
        depth: 0,
    }
}

// ------------------------------------------------------------------------------------------------
// Builtins

/// A builtin call. Arities and callback conventions follow the runtime (`call_closure` in
/// `backends/run.rs`); a builtin not named here computes from all its arguments.
fn builtin_src(f: &str, args: &[Expr], env: &Env, at: &At) -> Option<WholeSrc> {
    let a = |i: usize| args.get(i).and_then(|x| src(x, env, at));
    let a0 = a(0);
    let elem0 = a0.as_ref().and_then(WholeSrc::elem);
    let shp0 = shape(a0.clone());
    let moved0 = unpos(a0.clone());
    // What callback `args[i]` returns with its parameters bound to `binds`.
    let cb = |i: usize, binds: Vec<Local>| {
        let l = arg_local(args.get(i)?, env, at);
        apply_fx(&l, binds, env, at).ret
    };
    let one = |s: Option<WholeSrc>| vec![Local::Src(s, Kind::NONE)];
    let pair = |fs: Vec<(&str, Option<WholeSrc>)>| {
        let m: BTreeMap<String, WholeSrc> = fs
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (k.to_string(), v)))
            .collect();
        (!m.is_empty()).then(|| WholeSrc::holds(WholeSrc::Record(m)))
    };
    let rest = || holds(join_all((1..args.len()).map(a)));
    match f {
        // Shape reads.
        "len" | "is_empty" | "keys" | "type" => shp0,
        // `has_key` compares a key's rendering.
        "has_key" => join(shp0, computed(a(1))),
        "each" | "insert" => None,
        // Kept in place.
        "identity" | "secret_source" => a0,
        "take" => join(a0, computed(a(1))),
        "push" | "append" => join(a0, rest()),
        "first" => a0.as_ref().and_then(|s| s.at_pos("0")),
        "last" | "pop" => elem0,
        // Moved to other positions, or to another nesting level.
        "reverse" | "to_list" => moved0,
        "unshift" => join(moved0, rest()),
        "flatten" => join_all([
            shp0,
            shape(elem0.clone()),
            holds(elem0.and_then(|e| e.elem())),
        ]),
        "values" => join(shp0, holds(elem0)),
        "concat" => join(a0, unpos(a(1))),
        "merge" => join(a0, a(1)),
        "Some" | "Ok" => holds(a0),
        "None" if !args.is_empty() => {
            a0.map(|e| WholeSrc::Record([(WHOLE_ERR.to_string(), e)].into_iter().collect()))
        }
        "Err" => a0.map(|e| WholeSrc::Record([(WHOLE_ERR.to_string(), e)].into_iter().collect())),
        "drop" | "skip" => join(moved0, computed(a(1))),
        "chunk" | "window" => join_all([shp0, holds(holds(elem0)), computed(a(1))]),
        "slice" => join_all([moved0, computed(a(1)), computed(a(2))]),
        // A key or position chooses what is read; a struct yields the default.
        "get" => {
            let base = a0.and_then(WholeSrc::strip_struct);
            let from = match args.get(1) {
                Some(Expr::StrLiteral(k)) => base.and_then(|b| b.at_key(k, env.ctx)),
                Some(Expr::Literal(k)) if is_position(k) => base.and_then(|b| b.at_pos(k)),
                _ => join(base.and_then(|b| b.index_elem()), computed(a(1))),
            };
            join(from, a(2))
        }
        "remove" => join(
            a0.and_then(WholeSrc::strip_struct)
                .and_then(|b| b.index_elem()),
            computed(a(1)),
        ),
        "zip" => {
            let a1 = a(1);
            join_all([
                shp0,
                shape(a1.clone()),
                pair(vec![("0", elem0), ("1", a1.and_then(|s| s.elem()))]),
            ])
        }
        "enumerate" => join(shp0, pair(vec![("1", elem0)])),
        "entries" => join(shp0.clone(), pair(vec![("0", shp0), ("1", elem0)])),
        // Selection by what the callback returns: which elements survive, their order, or which one.
        "filter" | "take_while" | "drop_while" => join(moved0, computed(cb(1, one(elem0.clone())))),
        "partition" => join_all([
            shp0,
            holds(holds(elem0.clone())),
            computed(cb(1, one(elem0))),
        ]),
        "sort_by" => join(moved0, holds(computed(cb(1, one(elem0.clone()))))),
        "find" | "min_by" | "max_by" => join(elem0.clone(), computed(cb(1, one(elem0)))),
        // Choice or order by comparing whole values.
        "sort" => join(shp0, holds(computed(elem0))),
        "unique" | "dedup" => computed(a0),
        // The runtime spreads only a list: of anything else, the value itself.
        "max" | "min" if args.len() == 1 => {
            // A declared or inferred struct type is not enforced (TY-UNENFORCED): only a value known
            // to be a map is surely not spread.
            let k = kind_of(&args[0], env, at);
            if k.list {
                computed(elem0)
            } else if k.map {
                a0.clone()
            } else {
                join(computed(elem0), a0.as_ref().and_then(WholeSrc::self_part))
            }
        }
        // A bool or index decided per element.
        "any" | "all" | "count" | "position" => join(shp0, computed(cb(1, one(elem0)))),
        // What the callback returns.
        "map" | "map_values" => join(shp0, holds(cb(1, one(elem0)))),
        "flat_map" => {
            let r = cb(1, one(elem0));
            join_all([shp0, shape(r.clone()), holds(r.and_then(|r| r.elem()))])
        }
        "times" => join(computed(a0), holds(cb(1, one(None)))),
        "reduce" => join(shp0, reduce_src(args, env, at)),
        "call" => cb(0, args_locals(args.get(1..).unwrap_or(&[]), env, at)),
        "apply" => cb(0, apply_binds(args, env, at)),
        // A function value holds nothing until it is called.
        "compose" => None,
        // Everything else computes from every argument, and from what any callback returns.
        _ => {
            let data = join_all(args.iter().map(|x| src(x, env, at)));
            let returned = join_all(
                args.iter()
                    .enumerate()
                    .filter(|(_, x)| is_callback(x, env, at))
                    .map(|(i, x)| {
                        let n = arity(&arg_local(x, env, at), env).max(SPREAD);
                        cb(i, vec![Local::Src(data.clone(), Kind::NONE); n])
                    }),
            );
            computed(join(data, returned))
        }
    }
}

/// Whether an argument is a function value an unknown builtin may call: a lambda, or a name bound to
/// a closure, a user function or a builtin.
fn is_callback(x: &Expr, env: &Env, at: &At) -> bool {
    match x {
        Expr::Lambda { .. } => true,
        Expr::Var(_) => !matches!(arg_local(x, env, at), Local::Src(..)),
        _ => false,
    }
}

/// `apply(f, xs)` spreads a list into `f`'s parameters (each gets what that position holds); anything
/// else is passed whole, as the one argument.
fn apply_binds(args: &[Expr], env: &Env, at: &At) -> Vec<Local> {
    match args.get(1) {
        Some(Expr::ArrayLiteral { elements }) => {
            elements.iter().map(|x| arg_local(x, env, at)).collect()
        }
        Some(x) => {
            let s = src(x, env, at);
            if matches!(s, Some(WholeSrc::Value(_))) {
                return vec![Local::Src(s, Kind::NONE)];
            }
            let n = args
                .first()
                .map_or(0, |f| arity(&arg_local(f, env, at), env))
                .max(SPREAD);
            let mut binds: Vec<Local> = (0..n)
                .map(|i| {
                    Local::Src(
                        s.as_ref().and_then(|s| s.at_pos(&i.to_string())),
                        Kind::NONE,
                    )
                })
                .collect();
            if !is_listish(x, env, at) {
                let first = join(local_src(&binds[0]), s);
                binds[0] = Local::Src(first, Kind::NONE);
            }
            binds
        }
        None => Vec::new(),
    }
}

/// `reduce(xs, f)` seeds the accumulator with the first element; `reduce(xs, f, seed)` /
/// `reduce(xs, seed, f)` (the runtime takes whichever is a closure) with the seed and then with what
/// `f` returned, to a fixpoint (widened, and past that the worst). The result is the final
/// accumulator.
fn reduce_src(args: &[Expr], env: &Env, at: &At) -> Option<WholeSrc> {
    let elem = args
        .first()
        .and_then(|x| src(x, env, at))
        .and_then(|s| s.elem());
    let fold = |cb: &Expr, seed: Option<WholeSrc>| {
        let l = arg_local(cb, env, at);
        let mut acc = seed.clone();
        for k in 0..MAX_ITER {
            let out = apply_fx(
                &l,
                vec![
                    Local::Src(acc.clone(), Kind::NONE),
                    Local::Src(elem.clone(), Kind::NONE),
                ],
                env,
                at,
            )
            .ret;
            let mut next = join(acc.clone(), out);
            if k >= 2 {
                next = next.map(WholeSrc::widen);
            }
            if next == acc {
                return acc;
            }
            acc = next;
        }
        env.fell_back();
        let w = worst(
            &[
                l.clone(),
                Local::Src(elem.clone(), Kind::NONE),
                Local::Src(seed, Kind::NONE),
            ],
            None,
            env,
        );
        computed(join(acc, w.ret))
    };
    match args {
        [_, f] => fold(f, elem.clone()),
        [_, a, b] => {
            let orders: Vec<(&Expr, &Expr)> =
                match (is_callback(a, env, at), is_callback(b, env, at)) {
                    (true, false) => vec![(a, b)],
                    (false, true) => vec![(b, a)],
                    _ => vec![(a, b), (b, a)],
                };
            join_all(orders.into_iter().map(|(f, s)| fold(f, src(s, env, at))))
        }
        _ => None,
    }
}

/// Where a builtin's callbacks make a whole struct reach an egress, run with their parameters bound
/// as the builtin binds them.
fn builtin_egress(f: &str, args: &[Expr], env: &Env, at: &At) -> Option<WholeSrc> {
    if !args.iter().any(may_pass_fn) {
        return None;
    }
    let a = |i: usize| args.get(i).and_then(|x| src(x, env, at));
    let run = |i: usize, binds: Vec<Local>| {
        let l = arg_local(args.get(i)?, env, at);
        if matches!(l, Local::Src(..)) {
            return None;
        }
        apply_fx(&l, binds, env, at).egress
    };
    let one = |s: Option<WholeSrc>| vec![Local::Src(s, Kind::NONE)];
    match f {
        "map" | "flat_map" | "map_values" | "each" | "filter" | "find" | "any" | "all"
        | "count" | "sort_by" | "take_while" | "drop_while" | "position" | "min_by" | "max_by"
        | "partition" => run(1, one(a(0).and_then(|s| s.elem()))),
        "times" => run(1, one(None)),
        "reduce" => {
            let acc = reduce_src(args, env, at);
            let binds = vec![
                Local::Src(acc, Kind::NONE),
                Local::Src(a(0).and_then(|s| s.elem()), Kind::NONE),
            ];
            join(run(1, binds.clone()), run(2, binds))
        }
        "call" => run(0, args_locals(args.get(1..).unwrap_or(&[]), env, at)),
        "apply" => run(0, apply_binds(args, env, at)),
        _ => {
            let lambdas: Vec<usize> = args
                .iter()
                .enumerate()
                .filter(|(_, x)| is_callback(x, env, at))
                .map(|(i, _)| i)
                .collect();
            if lambdas.is_empty() {
                return None;
            }
            let data = join_all(args.iter().map(|x| src(x, env, at)));
            join_all(lambdas.into_iter().map(|i| {
                let n = arity(&arg_local(&args[i], env, at), env).max(SPREAD);
                run(i, vec![Local::Src(data.clone(), Kind::NONE); n])
            }))
        }
    }
}
