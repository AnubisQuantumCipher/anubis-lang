//! Interprocedural struct-type inference for unannotated function parameters (item-21 D9).
//!
//! A declared struct field qualifier (`struct S { k: secret<i64> }`) is only honored when the analysis
//! knows the base value's struct type. An unannotated formal (`fn leak(s) { print(s.k) }`) had type
//! `""`, so `s.k` resolved to no struct, the `secret` on `k` was never consulted, and the program
//! checked clean while printing the secret. The annotated twin `fn leak(s: S)` was rejected.
//!
//! This pass gives such a formal the struct type its callers actually pass, so every later lane sees
//! exactly what it would see had the user written the annotation. It infers a type ONLY when that is
//! provable from the program text:
//!
//! - the function is a free function with no generics;
//! - its name is never used as a value anywhere (not passed, stored, aliased or returned), so every
//!   call to it is a direct call the pass can see;
//! - at every direct call site the argument's struct type is known (a struct literal, a caller
//!   parameter whose type is declared or already inferred and which is never re-bound or assigned, a
//!   caller local bound exactly once and never assigned, or a call to a function with a declared struct
//!   return type), and all call sites agree.
//!
//! Anything else — no call sites, a disagreement, one unknown argument, an escaping function — leaves
//! the parameter unannotated, which is exactly the previous behavior. The pass only ever adds a type
//! that every real call satisfies, so it can make the analysis stricter (a declared qualifier is now
//! honored) but not more permissive. It is a fixpoint so a type flows through forwarding chains
//! (`fn a(s) { b(s) }`).
//!
//! Residual (recorded, not closed here): an unannotated formal whose type cannot be inferred this way
//! still resolves no struct type, so a declared qualifier on a field read through it is not applied.

use std::collections::{BTreeMap, BTreeSet};

use crate::frontend::{Expr, Item, Pattern, Stmt};

use super::visit::{each_expr, each_expr_in_items, each_expr_in_stmts, each_fn_item};

/// One inferred annotation: `(function, parameter index, struct type)`.
pub(crate) type Inferred = (String, usize, String);

/// Infer struct types for unannotated free-function parameters and write them into `items`.
/// Returns what was inferred, for diagnostics and tests.
pub(crate) fn infer_unannotated_struct_params(items: &mut [Item]) -> Vec<Inferred> {
    let structs = declared_structs(items);
    if structs.is_empty() {
        return Vec::new();
    }
    let free_fns = free_fn_names(items);
    let escaping = fns_used_as_values(items, &free_fns);
    let fn_ret: BTreeMap<String, String> = {
        let mut m = BTreeMap::new();
        each_fn_item(items, &mut |it| {
            if let Item::Fn {
                name, ret: Some(r), ..
            } = it
            {
                if structs.contains(r.trim()) {
                    m.insert(name.clone(), r.trim().to_string());
                }
            }
        });
        m
    };

    // Current parameter types of every free function: declared, or inferred in an earlier round.
    let mut param_tys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut candidates: BTreeSet<(String, usize)> = BTreeSet::new();
    collect_free_fns(items, &mut |name, params, generics| {
        param_tys.insert(
            name.to_string(),
            params.iter().map(|(_, t)| t.trim().to_string()).collect(),
        );
        if generics || escaping.contains(name) {
            return;
        }
        for (i, (_, t)) in params.iter().enumerate() {
            if t.trim().is_empty() {
                candidates.insert((name.to_string(), i));
            }
        }
    });

    // The free-function items themselves, by address: a method can share a free function's NAME, and
    // must never be given that function's inferred parameter types.
    let free_fn_items: BTreeSet<*const Item> = {
        let mut out = BTreeSet::new();
        collect_free_fn_items(items, &mut out);
        out
    };

    let mut inferred: BTreeMap<(String, usize), String> = BTreeMap::new();
    // Each round can only turn a parameter from untyped to typed, so this terminates within
    // `candidates.len()` productive rounds.
    for _ in 0..=candidates.len() {
        let mut observed: BTreeMap<(String, usize), Option<BTreeSet<String>>> = candidates
            .iter()
            .filter(|k| !inferred.contains_key(*k))
            .map(|k| (k.clone(), Some(BTreeSet::new())))
            .collect();
        if observed.is_empty() {
            break;
        }
        each_fn_item(items, &mut |item| {
            let Item::Fn {
                params,
                body,
                requires,
                ensures,
                ..
            } = item
            else {
                return;
            };
            let caller = CallerFacts::new(
                params,
                body,
                &param_tys_for(item, &param_tys, &free_fn_items),
            );
            let mut visit = |e: &Expr| {
                let Expr::Call { callee, args } = e else {
                    return;
                };
                for (i, arg) in args.iter().enumerate() {
                    let key = (callee.clone(), i);
                    let Some(slot) = observed.get_mut(&key) else {
                        continue;
                    };
                    match (
                        slot.as_mut(),
                        caller.struct_type_of(arg, &fn_ret, &structs, 0),
                    ) {
                        (Some(set), Some(t)) => {
                            set.insert(t);
                        }
                        _ => *slot = None,
                    }
                }
            };
            for c in requires.iter().chain(ensures) {
                each_expr(c, &mut visit);
            }
            each_expr_in_stmts(body, &mut visit);
        });
        let mut progressed = false;
        for (key, seen) in observed {
            if let Some(set) = seen {
                if set.len() == 1 {
                    let t = set.into_iter().next().expect("one element");
                    if let Some(ptys) = param_tys.get_mut(&key.0) {
                        ptys[key.1] = t.clone();
                    }
                    inferred.insert(key, t);
                    progressed = true;
                }
            }
        }
        if !progressed {
            break;
        }
    }

    if !inferred.is_empty() {
        write_back(items, &inferred);
    }
    inferred.into_iter().map(|((f, i), t)| (f, i, t)).collect()
}

/// Parameter types for `item` as the pass currently knows them: a free function's declared-or-inferred
/// types; any other function item (impl or trait method) keeps exactly its declared types.
fn param_tys_for(
    item: &Item,
    param_tys: &BTreeMap<String, Vec<String>>,
    free_fn_items: &BTreeSet<*const Item>,
) -> Vec<String> {
    let Item::Fn { name, params, .. } = item else {
        return Vec::new();
    };
    let declared = || params.iter().map(|(_, t)| t.trim().to_string()).collect();
    if !free_fn_items.contains(&(item as *const Item)) {
        return declared();
    }
    match param_tys.get(name) {
        Some(tys) if tys.len() == params.len() => tys.clone(),
        _ => declared(),
    }
}

fn collect_free_fn_items(items: &[Item], out: &mut BTreeSet<*const Item>) {
    for it in items {
        match it {
            Item::Fn { .. } => {
                out.insert(it as *const Item);
            }
            Item::Module { items, .. } => collect_free_fn_items(items, out),
            _ => {}
        }
    }
}

fn declared_structs(items: &[Item]) -> BTreeSet<String> {
    fn walk(items: &[Item], out: &mut BTreeSet<String>) {
        for it in items {
            match it {
                Item::Struct { name, generics, .. } if generics.is_empty() => {
                    out.insert(name.clone());
                }
                Item::Module { items, .. } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(items, &mut out);
    out
}

/// Free functions (top level and inside modules), not impl or trait methods.
/// Visitor for `collect_free_fns`: `(name, params, is_generic)`.
type FreeFnVisitor<'v> = dyn FnMut(&str, &[(String, String)], bool) + 'v;

fn collect_free_fns(items: &[Item], f: &mut FreeFnVisitor<'_>) {
    for it in items {
        match it {
            Item::Fn {
                name,
                params,
                generics,
                ..
            } => f(name, params, !generics.is_empty()),
            Item::Module { items, .. } => collect_free_fns(items, f),
            _ => {}
        }
    }
}

pub(super) fn free_fn_names(items: &[Item]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_free_fns(items, &mut |n, _, _| {
        out.insert(n.to_string());
    });
    out
}

/// Free functions whose name appears anywhere as a VALUE (an `Expr::Var`), rather than only as the
/// callee of a direct `Expr::Call`. Such a function may be invoked through a value the pass cannot
/// see, so its parameters are never inferred. Deliberately over-approximate: a local that merely
/// shares the name also counts.
fn fns_used_as_values(items: &[Item], free_fns: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    each_expr_in_items(items, &mut |e| {
        if let Expr::Var(v) = e {
            if free_fns.contains(v) {
                out.insert(v.clone());
            }
        }
    });
    out
}

/// What a caller's body says about the names it binds, enough to type an argument soundly.
struct CallerFacts<'a> {
    /// Caller parameters with their (declared or inferred) types.
    params: BTreeMap<&'a str, String>,
    /// For each name: how many times it is bound anywhere in the body (let, pattern, for var,
    /// lambda parameter), and whether it is ever the root of an assignment target.
    binds: BTreeMap<String, usize>,
    assigned: BTreeSet<String>,
    /// The single `let` of a name bound exactly once: its annotation and initializer.
    lets: BTreeMap<String, (Option<String>, &'a Expr)>,
}

impl<'a> CallerFacts<'a> {
    fn new(params: &'a [(String, String)], body: &'a [Stmt], ptys: &[String]) -> Self {
        let mut facts = CallerFacts {
            params: params
                .iter()
                .enumerate()
                .map(|(i, (n, t))| {
                    let t = ptys.get(i).cloned().unwrap_or_else(|| t.trim().to_string());
                    (n.as_str(), t)
                })
                .collect(),
            binds: BTreeMap::new(),
            assigned: BTreeSet::new(),
            lets: BTreeMap::new(),
        };
        facts.scan_stmts(body);
        // Expression-level binders (lambda parameters, match / if-let pattern names) and every
        // assignment nested anywhere, including inside value blocks and lambdas.
        let mut extra_binds: Vec<String> = Vec::new();
        let mut extra_assigned: Vec<String> = Vec::new();
        each_expr_in_stmts(body, &mut |e| match e {
            Expr::Lambda { params, .. } => extra_binds.extend(params.iter().cloned()),
            Expr::Match { arms, .. } => {
                for arm in arms {
                    extra_binds.extend(arm.pattern.bound_names());
                }
            }
            Expr::IfLet { pattern, .. } => extra_binds.extend(pattern.bound_names()),
            Expr::Block { stmts, .. } => {
                let mut sub = CallerFacts {
                    params: BTreeMap::new(),
                    binds: BTreeMap::new(),
                    assigned: BTreeSet::new(),
                    lets: BTreeMap::new(),
                };
                sub.scan_stmts(stmts);
                for (n, c) in sub.binds {
                    for _ in 0..c {
                        extra_binds.push(n.clone());
                    }
                }
                extra_assigned.extend(sub.assigned);
            }
            _ => {}
        });
        for n in extra_binds {
            *facts.binds.entry(n).or_default() += 1;
        }
        facts.assigned.extend(extra_assigned);
        facts
    }

    fn bind(&mut self, name: &str) {
        *self.binds.entry(name.to_string()).or_default() += 1;
    }

    fn bind_pattern(&mut self, p: &Pattern) {
        for n in p.bound_names() {
            self.bind(&n);
        }
    }

    /// Record `let`s, pattern binders, loop variables and assignment roots in a statement list.
    fn scan_stmts(&mut self, stmts: &'a [Stmt]) {
        for s in stmts {
            match s {
                Stmt::Let { name, ty, init, .. } => {
                    self.bind(name);
                    self.lets.insert(name.clone(), (ty.clone(), init));
                }
                Stmt::LetPattern { pattern, .. } => self.bind_pattern(pattern),
                Stmt::WhileLet { pattern, body, .. } => {
                    self.bind_pattern(pattern);
                    self.scan_stmts(body);
                }
                Stmt::Assign { target, .. } => {
                    if let Some(root) = assign_root(target) {
                        self.assigned.insert(root);
                    }
                }
                Stmt::If { then, else_, .. } => {
                    self.scan_stmts(then);
                    if let Some(e) = else_ {
                        self.scan_stmts(e);
                    }
                }
                Stmt::While { body, .. } | Stmt::Loop { body, .. } => self.scan_stmts(body),
                Stmt::For { var, body, .. } => {
                    self.bind(var);
                    self.scan_stmts(body);
                }
                Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                    self.scan_stmts(body)
                }
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        self.scan_stmts(b);
                    }
                }
                Stmt::ExprStmt(_) | Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
            }
        }
    }

    /// The struct type `arg` definitely has at runtime, or `None` when that is not provable here.
    fn struct_type_of(
        &self,
        arg: &Expr,
        fn_ret: &BTreeMap<String, String>,
        structs: &BTreeSet<String>,
        depth: u32,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let known = |t: &str| structs.contains(t.trim()).then(|| t.trim().to_string());
        match arg {
            Expr::StructLiteral { name, .. } => known(name),
            Expr::Var(v) => {
                if self.assigned.contains(v) {
                    return None;
                }
                if let Some(t) = self.params.get(v.as_str()) {
                    // A parameter re-bound anywhere in the body is ambiguous at the call site.
                    return if self.binds.contains_key(v) {
                        None
                    } else {
                        known(t)
                    };
                }
                if self.binds.get(v).copied() != Some(1) {
                    return None;
                }
                let (ty, init) = self.lets.get(v)?;
                match ty.as_deref().map(str::trim) {
                    Some(t) if !t.is_empty() => known(t),
                    _ => self.struct_type_of(init, fn_ret, structs, depth + 1),
                }
            }
            // A direct call to a function with a declared struct return type, provided no local of
            // the same name shadows it.
            Expr::Call { callee, .. } => {
                if self.binds.contains_key(callee) || self.params.contains_key(callee.as_str()) {
                    return None;
                }
                fn_ret.get(callee).cloned()
            }
            _ => None,
        }
    }
}

/// The variable at the root of an assignment target (`x`, `x.f`, `x[i].g` → `x`).
fn assign_root(target: &Expr) -> Option<String> {
    match target {
        Expr::Var(v) => Some(v.clone()),
        Expr::FieldAccess { base, .. } | Expr::Index { base, .. } => assign_root(base),
        _ => None,
    }
}

fn write_back(items: &mut [Item], inferred: &BTreeMap<(String, usize), String>) {
    for it in items.iter_mut() {
        match it {
            Item::Fn { name, params, .. } => {
                for (i, (_, t)) in params.iter_mut().enumerate() {
                    if let Some(new) = inferred.get(&(name.clone(), i)) {
                        *t = new.clone();
                    }
                }
            }
            Item::Module { items, .. } => write_back(items, inferred),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse_source;

    fn infer(src: &str) -> Vec<Inferred> {
        let mut ast = parse_source(src).expect("parse");
        infer_unannotated_struct_params(&mut ast.items)
    }

    const S: &str = "struct S { k: secret<i64> }\nstruct T { k: i64 }\n";

    #[test]
    fn infers_from_a_single_agreeing_call_site() {
        let r = infer(&format!(
            "{S}fn leak(s) {{ print(s.k); }}\nfn main() {{ let a = S {{ k: 1 }}; leak(a); }}"
        ));
        assert_eq!(r, vec![("leak".into(), 0, "S".into())]);
    }

    #[test]
    fn flows_through_forwarding_and_literals_and_declared_returns() {
        let r = infer(&format!(
            "{S}fn mk() -> S {{ return S {{ k: 1 }}; }}\nfn leak(s) {{ print(s.k); }}\nfn mid(s) {{ leak(s); }}\n\
             fn main() {{ mid(S {{ k: 1 }}); mid(mk()); }}"
        ));
        assert!(r.contains(&("mid".into(), 0, "S".into())), "{r:?}");
        assert!(r.contains(&("leak".into(), 0, "S".into())), "{r:?}");
    }

    #[test]
    fn refuses_disagreement_unknowns_escapes_and_rebinding() {
        // Two call sites with different struct types.
        assert!(infer(&format!(
            "{S}fn f(s) {{ print(s.k); }}\nfn main() {{ f(S {{ k: 1 }}); f(T {{ k: 1 }}); }}"
        ))
        .is_empty());
        // One argument of unknown type.
        assert!(infer(&format!(
            "{S}fn f(s) {{ print(s.k); }}\nfn main() {{ let xs = [S {{ k: 1 }}]; f(xs[0]); }}"
        ))
        .is_empty());
        // The function escapes as a value, so there may be calls the pass cannot see.
        assert!(infer(&format!("{S}fn f(s) {{ print(s.k); }}\nfn main() {{ f(S {{ k: 1 }}); let g = f; g(T {{ k: 1 }}); }}")).is_empty());
        // The argument variable is reassigned.
        assert!(infer(&format!("{S}fn f(s) {{ print(s.k); }}\nfn main() {{ let mut a = S {{ k: 1 }}; a = T {{ k: 1 }}; f(a); }}")).is_empty());
        // The argument variable is bound twice (shadowed).
        assert!(infer(&format!("{S}fn f(s) {{ print(s.k); }}\nfn main() {{ let a = S {{ k: 1 }}; let a = T {{ k: 1 }}; f(a); }}")).is_empty());
        // A METHOD named like a free function must not borrow that function's parameter types. With
        // equal parameter counts, the old name-keyed lookup typed the method's `s` as the free
        // function's inferred `S`, so the method's `sink(s)` (which passes a `T`) counted as evidence
        // and `sink` was inferred `S` — unsound.
        assert!(!infer(&format!(
            "{S}fn show(x, s) {{ sink(s); }}\nfn sink(t) {{ print(t.k); }}\nstruct W {{ n: i64 }}\n\
             impl W {{ fn show(self, s) {{ sink(s); }} }}\n\
             fn main() {{ show(1, S {{ k: 1 }}); let w = W {{ n: 1 }}; w.show(T {{ k: 1 }}); }}"
        ))
        .contains(&("sink".into(), 0, "S".into())));
        // No call sites at all.
        assert!(infer(&format!(
            "{S}fn f(s) {{ print(s.k); }}\nfn main() {{ print(1); }}"
        ))
        .is_empty());
    }
}
