//! The interpreter: statements and expressions evaluated over abstract values, following the
//! runtime's lowering (`backends/run.rs`) construct by construct.
//!
//! State is a [`Path`]: the lexical scopes of the running function with their values, the labels
//! that decided control reaches this point, and the exits taken on the way (a `return`, `break` or
//! `continue` on some path). Branches evaluate each side from a copy and merge; loops iterate to a
//! fixpoint with widening; calls are analyzed per (callee, arguments, program counter) and
//! memoized, iterating recursive calls to a fixpoint.
//!
//! Implicit flows. `St::cond` is the label of the conditions the current branch sits under.
//! A `return` (or an `exit` or `panic`, which end the program) taken under some label means the
//! code after it runs only when it was not taken: the continuation's `lift_ret` joins that label
//! for the rest of the function, and a call whose callee may end the program under a label lifts
//! its caller's continuation the same way. A `break` does the same for the rest of the loop body
//! and every later iteration (`lift_loop`), a `continue` for the rest of the body only (the next
//! iteration runs either way), and a loop's header decides whether the next iteration's header
//! runs; the loop's exit restores the outer `lift_loop`. The program counter at a point is
//! `cond ⊔ lift_ret ⊔ lift_loop`; an egress executed there with a secret program counter is an
//! implicit flow (decision D1).

use super::builtins;
use super::value::{self, join_env, Clo, EnumV, Env, FnSet, Lab, LamId, ListV, MapV, V};
use crate::frontend::{
    EnumVariant, EnumVariantKind, Expr, ForSource, Item, MatchArm, Pattern, Stmt,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

/// Abstract steps before the analysis gives up (fail closed).
const STEP_BUDGET: u64 = 1_500_000;
/// Work on abstract values (`value::work`) before the analysis gives up (fail closed).
const WORK_BUDGET: u64 = 40_000_000;
/// Loop iterations before widening.
const WIDEN_AFTER: usize = 2;
/// Loop iterations before the analysis gives up on a loop (fail closed).
const LOOP_LIMIT: usize = 60;
/// Distinct argument contexts per callee before they are joined into summary contexts.
const CONTEXTS_PER_CALLEE: usize = 12;
/// Global rounds of the closure-summary and store fixpoint.
const SUMMARY_ROUNDS: usize = 12;
/// Data depth a summary context's joined arguments keep.
const SUMMARY_DEPTH: usize = 4;
/// Rounds after which a loop (or a composition, or a reduce) that has not settled is widened at
/// `SUMMARY_DEPTH`, so it settles within `LOOP_LIMIT`.
const COARSE_AFTER: usize = 12;

/// A finding: an information flow the program performs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    pub code: &'static str,
    pub message: String,
}

/// A user function or method as the interpreter needs it.
struct FnDef<'a> {
    name: Rc<str>,
    params: &'a [(String, String)],
    body: &'a [Stmt],
    ret: Option<&'a str>,
    /// The runtime's call-resolution set (`collect_local_names`): params and every name bound
    /// anywhere in the body, nested lambdas' params included.
    locals: Rc<BTreeSet<String>>,
}

struct LamInfo<'a> {
    params: &'a [String],
    body: &'a Expr,
    /// The enclosing function's call-resolution set (lambda bodies are lowered with its `ctx`).
    owner_locals: Rc<BTreeSet<String>>,
    /// The names the runtime snapshots at creation.
    captures: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Callee {
    User(Rc<str>),
    Method(Rc<str>, Rc<str>),
    Lam(LamId, Clo),
    /// A summary context of a callee: every call past its precise contexts whose arguments have
    /// this joined label (`V::deep`) and hold these functions, analyzed with the join of their
    /// arguments.
    Summary(Rc<Callee>, Vec<Lab>, Rc<Vec<String>>),
}

type CallKey = (Callee, Vec<V>, Lab);

/// A call being solved: its key, whether its result is provisional, and the in-progress
/// approximations it read (key, value then).
type Solving = (CallKey, bool, Vec<(CallKey, V)>);

/// The state along a path.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct St {
    scopes: Vec<BTreeMap<String, V>>,
    cond: Lab,
    lift_ret: Lab,
    lift_loop: Lab,
}

impl St {
    pub fn pc(&self) -> Lab {
        self.cond.join(self.lift_ret).join(self.lift_loop)
    }
    fn lookup(&self, n: &str) -> Option<&V> {
        self.scopes.iter().rev().find_map(|s| s.get(n))
    }
    fn assign(&mut self, n: &str, v: V) -> bool {
        for s in self.scopes.iter_mut().rev() {
            if let Some(slot) = s.get_mut(n) {
                *slot = v;
                return true;
            }
        }
        false
    }
    fn bind(&mut self, n: &str, v: V) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(n.to_string(), v);
        }
    }
    fn join(&self, o: &St) -> St {
        // Paths merging at one point have the same scope structure; a name bound on one path
        // only (it cannot be read after the merge in a program that compiles) keeps its value.
        let n = self.scopes.len().min(o.scopes.len());
        let mut scopes = Vec::with_capacity(n);
        for i in 0..n {
            scopes.push(join_env(&self.scopes[i], &o.scopes[i]));
        }
        St {
            scopes,
            cond: self.cond.join(o.cond),
            lift_ret: self.lift_ret.join(o.lift_ret),
            lift_loop: self.lift_loop.join(o.lift_loop),
        }
    }
    fn widen(&self, it: &mut Interp<'_>) -> St {
        self.widen_to(it, super::value::DATA_DEPTH)
    }
    fn widen_to(&self, it: &mut Interp<'_>, depth: usize) -> St {
        let mut s = self.clone();
        for scope in &mut s.scopes {
            for v in scope.values_mut() {
                *v = it.widen_to(v, depth);
            }
        }
        s
    }
}

impl St {
    /// Whether every value of `self` is ⊑ the same name's in `o`, with no larger labels.
    fn leq(&self, o: &St) -> bool {
        self.scopes.len() == o.scopes.len()
            && self
                .scopes
                .iter()
                .zip(&o.scopes)
                .all(|(a, b)| super::value::env_leq(a, b))
            && (self.cond.0 & !o.cond.0) == 0
            && (self.lift_ret.0 & !o.lift_ret.0) == 0
            && (self.lift_loop.0 & !o.lift_loop.0) == 0
    }
}

impl St {
    pub(crate) fn lookup_pub(&self, n: &str) -> Option<&V> {
        self.lookup(n)
    }
    pub(crate) fn assign_pub(&mut self, n: &str, v: V) -> bool {
        self.assign(n, v)
    }
}

impl Path {
    /// A branch of this path under the extra condition `raise`, with no exits of its own.
    pub(crate) fn fork(&self, raise: Lab) -> Path {
        Path {
            st: self.st.clone().map(|mut s| {
                s.cond = s.cond.join(raise);
                s
            }),
            exits: Exits::default(),
        }
    }

    /// Carry back what a fork of this path learned about the continuation: a call in it that may
    /// end the program, or a `return`-like exit, lifts this path's continuation too.
    pub(crate) fn absorb_lift(&mut self, fork: &Path) {
        let lift = fork
            .st
            .as_ref()
            .map(|s| s.lift_ret)
            .unwrap_or_default()
            .join(fork.exits.ret_at.unwrap_or_default())
            .control();
        if let Some(st) = &mut self.st {
            st.lift_ret = st.lift_ret.join(lift);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_live() -> Path {
        Path {
            st: Some(St {
                scopes: vec![BTreeMap::new()],
                cond: Lab::PUB,
                lift_ret: Lab::PUB,
                lift_loop: Lab::PUB,
            }),
            exits: Exits::default(),
        }
    }
}

fn join_opt(a: Option<St>, b: Option<St>) -> Option<St> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(a.join(&b)),
    }
}

fn join_lab_opt(a: Option<Lab>, b: Option<Lab>) -> Option<Lab> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(a.join(b)),
    }
}

/// Exits taken along a path: the states at `break` and `continue`, and the program counters at
/// which a `return` (or program exit), `break` or `continue` was taken.
#[derive(Default, Clone)]
struct Exits {
    brk: Option<St>,
    cnt: Option<St>,
    ret_at: Option<Lab>,
    brk_at: Option<Lab>,
    cnt_at: Option<Lab>,
}

impl Exits {
    fn absorb(&mut self, o: Exits) {
        self.brk = join_opt(self.brk.take(), o.brk);
        self.cnt = join_opt(self.cnt.take(), o.cnt);
        self.ret_at = join_lab_opt(self.ret_at, o.ret_at);
        self.brk_at = join_lab_opt(self.brk_at, o.brk_at);
        self.cnt_at = join_lab_opt(self.cnt_at, o.cnt_at);
    }
}

/// One path of execution: its state (`None` once nothing reaches the point) and its exits.
pub(crate) struct Path {
    pub st: Option<St>,
    exits: Exits,
}

impl Path {
    pub fn pc(&self) -> Lab {
        self.st.as_ref().map(|s| s.pc()).unwrap_or_default()
    }
    fn live(&self) -> bool {
        self.st.is_some()
    }
}

/// The function being analyzed: what its `return`s give, whether it may end the program, its
/// call-resolution set, and its call context (for the loop cache).
struct Frame {
    ret: V,
    exit: Lab,
    locals: Rc<BTreeSet<String>>,
    name: Rc<str>,
    ctx: usize,
}

/// State outside the program's values that builtins write and read back.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Store {
    /// Files (`write_file`, `append_file` → `read_file`, `open`).
    Files,
    /// The process-global Keychain/Secure-Enclave binding result (`cap_acquire_nonexportable` →
    /// `keychain_se_last_bind`).
    KeychainBind,
}

pub(crate) struct Interp<'a> {
    fns: BTreeMap<String, FnDef<'a>>,
    fn_arity: BTreeMap<String, usize>,
    /// Method name → (type, definition).
    methods: BTreeMap<String, Vec<(Rc<str>, FnDef<'a>)>>,
    /// Struct name → field types.
    structs: BTreeMap<String, BTreeMap<String, String>>,
    /// Enum name → variants.
    enums: BTreeMap<String, Vec<EnumVariant>>,
    lams: Vec<LamInfo<'a>>,
    lam_ids: HashMap<usize, LamId>,
    /// Per (lambda, reach label): the join of every snapshot summarized past `CLOSURE_DEPTH`.
    summaries: BTreeMap<(LamId, Lab), Env>,
    summaries_changed: bool,
    /// What each store may hold (the join of what was written to it and the program counters it
    /// was written under).
    stores: BTreeMap<Store, Lab>,
    memo: HashMap<CallKey, V>,
    in_progress: HashSet<CallKey>,
    recursed: HashSet<CallKey>,
    /// Precise contexts analyzed per callee.
    contexts: HashMap<Callee, usize>,
    /// A summary context's joined arguments.
    summary_args: HashMap<CallKey, Vec<V>>,
    /// Call contexts by number (the loop cache is kept per context).
    ctx_ids: HashMap<CallKey, usize>,
    frames: Vec<Frame>,
    findings: BTreeSet<Finding>,
    steps: u64,
    exhausted: Option<&'static str>,
    /// Solved loops: (loop statement, call context) → (entry, exit, the frame's return value and
    /// exit label after it, the return program counters it took). A loop entered with a state ⊑
    /// a solved entry reuses that result (the analysis is monotone, so it covers the smaller
    /// state).
    loop_cache: HashMap<(usize, usize), Vec<LoopResult>>,
    loop_depth: usize,
    /// Calls being solved, innermost last, each marked when its analysis read the in-progress
    /// approximation of a call below it on this stack (its result is then provisional and is not
    /// kept: a later call analyzes it again).
    solving: Vec<Solving>,
    /// Provisional results: what each read of in-progress approximations (key, value then); one is
    /// reused while every approximation it read still has that value.
    memo_prov: HashMap<CallKey, (V, Vec<(CallKey, V)>)>,
    /// Callees seen recursing: later calls use their summary contexts (joined arguments), so a
    /// large recursive component settles over few keys.
    recursive_callees: HashSet<Callee>,
    /// Bumped whenever an approximation that other results may have read changes (a recursive
    /// fixpoint iterates, a provisional result is dropped); a cached loop result is reused only
    /// within its generation.
    generation: u64,
}

#[derive(Clone)]
struct LoopResult {
    generation: u64,
    entry: St,
    exit: Option<St>,
    ret: V,
    exit_lab: Lab,
    ret_at: Option<Lab>,
}

impl<'a> Interp<'a> {
    pub fn new(items: &'a [Item]) -> Self {
        value::reset();
        let mut it = Interp {
            fns: BTreeMap::new(),
            fn_arity: BTreeMap::new(),
            methods: BTreeMap::new(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            lams: Vec::new(),
            lam_ids: HashMap::new(),
            summaries: BTreeMap::new(),
            summaries_changed: false,
            stores: BTreeMap::new(),
            memo: HashMap::new(),
            in_progress: HashSet::new(),
            recursed: HashSet::new(),
            contexts: HashMap::new(),
            summary_args: HashMap::new(),
            ctx_ids: HashMap::new(),
            frames: Vec::new(),
            findings: BTreeSet::new(),
            steps: 0,
            exhausted: None,
            loop_cache: HashMap::new(),
            loop_depth: 0,
            solving: Vec::new(),
            memo_prov: HashMap::new(),
            recursive_callees: HashSet::new(),
            generation: 0,
        };
        it.register(items);
        it
    }

    fn register(&mut self, items: &'a [Item]) {
        for item in items {
            match item {
                // Inline modules are flattened with no prefix at runtime (`fn_rust_name`).
                Item::Module { items, .. } => self.register(items),
                Item::Fn {
                    name,
                    params,
                    body,
                    ret,
                    ..
                } => {
                    self.fn_arity.insert(name.clone(), params.len());
                    self.fns
                        .insert(name.clone(), fn_def(name, params, body, ret));
                }
                // Two definitions of one name (a module's and the program's): the runtime does not
                // know which a value was built with, so a field takes every type either declares.
                Item::Struct { name, fields, .. } => {
                    let decl = self.structs.entry(name.clone()).or_default();
                    for (f, ty) in fields {
                        let merged = match decl.get(f) {
                            Some(old) if old != ty => format!("{old} | {ty}"),
                            _ => ty.clone(),
                        };
                        decl.insert(f.clone(), merged);
                    }
                }
                Item::Enum { name, variants, .. } => {
                    self.enums
                        .entry(name.clone())
                        .or_default()
                        .extend(variants.iter().cloned());
                }
                Item::Impl {
                    type_name, methods, ..
                } => {
                    for m in methods {
                        if let Item::Fn {
                            name,
                            params,
                            body,
                            ret,
                            ..
                        } = m
                        {
                            self.methods.entry(name.clone()).or_default().push((
                                Rc::from(type_name.as_str()),
                                fn_def(name, params, body, ret),
                            ));
                        }
                    }
                }
                Item::Import { .. } | Item::Trait { .. } => {}
            }
        }
    }

    /// Analyze the program from `main` (or, with no `main`, from every function with its declared
    /// parameters) and return what it found.
    pub fn run(&mut self) -> Vec<Finding> {
        let entries: Vec<Rc<str>> = if self.fns.contains_key("main") {
            vec![Rc::from("main")]
        } else {
            self.fns.keys().map(|k| Rc::from(k.as_str())).collect()
        };
        for _round in 0..SUMMARY_ROUNDS {
            self.summaries_changed = false;
            self.memo.clear();
            self.in_progress.clear();
            self.recursed.clear();
            self.contexts.clear();
            self.summary_args.clear();
            self.findings.clear();
            self.loop_cache.clear();
            self.solving.clear();
            self.memo_prov.clear();
            self.recursive_callees.clear();
            for e in &entries {
                let arity = self.fn_arity.get(&**e).copied().unwrap_or(0);
                // An entry's arguments are anything a caller may pass, functions included.
                let mut unknown = V::top(Lab::PUB);
                unknown.fns.any = true;
                let args = vec![unknown; arity];
                self.call(Callee::User(e.clone()), args, Lab::PUB);
                if self.exhausted.is_some() {
                    break;
                }
            }
            self.drain_pending();
            if self.exhausted.is_some() || !self.summaries_changed {
                break;
            }
            if _round + 1 == SUMMARY_ROUNDS {
                self.exhausted = Some("the closure summaries did not settle");
            }
        }
        if std::env::var_os("ANUBIS_IFC2_TRACE").is_some() {
            eprintln!("ifc2 work={} steps={}", value::work(), self.steps);
        }
        let mut out: Vec<Finding> = self.findings.iter().cloned().collect();
        if let Some(why) = self.exhausted {
            out.push(Finding {
                code: "ANUBIS_IFC2_LIMIT",
                message: format!(
                    "the information-flow interpreter stopped before it finished ({why}); it cannot \
                     bound what the program discloses, so the program is refused"
                ),
            });
        }
        out
    }

    /// Whether the analysis has given up.
    pub(crate) fn exhausted(&self) -> bool {
        self.exhausted.is_some()
    }

    /// Give up (fail closed) for the reason `why`.
    pub(crate) fn give_up(&mut self, why: &'static str) {
        if self.exhausted.is_none() {
            self.exhausted = Some(why);
        }
    }

    fn tick(&mut self) -> bool {
        self.steps += 1;
        // The checker's stack and memory guard (`analysis_limit`): nesting too deep for this
        // thread's stack ends the analysis instead of overflowing it (the request is then refused).
        if self.exhausted.is_none() && super::super::analysis_limit::cut() {
            self.exhausted = Some("the checker's stack or memory guard was reached");
        }
        if self.steps > STEP_BUDGET && self.exhausted.is_none() {
            self.exhausted = Some("its step budget was spent");
        }
        if self.steps.is_multiple_of(64) {
            self.drain_pending();
            if value::work() > WORK_BUDGET && self.exhausted.is_none() {
                self.exhausted = Some("its work budget was spent");
            }
        }
        self.exhausted.is_none()
    }

    fn fname(&self) -> Rc<str> {
        self.frames
            .last()
            .map(|f| f.name.clone())
            .unwrap_or_else(|| Rc::from("?"))
    }

    pub(crate) fn report(&mut self, code: &'static str, message: String) {
        let message = format!("in `{}`: {message}", self.fname());
        self.findings.insert(Finding { code, message });
    }

    /// The value with bounded nesting; closures summarized past `CLOSURE_DEPTH`.
    pub(crate) fn widen(&mut self, v: &V) -> V {
        self.widen_to(v, super::value::DATA_DEPTH)
    }

    /// A summary context's arguments: folded below `SUMMARY_DEPTH` so they settle in a few rounds.
    fn widen_summary(&mut self, v: &V) -> V {
        self.widen_to(v, SUMMARY_DEPTH)
    }

    pub(crate) fn widen_coarse(&mut self, v: &V) -> V {
        self.widen_to(v, SUMMARY_DEPTH)
    }

    fn widen_to(&mut self, v: &V, depth: usize) -> V {
        let mut sums: Vec<(LamId, Lab, Env)> = Vec::new();
        let w = v.widen_from(depth, &mut |id, sig, env| sums.push((id, sig, env.clone())));
        for (id, sig, env) in sums {
            self.add_summary(id, sig, &env);
        }
        w
    }

    fn add_summary(&mut self, id: LamId, sig: Lab, env: &Env) {
        let cur = self.summaries.get(&(id, sig));
        if cur.is_some_and(|c| super::value::env_leq(env, c)) {
            return;
        }
        // The summary's own values are bounded like any other.
        let joined = match cur {
            Some(c) => join_env(c, env),
            None => env.clone(),
        };
        let mut sums: Vec<(LamId, Lab, Env)> = Vec::new();
        let joined: Env = joined
            .iter()
            .map(|(k, x)| {
                (
                    k.clone(),
                    x.widen_from(super::value::DATA_DEPTH, &mut |i, s, e| {
                        sums.push((i, s, e.clone()))
                    }),
                )
            })
            .collect();
        self.summaries.insert((id, sig), joined);
        self.summaries_changed = true;
        for (i, s, e) in sums {
            if (i, s) != (id, sig) {
                self.add_summary(i, s, &e);
            }
        }
    }

    /// Snapshots that joined a summary somewhere a value was joined or compared.
    fn drain_pending(&mut self) {
        for (id, sig, env) in value::take_pending() {
            self.add_summary(id, sig, &env);
        }
    }

    /// What a store may hold.
    pub(crate) fn store_read(&self, s: Store) -> Lab {
        self.stores.get(&s).copied().unwrap_or_default()
    }

    /// Something labelled `lab` written to a store: every read of it, anywhere, may return it
    /// (the analysis runs another round when a store grows).
    pub(crate) fn store_write(&mut self, s: Store, lab: Lab) {
        let cur = self.store_read(s);
        if cur.join(lab) != cur {
            self.stores.insert(s, cur.join(lab));
            self.summaries_changed = true;
        }
    }

    // --------------------------------------------------------------------------------------
    // Sinks

    /// An egress (`print`, `send`, …): its data must not be secret, and it must not run under a
    /// secret program counter. `what` names the sink.
    pub(crate) fn egress(&mut self, what: &str, data: Lab, p: &Path) {
        // Every egress is also an integrity sink (`is_sink` ⊇ `is_egress_sink`).
        self.sink(what, data);
        if data.secret() {
            self.report(
                "ANUBIS_SECRET_EXFILTRATION",
                format!("secret data reaches `{what}`"),
            );
        } else if p.pc().secret() {
            self.report(
                "ANUBIS_IMPLICIT_FLOW",
                format!(
                    "`{what}` runs under a condition a secret decides, so whether it runs \
                     discloses the secret (declassify the condition if that is intended)"
                ),
            );
        }
    }

    /// An integrity sink: its data must not be tainted.
    pub(crate) fn sink(&mut self, what: &str, data: Lab) {
        if data.tainted() {
            self.report(
                "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY",
                format!("untrusted data reaches `{what}`"),
            );
        }
    }

    /// `exit(code)` or `panic(msg)`: the program ends here. The code after it (in this function,
    /// and in every caller) runs only when this did not happen, like the code after a `return`.
    pub(crate) fn end_program(&mut self, p: &mut Path) {
        if let Some(st) = p.st.take() {
            let pc = st.pc().control();
            p.exits.ret_at = join_lab_opt(p.exits.ret_at, Some(pc));
            if let Some(f) = self.frames.last_mut() {
                f.exit = f.exit.join(pc);
            }
        }
    }

    // --------------------------------------------------------------------------------------
    // Calls

    /// A call from path `p`: analyzed at its program counter; a callee that may end the program
    /// lifts the caller's continuation.
    fn call_on(&mut self, callee: Callee, args: Vec<V>, p: &mut Path) -> V {
        let pc = p.pc();
        let mut v = self.call(callee, args, pc);
        let ex = v.exit.control();
        v.exit = Lab::PUB;
        if ex != Lab::PUB {
            if let Some(st) = &mut p.st {
                st.lift_ret = st.lift_ret.join(ex);
            }
            if let Some(f) = self.frames.last_mut() {
                f.exit = f.exit.join(ex);
            }
        }
        v
    }

    fn ctx_of(&mut self, key: &CallKey) -> usize {
        let n = self.ctx_ids.len();
        *self.ctx_ids.entry(key.clone()).or_insert(n)
    }

    /// Analyze a call: per (callee, arguments, program counter) up to `CONTEXTS_PER_CALLEE`
    /// distinct argument contexts; past that, one summary context per program counter and
    /// argument labels, whose arguments are the join of every argument it is called with,
    /// re-analyzed until both the arguments and the result settle.
    fn call(&mut self, callee: Callee, args: Vec<V>, pc: Lab) -> V {
        if !self.tick() {
            return V::bottom();
        }
        if std::env::var_os("ANUBIS_IFC2_TRACE").is_some() {
            let labs: Vec<String> = args
                .iter()
                .map(|a| format!("{}/{}", a.lab.0, a.deep().0))
                .collect();
            eprintln!(
                "ifc2 call {} pc={} args(lab/deep)={labs:?}",
                callee_name(&callee),
                pc.0
            );
        }
        let args: Vec<V> = args.iter().map(|a| self.widen(a)).collect();
        // The budget counts work: a call is charged by the size of what it is given.
        self.steps += args.iter().map(|a| a.size() as u64).sum::<u64>() / 8;
        let pc = pc.control();
        let precise = (callee.clone(), args.clone(), pc);
        if self.in_progress.contains(&precise) {
            self.recursed.insert(precise.clone());
            self.mark_provisional(&precise);
            self.recursive_callees.insert(callee.clone());
            return self.memo.get(&precise).cloned().unwrap_or_default();
        }
        if let Some(v) = self.memo.get(&precise) {
            return v.clone();
        }
        if let Some(v) = self.prov_hit(&precise) {
            return v;
        }
        // A callee already being solved further up the stack (with other arguments) recurses: its
        // later calls share summary contexts, so a recursion whose arguments grow at each level
        // does not open a precise context per level.
        let id = callee_identity(&callee);
        if self
            .solving
            .iter()
            .any(|(k, _, _)| callee_identity(&k.0) == id)
        {
            self.recursive_callees.insert(callee.clone());
        }
        let count = self.contexts.get(&callee).copied().unwrap_or(0);
        if count < CONTEXTS_PER_CALLEE && !self.recursive_callees.contains(&callee) {
            *self.contexts.entry(callee.clone()).or_insert(0) += 1;
            return self.solve(precise.clone(), &args);
        }
        // The summary context: calls at this program counter whose arguments carry these labels
        // (joined, so a recursion that moves a secret between parameters stays in one context).
        // Callbacks select their own context: a helper applied to many closures does not apply the
        // join of all of them (the functions a program names are finite, so contexts stay few).
        let sig: Vec<Lab> = vec![args.iter().fold(Lab::PUB, |l, a| l.join(a.deep()))];
        let fids: BTreeSet<String> = args.iter().flat_map(|a| fn_ids(&a.all_fns())).collect();
        let skey: CallKey = (
            Callee::Summary(
                Rc::new(callee.clone()),
                sig,
                Rc::new(fids.into_iter().collect()),
            ),
            Vec::new(),
            pc,
        );
        let cur = self.summary_args.get(&skey).cloned().unwrap_or_default();
        let grew = !(args.len() <= cur.len() && args.iter().zip(&cur).all(|(a, c)| a.leq(c)));
        if grew {
            let mut j = cur.clone();
            for (i, a) in args.iter().enumerate() {
                if i < j.len() {
                    j[i] = j[i].join(a);
                } else {
                    j.push(a.clone());
                }
            }
            let j: Vec<V> = j.iter().map(|a| self.widen_summary(a)).collect();
            self.summary_args.insert(skey.clone(), j);
            if self.in_progress.contains(&skey) {
                // A recursive call widened the summary's arguments: its analysis must run again.
                self.recursed.insert(skey.clone());
            } else {
                self.memo.remove(&skey);
                self.memo_prov.remove(&skey);
            }
        }
        if self.in_progress.contains(&skey) {
            self.recursed.insert(skey.clone());
            self.mark_provisional(&skey);
            return self.memo.get(&skey).cloned().unwrap_or_default();
        }
        if let Some(v) = self.memo.get(&skey) {
            return v.clone();
        }
        if let Some(v) = self.prov_hit(&skey) {
            return v;
        }
        self.solve_summary(skey)
    }

    /// Every call being solved above `key` on the stack read `key`'s in-progress approximation.
    fn mark_provisional(&mut self, key: &CallKey) {
        let val = self.memo.get(key).cloned().unwrap_or_default();
        if let Some(i) = self.solving.iter().rposition(|(k, _, _)| k == key) {
            for e in &mut self.solving[i + 1..] {
                e.1 = true;
                if !e.2.iter().any(|(k, _)| k == key) {
                    e.2.push((key.clone(), val.clone()));
                }
            }
        }
    }

    /// Finish solving `key`: a provisional result is kept with what it read, and reused only while
    /// all of that is unchanged.
    fn finish(&mut self, key: &CallKey) -> V {
        let v = self.memo.get(key).cloned().unwrap_or_default();
        if std::env::var_os("ANUBIS_IFC2_TRACE").is_some() {
            eprintln!(
                "ifc2 result {} -> lab={} deep={} shape={} exit={} top={}",
                callee_name(&key.0),
                v.lab.0,
                v.deep().0,
                v.shape_all().0,
                v.exit.0,
                v.top,
            );
        }
        let (provisional, deps) = self
            .solving
            .pop()
            .map(|(_, p, d)| (p, d))
            .unwrap_or((false, Vec::new()));
        self.in_progress.remove(key);
        self.recursed.remove(key);
        if provisional {
            self.memo.remove(key);
            // Whoever reuses this result read what it read.
            for e in &mut self.solving {
                for d in &deps {
                    if e.0 != d.0 && !e.2.iter().any(|(k, _)| k == &d.0) {
                        e.1 = true;
                        e.2.push(d.clone());
                    }
                }
            }
            self.memo_prov.insert(key.clone(), (v.clone(), deps));
        }
        v
    }

    /// A provisional result whose every read approximation is unchanged.
    fn prov_hit(&mut self, key: &CallKey) -> Option<V> {
        let (v, deps) = self.memo_prov.get(key)?.clone();
        let current = deps
            .iter()
            .all(|(k, val)| self.memo.get(k).map(|m| m == val).unwrap_or(false));
        if !current {
            return None;
        }
        for d in &deps {
            if self.in_progress.contains(&d.0) {
                self.mark_provisional(&d.0);
            }
        }
        Some(v)
    }

    /// A precise context to its fixpoint.
    fn solve(&mut self, key: CallKey, args: &[V]) -> V {
        self.in_progress.insert(key.clone());
        self.solving.push((key.clone(), false, Vec::new()));
        let ctx = self.ctx_of(&key);
        let mut rounds = 0;
        loop {
            let before = self.memo.get(&key).cloned().unwrap_or_default();
            self.recursed.remove(&key);
            let r = self.analyze_call(&key.0, args, key.2, ctx);
            let done = r.leq(&before);
            let joined = before.join(&r);
            // A recursive result that keeps growing is folded coarsely so it settles fast.
            let r = if rounds >= 1 {
                self.widen_summary(&joined)
            } else {
                self.widen(&joined)
            };
            self.memo.insert(key.clone(), r.clone());
            rounds += 1;
            if !self.recursed.contains(&key) || done || self.exhausted.is_some() {
                break;
            }
            if rounds > LOOP_LIMIT {
                self.exhausted = Some("a recursive call did not settle");
                break;
            }
            // The approximation other results may have read changed.
            self.generation += 1;
        }
        self.finish(&key)
    }

    /// A summary context to its fixpoint (arguments and result).
    fn solve_summary(&mut self, key: CallKey) -> V {
        self.in_progress.insert(key.clone());
        self.solving.push((key.clone(), false, Vec::new()));
        let ctx = self.ctx_of(&key);
        let mut rounds = 0;
        loop {
            let before = self.memo.get(&key).cloned().unwrap_or_default();
            let args_before = self.summary_args.get(&key).cloned().unwrap_or_default();
            self.recursed.remove(&key);
            let r = self.analyze_call(&key.0, &args_before, key.2, ctx);
            let done = r.leq(&before);
            let r = self.widen_summary(&before.join(&r));
            self.memo.insert(key.clone(), r.clone());
            let args_after = self.summary_args.get(&key).cloned().unwrap_or_default();
            rounds += 1;
            let args_same = args_after.len() == args_before.len()
                && args_after.iter().zip(&args_before).all(|(a, b)| a.leq(b));
            let settled = done && args_same;
            if std::env::var_os("ANUBIS_IFC2_TRACE").is_some() {
                let sz: Vec<usize> = args_after.iter().map(V::size).collect();
                eprintln!(
                    "ifc2 summary round {rounds} {} done={done} args_same={args_same} result_size={} args_sizes={sz:?}",
                    callee_name(&key.0),
                    r.size()
                );
            }
            if settled || self.exhausted.is_some() {
                break;
            }
            if rounds > LOOP_LIMIT {
                self.exhausted = Some("a recursive call did not settle");
                break;
            }
            self.generation += 1;
        }
        self.finish(&key)
    }

    fn analyze_call(&mut self, callee: &Callee, args: &[V], pc: Lab, ctx: usize) -> V {
        match callee {
            Callee::User(name) => {
                let Some(def) = self.fns.get(&**name) else {
                    return V::bottom();
                };
                let (params, body, ret, locals, name) = (
                    def.params,
                    def.body,
                    def.ret,
                    def.locals.clone(),
                    def.name.clone(),
                );
                self.analyze_body(name, params, body, ret, locals, args, pc, ctx)
            }
            Callee::Method(ty, m) => {
                let Some(defs) = self.methods.get(&**m) else {
                    return V::bottom();
                };
                let Some((_, def)) = defs.iter().find(|(t, _)| t == ty) else {
                    return V::bottom();
                };
                let (params, body, ret, locals) =
                    (def.params, def.body, def.ret, def.locals.clone());
                let name: Rc<str> = Rc::from(format!("{ty}::{m}"));
                self.analyze_body(name, params, body, ret, locals, args, pc, ctx)
            }
            Callee::Lam(id, clo) => self.analyze_lambda(*id, clo, args, pc, ctx),
            Callee::Summary(inner, _, _) => self.analyze_call(inner, args, pc, ctx),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn analyze_body(
        &mut self,
        name: Rc<str>,
        params: &'a [(String, String)],
        body: &'a [Stmt],
        ret: Option<&'a str>,
        locals: Rc<BTreeSet<String>>,
        args: &[V],
        pc: Lab,
        ctx: usize,
    ) -> V {
        let mut scope = BTreeMap::new();
        for (i, (p, ty)) in params.iter().enumerate() {
            // A missing argument is `Int(0)`; an extra one is dropped (the runtime pads).
            let v = args.get(i).cloned().unwrap_or_else(|| V::scalar(Lab::PUB));
            scope.insert(p.clone(), qualify(v, ty));
        }
        self.frames.push(Frame {
            ret: V::bottom(),
            exit: Lab::PUB,
            locals,
            name,
            ctx,
        });
        let mut p = Path {
            st: Some(St {
                scopes: vec![scope],
                cond: pc,
                lift_ret: Lab::PUB,
                lift_loop: Lab::PUB,
            }),
            exits: Exits::default(),
        };
        let tail = self.exec_block_value(body, &mut p);
        let frame = self.frames.pop().expect("frame");
        let mut out = frame.ret.join(&tail);
        if let Some(t) = ret {
            out = qualify(out, t);
        }
        out.exit = frame.exit;
        out
    }

    fn analyze_lambda(&mut self, id: LamId, clo: &Clo, args: &[V], pc: Lab, ctx: usize) -> V {
        let Some(info) = self.lams.get(id as usize) else {
            return V::top(Lab::SEC);
        };
        let (params, body, locals) = (info.params, info.body, info.owner_locals.clone());
        let mut scope: BTreeMap<String, V> = match clo {
            Clo::Env(env) => (**env).clone(),
            Clo::Summary(sigs) => {
                let mut e = Env::new();
                for s in sigs {
                    if let Some(x) = self.summaries.get(&(id, *s)) {
                        e = join_env(&e, x);
                    }
                }
                e
            }
        };
        for (i, p) in params.iter().enumerate() {
            scope.insert(
                p.clone(),
                args.get(i).cloned().unwrap_or_else(|| V::scalar(Lab::PUB)),
            );
        }
        self.frames.push(Frame {
            ret: V::bottom(),
            exit: Lab::PUB,
            locals,
            name: Rc::from("<closure>"),
            ctx,
        });
        let mut p = Path {
            st: Some(St {
                scopes: vec![scope],
                cond: pc,
                lift_ret: Lab::PUB,
                lift_loop: Lab::PUB,
            }),
            exits: Exits::default(),
        };
        let v = self.eval(body, &mut p);
        let frame = self.frames.pop().expect("frame");
        let mut out = frame.ret.join(&v);
        out.exit = frame.exit;
        out
    }

    /// How many parameters a function value takes at most (how far `apply` spreads a list).
    pub(crate) fn max_arity(&self, f: &V) -> usize {
        let fs = f.all_fns();
        let mut n = 1;
        for u in &fs.user {
            n = n.max(self.fn_arity.get(&**u).copied().unwrap_or(1));
        }
        for id in fs.clos.keys() {
            n = n.max(
                self.lams
                    .get(*id as usize)
                    .map(|l| l.params.len())
                    .unwrap_or(1),
            );
        }
        for (_, h) in &fs.composed {
            n = n.max(self.max_arity(h));
        }
        if let Some(c) = &fs.chain {
            n = n.max(self.max_arity(c));
        }
        if !fs.builtins.is_empty() || fs.any {
            n = n.max(8);
        }
        n
    }

    /// Apply a function value to already-evaluated arguments. Which function it is was decided
    /// by `f.lab`: each runs under it.
    pub(crate) fn apply(&mut self, f: &V, args: Vec<V>, p: &mut Path) -> V {
        if !p.live() {
            return V::bottom();
        }
        if std::env::var_os("ANUBIS_IFC2_TRACE").is_some() {
            let labs: Vec<String> = args
                .iter()
                .map(|a| format!("{}/{}", a.lab.0, a.deep().0))
                .collect();
            eprintln!(
                "ifc2 apply user={:?} builtins={:?} clos={} f.lab={} pc={} args={labs:?}",
                f.fns.user,
                f.fns.builtins,
                f.fns.clos.len(),
                f.lab.0,
                p.pc().0
            );
        }
        let flab = f.lab.control();
        let Some(outer) = p.st.clone() else {
            return V::bottom();
        };
        let mut out = V::bottom();
        // User functions, closures and compositions run on one path (a call changes nothing in
        // the caller but its continuation's lift); builtins each on their own (one may end it).
        let mut main = sub_path(&outer, flab);
        let mut paths = Vec::new();
        for u in f.fns.user.clone() {
            let arity = self.fn_arity.get(&*u).copied().unwrap_or(args.len());
            let mut a = args.clone();
            a.resize(arity, V::scalar(Lab::PUB));
            out = out.join(&self.call_on(Callee::User(u), a, &mut main));
        }
        for (id, clo) in f.fns.clos.clone() {
            out = out.join(&self.call_on(Callee::Lam(id, clo), args.clone(), &mut main));
        }
        for (g, h) in f.fns.composed.clone() {
            let inner = self.apply(&h, args.clone(), &mut main);
            out = out.join(&self.apply(&g, vec![inner], &mut main));
        }
        if let Some(c) = f.fns.chain.clone() {
            // f₁(f₂(…fₙ(args))) for any n ≥ 1 of the chain's functions.
            let mut r = self.apply(&c, args.clone(), &mut main);
            let mut rounds = 0;
            loop {
                rounds += 1;
                let r2 = self.apply(&c, vec![r.clone()], &mut main);
                let j = r.join(&r2);
                if j.leq(&r) || self.exhausted.is_some() {
                    break;
                }
                if rounds > LOOP_LIMIT {
                    self.exhausted = Some("a composition did not settle");
                    break;
                }
                r = if rounds > COARSE_AFTER {
                    self.widen_summary(&j)
                } else {
                    self.widen(&j)
                };
            }
            out = out.join(&r);
        }
        if f.fns.any {
            let pc = main.pc();
            out = out.join(&self.havoc("a function the analysis cannot name", &args, pc));
        }
        paths.push(main);
        for b in f.fns.builtins.clone() {
            let mut sub = sub_path(&outer, flab);
            let r = builtins::call(self, &b, args.clone(), &[], &mut sub)
                .unwrap_or_else(|| V::top(deep_all(&args)));
            out = out.join(&r);
            paths.push(sub);
        }
        merge(p, &outer, paths);
        out.raise(f.lab)
    }

    /// Applying a function the analysis cannot name: it may do anything with its arguments.
    fn havoc(&mut self, what: &str, args: &[V], pc: Lab) -> V {
        let data = deep_all(args);
        if data.secret() {
            self.report(
                "ANUBIS_SECRET_EXFILTRATION",
                format!("secret data is passed to {what}, which may disclose it"),
            );
        } else if pc.secret() {
            self.report(
                "ANUBIS_IMPLICIT_FLOW",
                format!("{what} is called under a condition a secret decides"),
            );
        }
        if data.tainted() {
            self.report(
                "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY",
                format!("untrusted data is passed to {what}, which may use it as a sink"),
            );
        }
        V::top(data)
    }

    // --------------------------------------------------------------------------------------
    // Statements

    fn exec_block(&mut self, stmts: &'a [Stmt], p: &mut Path) {
        for s in stmts {
            if !p.live() {
                return;
            }
            self.exec(s, p);
        }
    }

    /// A block's statements and its value (`split_tail_expr`): the last statement is the value
    /// when it is an expression statement (not `return` or `print*`) or an `if` with an `else`;
    /// otherwise the block yields `Int(0)`.
    fn exec_block_value(&mut self, stmts: &'a [Stmt], p: &mut Path) -> V {
        let Some((last, init)) = stmts.split_last() else {
            return V::scalar(Lab::PUB);
        };
        self.exec_block(init, p);
        if !p.live() {
            return V::bottom();
        }
        match last {
            Stmt::ExprStmt(e) if !is_stmt_only_call(e) => self.eval(e, p),
            Stmt::If {
                cond,
                then,
                else_: Some(els),
            } => self.if_value(cond, IfArm::Stmts(then), IfArm::Stmts(els), p),
            s => {
                self.exec(s, p);
                if p.live() {
                    V::scalar(Lab::PUB)
                } else {
                    V::bottom()
                }
            }
        }
    }

    fn exec(&mut self, s: &'a Stmt, p: &mut Path) {
        if !self.tick() {
            p.st = None;
            return;
        }
        match s {
            Stmt::Let { name, ty, init, .. } => {
                let v = self.eval(init, p);
                if let Some(st) = &mut p.st {
                    let mut v = v.raise(st.pc());
                    if let Some(t) = ty {
                        v = qualify(v, t);
                    }
                    st.bind(name, v);
                }
            }
            Stmt::LetPattern { pattern, init, .. } => {
                let v = self.eval(init, p);
                if let Some(st) = &mut p.st {
                    let v = v.raise(st.pc());
                    // Destructuring `let` does not test the pattern at runtime.
                    bind_pattern(pattern, &v, st, true);
                }
            }
            Stmt::Assign { target, value } => self.assign(target, value, p),
            Stmt::If { cond, then, else_ } => {
                let empty: &'a [Stmt] = &[];
                let els: &'a [Stmt] = else_.as_deref().unwrap_or(empty);
                // A constant condition: only the branch that runs.
                match const_truth(cond) {
                    Some(true) => return self.scoped(then, p),
                    Some(false) => return self.scoped(els, p),
                    None => {}
                }
                let c = self.eval(cond, p);
                let clab = c.truth().control();
                self.branch2(
                    clab,
                    p,
                    |it, q| it.scoped(then, q),
                    |it, q| it.scoped(els, q),
                );
            }
            Stmt::While { cond, body, .. } => match const_truth(cond) {
                Some(true) => self.exec_loop(s as *const Stmt as usize, LoopKind::Loop, body, p),
                Some(false) => {}
                None => self.exec_loop(s as *const Stmt as usize, LoopKind::While(cond), body, p),
            },
            Stmt::Loop { body, .. } => {
                self.exec_loop(s as *const Stmt as usize, LoopKind::Loop, body, p)
            }
            Stmt::For {
                var, source, body, ..
            } => {
                let (elem, clab) = match source {
                    ForSource::Range { start, end } => {
                        let a = self.eval(start, p);
                        let b = self.eval(end, p);
                        let l = a.deep().join(b.deep());
                        (V::scalar(l), l)
                    }
                    ForSource::Collection { expr } => {
                        let c = self.eval(expr, p);
                        (iter_elem(&c), iter_count_label(&c))
                    }
                };
                self.exec_loop(
                    s as *const Stmt as usize,
                    LoopKind::For(var, Box::new(elem), clab.control()),
                    body,
                    p,
                );
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => self.exec_loop(
                s as *const Stmt as usize,
                LoopKind::WhileLet(pattern, expr),
                body,
                p,
            ),
            Stmt::Break => {
                if let Some(st) = p.st.take() {
                    p.exits.brk_at = join_lab_opt(p.exits.brk_at, Some(st.pc()));
                    p.exits.brk = join_opt(p.exits.brk.take(), Some(st));
                }
            }
            Stmt::Continue => {
                if let Some(st) = p.st.take() {
                    p.exits.cnt_at = join_lab_opt(p.exits.cnt_at, Some(st.pc()));
                    p.exits.cnt = join_opt(p.exits.cnt.take(), Some(st));
                }
            }
            // Research and exploit blocks are emitted inline (the program is then not Safe, and
            // the interpreter does not run); hybrid and spec blocks do not run.
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                self.exec_block(body, p)
            }
            // Not runnable today; analyzed as alternatives so a future lowering is covered.
            Stmt::HybridBlock { gpu, cpu, prove } => {
                let Some(outer) = p.st.clone() else { return };
                let mut paths = vec![sub_path(&outer, Lab::PUB)];
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let mut q = sub_path(&outer, Lab::PUB);
                    self.scoped(b, &mut q);
                    paths.push(q);
                }
                merge(p, &outer, paths);
            }
            Stmt::SpecBlock { .. } => {}
            Stmt::ExprStmt(e) => {
                self.eval_stmt_expr(e, p);
            }
        }
    }

    /// Statements in a scope of their own.
    fn scoped(&mut self, stmts: &'a [Stmt], p: &mut Path) {
        if let Some(st) = &mut p.st {
            st.scopes.push(BTreeMap::new());
        }
        self.exec_block(stmts, p);
        pop_scope(p);
    }

    /// Two branches under the condition label `clab`, merged.
    fn branch2(
        &mut self,
        clab: Lab,
        p: &mut Path,
        a: impl FnOnce(&mut Self, &mut Path),
        b: impl FnOnce(&mut Self, &mut Path),
    ) {
        let Some(outer) = p.st.clone() else { return };
        let mut pa = sub_path(&outer, clab);
        a(self, &mut pa);
        let mut pb = sub_path(&outer, clab);
        b(self, &mut pb);
        merge(p, &outer, vec![pa, pb]);
    }

    fn assign(&mut self, target: &'a Expr, value: &'a Expr, p: &mut Path) {
        match target {
            Expr::Var(name) => {
                let v = self.eval(value, p);
                if let Some(st) = &mut p.st {
                    let v = v.raise(st.pc());
                    st.assign(name, v);
                }
            }
            _ => {
                // `root.set_at(&[segments], rhs)`: the index segments first, then the rhs.
                let Some((root, segs)) = self.place(target, p) else {
                    let _ = self.eval(value, p);
                    return;
                };
                let v = self.eval(value, p);
                let Some(st) = &mut p.st else { return };
                let pc = st.pc();
                let Some(cur) = st.lookup(&root).cloned() else {
                    return;
                };
                let updated = self.set_path(&cur, &segs, v.raise(pc), pc);
                if let Some(st) = &mut p.st {
                    st.assign(&root, updated);
                }
            }
        }
    }

    /// A place `a.b[i].c` as its root variable and evaluated segments.
    fn place(&mut self, e: &'a Expr, p: &mut Path) -> Option<(String, Vec<Seg>)> {
        match e {
            Expr::Var(n) => Some((n.clone(), Vec::new())),
            Expr::FieldAccess { base, field, .. } => {
                let (r, mut segs) = self.place(base, p)?;
                segs.push(Seg::Field(field.clone()));
                Some((r, segs))
            }
            Expr::Index { base, index } => {
                let (r, mut segs) = self.place(base, p)?;
                let i = self.eval(index, p);
                let key = literal_key(index);
                segs.push(Seg::Index(Box::new(i), key));
                Some((r, segs))
            }
            _ => None,
        }
    }

    /// `cur` with the place `segs` set to `v`, written under program counter `pc`: a write that
    /// may add a map key or a struct field changes the container's shape, so the shape is raised
    /// by `pc` (whether the write ran).
    fn set_path(&mut self, cur: &V, segs: &[Seg], v: V, pc: Lab) -> V {
        let Some((seg, rest)) = segs.split_first() else {
            return v;
        };
        let pc = pc.control();
        let mut out = cur.clone();
        match seg {
            Seg::Field(f) => {
                // Structs: set the field (a missing one is added); maps: the key.
                let tys: Vec<Rc<str>> = cur.structs.keys().cloned().collect();
                for t in tys {
                    let fields = cur.structs.get(&t).cloned().unwrap_or_default();
                    let maybe = cur.maybe_fields.get(&t).is_some_and(|m| m.contains(f));
                    if !fields.contains_key(f) || maybe {
                        out.lab = out.lab.join(pc);
                    }
                    if let Some(m) = out.maybe_fields.get_mut(&t) {
                        m.remove(f);
                        if m.is_empty() {
                            out.maybe_fields.remove(&t);
                        }
                    }
                    let old = fields
                        .get(f)
                        .cloned()
                        .unwrap_or_else(|| V::scalar(Lab::PUB));
                    let mut new = self.set_path(&old, rest, v.clone(), pc);
                    if let Some(ty) = self.structs.get(&*t).and_then(|m| m.get(f)).cloned() {
                        new = qualify(new, &ty);
                    }
                    let mut fields = (*fields).clone();
                    fields.insert(f.clone(), new);
                    out.structs.insert(t, Rc::new(fields));
                }
                if let Some(m) = &cur.map {
                    if !m.known.contains_key(f) || m.maybe.contains(f) {
                        out.lab = out.lab.join(pc);
                    }
                    let old = m.known.get(f).cloned().unwrap_or_else(|| m.other.clone());
                    let new = self.set_path(&old, rest, v.clone(), pc);
                    let mut m2 = (**m).clone();
                    m2.known.insert(f.clone(), new);
                    m2.maybe.remove(f);
                    out.map = Some(Rc::new(m2));
                }
                if cur.top {
                    out.lab = out.lab.join(pc);
                    out.store_in_top(Some(f), &v);
                    out.absorb_fns(&v);
                }
            }
            Seg::Index(i, key) => {
                let il = i.deep();
                if let Some(li) = &cur.list {
                    // `index_set` writes an existing position (out of range: nothing).
                    let mut li2 = (**li).clone();
                    let idx = key.as_ref().and_then(|k| k.index);
                    match (&mut li2.items, idx) {
                        (Some(items), Some(k)) if il == Lab::PUB => {
                            let n = items.len() as i64;
                            let pos = if k < 0 { k + n } else { k };
                            if pos >= 0 && pos < n {
                                let old = items[pos as usize].clone();
                                items[pos as usize] = self.set_path(&old, rest, v.clone(), pc);
                            }
                        }
                        (Some(items), _) => {
                            for slot in items.iter_mut() {
                                let old = slot.clone();
                                *slot = old.join(&self.set_path(&old, rest, v.raise(il), pc));
                            }
                        }
                        _ => {}
                    }
                    li2 = match li2.items {
                        Some(items) => ListV::of_items(items),
                        None => {
                            let old = li2.all.clone();
                            ListV {
                                items: None,
                                all: old.join(&self.set_path(&old, rest, v.raise(il), pc)),
                            }
                        }
                    };
                    out.list = Some(Rc::new(li2));
                }
                if let Some(m) = &cur.map {
                    let mut m2 = (**m).clone();
                    match key {
                        Some(k) if il == Lab::PUB => {
                            if !m.known.contains_key(&k.display) || m.maybe.contains(&k.display) {
                                out.lab = out.lab.join(pc);
                            }
                            let old = m
                                .known
                                .get(&k.display)
                                .cloned()
                                .unwrap_or_else(|| m.other.clone());
                            m2.known.insert(
                                k.display.clone(),
                                self.set_path(&old, rest, v.clone(), pc),
                            );
                            m2.maybe.remove(&k.display);
                        }
                        _ => {
                            // A computed key: a new key, or any known one.
                            out.lab = out.lab.join(pc);
                            let old = m2.other.clone();
                            m2.other = old.join(&self.set_path(&old, rest, v.raise(il), pc));
                            m2.klab = m2.klab.join(il);
                            for slot in m2.known.values_mut() {
                                let old = slot.clone();
                                *slot = old.join(&self.set_path(&old, rest, v.raise(il), pc));
                            }
                        }
                    }
                    out.map = Some(Rc::new(m2));
                }
                if cur.scalar {
                    // `s[i] = v` replaces a character.
                    out.lab = out.lab.join(il).join(v.deep());
                }
                if cur.top {
                    out.lab = out.lab.join(il).join(pc);
                    out.tlab = out.tlab.join(il);
                    out.store_in_top(None, &v);
                    out.absorb_fns(&v);
                }
            }
        }
        out
    }

    fn exec_loop(&mut self, id: usize, kind: LoopKind<'a>, body: &'a [Stmt], p: &mut Path) {
        let Some(entry) = p.st.clone() else { return };
        let ctx = self.frames.last().map(|f| f.ctx).unwrap_or(usize::MAX);
        let key = (id, ctx);
        if let Some(hit) = self
            .loop_cache
            .get(&key)
            .and_then(|v| {
                v.iter()
                    .rev()
                    .find(|r| r.generation == self.generation && entry.leq(&r.entry))
            })
            .cloned()
        {
            if let Some(f) = self.frames.last_mut() {
                f.ret = f.ret.join(&hit.ret);
                f.exit = f.exit.join(hit.exit_lab);
            }
            if let Some(r) = hit.ret_at {
                p.exits.ret_at = join_lab_opt(p.exits.ret_at, Some(r));
            }
            p.st = hit.exit;
            return;
        }
        // Re-entered with a larger state (in this call context): solve from the join with the
        // last one solved here, so a nest of loops does not re-solve each inner loop from scratch
        // on every outer pass.
        let entry = match self.loop_cache.get(&key).and_then(|v| v.last()) {
            Some(prev) if prev.entry.scopes.len() == entry.scopes.len() => {
                let j = prev.entry.join(&entry);
                j.widen(self)
            }
            _ => entry,
        };
        p.st = Some(entry.clone());
        let ret_at_before = p.exits.ret_at;
        self.loop_depth += 1;
        self.exec_loop_solve(kind, body, p);
        self.loop_depth -= 1;
        let (ret_after, exit_after) = self
            .frames
            .last()
            .map(|f| (f.ret.clone(), f.exit))
            .unwrap_or_default();
        let result = LoopResult {
            generation: self.generation,
            entry,
            exit: p.st.clone(),
            ret: ret_after,
            exit_lab: exit_after,
            ret_at: if p.exits.ret_at == ret_at_before {
                None
            } else {
                p.exits.ret_at
            },
        };
        let slot = self.loop_cache.entry(key).or_default();
        slot.push(result);
        if slot.len() > 4 {
            slot.remove(0);
        }
    }

    fn exec_loop_solve(&mut self, kind: LoopKind<'a>, body: &'a [Stmt], p: &mut Path) {
        let Some(entry) = p.st.clone() else { return };
        let depth = entry.scopes.len();
        let outer_loop = entry.lift_loop;
        let at_depth = |st: Option<St>| {
            st.map(|mut s| {
                s.scopes.truncate(depth);
                s
            })
        };
        let mut head = entry.clone();
        let mut exit: Option<St> = None;
        let mut rounds = 0;
        let mut ret_at: Option<Lab> = None;
        loop {
            rounds += 1;
            if rounds > LOOP_LIMIT {
                self.exhausted = Some("a loop did not settle");
                p.st = None;
                return;
            }
            // The loop header, from the head state.
            let mut hp = Path {
                st: Some(head.clone()),
                exits: Exits::default(),
            };
            let clab = match &kind {
                LoopKind::While(cond) => {
                    let c = self.eval(cond, &mut hp);
                    c.truth().control()
                }
                LoopKind::Loop => Lab::PUB,
                LoopKind::For(_, _, l) => *l,
                LoopKind::WhileLet(pat, e) => {
                    let v = self.eval(e, &mut hp);
                    let d = pattern_decision(pat, &v).control();
                    // The body runs with the pattern's names bound (a scope of its own).
                    let mut bound = hp.st.clone();
                    if let Some(st) = &mut bound {
                        st.scopes.push(BTreeMap::new());
                        bind_pattern(pat, &v, st, false);
                    }
                    // The no-match path leaves the loop without them.
                    exit = join_opt(exit, at_depth(hp.st.clone()));
                    hp.st = bound;
                    d
                }
            };
            // The condition-false (or exhausted) path leaves the loop.
            if matches!(kind, LoopKind::While(_) | LoopKind::For(..)) {
                exit = join_opt(exit, at_depth(hp.st.clone()));
            }
            // A `break` or `continue` inside the header (a `while let` scrutinee is lowered inside
            // the runtime loop) leaves the loop or goes around.
            let hx = std::mem::take(&mut hp.exits);
            ret_at = join_lab_opt(ret_at, hx.ret_at);
            exit = join_opt(exit, at_depth(hx.brk));
            let mut lift = hx.brk_at.unwrap_or_default().control();
            let mut around = at_depth(hx.cnt);
            if let Some(mut bst) = hp.st.clone() {
                bst.cond = bst.cond.join(clab);
                bst.scopes.push(BTreeMap::new());
                if let LoopKind::For(var, elem, _) = &kind {
                    bst.bind(var, elem.raise(bst.pc()));
                }
                let mut bp = Path {
                    st: Some(bst),
                    exits: Exits::default(),
                };
                self.exec_block(body, &mut bp);
                // `break` states leave the loop; `continue` states and the body's end go around.
                let ex = std::mem::take(&mut bp.exits);
                ret_at = join_lab_opt(ret_at, ex.ret_at);
                exit = join_opt(exit, at_depth(ex.brk));
                lift = lift.join(ex.brk_at.unwrap_or_default().control());
                around = join_opt(around, join_opt(at_depth(bp.st.take()), at_depth(ex.cnt)));
            }
            let mut next = match around {
                Some(a) => head.join(&a),
                None => head.clone(),
            };
            next.cond = entry.cond;
            // Whether a later iteration runs is decided by the headers so far and the breaks
            // taken; a `continue` does not decide it (the next iteration runs either way).
            next.lift_loop = head.lift_loop.join(lift).join(clab);
            if let Some(r) = ret_at {
                next.lift_ret = next.lift_ret.join(r.control());
            }
            if next.leq(&head) || self.exhausted.is_some() {
                break;
            }
            let widen_after = if self.loop_depth > 2 { 0 } else { WIDEN_AFTER };
            if rounds > COARSE_AFTER {
                next = next.widen_to(self, SUMMARY_DEPTH);
            } else if rounds > widen_after {
                next = next.widen(self);
            }
            head = next;
        }
        if let Some(r) = ret_at {
            p.exits.ret_at = join_lab_opt(p.exits.ret_at, Some(r));
        }
        p.st = exit.map(|mut st| {
            st.cond = entry.cond;
            st.lift_loop = outer_loop;
            if let Some(r) = ret_at {
                st.lift_ret = st.lift_ret.join(r.control());
            }
            st
        });
    }

    // --------------------------------------------------------------------------------------
    // Expressions

    /// An expression statement: the runtime special-cases `print*` and `push(var, v)` statements
    /// before any user function or local of that name (`emit_safe_run_stmt`).
    fn eval_stmt_expr(&mut self, e: &'a Expr, p: &mut Path) {
        if let Expr::Call { callee, args } = e {
            if matches!(callee.as_str(), "print" | "println" | "eprint" | "eprintln") {
                let vals = self.eval_args(args, p);
                if p.live() {
                    self.egress(callee, deep_all(&vals), p);
                }
                return;
            }
            if callee == "push" && matches!(args.first(), Some(Expr::Var(_))) {
                let _ = builtins::mutate(self, "push", args, p);
                return;
            }
        }
        let _ = self.eval(e, p);
    }

    pub(crate) fn eval_args(&mut self, args: &'a [Expr], p: &mut Path) -> Vec<V> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval(a, p));
        }
        out
    }

    pub(crate) fn eval(&mut self, e: &'a Expr, p: &mut Path) -> V {
        if !p.live() || !self.tick() {
            return V::bottom();
        }
        match e {
            Expr::Var(n) => self.var_value(n, p),
            Expr::Literal(_) | Expr::StrLiteral(_) => V::scalar(Lab::PUB),
            Expr::Call { callee, args } => self.eval_call(callee, args, p),
            Expr::CallExpr { callee, args } => {
                if let Expr::FieldAccess { base, field, .. } = &**callee {
                    if self.methods.contains_key(field.as_str()) {
                        return self.method_call(base, field, args, p);
                    }
                }
                let f = self.eval(callee, p);
                let a = self.eval_args(args, p);
                self.apply(&f, a, p)
            }
            Expr::Binary { op, lhs, rhs } => self.binary(op, lhs, rhs, p),
            Expr::Unary { op, expr } => {
                let v = self.eval(expr, p);
                if op == "!" {
                    V::scalar(v.truth())
                } else {
                    V::scalar(v.deep())
                }
            }
            // `as` a numeric type converts; any other target leaves the value unchanged.
            Expr::Cast { expr, ty } => {
                let v = self.eval(expr, p);
                if is_numeric_type(ty) {
                    V::scalar(v.deep())
                } else {
                    v
                }
            }
            Expr::ArrayLiteral { elements } => {
                let items = self.eval_args(elements, p);
                V::list(items, Lab::PUB)
            }
            Expr::Index { base, index } => {
                let b = self.eval(base, p);
                let i = self.eval(index, p);
                index_read(&b, &i, literal_key(index).as_ref())
            }
            Expr::FieldAccess { base, field, .. } => {
                let b = self.eval(base, p);
                field_read(&b, field)
            }
            Expr::StructLiteral { name, fields, .. } => {
                let mut map = BTreeMap::new();
                for (f, x) in fields {
                    let mut v = self.eval(x, p);
                    if let Some(ty) = self.structs.get(name).and_then(|m| m.get(f)).cloned() {
                        v = qualify(v, &ty);
                    }
                    // A field written twice keeps both at runtime; a read finds the first.
                    map.entry(f.clone()).or_insert(v);
                }
                // A declared field the literal omits is absent at runtime (a read gives Int(0)).
                let mut v = V::bottom();
                v.structs.insert(Rc::from(name.as_str()), Rc::new(map));
                v
            }
            Expr::EnumConstruct {
                enum_name,
                variant,
                fields,
                field_names,
                ..
            } => {
                let mut vals = self.eval_args(fields, p);
                if let Some(tys) = self.variant_types(enum_name, variant, field_names) {
                    for (v, ty) in vals.iter_mut().zip(tys) {
                        *v = qualify(v.clone(), &ty);
                    }
                }
                let mut v = V::bottom();
                v.enums.insert(
                    (Rc::from(enum_name.as_str()), Rc::from(variant.as_str())),
                    Rc::new(EnumV {
                        fields: vals,
                        names: field_names.iter().map(|n| Rc::from(n.as_str())).collect(),
                    }),
                );
                v
            }
            Expr::MapLiteral { entries, .. } => {
                // `anubis_map_lit`: keys by display text, in order, the last duplicate winning. A
                // computed key may equal any key written before it (replacing that value) or be
                // new; a literal key written after it replaces whatever is there.
                let mut known: BTreeMap<String, V> = BTreeMap::new();
                let mut other = V::bottom();
                let mut klab = Lab::PUB;
                for (k, x) in entries {
                    let kv = self.eval(k, p);
                    let v = self.eval(x, p);
                    match literal_key(k) {
                        Some(key) => {
                            known.insert(key.display, v);
                        }
                        None => {
                            klab = klab.join(kv.deep());
                            other = other.join(&v);
                            for slot in known.values_mut() {
                                *slot = slot.join(&v);
                            }
                        }
                    }
                }
                V {
                    map: Some(Rc::new(MapV {
                        known,
                        other,
                        klab,
                        maybe: BTreeSet::new(),
                    })),
                    ..V::default()
                }
            }
            Expr::If {
                cond, then, else_, ..
            } => self.if_value(cond, IfArm::Expr(then), IfArm::Expr(else_), p),
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let s = self.eval(scrutinee, p);
                self.match_value(&s, arms, p)
            }
            Expr::IfLet {
                pattern,
                scrutinee,
                then,
                else_,
                ..
            } => {
                let s = self.eval(scrutinee, p);
                let d = pattern_decision(pattern, &s).control();
                let Some(outer) = p.st.clone() else {
                    return V::bottom();
                };
                let mut pa = sub_path(&outer, d);
                if let Some(st) = &mut pa.st {
                    st.scopes.push(BTreeMap::new());
                    bind_pattern(pattern, &s, st, false);
                }
                let va = self.eval(then, &mut pa);
                pop_scope(&mut pa);
                let mut pb = sub_path(&outer, d);
                let vb = self.eval(else_, &mut pb);
                merge(p, &outer, vec![pa, pb]);
                V::select(&va, &vb, d)
            }
            Expr::Block { stmts, tail } => {
                if let Some(st) = &mut p.st {
                    st.scopes.push(BTreeMap::new());
                }
                let v = match tail {
                    Some(t) => {
                        self.exec_block(stmts, p);
                        self.eval(t, p)
                    }
                    None => self.exec_block_value(stmts, p),
                };
                pop_scope(p);
                v
            }
            Expr::Lambda { params, body } => self.make_closure(e, params, body, p),
            Expr::Try(inner) => {
                let v = self.eval(inner, p);
                self.try_value(v, p)
            }
            Expr::Declassify {
                inner,
                policy,
                reason,
            } => {
                let v = self.eval(inner, p);
                if super::super::declassify_wellformed(policy, reason) {
                    v.release(Lab::SEC.join(Lab::TNT))
                } else {
                    v
                }
            }
            Expr::TaintSource { .. } => V::scalar(Lab::TNT),
            Expr::Assume(x) | Expr::Assert(x) => {
                let _ = self.eval(x, p);
                V::scalar(Lab::PUB)
            }
            Expr::Tainted { inner, .. } => self.eval(inner, p).raise(Lab::TNT),
            Expr::Symbolic { .. }
            | Expr::UnifiedBuffer { .. }
            | Expr::RawPtr { .. }
            | Expr::Other(_) => V::scalar(Lab::PUB),
        }
    }

    fn variant_types(
        &self,
        enum_name: &str,
        variant: &str,
        field_names: &[String],
    ) -> Option<Vec<String>> {
        let vs = self.enums.get(enum_name)?;
        // Every definition of this variant (see `register`): a payload takes every type declared
        // for its position or name.
        let mut out: Option<Vec<String>> = None;
        for v in vs.iter().filter(|v| v.name == variant) {
            let tys: Vec<String> = match &v.kind {
                EnumVariantKind::Unit => Vec::new(),
                EnumVariantKind::Tuple(tys) => tys.clone(),
                EnumVariantKind::Struct(fs) => field_names
                    .iter()
                    .map(|n| {
                        fs.iter()
                            .find(|(f, _)| f == n)
                            .map(|(_, t)| t.clone())
                            .unwrap_or_default()
                    })
                    .collect(),
            };
            out = Some(match out {
                None => tys,
                Some(prev) => {
                    let n = prev.len().max(tys.len());
                    (0..n)
                        .map(|i| match (prev.get(i), tys.get(i)) {
                            (Some(a), Some(b)) if a != b => format!("{a} | {b}"),
                            (Some(a), _) => a.clone(),
                            (None, Some(b)) => b.clone(),
                            (None, None) => String::new(),
                        })
                        .collect()
                }
            });
        }
        out
    }

    /// A name in value position (`var_as_value`): a local, then a user function, then a builtin.
    fn var_value(&mut self, n: &str, p: &mut Path) -> V {
        if let Some(v) = p.st.as_ref().and_then(|s| s.lookup(n)) {
            return v.clone();
        }
        if self.fns.contains_key(n) {
            let mut fs = FnSet::default();
            fs.user.insert(Rc::from(n));
            return V::func(fs, Lab::PUB);
        }
        if crate::backends::run::is_builtin_name(n) {
            let mut fs = FnSet::default();
            fs.builtins.insert(Rc::from(n));
            return V::func(fs, Lab::PUB);
        }
        // Not bound: the emitted program does not compile.
        V::bottom()
    }

    fn current_locals(&self) -> Rc<BTreeSet<String>> {
        self.frames
            .last()
            .map(|f| f.locals.clone())
            .unwrap_or_default()
    }

    /// `callee(args)` resolved as the runtime does (`Expr::Call`): a user function, then a local
    /// (a closure call), then the builtins.
    fn eval_call(&mut self, callee: &str, args: &'a [Expr], p: &mut Path) -> V {
        if self.fns.contains_key(callee) {
            let a = self.eval_args(args, p);
            if !p.live() {
                return V::bottom();
            }
            let arity = self.fn_arity.get(callee).copied().unwrap_or(a.len());
            let mut a = a;
            a.resize(arity, V::scalar(Lab::PUB));
            return self.call_on(Callee::User(Rc::from(callee)), a, p);
        }
        if self.current_locals().contains(callee) {
            let f = p.st.as_ref().and_then(|s| s.lookup(callee)).cloned();
            let a = self.eval_args(args, p);
            return match f {
                Some(f) => self.apply(&f, a, p),
                None => V::bottom(),
            };
        }
        match callee {
            "print" | "println" | "eprint" | "eprintln" => {
                let a = self.eval_args(args, p);
                if p.live() {
                    self.egress(callee, deep_all(&a), p);
                }
                return V::scalar(Lab::PUB);
            }
            "return" => {
                let v = match args.first() {
                    Some(x) => self.eval(x, p),
                    None => V::scalar(Lab::PUB),
                };
                self.do_return(v, p);
                return V::bottom();
            }
            "break" => {
                if let Some(st) = p.st.take() {
                    p.exits.brk_at = join_lab_opt(p.exits.brk_at, Some(st.pc()));
                    p.exits.brk = join_opt(p.exits.brk.take(), Some(st));
                }
                return V::bottom();
            }
            "continue" => {
                if let Some(st) = p.st.take() {
                    p.exits.cnt_at = join_lab_opt(p.exits.cnt_at, Some(st.pc()));
                    p.exits.cnt = join_opt(p.exits.cnt.take(), Some(st));
                }
                return V::bottom();
            }
            "len" => {
                // Only the first argument is lowered.
                let v = match args.first() {
                    Some(x) => self.eval(x, p),
                    None => V::scalar(Lab::PUB),
                };
                return V::scalar(len_label(&v));
            }
            "push" | "pop" | "insert" | "remove" => {
                if matches!(args.first(), Some(Expr::Var(_))) {
                    return builtins::mutate(self, callee, args, p);
                }
            }
            _ => {}
        }
        let a = self.eval_args(args, p);
        if !p.live() {
            return V::bottom();
        }
        if let Some(v) = builtins::call(self, callee, a.clone(), args, p) {
            return v;
        }
        // A closure call on a Rust binding of that name.
        match p.st.as_ref().and_then(|s| s.lookup(callee)).cloned() {
            Some(f) => self.apply(&f, a, p),
            None => V::bottom(),
        }
    }

    fn do_return(&mut self, v: V, p: &mut Path) {
        if let Some(st) = p.st.take() {
            let pc = st.pc();
            p.exits.ret_at = join_lab_opt(p.exits.ret_at, Some(pc));
            if let Some(f) = self.frames.last_mut() {
                f.ret = f.ret.join(&v.raise(pc.control()));
            }
        }
    }

    /// `recv.m(args)` for a method some `impl` defines: the receiver once, first; then a match on
    /// its runtime type name, whose arm for each type with `m` evaluates the arguments that
    /// method takes (the others are dropped unevaluated, missing ones are `Int(0)`) and calls it,
    /// and whose fallback evaluates them all and calls the closure in field `m` (else `Int(0)`).
    /// Which arm runs is decided by the receiver's type, and only when it may be several.
    fn method_call(&mut self, base: &'a Expr, m: &str, args: &'a [Expr], p: &mut Path) -> V {
        let recv = self.eval(base, p);
        let Some(outer) = p.st.clone() else {
            return V::bottom();
        };
        let d = dispatch_label(&recv).control();
        let impls: Vec<(Rc<str>, usize)> = self
            .methods
            .get(m)
            .map(|v| v.iter().map(|(t, d)| (t.clone(), d.params.len())).collect())
            .unwrap_or_default();
        let mut out = V::bottom();
        let mut paths = Vec::new();
        for (ty, arity) in &impls {
            let mut narrowed = V::bottom();
            if let Some(f) = recv.structs.get(ty) {
                narrowed.structs.insert(ty.clone(), f.clone());
            }
            for ((et, tag), e) in &recv.enums {
                if et == ty {
                    narrowed.enums.insert((et.clone(), tag.clone()), e.clone());
                }
            }
            if recv.top {
                narrowed = recv.clone();
            }
            if narrowed.is_bottom() {
                continue;
            }
            narrowed.lab = recv.lab;
            let mut q = sub_path(&outer, d);
            let want = arity.saturating_sub(1);
            let mut a = vec![narrowed];
            a.extend(self.eval_args(&args[..want.min(args.len())], &mut q));
            a.resize(*arity, V::scalar(Lab::PUB));
            if q.live() {
                let r = self.call_on(Callee::Method(ty.clone(), Rc::from(m)), a, &mut q);
                out = out.join(&r);
            }
            paths.push(q);
        }
        let covered: BTreeSet<&Rc<str>> = impls.iter().map(|(t, _)| t).collect();
        let other_struct = recv.structs.keys().any(|t| !covered.contains(t));
        let other_enum = recv.enums.keys().any(|(t, _)| !covered.contains(t));
        let fallback = other_struct
            || other_enum
            || recv.scalar
            || recv.list.is_some()
            || recv.map.is_some()
            || !recv.fns.is_empty()
            || recv.top;
        if fallback {
            let mut q = sub_path(&outer, d);
            let a = self.eval_args(args, &mut q);
            let f = field_read(&recv, m);
            if q.live() && (!f.fns.is_empty() || f.top) {
                out = out.join(&self.apply(&f, a, &mut q));
            }
            out = out.join(&V::scalar(Lab::PUB));
            paths.push(q);
        }
        if paths.is_empty() {
            return V::bottom();
        }
        merge(p, &outer, paths);
        out.raise(d)
    }

    fn binary(&mut self, op: &str, lhs: &'a Expr, rhs: &'a Expr, p: &mut Path) -> V {
        // A constant left operand decides a short circuit: `false && x` and `true || x` never
        // evaluate `x`.
        match (op, const_truth(lhs)) {
            ("&&", Some(false)) | ("||", Some(true)) => return V::scalar(Lab::PUB),
            ("&&", Some(true)) | ("||", Some(false)) => {
                let r = self.eval(rhs, p);
                return V::scalar(r.truth());
            }
            _ => {}
        }
        let l = self.eval(lhs, p);
        if op == "&&" || op == "||" {
            // Short-circuit: the right operand runs only when the left's truth does not decide.
            let lt = l.truth();
            let clab = lt.control();
            let Some(outer) = p.st.clone() else {
                return V::bottom();
            };
            let mut pr = sub_path(&outer, clab);
            let r = self.eval(rhs, &mut pr);
            let pl = sub_path(&outer, clab);
            merge(p, &outer, vec![pl, pr]);
            return V::scalar(lt.join(r.truth()));
        }
        let r = self.eval(rhs, p);
        // An operand with no value (its evaluation cannot complete) gives no value.
        if l.is_bottom() || r.is_bottom() {
            return V::bottom();
        }
        // Comparing with an empty list literal or a unit variant (`xs == []`, `o == None`) reveals
        // only the other side's shape or tag (equality is structural and type-exact).
        if op == "==" || op == "!=" {
            if is_shape_literal(rhs) {
                return V::scalar(l.lab.join(r.deep()));
            }
            if is_shape_literal(lhs) {
                return V::scalar(r.lab.join(l.deep()));
            }
        }
        if op == "+" {
            return plus(&l, &r);
        }
        V::scalar(l.deep().join(r.deep()))
    }

    fn if_value(&mut self, cond: &'a Expr, a: IfArm<'a>, b: IfArm<'a>, p: &mut Path) -> V {
        match const_truth(cond) {
            Some(true) => return self.arm_value(a, p),
            Some(false) => return self.arm_value(b, p),
            None => {}
        }
        let c = self.eval(cond, p);
        let clab = c.truth().control();
        let Some(outer) = p.st.clone() else {
            return V::bottom();
        };
        let mut pa = sub_path(&outer, clab);
        let va = self.arm_value(a, &mut pa);
        let mut pb = sub_path(&outer, clab);
        let vb = self.arm_value(b, &mut pb);
        merge(p, &outer, vec![pa, pb]);
        V::select(&va, &vb, clab)
    }

    fn arm_value(&mut self, a: IfArm<'a>, p: &mut Path) -> V {
        match a {
            IfArm::Expr(e) => self.eval(e, p),
            IfArm::Stmts(s) => {
                if let Some(st) = &mut p.st {
                    st.scopes.push(BTreeMap::new());
                }
                let v = self.exec_block_value(s, p);
                pop_scope(p);
                v
            }
        }
    }

    fn match_value(&mut self, s: &V, arms: &'a [MatchArm], p: &mut Path) -> V {
        // Top-level or-patterns run as one sub-arm per alternative (`lower_match_expr`).
        let mut subs: Vec<(&'a Pattern, &'a MatchArm)> = Vec::new();
        for arm in arms {
            match &arm.pattern {
                Pattern::Or(alts) => {
                    for a in alts {
                        subs.push((a, arm));
                    }
                }
                pat => subs.push((pat, arm)),
            }
        }
        let Some(outer) = p.st.clone() else {
            return V::bottom();
        };
        // An arm runs when its pattern (and guard) matches and no arm before it did. An earlier
        // arm whose pattern cannot match a value this arm's matches never decides it.
        let mut prior: Vec<(&'a Pattern, Lab)> = Vec::new();
        let mut paths = Vec::new();
        let mut vals: Vec<V> = Vec::new();
        let mut dtot = Lab::PUB;
        // The state the next arm is tried in: the scrutinee's own, joined with the state after
        // each failed guard (a guard's writes stay when it fails).
        let mut fall: St = outer.clone();
        for (pat, arm) in &subs {
            if !pattern_may_match(pat, s) {
                continue;
            }
            let own = pattern_decision(pat, s);
            let mut d = own;
            for (q, ql) in &prior {
                if !patterns_disjoint(pat, q) {
                    d = d.join(*ql);
                }
            }
            let d = d.control();
            let mut q = sub_path(&fall, d);
            if let Some(st) = &mut q.st {
                st.cond = outer.cond.join(d);
                st.scopes.push(BTreeMap::new());
                bind_pattern(pat, s, st, false);
            }
            let mut glab = Lab::PUB;
            if let Some(g) = &arm.guard {
                let gv = self.eval(g, &mut q);
                glab = gv.truth().control();
                if let Some(st) = &q.st {
                    let mut failed = st.clone();
                    failed.scopes.pop();
                    failed.cond = outer.cond;
                    fall = fall.join(&failed);
                }
                if let Some(st) = &mut q.st {
                    st.cond = st.cond.join(glab);
                }
            }
            let v = self.eval(&arm.body, &mut q);
            pop_scope(&mut q);
            vals.push(v);
            dtot = dtot.join(d).join(glab);
            paths.push(q);
            prior.push((pat, own.join(glab)));
            if pat.is_irrefutable() && arm.guard.is_none() {
                break;
            }
        }
        if paths.is_empty() {
            // No arm can match: the runtime panics (a termination channel).
            p.st = None;
            return V::bottom();
        }
        merge(p, &outer, paths);
        let mut it = vals.into_iter();
        let first = it.next().unwrap_or_default();
        let mut val = first.clone();
        let mut many = false;
        for v in it {
            many = true;
            val = V::select(&val, &v, dtot);
        }
        if many {
            val
        } else {
            val.raise(dtot)
        }
    }

    fn try_value(&mut self, v: V, p: &mut Path) -> V {
        // `?`: Ok(x)/Some(x) gives x; Err/None returns that value from the enclosing function or
        // closure; any other enum passes through; a non-enum traps.
        let mut exits = V::bottom();
        let mut cont = V::bottom();
        for ((ty, tag), e) in &v.enums {
            match (&**ty, &**tag) {
                ("Result", "Err") | ("Option", "None") => {
                    let mut x = V::bottom();
                    x.enums.insert((ty.clone(), tag.clone()), e.clone());
                    exits = exits.join(&x);
                }
                ("Result", "Ok") | ("Option", "Some") => {
                    cont = cont.join(&e.fields.first().cloned().unwrap_or_default());
                }
                _ => {
                    let mut x = V::bottom();
                    x.enums.insert((ty.clone(), tag.clone()), e.clone());
                    cont = cont.join(&x);
                }
            }
        }
        if v.top {
            exits = exits.join(&v.part_of_top());
            cont = cont.join(&v.part_of_top());
        }
        let d = v.lab.control();
        if !exits.is_bottom() {
            let Some(outer) = p.st.clone() else {
                return V::bottom();
            };
            let mut pr = sub_path(&outer, d);
            self.do_return(exits.raise(v.lab), &mut pr);
            let pc = sub_path(&outer, d);
            merge(p, &outer, vec![pr, pc]);
        }
        cont.raise(v.lab)
    }

    fn make_closure(
        &mut self,
        e: &'a Expr,
        params: &'a [String],
        body: &'a Expr,
        p: &mut Path,
    ) -> V {
        let key = e as *const Expr as usize;
        let id = match self.lam_ids.get(&key) {
            Some(id) => *id,
            None => {
                let owner_locals = self.current_locals();
                let captures = captures_of(params, body, &owner_locals, &self.fns, &self.fn_arity);
                let id = self.lams.len() as LamId;
                self.lams.push(LamInfo {
                    params,
                    body,
                    owner_locals,
                    captures,
                });
                self.lam_ids.insert(key, id);
                id
            }
        };
        let Some(st) = &p.st else {
            return V::bottom();
        };
        let mut env = Env::new();
        for n in &self.lams[id as usize].captures {
            if let Some(v) = st.lookup(n) {
                env.insert(n.clone(), v.clone());
            }
        }
        let mut fs = FnSet::default();
        fs.clos.insert(id, Clo::Env(Rc::new(env)));
        let v = V::func(fs, Lab::PUB);
        self.widen(&v)
    }
}

/// The functions a function set names (lambdas by id, user functions and builtins by name).
fn fn_ids(f: &FnSet) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(f.user.iter().map(|u| format!("u:{u}")));
    out.extend(f.builtins.iter().map(|b| format!("b:{b}")));
    out.extend(f.clos.keys().map(|id| format!("l:{id}")));
    if !f.composed.is_empty() || f.chain.is_some() {
        out.push("compose".into());
    }
    if f.any {
        out.push("any".into());
    }
    out
}

/// Which function a callee is, whatever its captures (a closure's lambda) or context.
fn callee_identity(c: &Callee) -> (u8, Rc<str>, Rc<str>, LamId) {
    match c {
        Callee::User(n) => (0, n.clone(), Rc::from(""), 0),
        Callee::Method(t, m) => (1, t.clone(), m.clone(), 0),
        Callee::Lam(id, _) => (2, Rc::from(""), Rc::from(""), *id),
        Callee::Summary(inner, _, _) => callee_identity(inner),
    }
}

fn callee_name(c: &Callee) -> String {
    match c {
        Callee::User(n) => n.to_string(),
        Callee::Method(t, m) => format!("{t}::{m}"),
        Callee::Lam(id, _) => format!("<lambda {id}>"),
        Callee::Summary(inner, sig, fids) => {
            let s: Vec<u8> = sig.iter().map(|l| l.0).collect();
            format!("{} (summary {s:?} {fids:?})", callee_name(inner))
        }
    }
}

/// `l + r` (`anubis_add`): a list concatenates a list and appends anything else; a string
/// concatenates the other side's text; anything else is arithmetic on numbers (a collection's
/// number is its length). A `top` side may be any of these.
fn plus(l: &V, r: &V) -> V {
    let mut out = V::bottom();
    let l_list = l.list.is_some() || l.top;
    if l_list {
        // The left list (or a top's list part).
        let (lall, litems) = match &l.list {
            Some(ll) => (ll.all.clone(), ll.items.clone()),
            None => (V::bottom(), None),
        };
        let (lall, litems) = if l.top {
            (lall.join(&l.part_of_top()), None)
        } else {
            (lall, litems)
        };
        // list + list concatenates.
        if r.list.is_some() || r.top {
            let (rall, ritems) = match &r.list {
                Some(rl) => (rl.all.clone(), rl.items.clone()),
                None => (V::bottom(), None),
            };
            let (rall, ritems) = if r.top {
                (rall.join(&r.part_of_top()), None)
            } else {
                (rall, ritems)
            };
            let li = match (litems.clone(), ritems) {
                (Some(mut a), Some(b)) => {
                    a.extend(b);
                    ListV::of_items(a)
                }
                _ => ListV {
                    items: None,
                    all: lall.join(&rall),
                },
            };
            out = out.join(&V {
                lab: l.lab.join(r.lab),
                list: Some(Rc::new(li)),
                ..V::default()
            });
        }
        // list + x appends x.
        if r.scalar
            || r.map.is_some()
            || !r.structs.is_empty()
            || !r.enums.is_empty()
            || !r.fns.is_empty()
            || r.top
        {
            let mut rr = r.clone();
            rr.list = None;
            if rr.top {
                rr = rr.part_of_top();
            }
            let li = match litems {
                Some(mut i) => {
                    i.push(rr.clone());
                    ListV::of_items(i)
                }
                None => ListV {
                    items: None,
                    all: lall.join(&rr),
                },
            };
            out = out.join(&V {
                lab: l.lab.join(r.kind_label()),
                list: Some(Rc::new(li)),
                ..V::default()
            });
        }
    }
    let l_other = l.scalar
        || l.map.is_some()
        || !l.structs.is_empty()
        || !l.enums.is_empty()
        || !l.fns.is_empty()
        || l.top;
    if l_other {
        out = out.join(&V::scalar(l.deep().join(r.deep())));
    }
    if out.is_bottom() {
        out = V::scalar(l.deep().join(r.deep()));
    }
    // Which of these it is, is decided by the operands' kinds.
    if l.top {
        out = out.raise(l.lab.join(l.tlab));
    }
    out
}

/// What decides which arm of a method call's type match runs: the receiver's type name, when it
/// may have more than one (every tag of an enum type has that type's name; any other kind has
/// none).
fn dispatch_label(v: &V) -> Lab {
    if v.top {
        return v.lab;
    }
    let mut names: BTreeSet<&Rc<str>> = v.structs.keys().collect();
    for (t, _) in v.enums.keys() {
        names.insert(t);
    }
    let other = v.scalar || v.list.is_some() || v.map.is_some() || !v.fns.is_empty();
    if names.len() + usize::from(other) <= 1 {
        Lab::PUB
    } else {
        v.lab
    }
}

enum IfArm<'a> {
    Expr(&'a Expr),
    Stmts(&'a [Stmt]),
}

enum LoopKind<'a> {
    While(&'a Expr),
    Loop,
    For(&'a String, Box<V>, Lab),
    WhileLet(&'a Pattern, &'a Expr),
}

enum Seg {
    Field(String),
    /// The index's value and, when it is a literal, its key.
    Index(Box<V>, Option<LitKey>),
}

fn fn_def<'a>(
    name: &str,
    params: &'a [(String, String)],
    body: &'a [Stmt],
    ret: &'a Option<String>,
) -> FnDef<'a> {
    FnDef {
        name: Rc::from(name),
        params,
        body,
        ret: ret.as_deref(),
        locals: Rc::new(crate::backends::run::collect_local_names(params, body)),
    }
}

/// The names a lambda snapshots when it is created (`run.rs`, `Expr::Lambda`).
fn captures_of(
    params: &[String],
    body: &Expr,
    locals: &BTreeSet<String>,
    fns: &BTreeMap<String, FnDef<'_>>,
    arity: &BTreeMap<String, usize>,
) -> Vec<String> {
    let bound: BTreeSet<String> = params.iter().cloned().collect();
    let mut vars = BTreeSet::new();
    let mut callees = BTreeSet::new();
    crate::backends::run::collect_free_expr(body, &bound, &mut vars, &mut callees);
    let is_builtin = crate::backends::run::is_builtin_name;
    let mut out: BTreeSet<String> = vars
        .into_iter()
        .filter(|v| {
            locals.contains(v) || !(fns.contains_key(v) || arity.contains_key(v) || is_builtin(v))
        })
        .collect();
    for c in callees {
        if locals.contains(&c) || (!fns.contains_key(&c) && !is_builtin(&c)) {
            out.insert(c);
        }
    }
    out.into_iter().collect()
}

/// A value bound to a declared type. `secret<T>` makes the value secret and `tainted<T>` tainted;
/// inside `Option<…>`, `Result<…, …>`, `list<…>` and `map<…, …>` the qualifier applies to the
/// payloads or elements it wraps, when the value has that shape (declared types are not checked
/// at runtime, so a value of another shape takes every qualifier the type holds).
pub(crate) fn qualify(v: V, ty: &str) -> V {
    let t = ty.trim();
    let sec = super::super::ty::is_secret(t);
    let tnt = super::super::ty::is_tainted(t);
    if !sec && !tnt {
        return v;
    }
    let whole = |v: V| {
        let mut v = v;
        if sec {
            v = v.raise(Lab::SEC);
        }
        if tnt {
            v = v.raise(Lab::TNT);
        }
        v
    };
    let Some((head, args)) = split_generic(t) else {
        return whole(v);
    };
    let head_l = head.to_ascii_lowercase();
    match head_l.as_str() {
        "secret" | "tainted" => {
            let lab = if head_l == "secret" {
                Lab::SEC
            } else {
                Lab::TNT
            };
            let inner = args.first().copied().unwrap_or("");
            qualify(v.raise(lab), inner)
        }
        "option" | "result" => {
            let ty_name = if head_l == "option" {
                "Option"
            } else {
                "Result"
            };
            let only = !v.scalar
                && v.list.is_none()
                && v.map.is_none()
                && v.structs.is_empty()
                && v.fns.is_empty()
                && !v.top
                && v.enums.keys().all(|(t, _)| &**t == ty_name);
            if !only {
                return whole(v);
            }
            let mut out = v.clone();
            for ((t, tag), e) in &v.enums {
                let arg = match (ty_name, &**tag) {
                    ("Option", "Some") | ("Result", "Ok") => args.first().copied(),
                    ("Result", "Err") => args.get(1).copied(),
                    _ => None,
                };
                let Some(arg) = arg else { continue };
                let mut e2 = (**e).clone();
                if let Some(x) = e2.fields.first_mut() {
                    *x = qualify(x.clone(), arg);
                }
                out.enums.insert((t.clone(), tag.clone()), Rc::new(e2));
            }
            out
        }
        "list" | "vec" => {
            let only = v.list.is_some()
                && !v.scalar
                && v.map.is_none()
                && v.structs.is_empty()
                && v.enums.is_empty()
                && v.fns.is_empty()
                && !v.top;
            let (Some(li), Some(arg), true) = (&v.list, args.first(), only) else {
                return whole(v);
            };
            let li2 = match &li.items {
                Some(items) => {
                    ListV::of_items(items.iter().map(|x| qualify(x.clone(), arg)).collect())
                }
                None => ListV {
                    items: None,
                    all: qualify(li.all.clone(), arg),
                },
            };
            let mut out = v.clone();
            out.list = Some(Rc::new(li2));
            out
        }
        "map" | "hashmap" | "btreemap" => {
            let only = v.map.is_some()
                && !v.scalar
                && v.list.is_none()
                && v.structs.is_empty()
                && v.enums.is_empty()
                && v.fns.is_empty()
                && !v.top;
            let (Some(m), true, 2) = (&v.map, only, args.len()) else {
                return whole(v);
            };
            let key_lab = qualify(V::scalar(Lab::PUB), args[0]).lab;
            let mut m2 = (**m).clone();
            for x in m2.known.values_mut() {
                *x = qualify(x.clone(), args[1]);
            }
            m2.other = qualify(m2.other.clone(), args[1]);
            m2.klab = m2.klab.join(key_lab);
            let mut out = v.clone();
            out.lab = out.lab.join(key_lab);
            out.map = Some(Rc::new(m2));
            out
        }
        _ => whole(v),
    }
}

/// `Head<a, b>` as ("Head", ["a", "b"]), splitting only at top-level commas.
fn split_generic(t: &str) -> Option<(&str, Vec<&str>)> {
    let open = t.find('<')?;
    if !t.ends_with('>') {
        return None;
    }
    let head = t[..open].trim();
    let inner = &t[open + 1..t.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim());
    Some((head, args))
}

pub(crate) fn deep_all(vs: &[V]) -> Lab {
    vs.iter().fold(Lab::PUB, |l, v| l.join(v.deep()))
}

/// What `len(v)` reveals: a collection's length, a string's content (a scalar keeps it in `lab`,
/// a folded value possibly in `tlab`).
pub(crate) fn len_label(v: &V) -> Lab {
    let mut l = v.lab;
    if let Some(m) = &v.map {
        l = l.join(m.klab);
    }
    if v.top {
        l = l.join(v.tscal);
    }
    l
}

/// `for x in c`: the element and what decides the number of iterations.
fn iter_elem(c: &V) -> V {
    let mut e = V::bottom();
    if let Some(li) = &c.list {
        e = e.join(&li.all);
    }
    if c.scalar {
        e = e.join(&V::scalar(c.lab));
    }
    if let Some(m) = &c.map {
        // Iterating a map yields its keys.
        e = e.join(&V::scalar(m.klab.join(c.lab)));
    }
    if c.top {
        e = e.join(&c.part_of_top());
    }
    e.raise(c.lab)
}

fn iter_count_label(c: &V) -> Lab {
    len_label(c)
}

/// A literal index or key: its text as the runtime displays the value (the map key it names), and
/// the position it names in a list (`as_i64`). A string literal names a struct field.
#[derive(Clone, Debug)]
pub(crate) struct LitKey {
    pub display: String,
    pub index: Option<i64>,
    pub is_str: bool,
}

pub(crate) fn literal_key(e: &Expr) -> Option<LitKey> {
    match e {
        Expr::StrLiteral(s) => {
            let t = s.trim();
            let index = t
                .parse::<i64>()
                .ok()
                .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
                .or(Some(0));
            Some(LitKey {
                display: s.clone(),
                index,
                is_str: true,
            })
        }
        Expr::Literal(t) => {
            // `literal_to_anubis_value`: a bool, an i64, a u64 reinterpreted as i64, a float, or
            // (any other text) a string.
            let (display, index) = if t == "true" || t == "false" {
                (t.clone(), Some(i64::from(t == "true")))
            } else if let Ok(i) = t.parse::<i64>() {
                (i.to_string(), Some(i))
            } else if let Ok(u) = t.parse::<u64>() {
                let i = u as i64;
                (i.to_string(), Some(i))
            } else if let Ok(f) = t.parse::<f64>() {
                (float_str(f), Some(f as i64))
            } else {
                return Some(LitKey {
                    display: t.clone(),
                    index: t.trim().parse::<i64>().ok().or(Some(0)),
                    is_str: true,
                });
            };
            Some(LitKey {
                display,
                index,
                is_str: false,
            })
        }
        _ => None,
    }
}

/// `anubis_float_str`: how the runtime displays a float.
fn float_str(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

/// `b[i]` (`index_get`): a list's element (`i`'s position when both are known), a map's value, a
/// struct's field by name for a string index and by position otherwise, a string's character.
pub(crate) fn index_read(b: &V, i: &V, key: Option<&LitKey>) -> V {
    let il = i.deep();
    let mut out = V::bottom();
    if let Some(li) = &b.list {
        let exact = match (&li.items, key.and_then(|k| k.index)) {
            (Some(items), Some(k)) if il == Lab::PUB => {
                let n = items.len() as i64;
                let pos = if k < 0 { k + n } else { k };
                (pos >= 0 && pos < n).then(|| items[pos as usize].clone())
            }
            _ => None,
        };
        out = out.join(&exact.unwrap_or_else(|| li.all.clone()));
    }
    if let Some(m) = &b.map {
        let v = match key {
            Some(k) if il == Lab::PUB => m
                .known
                .get(&k.display)
                .cloned()
                .unwrap_or_else(|| m.other.clone()),
            _ => {
                let mut all = m.other.clone();
                for v in m.known.values() {
                    all = all.join(v);
                }
                all
            }
        };
        out = out.join(&v.raise(m.klab));
    }
    for f in b.structs.values() {
        match key {
            // `p["k"]` reads field k (missing: Int(0)); any other index is a position (or, out of
            // range, a field named by its text).
            Some(k) if k.is_str && il == Lab::PUB => {
                out = out.join(
                    &f.get(&k.display)
                        .cloned()
                        .unwrap_or_else(|| V::scalar(Lab::PUB)),
                );
            }
            _ => {
                for v in f.values() {
                    out = out.join(v);
                }
                out = out.join(&V::scalar(Lab::PUB));
            }
        }
    }
    if b.scalar {
        out = out.join(&V::scalar(b.lab));
    }
    if b.top {
        out = out.join(&b.part_of_top());
    }
    out.raise(b.lab.join(il))
}

/// `b.f` (`field_get`): a struct's field (Int(0) when missing), a struct-variant's field by name,
/// a map's key `f` (which value that is, is decided by the map's computed keys).
pub(crate) fn field_read(b: &V, f: &str) -> V {
    let mut out = V::bottom();
    for (t, fields) in &b.structs {
        out = out.join(
            &fields
                .get(f)
                .cloned()
                .unwrap_or_else(|| V::scalar(Lab::PUB)),
        );
        if b.maybe_fields.get(t).is_some_and(|m| m.contains(f)) {
            out = out.join(&V::scalar(Lab::PUB));
        }
    }
    if let Some(m) = &b.map {
        let v = m.known.get(f).cloned().unwrap_or_else(|| m.other.clone());
        out = out.join(&v.raise(m.klab));
        if !m.known.contains_key(f) || m.maybe.contains(f) {
            out = out.join(&V::scalar(m.klab));
        }
    }
    // A struct-variant enum's payload by name; any other variant reads Int(0).
    for e in b.enums.values() {
        match e.names.iter().position(|n| &**n == f) {
            Some(i) => out = out.join(&e.fields.get(i).cloned().unwrap_or_default()),
            None => out = out.join(&V::scalar(Lab::PUB)),
        }
    }
    if b.scalar || b.list.is_some() || !b.fns.is_empty() {
        out = out.join(&V::scalar(Lab::PUB));
    }
    if b.top {
        out = out.join(&b.field_of_top(f));
    }
    out.raise(b.lab)
}

/// Whether some pattern of `pat` has a literal to compare content with.
fn pattern_reads_content(pat: &Pattern) -> bool {
    match pat {
        Pattern::Wildcard | Pattern::Binding(_) => false,
        Pattern::Literal(_) | Pattern::StrLiteral(_) => true,
        Pattern::Or(ps) | Pattern::List(ps) => ps.iter().any(pattern_reads_content),
        Pattern::Struct { fields, .. } => fields.iter().any(|(_, q)| pattern_reads_content(q)),
        Pattern::EnumVariant {
            bindings,
            named_bindings,
            ..
        } => {
            bindings.iter().any(pattern_reads_content)
                || named_bindings.iter().any(|(_, q)| pattern_reads_content(q))
        }
    }
}

/// The label that decides whether `pat` matches `v`.
fn pattern_decision(pat: &Pattern, v: &V) -> Lab {
    if v.top {
        // A folded value's kinds and tags are its shape; a literal compares its content.
        return if pattern_reads_content(pat) {
            v.deep()
        } else {
            v.lab
        };
    }
    match pat {
        Pattern::Wildcard | Pattern::Binding(_) => Lab::PUB,
        Pattern::Literal(_) | Pattern::StrLiteral(_) => v.deep(),
        Pattern::Or(ps) => ps
            .iter()
            .fold(Lab::PUB, |l, q| l.join(pattern_decision(q, v))),
        Pattern::List(ps) => {
            let mut l = v.lab;
            if let Some(li) = &v.list {
                for (i, q) in ps.iter().enumerate() {
                    let e = li
                        .items
                        .as_ref()
                        .and_then(|it| it.get(i).cloned())
                        .unwrap_or_else(|| li.all.clone());
                    l = l.join(pattern_decision(q, &e));
                }
            }
            l
        }
        Pattern::Struct { fields, .. } => {
            let mut l = v.lab;
            for (f, q) in fields {
                l = l.join(pattern_decision(q, &field_read(v, f)));
            }
            l
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            named_bindings,
        } => {
            // The tag first; the sub-patterns are tested only on this variant's payload.
            let mut l = v.lab;
            for ((t, g), e) in &v.enums {
                if &**t != enum_name.as_str() || &**g != variant.as_str() {
                    continue;
                }
                for (i, q) in bindings.iter().enumerate() {
                    if let Some(x) = e.fields.get(i) {
                        l = l.join(pattern_decision(q, x));
                    }
                }
                for (n, q) in named_bindings {
                    if let Some(i) = e.names.iter().position(|m| &**m == n.as_str()) {
                        if let Some(x) = e.fields.get(i) {
                            l = l.join(pattern_decision(q, x));
                        }
                    }
                }
            }
            l
        }
    }
}

/// Whether no value matches both patterns.
fn patterns_disjoint(p: &Pattern, q: &Pattern) -> bool {
    use Pattern as P;
    match (p, q) {
        (P::Or(ps), _) => ps.iter().all(|x| patterns_disjoint(x, q)),
        (_, P::Or(qs)) => qs.iter().all(|y| patterns_disjoint(p, y)),
        (
            P::EnumVariant {
                enum_name: e1,
                variant: v1,
                bindings: b1,
                named_bindings: n1,
            },
            P::EnumVariant {
                enum_name: e2,
                variant: v2,
                bindings: b2,
                named_bindings: n2,
            },
        ) => {
            (e1, v1) != (e2, v2)
                || b1.iter().zip(b2).any(|(x, y)| patterns_disjoint(x, y))
                || n1
                    .iter()
                    .any(|(f, x)| n2.iter().any(|(g, y)| f == g && patterns_disjoint(x, y)))
        }
        (
            P::Struct {
                name: a,
                fields: fa,
            },
            P::Struct {
                name: b,
                fields: fb,
            },
        ) => {
            a != b
                || fa
                    .iter()
                    .any(|(f, x)| fb.iter().any(|(g, y)| f == g && patterns_disjoint(x, y)))
        }
        (P::List(a), P::List(b)) => {
            a.len() != b.len() || a.iter().zip(b).any(|(x, y)| patterns_disjoint(x, y))
        }
        (P::StrLiteral(a), P::StrLiteral(b)) => a != b,
        (P::Literal(a), P::Literal(b)) => {
            let int = |t: &str| match t {
                "true" | "false" => None,
                _ => t.parse::<i64>().ok(),
            };
            match (int(a), int(b)) {
                (Some(x), Some(y)) => x != y,
                _ => (a == "true" && b == "false") || (a == "false" && b == "true"),
            }
        }
        (P::EnumVariant { .. }, P::Struct { .. } | P::List(_))
        | (P::Struct { .. }, P::EnumVariant { .. } | P::List(_))
        | (P::List(_), P::EnumVariant { .. } | P::Struct { .. }) => true,
        _ => false,
    }
}

/// Whether `pat` can match some value `v` stands for (used only to skip arms no value reaches).
fn pattern_may_match(pat: &Pattern, v: &V) -> bool {
    if v.top {
        return true;
    }
    match pat {
        Pattern::EnumVariant {
            enum_name, variant, ..
        } => v
            .enums
            .keys()
            .any(|(t, g)| &**t == enum_name.as_str() && &**g == variant.as_str()),
        Pattern::Struct { name, .. } => v.structs.contains_key(name.as_str()),
        Pattern::Or(ps) => ps.iter().any(|q| pattern_may_match(q, v)),
        _ => true,
    }
}

/// Bind the names of `pat` to the parts of `v` they take. A destructuring `let` (`unchecked`)
/// does not test the pattern at runtime: an enum pattern takes position `i` (or the field of that
/// name) of whatever enum value is there.
fn bind_pattern(pat: &Pattern, v: &V, st: &mut St, unchecked: bool) {
    match pat {
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::StrLiteral(_) => {}
        Pattern::Binding(n) => st.bind(n, v.clone()),
        Pattern::Or(ps) => {
            for q in ps {
                bind_pattern(q, v, st, unchecked);
            }
        }
        Pattern::List(ps) => {
            for (i, q) in ps.iter().enumerate() {
                // `list_elem`: only a list yields elements; anything else gives Int(0).
                let mut e = match &v.list {
                    Some(li) => li
                        .items
                        .as_ref()
                        .and_then(|it| it.get(i).cloned())
                        .unwrap_or_else(|| li.all.join(&V::scalar(Lab::PUB))),
                    None => V::bottom(),
                };
                if v.top {
                    e = e.join(&v.part_of_top());
                }
                if e.is_bottom() || v.list.is_none() || unchecked {
                    e = e.join(&V::scalar(Lab::PUB));
                }
                bind_pattern(q, &e.raise(v.lab), st, unchecked);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (f, q) in fields {
                bind_pattern(q, &field_read(v, f), st, unchecked);
            }
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            named_bindings,
        } => {
            let takes =
                |t: &str, g: &str| unchecked || (t == enum_name.as_str() && g == variant.as_str());
            let positional = |i: usize| -> V {
                let mut out = V::bottom();
                let mut all_have = true;
                for ((t, g), e) in &v.enums {
                    if !takes(t, g) {
                        continue;
                    }
                    match e.fields.get(i) {
                        Some(x) => out = out.join(x),
                        None => all_have = false,
                    }
                }
                if v.top {
                    out = out.join(&v.part_of_top());
                }
                if !all_have || out.is_bottom() || unchecked {
                    out = out.join(&V::scalar(Lab::PUB));
                }
                out.raise(v.lab)
            };
            let named = |n: &str| -> V {
                let mut out = V::bottom();
                for ((t, g), e) in &v.enums {
                    if !takes(t, g) {
                        continue;
                    }
                    match e
                        .names
                        .iter()
                        .position(|m| &**m == n)
                        .and_then(|i| e.fields.get(i))
                    {
                        Some(x) => out = out.join(x),
                        None => out = out.join(&V::scalar(Lab::PUB)),
                    }
                }
                if v.top {
                    out = out.join(&v.field_of_top(n));
                }
                if out.is_bottom() || unchecked {
                    out = out.join(&V::scalar(Lab::PUB));
                }
                out.raise(v.lab)
            };
            for (i, q) in bindings.iter().enumerate() {
                bind_pattern(q, &positional(i), st, unchecked);
            }
            for (n, q) in named_bindings {
                bind_pattern(q, &named(n), st, unchecked);
            }
        }
    }
}

fn is_stmt_only_call(e: &Expr) -> bool {
    matches!(e, Expr::Call { callee, .. }
        if matches!(callee.as_str(), "return" | "print" | "println" | "eprint" | "eprintln"))
}

fn sub_path(outer: &St, clab: Lab) -> Path {
    let mut st = outer.clone();
    st.cond = st.cond.join(clab);
    Path {
        st: Some(st),
        exits: Exits::default(),
    }
}

/// Merge branch paths back into `p`: the continuation is the join of the branches that continue,
/// under the outer condition, lifted by every exit a branch took.
fn merge(p: &mut Path, outer: &St, branches: Vec<Path>) {
    let mut cont: Option<St> = None;
    let mut lift_ret = Lab::PUB;
    let mut lift_loop = Lab::PUB;
    for b in branches {
        lift_ret = lift_ret.join(b.exits.ret_at.unwrap_or_default());
        lift_loop = lift_loop
            .join(b.exits.brk_at.unwrap_or_default())
            .join(b.exits.cnt_at.unwrap_or_default());
        p.exits.absorb(b.exits);
        cont = join_opt(cont, b.st);
    }
    p.st = cont.map(|mut st| {
        st.cond = outer.cond;
        st.lift_ret = st.lift_ret.join(lift_ret.control());
        st.lift_loop = st.lift_loop.join(lift_loop.control());
        st
    });
}

fn pop_scope(p: &mut Path) {
    if let Some(st) = &mut p.st {
        st.scopes.pop();
    }
}

/// Targets of `as` the runtime converts to (`Expr::Cast` in `run.rs`); any other is unchanged.
fn is_numeric_type(ty: &str) -> bool {
    let t = ty.to_ascii_lowercase();
    matches!(
        t.as_str(),
        "f32"
            | "f64"
            | "float"
            | "double"
            | "u8"
            | "i8"
            | "u16"
            | "i16"
            | "u32"
            | "i32"
            | "u64"
            | "i64"
            | "u128"
            | "i128"
            | "usize"
            | "isize"
            | "int"
            | "integer"
    )
}

/// The truth of a condition that is a constant (`true`, `false`, an integer literal, `!` of one):
/// `as_bool` of the literal's value.
fn const_truth(e: &Expr) -> Option<bool> {
    match e {
        Expr::Literal(t) if t == "true" => Some(true),
        Expr::Literal(t) if t == "false" => Some(false),
        Expr::Literal(t) => t.parse::<i64>().ok().map(|i| i != 0),
        Expr::Unary { op, expr } if op == "!" => const_truth(expr).map(|b| !b),
        _ => None,
    }
}

/// An empty list literal or a variant with no payload: equality with it depends only on the other
/// side's shape or tag.
fn is_shape_literal(e: &Expr) -> bool {
    matches!(e, Expr::ArrayLiteral { elements } if elements.is_empty())
        || matches!(e, Expr::EnumConstruct { fields, .. } if fields.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_keys_are_the_runtime_display_text() {
        let k = |t: &str| literal_key(&Expr::Literal(t.to_string())).unwrap();
        assert_eq!(k("007").display, "7");
        assert_eq!(k("1.50").display, "1.5");
        assert_eq!(k("1e0").display, "1.0");
        assert_eq!(k("true").index, Some(1));
        assert!(!k("true").is_str);
        assert_eq!(k("18446744073709551615").display, "-1");
        let s = literal_key(&Expr::StrLiteral("a".into())).unwrap();
        assert!(s.is_str && s.display == "a");
    }

    #[test]
    fn typed_qualify_keeps_a_result_tag_public() {
        let mut ok = V::bottom();
        ok.enums.insert(
            (Rc::from("Result"), Rc::from("Ok")),
            Rc::new(EnumV {
                fields: vec![V::scalar(Lab::PUB)],
                names: vec![],
            }),
        );
        let q = qualify(ok, "Result<secret<str>, str>");
        assert!(!q.lab.secret());
        assert!(q.deep().secret());
        assert!(qualify(V::scalar(Lab::PUB), "Result<secret<str>, str>")
            .lab
            .secret());
    }

    #[test]
    fn disjoint_patterns() {
        let ev = |v: &str| Pattern::EnumVariant {
            enum_name: "E".into(),
            variant: v.into(),
            bindings: vec![],
            named_bindings: vec![],
        };
        assert!(patterns_disjoint(&ev("A"), &ev("B")));
        assert!(!patterns_disjoint(&ev("A"), &ev("A")));
        assert!(!patterns_disjoint(&ev("A"), &Pattern::Wildcard));
        assert!(patterns_disjoint(
            &Pattern::List(vec![Pattern::Wildcard]),
            &Pattern::List(vec![Pattern::Wildcard, Pattern::Wildcard])
        ));
    }
}
