//! Middle: typed HIR, mode/effect checks, taint tracking, and Z3 obligations.

use crate::frontend::{Expr, Item, Mode, Pattern, Span, Stmt, AST};
use crate::BuildMode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};

pub mod proptest;
pub(crate) mod capability;
pub(crate) mod effects;
pub(crate) mod trifecta;
pub(crate) mod ty;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingInfo {
    pub name: String,
    pub ty: Option<String>,
    pub mode: String,
    pub tainted: bool,
    pub taint_source: Option<String>,
    pub declassified: bool,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub module: Option<String>,
    pub mode: String,
    pub params: Vec<BindingInfo>,
    pub symbols: Vec<BindingInfo>,
    pub effects: Vec<String>,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hir {
    pub imports: Vec<String>,
    pub modules: Vec<String>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirBlock {
    pub function: String,
    pub mode: String,
    pub statement_count: usize,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaintTrace {
    pub source: String,
    pub sink: Option<String>,
    pub steps: Vec<String>,
    pub declassified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SolverObligation {
    pub name: String,
    pub assumptions: Vec<String>,
    pub assertion: String,
    pub vars: Vec<String>,
    /// Per-obligation QF_S sort tag. Theory selection normally sniffs a `"` in the SMT body to route to
    /// QF_S, but a VAR-vs-VAR string equality (`(= anb_s anb_t)`) carries no literal, so the string
    /// obligation builders set this true to force QF_S + String var declarations. `#[serde(default)]`
    /// keeps older serialized data (which lacks the field) deserializable.
    #[serde(default)]
    pub strings: bool,
    /// Branch path-condition assumptions active when this obligation was built (a subset of
    /// `assumptions`). A branch guard (`if a > 0 { … }`) is pushed as a scoped assumption so a
    /// guard-provable call discharges, but it is NOT a contract premise: the VACUITY check
    /// (`assumptions_satisfiable`, which flips a contract PASS→FAIL when the premises are
    /// self-contradictory) must EXCLUDE these — otherwise a provably-DEAD branch (guard contradicts the
    /// precondition, `{x>0} ∧ {x<0}`) is legitimately unreachable yet spuriously fails vacuous. Discharge
    /// still uses the full `assumptions` (guards included). `#[serde(default)]` for old-data compat.
    #[serde(default)]
    pub guard_assumptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SolverCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub model: Option<String>,
    pub smt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    pub code: Option<String>,
    pub message: String,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct TypedIR {
    pub mode: BuildMode,
    pub taint_labels: Vec<String>,
    pub constraints: Vec<String>,
    pub has_research: bool,
    pub body: Vec<Stmt>,
    pub hir: Hir,
    pub mir: Vec<MirBlock>,
    pub symbols: Vec<BindingInfo>,
    pub taint_traces: Vec<TaintTrace>,
    pub solver_obligations: Vec<SolverObligation>,
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub symbolic_defs: Vec<String>, // e.g. "(= result (bvadd ...))" for faithful binding
    pub symbolic_widths: BTreeMap<String, u32>, // var name -> bit width for faithful BV
}

#[derive(Debug, Clone)]
struct ScopeBinding {
    info: BindingInfo,
    /// Arity of the value when it is a closure / first-class function bound here (a lambda literal or
    /// a named-function reference); `None` when unknown. Used to arity-check direct closure calls.
    closure_arity: Option<usize>,
    /// CONFIDENTIALITY label (Phase-2, the dual of `info.tainted` which is the INTEGRITY label): this
    /// binding holds PRIVATE data, seeded by `secret_source(..)` and propagated flow-sensitively
    /// (let/assign/branch-merge). Rides on ScopeBinding (NOT the serialized `BindingInfo`) so it is
    /// pure analysis scratch — no HIR/evidence/fixpoint surface. A secret reaching a network/shell
    /// egress without declassify is `ANUBIS_SECRET_EXFILTRATION`.
    secret: bool,
}

/// Arity of an initializer if it is a closure or first-class function reference, else `None`.
/// Conservative: only a lambda literal, a named-function reference, or an alias of a known-arity
/// closure yields an arity — anything else is unknown and left unchecked (no false positives).
fn closure_arity_of(
    init: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<usize> {
    match init {
        Expr::Lambda { params, .. } => Some(params.len()),
        Expr::Var(n) => ctx
            .fn_params
            .get(n)
            .map(|p| p.len())
            .or_else(|| scope.get(n).and_then(|b| b.closure_arity)),
        _ => None,
    }
}

impl SemanticContext {
    /// The single diagnostic router for the type-system phase. New static checks (bidirectional
    /// inference, captured generics, trait coherence, typed `?`) emit through this with
    /// `shadow_gated=true`: while a check is in shadow mode AND `self.shadow` is on, its diagnostics
    /// are diverted to `shadow_diags` (logged, never enforced). A check is promoted to enforcing by
    /// passing `shadow_gated=false` — a one-line flip made ONLY after `run_shadow_diff.sh` proves
    /// UNEXPECTED = 0 over the whole corpus. Pre-existing diagnostics keep calling
    /// `diagnostics.push(...)` directly and are unaffected.
    ///
    /// Wired in by the bidirectional inference-core slice: the arm-join conflict and the
    /// `Call`/`Index`/`FieldAccess` check-direction mismatches all route through here with
    /// `shadow_gated=true`.
    fn emit(&mut self, diag: SemanticDiagnostic, shadow_gated: bool) {
        if shadow_gated {
            // A shadow-gated (not-yet-promoted) check NEVER enters the enforcing `diagnostics`
            // Err-gate. Under `ANUBIS_SHADOW_TYPES=1` its would-be rejection is logged to
            // `shadow_diags` for the corpus diff; with shadow off it is dropped entirely. Either
            // way the verdict path is bit-identical whether shadow is on or off (exactly the
            // property the `shadow` field's doc promises), so a new check lands atomic-green and
            // stays silent until it is promoted to enforcing by passing `shadow_gated=false`.
            if self.shadow {
                self.shadow_diags.push(diag);
            }
        } else {
            self.diagnostics.push(diag);
        }
    }
}

#[derive(Debug, Default)]
struct SemanticContext {
    hir: Hir,
    mir: Vec<MirBlock>,
    symbols: Vec<BindingInfo>,
    taint_labels: Vec<String>,
    constraints: Vec<String>,
    taint_traces: Vec<TaintTrace>,
    solver_obligations: Vec<SolverObligation>,
    diagnostics: Vec<SemanticDiagnostic>,
    /// Shadow-mode switch (set from `ANUBIS_SHADOW_TYPES=1`). When on, diagnostics emitted through
    /// `emit(.., shadow_gated=true)` are diverted to `shadow_diags` instead of `diagnostics`, so a
    /// NEW static check (the type-system phase: inference, generics, traits, typed `?`) can be
    /// exercised over the whole corpus WITHOUT rejecting any program. The verdict path (the
    /// `diagnostics` Err-gate below) is bit-identical whether shadow is on or off — the safety net
    /// that lets a check land atomic-green before it is promoted to enforcing. See
    /// `scripts/run_shadow_diff.sh`. Off by default ⇒ zero behavioral change in normal builds.
    shadow: bool,
    /// Would-be rejections logged in shadow mode. NEVER feed the `diagnostics` Err-gate; they are
    /// surfaced on stderr (as `ANUBIS_SHADOW: ...`) only when `shadow` is on, for the corpus-diff
    /// tooling to classify EXPECTED vs UNEXPECTED. Empty and inert in normal builds.
    shadow_diags: Vec<SemanticDiagnostic>,
    has_research: bool,
    symbolic_defs: Vec<String>,
    symbolic_widths: BTreeMap<String, u32>,
    /// Variables that are genuinely modelable as bit-vectors for the solver (a `symbolic()` source
    /// or an integer-arithmetic let over already-modelable vars). Distinct from `symbolic_widths`,
    /// which records a width for EVERY let — including string/bool/list bindings — and so cannot be
    /// used to decide whether an assertion is soundly modelable in QF_BV.
    solver_int_vars: BTreeSet<String>,
    /// Variables modeled as IEEE-754 Float64 in the QF_FP solver lane (Phase 3): float params (and,
    /// later, float `let`s). A symbol is EITHER an i64 bit-vector OR a Float64, never both — a mixed
    /// int/float comparison stays fail-closed. Reset per function alongside `solver_int_vars`.
    solver_float_vars: BTreeSet<String>,
    /// Variables modeled as SMT `String` in the QF_S solver lane (Phase 3): `string` params. A symbol is
    /// never both a String and an i64 bit-vector / Float64 (the modelability gates are mutually exclusive),
    /// so a string obligation is all-`String` and never sort-clashes. Reset per function alongside the
    /// other solver var sets.
    solver_string_vars: BTreeSet<String>,
    /// The branch path conditions currently in scope (a stack, innermost last): each `if`/`else` pushes
    /// its (possibly negated) guard here in parallel with `assumptions`, and pops it when the branch ends.
    /// An obligation built inside a branch records this set in `SolverObligation.guard_assumptions` so the
    /// VACUITY check can exclude guards (a dead branch must not fail vacuous). Kept in sync with the
    /// guard facts pushed to `assumptions`, but NOT with the frame sweep's reassignment drops — a stale
    /// entry here is harmless (it just won't match a dropped `assumptions` fact). Reset per function.
    active_branch_guards: Vec<String>,
    /// Every variable REASSIGNED anywhere in the current function body (`collect_assigned_roots` over
    /// the whole body, incl. writes embedded in a `match`-arm / `if`-expression / block). Phase-3 QF_FP
    /// `let` chaining is admitted ONLY for a float `let` whose name is NOT in this set: a never-reassigned
    /// variable's defining-fact is valid under every control-flow path, so it cannot leak past a write the
    /// statement-level frame sweep (which only visits `Stmt::If/While/…` bodies, not embedded expression
    /// writes) would miss. Reset + repopulated per function.
    reassigned_roots: BTreeSet<String>,
    /// Names bound by MORE THAN ONE `let` in the current function (a shadow: `let p = …; …; let p = …`).
    /// A struct-literal-let field fact rides `assumptions` keyed by a MANGLED per-field symbol the
    /// scalar shadow-clear does not evict, so a shadowed struct binding could leave a stale field fact —
    /// registration of struct-let field facts is skipped for a shadowed base (fail-open). Reset per fn.
    shadowed_lets: BTreeSet<String>,
    /// The subset of the modeled string-predicate builtins {`contains`,`starts_with`,`ends_with`} that a
    /// TOP-LEVEL user fn (`all_fns`) OR a LOCAL binding (param / `let`) in the current function SHADOWS.
    /// The runtime resolves a call local > user-fn > builtin, so a shadowed name must NOT be z3-modeled as
    /// the string predicate (it would mis-certify a diverging user definition). Reset + repopulated per fn.
    shadowed_string_preds: BTreeSet<String>,
    /// Variables given an EXPLICIT `: T` annotation. Only these have their reassignments
    /// type-checked (the user opted into the type); an inferred binding is dynamic and reassignable
    /// to any type, so enforcing type-stability on it would be a false positive.
    annotated_vars: BTreeSet<String>,
    known_bindings: BTreeSet<String>,
    /// Enum name → variant names (for match exhaustiveness).
    enum_variants: BTreeMap<String, Vec<String>>,
    /// Function name → ordered parameter types (for call-site type checks).
    fn_params: BTreeMap<String, Vec<String>>,
    /// Function name → declared return type (`-> T`, empty if omitted). A call-result binding may be
    /// modeled as a solver integer ONLY when this is an integer type — otherwise a float-returning
    /// callee (`frac -> f64`) would seed a float into the integer domain via composition.
    fn_ret_types: BTreeMap<String, String>,
    /// Function name → (parameter names, `requires` clauses, `ensures` clauses). Registered in
    /// pass 1 so a caller can, at a call site, ASSERT the callee's precondition and ASSUME its
    /// postcondition — the composition that makes contracts chain.
    #[allow(clippy::type_complexity)]
    fn_contracts: BTreeMap<String, (Vec<String>, Vec<Expr>, Vec<Expr>)>,
    /// Function name → declared `uses(...)` capability tags (raw strings from the AST).
    /// Phase-5: at a call site, the caller's inferred effects inherit the callee's declared
    /// capabilities so `std.io` / `std.pwn` wrappers cannot launder `fs.write` / `shell` past Safe.
    fn_declared_effects: BTreeMap<String, Vec<String>>,
    /// Every user-defined function name (flat namespace; used for duplicate + unknown-call checks).
    all_fns: BTreeSet<String>,
    /// Interprocedural taint summary: functions whose RETURN value carries INTERNAL taint (from a
    /// `taint_source()`/`tainted<T>` local, or a return of another such function), computed by a
    /// monotone fixpoint pre-pass before per-function analysis. `expr_taint_source`'s `Call` arm
    /// consults it so `sink(get_secret())` is flagged even with no tainted argument. Monotone (only
    /// grows), so no control-flow-merge hazard — the return-value over-approximation is the safe
    /// direction for a security check.
    tainting_fns: BTreeSet<String>,
    /// Interprocedural param→sink summary (Phase-3 A1): for each function, the set of formal
    /// parameter indices that can flow to a sink (builtin `is_sink`, or a call argument position
    /// that another function's summary marks as sinking) without declassify. Monotone fixpoint.
    /// Call sites consult it: `log(tainted)` is `ANUBIS_INTERPROC_SINK` when `fn log(x){sink(x);}`.
    param_sinks: BTreeMap<String, BTreeSet<usize>>,
    /// Interprocedural param→EGRESS summary — the confidentiality dual of `param_sinks`, built by the
    /// same fixpoint with the `is_egress_sink` predicate (net/shell only, not local writes). Call
    /// sites consult it: a SECRET argument into a summarized-egress param is `ANUBIS_INTERPROC_EXFILTRATION`
    /// when `fn leak(x){ send(x); }`. Monotone fixpoint.
    param_egress: BTreeMap<String, BTreeSet<usize>>,
    /// Interprocedural param→sink / param→egress summaries for IMPL METHODS, keyed by BARE method name
    /// and UNIONED across every impl that declares that name (receiver static type is generally
    /// unrecoverable, so bare-name + union is the fail-closed choice). SEPARATE from `param_sinks`/
    /// `param_egress` because a method's parameter space keeps `self` at index 0 (a free fn's index 0 is
    /// its first arg) and bare names collide with free fns. Consulted ONLY in the `Expr::CallExpr`
    /// (method-call) arm, with the self-offset: summary index 0 ↔ the receiver, index p≥1 ↔ call arg
    /// p-1. Closes `m.deliver(secret)` / `r.go(tainted)` laundering through a method that egresses/sinks.
    method_param_sinks: BTreeMap<String, BTreeSet<usize>>,
    method_param_egress: BTreeMap<String, BTreeSet<usize>>,
    /// Interprocedural return-secret / return-taint summaries for IMPL METHODS, keyed by BARE method
    /// name and UNIONED across every impl declaring it (receiver static type unrecoverable → fail-closed,
    /// like `method_param_sinks`). A method is in the set iff its RETURN value carries an INTERNALLY-minted
    /// secret/taint (a `secret_source`/`input()` seed, a `secret<T>`/`tainted<T>` param, a let-chain, or a
    /// return of an already-marked FREE function) — the impl-method twin of `secret_fns`/`tainting_fns`.
    /// Consulted ONLY in the `Expr::CallExpr` (method-call) arm of `expr_secret_source_m`/
    /// `expr_taint_source_m`, so `let k = v.key(); send(h,p,k)` and `send(h,p,v.key())` (getter/accessor
    /// exfil) are caught. Method→method RETURN chaining (`fn alias(self){ return self.key() }`) is now
    /// also caught (#70): `compute_method_{secret,tainting}_fns` are COMBINED fixpoints that consult the
    /// GROWING method set (not only the frozen free-fn set), so a method returning another method's
    /// minted secret chains.
    method_secret_fns: BTreeSet<String>,
    method_tainting_fns: BTreeSet<String>,
    /// Interprocedural param→return summary (Phase-3 A2): for each function, the set of formal
    /// parameter indices that can flow to the return value without declassify. Monotone fixpoint.
    /// Combined at call sites with argument taint: `wrap` with `returns_taint_of_params={0}` makes
    /// `wrap(tainted)` a taint source (even through further let/return chains). Value-flow is
    /// label-agnostic, so the SAME summary serves both labels: `expr_secret_source` consults it too
    /// (a formal that flows to the return carries its argument's SECRECY exactly as it carries taint,
    /// and the declassify clear is identical), which is what rules out the discard-arg false positive
    /// on the confidentiality side (`send(ignore(secret))` with `fn ignore(x){0}` does not fire).
    param_return_taint: BTreeMap<String, BTreeSet<usize>>,
    /// Interprocedural SECRET summary (Phase-2 leg-1, the confidentiality dual of `tainting_fns`):
    /// functions whose RETURN value carries a `secret_source(..)` secret — minted directly, through
    /// let-chains, or returned from another such function. Monotone fixpoint (`compute_secret_fns`).
    /// `expr_secret_source`'s `Call` arm consults it so `send(get_key())` fires
    /// `ANUBIS_SECRET_EXFILTRATION` even though the secret is minted inside a helper; the trifecta's
    /// leg-1 consults it too (a helper returning a secret IS private-data access).
    secret_fns: BTreeSet<String>,
    /// Interprocedural leg-2 EXPOSURE summary (lethal trifecta): functions whose body transitively
    /// CONTAINS an untrusted-input steering channel (`is_leg2_source` — input/recv/env/…, NEVER
    /// read_file/open, which are leg 1). PRESENCE semantics, not return-flow: a helper that reads
    /// `input()` and discards it still exposes its caller to steering (the injection risk is the read
    /// happening at all, not the value's flow) — matching the intra-procedural leg-2 which is also
    /// presence. Using `is_leg2_source` (not `is_io_taint_source`/`tainting_fns`) is exactly what
    /// keeps a file-reading helper out of leg 2 — the read_file/leg-2 conflation the design avoids.
    /// Computed by `trifecta::compute_leg2_fns`; consumed by the trifecta scan (enforcing in the Safe
    /// (default) and verified lanes).
    leg2_fns: BTreeSet<String>,
    /// Phase-3 C5: verification lane (`--verified` / `@verified` / `#[verified]`). When set,
    /// capability effects require an explicit `uses(...)` declaration (fail-closed).
    verified: bool,
    /// Declared capability set for the function currently under analysis (from `uses(...)`).
    /// Empty means the function has no `uses` clause. In Safe mode, write/network/shell are
    /// forbidden unless the matching cap is present here (declared I/O is authorized).
    authorized_caps: BTreeSet<String>,
    /// Method name → parameter count (including `self`). `None` marks a name defined with more than
    /// one arity across impls, so its direct-call arity is ambiguous and left unchecked.
    method_arities: BTreeMap<String, Option<usize>>,
    /// Struct name → (field name → declared field type). Registered in pass 1 so the bidirectional
    /// inference core can synthesize `FieldAccess` results (`p.x` → the declared type of `x` on
    /// `Point`). Purely additive analysis state — never consulted by codegen (types are erased).
    struct_fields: BTreeMap<String, BTreeMap<String, String>>,
    /// The declared return type of the function whose body is currently being walked (`None` for a
    /// function with no `-> T`, and cleared inside a lambda body — a `?` there early-returns from the
    /// closure, not the enclosing function). Read by the typed-`?` check in `check_expr_semantics` to
    /// compare each `?` operand's container kind against the enclosing `Result`/`Option` return.
    current_fn_return: Option<String>,
    /// Function name → its captured generic type-parameter names (`fn same<T>(a: T, b: T)` → `["T"]`).
    /// Registered in pass 1. Drives `ANUBIS_GENERIC_CONFLICT`: at a call, a type parameter used in two
    /// argument positions is unified across them, and two incompatible concrete arguments clash.
    fn_generics: BTreeMap<String, Vec<String>>,
    /// Phase-1 trait bounds: function name → its bounded generics (`fn f<T: Ord + Eq>` →
    /// `[("T", ["Ord","Eq"])]`). Registered in pass 1 from `Item::Fn.generic_bounds`. Drives
    /// `ANUBIS_TRAIT_BOUND_UNSATISFIED`: at a call, a generic bound to a KNOWN user type whose trait is
    /// declared in-program and has no matching `impl` is rejected. Checker-only.
    fn_bounds: BTreeMap<String, Vec<(String, Vec<String>)>>,
    /// Every `(trait_name, type_name)` with an `impl Trait for Type` in this program — the trait-bound
    /// satisfaction registry, sourced from `ast.trait_env.impls`. `(("Ord","Blob"))` present ⇒ `Blob`
    /// satisfies `Ord`.
    trait_impls: BTreeSet<(String, String)>,
    /// Trait names DECLARED in this program (`ast.trait_env.traits`). A bound naming a trait NOT here is
    /// foreign/std — we cannot enumerate its impls, so it is accepted (fail-closed toward accept).
    declared_traits: BTreeSet<String>,
    /// User struct/enum name → its declared generic-parameter arity (`struct Pair<A, B>` → 2).
    /// Registered in pass 1. Drives `ANUBIS_GENERIC_ARITY`: an instantiation `Pair<u32>` in an
    /// annotation supplies the wrong number of type arguments. Built-in containers (`Result`/`Option`/
    /// `list`/`Map`/…) are absent, so their instantiations are never arity-checked (accept).
    type_generics: BTreeMap<String, usize>,
    /// Phase-2 slice 1: function name → TRANSITIVE effect row (canonical capability ids reached
    /// through the whole call graph, `open` when an unknown callee / closure / method call is hit).
    /// Computed by the pure pre-pass `effects::compute_fn_effect_rows`; consulted by the transitive
    /// declared-vs-inferred check. Sidecar analysis state only — never serialized, never fed to
    /// codegen, absent from the `selfhost_schema` projection by construction.
    fn_effect_rows: BTreeMap<String, effects::EffectRow>,
}

pub fn typecheck(ast: AST, mode: Mode) -> Result<TypedIR, String> {
    typecheck_ex(ast, mode, false)
}

/// Typecheck with an explicit verification-lane flag (Phase-3 C5). Prefer `typecheck` for the
/// default lane; pass `verified=true` for `--verified` / fail-closed effect declarations.
pub fn typecheck_ex(ast: AST, mode: Mode, verified: bool) -> Result<TypedIR, String> {
    let bmode = match mode {
        Mode::Safe => BuildMode::Safe,
        Mode::Research => BuildMode::Research,
        Mode::Exploit => BuildMode::Exploit,
    };
    let mut ctx = SemanticContext {
        verified,
        // Shadow-mode is opt-in and read once here. Default off ⇒ the enforcing `diagnostics`
        // path (and therefore every gate verdict) is unchanged until a check is promoted.
        shadow: std::env::var("ANUBIS_SHADOW_TYPES").as_deref() == Ok("1"),
        ..SemanticContext::default()
    };
    // A+ pass 1: register enums + function signatures so call/match checks see the whole program.
    register_program_surface(&ast.items, &mut ctx);
    // Trait coherence + missing-required-method, over the trait environment captured before
    // `resolve_traits` erased it. Analysis-only: emits (shadow-gated) diagnostics and reads no
    // desugaring output, so it cannot move the fixpoint.
    check_trait_env(&ast.trait_env, &mut ctx);
    // Phase-1 trait-bound registry (same read-only trait_env): which (trait, type) pairs have an impl,
    // and which traits this program declares — consumed by `bound_unsatisfied_scoped` at call sites.
    for imp in &ast.trait_env.impls {
        ctx.trait_impls
            .insert((imp.trait_name.clone(), imp.type_name.clone()));
    }
    for tname in ast.trait_env.traits.keys() {
        ctx.declared_traits.insert(tname.clone());
    }
    // Pass 1.5: interprocedural taint summaries (return-taint + param→sink), computed before
    // per-function analysis so every `Call` the analysis sees can consult them.
    compute_tainting_fns(&ast.items, &mut ctx);
    // Interprocedural param→sink and param→egress summaries for free fns AND impl methods, in ONE
    // COMBINED joint fixpoint each (#68): a free fn can sink/egress a param THROUGH a method
    // (`fn f(m,x){ m.snd(x) }`) and a method through another method (`fn ship(self,p){ self.deliver(p) }`),
    // so the free-fn and method maps are mutually recursive and must converge together (the former staged
    // passes, with the method map frozen after the free-fn map, structurally missed both directions).
    // Keyed by bare method name, `self` at index 0; consulted at the enforcing Call / CallExpr arms.
    {
        let mut free_fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
        collect_fn_params_bodies(&ast.items, &mut free_fns);
        let mut impl_methods: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
        collect_impl_method_params_bodies(&ast.items, &mut impl_methods);
        compute_sink_summaries_joint(
            &free_fns,
            &impl_methods,
            is_sink,
            &mut ctx.param_sinks,
            &mut ctx.method_param_sinks,
        );
        compute_sink_summaries_joint(
            &free_fns,
            &impl_methods,
            is_egress_sink,
            &mut ctx.param_egress,
            &mut ctx.method_param_egress,
        );
    }
    // Impl-method RETURN-taint summary (#67): a method whose return carries internally-minted taint —
    // `let t = r.tag(); sink(t)` / `sink(r.tag())`. Must run AFTER compute_tainting_fns (frozen free-fn
    // set); method→method chaining stays the #68 residual.
    compute_method_tainting_fns(&ast.items, &mut ctx);
    compute_param_return_taint(&ast.items, &mut ctx);
    // Pass 1.5 confidentiality duals: the interprocedural SECRET summary (so `send(get_key())` fires
    // even when the secret is minted in a helper) and the leg-2 EXPOSURE summary (so a helper wrapping
    // `input()` counts as untrusted-input exposure for the trifecta scan — enforcing in the Safe
    // (default) and verified lanes). Both are monotone,
    // emit nothing themselves, and — like the taint summaries — populated before per-function analysis.
    compute_secret_fns(&ast.items, &mut ctx);
    // Impl-method RETURN-secret summary (#67): a method whose return carries an internally-minted secret
    // — the getter/accessor exfil `let k = v.key(); send(h,p,k)` / `send(h,p,v.key())`. Must run AFTER
    // compute_secret_fns (frozen free-fn set); method→method chaining stays the #68 residual.
    compute_method_secret_fns(&ast.items, &mut ctx);
    ctx.leg2_fns = trifecta::compute_leg2_fns(&ast.items);
    // Pass 1.6 (Phase-2 slice 1): transitive effect rows — a pure monotone fixpoint over the call
    // graph (middle/effects.rs). Reads only the pass-1 tables (`all_fns`, declared `uses`); emits
    // nothing itself. Consumed by the transitive declared-vs-inferred effect check per function.
    ctx.fn_effect_rows =
        effects::compute_fn_effect_rows(&ast.items, &ctx.all_fns, &ctx.fn_declared_effects);
    collect_items(&ast.items, None, mode, &mut ctx);

    if ctx.constraints.is_empty() {
        ctx.constraints.push("(assert true)".into());
    }

    // Shadow-mode sink: surface would-be rejections on stderr (never the Err-gate below) so the
    // corpus-diff tooling can classify them. Gated on `shadow` ⇒ silent and inert by default.
    if ctx.shadow && !ctx.shadow_diags.is_empty() {
        for d in &ctx.shadow_diags {
            eprintln!(
                "ANUBIS_SHADOW: {} {}",
                d.code.as_deref().unwrap_or("-"),
                d.message
            );
        }
    }

    if !ctx.diagnostics.is_empty() {
        let messages = ctx
            .diagnostics
            .iter()
            .map(|diag| {
                if let Some(c) = &diag.code {
                    format!("{}: {}", c, diag.message)
                } else {
                    diag.message.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(messages);
    }

    let captured_body = first_fn_body(&ast.items).unwrap_or_default();
    Ok(TypedIR {
        mode: bmode,
        taint_labels: ctx.taint_labels,
        constraints: ctx.constraints,
        has_research: ctx.has_research,
        body: captured_body,
        hir: ctx.hir,
        mir: ctx.mir,
        symbols: ctx.symbols,
        taint_traces: ctx.taint_traces,
        solver_obligations: ctx.solver_obligations,
        diagnostics: vec![],
        symbolic_defs: ctx.symbolic_defs,
        symbolic_widths: ctx.symbolic_widths,
    })
}

/// Pass-1 registration: enums and function parameter types (A+ call/match surface).
fn register_program_surface(items: &[Item], ctx: &mut SemanticContext) {
    // Built-in Option/Result variants, so a `match` on them can be checked for exhaustiveness.
    // A user-declared enum of the same name (processed below) overrides these.
    ctx.enum_variants
        .entry("Option".into())
        .or_insert_with(|| vec!["Some".into(), "None".into()]);
    ctx.enum_variants
        .entry("Result".into())
        .or_insert_with(|| vec!["Ok".into(), "Err".into()]);
    for item in items {
        match item {
            Item::Module { items, .. } => register_program_surface(items, ctx),
            Item::Enum {
                name,
                variants,
                generics,
                ..
            } => {
                let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                ctx.enum_variants.insert(name.clone(), names);
                if !generics.is_empty() {
                    ctx.type_generics.insert(name.clone(), generics.len());
                }
            }
            Item::Struct {
                name,
                fields,
                generics,
                ..
            } => {
                // Field name → declared type, for `FieldAccess` synthesis in the inference core.
                ctx.struct_fields.insert(
                    name.clone(),
                    fields.iter().map(|(f, t)| (f.clone(), t.clone())).collect(),
                );
                if !generics.is_empty() {
                    ctx.type_generics.insert(name.clone(), generics.len());
                }
            }
            Item::Fn {
                name,
                params,
                span,
                requires,
                ensures,
                ret,
                effects,
                generics,
                generic_bounds,
                ..
            } => {
                // Flat function namespace: a redefinition is an error.
                if !ctx.all_fns.insert(name.clone()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_DUPLICATE_FUNCTION".into()),
                        message: format!("function `{}` is defined more than once", name),
                        span: Some((span.start, span.end)),
                    });
                }
                ctx.fn_params.insert(
                    name.clone(),
                    params.iter().map(|(_, ty)| ty.clone()).collect(),
                );
                ctx.fn_ret_types
                    .insert(name.clone(), ret.clone().unwrap_or_default());
                if !generics.is_empty() {
                    ctx.fn_generics.insert(name.clone(), generics.clone());
                }
                if !generic_bounds.is_empty() {
                    ctx.fn_bounds.insert(name.clone(), generic_bounds.clone());
                }
                if !effects.is_empty() {
                    ctx.fn_declared_effects
                        .insert(name.clone(), effects.clone());
                }
                if !requires.is_empty() || !ensures.is_empty() {
                    ctx.fn_contracts.insert(
                        name.clone(),
                        (
                            params.iter().map(|(n, _)| n.clone()).collect(),
                            requires.clone(),
                            ensures.clone(),
                        ),
                    );
                }
            }
            // Collect method arities (including `self`) so direct method calls can be arity-checked;
            // a name defined with differing arities across impls is marked ambiguous (None).
            Item::Impl { methods, .. } => {
                for m in methods {
                    if let Item::Fn { name, params, .. } = m {
                        let arity = params.len();
                        ctx.method_arities
                            .entry(name.clone())
                            .and_modify(|e| {
                                if *e != Some(arity) {
                                    *e = None;
                                }
                            })
                            .or_insert(Some(arity));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Walk a function body flagging calls to names that are neither a user function, a reserved
/// builtin, nor a local binding (parameter / let / for-variable / lambda-parameter / match-binding).
/// Closure-valued locals are in `bound`, so `let f = |x| x; f(3)` is fine.
fn check_calls_stmts(
    stmts: &[Stmt],
    fns: &BTreeSet<String>,
    bound: &mut BTreeSet<String>,
    ctx: &mut SemanticContext,
) {
    use crate::frontend::ForSource;
    for s in stmts {
        match s {
            Stmt::Let { name, init, .. } => {
                check_calls_expr(init, fns, bound, ctx);
                bound.insert(name.clone());
            }
            Stmt::LetPattern { pattern, init, .. } => {
                check_calls_expr(init, fns, bound, ctx);
                for n in pattern.bound_names() {
                    bound.insert(n);
                }
            }
            Stmt::Assign { target, value } => {
                check_calls_expr(target, fns, bound, ctx);
                check_calls_expr(value, fns, bound, ctx);
            }
            Stmt::ExprStmt(e) => check_calls_expr(e, fns, bound, ctx),
            Stmt::If { cond, then, else_ } => {
                check_calls_expr(cond, fns, bound, ctx);
                let mut b = bound.clone();
                check_calls_stmts(then, fns, &mut b, ctx);
                if let Some(e) = else_ {
                    let mut b = bound.clone();
                    check_calls_stmts(e, fns, &mut b, ctx);
                }
            }
            Stmt::While { cond, body, .. } => {
                check_calls_expr(cond, fns, bound, ctx);
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                check_calls_expr(expr, fns, bound, ctx);
                let mut b = bound.clone();
                for n in pattern.bound_names() {
                    b.insert(n);
                }
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::Loop { body, .. } => {
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::For {
                var, source, body, ..
            } => {
                match source {
                    ForSource::Range { start, end } => {
                        check_calls_expr(start, fns, bound, ctx);
                        check_calls_expr(end, fns, bound, ctx);
                    }
                    ForSource::Collection { expr } => check_calls_expr(expr, fns, bound, ctx),
                }
                let mut b = bound.clone();
                b.insert(var.clone());
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for blk in [gpu, cpu, prove].into_iter().flatten() {
                    let mut b = bound.clone();
                    check_calls_stmts(blk, fns, &mut b, ctx);
                }
            }
            Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        }
    }
}

fn check_calls_expr(
    e: &Expr,
    fns: &BTreeSet<String>,
    bound: &BTreeSet<String>,
    ctx: &mut SemanticContext,
) {
    match e {
        Expr::Call { callee, args } => {
            if !fns.contains(callee)
                && !bound.contains(callee)
                && !crate::backends::run::is_builtin_name(callee)
            {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_UNKNOWN_FUNCTION".into()),
                    message: format!("call to unknown function `{}`", callee),
                    span: None,
                });
            }
            for a in args {
                check_calls_expr(a, fns, bound, ctx);
            }
        }
        Expr::CallExpr { callee, args } => {
            check_calls_expr(callee, fns, bound, ctx);
            for a in args {
                check_calls_expr(a, fns, bound, ctx);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            check_calls_expr(lhs, fns, bound, ctx);
            check_calls_expr(rhs, fns, bound, ctx);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Try(expr) => check_calls_expr(expr, fns, bound, ctx),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            check_calls_expr(inner, fns, bound, ctx)
        }
        Expr::ArrayLiteral { elements } => {
            for el in elements {
                check_calls_expr(el, fns, bound, ctx);
            }
        }
        Expr::Index { base, index } => {
            check_calls_expr(base, fns, bound, ctx);
            check_calls_expr(index, fns, bound, ctx);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                check_calls_expr(v, fns, bound, ctx);
            }
        }
        Expr::FieldAccess { base, .. } => check_calls_expr(base, fns, bound, ctx),
        Expr::EnumConstruct {
            enum_name,
            variant,
            fields,
            ..
        } => {
            // Fail-closed: `Foo::Bar` must name a declared enum and a real variant. An unknown
            // enum name is either a typo or a Rust-style qualified call (`math::double(...)`) —
            // the call namespace is flat, so neither is valid. Without this check both silently
            // lower to a stringy enum value at runtime instead of trapping.
            match ctx.enum_variants.get(enum_name).cloned() {
                None => ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_UNKNOWN_ENUM".into()),
                    message: format!(
                        "`{enum_name}::{variant}` refers to unknown type `{enum_name}` \
                         (declare `enum {enum_name}`, or call `{variant}(...)` directly — \
                         the call namespace is flat, there are no `::`-qualified calls)"
                    ),
                    span: None,
                }),
                Some(variants) if !variants.contains(variant) => {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_UNKNOWN_VARIANT".into()),
                        message: format!(
                            "enum `{enum_name}` has no variant `{variant}` (known: {})",
                            variants.join(", ")
                        ),
                        span: None,
                    })
                }
                _ => {}
            }
            for f in fields {
                check_calls_expr(f, fns, bound, ctx);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            check_calls_expr(scrutinee, fns, bound, ctx);
            for arm in arms {
                let mut b = bound.clone();
                for x in arm.pattern.bound_names() {
                    b.insert(x);
                }
                if let Some(guard) = &arm.guard {
                    check_calls_expr(guard, fns, &b, ctx);
                }
                check_calls_expr(&arm.body, fns, &b, ctx);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            check_calls_expr(cond, fns, bound, ctx);
            check_calls_expr(then, fns, bound, ctx);
            check_calls_expr(else_, fns, bound, ctx);
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            check_calls_expr(scrutinee, fns, bound, ctx);
            let mut b = bound.clone();
            for n in pattern.bound_names() {
                b.insert(n);
            }
            check_calls_expr(then, fns, &b, ctx);
            check_calls_expr(else_, fns, bound, ctx);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                check_calls_expr(k, fns, bound, ctx);
                check_calls_expr(v, fns, bound, ctx);
            }
        }
        Expr::Block { stmts, tail } => {
            let mut b = bound.clone();
            check_calls_stmts(stmts, fns, &mut b, ctx);
            if let Some(t) = tail {
                check_calls_expr(t, fns, &b, ctx);
            }
        }
        Expr::Lambda { params, body } => {
            let mut b = bound.clone();
            for p in params {
                b.insert(p.clone());
            }
            check_calls_expr(body, fns, &b, ctx);
        }
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::RawPtr { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::TaintSource { .. }
        | Expr::Other(_) => {}
    }
}

fn collect_items(
    items: &[Item],
    module: Option<&str>,
    requested_mode: Mode,
    ctx: &mut SemanticContext,
) {
    for item in items {
        match item {
            Item::Import { path, .. } => ctx.hir.imports.push(path.clone()),
            Item::Module { name, items, .. } => {
                ctx.hir.modules.push(name.clone());
                collect_items(items, Some(name), requested_mode, ctx);
            }
            Item::Fn {
                name,
                params,
                body,
                mode,
                span,
                attributes,
                ret,
                requires,
                ensures,
                effects: declared_effects,
                ..
            } => {
                let effective_mode = if *mode == Mode::Safe {
                    requested_mode
                } else {
                    *mode
                };
                // Gate 15: enforce authorization for research/poc/fuzz etc.
                if matches!(effective_mode, Mode::Research) {
                    let has_auth = attributes.iter().any(|attr| {
                        matches!(
                            attr.name.as_str(),
                            "research" | "poc" | "fuzz" | "proof" | "defensive" | "audit"
                        ) && attr
                            .args
                            .iter()
                            .any(|a| a.key == "authorization" && !a.value.is_empty())
                    });
                    if !has_auth && !attributes.is_empty() {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_RESEARCH_MISSING_AUTHORIZATION".into()),
                            message: "research/poc/fuzz/proof/defensive/audit requires authorization=... metadata".to_string(),
                            span: Some((span.start, span.end)),
                        });
                    }
                }
                // Per-item verification lane: `@verified` / `#[verified]` on this fn.
                let item_verified = attributes.iter().any(|a| a.name == "verified");
                let saved_verified = ctx.verified;
                if item_verified {
                    ctx.verified = true;
                }
                analyze_function(
                    name,
                    module,
                    params,
                    body,
                    ret.as_deref(),
                    requires,
                    ensures,
                    declared_effects,
                    effective_mode,
                    *span,
                    false,
                    ctx,
                );
                ctx.verified = saved_verified;
            }
            Item::Struct { .. } => {
                // Minimal support for this slice: structs are parsed and preserved in AST;
                // full type registration and field typing added in typechecker work.
            }
            Item::Enum { name, variants, .. } => {
                let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                ctx.enum_variants.insert(name.clone(), names);
            }
            // Methods are analyzed like free functions (their `self`/params are in scope for the
            // body) but flagged `is_method`, so they are not registered as callable-by-name —
            // they dispatch on the receiver and must not shadow a same-named builtin.
            Item::Impl { methods, .. } => {
                for m in methods {
                    if let Item::Fn {
                        name,
                        params,
                        body,
                        mode,
                        span,
                        ret,
                        requires,
                        ensures,
                        effects: declared_effects,
                        attributes,
                        ..
                    } = m
                    {
                        let effective_mode = if *mode == Mode::Safe {
                            requested_mode
                        } else {
                            *mode
                        };
                        let item_verified = attributes.iter().any(|a| a.name == "verified");
                        let saved_verified = ctx.verified;
                        if item_verified {
                            ctx.verified = true;
                        }
                        analyze_function(
                            name,
                            module,
                            params,
                            body,
                            ret.as_deref(),
                            requires,
                            ensures,
                            declared_effects,
                            effective_mode,
                            *span,
                            true,
                            ctx,
                        );
                        ctx.verified = saved_verified;
                    }
                }
            }
            // Traits are desugared away before this pass (resolve_traits); none should remain.
            Item::Trait { .. } => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn analyze_function(
    name: &str,
    module: Option<&str>,
    params: &[(String, String)],
    body: &[Stmt],
    ret: Option<&str>,
    requires: &[Expr],
    ensures: &[Expr],
    declared_effects: &[String],
    mode: Mode,
    span: Span,
    is_method: bool,
    ctx: &mut SemanticContext,
) {
    if mode != Mode::Safe {
        ctx.has_research = true;
    }

    // Record this function's declared return type for the typed-`?` check (read at each `?` site in
    // `check_expr_semantics`). Functions do not nest, so a plain assignment per function is correct;
    // the lambda arm of `check_expr_semantics` clears it around a closure body. `None` for no `-> T`.
    ctx.current_fn_return = ret.map(|r| r.to_string());

    // Generic-instantiation arity over this function's signature annotations (`fn f(p: Pair<u32>)` or
    // `-> Pair<u32>` when `Pair` declares two parameters). Shadow-gated inside the helper.
    for (_, pty) in params {
        check_generic_arity_annotation(pty, Some((span.start, span.end)), ctx);
    }
    if let Some(rty) = ret {
        check_generic_arity_annotation(rty, Some((span.start, span.end)), ctx);
    }

    // Solver integer-modelability and symbolic widths are FUNCTION-LOCAL: a variable modeled as an
    // i64 in one function must not leak that modelability to a same-named binding in another function
    // (which could hold a string/list/bool), or an integer predicate over the second would be "proved"
    // against the first's model. Reset per function. (Obligations/constraints accumulate globally.)
    ctx.solver_int_vars.clear();
    ctx.solver_float_vars.clear();
    ctx.solver_string_vars.clear();
    ctx.active_branch_guards.clear();
    ctx.symbolic_widths.clear();
    // BUILTIN-SHADOW detection — MUST run BEFORE any `requires` is seeded (a seeded `requires` over a
    // shadowed builtin would otherwise be modeled with builtin semantics and POISON the assumptions → a
    // review-confirmed false accept). Precompute which MODELED builtins are shadowed in this function by a
    // top-level user fn (`all_fns`) OR a local param/`let` (the runtime resolves user-fn/local before the
    // builtin). `shadowed_string_preds` gates the string-PREDICATE lane; the `shadow_builtin_mark` sentinels
    // ride solver_int_vars/solver_string_vars (like nzdiv/seq marks, never emitted to SMT) to gate the
    // int (`abs`/`min`/`max`/seq-`len`) and strlen (`len`) builtins. Runs for EVERY function — a contract-
    // less body can still `assert(max(3,5)==0)` over a shadowed `max`.
    ctx.shadowed_string_preds.clear();
    {
        let mut locals = BTreeSet::new();
        for (pn, _) in params {
            locals.insert(pn.clone());
        }
        collect_let_bound(body, &mut locals);
        for name in ["contains", "starts_with", "ends_with"] {
            if ctx.all_fns.contains(name) || locals.contains(name) {
                ctx.shadowed_string_preds.insert(name.to_string());
            }
        }
        for name in ["abs", "min", "max", "len"] {
            if ctx.all_fns.contains(name) || locals.contains(name) {
                ctx.solver_int_vars.insert(shadow_builtin_mark(name));
                if name == "len" {
                    ctx.solver_string_vars.insert(shadow_builtin_mark("len"));
                }
            }
        }
        // `index_of` (str.indexof lane) and `substr` (str.substr lane) — string-lane builtins; their shadow
        // marks ride solver_string_vars (is_strlen_term / is_string_modelable consult them).
        for name in ["index_of", "substr"] {
            if ctx.all_fns.contains(name) || locals.contains(name) {
                ctx.solver_string_vars.insert(shadow_builtin_mark(name));
            }
        }
    }
    // Authorize declared capabilities for Safe-mode I/O (Phase-3 C5 dual-mode crown).
    ctx.authorized_caps = declared_effects
        .iter()
        .map(|e| normalize_effect_name(e))
        .collect();

    // A declared `-> T` return type is checked against any return value that is a LITERAL of an
    // unambiguously incompatible type (a bare string/number/bool/list/map/enum). Non-literal
    // returns (variables, calls, if/match, a trailing statement that yields 0) are left unchecked
    // — the type is dynamic — so this catches `fn f() -> u32 { "s" }` with zero false positives.
    if let Some(rty) = ret {
        let pscope: BTreeMap<String, ScopeBinding> = params
            .iter()
            .map(|(n, t)| {
                (
                    n.clone(),
                    ScopeBinding {
                        info: BindingInfo {
                            name: n.clone(),
                            ty: Some(t.clone()),
                            mode: String::new(),
                            tainted: false,
                            taint_source: None,
                            declassified: false,
                            span: None,
                        },
                        closure_arity: None,
                        secret: false,
                    },
                )
            })
            .collect();
        check_return_types(body, rty, &pscope, span, ctx);
    }

    // The `?` operator unwraps `Some`/`Ok` and early-returns `None`/`Err`, so it only makes sense in
    // a function that returns `Option`/`Result`. If a function declares a CONCRETE non-Option/Result
    // return type and uses `?`, it can only fail closed at runtime (`ANUBIS_TRY_ON_NON_OPTION_RESULT`)
    // — reject it statically. A function with no declared return type is dynamic, and a generic or
    // opaque (`any`/`unknown`) return is left alone, so no working dynamic program is newly rejected.
    if let Some(rty) = ret {
        let r = rty.trim();
        let norm = normalize_ty(r);
        let result_like = r.starts_with("Option") || r.starts_with("Result");
        let opaque = norm == "any" || norm == "unknown";
        if !r.is_empty() && !result_like && !opaque && !ty::is_generic(r) && body_contains_try(body)
        {
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some("ANUBIS_TRY_OUTSIDE_RESULT".into()),
                message: format!(
                    "`{name}` uses the `?` operator but declares `-> {r}`; `?` requires the function to return `Option` or `Result`"
                ),
                span: Some((span.start, span.end)),
            });
        }
    }

    // A+ call-site typing: record this function's parameter types for later calls. Methods are
    // NOT recorded — they are only reachable via `recv.m(...)`, never a bare call, so recording
    // them would shadow a same-named stdlib builtin at free call sites.
    if !is_method {
        ctx.fn_params.insert(
            name.to_string(),
            params.iter().map(|(_, ty)| ty.clone()).collect(),
        );
    }

    // Duplicate parameter names are a hard error.
    let mut seen_params = BTreeSet::new();
    for (pname, _) in params {
        if !seen_params.insert(pname.clone()) {
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some("ANUBIS_DUPLICATE_PARAM".into()),
                message: format!("duplicate parameter `{}` in function `{}`", pname, name),
                span: Some((span.start, span.end)),
            });
        }
    }

    // Flag calls to unknown functions in this body (not a user fn, builtin, or local binding).
    {
        let fns = ctx.all_fns.clone();
        let mut bound: BTreeSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        check_calls_stmts(body, &fns, &mut bound, ctx);
    }

    let mut scope = BTreeMap::<String, ScopeBinding>::new();
    let mut fn_symbols = vec![];
    let mut effects = vec![];
    let mut assumptions = vec![];
    let param_bindings = params
        .iter()
        .map(|(name, ty)| {
            let tainted = is_tainted_type(Some(ty));
            let info = BindingInfo {
                name: name.clone(),
                ty: Some(ty.clone()),
                mode: mode_name(mode).into(),
                tainted,
                taint_source: tainted.then(|| name.clone()),
                declassified: false,
                span: None,
            };
            if tainted {
                ctx.taint_labels.push(format!("{}: {}", name, ty));
            }
            scope.insert(
                name.clone(),
                ScopeBinding {
                    info: info.clone(),
                    closure_arity: None,
                    // A `secret<T>` param qualifier auto-labels the parameter as secret (the
                    // confidentiality dual of the `tainted<T>` param seeded above), so a secret
                    // arriving via a param needs no `secret_source(..)` call — an egress of it is
                    // exfiltration. Retires the ROADMAP leg-1 "no secret<T> qualifier" boundary.
                    secret: is_secret_type(Some(ty)),
                },
            );
            // Parameters are in-scope for the whole body, so a `let s = param` must not
            // report the parameter as an unknown variable.
            ctx.known_bindings.insert(name.clone());
            info
        })
        .collect::<Vec<_>>();

    // B2 contracts: make integer parameters solver-modelable, assume each `requires` precondition,
    // then (after the body) assert each `ensures` postcondition at the tail return. The body plus
    // the precondition must PROVE the postcondition — discharged by the (now-sound i64) solver.
    // Only functions that DECLARE a contract model their parameters symbolically, so a plain
    // function's assertions keep their prior (param-opaque) semantics — no regression.
    let has_contract = !requires.is_empty() || !ensures.is_empty();
    if has_contract {
        // Make integer parameters solver-modelable. NOTE: a `u32`/`u8` annotation is INERT at
        // runtime (a parameter holds any i64; the call boundary applies no width clamp), so we must
        // NOT assume it lies in [0, 2^w-1] — doing so let the solver "prove" `x + 1 > x` while
        // `f(i64::MAX)` wraps and violates it. A contract that needs bounds must state them via
        // `requires`; unbounded i64 arithmetic that can overflow is (correctly) not provable.
        for (pname, pty) in params {
            // Only INTEGER params are solver-modelable. A float param must NOT be modeled as an i64
            // bit-vector (that "proved" `2*x != 1` for `x = 0.5`); an integer `ensures` that then
            // references it becomes non-modelable and fails closed below.
            if is_integer_ty(pty) {
                ctx.solver_int_vars.insert(pname.clone());
                ctx.symbolic_widths.insert(pname.clone(), 64);
            } else if is_float_ty(pty) {
                // Phase-3 QF_FP: a float param is now solver-modelable — declared IEEE-754 Float64, so a
                // float `ensures`/`assert` over `+ - *` and comparisons can be discharged (previously it
                // fell through to ANUBIS_FLOAT_CONTRACT_UNMODELED). Kept OUT of `solver_int_vars` so it is
                // never mixed into an i64 bit-vector model.
                ctx.solver_float_vars.insert(pname.clone());
            } else if pty == "string" {
                // Phase-3 QF_S: a `string` param is a QF_S `String` symbol, so a string-equality
                // `ensures`/`requires`/`assert` discharges (previously ANUBIS_STRING_CONTRACT_UNMODELED).
                ctx.solver_string_vars.insert(pname.clone());
            }
        }
        // Struct-PARAM field modeling: for each field of a struct parameter that a `requires` mentions,
        // register a canonical per-(base, field) symbol in the field's declared lane, so a later
        // assert/ensures over the SAME field is proven or DISPROVED instead of fail-opening (the P5
        // adversarial-hunt false accept). Skipped when the base param is reassigned OR SHADOWED anywhere in
        // the body — a `let p = P{..}` shadow (or a match/if-let arm re-binding `p`) leaves the entry
        // `requires` seed for the field stale, and an assert over `p.field` would then be proved against the
        // call-entry value while the runtime reads the shadowed one (a review-confirmed false proof). Both
        // `collect_assigned_roots` (assignments, incl. field writes `p.n = ..` rooted to `p`) AND
        // `collect_let_bound` (let/pattern shadows) are needed — exactly as the nzdiv-divisor gate below and
        // the ensures anti-launder guard do; the mangled symbol does not carry the base name as a token the
        // reassignment invalidation could drop, so this gate is the sole guard. Declining is fail-open.
        let mut registered_field_syms: Vec<String> = Vec::new();
        {
            let mut base_reassigned = BTreeSet::new();
            collect_assigned_roots(body, &mut base_reassigned);
            collect_let_bound(body, &mut base_reassigned);
            let param_ty: BTreeMap<&str, &str> =
                params.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
            let mut field_accesses = Vec::new();
            for req in requires {
                collect_field_accesses(req, &mut field_accesses);
            }
            for (base, field) in field_accesses {
                if base_reassigned.contains(&base) {
                    continue;
                }
                let Some(pty) = param_ty.get(base.as_str()) else {
                    continue;
                };
                let Some(fields) = ctx.struct_fields.get(*pty) else {
                    continue;
                };
                let Some(fty) = fields.get(&field) else {
                    continue;
                };
                let sym = mangle_field(&base, &field);
                if is_integer_ty(fty) {
                    ctx.solver_int_vars.insert(sym.clone());
                    ctx.symbolic_widths.insert(sym.clone(), 64);
                } else if is_float_ty(fty) {
                    ctx.solver_float_vars.insert(sym.clone());
                } else if fty.as_str() == "string" {
                    ctx.solver_string_vars.insert(sym.clone());
                } else {
                    continue;
                }
                registered_field_syms.push(sym);
            }
        }
        for req in requires {
            seed_requires_fact(ctx, &mut assumptions, req);
        }
        // COVERAGE PRUNE (LESSON 12 — the "stranded obligation" trap): a field mentioned in a `requires`
        // is registered ABOVE, but the clause may be UNSEEDABLE (a non-ASCII literal, a user-predicate
        // call, …) so `seed_requires_fact` pushed no constraining fact for it. A registered-but-unseeded
        // field would strand a later assert/ensures over it against a FREE solver var → z3 finds a spurious
        // counterexample → over-rejection of a valid program the pre-slice code fail-opened. So un-register
        // any field symbol NO assumption references: it reverts to unmodeled (fail-open), exactly as
        // before. Only fields whose fact actually landed in the shared `assumptions` channel stay modeled.
        registered_field_syms.retain(|sym| {
            let smt_name = smt_var(sym);
            let seeded = assumptions.iter().any(|a| {
                let mut vs = BTreeSet::new();
                collect_vars_from_smt(a, &mut vs);
                vs.contains(&smt_name)
            });
            if !seeded {
                ctx.solver_int_vars.remove(sym);
                ctx.solver_float_vars.remove(sym);
                ctx.solver_string_vars.remove(sym);
                ctx.symbolic_widths.remove(sym);
            }
            seeded
        });
        // Mark parameters that a `requires` guard proves non-zero AS DIVISORS, so `x / v` / `x % v`
        // become modelable. Sound only if the guarantee holds at the division: require the parameter
        // to be a modeled integer AND never reassigned or shadowed in the body (else the entry guard
        // need not hold later). Because such a variable is stable, the mark never needs removal.
        let mut rebound = BTreeSet::new();
        collect_assigned_roots(body, &mut rebound);
        collect_let_bound(body, &mut rebound);
        for req in requires {
            if let Some(v) = requires_nonzero_var(req) {
                if ctx.solver_int_vars.contains(&v) && !rebound.contains(&v) {
                    ctx.solver_int_vars.insert(nzdiv_mark(&v));
                }
            }
        }
    }
    // The precondition (parameter ranges + `requires`) dominates EVERY return; the body assumptions
    // added below (lets, composition) only dominate the tail return.
    let precondition_assumptions = assumptions.clone();

    // Phase-3 QF_FP: record every variable reassigned ANYWHERE in this body (incl. writes embedded in a
    // `match`-arm / `if`-expression / block the statement-level frame sweep does not visit). A float
    // `let` is chained only when its name is NOT here, so its defining-fact cannot leak past such a write.
    ctx.reassigned_roots.clear();
    collect_assigned_roots(body, &mut ctx.reassigned_roots);
    ctx.shadowed_lets.clear();
    collect_shadowed_lets(body, &mut ctx.shadowed_lets);
    // (BUILTIN-SHADOW detection — shadowed_string_preds + shadow_builtin_mark sentinels — is computed
    // earlier, right after the solver-set clears, so it precedes requires-seeding; see there.)

    analyze_stmts(
        body,
        mode,
        &mut scope,
        &mut fn_symbols,
        &mut effects,
        &mut assumptions,
        ctx,
    );

    // Phase-2 slice 2/3: capability-token LINEARITY + effect AUTHORIZATION. `check_linearity`
    // (middle/capability.rs) is the pure intraprocedural walk. Slice 2 (linearity): a
    // `cap_acquire`-minted token is used exactly once, non-duplicable (move-on-rebind), unforgeable
    // (`cap_use` on a non-token is MISSING) — dual-mode, default accept-biased / verified fail-closed
    // toward consumed. Slice 3 (composition): in verified mode, a directly-performed privileged
    // effect must have a genuine acquisition (`ANUBIS_EFFECT_UNAUTHORIZED`), closing the
    // unknown-provenance forge vector.
    // All three (REUSE / MISSING / EFFECT_UNAUTHORIZED) are ENFORCING. EFFECT_UNAUTHORIZED promoted
    // on evidence — the corpus shadow diff at UNEXPECTED=0 over 175 programs, zero shadow lines on
    // `selfhost/src/anubis_sh.anb` and every stdlib module, scratch fire/inert/accept runs, AND the
    // full `cargo test` green (the shadow diff scans only `.anb`, so it cannot see the verified-lane
    // Rust tests this composition tightens — those are updated to acquire their authorizing token).
    for f in capability::check_linearity(params, body, ctx.verified, (span.start, span.end), &ctx.all_fns)
    {
        ctx.emit(
            SemanticDiagnostic {
                code: Some(f.code.into()),
                message: f.message,
                span: f.span,
            },
            false,
        );
    }

    // Phase-3 C2/C5: declared-vs-inferred effect check.
    // - When `uses(...)` is present: inferred capability effects must be ⊆ declared.
    // - Verification lane (`ctx.verified` / `--verified`): capability effects also require that a
    //   `uses(...)` clause exists at all (absent clause is fail-closed).
    // Internal analysis tags (taint-*, assume, assert, loop, declassify, …) are not gated.
    let caps_used: BTreeSet<String> = effects
        .iter()
        .filter_map(|e| capability_effect(e))
        .collect();
    if ctx.verified && !caps_used.is_empty() && declared_effects.is_empty() {
        ctx.diagnostics.push(SemanticDiagnostic {
            code: Some("ANUBIS_UNDECLARED_EFFECT".into()),
            message: format!(
                "verification lane: function `{name}` uses capability effect(s) [{}] but declares no `uses(...)` clause",
                caps_used.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
            span: Some((span.start, span.end)),
        });
    }
    if !declared_effects.is_empty() {
        let declared: BTreeSet<String> = declared_effects
            .iter()
            .map(|e| normalize_effect_name(e))
            .collect();
        let mut seen_undeclared = BTreeSet::new();
        for cap in &caps_used {
            if !declared.contains(cap) && seen_undeclared.insert(cap.clone()) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_UNDECLARED_EFFECT".into()),
                    message: format!(
                        "function `{name}` uses effect `{cap}` but does not declare it in `uses(...)` (declared: {})",
                        declared_effects.join(", ")
                    ),
                    span: Some((span.start, span.end)),
                });
            }
        }
    }

    // Phase-2 slice 1: TRANSITIVE declared-vs-inferred effect check. The per-body check above sees
    // only direct builtins plus a direct callee's DECLARED caps, so an unclaused helper
    // (`fn helper() { time_now(); }`) launders its effects past every caller. `fn_effect_rows` is
    // the monotone call-graph fixpoint (middle/effects.rs); here we check the caps this function
    // TRANSITIVELY performs. Two directions held at once:
    //   - the EFFECT SET widens on uncertainty (an unresolvable callee marks the row `open`), so a
    //     genuine undeclared effect is never hidden by an unknown callee sitting next to it;
    //   - the REJECT DECISION stays accept-biased: `open` alone NEVER fires — only CONCRETE caps
    //     do — so effect-polymorphic higher-order code is never falsely rejected on ignorance.
    // Fires only on caps beyond `caps_used` (never re-reports what the enforcing check above
    // already caught). ENFORCING (`emit(.., false)`): promoted on evidence — corpus shadow diff at
    // UNEXPECTED=0 over 162 programs, zero shadow lines on `selfhost/src/anubis_sh.anb` and every
    // stdlib module, fire/inert/accept-edge scratch runs verified before the flip.
    let transitive_caps: Vec<String> = ctx
        .fn_effect_rows
        .get(name)
        .map(|row| {
            row.effects
                .iter()
                .filter(|cap| !caps_used.contains(*cap))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if !transitive_caps.is_empty() {
        if ctx.verified && declared_effects.is_empty() {
            ctx.emit(
                SemanticDiagnostic {
                    code: Some("ANUBIS_UNDECLARED_EFFECT".into()),
                    message: format!(
                        "verification lane: function `{name}` uses capability effect(s) [{}] (via transitive call) but declares no `uses(...)` clause",
                        transitive_caps.join(", ")
                    ),
                    span: Some((span.start, span.end)),
                },
                false,
            );
        }
        if !declared_effects.is_empty() {
            let declared: BTreeSet<String> = declared_effects
                .iter()
                .map(|e| normalize_effect_name(e))
                .collect();
            for cap in &transitive_caps {
                if !declared.contains(cap) {
                    ctx.emit(
                        SemanticDiagnostic {
                            code: Some("ANUBIS_UNDECLARED_EFFECT".into()),
                            message: format!(
                                "function `{name}` uses effect `{cap}` (via transitive call) but does not declare it in `uses(...)` (declared: {})",
                                declared_effects.join(", ")
                            ),
                            span: Some((span.start, span.end)),
                        },
                        false,
                    );
                }
            }
        }
    }

    // Phase-2 slice 2: an OPEN effect row is forbidden in the verification lane. The effect slice
    // left `open` accept-biased (an unresolvable callee — function-valued parameter, closure, or
    // method call — never fired a diagnostic). But the capability discipline needs the effect set
    // to be a total upper bound: tokens can only gate a COMPLETE set. Verified mode therefore
    // requires a closed row (no unbounded effect tail); the default lane stays permissive and open
    // rows remain legal. Independent of `transitive_caps` — a purely-open row (no concrete caps)
    // still fires. ENFORCING (`emit(.., false)`), promoted on evidence: inert on the corpus (no
    // committed program is checked under `--verified`/`@verified`, shadow diff UNEXPECTED=0), with
    // `@verified` fixtures and unit tests proving the reject/accept pair before the flip.
    if ctx.verified {
        if let Some(row) = ctx.fn_effect_rows.get(name) {
            if row.open {
                ctx.emit(
                    SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_OPEN_IN_VERIFIED".into()),
                        message: format!(
                            "verification lane: function `{name}` has an open (unbounded) effect row — it calls a function-valued parameter, closure, or method whose effects cannot be determined. Verified mode requires a complete effect set; make every callee's effects determinable."
                        ),
                        span: Some((span.start, span.end)),
                    },
                    false,
                );
            }
        }
    }

    // Phase-2 FINAL slice: the LETHAL TRIFECTA as a compile error — ENFORCING in the Safe (default)
    // lane (the Phase-2 differentiator). A function forms the trifecta when it holds all three lethal
    // capabilities at
    // once: leg 1 — accesses PRIVATE data (fs.read — a file was read — OR an explicit `secret_source(..)`
    // confidentiality label); leg 2 — is exposed to UNTRUSTED input from a channel DISTINCT from the
    // read (input/recv/env/taint_source/tainted<T> param); leg 3 — COMMUNICATES externally (net.send OR
    // a shell-out: exec/system/shell/target_run).
    // An injection in the untrusted input can then steer the private read and the egress — the danger
    // the value-flow taint check (`ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`, Safe-mode) cannot see when
    // no value literally flows read→send (e.g. a constant beacon steered by control flow). This is a
    // COEXISTENCE check, not a flow check, so on a value-flow program it may co-fire with the
    // Safe-mode sink gate (both are true); its genuinely-new coverage is the no-flow coexistence.
    // LANE: fires in Safe (default) AND verified — NOT in Research/Exploit unless `@verified` (the
    // dual-use lanes bypass, like the `mode == Mode::Safe` capability gates, plus the retained verified
    // firing). It is ENFORCING in both lanes (`emit(.., false)`) — this is the PROMOTION of the
    // Safe-mode move, which landed shadow-first (`ec69ab6`, `emit(.., !ctx.verified)`) to soak: the
    // shadow diff proved it fires on nothing committed (UNEXPECTED=0) and a full-corpus verdict-diff
    // confirmed zero flips, so it is now a Safe-mode compile error. Fixpoint-safe: checker-only sidecar
    // (no HIR/MIR/projection mutation,
    // only `ctx.emit`), so the self-host binary fixpoint is untouched regardless of lane — and no
    // committed program forms an undeclassified 3-leg trifecta in a Safe or verified function (corpus
    // shadow diff UNEXPECTED=0; zero trifecta lines on `selfhost/src/anubis_sh.anb` and every stdlib
    // module — anubis_sh.anb has no egress leg at all).
    // Legs 1/3 read off the transitive effect row. In VERIFIED, an open row was already rejected above
    // (ANUBIS_EFFECT_OPEN_IN_VERIFIED), so the row is closed; in SAFE, open rows stay legal, so a cap
    // reachable only through an unresolvable callee is absent from the row and leg detection
    // UNDER-approximates — accept-biased (it can only fail to fire, never over-fire). Leg 2 + the escape
    // hatch (a WELL-FORMED `declassify(v, policy, reason)` — NOT the raw effect tag, which is pushed even
    // for malformed ones) are body-scanned (middle/trifecta.rs).
    // Legs 1 and 2 are INTERPROCEDURAL: leg 1 = `fs.read` (coarse proxy) OR a `secret_source(..)` label
    // OR a call to a `secret_fns` helper; leg 2 = a direct `is_leg2_source`/`taint_source`/`tainted<T>`
    // param OR a call to a `leg2_fns` helper (transitively sources untrusted input, presence-based —
    // the `read_file`/leg-2 conflation is avoided because `leg2_fns` is built with `is_leg2_source`,
    // which excludes file reads).
    // Honest boundaries (deferred): a `secret<T>` param IS now leg-1 private data (auto-labelled via
    // the qualifier — see `scan_legs`), but an unannotated `getenv`/`env` value is deliberately NOT
    // auto-secreted (env is untrusted INPUT, an integrity taint source — conflating it with secret
    // OUTPUT would false-positive on non-secret config egressed to an egress-only sink); leg 3 =
    // net.send or a shell-out — `http_get`/`http_post` are NOT yet
    // classified as a net.send effect (so an http constant-beacon exfil under-fires — a leg-3
    // completeness residual alongside the deferred `sql` egress); the declassify hatch is coarse
    // (function-level, not tied to the specific outbound value — so on a constant beacon a
    // developer-attested `declassify` is the relief); leg-isolation only relieves when the
    // untrusted-input path and the read+egress path share NO transitive caller (a lone `main` calling
    // both RE-FORMS the trifecta — the diagnostic says so honestly); leg 1/2 helpers are keyed by
    // callee NAME (method/closure-valued calls are a further increment).
    if mode == Mode::Safe || ctx.verified {
        let (has_fs_read, has_net_send, has_shell) = {
            let row = ctx.fn_effect_rows.get(name);
            let held = |cap: &str| {
                caps_used.contains(cap) || row.is_some_and(|r| r.effects.contains(cap))
            };
            (held("fs.read"), held("net.send"), held("shell"))
        };
        // Leg 3 — external communication — is network egress (net.send) OR a shell-out (exec/system/
        // shell/target_run → the `shell` cap): a shell command (`curl -d @secret …`, `nc`) is the
        // canonical agent exfiltration channel and reaches the network as directly as net.send, so
        // the coexistence is exactly as lethal. Name the actual channel(s) held in the diagnostic.
        let egress = match (has_net_send, has_shell) {
            (true, true) => Some("net.send, shell"),
            (true, false) => Some("net.send"),
            (false, true) => Some("shell"),
            (false, false) => None,
        };
        if let Some(egress) = egress {
            let legs = trifecta::scan_legs(body, params, &ctx.secret_fns, &ctx.leg2_fns);
            // Leg 1 — private-data access — is `fs.read` (a file was read: coarse over-approximating
            // proxy) OR an explicit `secret_source(..)` confidentiality label (precise). The label
            // closes the gap that a secret held in memory — not from a file — was invisible to fs.read.
            // `secret_present` is set by a `secret_source(..)` value OR a `secret<T>` param qualifier,
            // so name it generically ("a secret value") rather than claiming a `secret_source` call.
            let leg1 = match (has_fs_read, legs.secret_present) {
                (true, true) => Some("fs.read + a secret value"),
                (true, false) => Some("fs.read"),
                (false, true) => Some("a secret value"),
                (false, false) => None,
            };
            if let (Some(leg1), Some(leg2)) = (leg1, legs.leg2_untrusted) {
                if !legs.wellformed_declassify {
                    // ENFORCING in BOTH the Safe (default) and verified lanes (`emit(.., false)`). This
                    // is the PROMOTION of the Safe-mode move: the lethal trifecta is now a Safe-mode
                    // compile error — the Phase-2 differentiator. It landed shadow-first (`ec69ab6`,
                    // `emit(.., !ctx.verified)`) to soak; the shadow diff proved it fires on nothing
                    // committed (UNEXPECTED=0), and a full-corpus verdict-diff confirmed zero flips, so
                    // it is promoted to enforcing. Research/Exploit (unverified) still bypass (the gate
                    // above excludes them) — the dual-use lanes.
                    ctx.emit(
                        SemanticDiagnostic {
                            code: Some("ANUBIS_LETHAL_TRIFECTA".into()),
                            message: format!(
                                "function `{name}` forms the lethal trifecta — it accesses private data (`{leg1}`), is exposed to untrusted input (`{leg2}`), and communicates externally (`{egress}`) with no declassify barrier. An injection in the untrusted input can steer the private read and the egress even with no direct read→send data flow. Interpose a well-formed `declassify(value, policy, reason)` on the outbound value, or restructure so that no single function — and no common transitive caller — holds all three legs at once (drop the external channel or the private-data access, or keep the untrusted-input path and the private read+egress in functions that share no caller)."
                            ),
                            span: Some((span.start, span.end)),
                        },
                        false,
                    );
                }
            }
        }
    }

    // Discharge each `ensures` at EVERY return, so no return path can violate the postcondition:
    //   - the TAIL return is verified under the full body assumptions (they all dominate it);
    //   - each EARLY/nested return is verified under the precondition alone (a sound subset — this
    //     catches an unconditionally-violating early return like `return 0` vs `ensures(result>0)`,
    //     and can only ever mis-DISPROVE a path-dependent return, never mis-prove one).
    // Modeling is best-effort: a postcondition the solver cannot express (strings/lists/division) is
    // left un-obligated rather than mis-disproved.
    if !ensures.is_empty() {
        // A parameter named in an `ensures` denotes the CALL-ENTRY value — composition substitutes the
        // caller's original argument into the callee's `ensures`. Anubis has no `old()`, so if the body
        // REASSIGNS or SHADOWS such a parameter, its `ensures` would be discharged against the mutated
        // value while the caller assumes the entry value — a false certification laundered through
        // composition (`ensures(result == x) { x = 9; return x; }`). Fail closed.
        let param_names: BTreeSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let mut ensures_vars = BTreeSet::new();
        for e in ensures {
            collect_expr_vars(e, &mut ensures_vars);
        }
        let mut rebound = BTreeSet::new();
        collect_assigned_roots(body, &mut rebound);
        collect_let_bound(body, &mut rebound);
        for p in ensures_vars.intersection(&param_names) {
            if rebound.contains(p) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_CONTRACT_UNPROVABLE".into()),
                    message: format!(
                        "cannot verify a postcondition over parameter `{p}`: it is reassigned or \
                         shadowed in the body, but `ensures` refers to the parameter's call-entry \
                         value (there is no `old()`). Keep the parameter unmodified and return a \
                         local instead (`let r = ...; return r;`)"
                    ),
                    span: Some((span.start, span.end)),
                });
            }
        }
        // Every value the body can yield at its tail (a bare tail `if`/`match`'s arms, a block tail,
        // or `0` when it falls off the end) is checked under the full body assumptions.
        let mut tail_vals = Vec::new();
        tail_values(body, true, &mut tail_vals);
        for tv in &tail_vals {
            push_ensures_obligations(ctx, ensures, tv, &assumptions, span);
        }
        // Every explicit return except the tail return-call (the last statement).
        let n = body.len();
        let mut early = Vec::new();
        for (i, s) in body.iter().enumerate() {
            let is_tail_ret = i + 1 == n
                && matches!(s, Stmt::ExprStmt(Expr::Call { callee, .. }) if callee == "return");
            if !is_tail_ret {
                collect_returns_in_stmt(s, &mut early);
            }
        }
        // SOUNDNESS: an EARLY return is discharged against `precondition_assumptions` — the FROZEN
        // call-entry state (the tail return, by contrast, uses the live body `assumptions` that reflect
        // reassignments). If the returned expression depends on a parameter REASSIGNED in the body, that
        // frozen precondition still asserts the `requires` over the parameter's ENTRY value, so
        // `requires(x > 0) { x = 0 - 100; if x < 0 { return x; } }` "proves" `result > 0` from the entry
        // `x > 0` while the runtime returns -100 — a false certification laundered through composition. The
        // anti-launder guard above catches a param NAMED in the `ensures` TEXT; this catches one flowing
        // through the RETURN EXPRESSION. Fail closed (there is no `old()`; return a local instead).
        let mutated_params: BTreeSet<String> =
            param_names.intersection(&rebound).cloned().collect();
        if !mutated_params.is_empty() {
            for r in &early {
                let mut rv = BTreeSet::new();
                collect_expr_vars(r, &mut rv);
                if let Some(p) = rv.intersection(&mutated_params).next() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_CONTRACT_UNPROVABLE".into()),
                        message: format!(
                            "cannot verify a postcondition at an early `return` whose value depends on \
                             parameter `{p}`: it is reassigned in the body, but an early return is \
                             discharged against the call-entry precondition (there is no `old()`). \
                             Return a local computed after the reassignment instead."
                        ),
                        span: Some((span.start, span.end)),
                    });
                }
            }
        }
        for r in &early {
            push_ensures_obligations(ctx, ensures, r, &precondition_assumptions, span);
        }
    }

    ctx.symbols.extend(fn_symbols.clone());
    ctx.mir.push(MirBlock {
        function: qualified_name(module, name),
        mode: mode_name(mode).into(),
        statement_count: count_stmts(body),
        effects: effects.clone(),
    });
    ctx.hir.functions.push(HirFunction {
        name: name.into(),
        module: module.map(str::to_string),
        mode: mode_name(mode).into(),
        params: param_bindings,
        symbols: fn_symbols,
        effects,
        span: Some((span.start, span.end)),
    });
}

/// Restore the lexical binding scope after analyzing a block (`if`/`else`/loop body/etc.).
///
/// A block-scoped `let` (including a name that shadows an outer binding) must not leak past the
/// block: `let x = 5; if c { let x = taint(); } sink(x);` must see the OUTER clean `x`, not the
/// inner tainted one. Mirrors the snapshot/restore that `body_returns_taint` already does for the
/// interprocedural summary. This ONLY rewrites the `scope` map (BindingInfo / closure_arity); it
/// does NOT touch solver `assumptions` or `solver_int_vars` — those have their own snapshot path
/// via `drop_written_after_scope` / `havoc_loop_written` and must stay undisturbed here.
fn restore_block_scope(
    scope: &mut BTreeMap<String, ScopeBinding>,
    saved: &BTreeMap<String, ScopeBinding>,
) {
    *scope = saved.clone();
}

/// Control-flow-merge for the two flow labels — INTEGRITY (`info.tainted`, may-taint) and
/// CONFIDENTIALITY (`secret`, may-secret) — the sound closure of the reassignment fail-open. `scope`
/// has already been restored to the pre-block state (so block-local `let`s are correctly dropped);
/// this re-applies a label that any alternative PATH established on an outer binding. A variable
/// carries a label after the block iff it carries it on ANY analyzed path, and is clean only if clean
/// on EVERY path (so a value declassified/reset on all branches is precisely cleared). `paths` are the
/// post-analysis scopes of the alternatives — an `if`'s then/else, or a loop's zero-iteration
/// pre-state and its body. Without this, `restore_block_scope` discarded a branch or loop reassignment
/// to a tainted/secret value, and a later sink read the stale clean state and never fired.
fn merge_taint_over(
    scope: &mut BTreeMap<String, ScopeBinding>,
    paths: &[&BTreeMap<String, ScopeBinding>],
) {
    let names: Vec<String> = scope.keys().cloned().collect();
    for name in names {
        // Identity by span: only the SAME binding — a reassignment (`x = …`), which preserves the
        // binding's span — carries a label across the merge. A block-local `let x = …` SHADOW inside a
        // branch is a NEW binding (its own span) that the restore already dropped; its label must not
        // leak to the outer `x` (`let x = clean; if c { let x = taint(); } sink(x)` stays clean). The
        // adversarial rounds that deferred this slice were about exactly this shadow-vs-reassign case.
        let outer_span = scope.get(&name).and_then(|b| b.info.span);
        let mut tainted = false;
        let mut source: Option<String> = None;
        let mut secret = false;
        for p in paths {
            if let Some(pb) = p.get(&name) {
                if pb.info.span != outer_span {
                    continue;
                }
                if pb.info.tainted {
                    tainted = true;
                    source = source.or_else(|| pb.info.taint_source.clone());
                }
                if pb.secret {
                    secret = true;
                }
            }
        }
        if let Some(b) = scope.get_mut(&name) {
            b.info.tainted = tainted;
            if tainted {
                b.info.declassified = false;
                b.info.taint_source = b.info.taint_source.take().or(source);
            }
            // Confidentiality is the DUAL: a secret cleared on every path (e.g. declassified in both
            // branches) is precisely un-labelled; secret on any path re-secrets the outer binding.
            b.secret = secret;
        }
    }
}

/// Discharge a contracted call's PRECONDITIONS at the call site: for each `requires`, substitute the
/// actual arguments and push a `requires@{callee}` obligation the CALLER must prove — in whichever
/// solver lane (int QF_BV / float QF_FP / string QF_S) the substituted predicate is modelable. Returns
/// whether EVERY precondition was checkable; an unmodelable-arg precondition is skipped (returns false),
/// which the Let-position caller uses to gate assuming the callee's `ensures` (assuming a postcondition
/// whose precondition was NOT discharged would be an unsound false proof).
///
/// This is the call-site half of B2 composition. It closes a hunt-confirmed FALSE-ACCEPT class: a
/// callee's `requires` is ASSUMED inside its body (seeding its `assert`/`ensures` discharge), so without
/// a matching call-site obligation a violating call certified a runtime-trapping `assert` / a false
/// `ensures`. Before this, only INT preconditions at a Let-initializer emitted an obligation; string and
/// float preconditions (once those lanes became modelable) and calls in STATEMENT/Assign position
/// emitted none. Each lane mirrors its sibling ensures-site encoding (sort-partitioned assumptions,
/// mangled assertion vars). Callers with no contract, or an arity mismatch (handled elsewhere), are a
/// vacuous `true` — no obligation.
///
/// Discharges at ANY depth / position. This is sound because a branch guard is pushed as a SCOPED path
/// condition (`push_branch_path_condition`) into `assumptions` before the branch is analyzed, so an
/// obligation built inside `if a > 0 { g(a) }` sees `a > 0` and a guard-provable precondition proves while
/// a guard-unrelated violating one (`if bb { g(a) }`, `a` unconstrained by `bb`) yields a counterexample.
/// A MODELABLE precondition that cannot be PROVEN from the caller's premises + path condition rejects
/// (fail-closed) and NEVER falsely discharges. Two directions must be named precisely: a precondition
/// whose argument the solver cannot MODEL at all (an unmodeled var — a bare param of a contract-less
/// caller, a var whose model was dropped by reassignment/loop-havoc) emits NO obligation and thus defers
/// to runtime — that is fail-OPEN in the accept-biased default mode (the runtime `assert`/trap is the only
/// backstop), NOT fail-closed. A modeled-but-unprovable precondition is the fail-closed one. A CLOSED
/// (ground) precondition is the degenerate case with no free variable. (Earlier revisions depth-gated
/// this because branches lacked their guard; the path-condition machinery made the gate obsolete.) KNOWN
/// CONSERVATIVE BOUNDARY: a modeled arg under a NON-modelable guard (`if is_pos(a) { g(a) }`) pushes no
/// path condition, so the discharge fires unguarded and REJECTS though the opaque guard may hold at
/// runtime — sound (fail-closed), incomplete. RESIDUAL: this fires only on a DIRECT `Expr::Call` at a
/// statement position; a call NESTED in an argument / binary operand / `return`-arg (`h(g(x))`,
/// `print(g(x))`, `return g(x)`) or inside a `match`-arm / `if`-expression is NOT discharged (fail-open).
/// Flatten a top-level `&&` conjunction into its leaf conjuncts (`A && (B && C)` → `[A, B, C]`), so each
/// can discharge in its own lane. A non-`&&` expr yields itself. Used by discharge_call_requires so a
/// MIXED-lane conjunction (`s == "ok" && len(s) >= 2`) is not left unmodelable-as-a-whole (fail-open).
fn flatten_and_exprs(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Binary { op, lhs, rhs } if op == "&&" => {
            let mut v = flatten_and_exprs(lhs);
            v.extend(flatten_and_exprs(rhs));
            v
        }
        _ => vec![e],
    }
}

fn discharge_call_requires(
    ctx: &mut SemanticContext,
    assumptions: &[String],
    callee: &str,
    args: &[Expr],
) -> bool {
    let Some((pnames, creq, _cens)) = ctx.fn_contracts.get(callee).cloned() else {
        return true;
    };
    if pnames.len() != args.len() {
        return true;
    }
    let sub: BTreeMap<String, Expr> =
        pnames.iter().cloned().zip(args.iter().cloned()).collect();
    // CROSS-CALL shadow scope (review Finding: a caller-local shadow leaking into the callee's requires):
    // the callee's `requires` resolves its builtin tokens (`abs`/`min`/`max`/`len`/`contains`/…) in the
    // CALLEE's scope, not the caller's. A CALLER-LOCAL shadow — a param/`let` named `max` in THIS function —
    // must NOT suppress modeling of the callee's builtin (that dropped the obligation → a false accept).
    // Only a TOP-LEVEL (`all_fns`) shadow applies to both. So temporarily drop the caller-LOCAL-only shadow
    // marks for the discharge and restore them after (the caller's own body still sees them).
    let mut restore_int: Vec<&str> = vec![];
    let mut restore_str: Vec<&str> = vec![];
    for name in ["abs", "min", "max", "len"] {
        if !ctx.all_fns.contains(name) && ctx.solver_int_vars.remove(&shadow_builtin_mark(name)) {
            restore_int.push(name);
            if name == "len" {
                ctx.solver_string_vars.remove(&shadow_builtin_mark("len"));
            }
        }
    }
    for name in ["contains", "starts_with", "ends_with"] {
        if !ctx.all_fns.contains(name) && ctx.shadowed_string_preds.remove(name) {
            restore_str.push(name);
        }
    }
    // `index_of`/`substr` ride solver_string_vars (str.indexof/str.substr lanes), not shadowed_string_preds
    // — drop their caller-local-only shadows for the discharge too, so a caller param named `index_of` /
    // `substr` doesn't suppress the callee's builtin requires. (Parity with `len`, handled in the int loop.)
    let mut restore_str_builtin: Vec<&str> = vec![];
    for name in ["index_of", "substr"] {
        if !ctx.all_fns.contains(name)
            && ctx.solver_string_vars.remove(&shadow_builtin_mark(name))
        {
            restore_str_builtin.push(name);
        }
    }
    let mut all_requires_checkable = true;
    for req in &creq {
        let concrete = substitute_vars(req, &sub);
        // Decompose a top-level `&&`: a MIXED-lane conjunction (`s == "ok" && len(s) >= 2`) is neither
        // fully string-eq nor fully strlen modelable, so tested atomically it matched NO lane → skipped
        // (fail-open — a call `gs("no")` certified a runtime-trapping precondition). `A && B` at a call
        // site requires the caller to prove BOTH, so each conjunct discharges in its OWN lane (mirrors
        // seed_requires_fact). Homogeneous `&&` is unchanged (two obligations vs one conjunction — same
        // verdict); an unmodelable conjunct still sets all_requires_checkable=false.
        for clause in flatten_and_exprs(&concrete) {
        if is_bool_modelable(clause, &ctx.solver_int_vars) {
            let smt = expr_to_smt(clause, &ctx.symbolic_widths);
            let int_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| {
                    !fact_is_float(a, &ctx.solver_float_vars)
                        && !fact_is_string(a, &ctx.solver_string_vars)
                })
                .cloned()
                .collect();
            let mut vars = BTreeSet::new();
            collect_vars_from_smt(&smt, &mut vars);
            for a in int_asm.iter() {
                collect_vars_from_smt(a, &mut vars);
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("requires@{callee}:{smt}"),
                assumptions: int_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: false,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else if is_bool_modelable_float(clause, &ctx.solver_float_vars) {
            // QF_FP: mirror the float ensures-site — FLOAT-only assumptions, mangled assertion vars.
            let smt = float_bool_to_smt(clause);
            let float_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| fact_is_float(a, &ctx.solver_float_vars))
                .cloned()
                .collect();
            let mut raw = BTreeSet::new();
            collect_expr_vars(clause, &mut raw);
            let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
            for a in &float_asm {
                let mut avs = BTreeSet::new();
                collect_vars_from_smt(a, &mut avs);
                vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("requires@{callee}:{smt}"),
                assumptions: float_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: false,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else if is_bool_modelable_string(clause, &ctx.solver_string_vars, &ctx.shadowed_string_preds) {
            // QF_S: mirror the string ensures-site — STRING-only assumptions, `strings: true` sort tag so
            // a quoteless var-var body still routes to QF_S. The shared `string_expr_to_smt` under
            // `string_bool_to_smt` carries the load-bearing backslash escape.
            let smt = string_bool_to_smt(clause);
            let str_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| fact_is_string(a, &ctx.solver_string_vars))
                .cloned()
                .collect();
            let mut raw = BTreeSet::new();
            collect_expr_vars(clause, &mut raw);
            let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
            for a in &str_asm {
                let mut avs = BTreeSet::new();
                collect_vars_from_smt(a, &mut avs);
                vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("requires@{callee}:{smt}"),
                assumptions: str_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: true,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else if is_bool_modelable_strlen(clause, &ctx.solver_string_vars) {
            // Phase-3 str.len: discharge a string-LENGTH precondition at the call site (`h("ab")` against
            // `requires(len(s) >= 3)` → `(str.len "ab") = 2 < 3` → caught). COVERAGE-GATED: a referenced
            // string var with no seeded String-lane fact would make z3 disprove via the spurious `s = ""`
            // (the caller's justification is real but unseedable) — skip instead (fail-open, pre-lane). A
            // predicate fact (str.contains/prefixof/suffixof) is excluded — it does not tightly bound length
            // (see is_predicate_fact), so it must not spuriously "cover" this strlen obligation.
            let str_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| fact_is_string(a, &ctx.solver_string_vars) && !is_predicate_fact(a))
                .cloned()
                .collect();
            if strlen_vars_covered(clause, &str_asm, &ctx.solver_string_vars) {
                let smt = strlen_bool_to_smt(clause, &ctx.solver_string_vars);
                let mut raw = BTreeSet::new();
                collect_expr_vars(clause, &mut raw);
                let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
                for a in &str_asm {
                    let mut avs = BTreeSet::new();
                    collect_vars_from_smt(a, &mut avs);
                    vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
                }
                ctx.solver_obligations.push(SolverObligation {
                    name: format!("requires@{callee}:{smt}"),
                    assumptions: str_asm,
                    assertion: smt,
                    vars: vars.into_iter().collect(),
                    strings: true,
                    guard_assumptions: ctx.active_branch_guards.clone(),
                });
            } else {
                all_requires_checkable = false;
            }
        } else {
            all_requires_checkable = false;
        }
        }
    }
    // Restore the caller-local shadow marks dropped for this discharge.
    for name in restore_int {
        ctx.solver_int_vars.insert(shadow_builtin_mark(name));
        if name == "len" {
            ctx.solver_string_vars.insert(shadow_builtin_mark("len"));
        }
    }
    for name in restore_str {
        ctx.shadowed_string_preds.insert(name.to_string());
    }
    for name in restore_str_builtin {
        ctx.solver_string_vars.insert(shadow_builtin_mark(name));
    }
    all_requires_checkable
}

/// The SOUND path-condition fact a `match` arm's pattern contributes about the scrutinee, for discharging
/// contracted calls in the arm body/guard. A `Literal`/`StrLiteral` pattern means the arm runs iff the
/// scrutinee equals that value → `scrutinee == literal`; an `Or` of literals → the disjunction. Any other
/// pattern (wildcard, binding, list, struct, enum variant) contributes no simple equality — its
/// sub-values are unmodeled and encoding "the scrutinee is this variant" needs enum/pattern SMT modeling
/// that does not exist yet — so it returns None (the arm body is then discharged under the enclosing
/// assumptions only, which is sound: a subset of the true facts). The returned `Expr` is only USED if it
/// is `is_bool_modelable*` (a non-modelable scrutinee makes `push_branch_path_condition` a no-op), so a
/// bogus equality can never be assumed.
fn match_arm_pattern_fact(scrutinee: &Expr, pattern: &crate::frontend::Pattern) -> Option<Expr> {
    use crate::frontend::Pattern;
    let eq = |rhs: Expr| Expr::Binary {
        op: "==".to_string(),
        lhs: Box::new(scrutinee.clone()),
        rhs: Box::new(rhs),
    };
    match pattern {
        Pattern::Literal(l) => Some(eq(Expr::Literal(l.clone()))),
        Pattern::StrLiteral(s) => Some(eq(Expr::StrLiteral(s.clone()))),
        Pattern::Or(alts) => {
            let mut facts: Vec<Expr> = Vec::new();
            for p in alts {
                match p {
                    Pattern::Literal(l) => facts.push(eq(Expr::Literal(l.clone()))),
                    Pattern::StrLiteral(s) => facts.push(eq(Expr::StrLiteral(s.clone()))),
                    // A non-literal alternative (`1 | _`) makes the disjunction match-anything → no fact.
                    _ => return None,
                }
            }
            facts.into_iter().reduce(|a, b| Expr::Binary {
                op: "||".to_string(),
                lhs: Box::new(a),
                rhs: Box::new(b),
            })
        }
        _ => None,
    }
}

/// Process-unique counter for minting fresh SMT symbols for whole-value match bindings (see the
/// `Expr::Match` arm of `discharge_calls_in_expr`). Only used to guarantee distinct names; the value
/// never affects a verdict or reaches the compiled output, so a global counter is fixpoint-safe.
static MATCH_BIND_CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Rename a whole-value match binding `name` to a FRESH symbol `fresh` in an arm's guard/body, so its
/// obligations no longer collide with a shadowed outer var of the same mangled SMT name (`anb_<name>`).
/// SHADOW-AWARE: a nested `match` arm that REBINDS `name` keeps its own scope — its guard/body are left
/// with `name` intact (the inner binding is a different value, renamed by its own arm recursion); its
/// SCRUTINEE is still renamed (it is evaluated before the inner pattern binds, so `name` there is the
/// OUTER binding). Mirrors EXACTLY the positions `discharge_calls_in_expr` descends, so every renamed
/// reference is one the walker will encode; deferred forms (Block/Lambda/IfLet — not discharged) and
/// leaves are cloned untouched.
fn rename_binding(e: &Expr, name: &str, fresh: &str) -> Expr {
    let go = |x: &Expr| rename_binding(x, name, fresh);
    match e {
        Expr::Var(v) if v == name => Expr::Var(fresh.to_string()),
        Expr::Call { callee, args } => Expr::Call {
            callee: callee.clone(),
            args: args.iter().map(&go).collect(),
        },
        Expr::CallExpr { callee, args } => Expr::CallExpr {
            callee: Box::new(go(callee)),
            args: args.iter().map(&go).collect(),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: op.clone(),
            lhs: Box::new(go(lhs)),
            rhs: Box::new(go(rhs)),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: op.clone(),
            expr: Box::new(go(expr)),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(go(expr)),
            ty: ty.clone(),
        },
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(go(base)),
            index: Box::new(go(index)),
        },
        Expr::FieldAccess { base, field, span } => Expr::FieldAccess {
            base: Box::new(go(base)),
            field: field.clone(),
            span: *span,
        },
        Expr::ArrayLiteral { elements } => Expr::ArrayLiteral {
            elements: elements.iter().map(&go).collect(),
        },
        Expr::StructLiteral { name: n, fields, span } => Expr::StructLiteral {
            name: n.clone(),
            fields: fields.iter().map(|(k, v)| (k.clone(), Box::new(go(v)))).collect(),
            span: *span,
        },
        Expr::EnumConstruct { enum_name, variant, fields, field_names, span } => Expr::EnumConstruct {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            fields: fields.iter().map(&go).collect(),
            field_names: field_names.clone(),
            span: *span,
        },
        Expr::MapLiteral { entries, span } => Expr::MapLiteral {
            entries: entries.iter().map(|(k, v)| (go(k), go(v))).collect(),
            span: *span,
        },
        Expr::Tainted { ty, inner } => Expr::Tainted {
            ty: ty.clone(),
            inner: Box::new(go(inner)),
        },
        Expr::Declassify { inner, policy, reason } => Expr::Declassify {
            inner: Box::new(go(inner)),
            policy: policy.clone(),
            reason: reason.clone(),
        },
        Expr::Assume(i) => Expr::Assume(Box::new(go(i))),
        Expr::Assert(i) => Expr::Assert(Box::new(go(i))),
        Expr::Try(i) => Expr::Try(Box::new(go(i))),
        Expr::If { cond, then, else_, span } => Expr::If {
            cond: Box::new(go(cond)),
            then: Box::new(go(then)),
            else_: Box::new(go(else_)),
            span: *span,
        },
        Expr::Match { scrutinee, arms, span } => Expr::Match {
            scrutinee: Box::new(go(scrutinee)),
            arms: arms
                .iter()
                .map(|a| {
                    // A nested arm that rebinds `name` shadows it — leave its guard/body untouched
                    // (its own arm recursion renames the inner binding to a distinct fresh symbol).
                    if a.pattern.bound_names().iter().any(|n| n == name) {
                        a.clone()
                    } else {
                        crate::frontend::MatchArm {
                            pattern: a.pattern.clone(),
                            guard: a.guard.as_ref().map(&go),
                            body: go(&a.body),
                        }
                    }
                })
                .collect(),
            span: *span,
        },
        // Leaves + deferred forms (Literal/StrLiteral/Symbolic/TaintSource/UnifiedBuffer/RawPtr/Block/
        // Lambda/IfLet/Other and a non-matching Var): the walker never discharges inside them, so a
        // clone is sound (renaming there would be dead, and IfLet/Lambda/Block may rebind `name`).
        _ => e.clone(),
    }
}

/// Discharge the precondition of every contracted call reachable in `expr`, under the correct path
/// condition. `discharge_call_requires` alone fires only on a DIRECT `Expr::Call` at a statement position,
/// so a call NESTED in an argument / operand / cast / `return`-arg (`h(g(x))`, `print(g(x))`, `g(x) + 0`,
/// `return g(x)`) went unchecked — a reachable fail-open (the callee's `requires` is assumed in its body
/// yet nothing proved it at the call). This walk closes that. It recurses two ways:
///
/// - UNCONDITIONAL positions (evaluated whenever `expr` is) are recursed under the current `assumptions`:
///   call args, non-`&&`/`||` binary operands, unary/cast/index/field/array/struct/map/enum children, a
///   `?`/assert/assume/declassify inner, a `match` scrutinee.
/// - CONDITIONALLY-executed positions are recursed under their SCOPED PATH CONDITION (pushed via
///   `push_branch_path_condition`, then restored): the RHS of a short-circuiting `&&`/`||` (under the LHS:
///   `a && rhs` iff `a`, `a || rhs` iff `!a`), an `if`-EXPRESSION's branches (under `cond`/`!cond`), and a
///   `match` arm body/guard (under the literal-pattern / guard fact — see `match_arm_pattern_fact`). A
///   call proves exactly when the path condition establishes its precondition and is caught otherwise.
///
/// STILL DEFERRED (the residual): `if let`/block/lambda bodies, and a `match` arm whose bound sub-values
/// (enum/struct/list pattern) would constrain the call — those are unmodeled. Every discharged call is as
/// sound as a direct call: the same `assumptions` (including the pushed path condition) are in scope.
fn discharge_calls_in_expr(ctx: &mut SemanticContext, assumptions: &mut Vec<String>, expr: &Expr) {
    match expr {
        Expr::Call { callee, args } => {
            discharge_call_requires(ctx, assumptions, callee, args);
            for a in args {
                discharge_calls_in_expr(ctx, assumptions, a);
            }
        }
        Expr::CallExpr { callee, args } => {
            discharge_calls_in_expr(ctx, assumptions, callee);
            for a in args {
                discharge_calls_in_expr(ctx, assumptions, a);
            }
        }
        // `&&`/`||` short-circuit (run.rs:3027): the RHS runs only when the LHS does not decide — a
        // CONDITIONAL position. Recurse the LHS unconditionally; for the RHS, push the LHS as a SCOPED
        // path condition (`a && rhs` runs the rhs iff `a`; `a || rhs` iff `!a`) so a call there discharges
        // under exactly the condition that guards it — then restore. Every other binary operator evaluates
        // both operands unconditionally.
        Expr::Binary { op, lhs, rhs } => {
            discharge_calls_in_expr(ctx, assumptions, lhs);
            if op == "&&" || op == "||" {
                let snap = assumptions.len();
                let snap_g = ctx.active_branch_guards.len();
                push_branch_path_condition(ctx, assumptions, lhs, op == "||");
                discharge_calls_in_expr(ctx, assumptions, rhs);
                assumptions.truncate(snap);
                ctx.active_branch_guards.truncate(snap_g);
            } else {
                discharge_calls_in_expr(ctx, assumptions, rhs);
            }
        }
        // An `if`-EXPRESSION (`let z = if c { g(x) } else { h(y) };`): the condition is unconditional; each
        // branch runs under the (negated) condition — push it as a scoped path condition, exactly like the
        // `Stmt::If` handler, so a guard-provable branch call proves and a guard-unrelated one is caught.
        // A block-bodied branch (`{ let t = …; g(t) }`) recurses into a `Block` which is left deferred (its
        // statements need full analysis, not just call discharge), so only simple-expression branches are
        // reached here — no intra-branch reassignment can stale the pushed condition.
        Expr::If {
            cond, then, else_, ..
        } => {
            discharge_calls_in_expr(ctx, assumptions, cond);
            let snap = assumptions.len();
            let snap_g = ctx.active_branch_guards.len();
            push_branch_path_condition(ctx, assumptions, cond, false);
            discharge_calls_in_expr(ctx, assumptions, then);
            assumptions.truncate(snap);
            ctx.active_branch_guards.truncate(snap_g);
            push_branch_path_condition(ctx, assumptions, cond, true);
            discharge_calls_in_expr(ctx, assumptions, else_);
            assumptions.truncate(snap);
            ctx.active_branch_guards.truncate(snap_g);
        }
        // A `match` scrutinee is unconditionally evaluated — recurse it. Each ARM body runs only when its
        // pattern matches (and its guard holds), so it is discharged under a SCOPED path condition: a
        // literal/`|`-of-literals pattern over the scrutinee contributes `scrutinee == literal` (the arm
        // runs iff the scrutinee equals that value); an `if`-GUARD is assumed for the body (and its own
        // calls discharged under the pattern fact). A binding/wildcard/list/struct/enum-variant pattern
        // contributes no simple equality — its bound sub-values are unmodeled, so a call over them simply
        // defers (fail-open, the documented residual); a call over an ENCLOSING modeled var still
        // discharges under the enclosing assumptions. All facts are true when the arm runs, so this is as
        // sound as a branch guard; the snapshot/truncate keeps each arm's facts from leaking to a sibling.
        Expr::Match {
            scrutinee, arms, ..
        } => {
            discharge_calls_in_expr(ctx, assumptions, scrutinee);
            // Arms are ordered: arm k runs only if NO earlier arm matched. So each arm additionally assumes
            // the NEGATION of what every preceding arm's non-match guarantees — this is what makes the
            // `match a { 0 => …, _ => recip(a) }` and `match a { _ if a>0 => …, _ => h(a) }` idioms sound
            // (the fall-through arm proves `a != 0` / `a <= 0`). A preceding arm contributes a sound
            // fall-through fact (stored here, pushed NEGATED for later arms) only when its non-match is a
            // single modelable condition:
            //   - a GUARDLESS literal / or-literal pattern: non-match ⟺ `scrutinee != lit`.
            //   - a WILDCARD (`_`) GUARDED arm: the pattern always matches, so non-match ⟺ the GUARD was
            //     false → `!guard` (a `_` guard references only enclosing/scrutinee vars, so `!guard` is
            //     stable across later arms; a BINDING guard `n if G` is NOT — `G` may reference `n`, whose
            //     name means something else later — so it is excluded below).
            // A guarded LITERAL arm (non-match = pattern-false OR guard-false), a binding-guarded arm, and a
            // refutable pattern's non-match (unmodeled) yield no single fact — they contribute nothing (sound:
            // fewer premises).
            let mut prior_negated: Vec<Expr> = Vec::new();
            for arm in arms {
                let snap = assumptions.len();
                let snap_g = ctx.active_branch_guards.len();
                for f in &prior_negated {
                    push_branch_path_condition(ctx, assumptions, f, true);
                }
                let this_fact = match_arm_pattern_fact(scrutinee, &arm.pattern);
                if let Some(fact) = &this_fact {
                    push_branch_path_condition(ctx, assumptions, fact, false);
                }
                // A whole-value binding `match S { name => body }` binds `name = S` for this arm. Looking
                // `name` up by its mangled SMT symbol `anb_<name>` CONFLATES it with a shadowed outer var of
                // the same name, discharging a body call `g(name)` against the OUTER var's facts — a false
                // accept (`match b { a => g(a) }`, param `a > 50`, yet the binding is `b = 0`, so `g(0)`
                // traps). Fix: rename `name` to a FRESH symbol in the arm's guard/body and model the fresh
                // symbol against the SCRUTINEE. The outer `anb_<name>` and every assumption over it are left
                // UNTOUCHED — so a scrutinee-constraining fact like `requires(p == a)` survives (no filtering)
                // and `p == a > 50` still proves `g(a)`. When `S` is int-modelable, alias `fresh == S` (the
                // scrutinee's facts carry through — identity `match a { a => .. }` becomes `fresh == a`, which
                // keeps `a`'s own facts, so no identity special-case is needed). When `S` is a non-modelable
                // INT expr (a user call / cast / division), `fresh` is added UNCONSTRAINED so an INT obligation
                // over it FAILS CLOSED (rejects — the binding value is unknown), never fail-open. (A FLOAT or
                // STRING binding gets an int-lane `fresh` with no matching float/string membership, so a
                // float/string call over it is skipped — fail-OPEN, the pre-existing residual, unchanged.)
                // The fresh name is a process-unique counter FOLLOWED by letters (`<n>mbind`). It starts with
                // a digit, which an Anubis identifier cannot — so no user var mangles to the same `anb_<n>mbind`
                // symbol — while using only `[0-9A-Za-z_]`, the exact charset `collect_vars_from_smt`
                // tokenizes as one identifier (a `$` would be split, breaking the alias↔assertion var link).
                // The counter (not `assumptions.len()`) guarantees a nested rebind gets a DISTINCT symbol even
                // when the outer pushed no alias. The name never affects a verdict or reaches the compiled
                // output, so a global counter is fixpoint-safe. Only added state (`fresh` membership + its
                // alias fact) is introduced, so `truncate(snap)` + `solver_int_vars.remove(fresh)` is exact.
                let mut fresh_syms: Vec<String> = Vec::new();
                let mut renamed: Option<(Option<Expr>, Expr)> = None;
                if let crate::frontend::Pattern::Binding(name) = &arm.pattern {
                    // WHOLE-VALUE binding: fresh symbol ALIASED to the scrutinee when modelable (its facts
                    // carry through), else unconstrained (fail-closed). The width entry lets a NESTED alias
                    // chain encode (`match a { p => match p { d => g(d) } }` — the inner alias's scrutinee
                    // is the OUTER fresh symbol, and expr_to_smt_value needs its width; without it a valid
                    // chain was fail-closed over-rejected).
                    let fresh = format!(
                        "{}mbind",
                        MATCH_BIND_CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    );
                    ctx.solver_int_vars.insert(fresh.clone());
                    ctx.symbolic_widths.insert(fresh.clone(), 64);
                    if is_int_modelable(scrutinee, &ctx.solver_int_vars) {
                        if let Some(s) = expr_to_smt_value(scrutinee, &ctx.symbolic_widths) {
                            assumptions.push(format!("(= {} {})", smt_var(&fresh), s));
                        }
                    }
                    let g = arm.guard.as_ref().map(|x| rename_binding(x, name, &fresh));
                    let b = rename_binding(&arm.body, name, &fresh);
                    fresh_syms.push(fresh);
                    renamed = Some((g, b));
                } else {
                    // DESTRUCTURING pattern (enum/list/struct): a bound name that SHADOWS a modeled outer
                    // var would be discharged against the OUTER var's facts — the same conflation the
                    // whole-value fix closed (`match o { Opt::Some(a) => g(a) }` where the param `a > 50`
                    // "proved" g's requires while the binding is the PAYLOAD; check ACCEPTED, run TRAPPED).
                    // The payload value is unknowable (no enum/list element modeling), so each SHADOWING
                    // name is renamed to a fresh UNCONSTRAINED int symbol: the obligation fires with no
                    // facts and FAILS CLOSED. This is monotone-toward-reject on exactly the shadow class —
                    // a wrong-premise accept becomes a reject, a wrong-premise reject stays a reject, and a
                    // NON-shadowing bound name is untouched (unmodeled → fail-open, the documented
                    // residual), so no fail-open is introduced anywhere.
                    let shadowing: Vec<String> = arm
                        .pattern
                        .bound_names()
                        .into_iter()
                        .filter(|n| {
                            ctx.solver_int_vars.contains(n)
                                || ctx.solver_float_vars.contains(n)
                                || ctx.solver_string_vars.contains(n)
                        })
                        .collect();
                    if !shadowing.is_empty() {
                        let mut g = arm.guard.clone();
                        let mut b = arm.body.clone();
                        for n in &shadowing {
                            let fresh = format!(
                                "{}mbind",
                                MATCH_BIND_CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                            );
                            // The fresh symbol joins the SHADOWED var's OWN lane. An int-only fresh
                            // symbol would make a FLOAT/STRING callee requires match NO lane → skipped →
                            // fail-OPEN — flipping the pre-diff behavior (the obligation was built in the
                            // float/string lane over the OUTER var, whose contradicting premise rejected
                            // it): a review-caught reject→accept regression. A free QF_FP/QF_S symbol
                            // makes the obligation fire and FAIL CLOSED, matching the int lane exactly.
                            if ctx.solver_float_vars.contains(n) {
                                ctx.solver_float_vars.insert(fresh.clone());
                            } else if ctx.solver_string_vars.contains(n) {
                                ctx.solver_string_vars.insert(fresh.clone());
                                // Seed a TAUTOLOGY (`str.len >= 0` always holds) so strlen_vars_covered
                                // sees the fresh var MENTIONED in a String-lane fact. Without it the
                                // string-LENGTH sub-lane's coverage gate (built for the unseedable-
                                // justification over-rejection) SKIPS a `requires(len(s) >= k)` over this
                                // unconstrained symbol → fail-OPEN — flipping the pre-diff reject built
                                // over the covered OUTER var (a review-caught regression; the string-
                                // EQUALITY sub-lane is ungated and already failed closed). The tautology
                                // constrains nothing, so z3 still disproves via len=0 → FAIL-CLOSED,
                                // matching the int/float/string-eq lanes. Pushed after `snap` → removed
                                // by the arm's truncate (leak-free).
                                assumptions
                                    .push(format!("(>= (str.len {}) 0)", smt_var(&fresh)));
                            } else {
                                ctx.solver_int_vars.insert(fresh.clone());
                                ctx.symbolic_widths.insert(fresh.clone(), 64);
                            }
                            g = g.map(|x| rename_binding(&x, n, &fresh));
                            b = rename_binding(&b, n, &fresh);
                            fresh_syms.push(fresh);
                        }
                        renamed = Some((g, b));
                    }
                }
                let (eff_guard, eff_body) = match &renamed {
                    Some((g, b)) => (g.as_ref(), b),
                    None => (arm.guard.as_ref(), &arm.body),
                };
                if let Some(guard) = eff_guard {
                    discharge_calls_in_expr(ctx, assumptions, guard);
                    push_branch_path_condition(ctx, assumptions, guard, false);
                }
                discharge_calls_in_expr(ctx, assumptions, eff_body);
                assumptions.truncate(snap);
                ctx.active_branch_guards.truncate(snap_g);
                for fresh in &fresh_syms {
                    // A fresh symbol lives in exactly ONE lane (int XOR float XOR string) and its
                    // `<ctr>mbind` name is globally unique, so removing from all three is exact/leak-free.
                    ctx.solver_int_vars.remove(fresh);
                    ctx.solver_float_vars.remove(fresh);
                    ctx.solver_string_vars.remove(fresh);
                    ctx.symbolic_widths.remove(fresh);
                }
                // A `_`-guarded arm's guard references only enclosing/scrutinee vars, so `!guard` means the
                // same thing in a later arm. A BINDING-guarded arm (`n if G`) is excluded even though it is
                // also irrefutable: `G` may reference the binding `n` (which aliases the scrutinee), and `n`
                // denotes a DIFFERENT thing (or an enclosing shadowed var) in a later arm — pushing `!G`
                // there could assert a wrong fact. Fail-closed: a binding-guarded arm contributes nothing.
                let wildcard_guarded =
                    matches!(arm.pattern, crate::frontend::Pattern::Wildcard);
                match (&arm.guard, wildcard_guarded) {
                    // Guardless literal/or-literal → later arms have `scrutinee != lit`.
                    (None, _) => {
                        if let Some(fact) = this_fact {
                            prior_negated.push(fact);
                        }
                    }
                    // Wildcard + guarded → the pattern always matches, so non-match ⟺ `!guard`.
                    (Some(guard), true) => prior_negated.push(guard.clone()),
                    // A binding-guarded, or guarded-literal, or refutable arm: no sound single fact.
                    (Some(_), false) => {}
                }
            }
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => {
            discharge_calls_in_expr(ctx, assumptions, expr)
        }
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            discharge_calls_in_expr(ctx, assumptions, inner)
        }
        Expr::Assume(inner) | Expr::Assert(inner) | Expr::Try(inner) => {
            discharge_calls_in_expr(ctx, assumptions, inner)
        }
        Expr::Index { base, index } => {
            discharge_calls_in_expr(ctx, assumptions, base);
            discharge_calls_in_expr(ctx, assumptions, index);
        }
        Expr::FieldAccess { base, .. } => discharge_calls_in_expr(ctx, assumptions, base),
        Expr::ArrayLiteral { elements } | Expr::EnumConstruct { fields: elements, .. } => {
            for e in elements {
                discharge_calls_in_expr(ctx, assumptions, e);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                discharge_calls_in_expr(ctx, assumptions, v);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                discharge_calls_in_expr(ctx, assumptions, k);
                discharge_calls_in_expr(ctx, assumptions, v);
            }
        }
        // Remaining CONDITIONAL / deferred positions: `match`-ARM bodies (handled above via scrutinee-only),
        // `if let` branches, block/lambda bodies. Leaves (Var/Literal/Symbolic/…) hold no call. Undischarged.
        _ => {}
    }
}

/// Push a branch guard into the solver `assumptions` (and the scoped `ctx.active_branch_guards` stack) as
/// a SCOPED path condition. Inside `if cond { … }` the guard is a TRUE fact — the branch is taken iff
/// `cond` is runtime-true — so a guard-provable call/assert inside discharges (e.g. `if a > 0 { let y =
/// g(a); }` where g `requires(x > 0)`, previously over-rejected). The `else` branch pushes the NEGATION
/// (`negate = true`): the runtime else is taken iff `cond` is false. Each lane reuses its exact contract
/// encoder — `float_bool_to_smt` already models the runtime `if`-comparison branch-taken semantics (both
/// use `partial_cmp().unwrap_or(Equal)`, run.rs `anubis_value_cmp`), so the negation is the sound
/// complement. A non-modelable guard pushes nothing. The fact is dual-tracked: in `assumptions` (so
/// discharge sees it, and the frame sweep drops it on reassignment of a mentioned variable) and in
/// `ctx.active_branch_guards` (so the obligation records it in `guard_assumptions` and the VACUITY check
/// can exclude it — a dead branch whose guard contradicts the precondition is unreachable, not vacuous).
/// The caller scopes both: snapshot/restore `assumptions` and `active_branch_guards` around the branches.
fn push_branch_path_condition(
    ctx: &mut SemanticContext,
    assumptions: &mut Vec<String>,
    cond: &Expr,
    negate: bool,
) {
    let smt = if is_bool_modelable(cond, &ctx.solver_int_vars) {
        expr_to_smt(cond, &ctx.symbolic_widths)
    } else if is_bool_modelable_float(cond, &ctx.solver_float_vars) {
        float_bool_to_smt(cond)
    } else if is_bool_modelable_string(cond, &ctx.solver_string_vars, &ctx.shadowed_string_preds) {
        string_bool_to_smt(cond)
    } else if is_bool_modelable_strlen(cond, &ctx.solver_string_vars) {
        // Phase-3 str.len: a string-LENGTH branch guard (`if len(s) >= 3 { … }`) is a scoped path condition.
        strlen_bool_to_smt(cond, &ctx.solver_string_vars)
    } else {
        return;
    };
    let fact = if negate {
        format!("(not {smt})")
    } else {
        smt
    };
    assumptions.push(fact.clone());
    ctx.active_branch_guards.push(fact);
}

// Threads the full intraprocedural analysis state (scope, symbols, effects, solver assumptions, ctx);
// bundling into a struct would obscure the borrow pattern for no gain.
fn analyze_stmts(
    stmts: &[Stmt],
    mode: Mode,
    scope: &mut BTreeMap<String, ScopeBinding>,
    fn_symbols: &mut Vec<BindingInfo>,
    effects: &mut Vec<String>,
    assumptions: &mut Vec<String>,
    ctx: &mut SemanticContext,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                name,
                ty,
                init,
                span,
            } => {
                if mode == Mode::Safe && type_has_raw_pointer(ty.as_deref()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_RAW_POINTER_IN_SAFE".into()),
                        message: format!(
                            "safe mode raw pointer binding `{}` requires a research/exploit boundary",
                            name
                        ),
                        span: Some((span.start, span.end)),
                    });
                }

                // Generic-instantiation arity on a `let x: Pair<u32> = …` annotation (shadow-gated).
                if let Some(t) = ty {
                    check_generic_arity_annotation(t, Some((span.start, span.end)), ctx);
                }

                // Unknown-variable detection (covers `let y = x;` and simple `x + 1` cases). A bare
                // name is only unknown if it is neither a local binding, a user-defined function,
                // nor a stdlib builtin — named functions and builtins are first-class values and may
                // be bound by name (`let f = double;`), mirroring the unknown-*call* check below.
                fn note_unknown(v: &str, ctx: &mut SemanticContext) {
                    if !ctx.known_bindings.contains(v)
                        && !ctx.all_fns.contains(v)
                        && !crate::backends::run::is_builtin_name(v)
                    {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_UNKNOWN_VARIABLE".into()),
                            message: format!("unknown variable `{}`", v),
                            span: None,
                        });
                    }
                }
                match init {
                    Expr::Var(v) => note_unknown(v, ctx),
                    Expr::Binary { lhs, rhs, .. } => {
                        if let Expr::Var(v) = &**lhs {
                            note_unknown(v, ctx);
                        }
                        if let Expr::Var(v) = &**rhs {
                            note_unknown(v, ctx);
                        }
                    }
                    _ => {}
                }

                let init_taint =
                    expr_taint_source_m(init, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns);
                // CONFIDENTIALITY seed (dual of `init_taint`): the secret label the initializer carries.
                // `expr_secret_source`'s Declassify arm already clears a released value, so no separate
                // declassify gate is needed here (unlike the taint side's `declass_source`).
                let init_secret =
                    expr_secret_source_m(init, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns);
                let declass_source =
                    declassify_source(init, scope, &ctx.tainting_fns, &ctx.param_return_taint);
                // Effect inference must see calls in let-initializers (`let d = read_file(p)`),
                // not only bare expression statements — otherwise uses(...) checks miss real I/O.
                analyze_expr_effect(init, mode, scope, effects, ctx);
                // mark known after unknown check so later stmts see it
                ctx.known_bindings.insert(name.clone());

                if ty.is_some() {
                    ctx.annotated_vars.insert(name.clone());
                }
                // A+ type mismatch: annotation vs inferred init type.
                if let Some(t) = ty.as_deref() {
                    if let Some(got) = infer_expr_type_scoped(init, scope) {
                        if !types_assignable(t, &got) {
                            ctx.diagnostics.push(SemanticDiagnostic {
                                code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                message: format!("type mismatch: expected `{}`, got `{}`", t, got),
                                span: Some((span.start, span.end)),
                            });
                        }
                    } else if matches!(
                        init,
                        Expr::Call { .. } | Expr::Index { .. } | Expr::FieldAccess { .. } | Expr::Try(_)
                    ) {
                        // A `Call`/`Index`/`FieldAccess` return — and a `?` on one — is invisible to the
                        // flat `infer_expr_type_scoped` above (it returns `None` for all of them), so
                        // `let x: string = produce()` and `let x: string = produce()?` slipped through
                        // even though the callee's declared return type contradicts the annotation. Fall
                        // back to the InferEnv path (it carries `fn_ret_types`), exactly mirroring the
                        // return-position (`ANUBIS_RETURN_TYPE_MISMATCH`) and argument-position checks.
                        // Accept-biased: an unknown callee / `Any` / assignable type yields `None`.
                        if let Some(got) = check_mismatch_scoped(init, t, scope, ctx) {
                            ctx.diagnostics.push(SemanticDiagnostic {
                                code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                message: format!("type mismatch: expected `{}`, got `{}`", t, got),
                                span: Some((span.start, span.end)),
                            });
                        }
                    }
                }
                // A+ walk init for call-site types + match exhaustiveness.
                check_expr_semantics(init, scope, ctx);

                if let Some(source) = &declass_source {
                    ctx.taint_traces.push(TaintTrace {
                        source: source.clone(),
                        sink: None,
                        steps: vec![format!("{} -> declassify -> {}", source, name)],
                        declassified: true,
                    });
                    effects.push("declassify".into());
                }

                let explicit_taint = is_tainted_type(ty.as_deref());
                // A `secret<T>` let annotation auto-labels the binding as secret — the dual of
                // `explicit_taint` — so `let k: secret<u64> = getenv("K")` is secret without a
                // `secret_source(..)` wrapper.
                let explicit_secret = is_secret_type(ty.as_deref());
                let tainted = explicit_taint || (init_taint.is_some() && declass_source.is_none());
                let taint_source = if explicit_taint {
                    Some(name.clone())
                } else {
                    init_taint.clone()
                };
                let info = BindingInfo {
                    name: name.clone(),
                    ty: ty.clone().or_else(|| infer_expr_type_scoped(init, scope)),
                    mode: mode_name(mode).into(),
                    tainted,
                    taint_source: taint_source.clone(),
                    declassified: declass_source.is_some(),
                    span: Some((span.start, span.end)),
                };
                if explicit_taint {
                    ctx.taint_labels.push(format!(
                        "{}: {}",
                        name,
                        ty.clone().unwrap_or_else(|| "tainted<unknown>".into())
                    ));
                    effects.push("taint-source".into());
                } else if let Some(source) = &taint_source {
                    ctx.taint_labels
                        .push(format!("{}: derived_from {}", name, source));
                    effects.push("taint-propagate".into());
                }
                let ca = closure_arity_of(init, scope, ctx);
                scope.insert(
                    name.clone(),
                    ScopeBinding {
                        info: info.clone(),
                        closure_arity: ca,
                        secret: init_secret.is_some() || explicit_secret,
                    },
                );
                fn_symbols.push(info);

                // Record width for solver per-var BV
                let w = if let Some(t) = &ty {
                    bitwidth_of(t)
                } else if let Expr::Symbolic { ty } = init {
                    bitwidth_of(ty)
                } else if let Expr::Binary { lhs, .. } = init {
                    // infer from lhs var width if known
                    if let Expr::Var(lv) = &**lhs {
                        *ctx.symbolic_widths.get(lv).unwrap_or(&32u32)
                    } else {
                        32u32
                    }
                } else {
                    32u32
                };
                // A `let` that SHADOWS an existing binding invalidates the old one's solver state:
                // drop its modelability and any stale fact, so an integer predicate over the NEW
                // binding (which may hold a string/list/bool) is not "proved" against the shadowed
                // integer's model (e.g. `let v = 0; let v = "hi"; assert(v + 0 == v)`).
                clear_binding_modelability(&mut ctx.solver_int_vars, name);
                // Phase-3 QF_FP: a shadowing `let` also drops the old binding's FLOAT modelability; its
                // stale float defining-fact is dropped by the sort-agnostic assumptions.retain below.
                ctx.solver_float_vars.remove(name);
                // Phase-3 QF_S: same parity for a shadowed STRING binding — drop its modelability so a
                // string predicate over the NEW binding is never proved against the shadowed string's
                // fact (which the sort-agnostic retain below also evicts).
                ctx.solver_string_vars.remove(name);
                {
                    // Also drop stale array/len symbols for a shadowed sequence binding.
                    let mangled = smt_var(name);
                    let arr = seq_arr_smt(name);
                    let len = seq_len_smt(name);
                    assumptions.retain(|a| {
                        let mut vs = BTreeSet::new();
                        collect_vars_from_smt(a, &mut vs);
                        !vs.contains(&mangled) && !vs.contains(&arr) && !vs.contains(&len)
                    });
                }
                // Phase-3 QF_S: a genuinely-string `let` must be kept OUT of `symbolic_widths` — string
                // PARAMS are excluded from the width map for exactly this reason: `expr_to_smt_value`
                // gates a Var on width membership, so a width entry would let it build a BIT-VECTOR fact
                // over a String-sorted symbol. `+` is runtime string concat, so a width entry on `u` would
                // let `expr_to_smt_value` push `(= anb_u (bvadd anb_s anb_s))`, which `fact_is_string` then
                // routes into a QF_S obligation → bvadd on String sorts → z3 error → fail-closed
                // over-rejection of an unrelated valid string contract. REMOVE (not merely skip) so a stale
                // width from a shadowed integer binding of the same name is evicted too. Concat IS now
                // modeled (is_string_modelable has a Binary `+` arm) — via `string_expr_to_smt`'s
                // `(str.++ …)` on the String-sorted symbols, NOT a bit-vector; this eviction is exactly what
                // keeps the concat def-fact in the String lane.
                let genuinely_string = !is_int_modelable(init, &ctx.solver_int_vars)
                    && is_string_modelable(init, &ctx.solver_string_vars);
                if genuinely_string {
                    ctx.symbolic_widths.remove(name);
                } else {
                    ctx.symbolic_widths.insert(name.clone(), w);
                }

                // Track whether this binding is genuinely integer-modelable for the solver: a
                // `symbolic()` source, or an integer-arithmetic init over already-modelable vars.
                // String/bool/list lets are excluded, so an assertion over them is never
                // (unsoundly) "disproved" by a fabricated bit-vector counterexample.
                if matches!(init, Expr::Symbolic { .. })
                    || is_int_modelable(init, &ctx.solver_int_vars)
                {
                    ctx.solver_int_vars.insert(name.clone());
                }

                // Phase-4 A2: a list literal of int-modelable elements is a *bounded* sequence —
                // model as QF_ABV array + fixed length. Unbounded lists (params, push results, …)
                // stay unmodeled (contracts fail with ANUBIS_SEQ_UNBOUNDED).
                if let Expr::ArrayLiteral { elements } = init {
                    if elements
                        .iter()
                        .all(|e| is_int_modelable(e, &ctx.solver_int_vars))
                    {
                        let n = elements.len() as u64;
                        ctx.solver_int_vars.insert(seq_mark(name));
                        ctx.solver_int_vars.insert(seq_len_mark(name, n));
                        let arr = seq_arr_smt(name);
                        let len = seq_len_smt(name);
                        let len_fact = format!("(= {len} (_ bv{n} 64))");
                        assumptions.push(len_fact.clone());
                        ctx.symbolic_defs.push(len_fact);
                        for (i, el) in elements.iter().enumerate() {
                            if let Some(es) = expr_to_smt_value(el, &ctx.symbolic_widths) {
                                let cell = format!("(= (select {arr} (_ bv{i} 64)) {es})");
                                assumptions.push(cell.clone());
                                ctx.symbolic_defs.push(cell);
                            }
                        }
                    }
                }

                // A `let` is EITHER an integer/bit-vector binding OR a Float64 one, never both. A bare
                // numeric literal (`let x = 5`) is int-modelable AND parses as a finite f64, so the
                // integer lane must claim it (its runtime value is `Int`, and int→float widening is
                // runtime-inert). "Genuinely float" = float-modelable but NOT int-modelable — a float
                // param/let arithmetic or a decimal literal. Computing it once keeps the two lanes
                // disjoint: without the `!genuinely_float` gate on the integer push below, a float
                // `let y = a * a` (with float `a` in `symbolic_widths`) would inject a spurious
                // `(= anb_y (bvmul anb_a anb_a))` that `fact_is_float` then keeps in the QF_FP obligation
                // (both vars float) → a `bvmul` on Float64 sorts → z3 error → fail-closed over-rejection.
                let genuinely_float = !is_int_modelable(init, &ctx.solver_int_vars)
                    && is_float_modelable(init, &ctx.solver_float_vars);

                // For solver faithfulness: concrete integer lets become path assumptions.
                // Symbolic sources remain unconstrained until assume()/assert() shape them.
                // (Array literals are handled above via select/len facts — do not also bind the
                // list name as a BitVec, which would be an unsound sort. A genuinely-float let is handled
                // by the QF_FP branch below, never as a bit-vector; a genuinely-string let by the QF_S
                // branch — the three lanes are mutually exclusive by construction.)
                if !matches!(init, Expr::ArrayLiteral { .. }) && !genuinely_float && !genuinely_string {
                    if let Some(init_smt) = expr_to_smt_value(init, &ctx.symbolic_widths) {
                        let def_smt = format!("(= {} {})", smt_var(name), init_smt);
                        ctx.symbolic_defs.push(def_smt.clone());
                        ctx.constraints.push(format!("(assert {})", def_smt));
                        assumptions.push(def_smt); // so it is included in subsequent obligations
                    }
                }
                // Phase-3 QF_FP: a genuinely-float `let` that is NEVER REASSIGNED becomes a Float64
                // DEFINING FACT in the SAME `assumptions` channel, so a later float `ensures`/`assert`
                // chains on it (`let y = x * 2.0; ensures(result < 4.0)`). Riding `assumptions` means the
                // existing var-name-based frame sweep (drop_written_after_scope / havoc_loop_written) drops
                // it when `y` is written under a `Stmt::If`/`While`/… scope. The `reassigned_roots` gate is
                // the SOUNDNESS boundary: the statement-level sweep does NOT visit a write EMBEDDED in a
                // `match`-arm / `if`-expression / block (those are analyzed as expressions, not restatemented
                // via analyze_stmts), so a fact for a variable reassigned in such a position could leak
                // unconditionally and certify a violable contract. Admitting only never-reassigned lets makes
                // the defining-fact valid on every path — no leak is possible. `fact_is_float` sort-partitions
                // the fact out of every integer obligation; STRUCTURAL `=` (not fp.eq, which is IEEE
                // NaN≠NaN); NOT pushed to ctx.constraints/symbolic_defs (kept byte-identical for the
                // self-host projection).
                if genuinely_float && !ctx.reassigned_roots.contains(name) {
                    ctx.solver_float_vars.insert(name.clone());
                    assumptions.push(format!("(= {} {})", smt_var(name), float_expr_to_smt(init)));
                }
                // Phase-3 QF_S: a genuinely-string `let` that is NEVER REASSIGNED becomes a String
                // DEFINING FACT in the same `assumptions` channel, so a later string `ensures`/`assert`
                // chains on it (`let s = "ok"; return s;` proves `ensures(result == "ok")`, incl. depth-2
                // alias chains `let t = s`). The exact mirror of the float branch above: the same
                // `reassigned_roots` gate is the soundness boundary (a write embedded in a `match`-arm /
                // `if`-expression escapes the statement-level frame sweep, so only never-reassigned lets
                // may carry an unconditional fact — no leak is possible by construction); riding
                // `assumptions` means the var-name-based frame sweeps drop the fact when the binding is
                // written under a `Stmt::If`/`While` scope; `fact_is_string` sort-partitions it out of
                // every integer obligation. The shared `string_expr_to_smt` encoder carries the
                // LOAD-BEARING backslash escape (every `\` → `\u{5c}`), so a `\u{..}`-shaped literal
                // cannot be re-decoded by z3 into a false accept. NOT pushed to
                // ctx.constraints/symbolic_defs (kept byte-identical for the self-host projection).
                if genuinely_string && !ctx.reassigned_roots.contains(name) {
                    ctx.solver_string_vars.insert(name.clone());
                    assumptions.push(format!("(= {} {})", smt_var(name), string_expr_to_smt(init)));
                }
                // Struct-literal LET field modeling — the body twin of the has_contract struct-PARAM path:
                // `let p = P{f: v, ...}` (p neither reassigned nor SHADOWED) registers each scalar modelable
                // field's canonical `mangle_field` symbol and seeds its defining fact `(= mangle(p,f) v)`, so
                // a later assert/ensures over `p.field` is checked instead of fail-opening. Riding
                // `assumptions` + the `reassigned_roots` gate mirrors the float/string let branches above; the
                // `shadowed_lets` gate additionally fail-opens a re-bound base whose mangled per-field symbol
                // the scalar shadow-clear cannot evict (incl. an expression-position match/if-let shadow). A
                // param of the same name is mutually exclusive: slice-19 registration skips a let-shadowed
                // param (its gate includes collect_let_bound), so no conflicting fact can coexist. Only a
                // field whose value ENCODES is registered (else it declines → fail-open, no free-var strand).
                if let Expr::StructLiteral { name: sname, fields, .. } = init {
                    if !ctx.reassigned_roots.contains(name) && !ctx.shadowed_lets.contains(name) {
                        if let Some(field_tys) = ctx.struct_fields.get(sname).cloned() {
                            for (fname, fexpr) in fields {
                                let Some(fty) = field_tys.get(fname) else {
                                    continue;
                                };
                                // VALUE-STABILITY gate (review Finding 1 — the stranded-field over-reject):
                                // the field DEFINING fact `(= sym <smt(fexpr)>)` rides `assumptions`, so a
                                // frame sweep drops it when a variable it MENTIONS is written — but the
                                // mangled field symbol stays registered, leaving a FREE var that makes a
                                // valid `assert(p.f == ..)` unprovable → over-rejection (`let p = P{v:x}; x =
                                // 10; assert(p.v == 5)` runs clean but check rejected). A struct field copies
                                // its value AT CONSTRUCTION (value semantics), so the fact is faithful only
                                // if it can never be dropped: register ONLY when no variable in the value
                                // expr is reassigned or shadowed anywhere in the body (a literal, or a value
                                // over stable vars). Otherwise fail-open (unmodeled), exactly as pre-slice.
                                let mut fvars = BTreeSet::new();
                                collect_expr_vars(fexpr, &mut fvars);
                                if fvars.iter().any(|v| {
                                    ctx.reassigned_roots.contains(v) || ctx.shadowed_lets.contains(v)
                                }) {
                                    continue;
                                }
                                let sym = mangle_field(name, fname);
                                if is_integer_ty(fty)
                                    && is_int_modelable(fexpr, &ctx.solver_int_vars)
                                {
                                    if let Some(vs) = expr_to_smt_value(fexpr, &ctx.symbolic_widths) {
                                        ctx.solver_int_vars.insert(sym.clone());
                                        ctx.symbolic_widths.insert(sym.clone(), 64);
                                        assumptions.push(format!("(= {} {})", smt_var(&sym), vs));
                                    }
                                } else if is_float_ty(fty)
                                    && is_float_modelable(fexpr, &ctx.solver_float_vars)
                                {
                                    ctx.solver_float_vars.insert(sym.clone());
                                    assumptions.push(format!(
                                        "(= {} {})",
                                        smt_var(&sym),
                                        float_expr_to_smt(fexpr)
                                    ));
                                } else if fty.as_str() == "string"
                                    && is_string_modelable(fexpr, &ctx.solver_string_vars)
                                {
                                    ctx.solver_string_vars.insert(sym.clone());
                                    assumptions.push(format!(
                                        "(= {} {})",
                                        smt_var(&sym),
                                        string_expr_to_smt(fexpr)
                                    ));
                                }
                            }
                        }
                    }
                }
                // A `let` INITIALIZER can hide a write to an OUTER variable in an `if`/`match`/block
                // expression (`let z = if c { y = 100; 0 } else { 0 };`). That write escapes the
                // statement-level frame sweep, so invalidate the written variables' stale facts here.
                invalidate_embedded_writes(ctx, assumptions, init);
                // NOTE: a symbolic input's `u8`/`u32` type annotation is NOT turned into a
                // [0, 2^w-1] range assumption — the annotation is runtime-inert, so assuming a range
                // the runtime does not enforce would be unsound (it would let the solver "prove"
                // overflow-free facts that the i64 runtime violates). The value is modeled as an
                // unconstrained i64.

                // A non-call initializer (`let z = g(x) + 1;`, `let z = arr[g(i)];`) can still contain
                // contracted calls in unconditional positions — discharge them. The top-level Call case is
                // handled by the B2 block below (which also binds the ensures), so it's excluded here to
                // avoid a double obligation.
                if !matches!(init, Expr::Call { .. }) {
                    discharge_calls_in_expr(ctx, assumptions, init);
                }
                // B2 composition: when the initializer calls a CONTRACTED function, specialize the
                // callee's contract to this call — ASSERT its precondition (the caller must satisfy
                // it) and ASSUME its postcondition with `result` bound to this variable, so a later
                // assertion can rely on it. This is how one function's `ensures` satisfies the next.
                if let Expr::Call { callee, args } = init {
                    // ASSERT each precondition in its lane (the guard, if any, is in scope as a path
                    // condition, so a variable-arg call in a branch discharges soundly).
                    let all_requires_checkable =
                        discharge_call_requires(ctx, assumptions, callee, args);
                    // Nested calls in the ARGUMENTS (`let z = f(g(x));`) are unconditionally evaluated —
                    // discharge them too; the top-level call is handled directly above (no double-count).
                    for a in args {
                        discharge_calls_in_expr(ctx, assumptions, a);
                    }
                    if let Some((pnames, _creq, cens)) = ctx.fn_contracts.get(callee).cloned() {
                        if pnames.len() == args.len() {
                            let mut sub: BTreeMap<String, Expr> =
                                pnames.iter().cloned().zip(args.iter().cloned()).collect();
                            // ASSUME the postcondition ONLY when every precondition was verifiable:
                            // the ensures holds only under the precondition, so assuming it when a
                            // `requires` was SKIPPED (a dynamic/unmodelable argument) would be an
                            // unsound false proof (the caller could be violating the precondition).
                            // Model this binding as a solver integer ONLY if the callee DECLARES an
                            // integer return type. Return types are inert at runtime, so a `-> u32`
                            // body is separately runtime-guarded (anubis_require_int_ret); but a
                            // `-> f64` callee must NOT seed a float into the integer domain here (its
                            // `ensures` may not even mention `result`, leaving the binding unconstrained
                            // yet modeled as i64 — a certified-false cast/bitwise identity at runtime).
                            let callee_returns_int = ctx
                                .fn_ret_types
                                .get(callee)
                                .map(|t| is_integer_ty(t))
                                .unwrap_or(false);
                            if !cens.is_empty() && all_requires_checkable && callee_returns_int {
                                // The callee guarantees an integer postcondition about its result,
                                // so this binding is solver-modelable.
                                ctx.solver_int_vars.insert(name.clone());
                                sub.insert("result".to_string(), Expr::Var(name.clone()));
                                for ens in &cens {
                                    let concrete = substitute_vars(ens, &sub);
                                    if is_bool_modelable(&concrete, &ctx.solver_int_vars) {
                                        let smt = expr_to_smt(&concrete, &ctx.symbolic_widths);
                                        ctx.constraints.push(format!("(assert {})", smt));
                                        assumptions.push(smt);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Stmt::LetPattern { pattern, init, span } => {
                // Destructuring binding: register each bound name so later statements don't
                // flag it as unknown. (No type annotation, so no raw-pointer/type-mismatch check.)
                for n in pattern.bound_names() {
                    ctx.known_bindings.insert(n);
                }
                // Solver soundness: this destructuring REBINDS (shadows) each bound name, so drop the
                // prior binding's stale solver fact/membership — exactly as `Stmt::Let` does on a shadow.
                // Without this a leftover `(= anb_s "A")` from an outer `let s = "A"` certifies a contract
                // over the NEW `s` (hunt-found false-accept `let s="A"; let [s]=["B"]; ensures(result=="A")`
                // returns "B" yet PASSED). The new binder is left unmodeled (fail-closed).
                for n in pattern.bound_names() {
                    invalidate_binding_facts(ctx, assumptions, &n);
                }
                // As with `Stmt::Let`, a destructuring initializer can hide an embedded write to an
                // outer variable; invalidate its stale solver fact.
                invalidate_embedded_writes(ctx, assumptions, init);
                // The initializer is evaluated UNCONDITIONALLY (like a plain `let`), so discharge a
                // contracted call in it (`let [p, q] = [g(a), 0];`).
                discharge_calls_in_expr(ctx, assumptions, init);
                // #69: SEED taint/secret from the initializer. Before this the arm labelled nothing
                // (not even inserting the bound names into `scope`), so a destructured secret/tainted
                // source laundered: `let [a,b] = [secret_source("k"),0]; send(a)` and the direct-source
                // `let [a,b] = [input(),0]; sink(a)` COMPILED — `a` was absent from scope and read clean
                // at egress. Compute the initializer's WHOLE-VALUE label once (conservative whole-value
                // granularity, matching seed_taint_pattern / the value-block walker's LetPattern arm) and
                // seed EVERY bound name with it. A well-formed declassify releases (taint cleared here;
                // secret cleared by the walker's own Declassify arm → None). Whole-value over-approx: a
                // clean sibling of a labelled destructure is conservatively labelled too (fail-closed).
                let taint = {
                    let it = expr_taint_source_m(
                        init,
                        scope,
                        &ctx.tainting_fns,
                        &ctx.param_return_taint,
                        &ctx.method_tainting_fns,
                    );
                    let declassified =
                        declassify_source(init, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                            .is_some();
                    if declassified {
                        None
                    } else {
                        it
                    }
                };
                let secret = expr_secret_source_m(
                    init,
                    scope,
                    &ctx.secret_fns,
                    &ctx.param_return_taint,
                    &ctx.method_secret_fns,
                )
                .is_some();
                seed_effect_pattern(scope, pattern, &taint, secret);
                // Patch each bound name's span to the real `let`-pattern span. `seed_effect_pattern`
                // inserts `span: None`, but `merge_taint_over` disambiguates a branch SHADOW from a
                // reassignment by span identity — so without a real, distinct span a statement-position
                // destructure shadow (`let [r,s]=[0,1]; if c { let [r,s]=[secret_source(),1]; } send(r)`)
                // would collide `None == None` with its outer binding and leak the shadow's label onto the
                // provably-clean outer `r` (a false positive). This mirrors the single-`let` arm and the
                // value-block walker's LetPattern arms, which patch the span for exactly this reason.
                for n in pattern.bound_names() {
                    if let Some(b) = scope.get_mut(&n) {
                        b.info.span = Some((span.start, span.end));
                    }
                }
            }
            Stmt::ResearchBlock { body, .. } => {
                ctx.has_research = true;
                effects.push("research-boundary".into());
                // Lexical block: a `let` inside `@research { ... }` must not escape.
                let snap_scope = scope.clone();
                analyze_stmts(
                    body,
                    Mode::Research,
                    scope,
                    fn_symbols,
                    effects,
                    assumptions,
                    ctx
                );
                restore_block_scope(scope, &snap_scope);
            }
            Stmt::ExploitBlock { body, .. } => {
                ctx.has_research = true;
                effects.push("exploit-boundary".into());
                let snap_scope = scope.clone();
                analyze_stmts(
                    body,
                    Mode::Exploit,
                    scope,
                    fn_symbols,
                    effects,
                    assumptions,
                    ctx
                );
                restore_block_scope(scope, &snap_scope);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                effects.push("hybrid".into());
                for block in [gpu, cpu, prove].into_iter().flatten() {
                    let snap_scope = scope.clone();
                    analyze_stmts(block, mode, scope, fn_symbols, effects, assumptions, ctx);
                    restore_block_scope(scope, &snap_scope);
                }
            }
            Stmt::ExprStmt(Expr::Assume(expr)) => {
                // A contracted call inside the assumed expression (`assume(g(x) == 0)`) is evaluated —
                // discharge its precondition (unconditional positions of `expr`).
                discharge_calls_in_expr(ctx, assumptions, expr);
                // Only ASSUME what the solver can model SOUNDLY (mirrors the assert handler below). An
                // unmodelable assumption — e.g. `assume((x as u8) == 0)`, whose truncating cast has no
                // sound i64 identity — would otherwise be lowered as if `x == 0` and let the solver
                // certify a violated contract (`ensures(result == 0)` while f(256) returns 256). An
                // unmodelable assume is still enforced at runtime (anubis_assume), just not trusted here.
                if is_bool_modelable(expr, &ctx.solver_int_vars) {
                    let smt = expr_to_smt(expr, &ctx.symbolic_widths);
                    assumptions.push(smt.clone());
                    ctx.constraints.push(format!("(assert {})", smt));
                }
                // A write embedded in the assumed expression escapes the statement sweep; invalidate it.
                invalidate_embedded_writes(ctx, assumptions, expr);
                effects.push("assume".into());
            }
            Stmt::ExprStmt(Expr::Assert(expr)) => {
                // Walk the assertion expression for effect/taint/crypto-misuse (RWC Ch3: HMAC == tag).
                analyze_expr_effect(expr, mode, scope, effects, ctx);
                // A contracted call inside the asserted expression (`assert(g(x) > 0)`) is evaluated —
                // discharge its precondition (unconditional positions of `expr`).
                discharge_calls_in_expr(ctx, assumptions, expr);
                // Only discharge an assertion the solver can soundly model in QF_BV (a boolean
                // formula over integer-modelable terms). A bare bool var, a string comparison, or
                // any other value is left to the runtime `assert` — the checker must not fabricate
                // a bit-vector counterexample and "disprove" a statement it cannot faithfully model
                // (that would make `check` unsound, e.g. disproving `assert(true)`).
                if is_bool_modelable(expr, &ctx.solver_int_vars) {
                    let smt = expr_to_smt(expr, &ctx.symbolic_widths);
                    ctx.constraints.push(format!("(assert {})", smt));
                    // Sort-partition: an integer (QF_BV) obligation assumes only integer facts. Dropping
                    // any float fact keeps a `fp.`/`to_fp` token out of the body (which would flip the
                    // query to QF_FP and mis-declare these i64 symbols as Float64). Corpus-inert: with no
                    // float vars in scope `fact_is_float` is always false, so this is `assumptions.clone()`.
                    let int_asm: Vec<String> = assumptions
                        .iter()
                        .filter(|a| {
                    !fact_is_float(a, &ctx.solver_float_vars)
                        && !fact_is_string(a, &ctx.solver_string_vars)
                })
                        .cloned()
                        .collect();
                    let mut vars = BTreeSet::new();
                    collect_vars_from_smt(&smt, &mut vars);
                    for assumption in &int_asm {
                        collect_vars_from_smt(assumption, &mut vars);
                    }
                    ctx.solver_obligations.push(SolverObligation {
                        name: format!("assert:{}", smt),
                        assumptions: int_asm,
                        assertion: smt,
                        vars: vars.into_iter().collect(),
                        strings: false,
                        guard_assumptions: ctx.active_branch_guards.clone(),
                    });
                } else if is_bool_modelable_float(expr, &ctx.solver_float_vars) {
                    // Phase-3 QF_FP: a float `assert` over the modelable subset is discharged in QF_FP.
                    // The shared `assumptions` channel is sort-partitioned — this obligation assumes only
                    // FLOAT facts (float `requires`, float `let`/reassignment defining-facts), so its body
                    // is all-Float64 and never sort-clashes with an integer bit-vector fact. Assertion
                    // vars come from the Expr (mangled); each consumed float fact's vars are scraped too
                    // (anb_-prefixed only, so the `fp.`/`RNE`/`to_fp` operator tokens are not mistaken for
                    // symbols) and declared, so a chained `let y = x * 2.0` fact is fully constrained.
                    let smt = float_bool_to_smt(expr);
                    let float_asm: Vec<String> = assumptions
                        .iter()
                        .filter(|a| fact_is_float(a, &ctx.solver_float_vars))
                        .cloned()
                        .collect();
                    let mut raw = BTreeSet::new();
                    collect_expr_vars(expr, &mut raw);
                    let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
                    for a in &float_asm {
                        let mut avs = BTreeSet::new();
                        collect_vars_from_smt(a, &mut avs);
                        vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
                    }
                    ctx.solver_obligations.push(SolverObligation {
                        name: format!("assert:{smt}"),
                        assumptions: float_asm,
                        assertion: smt,
                        vars: vars.into_iter().collect(),
                        strings: false,
                        guard_assumptions: ctx.active_branch_guards.clone(),
                    });
                } else if is_bool_modelable_string(expr, &ctx.solver_string_vars, &ctx.shadowed_string_preds) {
                    // Phase-3 QF_S: a string-equality/predicate `assert` is discharged in QF_S. The shared
                    // `assumptions` channel is sort-partitioned to STRING facts (a string `requires`), so
                    // the body is all-`String` and never sort-clashes with a bit-vector/float fact.
                    let smt = string_bool_to_smt(expr);
                    let str_asm: Vec<String> = assumptions
                        .iter()
                        .filter(|a| fact_is_string(a, &ctx.solver_string_vars))
                        .cloned()
                        .collect();
                    // COVERAGE-GATE a PURE-predicate assert (`contains`/`starts_with`/`ends_with`, no `==`):
                    // it is a WEAKER consequence than an equality pin, so with an UNSEEDABLE justification (a
                    // non-ASCII `requires(s == "café")` seeds nothing) the obligation would carry zero facts →
                    // z3's spurious `s = ""` counterexample → over-rejection of a program that runs clean
                    // (`starts_with("café","ca")` is true). Uncovered → skip (fail-open, the pre-lane stance).
                    // Equality is intentionally NOT gated (is_pure_string_predicate is false for it): an ASCII
                    // equality assert under an unseeable pin is genuinely runtime-false, so its reject is
                    // correct — gating it would fail-open a false accept.
                    if is_pure_string_predicate(expr)
                        && !strlen_vars_covered(expr, &str_asm, &ctx.solver_string_vars)
                    {
                        // stranded pure-predicate obligation — fail-open, exactly as before this lane.
                    } else {
                        let mut raw = BTreeSet::new();
                        collect_expr_vars(expr, &mut raw);
                        let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
                        for a in &str_asm {
                            let mut avs = BTreeSet::new();
                            collect_vars_from_smt(a, &mut avs);
                            vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
                        }
                        ctx.solver_obligations.push(SolverObligation {
                            name: format!("assert:{smt}"),
                            assumptions: str_asm,
                            assertion: smt,
                            vars: vars.into_iter().collect(),
                            strings: true,
                            guard_assumptions: ctx.active_branch_guards.clone(),
                        });
                    }
                } else if is_bool_modelable_strlen(expr, &ctx.solver_string_vars) {
                    // Phase-3 str.len: a string-LENGTH body `assert` (`assert(len(s) >= 1)`) discharges in
                    // QF_S. COVERAGE-GATED (see strlen_vars_covered): with an unseedable justification
                    // (`requires(s == "é")` / `requires(len(s) >= n)`) the obligation would carry zero
                    // facts → spurious `s = ""` rejection of a valid program. Uncovered → skip (fail-open;
                    // the assert is runtime-enforced, exactly the pre-lane stance for unmodeled asserts). A
                    // predicate fact (str.contains/prefixof/suffixof) is excluded — it does not tightly bound
                    // length, so it must not spuriously "cover" this strlen obligation (see is_predicate_fact).
                    let str_asm: Vec<String> = assumptions
                        .iter()
                        .filter(|a| fact_is_string(a, &ctx.solver_string_vars) && !is_predicate_fact(a))
                        .cloned()
                        .collect();
                    if strlen_vars_covered(expr, &str_asm, &ctx.solver_string_vars) {
                        let smt = strlen_bool_to_smt(expr, &ctx.solver_string_vars);
                        let mut raw = BTreeSet::new();
                        collect_expr_vars(expr, &mut raw);
                        let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
                        for a in &str_asm {
                            let mut avs = BTreeSet::new();
                            collect_vars_from_smt(a, &mut avs);
                            vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
                        }
                        ctx.solver_obligations.push(SolverObligation {
                            name: format!("assert:{smt}"),
                            assumptions: str_asm,
                            assertion: smt,
                            vars: vars.into_iter().collect(),
                            strings: true,
                            guard_assumptions: ctx.active_branch_guards.clone(),
                        });
                    }
                }
                // A write embedded in the asserted expression escapes the statement sweep; invalidate it
                // AFTER building this obligation (the obligation is over the pre-write value).
                invalidate_embedded_writes(ctx, assumptions, expr);
                effects.push("assert".into());
            }
            Stmt::ExprStmt(expr) => {
                analyze_expr_effect(expr, mode, scope, effects, ctx);
                check_expr_semantics(expr, scope, ctx);
                // A bare `match`/`if`-expression statement (or a `return <expr>` — parsed as
                // `ExprStmt(Call{"return", …})`) can hide a write to an outer variable in an arm/branch
                // body. That write is invisible to the statement-level frame sweep, so invalidate the
                // written variables' stale solver facts here — else a later obligation is discharged
                // against the pre-write value.
                invalidate_embedded_writes(ctx, assumptions, expr);
                // Discharge the precondition of every contracted call in an UNCONDITIONALLY-executed
                // position of the statement expression — the direct `g(x);`, and a call nested in an
                // argument / operand / `return`-arg (`print(g(x))`, `h(g(x))`, `return g(x)`). Inside a
                // branch the guard is in scope as a path condition. `return <expr>` parses as
                // `Call{"return", ..}` (no contract → no-op), and the walk then reaches `g(x)` in its arg.
                discharge_calls_in_expr(ctx, assumptions, expr);
            }
            Stmt::Assign { target, value } => {
                analyze_expr_effect(value, mode, scope, effects, ctx);
                // A contracted call in ASSIGN-value position (`y = g(x);`, `y = h(g(x));`) — discharge its
                // precondition under the pre-assignment assumptions (with any branch guard in scope). The
                // TARGET place-expression is also evaluated (`arr[g(i)] = x;` computes the index/base), so
                // discharge calls in it too.
                discharge_calls_in_expr(ctx, assumptions, value);
                discharge_calls_in_expr(ctx, assumptions, target);
                let value_taint =
                    expr_taint_source_m(value, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns);
                if let (Some(source), Expr::Var(name)) = (&value_taint, target) {
                    ctx.taint_traces.push(TaintTrace {
                        source: source.clone(),
                        sink: Some(name.clone()),
                        steps: vec![format!("{} -> assign -> {}", source, name)],
                        declassified: false,
                    });
                }
                // Flow-sensitive taint on reassignment: propagate the RHS's taint status to the
                // binding. SETTING it when the RHS is tainted closes the reassignment fail-open
                // (`let x = clean; x = input(); sink(x)` was accepted because the binding kept its
                // stale clean state); CLEARING it when the RHS is clean/declassified is sound
                // straight-line, and the branch/loop taint MERGE refines it across control flow.
                if let Expr::Var(name) = target {
                    if let Some(b) = scope.get_mut(name) {
                        b.info.tainted = value_taint.is_some();
                        b.info.taint_source = value_taint.clone();
                        if value_taint.is_some() {
                            b.info.declassified = false;
                        }
                    }
                } else if let (Some(src), Some(root)) = (&value_taint, assign_target_root(target)) {
                    // A non-`Var` place-assignment (`buf[0] = k`, `obj.f = k`) is a MAY-update of the ROOT
                    // container binding: with whole-binding taint granularity, writing a tainted value
                    // into ANY element/field taints the container (SET-only — never CLEAR, since the
                    // other elements may still be clean). Without this, `buf[0] = input(); sink(buf[0])`
                    // laundered the taint (the read walker reads `buf`'s whole-binding flag as clean).
                    // Mirrors `body_param_returns`' non-`Var` MAY-update of the root.
                    if let Some(b) = scope.get_mut(root) {
                        b.info.tainted = true;
                        b.info.taint_source = Some(src.clone());
                        b.info.declassified = false;
                    }
                }
                // Flow-sensitive CONFIDENTIALITY (dual of the taint propagation above): a reassignment
                // carries the RHS's secret label — SET when the RHS is a secret, CLEAR when it is
                // clean/declassified. The branch/loop merge (`merge_taint_over`) refines it across
                // control flow, exactly as for taint.
                if let Expr::Var(name) = target {
                    let value_secret =
                        expr_secret_source_m(value, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                            .is_some();
                    if let Some(b) = scope.get_mut(name) {
                        b.secret = value_secret;
                    }
                } else if let Some(root) = assign_target_root(target) {
                    // Confidentiality dual: a non-`Var` place-assignment of a SECRET value MAY-labels the
                    // root container secret (set-only), so egressing the container is caught. Without it,
                    // `a[0] = k; send(host, port, a)` laundered the secret past ANUBIS_SECRET_EXFILTRATION.
                    let value_secret =
                        expr_secret_source_m(value, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                            .is_some();
                    if value_secret {
                        if let Some(b) = scope.get_mut(root) {
                            b.secret = true;
                        }
                    }
                }
                // A reassigned binding can no longer be modeled from its initial `let` value: the
                // solver does straight-line analysis and cannot follow a loop/branch update, so its
                // concrete-let assumption goes stale. Drop it from the modelable set AND remove any
                // stale fact about it from the assumptions — an assertion over it is then left to the
                // runtime instead of being (unsoundly) "disproved" against its pre-assignment value
                // (e.g. `for i in 1..5 { total = total + i } assert(total == 10)` must not be refuted
                // with the stale `total == 0`). Removing the stale fact — not just dropping
                // modelability — is essential: a loop invariant later RE-MODELS the variable, and a
                // surviving `x == <old>` would then launder a false invariant/postcondition.
                if let Some(root) = assign_target_root(target) {
                    clear_binding_modelability(&mut ctx.solver_int_vars, root);
                    let mangled = smt_var(root);
                    let arr = seq_arr_smt(root);
                    let len = seq_len_smt(root);
                    assumptions.retain(|a| {
                        let mut vs = BTreeSet::new();
                        collect_vars_from_smt(a, &mut vs);
                        !vs.contains(&mangled) && !vs.contains(&arr) && !vs.contains(&len)
                    });
                    // Re-establish a fresh fact when the new value is modelable and does NOT reference
                    // the reassigned variable itself — a constant or an expression over OTHER modelable
                    // variables (an `x`-referencing RHS is now unmodelable, since `x` was just removed
                    // from the modelable set, so `x = x + 1` correctly adds nothing). This keeps the
                    // common `i = 0;` reset before a counted loop provable without introducing a
                    // self-referential false fact.
                    if matches!(target, Expr::Var(_))
                        && is_int_modelable(value, &ctx.solver_int_vars)
                    {
                        let smt = expr_to_smt(value, &ctx.symbolic_widths);
                        let def = format!("(= {} {})", mangled, smt);
                        ctx.solver_int_vars.insert(root.to_string());
                        ctx.symbolic_widths.entry(root.to_string()).or_insert(64);
                        ctx.constraints.push(format!("(assert {})", def));
                        assumptions.push(def);
                    }
                    // NOTE (Phase-3 QF_FP): a float reassignment deliberately does NOT re-establish a
                    // float fact. Float `let` chaining is admitted ONLY for a variable that is never
                    // reassigned anywhere in the function (the `reassigned_roots` gate at the `let` seed),
                    // so a reassigned float variable carries no float model at all and its assertions fall
                    // to the runtime — fail-closed. Soundly tracking a reassigned float across embedded
                    // control flow (a `match`-arm / `if`-expression write the statement sweep does not
                    // visit) is the shared frame-sweep gap tracked as a follow-up; until it is closed,
                    // re-establishing here would reintroduce the leak the reassigned-roots gate prevents.
                    // Re-bind a bounded sequence when reassigned from a modelable array literal.
                    if matches!(target, Expr::Var(_)) {
                        if let Expr::ArrayLiteral { elements } = value {
                            if elements
                                .iter()
                                .all(|e| is_int_modelable(e, &ctx.solver_int_vars))
                            {
                                let n = elements.len() as u64;
                                ctx.solver_int_vars.insert(seq_mark(root));
                                ctx.solver_int_vars.insert(seq_len_mark(root, n));
                                let len_fact = format!("(= {len} (_ bv{n} 64))");
                                assumptions.push(len_fact);
                                for (i, el) in elements.iter().enumerate() {
                                    if let Some(es) = expr_to_smt_value(el, &ctx.symbolic_widths) {
                                        assumptions
                                            .push(format!("(= (select {arr} (_ bv{i} 64)) {es})"));
                                    }
                                }
                            }
                        }
                    }
                }
                check_expr_semantics(value, scope, ctx);
                // The RHS itself can hide a write to a DIFFERENT outer variable in an `if`/`match`/block
                // expression (`x = if c { y = 100; 0 } else { 0 };`); invalidate that variable's fact too
                // (the reassignment of `target` was handled above).
                invalidate_embedded_writes(ctx, assumptions, value);
                // Reassignment changes what a closure-valued binding holds: recompute its arity
                // (or clear it) so a later direct call checks the current value, not a stale one.
                if let Expr::Var(name) = target {
                    if scope.contains_key(name) {
                        let ca = closure_arity_of(value, scope, ctx);
                        if let Some(b) = scope.get_mut(name) {
                            b.closure_arity = ca;
                        }
                    }
                }
                // A+: reassignment type-checking. Only an EXPLICITLY-annotated variable is held to
                // its declared type (a `let mut acc = 0` with an INFERRED type is dynamic and may be
                // reassigned to any type — enforcing stability there was a false positive). For an
                // inferred variable, update its tracked type to the new value's type (or clear it
                // when dynamic) so later uses see the current type rather than a stale one.
                if let Expr::Var(name) = target {
                    let got = infer_expr_type_scoped(value, scope);
                    if ctx.annotated_vars.contains(name) {
                        if let (Some(expected), Some(got)) = (
                            scope.get(name).and_then(|b| b.info.ty.clone()),
                            got.as_ref(),
                        ) {
                            if !types_assignable(&expected, got) {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                    message: format!(
                                        "type mismatch on assign to `{}`: expected `{}`, got `{}`",
                                        name, expected, got
                                    ),
                                    span: None,
                                });
                            }
                        }
                    } else if let Some(b) = scope.get_mut(name) {
                        b.info.ty = got; // flow-sensitive: track the reassigned type (None if dynamic)
                    }
                }
            }
            Stmt::If { cond, then, else_ } => {
                // CRYPTO_MISUSE / effect analysis must see the condition (fail-open if skipped:
                // `if hmac_sha256(k,m) == tag { ... }` would otherwise pass).
                analyze_expr_effect(cond, mode, scope, effects, ctx);
                if expr_taint_source(cond, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    .is_some()
                {
                    effects.push("tainted-branch".into());
                }
                // A write embedded in the CONDITION expression (`if (match c { 0 => { y = 5; true } … })`)
                // is not covered by the branch sweep below (which only sees `then`/`else`), so invalidate
                // it before snapshotting. No-op for an ordinary condition with no assignment.
                invalidate_embedded_writes(ctx, assumptions, cond);
                // The condition is evaluated UNCONDITIONALLY (before either branch), so a contracted call
                // in it (`if g(a) > 0 { … }`) must have its precondition discharged — under the pre-branch
                // assumptions (the guard is not yet in scope for its own condition).
                discharge_calls_in_expr(ctx, assumptions, cond);
                // A branch may not execute, so a fact it asserts (e.g. `x = 5`) must not leak out as
                // unconditional. Analyze each branch under the pre-`if` assumptions (the branches are
                // ALTERNATIVES — reset between them so `then`'s facts don't leak into `else`), then
                // discard the branch facts and drop every variable either branch conditionally writes.
                //
                // Taint scope is snapshotted/restored the same way `body_returns_taint` does: a
                // block-scoped `let` (incl. shadowing) must not escape the branch. Without this,
                // `let x=5; if c { let x=taint(); } sink(x);` was a false-positive reject — the
                // outer clean `x` was overwritten by the inner tainted binding. Solver assumptions
                // stay on their own snapshot path below; this only restores BindingInfo scope.
                let snapshot = assumptions.clone();
                let guard_snapshot = ctx.active_branch_guards.clone();
                let snap_scope = scope.clone();
                // The guard holds inside `then` — push it as a scoped path condition.
                push_branch_path_condition(ctx, assumptions, cond, false);
                analyze_stmts(then, mode, scope, fn_symbols, effects, assumptions, ctx);
                let then_scope = scope.clone();
                let else_scope = if let Some(else_body) = else_ {
                    *assumptions = snapshot.clone();
                    ctx.active_branch_guards = guard_snapshot.clone();
                    restore_block_scope(scope, &snap_scope);
                    // The negated guard holds inside `else`.
                    push_branch_path_condition(ctx, assumptions, cond, true);
                    analyze_stmts(
                        else_body,
                        mode,
                        scope,
                        fn_symbols,
                        effects,
                        assumptions,
                        ctx
                    );
                    scope.clone()
                } else {
                    // No `else`: the alternative path leaves outer bindings at their pre-`if` state.
                    snap_scope.clone()
                };
                restore_block_scope(scope, &snap_scope);
                // Control-flow-merge for taint (may-taint): a reassignment to a tainted value in
                // EITHER branch survives so a later sink sees it — closing the branch reassignment
                // fail-open a bare restore left open.
                merge_taint_over(scope, &[&then_scope, &else_scope]);
                let else_slice: &[Stmt] = else_.as_deref().unwrap_or(&[]);
                drop_written_after_scope(ctx, assumptions, snapshot, &[then, else_slice]);
                // Path conditions are scoped to the branches: restore the pre-`if` guard stack.
                ctx.active_branch_guards = guard_snapshot;
            }
            Stmt::While {
                cond,
                body,
                invariant,
            } => {
                analyze_expr_effect(cond, mode, scope, effects, ctx);
                if expr_taint_source(cond, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    .is_some()
                {
                    effects.push("tainted-branch".into());
                }
                // Invalidate any write embedded in the loop condition (see the `if` handler); no-op for
                // an ordinary condition.
                invalidate_embedded_writes(ctx, assumptions, cond);
                // The loop condition is evaluated at least once (unconditionally on entry), so discharge a
                // contracted call in it (`while g(a) > 0 { … }`). A loop-carried variable in the condition
                // is havoced below (→ unmodeled → skipped, the documented loop residual); a non-loop-var
                // call still discharges under the pre-loop assumptions.
                discharge_calls_in_expr(ctx, assumptions, cond);
                effects.push("loop".into());
                // B3: verify loop invariants (base case + preservation) BEFORE the body drops the
                // loop-carried variables, so the base case sees their pre-loop state.
                let admit = if invariant.is_empty() {
                    None
                } else {
                    verify_while_invariants(ctx, cond, invariant, body, assumptions)
                };
                // Snapshot the pre-loop assumptions. A loop can run ZERO times, so no fact its body
                // accumulates survives it: after analysis we restore this snapshot and drop every
                // written variable. Havoc the written variables first so an in-body `assert` is not
                // discharged against a stale pre-loop value the loop mutates each iteration.
                // Same for taint scope: a loop-body `let` is block-scoped and must not escape.
                let snapshot = assumptions.clone();
                let snap_scope = scope.clone();
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                let body_scope = scope.clone();
                restore_block_scope(scope, &snap_scope);
                // Taint merge: the loop may run (body_scope) or not (snap_scope); a body reassignment
                // to a tainted value survives (may-taint), closing the loop reassignment fail-open.
                merge_taint_over(scope, &[&snap_scope, &body_scope]);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
                if let Some((post, _written, readmit)) = admit {
                    // A VERIFIED invariant DOES hold after the loop: re-model the tracked variables
                    // (constrained by the proved invariants ∧ ¬cond) so a later `ensures`/`assert` can
                    // rely on them.
                    for v in &readmit {
                        ctx.solver_int_vars.insert(v.clone());
                        ctx.symbolic_widths.entry(v.clone()).or_insert(64);
                    }
                    for a in post {
                        ctx.constraints.push(format!("(assert {})", a));
                        assumptions.push(a);
                    }
                }
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
                ..
            } => {
                analyze_expr_effect(expr, mode, scope, effects, ctx);
                effects.push("loop".into());
                // Invalidate any write embedded in the `while let` scrutinee (see the `if` handler).
                invalidate_embedded_writes(ctx, assumptions, expr);
                // The scrutinee is evaluated at least once (unconditionally on entry) — discharge a
                // contracted call in it (`while let Ok(v) = g(a) { … }`).
                discharge_calls_in_expr(ctx, assumptions, expr);
                // Snapshot BEFORE inserting pattern bindings so they do not leak past the loop.
                let snap_scope = scope.clone();
                for n in pattern.bound_names() {
                    let info = BindingInfo {
                        name: n.clone(),
                        ty: None,
                        mode: mode_name(mode).into(),
                        tainted: false,
                        taint_source: None,
                        declassified: false,
                        span: None,
                    };
                    scope.insert(
                        n.clone(),
                        ScopeBinding {
                            info,
                            closure_arity: None,
                            secret: false,
                        },
                    );
                    ctx.known_bindings.insert(n);
                }
                // Solver soundness: a `while let` pattern binder SHADOWS any outer binding of the same name
                // inside the loop body — drop the outer binding's stale fact/membership (same class as the
                // `for`/LetPattern fix; without it `let s="a"; while let Some(s)=opt() { assert(s=="a") }`
                // could discharge a body assertion the runtime violates). Body-scoped like `for`: snapshot
                // BEFORE invalidation (drop_written_after_scope restores the outer facts) and restore each
                // binder's membership after, so a post-loop contract over the outer binding still discharges.
                let saved_models: Vec<(String, BindingMembership)> = pattern
                    .bound_names()
                    .into_iter()
                    .map(|n| {
                        let m = capture_binding_membership(ctx, &n);
                        (n, m)
                    })
                    .collect();
                let snapshot = assumptions.clone();
                for (n, _) in &saved_models {
                    invalidate_binding_facts(ctx, assumptions, n);
                }
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                let body_scope = scope.clone();
                restore_block_scope(scope, &snap_scope);
                // Taint merge: the loop may run (body_scope) or not (snap_scope); a body reassignment
                // to a tainted value survives (may-taint), closing the loop reassignment fail-open.
                merge_taint_over(scope, &[&snap_scope, &body_scope]);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
                for (n, m) in saved_models {
                    restore_binding_membership(ctx, &n, m);
                }
            }
            Stmt::Loop { body, invariant } => {
                effects.push("loop".into());
                if !invariant.is_empty() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE".into()),
                        message: "an unbounded `loop` has no exit condition to assume, so an \
                             invariant cannot be discharged inductively — use a `while` loop with an \
                             explicit condition and invariant instead"
                            .into(),
                        span: None,
                    });
                }
                let snapshot = assumptions.clone();
                let snap_scope = scope.clone();
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                let body_scope = scope.clone();
                restore_block_scope(scope, &snap_scope);
                // Taint merge: the loop may run (body_scope) or not (snap_scope); a body reassignment
                // to a tainted value survives (may-taint), closing the loop reassignment fail-open.
                merge_taint_over(scope, &[&snap_scope, &body_scope]);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
            }
            Stmt::For {
                var,
                body,
                source,
                invariant,
            } => {
                effects.push("loop".into());
                if !invariant.is_empty() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE".into()),
                        message: "loop invariants are currently verified on `while` loops only; \
                             rewrite this `for` as a `while` with an explicit counter to attach an \
                             invariant (a green check must not silently ignore an invariant)"
                            .into(),
                        span: None,
                    });
                }
                // Wire the embedded-write sweep (like the 9 other statement sites): a write hidden in the
                // for-loop SOURCE — a range bound (`0..(match c { 0 => { y = 100; 1 } _ => 1 })`) or the
                // collection expression — mutates an outer variable but escapes the body frame sweep, so
                // invalidate its stale solver fact here before analyzing the body.
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        invalidate_embedded_writes(ctx, assumptions, start);
                        invalidate_embedded_writes(ctx, assumptions, end);
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        invalidate_embedded_writes(ctx, assumptions, expr);
                    }
                }
                // The range/collection header is evaluated UNCONDITIONALLY to establish the iteration, so a
                // contracted call in it (`for i in 0..g(a) { … }`) must have its precondition discharged.
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        discharge_calls_in_expr(ctx, assumptions, start);
                        discharge_calls_in_expr(ctx, assumptions, end);
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        discharge_calls_in_expr(ctx, assumptions, expr);
                    }
                }
                let taint_src = match source {
                    crate::frontend::ForSource::Range { start, .. } => {
                        expr_taint_source_m(start, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns)
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_taint_source_m(expr, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns)
                    }
                };
                // Confidentiality dual: iterating a secret collection binds a secret element.
                let secret_src = match source {
                    crate::frontend::ForSource::Range { start, .. } => {
                        expr_secret_source_m(start, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                            .is_some()
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_secret_source_m(expr, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                            .is_some()
                    }
                };
                // The loop variable is a fresh in-scope binding for the body's analysis. A range
                // loop (`for i in a..b`) binds a number; a collection loop (`for x in xs`) binds an
                // element whose type is dynamic (unknown) — typing it `u32` was a heuristic that
                // mis-flagged `for x in xs { x[0] }` as "indexing a number".
                // Snapshot BEFORE inserting the loop var so it (and any body `let`) do not escape.
                let snap_scope = scope.clone();
                let var_ty = match source {
                    crate::frontend::ForSource::Range { .. } => Some("u32".into()),
                    crate::frontend::ForSource::Collection { .. } => None,
                };
                let info = BindingInfo {
                    name: var.clone(),
                    ty: var_ty,
                    mode: mode_name(mode).into(),
                    tainted: taint_src.is_some(),
                    taint_source: taint_src,
                    declassified: false,
                    span: None,
                };
                scope.insert(
                    var.clone(),
                    ScopeBinding {
                        info: info.clone(),
                        closure_arity: None,
                        secret: secret_src,
                    },
                );
                ctx.known_bindings.insert(var.clone());
                // Solver soundness: the loop VARIABLE shadows any outer binding of the same name inside
                // the body, so drop the outer binding's stale fact/membership before analyzing it. Without
                // this, `let s="a"; for s in [...] { assert(s=="a") }` (and the string-param variant with a
                // `requires(s=="a")` fact) discharges a body assertion the runtime violates on iteration 1
                // (hunt-found false-accept). But the shadow is BODY-SCOPED: `snapshot` is taken BEFORE the
                // invalidation so drop_written_after_scope (which resets `assumptions` to it) restores the
                // outer facts — INCLUDING transitive dependents like `(= anb_y anb_n)` — after the loop, and
                // `restore_binding_membership` restores the modelability the invalidation cleared. So a
                // post-loop contract over the outer binding still discharges (review caught the over-reject).
                let saved_var_model = capture_binding_membership(ctx, var);
                let snapshot = assumptions.clone();
                invalidate_binding_facts(ctx, assumptions, var);
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                let body_scope = scope.clone();
                restore_block_scope(scope, &snap_scope);
                // Taint merge: the loop may run (body_scope) or not (snap_scope); a body reassignment
                // to a tainted value survives (may-taint), closing the loop reassignment fail-open.
                merge_taint_over(scope, &[&snap_scope, &body_scope]);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
                restore_binding_membership(ctx, var, saved_var_model);
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::SpecBlock { .. } => effects.push("spec".into()),
        }
    }
}

/// RWC Ch3: comparing HMAC tags with `==` (early-exit) enables timing attacks.
/// Prefer `hmac_sha256_verify` / `std.crypto::mac_verify` (constant-time).
fn expr_is_hmac_tag_call(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Call { callee, .. }
            if callee == "hmac_sha256"
                || callee == "hmac_sha256_hex"
                || callee == "hmac_sha256_bytes"
                || callee.ends_with("__mac_hmac_sha256")
                || callee.ends_with("__mac_hmac_sha256_bytes")
    )
}

/// RWC Ch8: password encodings / KDF outputs must not be compared with early-exit `==`.
fn expr_is_password_secret_call(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Call { callee, .. }
            if callee == "password_hash"
                || callee == "password_hash_encode"
                || callee == "password_hash_pbkdf2"
                || callee == "password_hash_pbkdf2_encode"
                || callee == "password_hash_phc"
                || callee == "password_hash_phc_raw"
                || callee == "argon2id_hash"
                || callee == "pbkdf2_hmac_sha256"
                || callee == "ed25519_sign"
                || callee.ends_with("__password_hash")
                || callee.ends_with("__password_hash_pbkdf2")
                || callee.ends_with("__password_hash_phc")
                || callee.ends_with("__kdf_argon2id")
                || callee.ends_with("__kdf_pbkdf2_hmac_sha256")
                || callee.ends_with("__sign")
    )
}

fn analyze_expr_effect(
    expr: &Expr,
    mode: Mode,
    scope: &BTreeMap<String, ScopeBinding>,
    effects: &mut Vec<String>,
    ctx: &mut SemanticContext,
) {
    match expr {
        Expr::Binary { op, lhs, rhs } if op == "==" || op == "!=" => {
            if expr_is_hmac_tag_call(lhs) || expr_is_hmac_tag_call(rhs) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_CRYPTO_MISUSE".into()),
                    message: "comparing an HMAC tag with `==`/`!=` is not constant-time and is \
                         vulnerable to timing attacks (RWC Ch3). Use `hmac_sha256_verify(key, msg, tag)` \
                         or `std.crypto::mac_verify` instead"
                        .into(),
                    span: None,
                });
            }
            if expr_is_password_secret_call(lhs) || expr_is_password_secret_call(rhs) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_CRYPTO_MISUSE".into()),
                    message: "comparing a password hash/KDF output with `==`/`!=` is not constant-time \
                         (RWC Ch8). Use `password_verify(password, encoding)` or `std.crypto::password_verify`"
                        .into(),
                    span: None,
                });
            }
            analyze_expr_effect(lhs, mode, scope, effects, ctx);
            analyze_expr_effect(rhs, mode, scope, effects, ctx);
        }
        Expr::Call { callee, args } => {
            // A+ call-site type checks for user functions (not builtins).
            if let Some(param_tys) = ctx.fn_params.get(callee).cloned() {
                if args.len() != param_tys.len() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_ARITY_MISMATCH".into()),
                        message: format!(
                            "function `{}` expects {} argument(s), got {}",
                            callee,
                            param_tys.len(),
                            args.len()
                        ),
                        span: None,
                    });
                } else {
                    for (i, (arg, expected)) in args.iter().zip(param_tys.iter()).enumerate() {
                        if let Some(got) = infer_expr_type_scoped(arg, scope) {
                            if !types_assignable(expected, &got) {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                    message: format!(
                                        "type mismatch: argument {} of `{}` expects `{}`, got `{}`",
                                        i, callee, expected, got
                                    ),
                                    span: None,
                                });
                            }
                        }
                    }
                }
            }
            if callee == "shell" || callee == "exec" || callee == "system" || callee == "target_run"
            {
                effects.push("shell".to_string());
                // Safe: forbidden unless `uses(shell)` (or uses(exec)/proc.exec) declared.
                // `target_run` is the PoC process harness — same capability gate as shell/exec.
                if mode == Mode::Safe && !safe_cap_allowed(ctx, "shell") {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                        message: "safe mode shell/exec/target_run effect is forbidden without `uses(shell)` (or use @research/@poc with authorization)".to_string(),
                        span: None,
                    });
                }
            }
            if callee == "read_file" || callee == "open" {
                effects.push("file_read".to_string());
                // file_read is allowed in Safe by default (legacy); verified lane still requires uses.
            }
            if callee == "write_file" || callee == "write" || callee == "append_file" {
                effects.push("file_write".to_string());
                // Safe: authorized when `uses(fs.write)` is declared on this function.
                if mode == Mode::Safe && !safe_cap_allowed(ctx, "fs.write") {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                        message: "safe mode file_write forbidden without `uses(fs.write)`"
                            .to_string(),
                        span: None,
                    });
                }
            }
            if callee.contains("network") || callee == "send" || callee == "connect" {
                effects.push("network".to_string());
                // Safe: authorized when `uses(net.send)` (or net.connect) is declared.
                if mode == Mode::Safe && !safe_cap_allowed(ctx, "net.send") {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                        message: "safe mode network effect forbidden without `uses(net.send)`"
                            .to_string(),
                        span: None,
                    });
                }
            }
            if matches!(callee.as_str(), "time" | "time_now" | "now") {
                effects.push("time".to_string());
            }
            if matches!(callee.as_str(), "rand" | "rand_gen" | "random") {
                effects.push("rand".to_string());
            }
            // Phase-5: inherit declared `uses(...)` from a user-defined callee (incl. namespaced
            // std wrappers after combine). Without this, `std.io::write_text` / `std.pwn::run_local`
            // would launder fs.write/shell past Safe — the wrapper itself has the uses clause, but
            // the *caller* never saw the capability. Fail-closed: same Safe/verified gates as a
            // direct builtin of that capability.
            if let Some(caps) = ctx.fn_declared_effects.get(callee).cloned() {
                for raw in caps {
                    apply_inherited_capability(raw, mode, effects, ctx);
                }
            }
            if is_sink(callee) {
                effects.push(format!("sink:{}", callee));
                for arg in args {
                    if let Some(source) =
                        expr_taint_source_m(arg, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns)
                    {
                        let declassified = expr_is_declassified(arg, scope);
                        ctx.taint_traces.push(TaintTrace {
                            source: source.clone(),
                            sink: Some(callee.clone()),
                            steps: vec![format!("{} -> {}", source, callee)],
                            declassified,
                        });
                        if mode == Mode::Safe && !declassified {
                            ctx.diagnostics.push(SemanticDiagnostic {
                                code: Some("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY".into()),
                                message: format!(
                                    "safe mode tainted flow from `{}` to sink `{}` requires declassify() or research boundary",
                                    source, callee
                                ),
                                span: None,
                            });
                        }
                    }
                }
            }
            // CONFIDENTIALITY egress — the DUAL of the taint sink check above (leg-1 of the lethal
            // trifecta, made precise). A SECRET value (seeded by `secret_source(..)`, flowed through
            // let/assign/branch-merge) reaching a network/shell EGRESS is exfiltration unless it was
            // released. `expr_secret_source` already returns `None` for a well-formed
            // `declassify(x, policy, reason)`, so the release hatch is built in (a MALFORMED declassify
            // does NOT hatch — AST-shape keyed, matching the taint side). Safe-mode; independent of
            // `is_sink` so it also covers egress builtins (`http_post`, `connect`) not in that set.
            if mode == Mode::Safe && is_egress_sink(callee) {
                for arg in args {
                    if let Some(source) =
                        expr_secret_source_m(arg, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                    {
                        ctx.emit(
                            SemanticDiagnostic {
                                code: Some("ANUBIS_SECRET_EXFILTRATION".into()),
                                message: format!(
                                    "safe mode confidentiality violation: secret `{source}` flows to egress `{callee}` without declassify() — release a private value with declassify(value, policy=…, reason=…) before it leaves the program"
                                ),
                                span: None,
                            },
                            // ENFORCING. Corpus-inert by construction (no committed program sends a
                            // `secret_source` VALUE — every existing usage egresses a literal), verified
                            // both statically (every `secret_source` occurrence audited) and empirically
                            // (the enforcing language/turing/security/stdlib gates stay green in the VM).
                            false,
                        );
                    }
                }
            }
            // Phase-3 A1: interprocedural param→sink. A callee whose formal N reaches a sink
            // makes the call site a sink for argument N — even though the actual `sink(...)` is
            // inside the callee. Distinct code from the direct-sink check so callers can see
            // `ANUBIS_INTERPROC_SINK` (the leak is at the call boundary, not a local sink name).
            if let Some(sink_params) = ctx.param_sinks.get(callee).cloned() {
                for i in sink_params {
                    if let Some(arg) = args.get(i) {
                        if let Some(source) = expr_taint_source_m(
                            arg,
                            scope,
                            &ctx.tainting_fns,
                            &ctx.param_return_taint,
                            &ctx.method_tainting_fns,
                        ) {
                            let declassified = expr_is_declassified(arg, scope);
                            ctx.taint_traces.push(TaintTrace {
                                source: source.clone(),
                                sink: Some(format!("{}(param {})", callee, i)),
                                steps: vec![format!(
                                    "{} -> call `{}` param {} -> sink",
                                    source, callee, i
                                )],
                                declassified,
                            });
                            if mode == Mode::Safe && !declassified {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_INTERPROC_SINK".into()),
                                    message: format!(
                                        "safe mode tainted flow from `{}` into parameter {} of `{}`, which reaches a sink without declassify",
                                        source, i, callee
                                    ),
                                    span: None,
                                });
                            }
                        }
                    }
                }
            }
            // CONFIDENTIALITY interprocedural egress — the DUAL of the ANUBIS_INTERPROC_SINK block
            // above, and the interprocedural twin of the direct ANUBIS_SECRET_EXFILTRATION. A callee
            // whose formal N flows to a network/shell EGRESS (per `ctx.param_egress`) makes a SECRET
            // argument N an exfiltration at the call boundary — even though the actual `send(...)` is
            // inside the callee, and even when no `secret_source` appears at the call site (the arg may
            // itself be a secret-returning helper). Egress-only, so a secret into a LOCAL write is not
            // flagged. A well-formed declassify releases (via `expr_secret_source` → None). Safe-mode.
            if mode == Mode::Safe {
                if let Some(egress_params) = ctx.param_egress.get(callee).cloned() {
                    for i in egress_params {
                        if let Some(arg) = args.get(i) {
                            if let Some(source) = expr_secret_source_m(
                                arg,
                                scope,
                                &ctx.secret_fns,
                                &ctx.param_return_taint,
                                &ctx.method_secret_fns,
                            ) {
                                ctx.emit(
                                    SemanticDiagnostic {
                                        code: Some("ANUBIS_INTERPROC_EXFILTRATION".into()),
                                        message: format!(
                                            "safe mode confidentiality violation: secret `{source}` flows into parameter {i} of `{callee}`, which reaches an egress without declassify — release a private value with declassify(value, policy=…, reason=…) before it leaves the program"
                                        ),
                                        span: None,
                                    },
                                    false,
                                );
                            }
                        }
                    }
                }
            }
            // Nested calls in arguments also produce effects (`return read_file(p)`, `sink(read_file(p))`).
            for arg in args {
                analyze_expr_effect(arg, mode, scope, effects, ctx);
            }
            // #65: a HIGHER-ORDER builtin (`map`/`each`/`times`/…) APPLIES its inline closure argument
            // internally (the apply is inside the `anubis_*` runtime fn — there is NO source-level
            // application node), so a sink/egress/privileged call in the lambda body is otherwise never
            // enforced: `each([1], |x| send(h,p,secret))` fires NOTHING. Re-enter the body here so the
            // Safe capability gate, the tainted-sink check, and the secret-exfiltration check run — under
            // a CLONE of the ambient scope so a captured secret/tainted binding is seen, with the lambda
            // params inserted as fresh unlabelled bindings (a param SHADOWS a same-named captured var).
            // The generic `args` walk above hit the `Expr::Lambda` catch-all (opaque), so this is the
            // only visit of the body — no double-emit. Guarded to fire ONLY when `callee` resolves to the
            // real builtin (not a local binding and not a user fn — which is analyzed on its own).
            if !scope.contains_key(callee) && !ctx.all_fns.contains(callee) {
                for &i in effects::higher_order_closure_args(callee) {
                    if let Some(Expr::Lambda { params, body }) = args.get(i) {
                        let mut local = scope.clone();
                        for p in params {
                            local.insert(
                                p.clone(),
                                ScopeBinding {
                                    info: BindingInfo {
                                        name: p.clone(),
                                        ty: None,
                                        mode: String::new(),
                                        tainted: false,
                                        taint_source: None,
                                        declassified: false,
                                        span: None,
                                    },
                                    closure_arity: None,
                                    secret: false,
                                },
                            );
                        }
                        analyze_expr_effect(body, mode, &local, effects, ctx);
                    }
                }
            }
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => {
            if let Some(source) =
                expr_taint_source(inner, scope, &ctx.tainting_fns, &ctx.param_return_taint)
            {
                let mut steps = vec![format!("{} -> declassify", source)];
                if let Some(p) = policy {
                    steps.push(format!("policy={}", p));
                }
                if let Some(r) = reason {
                    steps.push(format!("reason={}", r));
                }
                let has_policy = policy.is_some() && reason.is_some();
                ctx.taint_traces.push(TaintTrace {
                    source: source.clone(),
                    sink: None,
                    steps,
                    declassified: has_policy,
                });
                effects.push("declassify".into());
                if mode == Mode::Safe && !has_policy {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_DECLASSIFY_MISSING_POLICY_REASON".into()),
                        message: "declassify in safe mode requires policy and reason: declassify(value, policy: \"...\", reason: \"...\")".into(),
                        span: None,
                    });
                }
            }
            // A CALL buried in the declassified value carries its own effects (`declassify(shell("id"),
            // …)` must still register the shell capability) — walk it unconditionally, independent of
            // whether the inner value is tainted.
            analyze_expr_effect(inner, mode, scope, effects, ctx);
        }
        // Control-flow value expressions — SCOPE-AWARE effect descent. Before this, a sink/egress/
        // privileged CALL buried in a match arm, an if/if-let branch, or a block statement fell to the
        // catch-all below and was NOT enforced — a real capability-laundering bypass (`if true {
        // shell("id") }` with no `uses(shell)` was accepted). Each arm/branch/block extends a CLONE of
        // the ambient scope so a pattern var / block-local `let` SHADOWS an outer same-named binding
        // (the sink check inside then resolves the inner clean binding, not the outer labelled one).
        // The `if` condition and match guards ARE walked for effects (a sink in a guard/cond is a real
        // effect) — unlike the value walkers, which ignore them because they are control, not value.
        Expr::If { cond, then, else_, .. } => {
            analyze_expr_effect(cond, mode, scope, effects, ctx);
            analyze_expr_effect(then, mode, scope, effects, ctx);
            analyze_expr_effect(else_, mode, scope, effects, ctx);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            analyze_expr_effect(scrutinee, mode, scope, effects, ctx);
            let st = expr_taint_source_m(scrutinee, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns);
            let ss = expr_secret_source_m(scrutinee, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                .is_some();
            for arm in arms {
                let mut local = scope.clone();
                seed_effect_pattern(&mut local, &arm.pattern, &st, ss);
                if let Some(guard) = &arm.guard {
                    analyze_expr_effect(guard, mode, &local, effects, ctx);
                }
                analyze_expr_effect(&arm.body, mode, &local, effects, ctx);
            }
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            analyze_expr_effect(scrutinee, mode, scope, effects, ctx);
            let st = expr_taint_source_m(scrutinee, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns);
            let ss = expr_secret_source_m(scrutinee, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                .is_some();
            let mut local = scope.clone();
            seed_effect_pattern(&mut local, pattern, &st, ss);
            analyze_expr_effect(then, mode, &local, effects, ctx);
            analyze_expr_effect(else_, mode, scope, effects, ctx);
        }
        Expr::Block { stmts, tail } => {
            let mut local = scope.clone();
            walk_block_effects(stmts, tail.as_deref(), mode, &mut local, effects, ctx);
        }
        // Non-control-flow COMPOUND expressions — recurse into every sub-expression so a sink/egress/
        // privileged CALL buried in an aggregate, an index, a cast, a unary/binary operand, a field
        // access, a `?`, a declassify operand, or a closure/method-application arg is enforced. These
        // introduce no bindings, so (unlike the control-flow descent above) no scope handling is needed
        // — plain recursion. Before this, `let x = shell("id") as u64;` / `let x = [shell("id")];` /
        // `send("h",80,"x") + 1` / `arr[shell("id")]` / `obj.method(shell("id"))` laundered the buried
        // call's effects (capability/sink/egress) past the catch-all below — the same bypass class the
        // control-flow descent closed, for the value-shape wrappers. Only the `Lambda` literal BODY
        // stays opaque — its effects are deferred to the application site, not the definition (a
        // higher-order boundary; the closure-application callee/args ARE concrete call-site exprs and
        // are walked via `CallExpr`).
        Expr::CallExpr { callee, args } => {
            // Interprocedural METHOD sink/egress — the impl-method twin of the two blocks in the bare
            // `Expr::Call` arm above. A method call `recv.name(a, b)` parses as `CallExpr { callee:
            // FieldAccess{ base: recv, field: name }, args: [a, b] }`; the sink/egress laundering checks
            // only ran on bare `Expr::Call`, so `m.deliver(secret)` / `r.go(tainted)` — a method that
            // sends/execs its argument — bypassed them entirely. We consult the method summaries
            // (`method_param_sinks` / `method_param_egress`, keyed by bare method name, unioned across
            // impls) with the SELF-OFFSET: summary index 0 is the receiver (`base`), index p≥1 is call
            // arg p-1 (matching the runtime `call_args = [receiver, …args]`).
            //
            // SOUNDNESS BOUNDARY. This block catches ONE-HOP argument laundering with a directly-supplied
            // labelled value (and a secret receiver, via index 0). Two former residuals are now CLOSED:
            // a secret/taint ORIGINATING from a method return (`let k = v.key(); send(h,p,k)`) by #67
            // (`method_secret_fns`/`method_tainting_fns` + the source-walker `_m` `CallExpr` arm), and
            // ARGUMENT laundering THROUGH a method (`fn f(m,x){ m.snd(x) }`, `fn ship(self,p){
            // self.deliver(p) }`) by #68 (the joint free-fn↔method `param_sinks`/`param_egress` fixpoint +
            // the `collect_param_sinks_in_expr`/`expr_param_flow` `CallExpr` arms). A third — method-RETURN
            // CHAINING (`fn alias(self){ return self.key() }` where `key` mints a secret) — is now CLOSED
            // too by #70 (`compute_method_{secret,tainting}_fns` combined fixpoints consulting the growing
            // method set). REMAINING fail-open residuals (no false rejects): a sink/egress inside a lambda
            // body (#65); and a sink buried in non-linear control-flow inside a method's value-position
            // block (inherited from the summary walker's linear block handling).
            if let Expr::FieldAccess { base, field, .. } = callee.as_ref() {
                // Resolve a method-summary parameter index to its call-site expression under the
                // self-offset: index 0 is the receiver (`base`), index p≥1 is call arg p-1. Inlined
                // (rather than a closure) to sidestep closure return-lifetime elision on the `&Expr`.
                // INTEGRITY: a tainted value in a summarized-sink method parameter is INTERPROC_SINK.
                if let Some(sink_params) = ctx.method_param_sinks.get(field).cloned() {
                    for i in sink_params {
                        let arg_opt: Option<&Expr> = if i == 0 {
                            Some(base.as_ref())
                        } else {
                            args.get(i - 1)
                        };
                        if let Some(arg) = arg_opt {
                            if let Some(source) = expr_taint_source_m(
                                arg,
                                scope,
                                &ctx.tainting_fns,
                                &ctx.param_return_taint,
                                &ctx.method_tainting_fns,
                            ) {
                                let declassified = expr_is_declassified(arg, scope);
                                ctx.taint_traces.push(TaintTrace {
                                    source: source.clone(),
                                    sink: Some(format!("{}(param {})", field, i)),
                                    steps: vec![format!(
                                        "{} -> call method `{}` param {} -> sink",
                                        source, field, i
                                    )],
                                    declassified,
                                });
                                if mode == Mode::Safe && !declassified {
                                    ctx.diagnostics.push(SemanticDiagnostic {
                                        code: Some("ANUBIS_INTERPROC_SINK".into()),
                                        message: format!(
                                            "safe mode tainted flow from `{}` into parameter {} of method `{}`, which reaches a sink without declassify",
                                            source, i, field
                                        ),
                                        span: None,
                                    });
                                }
                            }
                        }
                    }
                }
                // CONFIDENTIALITY: a secret value in a summarized-egress method parameter is
                // INTERPROC_EXFILTRATION — the dual, Safe-mode-gated exactly like the free-fn block.
                if mode == Mode::Safe {
                    if let Some(egress_params) = ctx.method_param_egress.get(field).cloned() {
                        for i in egress_params {
                            let arg_opt: Option<&Expr> = if i == 0 {
                                Some(base.as_ref())
                            } else {
                                args.get(i - 1)
                            };
                            if let Some(arg) = arg_opt {
                                if let Some(source) = expr_secret_source_m(
                                    arg,
                                    scope,
                                    &ctx.secret_fns,
                                    &ctx.param_return_taint,
                                    &ctx.method_secret_fns,
                                ) {
                                    ctx.emit(
                                        SemanticDiagnostic {
                                            code: Some("ANUBIS_INTERPROC_EXFILTRATION".into()),
                                            message: format!(
                                                "safe mode confidentiality violation: secret `{source}` flows into parameter {i} of method `{field}`, which reaches an egress without declassify — release a private value with declassify(value, policy=…, reason=…) before it leaves the program"
                                            ),
                                            span: None,
                                        },
                                        false,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            analyze_expr_effect(callee, mode, scope, effects, ctx);
            for arg in args {
                analyze_expr_effect(arg, mode, scope, effects, ctx);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            analyze_expr_effect(lhs, mode, scope, effects, ctx);
            analyze_expr_effect(rhs, mode, scope, effects, ctx);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => {
            analyze_expr_effect(expr, mode, scope, effects, ctx);
        }
        Expr::Tainted { inner, .. } => analyze_expr_effect(inner, mode, scope, effects, ctx),
        Expr::Assume(inner) | Expr::Assert(inner) => {
            analyze_expr_effect(inner, mode, scope, effects, ctx);
        }
        Expr::FieldAccess { base, .. } => analyze_expr_effect(base, mode, scope, effects, ctx),
        Expr::Index { base, index } => {
            analyze_expr_effect(base, mode, scope, effects, ctx);
            analyze_expr_effect(index, mode, scope, effects, ctx);
        }
        Expr::Try(inner) => analyze_expr_effect(inner, mode, scope, effects, ctx),
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                analyze_expr_effect(e, mode, scope, effects, ctx);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                analyze_expr_effect(e, mode, scope, effects, ctx);
            }
        }
        Expr::EnumConstruct { fields, .. } => {
            for e in fields {
                analyze_expr_effect(e, mode, scope, effects, ctx);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                analyze_expr_effect(k, mode, scope, effects, ctx);
                analyze_expr_effect(v, mode, scope, effects, ctx);
            }
        }
        _ => {}
    }
}

/// Walk the statements (and tail) of a value-position block for EFFECTS ONLY, scope-aware. A focused
/// walker rather than a reuse of `analyze_stmts`, because `analyze_stmts` also re-runs the per-`let`
/// SEMANTIC checks (unknown-var / raw-pointer / generic-arity / annotation type-mismatch) that
/// `check_expr_semantics`/`check_block_exprs` already own for block-locals — reusing it would newly
/// (double-)enforce those on block-local `let`s, out of this slice's scope. Here we only: seed each
/// block-local binding (both labels, in `let`-then-analyze order matching the main analyzer) so a
/// later/tail sink sees the right taint/secret, and hand each statement's effect-bearing
/// sub-expressions back to `analyze_expr_effect`. Nested control-flow STATEMENTS recurse with
/// snapshot/restore (no cross-body merge — a loop-carried label escaping to the block tail is a named
/// fail-open residual, unchanged from the pre-slice opaque behavior). `Assume`-inner, non-`Var`
/// assign LHS, and research/exploit/spec blocks (mode-elevated) are left to their existing handling.
fn walk_block_effects(
    stmts: &[Stmt],
    tail: Option<&Expr>,
    mode: Mode,
    scope: &mut BTreeMap<String, ScopeBinding>,
    effects: &mut Vec<String>,
    ctx: &mut SemanticContext,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, ty, init, .. } => {
                analyze_expr_effect(init, mode, scope, effects, ctx);
                seed_effect_let(
                    name,
                    ty.as_deref(),
                    init,
                    scope,
                    &ctx.tainting_fns,
                    &ctx.secret_fns,
                    &ctx.param_return_taint,
                    &ctx.method_tainting_fns,
                    &ctx.method_secret_fns,
                );
            }
            Stmt::LetPattern { pattern, init, .. } => {
                analyze_expr_effect(init, mode, scope, effects, ctx);
                let t = expr_taint_source_m(init, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns);
                let s = expr_secret_source_m(init, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                    .is_some();
                seed_effect_pattern(scope, pattern, &t, s);
            }
            Stmt::Assign {
                target: Expr::Var(name),
                value,
            } => {
                analyze_expr_effect(value, mode, scope, effects, ctx);
                let t = expr_taint_source_m(value, scope, &ctx.tainting_fns, &ctx.param_return_taint, &ctx.method_tainting_fns);
                let s = expr_secret_source_m(value, scope, &ctx.secret_fns, &ctx.param_return_taint, &ctx.method_secret_fns)
                    .is_some();
                if let Some(b) = scope.get_mut(name) {
                    b.info.tainted = t.is_some();
                    if t.is_some() {
                        b.info.declassified = false;
                    }
                    b.info.taint_source = t;
                    b.secret = s;
                }
            }
            Stmt::Assign { value, .. } => {
                // Non-`Var` LHS: still walk the value for buried effects; the binding is untracked.
                analyze_expr_effect(value, mode, scope, effects, ctx);
            }
            Stmt::ExprStmt(Expr::Assert(inner)) => {
                analyze_expr_effect(inner, mode, scope, effects, ctx);
            }
            Stmt::ExprStmt(e) => analyze_expr_effect(e, mode, scope, effects, ctx),
            Stmt::If { cond, then, else_ } => {
                analyze_expr_effect(cond, mode, scope, effects, ctx);
                let snap = scope.clone();
                walk_block_effects(then, None, mode, scope, effects, ctx);
                *scope = snap.clone();
                if let Some(else_body) = else_ {
                    walk_block_effects(else_body, None, mode, scope, effects, ctx);
                }
                *scope = snap;
            }
            Stmt::While { cond, body, .. } => {
                analyze_expr_effect(cond, mode, scope, effects, ctx);
                let snap = scope.clone();
                walk_block_effects(body, None, mode, scope, effects, ctx);
                *scope = snap;
            }
            Stmt::WhileLet { expr, body, .. } => {
                analyze_expr_effect(expr, mode, scope, effects, ctx);
                let snap = scope.clone();
                walk_block_effects(body, None, mode, scope, effects, ctx);
                *scope = snap;
            }
            Stmt::Loop { body, .. } => {
                let snap = scope.clone();
                walk_block_effects(body, None, mode, scope, effects, ctx);
                *scope = snap;
            }
            Stmt::For {
                source, body, ..
            } => {
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        analyze_expr_effect(start, mode, scope, effects, ctx);
                        analyze_expr_effect(end, mode, scope, effects, ctx);
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        analyze_expr_effect(expr, mode, scope, effects, ctx)
                    }
                }
                let snap = scope.clone();
                walk_block_effects(body, None, mode, scope, effects, ctx);
                *scope = snap;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let snap = scope.clone();
                    walk_block_effects(b, None, mode, scope, effects, ctx);
                    *scope = snap;
                }
            }
            _ => {}
        }
    }
    if let Some(t) = tail {
        analyze_expr_effect(t, mode, scope, effects, ctx);
    }
}

pub struct TaintPass;
impl TaintPass {
    pub fn apply(mut typed: TypedIR) -> TypedIR {
        if !typed.taint_labels.is_empty() {
            let sources: Vec<String> = typed
                .symbols
                .iter()
                .filter(|binding| binding.tainted)
                .filter_map(|binding| binding.taint_source.clone())
                .collect();
            if !sources.is_empty() {
                typed
                    .taint_labels
                    .push(format!("derived_from: {}", sources.join(",")));
            }
        }
        for trace in &typed.taint_traces {
            let sink = trace.sink.as_deref().unwrap_or("declassify");
            typed.taint_labels.push(format!(
                "trace: {} -> {}{}",
                trace.source,
                sink,
                if trace.declassified {
                    " (declassified)"
                } else {
                    ""
                }
            ));
        }
        typed
    }
}

/// Whether an obligation whose solver verdict is `unknown` (UNDECIDED within the z3 time budget — not
/// disproved, no counterexample) must fail closed. Every proof-carrying contract obligation qualifies:
/// an `ensures`, a `requires@` call-site precondition, an `assert`, AND BOTH loop-invariant obligations —
/// the base case AND the preservation STEP. The step is deliberately excluded from the separate VACUITY
/// check (a loop whose invariant implies `¬cond` legitimately never iterates, so a vacuous step is fine),
/// but an UNDECIDED step is still not a proof: admitting the invariant's post-loop fact on a timed-out
/// preservation step would certify a possibly-false invariant — a fail-open gap the vacuity exclusion
/// must NOT extend to the undecided-verdict handling.
pub(crate) fn obligation_undecided_is_unsound(name: &str) -> bool {
    name.starts_with("ensures:")
        || name.starts_with("requires@")
        || name.starts_with("loop-invariant-base:")
        || name.starts_with("loop-invariant-step:")
        || name.starts_with("assert:")
}

pub struct SymbolicEngine;
impl SymbolicEngine {
    /// Returns usable SMT-LIB path constraints (ready for Z3 or other solver).
    pub fn generate_constraints(source: &str) -> Vec<String> {
        let ast = crate::frontend::parse_source(source)
            .unwrap_or(crate::frontend::AST {
                items: vec![],
                ..Default::default()
            });
        let ir = typecheck(ast, Mode::Safe).unwrap_or_else(|_| empty_ir());
        ir.constraints
    }

    pub fn check_obligations(ir: &TypedIR) -> Vec<SolverCheck> {
        if ir.solver_obligations.is_empty() {
            return vec![SolverCheck {
                name: "solver:no-obligations".into(),
                status: "PASS".into(),
                detail: "no assertions to discharge".into(),
                model: None,
                smt: "(check-sat)".into(),
            }];
        }

        ir.solver_obligations
            .iter()
            .map(|obl| {
                // Faithful complete smt with defs from ir + obligation
                let vars: BTreeSet<String> = obl.vars.iter().cloned().collect();
                let mut body = String::new();
                for a in &obl.assumptions {
                    body.push_str(&format!("(assert {})\n", a));
                }
                body.push_str(&format!("(assert (not {}))\n", obl.assertion));
                // Per-obligation theory (a symbol is never two sorts, so this is per-obligation): QF_S if
                // the body carries an SMT string literal (`"`, only the string encoder emits it), else
                // QF_FP for a float obligation (`fp.`/`to_fp`), else QF_ABV for the array sort, else QF_BV.
                // A var-vs-var string obligation carries no `"`, so the body sniff cannot route it — the
                // builder's explicit `strings` sort tag forces QF_S (a symbol is never two sorts, so this
                // stays per-obligation and cannot mis-sort an int/float body).
                let uses_strings = obl.strings || smt_uses_strings(&body);
                let uses_floats = smt_uses_floats(&body);
                let logic = if uses_strings {
                    "QF_S"
                } else if uses_floats {
                    "QF_FP"
                } else if smt_uses_arrays(&body, &vars) {
                    "QF_ABV"
                } else {
                    "QF_BV"
                };
                let mut smt = format!("(set-logic {logic})\n");
                for v in &vars {
                    if !v.starts_with("bv") && v != "_" && !v.chars().all(|c| c.is_ascii_digit()) {
                        // Every integer variable is a 64-bit bit-vector: the runtime is i64 and
                        // type-annotation widths are inert, so a narrower declaration would be an
                        // unsound abstraction. Sequence arrays use the `__arr` Array sort (A2); float
                        // vars are IEEE-754 Float64 (QF_FP); string vars are `String` (QF_S).
                        smt.push_str(&declare_smt_var_maybe_float(v, uses_strings, uses_floats));
                    }
                }
                smt.push_str(&body);
                smt.push_str("(check-sat)\n(get-model)\n");
                let mut check = run_z3_obligation_with_smt(obl, smt);
                // Vacuity guard for CONTRACT obligations: `A ⟹ P` is proved by `A ∧ ¬P` UNSAT, but
                // that is also UNSAT when the assumptions `A` are self-contradictory — a VACUOUS
                // "proof". A precondition + `assume` that cannot both hold (e.g. `requires(x < 100)`
                // with `assume(x > 1000)`) would otherwise certify any postcondition while the code
                // runs and violates it. If a passing contract obligation has contradictory
                // assumptions, fail closed.
                // The loop-invariant BASE case uses the pre-loop assumptions; a contradictory
                // pre-loop state (a false `assume`/`requires` about a loop-carried variable) would
                // otherwise let the base pass vacuously, the bogus invariant be assumed after the
                // loop, and a false postcondition be certified. (The preservation STEP is NOT
                // vacuity-checked: a loop whose invariant implies ¬cond legitimately never iterates.)
                let is_contract = obl.name.starts_with("ensures:")
                    || obl.name.starts_with("requires@")
                    || obl.name.starts_with("loop-invariant-base:")
                    || obl.name.starts_with("assert:");
                if check.status == "PASS" && is_contract && !obl.assumptions.is_empty() {
                    if let Some(false) = assumptions_satisfiable(obl) {
                        check.status = "FAIL".into();
                        check.detail = "vacuous proof: the contract's assumptions are \
                             self-contradictory (unsatisfiable), so the postcondition is not really \
                             established — check for a `requires`/`assume` that cannot hold"
                            .into();
                    }
                }
                // A contract obligation the solver could not DECIDE (z3 `unknown`, e.g. a per-query
                // timeout on a hard symbolic division/remainder — see `Z3_ARGS`) is NOT proven. The
                // proof-carrying gate fails closed on it rather than accept an unverified postcondition.
                // It was not disproved (no counterexample), only undecided within budget — say so, and
                // clear any model. This branch became reachable once queries got a time budget.
                // NOTE the predicate is BROADER than the vacuity `is_contract` above: a loop-invariant
                // PRESERVATION step is (correctly) NOT vacuity-checked, but an UNDECIDED step is still not
                // a proof — an `unknown` preservation must fail closed exactly like an `ensures`, else a
                // timed-out step silently admits a possibly-false invariant whose post-loop fact then
                // certifies a false postcondition (a fail-OPEN gap the step's vacuity exclusion left open).
                if check.status == "UNKNOWN" && obligation_undecided_is_unsound(&obl.name) {
                    check.status = "FAIL".into();
                    check.detail = "solver could not decide this contract within its time budget (z3 \
                         returned `unknown`, typically a hard symbolic division/remainder); failing \
                         closed — an undecided postcondition is not a proof. Restate it as a simpler \
                         or better-bounded obligation"
                        .into();
                    check.model = None;
                }
                check
            })
            .collect()
    }
}

/// Whether a contract obligation's assumptions are jointly satisfiable. `Some(true)`/`Some(false)`
/// from z3; `None` if the solver did not cleanly decide (in which case the caller keeps the original
/// verdict rather than fabricating a vacuity failure).
/// z3 CLI args for every obligation query. `-t` is a per-check SOFT timeout (ms) and `-T` a HARD
/// wall-clock backstop (s): a query z3 cannot decide in budget returns `unknown` (or the process is
/// killed and yields empty output) instead of hanging the checker indefinitely. Bit-blasting a
/// symbolic `bvsdiv`/`bvsrem` over two free 64-bit operands can otherwise blow up unpredictably.
/// Both timeout outcomes are handled FAIL-CLOSED downstream (UNKNOWN / None — never a proof).
const Z3_ARGS: [&str; 4] = ["-in", "-smt2", "-t:10000", "-T:20"];

fn assumptions_satisfiable(obl: &SolverObligation) -> Option<bool> {
    let vars: BTreeSet<String> = obl.vars.iter().cloned().collect();
    let mut body = String::new();
    // The vacuity check asks "are the CONTRACT PREMISES self-contradictory?" — a branch PATH CONDITION is
    // not a premise, so EXCLUDE it. Otherwise a provably-DEAD branch (guard contradicts the precondition,
    // `{x>0} ∧ {x<0}`) — which is legitimately unreachable — would be reported unsatisfiable and its
    // obligation spuriously flipped to a "vacuous proof" FAIL. Exclude by MULTISET, not value: remove ONE
    // assumption per guard entry, so a genuine `requires(C)` that lowers to the SAME SMT string as an
    // in-scope guard `if C` keeps its own copy in the vacuity test (else a real requires-contradiction —
    // `requires(x>0) requires(x<0) { if x>0 { … } }` — would be masked because one `x>0` string doubles as
    // the guard). Discharge still uses the full assumptions (guards included).
    let mut remaining_guards: Vec<&String> = obl.guard_assumptions.iter().collect();
    for a in &obl.assumptions {
        if let Some(pos) = remaining_guards.iter().position(|g| *g == a) {
            remaining_guards.remove(pos);
            continue;
        }
        body.push_str(&format!("(assert {})\n", a));
    }
    // Same per-obligation QF_S routing as check_obligations: the `strings` tag covers a quoteless
    // var-vs-var string body the `"`-sniff would otherwise miss.
    let uses_strings = obl.strings || smt_uses_strings(&body);
    let uses_floats = smt_uses_floats(&body);
    let logic = if uses_strings {
        "QF_S"
    } else if uses_floats {
        "QF_FP"
    } else if smt_uses_arrays(&body, &vars) {
        "QF_ABV"
    } else {
        "QF_BV"
    };
    let mut smt = format!("(set-logic {logic})\n");
    for v in &vars {
        if !v.starts_with("bv") && v != "_" && !v.chars().all(|c| c.is_ascii_digit()) {
            smt.push_str(&declare_smt_var_maybe_float(v, uses_strings, uses_floats));
        }
    }
    smt.push_str(&body);
    smt.push_str("(check-sat)\n");
    let out = Command::new("z3")
        .args(Z3_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()
        .and_then(|mut child| {
            child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
            child.wait_with_output().ok()
        })?;
    match String::from_utf8_lossy(&out.stdout).lines().next()?.trim() {
        "sat" => Some(true),
        "unsat" => Some(false),
        _ => None,
    }
}

fn run_z3_obligation_with_smt(obligation: &SolverObligation, smt: String) -> SolverCheck {
    // Optional debug dump of the exact SMT handed to z3. Opt-in (ANUBIS_DUMP_SMT) and written to a
    // per-process path so concurrent `anubis check` runs never clobber a shared /tmp file.
    if std::env::var_os("ANUBIS_DUMP_SMT").is_some() {
        let path = std::env::temp_dir().join(format!("anubis_solver_{}.smt2", std::process::id()));
        let _ = std::fs::write(path, &smt);
    }
    let mut child = match Command::new("z3")
        .args(Z3_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return SolverCheck {
                name: obligation.name.clone(),
                status: "FAIL".into(),
                detail: format!("z3 unavailable: {}", err),
                model: None,
                smt,
            };
        }
    };

    if let Some(stdin) = child.stdin.as_mut() {
        if let Err(err) = stdin.write_all(smt.as_bytes()) {
            return SolverCheck {
                name: obligation.name.clone(),
                status: "FAIL".into(),
                detail: format!("z3 stdin failed: {}", err),
                model: None,
                smt,
            };
        }
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(err) => {
            return SolverCheck {
                name: obligation.name.clone(),
                status: "FAIL".into(),
                detail: format!("z3 execution failed: {}", err),
                model: None,
                smt,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let first = stdout.lines().next().unwrap_or("").trim();
    match first {
        "unsat" => SolverCheck {
            name: obligation.name.clone(),
            status: "PASS".into(),
            detail: "assertion proved: assumptions imply assertion".into(),
            model: None,
            smt,
        },
        "sat" => {
            // Phase-4 B1: every FAIL model must replay. A model that does not re-satisfy the
            // query is an encoder/solver soundness alarm — not a trustworthy counterexample.
            let model = stdout.clone();
            if !replay_counterexample(&smt, &model) {
                SolverCheck {
                    name: obligation.name.clone(),
                    status: "FAIL".into(),
                    detail: "ANUBIS_REPLAY_MISMATCH: z3 returned sat with a model that does not \
                         re-verify under model-substitution replay (encoder-vs-solver soundness \
                         alarm); failing closed"
                        .into(),
                    model: Some(model),
                    smt,
                }
            } else {
                SolverCheck {
                    name: obligation.name.clone(),
                    status: "FAIL".into(),
                    detail: "counterexample satisfies assumptions and negates assertion (replayed)"
                        .into(),
                    model: Some(model),
                    smt,
                }
            }
        }
        // A z3 parse/sort ERROR means the SMT WE emitted is malformed (e.g. an undeclared symbol).
        // That is our bug, not an undecidable query — treat it as FAIL so it fails CLOSED. Emitting a
        // malformed obligation and then calling it "not a disproof" was the fail-OPEN hole that let a
        // parameter named `model`/`set`/`bvx` slip an unverified overflow contract past `check`.
        other if other.starts_with("(error") || stderr.contains("error") => SolverCheck {
            name: obligation.name.clone(),
            status: "FAIL".into(),
            detail: format!(
                "solver rejected the emitted SMT (z3: `{}` stderr `{}`); failing closed — a \
                 malformed obligation is not a proof",
                other,
                stderr.trim()
            ),
            model: None,
            smt,
        },
        // A genuine `unknown` (or empty output) on a well-formed query is NOT a counterexample.
        // Reporting it as FAIL would be an unsound "disproof". A runtime `assert` is still enforced at
        // runtime; QF_BV is decidable, so this branch is effectively unreachable for our obligations.
        other => SolverCheck {
            name: obligation.name.clone(),
            status: "UNKNOWN".into(),
            detail: format!(
                "solver did not decide this obligation (z3 returned `{}` stderr `{}`); not a disproof",
                other,
                stderr.trim()
            ),
            model: None,
            smt,
        },
    }
}

/// Parses a z3 `(get-model)` response into a map from declared variable name to its literal
/// SMT-LIB bit-vector value. Every variable this checker declares is a 64-bit bit-vector
/// (`smt_bv_type(64)`, see `check_obligations`), so each model entry has the fixed shape
/// `(define-fun <name> () (_ BitVec 64) <value>)`, with `<value>` either a `#x…`/`#b…` literal
/// or a `(_ bvN 64)` term, and the whole entry possibly wrapped across lines. Anchoring on the
/// fixed `(_ BitVec 64)` return-type tag (rather than a general-purpose SMT-LIB parser) is
/// sufficient because that invariant already holds everywhere else in this module.
fn parse_z3_model(model: &str) -> BTreeMap<String, String> {
    let mut bindings = BTreeMap::new();
    const MARK: &str = "(define-fun ";
    const TYPE_TAG: &str = "(_ BitVec 64)";
    let mut cursor = model;
    while let Some(rel) = cursor.find(MARK) {
        let after_mark = &cursor[rel + MARK.len()..];
        let Some(name) = after_mark.split_whitespace().next() else {
            break;
        };
        let name = name.to_string();
        let Some(type_rel) = after_mark.find(TYPE_TAG) else {
            break;
        };
        let after_type = after_mark[type_rel + TYPE_TAG.len()..].trim_start();
        let (value, tail) = if let Some(inner) = after_type.strip_prefix('(') {
            // `(_ bvDECIMAL 64)` nested-literal form — capture through its own close paren,
            // then skip the outer define-fun's closing paren.
            match inner.find(')') {
                Some(close) => {
                    let value = format!("({}", &inner[..=close]);
                    let after_value = &inner[close + 1..];
                    let tail = after_value.strip_prefix(')').unwrap_or(after_value);
                    (value, tail)
                }
                None => break,
            }
        } else {
            // `#x…`/`#b…` literal form — capture up to the define-fun's closing paren.
            match after_type.find(')') {
                Some(close) => (
                    after_type[..close].trim().to_string(),
                    &after_type[close + 1..],
                ),
                None => break,
            }
        };
        if !value.is_empty() {
            bindings.insert(name, value);
        }
        cursor = tail;
    }
    bindings
}

/// Runs `smt` through z3 and returns the first line of its stdout (`sat`/`unsat`/`unknown`),
/// or `None` if z3 could not be spawned or its output could not be read.
fn z3_check_sat_raw(smt: &str) -> Option<String> {
    let mut child = Command::new("z3")
        .args(Z3_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
}

/// Real counterexample replay: independent re-verification of a `sat` result, not a trust-the-model
/// string match. `smt` is the EXACT query the solver already decided as `sat` (assumptions ∧
/// ¬assertion, still carrying its own trailing `(check-sat)(get-model)`); `model` is the raw
/// `(get-model)` response z3 returned for it. This parses the concrete witness z3 assigned to each
/// variable, pins every variable to that literal value on top of the SAME assumptions and negated
/// assertion the solver checked, and asks z3 to re-decide the now fully-ground formula.
///
/// A genuine counterexample stays `sat` under its own witness (evaluating a ground formula is
/// decidable, not really "solving"). A bogus, hostile, or internally-inconsistent model — one that
/// doesn't actually satisfy the assumptions, or doesn't actually violate the assertion — makes the
/// ground formula `unsat`, and this returns `false`. Unlike the model text, this does not depend on
/// variable names or on any pre-known "bad" values; it re-derives the answer from the query itself.
pub fn replay_counterexample(smt: &str, model: &str) -> bool {
    let bindings = parse_z3_model(model);
    let base = match smt.find("(check-sat)") {
        Some(idx) => &smt[..idx],
        None => smt,
    };
    // Ground formulas (no free constants) decide `sat` with an empty model `()`. That is a real
    // counterexample: re-check the base query alone. Open formulas must produce parseable BitVec
    // bindings — without them we cannot pin a witness and fail closed (a bare re-check of an open
    // formula would stay `sat` even when the model text was garbage).
    if bindings.is_empty() {
        let has_declare = base.contains("declare-const") || base.contains("declare-fun");
        if has_declare {
            return false;
        }
        // Refuse garbage model text: only z3's empty-model shape (typically `sat` + `()`) qualifies.
        let looks_empty_model = !model.contains("define-fun")
            && (model.contains("sat")
                || model.trim() == "()"
                || model.contains("(\n)")
                || model.contains("( )"));
        if !looks_empty_model {
            return false;
        }
        let mut replay_smt = base.to_string();
        replay_smt.push_str("(check-sat)\n");
        return matches!(z3_check_sat_raw(&replay_smt).as_deref(), Some("sat"));
    }
    let mut replay_smt = base.to_string();
    for (name, value) in &bindings {
        replay_smt.push_str(&format!("(assert (= {name} {value}))\n"));
    }
    replay_smt.push_str("(check-sat)\n");
    matches!(z3_check_sat_raw(&replay_smt).as_deref(), Some("sat"))
}

fn expr_to_smt(e: &Expr, widths: &BTreeMap<String, u32>) -> String {
    expr_to_smt_with_width(e, widths, None)
}

/// The root variable of an assignment place (`x` in `x`, `xs[i]`, `p.f.g`), if any. Used to drop
/// a reassigned binding from the solver's modelable set.
fn assign_target_root(e: &Expr) -> Option<&str> {
    match e {
        Expr::Var(v) => Some(v),
        Expr::Index { base, .. } => assign_target_root(base),
        Expr::FieldAccess { base, .. } => assign_target_root(base),
        _ => None,
    }
}

/// True when `e` is a genuine integer term over solver-modelable variables: an integer literal,
/// a modelable variable, or arithmetic/bitwise composition of such. Used to decide whether an
/// assertion can be soundly encoded in QF_BV — a var that is NOT here (e.g. a string or bool
/// binding) must not be silently treated as a 32-bit integer.
/// A non-zero integer literal — a statically safe divisor for `/` and `%`.
fn is_nonzero_int_literal(e: &Expr) -> bool {
    matches!(e, Expr::Literal(l) if l.parse::<i64>().map(|n| n != 0).unwrap_or(false))
}

/// Sentinel key inserted into `solver_int_vars` to mark a variable as a PROVEN non-zero divisor — a
/// parameter guarded by `requires(v != 0)`/`requires(v > 0)` that the body never reassigns or shadows.
/// The `\u{1}` prefix is not a valid Anubis identifier, so the key can never collide with a real
/// variable and stays inert in the SMT (nothing references it); it only gates variable-divisor modeling.
fn nzdiv_mark(v: &str) -> String {
    format!("\u{1}nzdiv:{v}")
}

/// Sentinel key marking that a modeled INT builtin (`abs`/`min`/`max`/`len`) is SHADOWED in the current
/// function by a user fn or a local (param/`let`) of the same name — the runtime calls THAT, not the
/// builtin, so `is_int_modelable`/`is_strlen_term` must decline to model the call (else a mis-model: a
/// wrong-proof false accept OR an over-rejection of a valid user-fn program). Rides `solver_int_vars` /
/// `solver_string_vars` (like `nzdiv_mark`/`seq_mark`): the `\u{1}` prefix is not a valid identifier, so it
/// never collides with a real var and is never emitted into SMT (it only gates modelability).
fn shadow_builtin_mark(name: &str) -> String {
    format!("\u{1}shadowbuiltin:{name}")
}

/// Phase-4 A2: mark `v` as a **bounded** sequence modelable in QF_ABV (array + length).
fn seq_mark(v: &str) -> String {
    format!("\u{1}seq:{v}")
}

/// Phase-4 A2: fixed length of a modeled sequence (literal-derived, compile-time known).
fn seq_len_mark(v: &str, n: u64) -> String {
    format!("\u{1}seqlen:{v}:{n}")
}

fn is_seq_var(v: &str, int_vars: &BTreeSet<String>) -> bool {
    int_vars.contains(&seq_mark(v))
}

fn seq_fixed_len(v: &str, int_vars: &BTreeSet<String>) -> Option<u64> {
    let prefix = format!("\u{1}seqlen:{v}:");
    int_vars
        .iter()
        .find_map(|m| m.strip_prefix(&prefix)?.parse().ok())
}

/// Drop every solver fact / modelability mark for a binding (int, nzdiv, seq, seqlen).
fn clear_binding_modelability(int_vars: &mut BTreeSet<String>, name: &str) {
    int_vars.remove(name);
    int_vars.remove(&nzdiv_mark(name));
    int_vars.remove(&seq_mark(name));
    let prefix = format!("\u{1}seqlen:{name}:");
    int_vars.retain(|m| !m.starts_with(&prefix));
}

/// Drop ALL solver state for a name that a binding form REBINDS or SHADOWS — a destructuring `let [..]`
/// binder or a `for`/`while let` loop variable: its modelability in every lane (int/float/string), its
/// symbolic width, and every assumption fact mentioning its mangled symbol / seq array / seq length. A
/// stale fact from the PRIOR binding of the same name would otherwise certify a contract over the NEW
/// (differently-valued) binding — a solver FALSE-ACCEPT (hunt-found: `let s="A"; let [s]=["B"];
/// ensures(result=="A")` and `let s="a"; for s in [...] { assert(s=="a") }`). This mirrors the
/// shadow-clear the plain `Stmt::Let` arm already performs; the plain-`let` path additionally re-models
/// the new value, whereas these binder forms leave the new binding unmodeled (fail-closed).
fn invalidate_binding_facts(ctx: &mut SemanticContext, assumptions: &mut Vec<String>, name: &str) {
    clear_binding_modelability(&mut ctx.solver_int_vars, name);
    ctx.solver_float_vars.remove(name);
    ctx.solver_string_vars.remove(name);
    ctx.symbolic_widths.remove(name);
    let mangled = smt_var(name);
    let arr = seq_arr_smt(name);
    let len = seq_len_smt(name);
    assumptions.retain(|a| {
        let mut vs = BTreeSet::new();
        collect_vars_from_smt(a, &mut vs);
        !vs.contains(&mangled) && !vs.contains(&arr) && !vs.contains(&len)
    });
}

/// Membership-only snapshot of a name's solver modelability (NOT its assumption facts — those ride the
/// `assumptions` snapshot the loop already restores via drop_written_after_scope). Used to restore a
/// `for`/`while let` loop variable's OUTER binding after the loop: that shadow is BODY-SCOPED, so after
/// the loop the outer binding is back in scope with its unchanged value and must regain its model,
/// otherwise a post-loop contract over it (or over a transitive dependent) is spuriously rejected. A
/// plain `let` / `LetPattern` shadow is PERMANENT and needs no restore.
struct BindingMembership {
    int: bool,
    nzdiv: bool,
    seq: bool,
    seqlen: Vec<String>,
    float: bool,
    string: bool,
    width: Option<u32>,
}

fn capture_binding_membership(ctx: &SemanticContext, name: &str) -> BindingMembership {
    let seqlen_prefix = format!("\u{1}seqlen:{name}:");
    BindingMembership {
        int: ctx.solver_int_vars.contains(name),
        nzdiv: ctx.solver_int_vars.contains(&nzdiv_mark(name)),
        seq: ctx.solver_int_vars.contains(&seq_mark(name)),
        seqlen: ctx
            .solver_int_vars
            .iter()
            .filter(|m| m.starts_with(&seqlen_prefix))
            .cloned()
            .collect(),
        float: ctx.solver_float_vars.contains(name),
        string: ctx.solver_string_vars.contains(name),
        width: ctx.symbolic_widths.get(name).copied(),
    }
}

fn restore_binding_membership(ctx: &mut SemanticContext, name: &str, m: BindingMembership) {
    if m.int {
        ctx.solver_int_vars.insert(name.to_string());
    }
    if m.nzdiv {
        ctx.solver_int_vars.insert(nzdiv_mark(name));
    }
    if m.seq {
        ctx.solver_int_vars.insert(seq_mark(name));
    }
    for s in m.seqlen {
        ctx.solver_int_vars.insert(s);
    }
    if m.float {
        ctx.solver_float_vars.insert(name.to_string());
    }
    if m.string {
        ctx.solver_string_vars.insert(name.to_string());
    }
    if let Some(w) = m.width {
        ctx.symbolic_widths.insert(name.to_string(), w);
    }
}

fn as_nonneg_int_lit(e: &Expr) -> Option<u64> {
    match e {
        Expr::Literal(l) => l.parse::<i64>().ok().filter(|&n| n >= 0).map(|n| n as u64),
        _ => None,
    }
}

/// Index proven in-range for a fixed-length modeled sequence (non-negative constant only).
/// Negative indices exist at runtime (`anubis_norm_index`) but are NOT modeled — contracts that
/// need them stay fail-closed via `ANUBIS_INDEX_MAYBE_OOB`.
fn index_bounds_proven(seq: &str, index: &Expr, int_vars: &BTreeSet<String>) -> bool {
    match seq_fixed_len(seq, int_vars) {
        Some(n) => as_nonneg_int_lit(index).is_some_and(|i| i < n),
        None => false,
    }
}

fn seq_arr_smt(name: &str) -> String {
    smt_var(&format!("{name}__arr"))
}

fn seq_len_smt(name: &str) -> String {
    smt_var(&format!("{name}__len"))
}

/// If `req` directly guarantees a bare variable is non-zero, return that variable. Recognizes a
/// comparison of a variable against an integer literal whose truth EXCLUDES 0 — `v != 0`, `v > k`
/// (k≥0), `v >= k` (k≥1), `v < k` (k≤0), `v <= k` (k≤−1), and the mirror `k OP v`. Conservative: a
/// form that does NOT exclude 0 (`v >= 0`, `v > -1`, `v != 5`, …) returns None, so an unproven divisor
/// stays fail-closed. Soundness: only a modeled-integer variable is ever marked (see the call site),
/// so this same clause is a modelable assumption in every obligation, and z3 therefore evaluates
/// `bvsdiv`/`bvsrem` only over models where the divisor is non-zero.
fn requires_nonzero_var(req: &Expr) -> Option<String> {
    fn as_var(e: &Expr) -> Option<String> {
        match e {
            Expr::Var(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn as_int_lit(e: &Expr) -> Option<i64> {
        match e {
            Expr::Literal(l) => l.parse::<i64>().ok(),
            // A negative literal often parses as unary minus over a non-negative literal.
            Expr::Unary { op, expr } if op == "-" => match expr.as_ref() {
                Expr::Literal(l) => l.parse::<i64>().ok().map(i64::wrapping_neg),
                _ => None,
            },
            _ => None,
        }
    }
    // `k OP v` is the same relation as `v FLIP(OP) k`.
    fn flip(op: &str) -> &str {
        match op {
            ">" => "<",
            ">=" => "<=",
            "<" => ">",
            "<=" => ">=",
            other => other, // `==`/`!=` are symmetric
        }
    }
    let Expr::Binary { op, lhs, rhs } = req else {
        return None;
    };
    let (var, op, k) = if let (Some(v), Some(k)) = (as_var(lhs), as_int_lit(rhs)) {
        (v, op.as_str(), k)
    } else if let (Some(k), Some(v)) = (as_int_lit(lhs), as_var(rhs)) {
        (v, flip(op.as_str()), k)
    } else {
        return None;
    };
    let excludes_zero = match op {
        "!=" => k == 0,
        ">" => k >= 0,  // v > k ≥ 0  ⟹  v ≥ 1
        ">=" => k >= 1, // v ≥ k ≥ 1
        "<" => k <= 0,  // v < k ≤ 0  ⟹  v ≤ -1
        "<=" => k <= -1,
        _ => false,
    };
    excludes_zero.then_some(var)
}

fn is_int_modelable(e: &Expr, int_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Var(v) => int_vars.contains(v),
        // Only a literal that fits i64 is modelable: the runtime holds integers as i64, and a literal
        // beyond i64::MAX (e.g. 2^64) is parsed as f64 at runtime while `(_ bv… 64)` would silently
        // reduce it mod 2^64 — the solver "proved" `x + 2^64 <= x` because it saw `x + 0`.
        Expr::Literal(l) => !l.is_empty() && l.parse::<i64>().is_ok(),
        Expr::Binary { op, lhs, rhs } => {
            // Ops that model i64 EXACTLY as 64-bit bit-vectors: add/sub/mul (wrap like i64), bitwise
            // and/or/xor, and the shifts `<<`/`>>` (mod-64 mask + arithmetic right shift — see the
            // encoder). `/` and `%` are modelable only with a statically NON-ZERO divisor (a non-zero
            // integer literal): then bvsdiv/bvsrem match wrapping_div/wrapping_rem and never model the
            // runtime's division-by-zero trap. A variable divisor needs a proof it is non-zero first
            // (a later increment), so it stays unmodelable — the contract fails closed, not unsound.
            match op.as_str() {
                "+" | "-" | "*" | "&" | "|" | "^" | "<<" | ">>" => {
                    is_int_modelable(lhs, int_vars) && is_int_modelable(rhs, int_vars)
                }
                // `/`/`%` model soundly (bvsdiv/bvsrem match wrapping_div/wrapping_rem and never model
                // the runtime's division-by-zero trap) only when the divisor is statically non-zero: a
                // non-zero integer literal, or a variable proven non-zero by a `requires` guard (marked
                // via `nzdiv_mark` in `int_vars`). A bare variable with no such guard stays fail-closed.
                "/" | "%" => {
                    is_int_modelable(lhs, int_vars) && divisor_is_proven_nonzero(rhs, int_vars)
                }
                _ => false,
            }
        }
        // Unary negation (`-`) and bitwise NOT (`~`, i.e. `!v` on i64 = -v-1) model exactly.
        Expr::Unary { op, expr } => (op == "-" || op == "~") && is_int_modelable(expr, int_vars),
        // A cast is modelable only when it cannot change the i64 value. `x as u8`/`u16`/`u32` truncate
        // at runtime, so modeling them as the identity is unsound (it "proved" `(x as u8) == x` while
        // `ident8(256)` runs to 0). Only 64-bit-target casts are value-preserving.
        Expr::Cast { expr, ty } => cast_preserves_i64(ty) && is_int_modelable(expr, int_vars),
        // `declassify(x)` forwards x's value, so it is int-modelable iff x is. But `assume(E)`/`assert(E)`
        // in VALUE position evaluate to Bool(true) at runtime (NOT E), so they are never integer-valued —
        // modeling them as E let `return assume(x)` certify `result == x`. They fall through to `false`.
        Expr::Declassify { inner, .. } => is_int_modelable(inner, int_vars),
        // Pure integer builtins that select/negate operands: `abs(x)` (wrapping_abs -> bvneg, wraps at
        // MIN identically), and `min`/`max` of two i64 args (signed `bvsle` select, matching
        // anubis_value_cmp). Only these exact callee/arity shapes; any other call stays unmodelable.
        // Phase-4 A2: `len(xs)` over a bounded modeled sequence (fixed array literal or seq-marked var).
        // #12: a builtin SHADOWED in this function (a user fn / local `let`/param of the same name) is NOT
        // modeled — the runtime calls the shadow, so the builtin semantics would mis-certify. The
        // `shadow_builtin_mark` sentinel rides `int_vars` (inserted per function; never emitted to SMT).
        Expr::Call { callee, .. } if int_vars.contains(&shadow_builtin_mark(callee)) => false,
        Expr::Call { callee, args } => match (callee.as_str(), args.len()) {
            ("abs", 1) => is_int_modelable(&args[0], int_vars),
            ("min", 2) | ("max", 2) => args.iter().all(|a| is_int_modelable(a, int_vars)),
            ("len", 1) => match &args[0] {
                Expr::Var(v) if is_seq_var(v, int_vars) => true,
                Expr::ArrayLiteral { elements }
                    if !elements.is_empty()
                        && elements.iter().all(|e| is_int_modelable(e, int_vars)) =>
                {
                    true
                }
                Expr::ArrayLiteral { elements } if elements.is_empty() => true,
                _ => false,
            },
            _ => false,
        },
        // Phase-4 A2: `xs[i]` only when the base is a bounded modeled seq AND `i` is a proven
        // in-range non-negative constant (else OOB traps at runtime — fail closed, not select).
        Expr::Index { base, index } => match base.as_ref() {
            Expr::Var(v) if is_seq_var(v, int_vars) => {
                is_int_modelable(index, int_vars) && index_bounds_proven(v, index, int_vars)
            }
            Expr::ArrayLiteral { elements }
                if elements.iter().all(|e| is_int_modelable(e, int_vars)) =>
            {
                as_nonneg_int_lit(index).is_some_and(|i| (i as usize) < elements.len())
            }
            _ => false,
        },
        // A struct-LITERAL field read `P{v: e, ..}.v` is value-equal to the named field's expr — the
        // struct analog of the modeled `[a, b][0]` above (leaving it unmodeled let `g(P{v: 0}.v)` certify
        // against `requires(x > 0)`, a hunt-confirmed false accept). Mirroring the array rule, ALL field
        // values must be int-modelable and the accessed name must exist (a missing field is a type error
        // upstream; here it just declines fail-closed). A field access on a struct VAR stays unmodeled —
        // per-field facts need their own symbols + reassignment invalidation (a documented residual).
        // NOTE the walker coupling (see substitute_vars): every shape admitted here MUST be substituted
        // and var-collected — FieldAccess/StructLiteral arms exist in substitute_vars/collect_expr_vars.
        Expr::FieldAccess { base, field, .. } => match base.as_ref() {
            Expr::StructLiteral { fields, .. } => {
                fields.iter().any(|(n, _)| n == field)
                    && fields.iter().all(|(_, v)| is_int_modelable(v, int_vars))
            }
            // A field read `p.field` off a struct VAR is modelable IFF that (base, field) has a
            // registered symbol — i.e. `p` is a struct param whose integer field `field` is constrained
            // by a `requires` (see the has_contract registration). An unregistered field is unmodeled
            // (fail-open), so this can only strengthen checking, never over-reject an unseeded field.
            Expr::Var(v) => int_vars.contains(&mangle_field(v, field)),
            _ => false,
        },
        // A bare array literal is a sequence value, not an integer — not int-modelable.
        _ => false,
    }
}

/// True when `e` is a boolean formula the solver can soundly discharge: a boolean literal, a
/// comparison of integer-modelable terms, or a boolean combination of such. A bare variable, a
/// string comparison, or anything else is NOT modelable — the checker must decline to prove or
/// disprove it (it is still enforced at runtime) rather than fabricate a bit-vector counterexample.
fn is_bool_modelable(e: &Expr, int_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Literal(l) => l == "true" || l == "false",
        Expr::Binary { op, lhs, rhs } => match op.as_str() {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                is_int_modelable(lhs, int_vars) && is_int_modelable(rhs, int_vars)
            }
            "&&" | "||" => is_bool_modelable(lhs, int_vars) && is_bool_modelable(rhs, int_vars),
            _ => false,
        },
        Expr::Unary { op, expr } => op == "!" && is_bool_modelable(expr, int_vars),
        Expr::Declassify { inner, .. } => is_bool_modelable(inner, int_vars),
        // `assume(E)`/`assert(E)` evaluate to Bool(true) at runtime regardless of E, so as a VALUE they
        // are the boolean literal `true` — modelable, but as `true`, never as E (see the encoder).
        Expr::Assume(_) | Expr::Assert(_) => true,
        _ => false,
    }
}

/// Phase-3 QF_FP: is `e` a modelable FLOAT arithmetic term — a float var, a FINITE decimal-representable
/// f64 literal, or `+ - *` (and unary `-`) over those? `/` `%`, casts, and non-finite / scientific-
/// notation literals are excluded (fail-closed). Mirrors `is_int_modelable`, over `solver_float_vars`.
fn is_float_modelable(e: &Expr, float_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Var(v) => float_vars.contains(v),
        // A finite f64 literal that has a plain decimal form: `"NaN"`/`"inf"` both `.parse::<f64>()` OK in
        // Rust (a non-finite literal would break the NaN/inf reasoning), and a very large/small value
        // formats to scientific notation (`1e20`) which is NOT a valid SMT-LIB Real — reject both.
        Expr::Literal(l) => l
            .parse::<f64>()
            .map(|v| v.is_finite() && !format!("{v:?}").contains(['e', 'E']))
            .unwrap_or(false),
        // `/` is included: runtime float division is TOTAL (no trap, unlike integer `/` — run.rs), and
        // `fp.div RNE` is bit-exact to it including x/0 → ±inf/NaN, which the NaN-aware comparison
        // encoding then handles. So the whole "prove divisor non-zero" machinery the QF_BV lane needs is
        // unnecessary here. `%` is still excluded (Rust f64 `%` is fmod, not SMT fp.rem).
        Expr::Binary { op, lhs, rhs } if op == "+" || op == "-" || op == "*" || op == "/" => {
            is_float_modelable(lhs, float_vars) && is_float_modelable(rhs, float_vars)
        }
        Expr::Unary { op, expr } if op == "-" => is_float_modelable(expr, float_vars),
        Expr::Declassify { inner, .. } => is_float_modelable(inner, float_vars),
        // A float field `p.field` off a struct VAR — modelable IFF registered (a struct param's float
        // field constrained by a `requires`). Mirrors the int/string field arms.
        Expr::FieldAccess { base, field, .. } => {
            matches!(base.as_ref(), Expr::Var(v) if float_vars.contains(&mangle_field(v, field)))
        }
        _ => false,
    }
}

/// Phase-3 QF_FP: is `e` a modelable FLOAT boolean formula — a comparison of two float-modelable terms,
/// or `&& || !` over such. Kept SEPARATE from `is_bool_modelable` so the integer gate's signature and its
/// call sites stay untouched; obligation builders try the int gate FIRST (so a pure-int formula never
/// takes the float path), then this.
fn is_bool_modelable_float(e: &Expr, float_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Binary { op, lhs, rhs } => match op.as_str() {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                is_float_modelable(lhs, float_vars) && is_float_modelable(rhs, float_vars)
            }
            "&&" | "||" => {
                is_bool_modelable_float(lhs, float_vars) && is_bool_modelable_float(rhs, float_vars)
            }
            _ => false,
        },
        Expr::Unary { op, expr } => op == "!" && is_bool_modelable_float(expr, float_vars),
        Expr::Declassify { inner, .. } => is_bool_modelable_float(inner, float_vars),
        _ => false,
    }
}

/// Phase-3 QF_FP: encode a float ARITHMETIC term to SMT Float64 `(_ FloatingPoint 11 53)`. Invoked ONLY
/// on `is_float_modelable` exprs, so every reachable case is total. `+ - *` use round-to-nearest-even
/// (RNE), matching the runtime's native f64 ops (run.rs anubis_add/sub/mul); a finite literal is a Real
/// lifted via `to_fp RNE`, with SMT Real negation `(- …)` for a negative value.
fn float_expr_to_smt(e: &Expr) -> String {
    match e {
        Expr::Var(v) => smt_var(v),
        Expr::Literal(l) => {
            let v: f64 = l.parse().unwrap_or(0.0);
            if v.is_sign_negative() {
                format!("((_ to_fp 11 53) RNE (- {:?}))", -v)
            } else {
                format!("((_ to_fp 11 53) RNE {v:?})")
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let l = float_expr_to_smt(lhs);
            let r = float_expr_to_smt(rhs);
            let f = match op.as_str() {
                "+" => "fp.add",
                "-" => "fp.sub",
                "*" => "fp.mul",
                _ => "fp.div",
            };
            format!("({f} RNE {l} {r})")
        }
        Expr::Unary { expr, .. } => format!("(fp.neg {})", float_expr_to_smt(expr)),
        Expr::FieldAccess { base, field, .. } => match base.as_ref() {
            // A registered struct-param float field → its canonical mangled symbol (gated by
            // is_float_modelable's FieldAccess arm, so an unregistered field never reaches here).
            Expr::Var(v) => smt_var(&mangle_field(v, field)),
            _ => "((_ to_fp 11 53) RNE 0.0)".to_string(),
        },
        Expr::Declassify { inner, .. } => float_expr_to_smt(inner),
        _ => "((_ to_fp 11 53) RNE 0.0)".to_string(),
    }
}

/// Phase-3 QF_FP: encode a float BOOLEAN formula to SMT. `<`→fp.lt, `>`→fp.gt, `==`→fp.eq, `!=`→(not
/// fp.eq) already agree with the runtime at NaN. But `<=`/`>=` do NOT: the runtime cmp is
/// `partial_cmp().unwrap_or(Equal)` (run.rs), so `NaN <= c` / `NaN >= c` are TRUE at runtime while IEEE
/// fp.leq/geq(NaN,·) are FALSE — and a bare fp.leq on the ASSUMPTION side would exclude a runtime-reachable
/// NaN input and FALSELY certify a contract the runtime violates. So `<=`/`>=` add the NaN disjunction to
/// match run.rs exactly. (The soundness fix the design workflow flagged.)
fn float_bool_to_smt(e: &Expr) -> String {
    match e {
        Expr::Binary { op, lhs, rhs } => {
            if op == "&&" || op == "||" {
                let l = float_bool_to_smt(lhs);
                let r = float_bool_to_smt(rhs);
                let o = if op == "&&" { "and" } else { "or" };
                return format!("({o} {l} {r})");
            }
            let l = float_expr_to_smt(lhs);
            let r = float_expr_to_smt(rhs);
            match op.as_str() {
                "==" => format!("(fp.eq {l} {r})"),
                "!=" => format!("(not (fp.eq {l} {r}))"),
                "<" => format!("(fp.lt {l} {r})"),
                ">" => format!("(fp.gt {l} {r})"),
                "<=" => format!("(or (fp.leq {l} {r}) (fp.isNaN {l}) (fp.isNaN {r}))"),
                ">=" => format!("(or (fp.geq {l} {r}) (fp.isNaN {l}) (fp.isNaN {r}))"),
                _ => "true".to_string(),
            }
        }
        Expr::Unary { op, expr } if op == "!" => format!("(not {})", float_bool_to_smt(expr)),
        Expr::Declassify { inner, .. } => float_bool_to_smt(inner),
        _ => "true".to_string(),
    }
}

// ── Phase-3 QF_S (string) contract lane ──────────────────────────────────────────────────────────
// The confidentiality/physics-orthogonal third solver lane: an `ensures`/`requires`/`assert` that is a
// STRING EQUALITY (`s == "literal"`, `!=`, `&& || !`) over a `string` param discharges in Z3 QF_S,
// instead of the blanket ANUBIS_STRING_CONTRACT_UNMODELED fall-through. SOUND BOTH WAYS by construction:
// the runtime `==` on strings is exact structural equality (run.rs `AnubisValue::Str(a) == Str(b)`), and
// SMT QF_S `(= a b)` is exact structural equality — there is NO NaN-like partial-order edge case (unlike
// QF_FP's `<=`/`>=`), so no disjunction correction is needed. Scope: an `==`/`!=` between two
// string-modelable terms — literal-anchored OR var-vs-var (`s == t`; the obligation's `strings` sort tag
// routes a quoteless var-var body to QF_S, since the `"`-sniff cannot). `str.len` and concatenation are
// DEFERRED residuals (fail-closed → they stay unmodeled and defer to runtime, never a false accept).

/// Is `e` a modelable STRING term — a string var (in `string_vars`) or a string literal.
fn is_string_modelable(e: &Expr, string_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Var(v) => string_vars.contains(v),
        // A StrLiteral is modelable ONLY over the printable-ASCII domain the QF_S encoder round-trips
        // EXACTLY. A control character is NOT fail-closed as once assumed: z3 TRUNCATES a raw-NUL literal
        // — `(assert (= s "A\0B")) (assert (= s "A"))` is `sat`, collapsing two runtime-distinct strings
        // into a false discharge (hunt-found false-accept). Non-ASCII is excluded too (z3's UTF-8 handling
        // is unvalidated here). Both fall through to fail-closed (deferred to runtime), never a wrong proof.
        Expr::StrLiteral(s) => s.chars().all(|c| c == ' ' || c.is_ascii_graphic()),
        // Phase-3 str.++: `s + t` is runtime string CONCATENATION (run.rs) = SMT `str.++`, sound both ways
        // (concat appends char sequences; z3 str.++ is exact, and str.len(str.++ a b) = len a + len b). Both
        // operands must be string-modelable — a non-ASCII/NUL literal operand fails the StrLiteral arm →
        // the whole concat declines (fail-open/closed), never a byte-wise mis-model.
        Expr::Binary { op, lhs, rhs } if op == "+" => {
            is_string_modelable(lhs, string_vars) && is_string_modelable(rhs, string_vars)
        }
        // Phase-3 str.substr: `substr(s, off, len)` with NONNEG INT LITERAL off/len is `(str.substr s off
        // len)`. Runtime `chars.skip(off).take(len)` = z3 str.substr for off>=0 over printable-ASCII
        // (validated: clamps len past the end, off>=|s|→""). The ONLY divergence — Anubis clamps a NEGATIVE
        // start to 0 while z3 returns "" — is excluded by the nonneg-literal gate; an int-VAR off/len is
        // excluded too (it is BitVec-sorted, str.substr wants Int — no unsound BV↔Int mix). #12: a user/local
        // `substr` shadow declines (the shadow_builtin_mark rides string_vars).
        Expr::Call { callee, args } if callee == "substr" && args.len() == 3 => {
            !string_vars.contains(&shadow_builtin_mark("substr"))
                && is_string_modelable(&args[0], string_vars)
                && as_nonneg_int_lit(&args[1]).is_some()
                && as_nonneg_int_lit(&args[2]).is_some()
        }
        // A string field `p.field` off a struct VAR — modelable IFF registered (a struct param's string
        // field constrained by a `requires`). Mirrors the int/float field arms.
        Expr::FieldAccess { base, field, .. } => {
            matches!(base.as_ref(), Expr::Var(v) if string_vars.contains(&mangle_field(v, field)))
        }
        Expr::Declassify { inner, .. } => is_string_modelable(inner, string_vars),
        _ => false,
    }
}

/// Is `e` a modelable STRING boolean formula — `==`/`!=` between two string-modelable terms (a string
/// param/let var in `string_vars`, or a printable-ASCII StrLiteral), or `&& || !` over such. VAR-vs-VAR
/// equality is now included (it was the deferred residual): both runtime `String==String` and SMT
/// `(= a b)` are exact structural equality, so a var-var comparison is sound and strictly simpler than the
/// literal case. The obligation is tagged `strings: true` at its push site so theory selection routes it
/// to QF_S even though a quoteless var-var body carries no `"`. Tried AFTER the int/float gates (which
/// require int/float membership) so a numeric formula never takes this path.
fn is_bool_modelable_string(
    e: &Expr,
    string_vars: &BTreeSet<String>,
    shadowed_builtins: &BTreeSet<String>,
) -> bool {
    match e {
        Expr::Binary { op, lhs, rhs } => match op.as_str() {
            "==" | "!=" => {
                is_string_modelable(lhs, string_vars) && is_string_modelable(rhs, string_vars)
            }
            "&&" | "||" => {
                is_bool_modelable_string(lhs, string_vars, shadowed_builtins)
                    && is_bool_modelable_string(rhs, string_vars, shadowed_builtins)
            }
            _ => false,
        },
        Expr::Unary { op, expr } => {
            op == "!" && is_bool_modelable_string(expr, string_vars, shadowed_builtins)
        }
        // Phase-3 QF_S string PREDICATES: `contains(s,sub)`/`starts_with(s,p)`/`ends_with(s,p)` are boolean
        // string formulas (runtime Rust `str::contains`/`starts_with`/`ends_with`), modeled as z3
        // `str.contains`/`str.prefixof`/`str.suffixof`. Both args must be string-modelable — a non-ASCII
        // literal arg fails the is_string_modelable gate → the whole predicate declines (fail-open), never a
        // byte-wise mis-model. A string VAR arg is abstract (z3 reasons soundly). Exactly matches over the
        // printable-ASCII domain, as validated at the z3 layer. The `!shadowed_builtins.contains(callee)`
        // guard is the builtin-shadow soundness boundary (the #12 class): the runtime resolves a call in the
        // order LOCAL binding (param / `let`) > top-level user fn > builtin (run.rs), so if the user DEFINES
        // `fn starts_with` OR binds a LOCAL `let contains = …`/`fn f(contains, …)` of that name, the runtime
        // calls THEIRS (which may diverge from the builtin) — modeling it as the z3 predicate would
        // mis-certify (a review-confirmed false accept). `shadowed_builtins` is precomputed per function as
        // exactly the subset of the three names shadowed by all_fns ∪ params ∪ let-bound names, so a shadowed
        // predicate declines (fail-open, as pre-lane). (`len`/`abs`/`min`/`max` carry the same guard via the
        // `shadow_builtin_mark` sentinel on `int_vars`/`string_vars` — #12.)
        Expr::Call { callee, args } => {
            matches!(callee.as_str(), "contains" | "starts_with" | "ends_with")
                && args.len() == 2
                && !shadowed_builtins.contains(callee)
                && is_string_modelable(&args[0], string_vars)
                && is_string_modelable(&args[1], string_vars)
        }
        Expr::Declassify { inner, .. } => {
            is_bool_modelable_string(inner, string_vars, shadowed_builtins)
        }
        _ => false,
    }
}

/// Encode a string term to SMT-LIB: a var → its mangled symbol; a literal → a quoted SMT string with `"`
/// doubled per SMT-LIB. Invoked ONLY on `is_string_modelable` exprs, so every reachable case is total.
fn string_expr_to_smt(e: &Expr) -> String {
    match e {
        Expr::Var(v) => smt_var(v),
        // Escape BACKSLASH before doubling `"`. SMT-LIB doubles `"`, but z3's Unicode-strings theory
        // ALSO decodes `\u{XXXX}`/`\uXXXX` inside a literal — so a runtime string containing a literal
        // `\u{41}` would be re-decoded by z3 to `A`, collapsing two runtime-DISTINCT literals to one SMT
        // value and letting a false contract prove (the soundness hole the review caught). Rewriting every
        // backslash to `\u{5c}` restores it to a single backslash under z3's decoder AND stays injective
        // even if z3 does not decode at all (distinct runtime strings → distinct SMT literals either way),
        // so no `\u`-shaped substring can survive to be mis-decoded.
        Expr::StrLiteral(s) => {
            let escaped = s.replace('\\', "\\u{5c}").replace('"', "\"\"");
            format!("\"{escaped}\"")
        }
        // Phase-3 str.++: string concat (gated by is_string_modelable's Binary `+` arm — both operands
        // string-modelable). Total on reachable exprs; a non-string operand can never arrive here.
        Expr::Binary { op, lhs, rhs } if op == "+" => {
            format!("(str.++ {} {})", string_expr_to_smt(lhs), string_expr_to_smt(rhs))
        }
        // Phase-3 str.substr (gated by is_string_modelable's `substr` arm — off/len are nonneg int
        // literals). The off/len literals encode directly as SMT-LIB Int (nonneg, fit i64).
        Expr::Call { callee, args } if callee == "substr" && args.len() == 3 => {
            format!(
                "(str.substr {} {} {})",
                string_expr_to_smt(&args[0]),
                as_nonneg_int_lit(&args[1]).unwrap_or(0),
                as_nonneg_int_lit(&args[2]).unwrap_or(0)
            )
        }
        Expr::FieldAccess { base, field, .. } => match base.as_ref() {
            // A registered struct-param string field → its canonical mangled symbol (gated by
            // is_string_modelable's FieldAccess arm, so an unregistered field never reaches here).
            Expr::Var(v) => smt_var(&mangle_field(v, field)),
            _ => "\"\"".to_string(),
        },
        Expr::Declassify { inner, .. } => string_expr_to_smt(inner),
        _ => "\"\"".to_string(),
    }
}

/// Encode a string boolean formula. `==` → `(= a b)`, `!=` → `(not (= a b))`, `&& || !` → and/or/not.
fn string_bool_to_smt(e: &Expr) -> String {
    match e {
        Expr::Binary { op, lhs, rhs } => {
            if op == "&&" || op == "||" {
                let l = string_bool_to_smt(lhs);
                let r = string_bool_to_smt(rhs);
                let o = if op == "&&" { "and" } else { "or" };
                return format!("({o} {l} {r})");
            }
            let l = string_expr_to_smt(lhs);
            let r = string_expr_to_smt(rhs);
            if op == "!=" {
                format!("(not (= {l} {r}))")
            } else {
                format!("(= {l} {r})")
            }
        }
        Expr::Unary { expr, .. } => format!("(not {})", string_bool_to_smt(expr)),
        // Phase-3 QF_S string predicates (gated by is_bool_modelable_string's Call arm — callee is one of
        // the three, both args string-modelable). `starts_with(s,p)` / `ends_with(s,p)` swap args for z3's
        // `(str.prefixof p s)` / `(str.suffixof p s)`; `contains(s,sub)` → `(str.contains s sub)`.
        Expr::Call { callee, args }
            if args.len() == 2
                && matches!(callee.as_str(), "contains" | "starts_with" | "ends_with") =>
        {
            let a0 = string_expr_to_smt(&args[0]);
            let a1 = string_expr_to_smt(&args[1]);
            match callee.as_str() {
                "starts_with" => format!("(str.prefixof {a1} {a0})"),
                "ends_with" => format!("(str.suffixof {a1} {a0})"),
                _ => format!("(str.contains {a0} {a1})"),
            }
        }
        Expr::Declassify { inner, .. } => string_bool_to_smt(inner),
        _ => "true".to_string(),
    }
}

/// A `len(s)` term whose `str.len` model is SOUND, encoded as the SMT `str.len` (an Int). The arg must be
/// `is_string_modelable` — a string VAR (abstract: z3 reasons about its length monotonically, matching
/// runtime `chars().count()`) or a PRINTABLE-ASCII string literal. Reusing the sibling gate is deliberate:
/// its printable-ASCII restriction is exactly what `str.len` also needs — a NON-ASCII literal is
/// materialized BYTE-wise by z3 (`str.len("é")`=2 ≠ runtime 1), and a raw-NUL/control literal is TRUNCATED
/// at the NUL (`str.len("ab\0cd")`=2 ≠ runtime 5) — both would mis-model the length. Any weaker gate (e.g.
/// `s.is_ascii()`, which admits NUL) reintroduces the exact false accept the sibling was hardened against.
fn is_strlen_term(e: &Expr, string_vars: &BTreeSet<String>) -> bool {
    // An INT-valued STRING term modeled in QF_S: `len(s)` → `(str.len s)`, or `index_of(s, sub)` →
    // `(str.indexof s sub 0)` (runtime `s.find` → char index / -1, exact vs z3 over printable-ASCII). A
    // string VAR arg is abstract (sound); a non-ASCII literal or a LIST/int arg fails is_string_modelable →
    // declines (the list `index_of`/`len` overload is never mis-modeled). #12: a `len`/`index_of` SHADOWED
    // by a user fn / local declines (the shadow_builtin_mark rides string_vars) — the runtime calls the
    // shadow, so modeling the builtin would mis-certify.
    match e {
        Expr::Call { callee, args } if callee == "len" && args.len() == 1 => {
            !string_vars.contains(&shadow_builtin_mark("len"))
                && is_string_modelable(&args[0], string_vars)
        }
        Expr::Call { callee, args } if callee == "index_of" && args.len() == 2 => {
            !string_vars.contains(&shadow_builtin_mark("index_of"))
                && is_string_modelable(&args[0], string_vars)
                && is_string_modelable(&args[1], string_vars)
        }
        _ => false,
    }
}

/// An operand valid on either side of a string-length comparison: a `len(..)` term, or a NON-NEGATIVE
/// integer literal. A bit-vector int VAR is deliberately EXCLUDED — it is `(_ BitVec 64)`-sorted while this
/// obligation is QF_S/Int, and mixing the two sorts is unsound (z3 errors, or a silent misencoding). So the
/// lane admits `len(s) OP <lit>`, `<lit> OP len(s)`, and `len(s) OP len(t)` — the common, sound subset.
fn is_strlen_int_operand(e: &Expr, string_vars: &BTreeSet<String>) -> bool {
    is_strlen_term(e, string_vars)
        || matches!(e, Expr::Literal(l) if l.parse::<i64>().map(|n| n >= 0).unwrap_or(false))
}

/// A boolean formula comparing string LENGTHS: `len(s) OP k` / `k OP len(s)` / `len(s) OP len(t)` for a
/// numeric comparison OP, plus `&&`/`||`/`!`. At least one side must be a `len(..)` term (else it is a pure
/// literal comparison the int lane owns). Disjoint from `is_bool_modelable_string` (which requires both
/// sides to be string TERMS — a `len(..)` Call is not `is_string_modelable`, and only `==`/`!=` there).
fn is_bool_modelable_strlen(e: &Expr, string_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Binary { op, lhs, rhs } => match op.as_str() {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                (is_strlen_term(lhs, string_vars) || is_strlen_term(rhs, string_vars))
                    && is_strlen_int_operand(lhs, string_vars)
                    && is_strlen_int_operand(rhs, string_vars)
            }
            "&&" | "||" => {
                is_bool_modelable_strlen(lhs, string_vars)
                    && is_bool_modelable_strlen(rhs, string_vars)
            }
            _ => false,
        },
        Expr::Unary { op, expr } => op == "!" && is_bool_modelable_strlen(expr, string_vars),
        Expr::Declassify { inner, .. } => is_bool_modelable_strlen(inner, string_vars),
        _ => false,
    }
}

/// Encode a length-comparison operand to an SMT Int: `len(s)` → `(str.len <s>)`, a literal verbatim.
/// Total on `is_strlen_int_operand` terms (the only ones reachable).
fn strlen_int_to_smt(e: &Expr, string_vars: &BTreeSet<String>) -> String {
    if is_strlen_term(e, string_vars) {
        if let Expr::Call { callee, args } = e {
            if callee == "index_of" {
                return format!(
                    "(str.indexof {} {} 0)",
                    string_expr_to_smt(&args[0]),
                    string_expr_to_smt(&args[1])
                );
            }
            return format!("(str.len {})", string_expr_to_smt(&args[0]));
        }
    }
    if let Expr::Literal(l) = e {
        return l.clone();
    }
    "0".to_string()
}

/// Encode a string-length boolean formula to SMT (Int comparisons over `str.len`, solved in QF_S). Uses the
/// SMT-LIB Int relations `< <= > >=` and `=`; `!=` negates equality; `&& || !` map to and/or/not.
fn strlen_bool_to_smt(e: &Expr, string_vars: &BTreeSet<String>) -> String {
    match e {
        Expr::Binary { op, lhs, rhs } => {
            let l = strlen_int_to_smt(lhs, string_vars);
            let r = strlen_int_to_smt(rhs, string_vars);
            match op.as_str() {
                "==" => format!("(= {l} {r})"),
                "!=" => format!("(not (= {l} {r}))"),
                "<" | "<=" | ">" | ">=" => format!("({op} {l} {r})"),
                "&&" => format!(
                    "(and {} {})",
                    strlen_bool_to_smt(lhs, string_vars),
                    strlen_bool_to_smt(rhs, string_vars)
                ),
                "||" => format!(
                    "(or {} {})",
                    strlen_bool_to_smt(lhs, string_vars),
                    strlen_bool_to_smt(rhs, string_vars)
                ),
                _ => "true".to_string(),
            }
        }
        Expr::Unary { op, expr } if op == "!" => {
            format!("(not {})", strlen_bool_to_smt(expr, string_vars))
        }
        Expr::Declassify { inner, .. } => strlen_bool_to_smt(inner, string_vars),
        _ => "true".to_string(),
    }
}

/// True when every string VAR a strlen obligation references is COVERED by at least one String-lane
/// assumption. Gate for the two previously-FAIL-OPEN strlen obligation sites (body `assert` + call-site
/// `requires@`): when the justifying precondition lives in a lane that CANNOT be seeded (a non-ASCII
/// string-equality literal `requires(s == "é")`, an int-var length bound `requires(len(s) >= n)`), the
/// obligation would carry ZERO assumptions and z3 disproves it with the spurious `s = ""` — over-rejecting
/// a valid program that fail-open-accepted before the lane (review-caught regression). Uncovered → the
/// caller SKIPS the obligation (fail-open, the pre-lane behavior; a body assert stays runtime-enforced).
/// A VAR-LESS obligation (a printable-ASCII literal, `len("ab") >= 3`) is self-contained and stays
/// modeled. NOT applied to the `ensures` site: that site fails CLOSED when unmodeled (pre-lane rejected
/// every strlen ensures), and its tautology case (`ensures(len(result) >= len(s))` over `return s`)
/// legitimately proves with no assumptions — gating it would regress a completeness gain.
/// True when a seeded SMT fact is a string PREDICATE fact (`str.contains`/`str.prefixof`/`str.suffixof`).
/// Such a fact mentions its string var but does NOT tightly constrain its LENGTH (`contains(s,"f")` only
/// implies `len(s) >= 1`), so it must be EXCLUDED from a str.len obligation's assumptions + coverage: a
/// predicate fact would otherwise "cover" a strlen var (defeating the strlen coverage gate) yet leave the
/// obligation stranded against a tighter bound whose real justification is unseeable (a non-ASCII
/// `requires`) → over-rejection. Excluding predicate facts keeps the strlen lane behaving exactly as it did
/// before the predicate lane existed. (Predicate facts still cover PREDICATE obligations — that gate does
/// not filter them.)
fn is_predicate_fact(a: &str) -> bool {
    a.contains("str.contains") || a.contains("str.prefixof") || a.contains("str.suffixof")
}

/// True when `e` is a PURELY WEAK-consequence string obligation — built only from string predicates
/// (`contains`/`starts_with`/`ends_with`), SUBSTR-equalities (`substr(s,..) == lit`), and `&&`/`||`/`!`.
/// A weak consequence constrains only PART of a string (a prefix/containment/substring), so an uncovered
/// one (an unseeable justification like a non-ASCII `requires`) must fail-OPEN rather than strand against a
/// free var and over-reject a valid program. A PLAIN var/literal equality PIN (`s == "hi"`) is NOT weak:
/// its uncovered reject is correct (an ASCII pin under an unseeable premise is genuinely runtime-false), and
/// gating it would fail-open a false accept. A MIXED `s == "hi" && starts_with(s,"h")` is NOT pure (the
/// plain `==` conjunct → false), so it keeps the equality lane's fail-closed behavior.
fn is_pure_string_predicate(e: &Expr) -> bool {
    match e {
        Expr::Call { callee, args } => {
            args.len() == 2 && matches!(callee.as_str(), "contains" | "starts_with" | "ends_with")
        }
        // A `==`/`!=` is weak (gateable) ONLY when a side is a DERIVED substring term (`substr(..)`); a
        // plain var/literal equality pin is not, and stays fail-closed.
        Expr::Binary { op, lhs, rhs } if op == "==" || op == "!=" => {
            expr_contains_substr(lhs) || expr_contains_substr(rhs)
        }
        Expr::Binary { op, lhs, rhs } if op == "&&" || op == "||" => {
            is_pure_string_predicate(lhs) && is_pure_string_predicate(rhs)
        }
        Expr::Unary { op, expr } if op == "!" => is_pure_string_predicate(expr),
        Expr::Declassify { inner, .. } => is_pure_string_predicate(inner),
        _ => false,
    }
}

/// True when `e` contains a `substr(s, off, len)` call anywhere — used to classify a `==` as a weak
/// (coverage-gateable) substring consequence rather than a fail-closed pin.
fn expr_contains_substr(e: &Expr) -> bool {
    match e {
        Expr::Call { callee, args } if callee == "substr" && args.len() == 3 => true,
        Expr::Call { args, .. } => args.iter().any(expr_contains_substr),
        Expr::Binary { lhs, rhs, .. } => {
            expr_contains_substr(lhs) || expr_contains_substr(rhs)
        }
        Expr::Unary { expr, .. } => expr_contains_substr(expr),
        Expr::Declassify { inner, .. } => expr_contains_substr(inner),
        _ => false,
    }
}

fn strlen_vars_covered(
    concrete: &Expr,
    str_asm: &[String],
    string_vars: &BTreeSet<String>,
) -> bool {
    let mut raw = BTreeSet::new();
    collect_expr_vars(concrete, &mut raw);
    let referenced: Vec<&String> = raw.iter().filter(|v| string_vars.contains(*v)).collect();
    if referenced.is_empty() {
        return true;
    }
    let mut mentioned = BTreeSet::new();
    for a in str_asm {
        collect_vars_from_smt(a, &mut mentioned);
    }
    referenced.iter().all(|v| mentioned.contains(&smt_var(v)))
}

/// Seed a `requires` as a solver ASSUMPTION in whichever lane its predicate is modelable (int QF_BV / float
/// QF_FP / string-equality QF_S / string-length QF_S). A top-level `&&` is DECOMPOSED so a MIXED-lane
/// compound seeds each conjunct SEPARATELY: `requires(len(s) >= 3 && s == "abc")` matches no single lane as
/// a whole (the `&&` arm of each `is_bool_modelable_*` requires both sides in that ONE lane), so without
/// decomposition NEITHER conjunct is assumed and a body that chains on one (`assert(len(s) >= 1)`) is
/// spuriously rejected. `A && B` ⟺ assuming A and assuming B, so this is sound and, for a homogeneous
/// compound, identical to seeding the whole formula. An unmodelable conjunct is simply skipped (fail-open).
fn seed_requires_fact(ctx: &mut SemanticContext, assumptions: &mut Vec<String>, req: &Expr) {
    if let Expr::Binary { op, lhs, rhs } = req {
        if op.as_str() == "&&" {
            seed_requires_fact(ctx, assumptions, lhs);
            seed_requires_fact(ctx, assumptions, rhs);
            return;
        }
    }
    if is_bool_modelable(req, &ctx.solver_int_vars) {
        assumptions.push(expr_to_smt(req, &ctx.symbolic_widths));
    } else if is_bool_modelable_float(req, &ctx.solver_float_vars) {
        // Phase-3 QF_FP: the `<=`/`>=` NaN disjunction in float_bool_to_smt keeps a runtime-reachable NaN
        // input IN the assumed set, matching run.rs — a bare fp.leq would wrongly exclude it and over-certify.
        assumptions.push(float_bool_to_smt(req));
    } else if is_bool_modelable_string(req, &ctx.solver_string_vars, &ctx.shadowed_string_preds) {
        assumptions.push(string_bool_to_smt(req));
    } else if is_bool_modelable_strlen(req, &ctx.solver_string_vars) {
        assumptions.push(strlen_bool_to_smt(req, &ctx.solver_string_vars));
    }
}

/// Whether an obligation body is a QF_S (string) query — detected by an SMT string literal (`"`), which
/// ONLY the string encoder emits (int/float encoders never produce `"`). A quoteless VAR-vs-VAR string
/// obligation is invisible to this sniff — its push site tags the obligation `strings: true`, and both
/// theory-selection sites OR the tag in (`obl.strings || smt_uses_strings(..)`).
fn smt_uses_strings(smt_body: &str) -> bool {
    smt_body.contains('"')
}

/// Whether a fact in the shared `assumptions` channel is a STRING fact — it mentions a currently
/// string-modelable variable (mangled `anb_v`). Membership (not a `"` sniff) so the int/float obligation
/// filters can drop a string fact rather than pull a `String`-sorted symbol into a QF_BV/QF_FP body.
fn fact_is_string(smt: &str, string_vars: &BTreeSet<String>) -> bool {
    // A `str.len` term or a string literal is String-theory even when the fact mentions NO string VAR — a
    // constant seed like `(= (str.len "abc") 3)` (from `requires(len("abc") == 3)`) has no var, yet it must
    // NOT leak into a bit-vector int/float obligation: the `"` would re-route that query to QF_S and the
    // int var would be mis-declared `String` → z3 sort error → fail-closed over-rejection. Only the string
    // encoders emit `str.len`/`"`, so this is a precise String-lane marker.
    if smt.contains("str.len") || smt.contains('"') {
        return true;
    }
    if string_vars.is_empty() {
        return false;
    }
    let mut vs = BTreeSet::new();
    collect_vars_from_smt(smt, &mut vs);
    string_vars.iter().any(|v| vs.contains(&smt_var(v)))
}

fn expr_to_smt_value(e: &Expr, widths: &BTreeMap<String, u32>) -> Option<String> {
    match e {
        Expr::Var(v) if widths.contains_key(v) => Some(smt_var(v)),
        Expr::Literal(l) if !l.is_empty() && l.parse::<i64>().is_ok() => {
            Some(expr_to_smt(e, widths))
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_to_smt_value(lhs, widths)?;
            expr_to_smt_value(rhs, widths)?;
            Some(expr_to_smt(e, widths))
        }
        Expr::Unary { op, expr } if op == "-" || op == "!" || op == "~" => {
            expr_to_smt_value(expr, widths)?;
            Some(expr_to_smt(e, widths))
        }
        // A modeled builtin value: `let y = abs(x)` (or min/max) MUST emit its defining fact `y == abs(x)`
        // via the shared encoder — otherwise `is_int_modelable` admits `y` into the modelable set as a
        // FREE variable and a later contract over `y` is checked against an unconstrained symbol (a false
        // ALARM). Mirror the exact callee/arity set is_int_modelable admits, so the two never diverge.
        Expr::Call { callee, args }
            if (callee == "abs" && args.len() == 1)
                || ((callee == "min" || callee == "max") && args.len() == 2)
                || (callee == "len" && args.len() == 1) =>
        {
            for a in args {
                expr_to_smt_value(a, widths)?;
            }
            Some(expr_to_smt(e, widths))
        }
        // Phase-4 A2: bounded index read → `(select arr idx)` (or unfolded literal element).
        Expr::Index { base, index } => {
            expr_to_smt_value(index, widths)?;
            match base.as_ref() {
                Expr::Var(_) => Some(expr_to_smt(e, widths)),
                Expr::ArrayLiteral { elements } => {
                    for el in elements {
                        expr_to_smt_value(el, widths)?;
                    }
                    Some(expr_to_smt(e, widths))
                }
                _ => None,
            }
        }
        // A struct-literal field read: the accessed field must EXIST (symmetry with is_int_modelable —
        // without it a missing-field access would mint an inert-but-wrong `== 0` fact) and every field
        // value must itself encode (mirrors the array-literal rule above); the encoding unfolds to the
        // named field's value (see expr_to_smt_with_width).
        Expr::FieldAccess { base, field, .. } => match base.as_ref() {
            Expr::StructLiteral { fields, .. } => {
                if !fields.iter().any(|(n, _)| n == field) {
                    return None;
                }
                for (_, v) in fields {
                    expr_to_smt_value(v, widths)?;
                }
                Some(expr_to_smt(e, widths))
            }
            // A registered struct-param integer field → its canonical symbol (present in `widths` with
            // width 64 when registered; an unregistered field declines, so the anti-launder value check
            // stays fail-open on it).
            Expr::Var(v) if widths.contains_key(&mangle_field(v, field)) => {
                Some(smt_var(&mangle_field(v, field)))
            }
            _ => None,
        },
        Expr::Cast { expr, ty } => {
            // A TRUNCATING cast (`x as u8`) has NO sound integer value fact — modeling it as the
            // identity recorded a false `y == x` that a loop invariant could later force-model and
            // "prove" against the pre-truncation value. Only a value-preserving (64-bit) cast keeps
            // the inner's value. (Mirrors `is_int_modelable`'s cast rule.)
            if !cast_preserves_i64(ty) {
                return None;
            }
            expr_to_smt_value(expr, widths)
        }
        Expr::Declassify { inner, .. } => expr_to_smt_value(inner, widths),
        // `assume(E)`/`assert(E)` as a value are Bool(true) at runtime, not E (see is_int_modelable).
        Expr::Assume(_) | Expr::Assert(_) => Some("true".to_string()),
        _ => None,
    }
}

#[allow(clippy::only_used_in_recursion)]
fn expr_to_smt_with_width(
    e: &Expr,
    widths: &BTreeMap<String, u32>,
    expected_width: Option<u32>,
) -> String {
    match e {
        Expr::Var(v) => smt_var(v),
        // Boolean literals are SMT `Bool`, not bit-vectors: emitting `(_ bvtrue 32)` produced the
        // Z3 error "unknown constant bvtrue" that made `check` reject `assert(true)`.
        Expr::Literal(l) if l == "true" || l == "false" => l.clone(),
        // The runtime represents EVERY integer as i64 (type annotations like u8/u32 are inert —
        // plain arithmetic never wraps to the annotated width, e.g. `let x: u8 = 200; x + 100` is
        // 300, not 44). So integers are modeled as 64-bit bit-vectors with SIGNED comparisons,
        // matching i64 exactly. A 32-bit unsigned model was unsound: it "disproved" true statements
        // like `65536 * 65536 != 0` (wrapped to 0) and `0 - 1 < 0` (unsigned bvult).
        Expr::Literal(l) => format!("(_ bv{} 64)", l),
        Expr::Binary { op, lhs, rhs } => {
            // Logical connectives combine Bool operands, not bit-vectors.
            if op == "&&" || op == "||" {
                let l = expr_to_smt_with_width(lhs, widths, None);
                let r = expr_to_smt_with_width(rhs, widths, None);
                let smt_op = if op == "&&" { "and" } else { "or" };
                return format!("({} {} {})", smt_op, l, r);
            }
            let l = expr_to_smt_with_width(lhs, widths, Some(64));
            let r = expr_to_smt_with_width(rhs, widths, Some(64));
            match op.as_str() {
                "+" => format!("(bvadd {} {})", l, r),
                "-" => format!("(bvsub {} {})", l, r),
                "*" => format!("(bvmul {} {})", l, r),
                "&" => format!("(bvand {} {})", l, r),
                "|" => format!("(bvor {} {})", l, r),
                "^" => format!("(bvxor {} {})", l, r),
                "==" => format!("(= {} {})", l, r),
                "!=" => format!("(not (= {} {}))", l, r),
                "<" => format!("(bvslt {} {})", l, r),
                "<=" => format!("(bvsle {} {})", l, r),
                ">" => format!("(bvsgt {} {})", l, r),
                ">=" => format!("(bvsge {} {})", l, r),
                // Shifts mask the shift amount mod 64 (matching the runtime's `rem_euclid(64)`, which
                // equals the low 6 bits via unsigned `bvurem`), and `>>` is ARITHMETIC — the runtime
                // uses `i64::wrapping_shr`, which sign-extends. `bvlshr` (logical) would be UNSOUND
                // (it would "prove" `(-8 >> 1) == 4` while the program computes -4).
                "<<" => format!("(bvshl {} (bvurem {} (_ bv64 64)))", l, r),
                ">>" => format!("(bvashr {} (bvurem {} (_ bv64 64)))", l, r),
                // Division/modulo, reached only with a non-zero literal divisor (is_int_modelable).
                // bvsdiv = truncated toward zero and bvsdiv(MIN,-1)=MIN, matching i64::wrapping_div;
                // bvsrem takes the sign of the dividend, matching i64::wrapping_rem (NOT bvsmod,
                // which takes the sign of the divisor).
                "/" => format!("(bvsdiv {} {})", l, r),
                "%" => format!("(bvsrem {} {})", l, r),
                _ => format!("({} {} {})", op, l, r),
            }
        }
        Expr::Unary { op, expr } => {
            let inner = expr_to_smt_with_width(expr, widths, expected_width);
            match op.as_str() {
                "-" => format!("(bvneg {})", inner),
                "~" => format!("(bvnot {})", inner),
                "!" => format!("(not {})", inner),
                _ => inner,
            }
        }
        Expr::Cast { expr, ty } => expr_to_smt_with_width(expr, widths, Some(bitwidth_of(ty))),
        Expr::Declassify { inner, .. } => expr_to_smt_with_width(inner, widths, expected_width),
        // `assume(E)`/`assert(E)` in value position evaluate to Bool(true) at runtime, not E.
        Expr::Assume(_) | Expr::Assert(_) => "true".to_string(),
        // `abs`/`min`/`max` builtins as ite (vetted by is_int_modelable, so the shapes always match).
        // `abs`: `bvneg` wraps at MIN exactly like `wrapping_abs`. `min`/`max`: signed `bvsle` select,
        // matching `anubis_value_cmp`'s i64 ordering (min picks the smaller, max the larger).
        Expr::Call { callee, args } if callee == "abs" && args.len() == 1 => {
            let x = expr_to_smt_with_width(&args[0], widths, Some(64));
            format!("(ite (bvslt {x} (_ bv0 64)) (bvneg {x}) {x})")
        }
        Expr::Call { callee, args } if (callee == "min" || callee == "max") && args.len() == 2 => {
            let a = expr_to_smt_with_width(&args[0], widths, Some(64));
            let b = expr_to_smt_with_width(&args[1], widths, Some(64));
            if callee == "min" {
                format!("(ite (bvsle {a} {b}) {a} {b})")
            } else {
                format!("(ite (bvsle {a} {b}) {b} {a})")
            }
        }
        // Phase-4 A2: `len(xs)` → length symbol (or ground length of a literal).
        Expr::Call { callee, args } if callee == "len" && args.len() == 1 => match &args[0] {
            Expr::Var(v) => seq_len_smt(v),
            Expr::ArrayLiteral { elements } => format!("(_ bv{} 64)", elements.len()),
            _ => "(_ bv0 64)".into(),
        },
        // Phase-4 A2: `xs[i]` → `(select arr i)` or unfolded constant element of a literal.
        Expr::Index { base, index } => match base.as_ref() {
            Expr::Var(v) => {
                let arr = seq_arr_smt(v);
                let idx = expr_to_smt_with_width(index, widths, Some(64));
                format!("(select {arr} {idx})")
            }
            Expr::ArrayLiteral { elements } => {
                if let Some(i) = as_nonneg_int_lit(index) {
                    if (i as usize) < elements.len() {
                        return expr_to_smt_with_width(
                            &elements[i as usize],
                            widths,
                            expected_width,
                        );
                    }
                }
                // Fallback (should be unreachable when gated by is_int_modelable): const array.
                let mut acc =
                    "((as const (Array (_ BitVec 64) (_ BitVec 64))) (_ bv0 64))".to_string();
                for (i, el) in elements.iter().enumerate() {
                    let es = expr_to_smt_with_width(el, widths, Some(64));
                    acc = format!("(store {acc} (_ bv{i} 64) {es})");
                }
                let idx = expr_to_smt_with_width(index, widths, Some(64));
                format!("(select {acc} {idx})")
            }
            _ => "(_ bv0 64)".into(),
        },
        // A struct-literal field read unfolds to the NAMED field's value expr (value-equal at runtime;
        // gated by is_int_modelable, which requires the field to exist and all values int-modelable).
        Expr::FieldAccess { base, field, .. } => match base.as_ref() {
            Expr::StructLiteral { fields, .. } => fields
                .iter()
                .find(|(n, _)| n == field)
                .map(|(_, v)| expr_to_smt_with_width(v, widths, expected_width))
                .unwrap_or_else(|| "(_ bv0 64)".into()),
            // A registered struct-param integer field → its canonical symbol (only reached for a
            // registered field, since is_int_modelable gates the obligation upstream).
            Expr::Var(v) if widths.contains_key(&mangle_field(v, field)) => {
                smt_var(&mangle_field(v, field))
            }
            _ => "(_ bv0 64)".into(),
        },
        Expr::TaintSource { label } => format!("taint_source_{}", label.replace("\"", "")),
        Expr::Symbolic { .. } => "symbolic".into(),
        _ => "true".into(),
    }
}

fn collect_vars_from_smt(smt: &str, vars: &mut BTreeSet<String>) {
    for token in smt.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if token.is_empty()
            || token.chars().all(|c| c.is_ascii_digit())
            || matches!(
                token,
                "and"
                    | "or"
                    | "not"
                    | "ite"
                    | "true"
                    | "false"
                    | "Int"
                    | "bvadd"
                    | "bvmul"
                    | "bvand"
                    | "bvult"
                    | "bvugt"
                    | "bvule"
                    | "bvuge"
                    | "bvsub"
                    | "bvshl"
                    | "bvashr"
                    | "bvlshr"
                    | "bvsdiv"
                    | "bvsrem"
                    | "bvsmod"
                    | "bvslt"
                    | "bvsle"
                    | "bvsgt"
                    | "bvsge"
                    | "bvneg"
                    | "bvnot"
                    | "bvxor"
                    | "bvor"
                    | "bvurem"
                    | "select"
                    | "store"
                    | "Array"
                    | "BitVec"
                    | "as"
                    | "set"
                    | "logic"
                    | "QF_BV"
                    | "QF_ABV"
                    | "declare"
                    | "const"
                    | "check"
                    | "sat"
                    | "get"
                    | "model"
                    | "define"
                    | "fun"
                    | "_"
            )
        {
            continue;
        }
        if token.starts_with("bv") || token == "_" {
            continue;
        }
        vars.insert(token.to_string());
    }
}

/// Declare free symbols for an obligation. BitVec-64 by default; `*__arr` symbols are
/// `(Array (_ BitVec 64) (_ BitVec 64))` (Phase-4 A2 QF_ABV sequences).
fn declare_smt_var(v: &str) -> String {
    if v.ends_with("__arr") {
        format!("(declare-const {v} (Array (_ BitVec 64) (_ BitVec 64)))\n")
    } else {
        format!("(declare-const {v} {})\n", smt_bv_type(64))
    }
}

/// Declare a symbol for the per-obligation theory: `String` for QF_S, IEEE-754 Float64 for QF_FP, else
/// the integer/array declaration for QF_BV/QF_ABV. A modeled obligation is single-sort (the modelability
/// gates are mutually exclusive — a symbol is never both), so every non-literal symbol takes the same
/// sort as the obligation's theory.
fn declare_smt_var_maybe_float(v: &str, is_string: bool, is_float: bool) -> String {
    if is_string {
        format!("(declare-const {v} String)\n")
    } else if is_float {
        format!("(declare-const {v} (_ FloatingPoint 11 53))\n")
    } else {
        declare_smt_var(v)
    }
}

/// Whether an obligation's SMT body is a QF_FP (float) query — detected by the float operators the float
/// encoder emits (`fp.` / `to_fp`), mirroring how `smt_uses_arrays` detects the array sort from the body.
fn smt_uses_floats(smt_body: &str) -> bool {
    smt_body.contains("fp.") || smt_body.contains("to_fp")
}

/// Whether a fact/assumption in the shared `assumptions` channel is a FLOAT fact — it mentions at least
/// one currently float-modelable variable (the mangled `anb_v` for some `v` in `solver_float_vars`).
///
/// Used to sort-partition `assumptions` at every obligation build (Phase-3 QF_FP `let` chaining): a
/// float (QF_FP) obligation assumes only float facts and an integer (QF_BV) obligation assumes only
/// integer facts. This is sound because a fact never mixes sorts — `is_int_modelable` /
/// `is_float_modelable` each reject a mixed expression, and a symbol is EITHER an i64 bit-vector OR a
/// Float64, never both. Membership (not an `fp.`/`to_fp` substring sniff) is the robust test: a plain
/// float copy `let y = x` encodes as `(= anb_y anb_x)` with no float operator token, which a substring
/// sniff would misclassify as integer. `collect_vars_from_smt` over-scrapes the `fp.`/`RNE`/`to_fp`
/// operator tokens, but those are never `anb_`-mangled so they never equal an `smt_var(v)` — harmless.
fn fact_is_float(smt: &str, float_vars: &BTreeSet<String>) -> bool {
    if float_vars.is_empty() {
        return false;
    }
    let mut vs = BTreeSet::new();
    collect_vars_from_smt(smt, &mut vs);
    float_vars.iter().any(|v| vs.contains(&smt_var(v)))
}

fn smt_uses_arrays(smt_body: &str, vars: &BTreeSet<String>) -> bool {
    smt_body.contains("(select ")
        || smt_body.contains("(store ")
        || vars.iter().any(|v| v.ends_with("__arr"))
}

fn first_fn_body(items: &[Item]) -> Option<Vec<Stmt>> {
    for item in items {
        match item {
            Item::Fn { body, .. } => return Some(body.clone()),
            Item::Module { items, .. } => {
                if let Some(body) = first_fn_body(items) {
                    return Some(body);
                }
            }
            Item::Import { .. } => {}
            Item::Struct { .. } => {}
            Item::Enum { .. } => {}
            Item::Impl { .. } => {}
            Item::Trait { .. } => {}
        }
    }
    None
}

fn count_stmts(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                1 + count_stmts(body)
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                1 + [gpu, cpu, prove]
                    .into_iter()
                    .flatten()
                    .map(|body| count_stmts(body))
                    .sum::<usize>()
            }
            Stmt::If { then, else_, .. } => {
                1 + count_stmts(then) + else_.as_deref().map(count_stmts).unwrap_or(0)
            }
            _ => 1,
        })
        .sum()
}

fn type_has_raw_pointer(ty: Option<&str>) -> bool {
    ty.is_some_and(|ty| ty.contains('*') || ty.contains("rawptr") || ty.contains("RawPtr"))
}

/// Whether a declared type annotation carries the `tainted<T>` qualifier. Delegates to the anchored
/// `ty::is_tainted` rather than a bare `.contains("tainted")` substring test — the substring version
/// this replaced false-positived on any type merely NAMED with that substring (e.g. a struct called
/// `TaintedRecord` would have been wrongly seeded as tainted).
fn is_tainted_type(ty: Option<&str>) -> bool {
    ty.is_some_and(ty::is_tainted)
}

/// Whether a declared type annotation carries the `secret<T>` qualifier — the confidentiality dual of
/// [`is_tainted_type`]. Delegates to the anchored `ty::is_secret` (`secret<`, not a bare `secret`
/// substring) so a `secret_key` binding or a `SecretManager` struct is not mis-seeded as secret.
fn is_secret_type(ty: Option<&str>) -> bool {
    ty.is_some_and(ty::is_secret)
}

/// The expression a function returns at its tail: the last statement when it is `return X` or a bare
/// value expression. None when the body ends in a statement (which yields the default `0`).
/// Collect the IMPLICIT tail values a function body (or an `if`-arm) can yield: every branch of a
/// tail `if`/`match`, a block's tail expression, or the literal `0` when the body falls off the end
/// (ends in a `let`/assign/loop, or a tail `if` with no `else`). Without this, a function whose body
/// is a bare tail `if/else` (the idiomatic tail expression) has its `ensures` obligated at ZERO points
/// and is silently certified. `collect_tail_return` collects a bare tail `return X` value here (true
/// at the function level, where the early-return scan excludes it); inside an `if`-arm it is false
/// (the early-return scan already covers those explicit returns — avoids a double, weaker check).
fn tail_values(body: &[Stmt], collect_tail_return: bool, out: &mut Vec<Expr>) {
    match body.last() {
        None => out.push(zero_literal()),
        Some(Stmt::ExprStmt(Expr::Call { callee, args })) if callee == "return" => {
            if collect_tail_return {
                if let Some(e) = args.first() {
                    out.push(e.clone());
                }
            }
        }
        Some(Stmt::ExprStmt(e)) => expr_tail_values(e, out),
        Some(Stmt::If { then, else_, .. }) => {
            tail_values(then, false, out);
            match else_ {
                Some(e) => tail_values(e, false, out),
                None => out.push(zero_literal()),
            }
        }
        // A tail `let`/assign/`while`/`for`/`loop`/etc. yields the default `0`.
        Some(_) => out.push(zero_literal()),
    }
}

/// The tail values of an expression in value position (an `if`/`match`/block used as a tail value).
fn expr_tail_values(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
            expr_tail_values(then, out);
            expr_tail_values(else_, out);
        }
        Expr::Match { arms, .. } => {
            for a in arms {
                expr_tail_values(&a.body, out);
            }
        }
        Expr::Block { stmts, tail } => match tail {
            Some(t) => expr_tail_values(t, out),
            None => tail_values(stmts, false, out),
        },
        // An explicit `return` in expression position is handled by the early-return scan.
        Expr::Call { callee, .. } if callee == "return" => {}
        other => out.push(other.clone()),
    }
}

fn zero_literal() -> Expr {
    Expr::Literal("0".to_string())
}

/// Substitute variables in a contract expression by name. Used to specialize a callee's contract at
/// a call site (`result` -> the returned expression, each parameter -> its argument), so
/// `ensures(result > x)` over `return x + 1` becomes `(x + 1) > x`, and a caller's
/// `ensures(result > 0)` with `x := 5` at `let a = f(5)` becomes `a > 0`.
fn substitute_vars(e: &Expr, map: &BTreeMap<String, Expr>) -> Expr {
    match e {
        Expr::Var(v) => map.get(v).cloned().unwrap_or_else(|| e.clone()),
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: op.clone(),
            lhs: Box::new(substitute_vars(lhs, map)),
            rhs: Box::new(substitute_vars(rhs, map)),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: op.clone(),
            expr: Box::new(substitute_vars(expr, map)),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(substitute_vars(expr, map)),
            ty: ty.clone(),
        },
        // SOUNDNESS-CRITICAL: substitute_vars MUST recurse into every form that `is_int_modelable`/
        // `is_bool_modelable` admit. Composition substitutes the callee's params/`result` into its
        // contract; if a MODELABLE subterm is cloned WITHOUT substituting, its callee-parameter names
        // survive and re-bind to the caller's scope — a precondition bypass + a certified-false
        // postcondition (e.g. `g(150)` against `requires(abs(x) < 100)` was checked as `abs(5) < 100`).
        // The modelable set is exactly: Var/Literal/Binary/Unary/Cast (above) + the abs/min/max/len
        // builtin Call + Index (bounded seq) + Declassify/Assume/Assert. Any NON-modelable form is
        // safely cloned by the catch-all: a mapped var hidden inside it makes the whole contract
        // non-modelable, so the gate rejects it fail-closed rather than mis-model it. If a new
        // modelable form is added to the gate, ADD IT HERE.
        Expr::Call { callee, args } => Expr::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| substitute_vars(a, map)).collect(),
        },
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(substitute_vars(base, map)),
            index: Box::new(substitute_vars(index, map)),
        },
        Expr::ArrayLiteral { elements } => Expr::ArrayLiteral {
            elements: elements.iter().map(|a| substitute_vars(a, map)).collect(),
        },
        // Struct-literal field reads joined the modelable set (`P{v: x}.v` — see is_int_modelable), so
        // BOTH forms must substitute: a cloned-unsubstituted callee-param name inside a now-modelable
        // contract would re-bind to the caller's scope (the precondition-bypass class this comment block
        // warns about).
        Expr::FieldAccess { base, field, span } => Expr::FieldAccess {
            base: Box::new(substitute_vars(base, map)),
            field: field.clone(),
            span: *span,
        },
        Expr::StructLiteral { name, fields, span } => Expr::StructLiteral {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(n, v)| (n.clone(), Box::new(substitute_vars(v, map))))
                .collect(),
            span: *span,
        },
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => Expr::Declassify {
            inner: Box::new(substitute_vars(inner, map)),
            policy: policy.clone(),
            reason: reason.clone(),
        },
        Expr::Assume(inner) => Expr::Assume(Box::new(substitute_vars(inner, map))),
        Expr::Assert(inner) => Expr::Assert(Box::new(substitute_vars(inner, map))),
        other => other.clone(),
    }
}

/// Convenience: substitute a single `result` variable.
fn substitute_result(e: &Expr, repl: &Expr) -> Expr {
    let mut m = BTreeMap::new();
    m.insert("result".to_string(), repl.clone());
    substitute_vars(e, &m)
}

/// Create an `ensures` obligation for a return: substitute `result` with the returned expression and
/// assert it under the given assumptions.
///
/// Soundness rule (FAIL-CLOSED — discharge or reject, never silently skip). `ensures`/`requires`
/// contracts are compile-time only: the transpiler (`backends/run.rs`) emits NO runtime check for
/// them, so a contract the checker does not prove is enforced NOWHERE. Therefore every `ensures`
/// must be either discharged by the solver or reported as an error — a silent skip would let a green
/// `anubis check` certify a postcondition that is false at runtime (e.g. `ensures(result == "wrong")`
/// returning `"ok"`, or a float/cast/untyped-param contract the bit-vector solver cannot model).
///
/// So: substitute `result` with the returned expression; if the concrete predicate is modelable in
/// QF_BV, emit the obligation (the solver proves or disproves it); otherwise REJECT with
/// `ANUBIS_CONTRACT_UNPROVABLE`. A postcondition that needs a value the solver cannot reason about
/// (a string/list, a float, a truncating cast, a call whose contract we did not carry) must be
/// rewritten as a provable integer bound, or expressed as a runtime `assert` in the body (which IS
/// enforced at runtime), not as an `ensures`.
fn push_ensures_obligations(
    ctx: &mut SemanticContext,
    ensures: &[Expr],
    ret_expr: &Expr,
    assumptions: &[String],
    span: Span,
) {
    for ens in ensures {
        let concrete = substitute_result(ens, ret_expr);
        if is_bool_modelable(&concrete, &ctx.solver_int_vars) {
            let smt = expr_to_smt(&concrete, &ctx.symbolic_widths);
            // Sort-partition (mirror the assert handler): an integer postcondition assumes only integer
            // facts, so a float `requires`/`let` fact cannot flip this obligation to QF_FP. Corpus-inert
            // (no float vars in scope ⇒ `fact_is_float` always false ⇒ identical to `assumptions.to_vec()`).
            let int_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| {
                    !fact_is_float(a, &ctx.solver_float_vars)
                        && !fact_is_string(a, &ctx.solver_string_vars)
                })
                .cloned()
                .collect();
            let mut vars = BTreeSet::new();
            collect_vars_from_smt(&smt, &mut vars);
            for a in &int_asm {
                collect_vars_from_smt(a, &mut vars);
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("ensures:{smt}"),
                assumptions: int_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: false,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else if is_bool_modelable_float(&concrete, &ctx.solver_float_vars) {
            // Phase-3 QF_FP: a float postcondition over the modelable subset (`+ - * /`, comparisons) is
            // DISCHARGED via the float encoder. The shared `assumptions` channel is sort-partitioned to
            // FLOAT facts only (float `requires` + float `let`/reassignment defining-facts), so a chained
            // `let y = x * 2.0; ensures(result < 4.0)` proves. Assertion vars come from the Expr (mangled,
            // dodging the `fp.`/`RNE`/`to_fp` operator-token trap); each consumed float fact's vars are
            // scraped too (anb_-prefixed only) and declared, so every QF_FP symbol is constrained.
            let smt = float_bool_to_smt(&concrete);
            let float_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| fact_is_float(a, &ctx.solver_float_vars))
                .cloned()
                .collect();
            let mut raw = BTreeSet::new();
            collect_expr_vars(&concrete, &mut raw);
            let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
            for a in &float_asm {
                let mut avs = BTreeSet::new();
                collect_vars_from_smt(a, &mut avs);
                vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("ensures:{smt}"),
                assumptions: float_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: false,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else if is_bool_modelable_string(&concrete, &ctx.solver_string_vars, &ctx.shadowed_string_preds) {
            // Phase-3 QF_S: a string-equality postcondition is DISCHARGED via the string encoder, with
            // the shared `assumptions` channel sort-partitioned to STRING facts (a string `requires`), so
            // `requires(s == "open") ensures(s == "open")` proves. All-`String` body, no sort clash.
            let smt = string_bool_to_smt(&concrete);
            let str_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| fact_is_string(a, &ctx.solver_string_vars))
                .cloned()
                .collect();
            let mut raw = BTreeSet::new();
            collect_expr_vars(&concrete, &mut raw);
            let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
            for a in &str_asm {
                let mut avs = BTreeSet::new();
                collect_vars_from_smt(a, &mut avs);
                vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("ensures:{smt}"),
                assumptions: str_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: true,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else if is_bool_modelable_strlen(&concrete, &ctx.solver_string_vars) {
            // Phase-3 str.len: a string-LENGTH postcondition (`ensures(len(result) >= 3)`) discharges in
            // QF_S over `str.len` — runtime `len(String)` = `chars().count()` = SMT `str.len`, sound both ways.
            let smt = strlen_bool_to_smt(&concrete, &ctx.solver_string_vars);
            let str_asm: Vec<String> = assumptions
                .iter()
                .filter(|a| fact_is_string(a, &ctx.solver_string_vars))
                .cloned()
                .collect();
            let mut raw = BTreeSet::new();
            collect_expr_vars(&concrete, &mut raw);
            let mut vars: BTreeSet<String> = raw.iter().map(|v| smt_var(v)).collect();
            for a in &str_asm {
                let mut avs = BTreeSet::new();
                collect_vars_from_smt(a, &mut avs);
                vars.extend(avs.into_iter().filter(|v| v.starts_with("anb_")));
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("ensures:{smt}"),
                assumptions: str_asm,
                assertion: smt,
                vars: vars.into_iter().collect(),
                strings: true,
                guard_assumptions: ctx.active_branch_guards.clone(),
            });
        } else {
            // A postcondition the solver cannot faithfully model. Contracts are NOT runtime-enforced,
            // so certifying this would be a silent overclaim: fail closed. Name the detectable cause
            // precisely (float vs string) so the diagnostic tells the truth about *why*, rather than
            // lumping every non-modelable case under one code.
            let code = unmodelable_contract_code(&concrete, &ctx.solver_int_vars);
            let message = match code {
                "ANUBIS_DIVISOR_MAYBE_ZERO" => {
                    "cannot verify this `ensures`: it uses `/` or `%` with a divisor that is not \
                     proven non-zero (need a non-zero literal, or `requires(d != 0)` / `requires(d > 0)` \
                     on an unreassigned parameter). Modeling an unguarded divide would silently ignore \
                     the runtime division-by-zero trap — fail closed"
                        .to_string()
                }
                "ANUBIS_INDEX_MAYBE_OOB" => {
                    "cannot verify this `ensures`: it indexes a sequence at an index that is not \
                     proven in-range (need a non-negative constant index strictly less than the \
                     known fixed length). Modeling an unguarded index would ignore the runtime \
                     ANUBIS_INDEX_OUT_OF_BOUNDS trap — fail closed"
                        .to_string()
                }
                "ANUBIS_SEQ_UNBOUNDED" => {
                    "cannot verify this `ensures`: it uses a sequence that is not a bounded, \
                     int-element array literal (or a let bound to one). Unbounded lists \
                     (parameters, push results, maps, …) are not modeled in QF_ABV — fail closed"
                        .to_string()
                }
                "ANUBIS_STRING_CONTRACT_UNMODELED" => {
                    "cannot verify this `ensures`: it mentions a string value. Strings stay opaque \
                     in the QF_BV/QF_ABV checker (optional QF_S later) — use a runtime `assert` or \
                     restate as an integer bound"
                        .to_string()
                }
                "ANUBIS_FLOAT_CONTRACT_UNMODELED" => {
                    "cannot verify this `ensures`: it mentions a float value. Floats stay opaque \
                     in the QF_BV/QF_ABV checker (optional QF_FP later) — use a runtime `assert` or \
                     restate as an integer bound"
                        .to_string()
                }
                _ => {
                    "cannot verify this `ensures` postcondition: it is not statically \
                     modelable (a float, a string/list, a truncating cast, an unmodeled or reassigned \
                     variable, or a value from a call whose contract is not carried). Contracts are \
                     compile-time only and are never checked at runtime, so an unprovable one is \
                     rejected — restate it as a provable integer bound, or use a runtime `assert` in \
                     the body instead"
                        .to_string()
                }
            };
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some(code.into()),
                message,
                span: Some((span.start, span.end)),
            });
        }
    }
}

/// Best-effort precise diagnostic code for a non-modelable `ensures`. Specific causes (string,
/// float, unguarded `/`/`%`, OOB index, unbounded seq) win over the general
/// `ANUBIS_CONTRACT_UNPROVABLE`. Phase-4 A1: `ANUBIS_DIVISOR_MAYBE_ZERO`. Phase-4 A2:
/// `ANUBIS_INDEX_MAYBE_OOB` / `ANUBIS_SEQ_UNBOUNDED`.
fn unmodelable_contract_code(e: &Expr, int_vars: &BTreeSet<String>) -> &'static str {
    fn scan(e: &Expr, int_vars: &BTreeSet<String>) -> Option<&'static str> {
        match e {
            Expr::StrLiteral(_) => Some("ANUBIS_STRING_CONTRACT_UNMODELED"),
            Expr::Literal(s) => {
                let t = s.trim();
                if t.starts_with('"') || t.starts_with('\'') {
                    Some("ANUBIS_STRING_CONTRACT_UNMODELED")
                } else if t.parse::<i64>().is_err() && t.parse::<f64>().is_ok() {
                    Some("ANUBIS_FLOAT_CONTRACT_UNMODELED")
                } else {
                    None
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                if (op == "/" || op == "%") && !divisor_is_proven_nonzero(rhs, int_vars) {
                    return Some("ANUBIS_DIVISOR_MAYBE_ZERO");
                }
                scan(lhs, int_vars).or_else(|| scan(rhs, int_vars))
            }
            Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => scan(expr, int_vars),
            // Phase-4 A2: index / sequence diagnostics (priority: OOB over unbounded when known).
            Expr::Index { base, index } => match base.as_ref() {
                Expr::Var(v) if is_seq_var(v, int_vars) => {
                    // Bounded seq known but index not proven in-range.
                    Some("ANUBIS_INDEX_MAYBE_OOB")
                }
                Expr::ArrayLiteral { elements }
                    if elements.iter().all(|el| is_int_modelable(el, int_vars)) =>
                {
                    // Literal elements modelable but index not a proven constant in range.
                    Some("ANUBIS_INDEX_MAYBE_OOB")
                }
                Expr::Var(_) | Expr::Call { .. } | Expr::Index { .. } => {
                    Some("ANUBIS_SEQ_UNBOUNDED")
                }
                Expr::ArrayLiteral { elements } => elements
                    .iter()
                    .find_map(|el| scan(el, int_vars))
                    .or(Some("ANUBIS_SEQ_UNBOUNDED")),
                other => scan(other, int_vars)
                    .or_else(|| scan(index, int_vars))
                    .or(Some("ANUBIS_SEQ_UNBOUNDED")),
            },
            Expr::ArrayLiteral { elements } => elements
                .iter()
                .find_map(|el| scan(el, int_vars))
                .or(Some("ANUBIS_SEQ_UNBOUNDED")),
            Expr::Call { callee, args } if callee == "len" => {
                // len of something that is not a bounded modeled sequence.
                Some("ANUBIS_SEQ_UNBOUNDED")
            }
            Expr::Call { args, .. } => args.iter().find_map(|a| scan(a, int_vars)),
            Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
                scan(inner, int_vars)
            }
            _ => None,
        }
    }
    scan(e, int_vars).unwrap_or("ANUBIS_CONTRACT_UNPROVABLE")
}

/// True when `e` is a divisor the solver may use in `bvsdiv`/`bvsrem` without modeling a trap:
/// non-zero integer literal, or a variable marked `nzdiv` (from `requires` that excludes 0).
fn divisor_is_proven_nonzero(e: &Expr, int_vars: &BTreeSet<String>) -> bool {
    is_nonzero_int_literal(e) || matches!(e, Expr::Var(v) if int_vars.contains(&nzdiv_mark(v)))
}

/// Collect the variable names referenced by an expression (for deciding which loop-carried
/// variables an invariant / condition constrains).
/// Collect every single-level struct-field access `base.field` (base a plain `Var`) reachable in a
/// contract predicate, so the has_contract registration can give each field a solver symbol. Direction
/// of failure is SAFE: UNDER-collection just leaves a field unmodeled (fail-open — it can only decline to
/// check, never over-prove), so a `_ => {}` catch-all here cannot cause a false accept. A nested `p.a.b`
/// yields `(p, a)` via the base recursion (its type is a struct → not registered as a scalar), never
/// `(p.a, b)`. Traverses the shapes a `requires` predicate can carry.
fn collect_field_accesses(e: &Expr, out: &mut Vec<(String, String)>) {
    match e {
        Expr::FieldAccess { base, field, .. } => {
            if let Expr::Var(v) = base.as_ref() {
                out.push((v.clone(), field.clone()));
            }
            collect_field_accesses(base, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_field_accesses(lhs, out);
            collect_field_accesses(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => collect_field_accesses(expr, out),
        Expr::Declassify { inner, .. }
        | Expr::Assume(inner)
        | Expr::Assert(inner)
        | Expr::Tainted { inner, .. }
        | Expr::Try(inner) => collect_field_accesses(inner, out),
        Expr::Call { args, .. } => {
            for a in args {
                collect_field_accesses(a, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            collect_field_accesses(callee, out);
            for a in args {
                collect_field_accesses(a, out);
            }
        }
        Expr::Index { base, index } => {
            collect_field_accesses(base, out);
            collect_field_accesses(index, out);
        }
        Expr::ArrayLiteral { elements } => {
            for el in elements {
                collect_field_accesses(el, out);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_field_accesses(v, out);
            }
        }
        // Remaining shapes carry no additional field access relevant to registration; under-collection is
        // fail-open-safe (see the doc comment), so no exhaustive enumeration is required here.
        _ => {}
    }
}

fn collect_expr_vars(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Var(v) => {
            out.insert(v.clone());
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_vars(lhs, out);
            collect_expr_vars(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => collect_expr_vars(expr, out),
        Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
            collect_expr_vars(inner, out)
        }
        // Must descend into the abs/min/max/len builtin Call AND into a bounded-seq Index / a
        // FieldAccess base: this set drives the "ensures references a reassigned/shadowed parameter"
        // fail-closed check, and MISSING a variable (under-approx) is unsound — it would let
        // `ensures(result == abs(x)) { x = 0-5; ... }` (or `ensures(result == arr[0]) { arr = …; }`)
        // be certified against the mutated binding while a caller assumes the entry value. It ALSO
        // drives the call-site `closed`-precondition depth gate (`discharge_call_requires`): missing
        // `arr` in `arr[0]` there mis-classifies a bounded-seq index as a constant and discharges it in
        // a branch without the guard, over-rejecting a safe program. Over-approx (collecting too many)
        // only makes both checks more conservative. MUST stay coupled with the modelable set
        // `is_int_modelable` admits (`Expr::Index` over a seq var / array literal) and `substitute_vars`.
        Expr::Call { args, .. } => {
            for a in args {
                collect_expr_vars(a, out);
            }
        }
        Expr::Index { base, index } => {
            collect_expr_vars(base, out);
            collect_expr_vars(index, out);
        }
        Expr::FieldAccess { base, .. } => collect_expr_vars(base, out),
        // A bare ARRAY LITERAL carries variables that `is_int_modelable` models: `[x][0]` == x and
        // `len([x])` are int-modelable, so a param reassigned in the body but referenced through such a
        // literal MUST be collected or the ensures anti-launder guard misses it (`ensures(result==[x][0])
        // { x = 0-42; ... }` was certified then trapped at runtime). This was the residual after the
        // Index/FieldAccess arms landed — an `Index` over an `[x]` base dead-ended here.
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                collect_expr_vars(e, out);
            }
        }
        // The remaining value-carrying container/wrapper shapes. None are in the modelable set TODAY, but
        // this walker's two consumers (the ensures anti-launder guard and the call-site `closed`-depth
        // gate) both OVER-APPROXIMATE safely (a collected-but-irrelevant var only makes them more
        // conservative — fail-closed), and leaving a `_ => {}` default is exactly the catch-all that let
        // `Index`/`ArrayLiteral` slip twice. Enumerate them so a future modelable-form addition cannot
        // silently re-break the coupling. (Statement-bearing shapes — Block/If/Match/IfLet/Lambda — are
        // never modelable value positions and carry their own scopes, so they are intentionally omitted.)
        Expr::CallExpr { callee, args } => {
            collect_expr_vars(callee, out);
            for a in args {
                collect_expr_vars(a, out);
            }
        }
        Expr::Tainted { inner, .. } | Expr::Try(inner) => collect_expr_vars(inner, out),
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_expr_vars(v, out);
            }
        }
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                collect_expr_vars(f, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_expr_vars(k, out);
                collect_expr_vars(v, out);
            }
        }
        // EXHAUSTIVE on purpose (no `_` default): a catch-all is exactly what let `Index` then
        // `ArrayLiteral` slip through the modelable-coupling twice. Every remaining variant carries no
        // modelable free variable — a literal/symbolic leaf, or a statement-bearing shape
        // (`Match`/`If`/`Block`/`Lambda`/`IfLet`) that `is_*_modelable` never admits, so it cannot appear
        // in a discharged predicate. If a future variant CAN carry one, the compiler now forces a decision
        // here rather than silently under-approximating.
        Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Match { .. }
        | Expr::If { .. }
        | Expr::Block { .. }
        | Expr::Lambda { .. }
        | Expr::IfLet { .. }
        | Expr::Other(_) => {}
    }
}

/// True when an expression contains NO statement-bearing construct — no block, `if`/`match`/`if let`
/// expression, or lambda — so it cannot hide an assignment or an escape. A loop body made only of
/// such simple expressions (plus plain assignments) is a flat straight-line sequence the transition
/// extractor can model soundly. This is the robust guard against writes hidden inside expressions
/// (e.g. `let z = if c { x = x + 1; 0 } else { 0 }`), which a statement-only scan never sees.
fn expr_is_simple(e: &Expr) -> bool {
    match e {
        Expr::Block { .. }
        | Expr::If { .. }
        | Expr::Match { .. }
        | Expr::IfLet { .. }
        | Expr::Lambda { .. } => false,
        Expr::Call { args, .. } | Expr::ArrayLiteral { elements: args } => {
            args.iter().all(expr_is_simple)
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().all(expr_is_simple),
        Expr::CallExpr { callee, args } => {
            expr_is_simple(callee) && args.iter().all(expr_is_simple)
        }
        Expr::Binary { lhs, rhs, .. } => expr_is_simple(lhs) && expr_is_simple(rhs),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_is_simple(expr),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_is_simple(expr),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_is_simple(inner),
        Expr::Index { base, index } => expr_is_simple(base) && expr_is_simple(index),
        Expr::FieldAccess { base, .. } => expr_is_simple(base),
        Expr::StructLiteral { fields, .. } => fields.iter().all(|(_, e)| expr_is_simple(e)),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .all(|(k, v)| expr_is_simple(k) && expr_is_simple(v)),
        // Leaves: Var / Literal / StrLiteral / Symbolic / TaintSource / UnifiedBuffer / RawPtr / Other.
        _ => true,
    }
}

/// Collect the ROOT of every place the loop body assigns, at ANY depth and in ANY form (a plain
/// `x = ...`, a compound `x += ...`, an `a[i] = ...` / `p.f = ...` place, or a write nested in an
/// `if`/nested loop). These are exactly the variables whose value can change across iterations, so a
/// pre-loop fact about any of them is stale inside the loop and after it — even an AUXILIARY variable
/// that no invariant mentions but a loop-carried variable reads (`x = x + z`). Without this, such a
/// frozen auxiliary let the checker "prove" a false invariant.
fn collect_assigned_roots(body: &[Stmt], out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Stmt::Assign { target, value } => {
                if let Some(r) = assign_target_root(target) {
                    out.insert(r.to_string());
                }
                expr_assigned_roots(target, out);
                expr_assigned_roots(value, out);
            }
            // A `let`/`let-pattern`/expression statement can hide a write inside an `if`/`match`/block
            // EXPRESSION (`let z = if c { x = x + 1; 0 } else { 0 };`) which a statement-only scan would
            // miss — walk the expressions too.
            Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
                expr_assigned_roots(init, out)
            }
            Stmt::ExprStmt(e) => expr_assigned_roots(e, out),
            Stmt::If { cond, then, else_ } => {
                expr_assigned_roots(cond, out);
                collect_assigned_roots(then, out);
                if let Some(e) = else_ {
                    collect_assigned_roots(e, out);
                }
            }
            Stmt::While { cond, body, .. } => {
                expr_assigned_roots(cond, out);
                collect_assigned_roots(body, out);
            }
            Stmt::WhileLet { expr, body, .. } => {
                expr_assigned_roots(expr, out);
                collect_assigned_roots(body, out);
            }
            Stmt::For { source, body, .. } => {
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        expr_assigned_roots(start, out);
                        expr_assigned_roots(end, out);
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_assigned_roots(expr, out)
                    }
                }
                collect_assigned_roots(body, out);
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => collect_assigned_roots(body, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    collect_assigned_roots(b, out);
                }
            }
            _ => {}
        }
    }
}

/// Collect every name a body BINDS with `let`/`let (…)` at any depth (used to detect a `let` that
/// shadows a parameter named in an `ensures`).
/// Collect every name a body BINDS at any depth — `let`/`let (…)`, a `for`/`while let` binder, AND
/// (crucially) every binder introduced in EXPRESSION position: a `match`-arm or `if let` pattern, a
/// lambda parameter, or a `let` inside an expression-position block. SOUNDNESS: this set is the rebind
/// set gating both the "ensures over a reassigned/shadowed parameter" fail-closed rejection AND
/// guarded-divisor nzdiv eligibility. Missing an expression-position binder let `match 2 { n => 6/n }`
/// shadow a guarded parameter `n` and certify a false contract (the arm rebinds n, so the runtime body
/// is `6/2`, not `6/entry-n`). COMPLETE recursion — every prior incremental fix (for-loop var, while-let)
/// missed a variant, so this walks the full statement AND expression tree; over-collection is safe.
fn collect_let_bound(body: &[Stmt], out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Stmt::Let { name, init, .. } => {
                out.insert(name.clone());
                expr_let_bound(init, out);
            }
            Stmt::LetPattern { pattern, init, .. } => {
                for n in pattern.bound_names() {
                    out.insert(n);
                }
                expr_let_bound(init, out);
            }
            Stmt::If { cond, then, else_ } => {
                expr_let_bound(cond, out);
                collect_let_bound(then, out);
                if let Some(e) = else_ {
                    collect_let_bound(e, out);
                }
            }
            Stmt::For {
                var, source, body, ..
            } => {
                out.insert(var.clone());
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        expr_let_bound(start, out);
                        expr_let_bound(end, out);
                    }
                    crate::frontend::ForSource::Collection { expr } => expr_let_bound(expr, out),
                }
                collect_let_bound(body, out);
            }
            Stmt::While { cond, body, .. } => {
                expr_let_bound(cond, out);
                collect_let_bound(body, out);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                for n in pattern.bound_names() {
                    out.insert(n);
                }
                expr_let_bound(expr, out);
                collect_let_bound(body, out);
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => collect_let_bound(body, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    collect_let_bound(b, out);
                }
            }
            Stmt::Assign { target, value } => {
                expr_let_bound(target, out);
                expr_let_bound(value, out);
            }
            Stmt::ExprStmt(e) => expr_let_bound(e, out),
            // Break / Continue / SpecBlock bind no runtime names.
            _ => {}
        }
    }
}

/// Names bound by MORE THAN ONE binding site in the function — a SHADOW (`let p = …; …; let p = …`, or an
/// outer `let p` re-bound by a `match`/`if let` arm / lambda param / `for` var). A struct-literal-let field
/// fact rides `assumptions` keyed by a MANGLED per-field symbol (`mangle_field`); the scalar shadow-clear
/// only evicts `anb_<name>`, not the field mangles, and an EXPRESSION-position shadow (a match arm binding
/// `p`) never reaches the `Stmt::Let` clear at all — so a shadowed base could reuse the outer's field fact
/// for a DIFFERENT value (a false proof). The struct-let field registration therefore SKIPS a shadowed base
/// (fail-open). Over-collection is safe: it only declines to model a struct-let field, never over-proves;
/// two disjoint-branch `let p`s counting as a shadow just fail-opens both. Uses `expr_let_bound` for
/// expression-embedded bindings so no binding form is missed.
fn collect_shadowed_lets(body: &[Stmt], out: &mut BTreeSet<String>) {
    let mut seen = BTreeSet::new();
    note_shadowed_lets(body, &mut seen, out);
}

fn note_shadowed_lets(body: &[Stmt], seen: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
    // A binding is a shadow iff its name was already seen at another site (Stmt or expression).
    let note_expr = |e: &Expr, seen: &mut BTreeSet<String>, out: &mut BTreeSet<String>| {
        let mut here = BTreeSet::new();
        expr_let_bound(e, &mut here);
        for n in here {
            if !seen.insert(n.clone()) {
                out.insert(n);
            }
        }
    };
    for s in body {
        match s {
            Stmt::Let { name, init, .. } => {
                if !seen.insert(name.clone()) {
                    out.insert(name.clone());
                }
                note_expr(init, seen, out);
            }
            Stmt::LetPattern { pattern, init, .. } => {
                for n in pattern.bound_names() {
                    if !seen.insert(n.clone()) {
                        out.insert(n);
                    }
                }
                note_expr(init, seen, out);
            }
            Stmt::If { cond, then, else_ } => {
                note_expr(cond, seen, out);
                note_shadowed_lets(then, seen, out);
                if let Some(e) = else_ {
                    note_shadowed_lets(e, seen, out);
                }
            }
            Stmt::For { var, source, body, .. } => {
                if !seen.insert(var.clone()) {
                    out.insert(var.clone());
                }
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        note_expr(start, seen, out);
                        note_expr(end, seen, out);
                    }
                    crate::frontend::ForSource::Collection { expr } => note_expr(expr, seen, out),
                }
                note_shadowed_lets(body, seen, out);
            }
            Stmt::While { cond, body, .. } => {
                note_expr(cond, seen, out);
                note_shadowed_lets(body, seen, out);
            }
            Stmt::WhileLet { pattern, expr, body } => {
                for n in pattern.bound_names() {
                    if !seen.insert(n.clone()) {
                        out.insert(n);
                    }
                }
                note_expr(expr, seen, out);
                note_shadowed_lets(body, seen, out);
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => note_shadowed_lets(body, seen, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    note_shadowed_lets(b, seen, out);
                }
            }
            Stmt::Assign { target, value } => {
                note_expr(target, seen, out);
                note_expr(value, seen, out);
            }
            Stmt::ExprStmt(e) => note_expr(e, seen, out),
            _ => {}
        }
    }
}

/// Collect names BOUND by patterns/lambdas inside an EXPRESSION (match arms, `if let`, lambda params,
/// and `let`s in a block-expr). Mirrors `expr_assigned_roots`' full traversal so no subexpression is
/// missed. See `collect_let_bound` for why under-collection is unsound and over-collection is safe.
fn expr_let_bound(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Block { stmts, tail } => {
            collect_let_bound(stmts, out);
            if let Some(t) = tail {
                expr_let_bound(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            expr_let_bound(cond, out);
            expr_let_bound(then, out);
            expr_let_bound(else_, out);
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            for n in pattern.bound_names() {
                out.insert(n);
            }
            expr_let_bound(scrutinee, out);
            expr_let_bound(then, out);
            expr_let_bound(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_let_bound(scrutinee, out);
            for a in arms {
                for n in a.pattern.bound_names() {
                    out.insert(n);
                }
                if let Some(g) = &a.guard {
                    expr_let_bound(g, out);
                }
                expr_let_bound(&a.body, out);
            }
        }
        Expr::Lambda { params, body } => {
            for p in params {
                out.insert(p.clone());
            }
            expr_let_bound(body, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_let_bound(lhs, out);
            expr_let_bound(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_let_bound(expr, out),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_let_bound(expr, out),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_let_bound(inner, out),
        Expr::Index { base, index } => {
            expr_let_bound(base, out);
            expr_let_bound(index, out);
        }
        Expr::FieldAccess { base, .. } => expr_let_bound(base, out),
        Expr::Call { args, .. }
        | Expr::ArrayLiteral { elements: args }
        | Expr::EnumConstruct { fields: args, .. } => {
            for x in args {
                expr_let_bound(x, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            expr_let_bound(callee, out);
            for x in args {
                expr_let_bound(x, out);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                expr_let_bound(x, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                expr_let_bound(k, out);
                expr_let_bound(v, out);
            }
        }
        _ => {}
    }
}

/// Collect assignment roots hidden INSIDE an expression — an assignment can live in a block, `if`,
/// `match`, or `if let` used in expression position (a `let` initializer, a call argument, a branch
/// value). Mutually recursive with `collect_assigned_roots` via `Expr::Block`.
fn expr_assigned_roots(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Block { stmts, tail } => {
            collect_assigned_roots(stmts, out);
            if let Some(t) = tail {
                expr_assigned_roots(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            expr_assigned_roots(cond, out);
            expr_assigned_roots(then, out);
            expr_assigned_roots(else_, out);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            expr_assigned_roots(scrutinee, out);
            expr_assigned_roots(then, out);
            expr_assigned_roots(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_assigned_roots(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    expr_assigned_roots(g, out);
                }
                expr_assigned_roots(&a.body, out);
            }
        }
        Expr::Lambda { body, .. } => expr_assigned_roots(body, out),
        Expr::Call { args, .. }
        | Expr::ArrayLiteral { elements: args }
        | Expr::EnumConstruct { fields: args, .. } => {
            for x in args {
                expr_assigned_roots(x, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            expr_assigned_roots(callee, out);
            for x in args {
                expr_assigned_roots(x, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_assigned_roots(lhs, out);
            expr_assigned_roots(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_assigned_roots(expr, out),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_assigned_roots(expr, out),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            expr_assigned_roots(inner, out)
        }
        Expr::Index { base, index } => {
            expr_assigned_roots(base, out);
            expr_assigned_roots(index, out);
        }
        Expr::FieldAccess { base, .. } => expr_assigned_roots(base, out),
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                expr_assigned_roots(x, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                expr_assigned_roots(k, out);
                expr_assigned_roots(v, out);
            }
        }
        _ => {}
    }
}

/// After a scope that MAY NOT fully execute — a loop body that can run zero times, or an `if` branch
/// that may not be taken — the variables it writes are UNCERTAIN. Restore the pre-scope assumptions
/// (discarding whatever facts the scope accumulated) and drop the fact + modelability of every
/// variable the scope writes. Without this, a fact asserted on a conditional path (e.g. `x = 5` inside
/// `while i < n { … }` or `if c { … }`) would leak out as an UNCONDITIONAL fact, letting `check`
/// certify an `ensures`/`assert` that is false when the path does not run.
fn drop_written_after_scope(
    ctx: &mut SemanticContext,
    assumptions: &mut Vec<String>,
    snapshot: Vec<String>,
    bodies: &[&[Stmt]],
) {
    *assumptions = snapshot;
    let mut written = BTreeSet::new();
    for b in bodies {
        collect_assigned_roots(b, &mut written);
    }
    let mut wm: BTreeSet<String> = BTreeSet::new();
    for v in &written {
        wm.insert(smt_var(v));
        wm.insert(seq_arr_smt(v));
        wm.insert(seq_len_smt(v));
    }
    assumptions.retain(|a| {
        let mut vs = BTreeSet::new();
        collect_vars_from_smt(a, &mut vs);
        vs.is_disjoint(&wm)
    });
    for v in &written {
        clear_binding_modelability(&mut ctx.solver_int_vars, v);
    }
}

/// Invalidate the solver model of every variable WRITTEN inside an expression — an assignment embedded in
/// a `match`-arm body, an `if`-expression branch, or an expression-position block (`let z = if c { y = 5;
/// 0 } else { 0 };`, or a bare `match c { 0 => { y = 5; } _ => {} }` statement). The statement-level frame
/// sweep (`drop_written_after_scope` / `havoc_loop_written`) is invoked ONLY by the `Stmt::If`/`While`/…
/// arms, so a write hidden inside an EXPRESSION escapes it: `Stmt::ExprStmt`/`Stmt::Let` only run
/// `analyze_expr_effect` + `check_expr_semantics`, which never touch the `assumptions` channel. Without
/// this, the variable's pre-write `let`/reassignment fact survives unconditionally and a later
/// `ensures`/`assert` is discharged against a value the runtime has moved past — a false proof the checker
/// then certifies. Mirror the straight-line reassignment invalidation (clear modelability — INT and float
/// — and drop the stale fact) but do NOT re-establish: the write is path-dependent (it fires on only one
/// arm/branch), so no unconditional fact can replace the dropped one. The variable falls to the runtime.
fn invalidate_embedded_writes(ctx: &mut SemanticContext, assumptions: &mut Vec<String>, expr: &Expr) {
    let mut written = BTreeSet::new();
    expr_assigned_roots(expr, &mut written);
    for root in &written {
        clear_binding_modelability(&mut ctx.solver_int_vars, root);
        ctx.solver_float_vars.remove(root);
        let mangled = smt_var(root);
        let arr = seq_arr_smt(root);
        let len = seq_len_smt(root);
        assumptions.retain(|a| {
            let mut vs = BTreeSet::new();
            collect_vars_from_smt(a, &mut vs);
            !vs.contains(&mangled) && !vs.contains(&arr) && !vs.contains(&len)
        });
    }
}

/// Havoc (invalidate) every variable a loop body writes BEFORE the body is analyzed: drop it from the
/// modelable set and remove its stale pre-loop fact. Without this, an obligation INSIDE the loop body
/// (e.g. `assert(x < 2)` before `x = x + 1`) is discharged against the pre-loop value of a variable
/// the loop mutates every iteration — a false proof that `check` accepts but the runtime panics on.
/// After havoc, an in-body assertion over a loop-written variable is left to the runtime (which does
/// enforce `assert`), rather than "proved" from a value the loop has moved past.
fn havoc_loop_written(ctx: &mut SemanticContext, assumptions: &mut Vec<String>, body: &[Stmt]) {
    let mut written = BTreeSet::new();
    collect_assigned_roots(body, &mut written);
    let mut mangled: BTreeSet<String> = BTreeSet::new();
    for v in &written {
        mangled.insert(smt_var(v));
        mangled.insert(seq_arr_smt(v));
        mangled.insert(seq_len_smt(v));
    }
    for v in &written {
        clear_binding_modelability(&mut ctx.solver_int_vars, v);
    }
    assumptions.retain(|a| {
        let mut vs = BTreeSet::new();
        collect_vars_from_smt(a, &mut vs);
        vs.is_disjoint(&mangled)
    });
}

/// True when statement `s` can break/continue/return OUT of the enclosing loop being analyzed. A
/// `break`/`continue` nested in an `if` still targets THIS loop (so it counts), but one inside a
/// NESTED loop targets the inner loop (so it does not) — only a `return` inside a nested loop escapes
/// further. Any such escape makes the per-iteration transition (and the post-loop `¬cond` assumption)
/// unsound, because the loop can exit while its condition is still true.
/// True when an EXPRESSION contains a `break`/`continue`/`return` that escapes the enclosing loop — a
/// control-flow call embedded in a value position the statement scan misses (`let s = break;`,
/// `x = if c { break; 0 } else { 0 };`, `for i in 0..(match c { _ => { return 0; } }) {}`). In
/// expression position break/continue/return are `Expr::Call{callee: "break"|"continue"|"return"}`
/// (frontend/mod.rs:3508-3515). Conservative — over-detection only makes the loop-invariant engine
/// fall back to runtime enforcement (fail-closed). Does not descend into a lambda body (its
/// break/continue targets a different scope).
fn expr_escapes_loop(e: &Expr) -> bool {
    match e {
        Expr::Call { callee, args } => {
            callee == "break"
                || callee == "continue"
                || callee == "return"
                || args.iter().any(expr_escapes_loop)
        }
        Expr::CallExpr { callee, args } => {
            expr_escapes_loop(callee) || args.iter().any(expr_escapes_loop)
        }
        Expr::Block { stmts, tail } => {
            stmts.iter().any(stmt_escapes_loop)
                || tail.as_deref().is_some_and(expr_escapes_loop)
        }
        Expr::If {
            cond, then, else_, ..
        } => expr_escapes_loop(cond) || expr_escapes_loop(then) || expr_escapes_loop(else_),
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => expr_escapes_loop(scrutinee) || expr_escapes_loop(then) || expr_escapes_loop(else_),
        Expr::Match { scrutinee, arms, .. } => {
            expr_escapes_loop(scrutinee)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(expr_escapes_loop) || expr_escapes_loop(&a.body)
                })
        }
        Expr::Binary { lhs, rhs, .. } => expr_escapes_loop(lhs) || expr_escapes_loop(rhs),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_escapes_loop(expr),
        Expr::Try(inner) | Expr::Assume(inner) | Expr::Assert(inner) => expr_escapes_loop(inner),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_escapes_loop(inner),
        Expr::ArrayLiteral { elements } => elements.iter().any(expr_escapes_loop),
        Expr::EnumConstruct { fields, .. } => fields.iter().any(expr_escapes_loop),
        Expr::Index { base, index } => expr_escapes_loop(base) || expr_escapes_loop(index),
        Expr::FieldAccess { base, .. } => expr_escapes_loop(base),
        _ => false,
    }
}

fn stmt_escapes_loop(s: &Stmt) -> bool {
    match s {
        Stmt::Break | Stmt::Continue => true,
        Stmt::ExprStmt(Expr::Call { callee, .. }) if callee == "return" => true,
        // A break/continue/return EMBEDDED in an expression position (a `let`/assign initializer or a
        // value statement) escapes just as much as a bare one — the loop can exit while its condition is
        // still true, so the per-iteration transition and post-loop `¬cond` assumption would be unsound.
        Stmt::ExprStmt(e) => expr_escapes_loop(e),
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => expr_escapes_loop(init),
        Stmt::Assign { value, .. } => expr_escapes_loop(value),
        Stmt::If { then, else_, .. } => {
            then.iter().any(stmt_escapes_loop)
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(stmt_escapes_loop))
        }
        Stmt::While { body, .. }
        | Stmt::Loop { body, .. }
        | Stmt::For { body, .. }
        | Stmt::WhileLet { body, .. } => body.iter().any(stmt_contains_return),
        _ => false,
    }
}

/// True when statement `s` contains a `return` at any depth (a return escapes the whole function, so
/// even one inside a nested loop invalidates the outer loop's straight-line transition).
fn stmt_contains_return(s: &Stmt) -> bool {
    match s {
        Stmt::ExprStmt(Expr::Call { callee, .. }) if callee == "return" => true,
        Stmt::If { then, else_, .. } => {
            then.iter().any(stmt_contains_return)
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(stmt_contains_return))
        }
        Stmt::While { body, .. }
        | Stmt::Loop { body, .. }
        | Stmt::For { body, .. }
        | Stmt::WhileLet { body, .. } => body.iter().any(stmt_contains_return),
        _ => false,
    }
}

/// Extract the straight-line transition a loop body applies to integer variables: a map from each
/// reassigned variable to its post-iteration value as an expression of the pre-iteration values.
/// Returns None when the body updates a variable in a way the checker cannot model soundly — a
/// branch/nested-loop write to a tracked variable, a non-modelable right-hand side, or any
/// break/continue/return — so the invariant cannot be verified inductively and the loop is rejected.
fn extract_loop_transition(
    body: &[Stmt],
    tracked: &BTreeSet<String>,
    model_vars: &BTreeSet<String>,
) -> Option<BTreeMap<String, Expr>> {
    // A break/continue/return anywhere in the body (including nested inside an `if`) can exit the
    // loop while its condition is still true, so the post-loop `¬cond` assumption would be unsound.
    if body.iter().any(stmt_escapes_loop) {
        return None;
    }
    // The body must be a FLAT, SIMPLE straight-line sequence: only plain `x = <simple expr>`
    // assignments, `let` bindings with a simple initializer, and neutral expression statements
    // (print/assert with no embedded assignment). A branch (`if`), a nested loop, a `match`, an
    // index/field-place assignment, or ANY expression that embeds a block/`if`/`match` (which could
    // hide a conditional write, e.g. `let z = if c { x = x + 1; 0 }`) is not soundly modelable as a
    // single unconditional transition — reject. This flat-body rule is the robust guard: it does not
    // depend on chasing writes through every expression form.
    let mut sub: BTreeMap<String, Expr> = BTreeMap::new();
    for st in body {
        match st {
            Stmt::Assign {
                target: Expr::Var(v),
                value,
            } => {
                if !expr_is_simple(value) {
                    return None;
                }
                let concrete = substitute_vars(value, &sub);
                // A non-modelable update to a TRACKED variable defeats verification. A non-modelable
                // update to an auxiliary is allowed here, but its stale fact is dropped by the
                // caller's `written`/frame handling so it cannot be read as a frozen constant.
                if tracked.contains(v) && !is_int_modelable(&concrete, model_vars) {
                    return None;
                }
                if is_int_modelable(&concrete, model_vars) {
                    sub.insert(v.clone(), concrete);
                } else {
                    sub.remove(v);
                }
            }
            // A `let` with a simple initializer is a loop-local binding, neutral for the transition —
            // UNLESS it SHADOWS a modeled variable. A shadowing `let y = 5` would make a later in-body
            // read of `y` resolve, in the model, to the outer symbolic (carrying its stale pre-loop
            // fact `y == 0`) while the runtime uses the shadow (5), certifying a false invariant.
            // Reject such a shadow rather than mis-model it (a fresh, non-shadowing name is fine).
            Stmt::Let { name, init, .. } => {
                if !expr_is_simple(init) || model_vars.contains(name) {
                    return None;
                }
            }
            Stmt::LetPattern { pattern, init, .. } => {
                if !expr_is_simple(init)
                    || pattern.bound_names().iter().any(|n| model_vars.contains(n))
                {
                    return None;
                }
            }
            // A print/assert/assume with a simple argument cannot mutate an integer binding (Anubis
            // is by-value); a call whose argument embeds a block could, so require simplicity.
            Stmt::ExprStmt(e) => {
                if !expr_is_simple(e) {
                    return None;
                }
            }
            // Anything else — a branch, a nested loop, a `match` statement, an index/field-place
            // assignment, a bare break/continue — is not a flat straight-line statement.
            _ => return None,
        }
    }
    Some(sub)
}

/// Verify a `while` loop's invariants by the Hoare rule and, on success, return the assumptions to
/// admit AFTER the loop (each invariant, plus the negated condition) together with the loop-carried
/// variables to re-model. Emits base-case and preservation obligations for the solver to discharge;
/// rejects (fail-closed) when the invariant or loop cannot be modeled inductively.
fn verify_while_invariants(
    ctx: &mut SemanticContext,
    cond: &Expr,
    invariants: &[Expr],
    body: &[Stmt],
    outer_assumptions: &[String],
) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    // Phase-3 QF_FP/QF_S: loop invariants are an INTEGER-only lane (the cond/invariant modelability gates
    // below reject non-integer formulas). A float `requires`/`let` fact from an enclosing float contract —
    // OR a string `requires` fact from an enclosing string contract — must therefore be dropped from the
    // outer assumptions, or it would flip an integer base-case/step obligation to QF_FP/QF_S and
    // mis-declare its i64 symbols. Corpus-inert (no float/string contract has a loop).
    let outer_assumptions: Vec<String> = outer_assumptions
        .iter()
        .filter(|a| {
            !fact_is_float(a, &ctx.solver_float_vars)
                && !fact_is_string(a, &ctx.solver_string_vars)
        })
        .cloned()
        .collect();
    let outer_assumptions: &[String] = &outer_assumptions;
    let reject = |ctx: &mut SemanticContext, why: &str| {
        ctx.diagnostics.push(SemanticDiagnostic {
            code: Some("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE".into()),
            message: format!(
                "cannot verify this loop invariant inductively: {why}. Invariants are supported on \
                 `while` loops whose body is straight-line integer assignments (no branch/nested-loop \
                 write to a loop-carried variable, no break/continue/return); state the invariant \
                 over integer variables the solver can model"
            ),
            span: None,
        });
    };

    // The loop-carried variables the invariant / condition constrain.
    let mut tracked = BTreeSet::new();
    collect_expr_vars(cond, &mut tracked);
    for inv in invariants {
        collect_expr_vars(inv, &mut tracked);
    }
    // Model those variables as fresh 64-bit symbolics for the inductive step.
    let mut model_vars = ctx.solver_int_vars.clone();
    for v in &tracked {
        model_vars.insert(v.clone());
        ctx.symbolic_widths.entry(v.clone()).or_insert(64);
    }

    // The condition and every invariant must be modelable, else induction is impossible.
    if !is_bool_modelable(cond, &model_vars) {
        reject(
            ctx,
            "the loop condition is not an integer formula the solver can model",
        );
        return None;
    }
    for inv in invariants {
        if !is_bool_modelable(inv, &model_vars) {
            reject(
                ctx,
                "an invariant is not an integer formula the solver can model",
            );
            return None;
        }
    }

    let push_ob = |ctx: &mut SemanticContext, name: String, asm: Vec<String>, assertion: String| {
        let mut vars = BTreeSet::new();
        collect_vars_from_smt(&assertion, &mut vars);
        for a in &asm {
            collect_vars_from_smt(a, &mut vars);
        }
        ctx.solver_obligations.push(SolverObligation {
            name,
            assumptions: asm,
            assertion,
            vars: vars.into_iter().collect(),
            strings: false,
            guard_assumptions: ctx.active_branch_guards.clone(),
        });
    };

    // BASE CASE: on entry, the pre-loop state implies each invariant.
    for inv in invariants {
        let smt = expr_to_smt(inv, &ctx.symbolic_widths);
        push_ob(
            ctx,
            format!("loop-invariant-base:{smt}"),
            outer_assumptions.to_vec(),
            smt,
        );
    }

    // TRANSITION: the straight-line effect of one iteration on the tracked variables.
    let transition = match extract_loop_transition(body, &tracked, &model_vars) {
        Some(t) => t,
        None => {
            reject(
                ctx,
                "the loop body is not straight-line integer assignments",
            );
            return None;
        }
    };

    // PRESERVATION: assuming the invariants, the loop condition, and the loop's FRAME, each invariant
    // still holds after one iteration. The WRITTEN variables (every variable the body assigns at any
    // depth — including an auxiliary the transition does not capture, e.g. one written in a branch or
    // via a non-modelable RHS) are fresh symbolic: their concrete pre-loop values are stale and must
    // be dropped. Only an outer fact about a variable the loop NEVER writes (e.g. `requires(n < 100)`
    // while the loop touches `i`/`total`) holds every iteration and stays in scope — without it a
    // bound like `total <= n` could not be shown overflow-free.
    let mut written: BTreeSet<String> = BTreeSet::new();
    collect_assigned_roots(body, &mut written);
    let written_mangled: BTreeSet<String> = written.iter().map(|v| smt_var(v)).collect();
    let frame: Vec<String> = outer_assumptions
        .iter()
        .filter(|a| {
            let mut vs = BTreeSet::new();
            collect_vars_from_smt(a, &mut vs);
            vs.is_disjoint(&written_mangled)
        })
        .cloned()
        .collect();
    let cond_smt = expr_to_smt(cond, &ctx.symbolic_widths);
    let inv_smts: Vec<String> = invariants
        .iter()
        .map(|i| expr_to_smt(i, &ctx.symbolic_widths))
        .collect();
    let mut step_assumptions = inv_smts.clone();
    step_assumptions.push(cond_smt.clone());
    step_assumptions.extend(frame);
    for inv in invariants {
        let stepped = substitute_vars(inv, &transition);
        if !is_bool_modelable(&stepped, &model_vars) {
            reject(
                ctx,
                "an invariant is not modelable after the loop body's update",
            );
            return None;
        }
        let smt = expr_to_smt(&stepped, &ctx.symbolic_widths);
        push_ob(
            ctx,
            format!("loop-invariant-step:{smt}"),
            step_assumptions.clone(),
            smt,
        );
    }

    // SUCCESS: after the loop the invariants hold and the loop has exited (¬cond). Return (1) EVERY
    // written variable — a stale pre-loop fact about any of them (even an auxiliary) must be dropped
    // after the loop, while a fact about an unwritten variable (e.g. `n < 1000`) stays true — and
    // (2) the tracked variables to re-model so the post-loop invariant assumptions are usable.
    let mut post = inv_smts;
    post.push(format!("(not {cond_smt})"));
    let written_vars: Vec<String> = written.into_iter().collect();
    let readmit: Vec<String> = tracked.into_iter().collect();
    Some((post, written_vars, readmit))
}

/// Collect every explicit `return X` expression in a statement (recursing into nested blocks), so a
/// contract's `ensures` can be checked at every return point, not only the tail.
fn collect_returns_in_stmt(s: &Stmt, out: &mut Vec<Expr>) {
    match s {
        // A statement's expressions can hide a `return` inside a `match`-arm / `if`/block expression or
        // a `let`/assign initializer — walk them, else such a return escapes the `ensures` check.
        Stmt::ExprStmt(e) => expr_returns(e, out),
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => expr_returns(init, out),
        Stmt::Assign { target, value } => {
            expr_returns(target, out);
            expr_returns(value, out);
        }
        Stmt::If { cond, then, else_ } => {
            expr_returns(cond, out);
            for st in then {
                collect_returns_in_stmt(st, out);
            }
            if let Some(e) = else_ {
                for st in e {
                    collect_returns_in_stmt(st, out);
                }
            }
        }
        Stmt::While { cond, body, .. } => {
            expr_returns(cond, out);
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::WhileLet { expr, body, .. } => {
            expr_returns(expr, out);
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::For { source, body, .. } => {
            match source {
                crate::frontend::ForSource::Range { start, end } => {
                    expr_returns(start, out);
                    expr_returns(end, out);
                }
                crate::frontend::ForSource::Collection { expr } => expr_returns(expr, out),
            }
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::Loop { body, .. }
        | Stmt::ResearchBlock { body, .. }
        | Stmt::ExploitBlock { body, .. } => {
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                for st in b {
                    collect_returns_in_stmt(st, out);
                }
            }
        }
        _ => {}
    }
}

/// Collect the values of `return X` calls hidden INSIDE an expression — a `match` arm, an `if`/block
/// expression, or any subexpression. Mirrors `expr_assigned_roots`; without it, a postcondition is not
/// checked at a return embedded in expression position (e.g. `match c { 0 => return 0, _ => 1 }`). A
/// `Lambda` body is NOT descended into — its `return` belongs to the closure, not the enclosing fn.
fn expr_returns(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Call { callee, args } if callee == "return" => {
            if let Some(first) = args.first() {
                out.push(first.clone());
            }
            for a in args {
                expr_returns(a, out);
            }
        }
        Expr::Block { stmts, tail } => {
            for st in stmts {
                collect_returns_in_stmt(st, out);
            }
            if let Some(t) = tail {
                expr_returns(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            expr_returns(cond, out);
            expr_returns(then, out);
            expr_returns(else_, out);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            expr_returns(scrutinee, out);
            expr_returns(then, out);
            expr_returns(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_returns(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    expr_returns(g, out);
                }
                expr_returns(&a.body, out);
            }
        }
        Expr::Lambda { .. } => {}
        Expr::Call { args, .. }
        | Expr::ArrayLiteral { elements: args }
        | Expr::EnumConstruct { fields: args, .. } => {
            for x in args {
                expr_returns(x, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            expr_returns(callee, out);
            for x in args {
                expr_returns(x, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_returns(lhs, out);
            expr_returns(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_returns(expr, out),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_returns(expr, out),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_returns(inner, out),
        Expr::Index { base, index } => {
            expr_returns(base, out);
            expr_returns(index, out);
        }
        Expr::FieldAccess { base, .. } => expr_returns(base, out),
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                expr_returns(x, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                expr_returns(k, out);
                expr_returns(v, out);
            }
        }
        _ => {}
    }
}

/// Whether a function body uses the `?` operator, NOT descending into nested lambdas (a `?` inside a
/// closure early-returns from the closure, not this function).
fn body_contains_try(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_try)
}

fn stmt_contains_try(s: &Stmt) -> bool {
    match s {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => expr_contains_try(init),
        Stmt::Assign { target, value } => expr_contains_try(target) || expr_contains_try(value),
        Stmt::If { cond, then, else_ } => {
            expr_contains_try(cond)
                || then.iter().any(stmt_contains_try)
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(stmt_contains_try))
        }
        Stmt::While { cond, body, .. } => {
            expr_contains_try(cond) || body.iter().any(stmt_contains_try)
        }
        Stmt::WhileLet { expr, body, .. } => {
            expr_contains_try(expr) || body.iter().any(stmt_contains_try)
        }
        Stmt::For { source, body, .. } => {
            let in_source = match source {
                crate::frontend::ForSource::Range { start, end } => {
                    expr_contains_try(start) || expr_contains_try(end)
                }
                crate::frontend::ForSource::Collection { expr } => expr_contains_try(expr),
            };
            in_source || body.iter().any(stmt_contains_try)
        }
        Stmt::Loop { body, .. }
        | Stmt::ResearchBlock { body, .. }
        | Stmt::ExploitBlock { body, .. } => body.iter().any(stmt_contains_try),
        Stmt::HybridBlock { gpu, cpu, prove } => [gpu, cpu, prove]
            .into_iter()
            .flatten()
            .any(|b| b.iter().any(stmt_contains_try)),
        Stmt::ExprStmt(e) => expr_contains_try(e),
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => false,
    }
}

fn expr_contains_try(e: &Expr) -> bool {
    match e {
        Expr::Try(_) => true,
        Expr::Lambda { .. } => false, // a `?` in a nested closure belongs to the closure
        Expr::Binary { lhs, rhs, .. } => expr_contains_try(lhs) || expr_contains_try(rhs),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_contains_try(expr),
        Expr::Call { args, .. } | Expr::ArrayLiteral { elements: args } => {
            args.iter().any(expr_contains_try)
        }
        Expr::CallExpr { callee, args } => {
            expr_contains_try(callee) || args.iter().any(expr_contains_try)
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().any(expr_contains_try),
        Expr::Index { base, index } => expr_contains_try(base) || expr_contains_try(index),
        Expr::FieldAccess { base, .. } => expr_contains_try(base),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_contains_try(inner),
        Expr::Assume(x) | Expr::Assert(x) => expr_contains_try(x),
        Expr::StructLiteral { fields, .. } => fields.iter().any(|(_, v)| expr_contains_try(v)),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_contains_try(scrutinee)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(expr_contains_try) || expr_contains_try(&a.body)
                })
        }
        Expr::If {
            cond, then, else_, ..
        } => expr_contains_try(cond) || expr_contains_try(then) || expr_contains_try(else_),
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => expr_contains_try(scrutinee) || expr_contains_try(then) || expr_contains_try(else_),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .any(|(k, v)| expr_contains_try(k) || expr_contains_try(v)),
        Expr::Block { stmts, tail } => {
            stmts.iter().any(stmt_contains_try)
                || tail.as_ref().is_some_and(|t| expr_contains_try(t))
        }
        _ => false,
    }
}

/// Check a function's declared `-> rty` against the value it returns, but only where that value is
/// a LITERAL of a statically-known type (so a dynamic return is never falsely rejected). Covers the
/// implicit tail expression and top-level explicit `return X;` (which parses as a `return(...)` call).
fn check_return_types(
    body: &[Stmt],
    rty: &str,
    scope: &BTreeMap<String, ScopeBinding>,
    span: Span,
    ctx: &mut SemanticContext,
) {
    // Implicit-return tail: the last statement when it is a bare value expression.
    if let Some(Stmt::ExprStmt(e)) = body.last() {
        check_one_return(e, rty, scope, span, ctx);
    }
    // Explicit `return X;` at the top level (deeper returns are left dynamic — conservative).
    for st in body {
        if let Stmt::ExprStmt(Expr::Call { callee, args }) = st {
            if callee == "return" {
                if let Some(v) = args.first() {
                    check_one_return(v, rty, scope, span, ctx);
                }
            }
        }
    }
}

fn check_one_return(
    expr: &Expr,
    rty: &str,
    scope: &BTreeMap<String, ScopeBinding>,
    span: Span,
    ctx: &mut SemanticContext,
) {
    // Only a CONSTANT has a reliable, stable static type; anything dynamic (variable, call, if/match
    // over variables, a trailing statement that yields the default 0) is left unchecked here. This
    // also catches `return 5 as u32` from a `-> string` fn — a cast constant the checker trusts.
    if is_constant_expr(expr) {
        if let Some(actual) = infer_expr_type_scoped(expr, scope) {
            if !types_assignable(rty, &actual) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_RETURN_TYPE_MISMATCH".into()),
                    message: format!(
                        "function declared `-> {}` but returns a value of type `{}`",
                        rty, actual
                    ),
                    span: Some((span.start, span.end)),
                });
            }
        }
        return;
    }
    // A `Call`/`Index`/`FieldAccess` return — invisible to the flat inference above (it returned
    // `None` for all three) — can now be synthesized and checked against the declared return type
    // (e.g. `return helper()` where `helper -> string` in a `-> u32` fn). Restricted to exactly those
    // three newly-synthesizable forms so a variable/`if`/`match` return stays dynamic as before.
    // PROMOTED to enforcing — corpus shadow diff UNEXPECTED=0.
    if matches!(
        expr,
        Expr::Call { .. } | Expr::Index { .. } | Expr::FieldAccess { .. }
    ) {
        if let Some(actual) = check_mismatch_scoped(expr, rty, scope, ctx) {
            ctx.emit(
                SemanticDiagnostic {
                    code: Some("ANUBIS_RETURN_TYPE_MISMATCH".into()),
                    message: format!(
                        "function declared `-> {}` but returns a value of type `{}`",
                        rty, actual
                    ),
                    span: Some((span.start, span.end)),
                },
                false,
            );
        }
    }
}

fn infer_expr_type_scoped(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> Option<String> {
    match expr {
        Expr::Symbolic { ty } => Some(ty.clone()),
        Expr::Tainted { ty, .. } => Some(format!("tainted<{}>", ty)),
        Expr::UnifiedBuffer { ty } => Some(format!("unified Buffer<{}>", ty)),
        Expr::RawPtr { mutable } => Some(if *mutable {
            "*mut unknown".into()
        } else {
            "*const unknown".into()
        }),
        Expr::Declassify { inner, .. } => infer_expr_type_scoped(inner, scope),
        // Typed `?` propagation: `r?` unwraps `Option<T>`/`Result<T, E>` to its Ok/Some type `T`, so a
        // `let x: WrongT = r?` mismatch is caught when `r`'s type is known. (Residual: `f()?` where the
        // callee's return type is not inferable in this scope-only inferer — no fn-return table here.)
        Expr::Try(inner) => infer_expr_type_scoped(inner, scope).and_then(|t| ty::try_unwrap_ok(&t)),
        Expr::TaintSource { .. } => Some("tainted<string>".into()),
        Expr::Literal(s) if s == "true" || s == "false" => Some("bool".into()),
        // Integer literal (i64, or a u64 bit-pattern for magnitudes above i64::MAX) → the
        // width-polymorphic integer default. A literal that is ONLY f64-parseable (`3.14`, `1e9`)
        // is a FLOAT: typing it as an integer would let the solver model it as an i64 bit-vector
        // (unsound — it "proved" `2*x != 1` for x = 0.5). Mirrors the runtime discrimination in
        // `literal_to_anubis_value`, so a float is kept out of the solver's integer domain.
        Expr::Literal(s) if s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok() => {
            Some("u32".into())
        }
        Expr::Literal(s) if s.parse::<f64>().is_ok() => Some("f64".into()),
        Expr::Literal(s) if s.starts_with('"') || s.starts_with('\'') => Some("string".into()),
        Expr::StrLiteral(_) => Some("string".into()),
        Expr::Var(name) => scope.get(name).and_then(|b| b.info.ty.clone()),
        Expr::Unary { op, expr } if op == "!" => Some("bool".into()),
        // Bitwise-not is integer at runtime (anubis_bnot `as_i64()`s and returns `Int`); unary `-`
        // (anubis_neg) is float iff its operand is float, so it propagates.
        Expr::Unary { op, .. } if op == "~" => Some("u32".into()),
        Expr::Unary { expr, .. } => infer_expr_type_scoped(expr, scope),
        Expr::Binary { op, lhs, rhs } => {
            if matches!(
                op.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
            ) {
                Some("bool".into())
            } else if op == "+" {
                // `+` is overloaded (anubis_add): string concat if EITHER operand is a string, list
                // concat if either is a list, otherwise numeric. Inferring the result from the lhs
                // alone wrongly typed `1 + "a"` as a number (accepting it into a u32 slot) and
                // `404 + ": x"` as a number (rejecting it from a string slot).
                let lt = infer_expr_type_scoped(lhs, scope).map(|t| normalize_ty(&t));
                let rt = infer_expr_type_scoped(rhs, scope).map(|t| normalize_ty(&t));
                if lt.as_deref() == Some("string") || rt.as_deref() == Some("string") {
                    Some("string".into())
                } else if lt.as_deref() == Some("list") || rt.as_deref() == Some("list") {
                    Some("list".into())
                } else if lt.as_deref().is_some_and(is_float_ty)
                    || rt.as_deref().is_some_and(is_float_ty)
                {
                    // `anubis_add` returns `Float` when EITHER operand is float, so `2 + 1.5` is f64 —
                    // NOT the lhs's u32. Inferring lhs-first let a float RHS narrow into an integer slot
                    // (`let x: u32 = 2 + 1.5` was accepted while `1.5 + 2` was correctly rejected).
                    Some("f64".into())
                } else {
                    lt.or(rt)
                }
            } else if matches!(op.as_str(), "&" | "|" | "^" | "<<" | ">>") {
                // Bitwise/shift are INTEGER at runtime regardless of operands: anubis_band/bor/bxor/
                // shl/shr (run.rs) `as_i64()` both operands and unconditionally return `Int`. So
                // `avg & 7` is an integer even when `avg` is a float — inferring it from the float
                // operand (the arithmetic `else` below) wrongly typed it f64 and made the float→int
                // narrowing rule reject a program that runs and yields an integer.
                Some("u32".into())
            } else {
                // Arithmetic (`- * / %`): float iff EITHER operand is float (anubis_sub/mul/div/mod
                // return `Float` on any float operand), so infer f64 when either side is float — a UNION,
                // not lhs-first, which dropped a float RHS and let `2 - 1.5` / `2 * 1.5` narrow into an
                // integer slot. Otherwise propagate the (integer) operand type.
                let lt = infer_expr_type_scoped(lhs, scope);
                let rt = infer_expr_type_scoped(rhs, scope);
                if lt.as_deref().is_some_and(is_float_ty) || rt.as_deref().is_some_and(is_float_ty) {
                    Some("f64".into())
                } else {
                    lt.or(rt)
                }
            }
        }
        Expr::ArrayLiteral { .. } => Some("list".into()),
        Expr::MapLiteral { .. } => Some("map".into()),
        Expr::EnumConstruct { enum_name, .. } => Some(enum_name.clone()),
        Expr::If { then, else_, .. } => value_branch_type(&[
            infer_expr_type_scoped(then, scope),
            infer_expr_type_scoped(else_, scope),
        ]),
        Expr::Match { arms, .. } => value_branch_type(
            &arms
                .iter()
                .map(|a| infer_expr_type_scoped(&a.body, scope))
                .collect::<Vec<_>>(),
        ),
        // A block used as a value (e.g. an `if`/`match` branch `{ let a = 3.14; a }`) has the type
        // of its trailing expression; a statement block with no tail has no value. Without this,
        // block-wrapped branches inferred `None`, letting an all-float nested `if` escape the
        // float→int narrowing rule.
        Expr::Block { tail, .. } => tail.as_ref().and_then(|t| infer_expr_type_scoped(t, scope)),
        // `Call`/`Index`/`FieldAccess` are now genuinely synthesizable — but in the bidirectional core
        // (`ty::synth`), not here. This flat function stays the LEGACY substrate for the checks that
        // are already enforcing (arg/return/let-init type checks that call it directly); returning
        // `None` for these three preserves their exact behavior so the new synthesis lands only
        // through the shadow-gated `check_mismatch_scoped` path until a check is promoted. See the
        // inference-core section of `middle/ty.rs`.
        Expr::Index { .. } => None,
        Expr::FieldAccess { .. } => None,
        Expr::Call { .. } => None,
        Expr::Cast { ty, .. } => Some(ty.clone()),
        _ => None,
    }
}

/// A CONSTANT expression — one built solely from literals and operators, with NO variables, calls,
/// or index/field accesses. Its type is intrinsic and immutable, so B1 can act on it soundly. B1
/// only acts on constants: a variable's type is NOT stable in a language with `let mut` rebinding
/// (a `let mut v = 0` reassigned from a dynamic value keeps its stale numeric type), so trusting a
/// variable's inferred type would produce false positives. This widens the earlier bare-literal
/// gate so nested-but-still-constant errors like `(2 + 3)[0]` and `("a" + "b") - 1` are caught.
fn is_constant_expr(e: &Expr) -> bool {
    match e {
        Expr::Literal(_) | Expr::StrLiteral(_) => true,
        Expr::ArrayLiteral { elements } => elements.iter().all(is_constant_expr),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .all(|(k, v)| is_constant_expr(k) && is_constant_expr(v)),
        Expr::Binary { lhs, rhs, .. } => is_constant_expr(lhs) && is_constant_expr(rhs),
        Expr::Unary { expr, .. } => is_constant_expr(expr),
        Expr::If {
            cond, then, else_, ..
        } => is_constant_expr(cond) && is_constant_expr(then) && is_constant_expr(else_),
        Expr::Cast { expr, .. } => is_constant_expr(expr),
        _ => false, // Var, Call, Index, FieldAccess, Match, … are dynamic
    }
}

/// B1 static type checking. A statically-known string/list/map CONSTANT is never a valid arithmetic
/// operand (a number/bool constant is fine — bool 0/1 arithmetic is idiomatic). A non-constant
/// (variable, call, index) returns None and is left untouched — zero false positives.
fn static_non_numeric_operand(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
) -> Option<String> {
    if !is_constant_expr(expr) {
        return None;
    }
    let n = normalize_ty(&infer_expr_type_scoped(expr, scope)?);
    matches!(n.as_str(), "string" | "list" | "map").then_some(n)
}

/// B1: a statically-known non-indexable CONSTANT (a number or bool). Only lists/strings/maps/structs
/// are indexable. A non-constant base returns None (dynamic bases stay fail-closed at runtime).
fn static_non_indexable(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> Option<String> {
    if !is_constant_expr(expr) {
        return None;
    }
    let n = normalize_ty(&infer_expr_type_scoped(expr, scope)?);
    (is_numeric_ty(&n) || n == "bool").then_some(n)
}

// The type-reasoning predicates now live in `middle/ty.rs` (the single source of truth and the
// foundation for the structured `Ty` enum). These are thin shims delegating to it; behavior is
// pinned identical by the `ty_parity` test below.
fn normalize_ty(ty: &str) -> String {
    ty::normalize(ty)
}

fn is_numeric_ty(ty: &str) -> bool {
    ty::is_numeric(ty)
}

fn is_integer_ty(ty: &str) -> bool {
    ty::is_integer(ty)
}

fn is_float_ty(ty: &str) -> bool {
    ty::is_float(ty)
}

fn cast_preserves_i64(ty: &str) -> bool {
    ty::cast_preserves_i64(ty)
}

/// Mangle an Anubis identifier into an SMT symbol that can never collide with an SMT-LIB keyword or
/// a `bv…` literal/operator. Without this, a parameter named `model`, `set`, `check`, or `bvx` was
/// dropped by `collect_vars_from_smt` (it looked like a keyword), left undeclared, and z3 returned a
/// parse error that `check` treated as "not a disproof" — a fail-OPEN hole. Every variable emitted
/// into SMT goes through here so declaration, emission, and collection agree.
fn smt_var(name: &str) -> String {
    format!("anb_{}", name)
}

/// Canonical solver-var name for a struct-PARAM field access `base.field` (used when a `requires`
/// constrains the field of a struct parameter, so an `assert`/`ensures` over the SAME field is checked
/// rather than fail-opened). The name is:
///   * DIGIT-LED (`base.len()` prefixes it) — a user identifier can never start with a digit, so after
///     `smt_var` prepends `anb_` the result cannot collide with any real `anb_<uservar>`;
///   * LENGTH-PREFIXED on the base — distinct `(base, field)` pairs map to distinct symbols even when a
///     name contains `_` (base=`p_a` field=`b` → `3fld_p_a_b`; base=`p` field=`a_b` → `1fld_p_a_b`);
///   * TOKEN-SAFE (`[0-9A-Za-z_]` only) — `collect_vars_from_smt` tokenizes it as ONE symbol.
///   * SENTINEL-SAFE — a fixed `_e` terminator means the symbol can never END in the `__arr` suffix that
///     `smt_uses_arrays` treats as a sequence-Array marker, even for a field literally named `_arr`
///     (which would otherwise route the obligation to Array-sort logic → z3 sort error → fail-open). The
///     terminator keeps injectivity: the field is exactly the chars between the base and the final `_e`.
///
/// Only single-level `Var.field` gets a symbol; a nested `p.a.b` (base is itself a FieldAccess) stays
/// unmodeled (fail-open), as does any base that is not a registered struct-param field.
fn mangle_field(base: &str, field: &str) -> String {
    format!("{}fld_{}_{}_e", base.len(), base, field)
}

/// Directional assignability for binding contexts (let-init, assignment to an annotated variable,
/// call arguments, returns): `ty::compatible` (numeric widths interoperate; bool/string/enums do
/// not cross; `tainted<T>` is a qualifier still policed by the taint analysis), refined with the
/// one directional Phase-2 rule — a float value may not narrow into an integer annotation. Used
/// only where a value flows INTO a declared type; the `if`/`match` arm-type join during inference
/// is a symmetric context and uses `value_branch_type` instead. See `ty::assignable`.
fn types_assignable(expected: &str, actual: &str) -> bool {
    ty::assignable(expected, actual)
}

/// The inferred type of an `if`/`match` used as a value. The runtime takes exactly ONE branch, so
/// the value is *definitely* float only when EVERY branch is a known float — order-independently.
/// This drives the float→int narrowing rule, so it must be exact in both directions:
///
/// - every branch a known float → that float type (a real `let x: u32 = if c { 3.14 } else { 2.71 }`
///   lie is caught regardless of branch order or block nesting);
/// - a float branch mixed with a definite non-float, OR with an unseeable (`None`) branch → NOT
///   definitely float (the taken branch may be the non-float one), so never report a float: prefer
///   a known non-float type (keeps numeric-into-string/bool mismatches catchable), else `None`.
///   This is what stops the Round-1 false positive `if c { 3.14 } else { 5 }` from being rejected;
/// - no float branch → the first known branch's type (ordinary, historical inference — also
///   restores the old `(None, Some(a)) => Some(a)` fallback that a first-`None` branch needs).
///
/// A branch whose type the checker cannot see (`None` — e.g. a call/index result) makes the value
/// not-definitely-float and is therefore NOT narrowed; that residual float→int case is a documented
/// completeness gap, not a soundness hole (the solver still fails closed on such a value).
fn value_branch_type(branches: &[Option<String>]) -> Option<String> {
    let known: Vec<&String> = branches.iter().flatten().collect();
    let first_known = (*known.first()?).clone();
    let any_float = known.iter().any(|t| ty::is_float(t));
    if !any_float {
        return Some(first_known);
    }
    let all_known = known.len() == branches.len();
    let any_nonfloat = known.iter().any(|t| !ty::is_float(t));
    if all_known && !any_nonfloat {
        return Some(first_known); // every branch a known float → definitely float
    }
    known
        .iter()
        .find(|t| !ty::is_float(t))
        .map(|t| (*t).clone())
}

/// The owned variable-type view the bidirectional inference core (`ty::synth`) consults, projected
/// from the current lexical scope: variable name → its inferred/declared annotation.
fn scope_vars(scope: &BTreeMap<String, ScopeBinding>) -> BTreeMap<String, String> {
    scope
        .iter()
        .filter_map(|(k, b)| b.info.ty.clone().map(|t| (k.clone(), t)))
        .collect()
}

/// Arm-join conflict across the branches of an `if`/`match`, via the bidirectional core. Returns the
/// first genuine cross-category clash as `(left, right)` annotations, or `None` when the arms join
/// cleanly (any arm the checker cannot see resolves to `Any` and absorbs). Borrows `ctx` immutably
/// and returns owned data so the caller can then `emit` the shadow-gated diagnostic without a
/// borrow conflict.
fn arm_join_conflict_scoped(
    branches: &[&Expr],
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<(String, String)> {
    let vars = scope_vars(scope);
    let env = ty::InferEnv {
        vars: &vars,
        fns: &ctx.fn_ret_types,
        structs: &ctx.struct_fields,
    };
    ty::arm_join_conflict(&env, branches)
}

/// Check-direction mismatch for a value flowing into `expected`, via the bidirectional core. Unlike
/// the flat `infer_expr_type_scoped` (which returns `None` for every `Call`/`Index`/`FieldAccess`),
/// this synthesizes those; it returns the concrete synthesized type when it is NOT assignable to
/// `expected`, else `None` (the accept direction — `Any`/unbound-var/generic and every assignable
/// type). Borrows `ctx` immutably; the caller emits.
fn check_mismatch_scoped(
    expr: &Expr,
    expected: &str,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<String> {
    let vars = scope_vars(scope);
    let env = ty::InferEnv {
        vars: &vars,
        fns: &ctx.fn_ret_types,
        structs: &ctx.struct_fields,
    };
    ty::check_mismatch(&env, expr, expected)
}

/// The `Result`/`Option` container kind of a `?` operand, via the bidirectional core — `Some("Result")`
/// / `Some("Option")` or `None` (accept). Borrows `ctx` immutably; the caller emits.
fn try_operand_kind_scoped(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<String> {
    let vars = scope_vars(scope);
    let env = ty::InferEnv {
        vars: &vars,
        fns: &ctx.fn_ret_types,
        structs: &ctx.struct_fields,
    };
    ty::synth_container_kind(&env, expr)
}

/// The `Result`/`Option` kind named by a declared return-type annotation, or `None` when it is not a
/// container (empty / `Any` / a concrete non-container / a generic parameter) — the accept direction.
fn return_container_kind(rty: &str) -> Option<&'static str> {
    let r = rty.trim();
    if r.starts_with("Result") {
        Some("Result")
    } else if r.starts_with("Option") {
        Some("Option")
    } else {
        None
    }
}

/// Generic-parameter conflict at a call to a generic function, via the bidirectional core's
/// monomorphization (`ty::generic_call_conflict`). Returns `(param, first, second)` on a clash, else
/// `None` (including for a non-generic callee). Borrows `ctx` immutably; the caller emits.
fn generic_conflict_scoped(
    callee: &str,
    args: &[Expr],
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<(String, String, String)> {
    let generics = ctx.fn_generics.get(callee)?;
    let params = ctx.fn_params.get(callee)?;
    let vars = scope_vars(scope);
    let env = ty::InferEnv {
        vars: &vars,
        fns: &ctx.fn_ret_types,
        structs: &ctx.struct_fields,
    };
    ty::generic_call_conflict(&env, generics, params, args)
}

/// Phase-1 trait-bound check at a call site: for each generic of `callee` that declares a bound, if the
/// argument pins it to a KNOWN concrete type that is a USER nominal type (struct/enum) whose required
/// trait is DECLARED in this program and has NO matching `impl`, return `(generic, concrete, trait)` for
/// `ANUBIS_TRAIT_BOUND_UNSATISFIED`. Accept-biased on EVERY axis (the four gates below), so an unknown
/// concrete type, a built-in primitive, or a foreign trait never falsely rejects a valid program.
fn bound_unsatisfied_scoped(
    callee: &str,
    args: &[Expr],
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<(String, String, String)> {
    let bounds = ctx.fn_bounds.get(callee)?;
    let generics = ctx.fn_generics.get(callee)?;
    let params = ctx.fn_params.get(callee)?;
    let vars = scope_vars(scope);
    let env = ty::InferEnv {
        vars: &vars,
        fns: &ctx.fn_ret_types,
        structs: &ctx.struct_fields,
    };
    let bindings = ty::generic_call_bindings(&env, generics, params, args);
    for (g, traits) in bounds {
        // (1) The generic must be pinned to a KNOWN concrete type at this call (else accept — a
        // return-only/nested/unpinnable generic contributes no binding).
        let concrete = match bindings.get(g) {
            Some(c) => c,
            None => continue,
        };
        // (3) Only a USER nominal type is checked: a declared struct (ALL structs are in `struct_fields`,
        // unlike `type_generics` which holds only GENERIC types) or a user-declared enum (`enum_variants`
        // minus the two built-in `Option`/`Result`). A built-in primitive (u32/i64/f64/bool/string/char/…)
        // is in NEITHER, so it is accepted unconditionally — a primitive never appears as `impl Ord for
        // u32` in source, and demanding one would invert the language's accept-bias. Strip any generic
        // args (`Pair<u32>` → `Pair`).
        let base = concrete.split('<').next().unwrap_or(concrete).trim();
        let is_user_type = ctx.struct_fields.contains_key(base)
            || (ctx.enum_variants.contains_key(base) && base != "Option" && base != "Result");
        if !is_user_type {
            continue;
        }
        for tr in traits {
            // (2) A trait this program does not DECLARE is foreign/std — its impl set is not enumerable,
            // so accept (mirrors check_trait_env's "a trait we cannot see is not checked").
            if !ctx.declared_traits.contains(tr) {
                continue;
            }
            // (4) The type is KNOWN and the trait is enumerable, yet no `impl trait for base` exists.
            if !ctx.trait_impls.contains(&(tr.clone(), base.to_string())) {
                return Some((g.clone(), base.to_string(), tr.clone()));
            }
        }
    }
    None
}

/// Trait coherence + missing-required-method, over the [`TraitEnv`] captured before `resolve_traits`
/// erased it. Both are STRUCTURAL (no type inference): they read only trait declarations and impl
/// blocks. Fail-closed toward accept — an `impl` of a trait NOT declared in this program is skipped
/// (we never reject on a trait we cannot see), and a trait with no required methods can never be
/// missing one. ENFORCING (`shadow_gated=false`): the corpus shadow diff is UNEXPECTED=0 and NO
/// existing program triggers either check — verified across examples/, tests/fixtures/, and the
/// self-host source anubis_sh.anb — so they fire only on the two `EXPECT: FAIL` trait fixtures.
fn check_trait_env(env: &crate::frontend::TraitEnv, ctx: &mut SemanticContext) {
    use std::collections::BTreeSet;
    // Coherence: two `impl Trait for Type` for the same (trait, type) pair conflict.
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for imp in &env.impls {
        if !seen.insert((imp.trait_name.as_str(), imp.type_name.as_str())) {
            ctx.emit(
                SemanticDiagnostic {
                    code: Some("ANUBIS_TRAIT_OVERLAP".into()),
                    message: format!(
                        "conflicting implementations of trait `{}` for type `{}` — only one \
                         `impl {} for {}` is allowed",
                        imp.trait_name, imp.type_name, imp.trait_name, imp.type_name
                    ),
                    span: None,
                },
                false,
            );
        }
    }
    // Missing method: an `impl Trait for Type` must provide every REQUIRED (bodyless) trait method.
    for imp in &env.impls {
        // Fail-closed toward accept: a trait this program does not declare is not checked.
        let Some(decl) = env.traits.get(&imp.trait_name) else {
            continue;
        };
        let provided: BTreeSet<&str> = imp.methods.iter().map(String::as_str).collect();
        for req in &decl.required {
            if !provided.contains(req.as_str()) {
                ctx.emit(
                    SemanticDiagnostic {
                        code: Some("ANUBIS_TRAIT_MISSING_METHOD".into()),
                        message: format!(
                            "`impl {} for {}` is missing required method `{}` of trait `{}`",
                            imp.trait_name, imp.type_name, req, imp.trait_name
                        ),
                        span: None,
                    },
                    false,
                );
            }
        }
    }
}

/// Emit `ANUBIS_GENERIC_ARITY` when `annotation` instantiates a user generic type with the wrong
/// number of type arguments. No-op for a bare parameter / built-in container / matching arity, and
/// no-op on an unknown/`Any` base — fail-closed toward accept. ENFORCING: the corpus shadow diff is
/// UNEXPECTED=0 (no existing program, including the self-host source, triggers it — verified), so it
/// fires only on the `EXPECT: FAIL` generic-arity fixture. Kept as one call so every annotation site
/// is a single line.
fn check_generic_arity_annotation(
    annotation: &str,
    span: Option<(usize, usize)>,
    ctx: &mut SemanticContext,
) {
    if let Some((base, declared, given)) =
        ty::generic_arity_mismatch(annotation, &ctx.type_generics)
    {
        ctx.emit(
            SemanticDiagnostic {
                code: Some("ANUBIS_GENERIC_ARITY".into()),
                message: format!(
                    "generic type `{base}` expects {declared} type argument(s), but {given} \
                     {} supplied",
                    if given == 1 { "was" } else { "were" }
                ),
                span,
            },
            false,
        );
    }
}

/// Walk expressions for A+ call typing + match exhaustiveness (fail-closed).
fn check_expr_semantics(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &mut SemanticContext,
) {
    match expr {
        Expr::Call { callee, args } => {
            if let Some(param_tys) = ctx.fn_params.get(callee).cloned() {
                if args.len() != param_tys.len() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_ARITY_MISMATCH".into()),
                        message: format!(
                            "function `{}` expects {} argument(s), got {}",
                            callee,
                            param_tys.len(),
                            args.len()
                        ),
                        span: None,
                    });
                } else {
                    for (i, (arg, expected)) in args.iter().zip(param_tys.iter()).enumerate() {
                        if let Some(got) = infer_expr_type_scoped(arg, scope) {
                            if !types_assignable(expected, &got) {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                    message: format!(
                                        "type mismatch: argument {} of `{}` expects `{}`, got `{}`",
                                        i, callee, expected, got
                                    ),
                                    span: None,
                                });
                            }
                        } else if let Some(got) = check_mismatch_scoped(arg, expected, scope, ctx) {
                            // The bidirectional core can type a `Call`/`Index`/`FieldAccess` argument
                            // the flat inference above returned `None` for (e.g.
                            // `takes_u32(returns_string())`). PROMOTED to enforcing — corpus shadow
                            // diff UNEXPECTED=0.
                            ctx.emit(
                                SemanticDiagnostic {
                                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                    message: format!(
                                        "type mismatch: argument {} of `{}` expects `{}`, got `{}`",
                                        i, callee, expected, got
                                    ),
                                    span: None,
                                },
                                false,
                            );
                        }
                    }
                }
            } else if let Some(arity) = scope.get(callee).and_then(|b| b.closure_arity) {
                // Direct call of a closure-valued local (`let f = |x, y| …; f(1)`): arity-check it.
                // Higher-order use (`map(xs, f)`) is an internal call, not a source `f(args)`, so it
                // still pads — matching the strict-direct / pad-higher-order arity policy.
                if args.len() != arity {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_ARITY_MISMATCH".into()),
                        message: format!(
                            "closure `{}` expects {} argument(s), got {}",
                            callee,
                            arity,
                            args.len()
                        ),
                        span: None,
                    });
                }
            }
            // Generic-parameter conflict: a type parameter used in two argument positions bound to two
            // incompatible concrete arguments (`fn same<T>(a: T, b: T)` called `same(1, "x")`). This is
            // the checker-only monomorphization the captured generics enable — the flat arg checks
            // above accept it (a bare `T` is compatible with everything). Fires only on two definitely-
            // incompatible CONCRETE args; an unknown/`Any`/generic arg is not a conflict, so it is
            // fail-closed toward accept. ENFORCING (`shadow_gated=false`): the corpus shadow diff is
            // UNEXPECTED=0 — no existing program triggers it (verified across examples, fixtures, and
            // the self-host source), so it fires only on the `EXPECT: FAIL` generic-conflict fixture.
            if let Some((param, first, second)) = generic_conflict_scoped(callee, args, scope, ctx) {
                ctx.emit(
                    SemanticDiagnostic {
                        code: Some("ANUBIS_GENERIC_CONFLICT".into()),
                        message: format!(
                            "generic parameter `{param}` of `{callee}` is used as both `{first}` and \
                             `{second}` at the same call",
                        ),
                        span: None,
                    },
                    false,
                );
            }
            // Phase-1 trait bound (ENFORCING): a generic bound to a KNOWN user type (struct/enum) whose
            // required trait is declared in-program and has no `impl` is rejected. Accept-biased on all
            // four gates in `bound_unsatisfied_scoped` (an unknown/unpinnable type, a built-in primitive,
            // or a foreign trait all accept), so no valid program falsely rejects. Corpus-inert: no
            // committed program declares a bounded generic, so the shadow diff was UNEXPECTED=0 and the
            // verdict-diff is 0-flip. The self-host schema ignores `generic_bounds`, so the fixpoint is
            // unmoved (checker-only field, absorbed by `..` in `project_item`).
            if let Some((generic, concrete, tr)) = bound_unsatisfied_scoped(callee, args, scope, ctx) {
                ctx.emit(
                    SemanticDiagnostic {
                        code: Some("ANUBIS_TRAIT_BOUND_UNSATISFIED".into()),
                        message: format!(
                            "generic parameter `{generic}` of `{callee}` is bound to `{concrete}`, which \
                             does not implement the required trait `{tr}` (no `impl {tr} for {concrete}` \
                             in this program)",
                        ),
                        span: None,
                    },
                    false,
                );
            }
            for a in args {
                check_expr_semantics(a, scope, ctx);
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            check_expr_semantics(lhs, scope, ctx);
            check_expr_semantics(rhs, scope, ctx);
            // B1: arithmetic/bitwise operators require numeric operands. `+` is overloaded
            // (string/list concat), comparisons and `&&`/`||` are lenient, so only these numeric-only
            // operators are checked — and only against a statically-known string/list/map operand.
            if matches!(
                op.as_str(),
                "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>"
            ) {
                for operand in [lhs.as_ref(), rhs.as_ref()] {
                    if let Some(bad) = static_non_numeric_operand(operand, scope) {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_TYPE_MISMATCH".into()),
                            message: format!(
                                "operator `{}` requires numeric operands, but an operand has type `{}`",
                                op, bad
                            ),
                            span: None,
                        });
                        break;
                    }
                }
            }
        }
        Expr::Unary { op, expr } => {
            check_expr_semantics(expr, scope, ctx);
            // B1: unary `-` requires a numeric operand.
            if op == "-" {
                if let Some(bad) = static_non_numeric_operand(expr, scope) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_TYPE_MISMATCH".into()),
                        message: format!("unary `-` requires a numeric operand, got `{}`", bad),
                        span: None,
                    });
                }
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            check_expr_semantics(cond, scope, ctx);
            check_expr_semantics(then, scope, ctx);
            check_expr_semantics(else_, scope, ctx);
            // Arm-join type conflict. `if c { "a" } else { 1 }` is silently accepted by the flat
            // inference; the bidirectional core unifies the branch types and surfaces a genuine
            // cross-category clash. PROMOTED to enforcing (`shadow_gated=false`): the corpus shadow
            // diff reported UNEXPECTED=0 (fires on zero currently-accepted programs).
            if let Some((a, b)) = arm_join_conflict_scoped(&[&**then, &**else_], scope, ctx) {
                ctx.emit(
                    SemanticDiagnostic {
                        code: Some("ANUBIS_ARM_TYPE_CONFLICT".into()),
                        message: format!(
                            "type mismatch: `if` branches have incompatible types `{}` and `{}`",
                            a, b
                        ),
                        span: None,
                    },
                    false,
                );
            }
        }
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                check_expr_semantics(e, scope, ctx);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                check_expr_semantics(k, scope, ctx);
                check_expr_semantics(v, scope, ctx);
            }
        }
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                check_expr_semantics(f, scope, ctx);
            }
        }
        Expr::Index { base, index } => {
            check_expr_semantics(base, scope, ctx);
            check_expr_semantics(index, scope, ctx);
            // B1: only lists, strings, maps, and structs are indexable. A statically-known numeric
            // or bool base is a type error (dynamic bases are left to the fail-closed runtime).
            if let Some(bad) = static_non_indexable(base, scope) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                    message: format!(
                        "cannot index a value of type `{}` (only lists, strings, and maps are indexable)",
                        bad
                    ),
                    span: None,
                });
            }
        }
        Expr::FieldAccess { base, .. } => check_expr_semantics(base, scope, ctx),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            check_expr_semantics(scrutinee, scope, ctx);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    check_expr_semantics(guard, scope, ctx);
                }
                check_expr_semantics(&arm.body, scope, ctx);
            }
            check_match_exhaustiveness(scrutinee, arms, scope, ctx);
            // Arm-join type conflict across the match arms' bodies (same core as the `if` branch join
            // above). PROMOTED to enforcing — corpus shadow diff UNEXPECTED=0.
            let bodies: Vec<&Expr> = arms.iter().map(|arm| &arm.body).collect();
            if let Some((a, b)) = arm_join_conflict_scoped(&bodies, scope, ctx) {
                ctx.emit(
                    SemanticDiagnostic {
                        code: Some("ANUBIS_ARM_TYPE_CONFLICT".into()),
                        message: format!(
                            "type mismatch: `match` arms have incompatible types `{}` and `{}`",
                            a, b
                        ),
                        span: None,
                    },
                    false,
                );
            }
        }
        Expr::CallExpr { callee, args } => {
            check_expr_semantics(callee, scope, ctx);
            for a in args {
                check_expr_semantics(a, scope, ctx);
            }
            // Direct method call `recv.method(args)`: arity-check when the method name resolves to a
            // single known arity. `self` is the receiver, so `args.len() + 1` must equal that arity.
            if let Expr::FieldAccess { field, .. } = &**callee {
                if let Some(Some(arity)) = ctx.method_arities.get(field).copied() {
                    if args.len() + 1 != arity {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_ARITY_MISMATCH".into()),
                            message: format!(
                                "method `{}` expects {} argument(s), got {}",
                                field,
                                arity.saturating_sub(1),
                                args.len()
                            ),
                            span: None,
                        });
                    }
                }
            }
        }
        Expr::Declassify { inner, .. } => check_expr_semantics(inner, scope, ctx),
        Expr::Cast { expr, .. } => check_expr_semantics(expr, scope, ctx),
        Expr::StructLiteral { name, fields, .. } => {
            let mut seen = BTreeSet::new();
            for (fname, fexpr) in fields {
                if !seen.insert(fname.clone()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_DUPLICATE_FIELD".into()),
                        message: format!(
                            "duplicate field `{}` in `{}` struct literal",
                            fname, name
                        ),
                        span: None,
                    });
                }
                check_expr_semantics(fexpr, scope, ctx);
            }
        }
        // Descend into closure and block bodies so B1's constant-type checks apply there too —
        // otherwise `|q| 5[0]` or `{ let z = 7[2]; z }` slipped past the checker and crashed at run.
        // A `?` inside a closure early-returns from the CLOSURE (not the enclosing function), so clear
        // the enclosing return type across the lambda body — the typed-`?` check must not compare a
        // closure's `?` against the outer function's declared return.
        Expr::Lambda { body, .. } => {
            let saved = ctx.current_fn_return.take();
            check_expr_semantics(body, scope, ctx);
            ctx.current_fn_return = saved;
        }
        Expr::Block { stmts, tail } => check_block_exprs(stmts, tail.as_deref(), scope, ctx),
        // Typed `?`: the operator early-returns the operand's `Err`/`None` VERBATIM from the enclosing
        // function (run.rs lowers it with no coercion), so propagating an `Option`'s `None` out of a
        // `-> Result<…>` function — or an `Err` out of a `-> Option<…>` function — is a genuine kind
        // mismatch. Checked only when the enclosing return is itself a `Result`/`Option` (the
        // `ANUBIS_TRY_OUTSIDE_RESULT` check already governs `?` under a concrete non-container return,
        // and an `Any`/generic/absent return is dynamic ⇒ skip). Accept-biased: an operand whose
        // container kind the core cannot see resolves to `None` and never mismatches.
        Expr::Try(inner) => {
            check_expr_semantics(inner, scope, ctx);
            // `return_container_kind` yields a `&'static str`, so this holds no borrow of `ctx`.
            let ret_kind = ctx.current_fn_return.as_deref().and_then(return_container_kind);
            if let Some(ret_kind) = ret_kind {
                let op_kind = try_operand_kind_scoped(inner, scope, ctx);
                if let Some(op_kind) = op_kind {
                    if op_kind != ret_kind {
                        // PROMOTED to enforcing (`shadow_gated=false`) — corpus shadow diff
                        // UNEXPECTED=0.
                        ctx.emit(
                            SemanticDiagnostic {
                                code: Some("ANUBIS_TRY_TYPE_MISMATCH".into()),
                                message: format!(
                                    "`?` propagates a `{op_kind}` value but the enclosing function returns `{ret_kind}`; \
                                     the propagated `{}` cannot be returned from a `{ret_kind}` function",
                                    if op_kind == "Option" { "None" } else { "Err" }
                                ),
                                span: None,
                            },
                            false,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

/// Walk the expressions inside a block / closure body for B1's constant-type checks. The checks are
/// constant-only (they never flag a variable), so re-using the enclosing scope is sound.
fn check_block_exprs(
    stmts: &[Stmt],
    tail: Option<&Expr>,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &mut SemanticContext,
) {
    for s in stmts {
        check_stmt_exprs(s, scope, ctx);
    }
    if let Some(t) = tail {
        check_expr_semantics(t, scope, ctx);
    }
}

fn check_stmt_exprs(s: &Stmt, scope: &BTreeMap<String, ScopeBinding>, ctx: &mut SemanticContext) {
    use crate::frontend::ForSource;
    match s {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
            check_expr_semantics(init, scope, ctx)
        }
        Stmt::Assign { value, .. } => check_expr_semantics(value, scope, ctx),
        Stmt::ExprStmt(e) => check_expr_semantics(e, scope, ctx),
        Stmt::If { cond, then, else_ } => {
            check_expr_semantics(cond, scope, ctx);
            check_block_exprs(then, None, scope, ctx);
            if let Some(e) = else_ {
                check_block_exprs(e, None, scope, ctx);
            }
        }
        Stmt::While { cond, body, .. } => {
            check_expr_semantics(cond, scope, ctx);
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::WhileLet { expr, body, .. } => {
            check_expr_semantics(expr, scope, ctx);
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::Loop { body, .. } => check_block_exprs(body, None, scope, ctx),
        Stmt::For { source, body, .. } => {
            match source {
                ForSource::Range { start, end } => {
                    check_expr_semantics(start, scope, ctx);
                    check_expr_semantics(end, scope, ctx);
                }
                ForSource::Collection { expr } => check_expr_semantics(expr, scope, ctx),
            }
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            check_block_exprs(body, None, scope, ctx)
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                check_block_exprs(b, None, scope, ctx);
            }
        }
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
    }
}

fn check_match_exhaustiveness(
    scrutinee: &Expr,
    arms: &[crate::frontend::MatchArm],
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &mut SemanticContext,
) {
    // An unguarded irrefutable arm (`_` or a bare binding) → exhaustive.
    if arms
        .iter()
        .any(|a| a.pattern.is_irrefutable() && a.guard.is_none())
    {
        return;
    }
    // Determine enum type of scrutinee.
    let enum_name = match scrutinee {
        Expr::Var(n) => scope
            .get(n)
            .and_then(|b| b.info.ty.clone())
            .filter(|t| ctx.enum_variants.contains_key(t)),
        Expr::EnumConstruct { enum_name, .. } => Some(enum_name.clone()),
        _ => infer_expr_type_scoped(scrutinee, scope).filter(|t| ctx.enum_variants.contains_key(t)),
    };
    // If the scrutinee's type is unknown, fall back to arm-based inference: if the arms cover
    // variants of a declared enum (or built-in Option/Result), that is the enum being matched.
    let enum_name = enum_name.or_else(|| {
        arms.iter().find_map(|arm| {
            let mut pairs = Vec::new();
            arm.pattern.covered_enum_variants(&mut pairs);
            pairs
                .into_iter()
                .map(|(en, _)| en)
                .find(|en| ctx.enum_variants.contains_key(en))
        })
    });
    let Some(enum_name) = enum_name else {
        return; // unknown scrutinee type — do not false-positive
    };
    let Some(all_variants) = ctx.enum_variants.get(&enum_name).cloned() else {
        return;
    };
    let mut covered = BTreeSet::new();
    for arm in arms {
        // A guarded arm may not fire, so it cannot be counted toward exhaustiveness.
        if arm.guard.is_some() {
            continue;
        }
        let mut pairs = Vec::new();
        arm.pattern.covered_enum_variants(&mut pairs);
        for (en, variant) in pairs {
            if en == enum_name {
                covered.insert(variant);
            }
        }
    }
    let missing: Vec<_> = all_variants
        .into_iter()
        .filter(|v| !covered.contains(v))
        .collect();
    if !missing.is_empty() {
        ctx.diagnostics.push(SemanticDiagnostic {
            code: Some("ANUBIS_MATCH_NON_EXHAUSTIVE".into()),
            message: format!(
                "non-exhaustive match on `{}`: missing variant(s) {} (add arms or `_`)",
                enum_name,
                missing.join(", ")
            ),
            span: None,
        });
    }
}

fn declassify_source(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
) -> Option<String> {
    match expr {
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } if policy.is_some() && reason.is_some() => {
            expr_taint_source(inner, scope, tainting_fns, param_return_taint)
        }
        _ => None,
    }
}

/// Recursively thread a value-position block's STATEMENT flow into `local` for the INTEGRITY walker, at
/// parity with the enforcing `analyze_stmts` control-flow-merge discipline so the two never disagree.
/// Straight-line `let`/`Assign{Var}` set/CLEAR the label; nested control-flow (`if`/`while`/`while let`/
/// `loop`/`for`) is MAY-merged via `merge_taint_over` over EXACTLY the oracle's path set — If: `[then,
/// (else | snap)]` (a value cleared on BOTH branches is precisely cleared; a uniform base path would OR it
/// back), loops: `[snap, body]` (snap = the zero-iteration path). A `for` loop var inherits the
/// collection/range label (mirrors the oracle's For arm); a `hybrid` block is left UNMERGED (the oracle
/// recurses+restores it, so merging here would be STRICTER than the enforcing pass → a spurious reject).
/// Block-`let`s AND destructured pattern vars are seeded with their REAL span so `merge_taint_over`'s
/// span-identity distinguishes a branch-nested SHADOW (a new binding — its label must not leak to the
/// outer same-named binding) from a REASSIGNMENT (same span — its label carries). This closes the
/// value-position nested-control-flow fail-open the former hand-rolled `_ => {}` left open. Non-`Var`
/// assign, `match`/`if let`/`@research`/`@exploit` used as a STATEMENT, and cross-iteration loop carry
/// stay named residuals — each SHARED with the enforcing pass, so ignoring them keeps parity (never
/// stricter → no new false positive).
fn walk_block_taint(
    stmts: &[Stmt],
    local: &mut BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_tainting_fns: &BTreeSet<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                name, ty, init, span,
            } => {
                seed_one_let(name, ty.as_deref(), init, local, tainting_fns, param_return_taint, method_tainting_fns);
                if let Some(b) = local.get_mut(name) {
                    b.info.span = Some((span.start, span.end));
                }
            }
            Stmt::LetPattern { pattern, init, span } => {
                let label = expr_taint_source_m(init, local, tainting_fns, param_return_taint, method_tainting_fns);
                seed_taint_pattern(local, pattern, &label);
                for n in pattern.bound_names() {
                    if let Some(b) = local.get_mut(&n) {
                        b.info.span = Some((span.start, span.end));
                    }
                }
            }
            Stmt::Assign {
                target: Expr::Var(name),
                value,
            } => {
                let label = expr_taint_source_m(value, local, tainting_fns, param_return_taint, method_tainting_fns);
                if let Some(b) = local.get_mut(name) {
                    b.info.tainted = label.is_some();
                    if label.is_some() {
                        b.info.declassified = false;
                    }
                    b.info.taint_source = label;
                }
            }
            Stmt::Assign { target, value } => {
                // Value-position dual of the statement-level place-assignment fix: a non-`Var` target
                // (`buf[0] = k` inside a value block) MAY-taints the root container binding (set-only).
                if let Some(root) = assign_target_root(target) {
                    if let Some(src) =
                        expr_taint_source_m(value, local, tainting_fns, param_return_taint, method_tainting_fns)
                    {
                        if let Some(b) = local.get_mut(root) {
                            b.info.tainted = true;
                            b.info.taint_source = Some(src);
                            b.info.declassified = false;
                        }
                    }
                }
            }
            Stmt::If { then, else_, .. } => {
                let mut then_c = local.clone();
                walk_block_taint(then, &mut then_c, tainting_fns, param_return_taint, method_tainting_fns);
                let else_c = if let Some(eb) = else_ {
                    let mut c = local.clone();
                    walk_block_taint(eb, &mut c, tainting_fns, param_return_taint, method_tainting_fns);
                    c
                } else {
                    local.clone()
                };
                merge_taint_over(local, &[&then_c, &else_c]);
            }
            Stmt::While { body, .. } | Stmt::Loop { body, .. } => {
                let snap = local.clone();
                let mut body_c = local.clone();
                walk_block_taint(body, &mut body_c, tainting_fns, param_return_taint, method_tainting_fns);
                merge_taint_over(local, &[&snap, &body_c]);
            }
            Stmt::WhileLet { pattern, body, .. } => {
                let snap = local.clone();
                let mut body_c = local.clone();
                // Oracle seeds `while let` pattern vars CLEAN (analyze_stmts WhileLet arm).
                seed_taint_pattern(&mut body_c, pattern, &None);
                walk_block_taint(body, &mut body_c, tainting_fns, param_return_taint, method_tainting_fns);
                merge_taint_over(local, &[&snap, &body_c]);
            }
            Stmt::For {
                var, body, source, ..
            } => {
                let taint_src = match source {
                    crate::frontend::ForSource::Range { start, .. } => {
                        expr_taint_source_m(start, local, tainting_fns, param_return_taint, method_tainting_fns)
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_taint_source_m(expr, local, tainting_fns, param_return_taint, method_tainting_fns)
                    }
                };
                let snap = local.clone();
                let mut body_c = local.clone();
                body_c.insert(
                    var.clone(),
                    ScopeBinding {
                        info: BindingInfo {
                            name: var.clone(),
                            ty: None,
                            mode: String::new(),
                            tainted: taint_src.is_some(),
                            taint_source: taint_src,
                            declassified: false,
                            span: None,
                        },
                        closure_arity: None,
                        secret: false,
                    },
                );
                walk_block_taint(body, &mut body_c, tainting_fns, param_return_taint, method_tainting_fns);
                merge_taint_over(local, &[&snap, &body_c]);
            }
            _ => {}
        }
    }
}

/// Confidentiality dual of [`walk_block_taint`] — see its doc for the merge discipline and residuals.
/// Tracks the `secret` bool; a `for` over a secret collection binds a secret element; `while let` pattern
/// vars are seeded non-secret (oracle parity).
fn walk_block_secret(
    stmts: &[Stmt],
    local: &mut BTreeMap<String, ScopeBinding>,
    secret_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_secret_fns: &BTreeSet<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                name, ty, init, span,
            } => {
                seed_one_let_secret(name, ty.as_deref(), init, local, secret_fns, param_return_taint, method_secret_fns);
                if let Some(b) = local.get_mut(name) {
                    b.info.span = Some((span.start, span.end));
                }
            }
            Stmt::LetPattern { pattern, init, span } => {
                let secret = expr_secret_source_m(init, local, secret_fns, param_return_taint, method_secret_fns).is_some();
                seed_secret_pattern(local, pattern, secret);
                for n in pattern.bound_names() {
                    if let Some(b) = local.get_mut(&n) {
                        b.info.span = Some((span.start, span.end));
                    }
                }
            }
            Stmt::Assign {
                target: Expr::Var(name),
                value,
            } => {
                let secret = expr_secret_source_m(value, local, secret_fns, param_return_taint, method_secret_fns).is_some();
                if let Some(b) = local.get_mut(name) {
                    b.secret = secret;
                }
            }
            Stmt::Assign { target, value } => {
                // Value-position dual: a non-`Var` place-assignment of a secret MAY-labels the root
                // container secret (set-only).
                if let Some(root) = assign_target_root(target) {
                    if expr_secret_source_m(value, local, secret_fns, param_return_taint, method_secret_fns).is_some() {
                        if let Some(b) = local.get_mut(root) {
                            b.secret = true;
                        }
                    }
                }
            }
            Stmt::If { then, else_, .. } => {
                let mut then_c = local.clone();
                walk_block_secret(then, &mut then_c, secret_fns, param_return_taint, method_secret_fns);
                let else_c = if let Some(eb) = else_ {
                    let mut c = local.clone();
                    walk_block_secret(eb, &mut c, secret_fns, param_return_taint, method_secret_fns);
                    c
                } else {
                    local.clone()
                };
                merge_taint_over(local, &[&then_c, &else_c]);
            }
            Stmt::While { body, .. } | Stmt::Loop { body, .. } => {
                let snap = local.clone();
                let mut body_c = local.clone();
                walk_block_secret(body, &mut body_c, secret_fns, param_return_taint, method_secret_fns);
                merge_taint_over(local, &[&snap, &body_c]);
            }
            Stmt::WhileLet { pattern, body, .. } => {
                let snap = local.clone();
                let mut body_c = local.clone();
                seed_secret_pattern(&mut body_c, pattern, false);
                walk_block_secret(body, &mut body_c, secret_fns, param_return_taint, method_secret_fns);
                merge_taint_over(local, &[&snap, &body_c]);
            }
            Stmt::For {
                var, body, source, ..
            } => {
                let secret_src = match source {
                    crate::frontend::ForSource::Range { start, .. } => {
                        expr_secret_source_m(start, local, secret_fns, param_return_taint, method_secret_fns).is_some()
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_secret_source_m(expr, local, secret_fns, param_return_taint, method_secret_fns).is_some()
                    }
                };
                let snap = local.clone();
                let mut body_c = local.clone();
                body_c.insert(
                    var.clone(),
                    ScopeBinding {
                        info: BindingInfo {
                            name: var.clone(),
                            ty: None,
                            mode: String::new(),
                            tainted: false,
                            taint_source: None,
                            declassified: false,
                            span: None,
                        },
                        closure_arity: None,
                        secret: secret_src,
                    },
                );
                walk_block_secret(body, &mut body_c, secret_fns, param_return_taint, method_secret_fns);
                merge_taint_over(local, &[&snap, &body_c]);
            }
            _ => {}
        }
    }
}

/// The taint-source label of an expression, or `None` if clean — the 4-arg wrapper over
/// [`expr_taint_source_m`], with no impl-method return-taint awareness (the pre-#67 behavior). Passing an
/// empty method set makes the new `CallExpr` method-return arm inert, so summary computation and every
/// un-migrated caller behaves EXACTLY as before #67 — keeping the free-fn return-summary fixpoints
/// byte-identical, hence the self-host binary fixpoint dc680001 unmoved. Every existing caller uses this;
/// only the enforcing egress/sink and enforcing let/seed sites call the `_m` variant with
/// `ctx.method_tainting_fns` to catch the getter/accessor exfil. (`&BTreeSet::new()` is a zero-alloc
/// temporary that lives for the call.)
fn expr_taint_source(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
) -> Option<String> {
    expr_taint_source_m(
        expr,
        scope,
        tainting_fns,
        param_return_taint,
        &BTreeSet::new(),
    )
}

/// The taint-source label of an expression, or `None` if clean.
///
/// - `tainting_fns`: functions whose return carries INTERNAL taint (`sink(get_secret())`).
/// - `param_return_taint`: functions → which formal params flow to the return value. A call is
///   tainted from argument i only when i is in this set (Phase-3 A2). When the map has no entry for
///   a callee (builtins / bootstrap before the summary runs), any tainted argument conservatively
///   taints the call (fail-closed over-approx). When the map HAS an entry (even empty), only the
///   summarized params apply — so `fn ignore(x){return 5;}` no longer falsely taints `ignore(secret)`.
/// - `method_tainting_fns`: impl methods whose RETURN carries internally-minted taint (#67); consulted
///   in the `CallExpr` arm so `r.tag()` is a taint source. Empty via the 4-arg wrapper.
fn expr_taint_source_m(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_tainting_fns: &BTreeSet<String>,
) -> Option<String> {
    match expr {
        Expr::Var(name) => scope
            .get(name)
            .and_then(|binding| binding.info.taint_source.clone())
            .filter(|_| scope.get(name).is_some_and(|binding| binding.info.tainted)),
        Expr::Binary { lhs, rhs, .. } => {
            expr_taint_source_m(lhs, scope, tainting_fns, param_return_taint, method_tainting_fns)
                .or_else(|| expr_taint_source_m(rhs, scope, tainting_fns, param_return_taint, method_tainting_fns))
        }
        Expr::Unary { expr, .. } => {
            expr_taint_source_m(expr, scope, tainting_fns, param_return_taint, method_tainting_fns)
        }
        Expr::Call { callee, args } => {
            // C4: an I/O read is itself a taint source (untrusted input), even with clean args.
            if is_io_taint_source(callee) {
                Some(format!("io source `{callee}`"))
            } else if tainting_fns.contains(callee) {
                Some(format!("return value of `{}`", callee))
            } else if let Some(rets) = param_return_taint.get(callee) {
                // Known user function: only params that the summary says reach the return.
                rets.iter().find_map(|&i| {
                    args.get(i)
                        .and_then(|a| expr_taint_source_m(a, scope, tainting_fns, param_return_taint, method_tainting_fns))
                })
            } else {
                // Builtin / not-yet-summarized: any tainted argument taints the call (conservative).
                args.iter()
                    .find_map(|arg| expr_taint_source_m(arg, scope, tainting_fns, param_return_taint, method_tainting_fns))
            }
        }
        Expr::Tainted { inner, .. } => {
            expr_taint_source_m(inner, scope, tainting_fns, param_return_taint, method_tainting_fns)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_taint_source_m(inner, scope, tainting_fns, param_return_taint, method_tainting_fns)
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                None // cleared
            } else {
                expr_taint_source_m(inner, scope, tainting_fns, param_return_taint, method_tainting_fns)
            }
        }
        Expr::TaintSource { label } => Some(label.clone()),
        // Indexing/field-access on a tainted binding must not launder the taint — without these
        // arms, `sink(tainted_arr[i])` / `sink(tainted_struct.field)` fell through to the catch-all
        // below and silently escaped `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` (a real fail-open gap).
        // `Index` checks both operands (like `Binary`, not `Unary`'s single-operand shape): a tainted
        // INDEX into an otherwise-clean array (`sink(arr[tainted_offset])`) is an equally real leak.
        // Whole-binding granularity only: a struct's OWN field individually declared `tainted<T>` in
        // its type definition does not, by itself, make `.field` access on an otherwise-clean instance
        // tainted — only a binding whose own `let`/param annotation (or tainted initializer) seeded it
        // tainted propagates here, matching how every other walker in this file treats field/struct
        // definitions as opaque to flow analysis.
        Expr::Index { base, index } => {
            expr_taint_source_m(base, scope, tainting_fns, param_return_taint, method_tainting_fns)
                .or_else(|| expr_taint_source_m(index, scope, tainting_fns, param_return_taint, method_tainting_fns))
        }
        Expr::FieldAccess { base, .. } => {
            expr_taint_source_m(base, scope, tainting_fns, param_return_taint, method_tainting_fns)
        }
        // A cast reinterprets a value without changing its provenance — `secret as u64` is still the
        // secret. Without this arm, `sink(s as u64)` (and `return s as u64` interprocedurally)
        // laundered taint through the cast (adversary-found fail-open, both intra- and inter-procedural).
        Expr::Cast { expr, .. } => expr_taint_source_m(expr, scope, tainting_fns, param_return_taint, method_tainting_fns),
        // Composite value propagation: a container/aggregate carries taint if ANY sub-expression it is
        // built from does. Without these, `sink([tainted])` / `sink(Struct{f: tainted})` /
        // `sink(Enum::V(tainted))` / `sink({k: tainted})` laundered taint through the aggregate — an
        // adversary-shaped bypass (the composite-laundering boundary the review confirmed, closed here
        // symmetrically with the secrecy dual and the interprocedural param-flow summary).
        Expr::ArrayLiteral { elements } => elements
            .iter()
            .find_map(|e| expr_taint_source_m(e, scope, tainting_fns, param_return_taint, method_tainting_fns)),
        Expr::StructLiteral { fields, .. } => fields
            .iter()
            .find_map(|(_, e)| expr_taint_source_m(e, scope, tainting_fns, param_return_taint, method_tainting_fns)),
        Expr::EnumConstruct { fields, .. } => fields
            .iter()
            .find_map(|e| expr_taint_source_m(e, scope, tainting_fns, param_return_taint, method_tainting_fns)),
        Expr::MapLiteral { entries, .. } => entries.iter().find_map(|(k, v)| {
            expr_taint_source_m(k, scope, tainting_fns, param_return_taint, method_tainting_fns)
                .or_else(|| expr_taint_source_m(v, scope, tainting_fns, param_return_taint, method_tainting_fns))
        }),
        // Control-flow value expressions (`match` / `if` / `if let` / block) — SCOPE-AWARE walk.
        // Each arm/branch/block extends a CLONE of the ambient scope: inner bindings (pattern vars,
        // block-local `let`s) SHADOW a same-named outer binding (no false positive — the exact
        // defect that deferred this from the composite slice), and a value passed THROUGH an inner
        // binding is tracked (no laundering). The clone is local to one arm/block — nothing leaks
        // to a sibling arm or to code after the expression. Combine is may-carry (`or_else`): any
        // branch carrying taint taints the whole value. The `if` condition and match guards are
        // control, not value — ignored (explicit-flow model; implicit flows stay a named boundary).
        // Pattern vars inherit the WHOLE scrutinee's label (whole-value granularity, matching the
        // `Index`/`FieldAccess` arms above). A straight-line `Assign` to a Var inside a block is
        // applied to the local clone with the same set/CLEAR discipline as the main analyzer's
        // `Stmt::Assign` arm, so a reassign-to-clean before the tail stays accepted (the committed
        // `taint_reassign_to_clean` contract, in value position) and a reassign-to-tainted is caught.
        // Value-position block: thread the block's statement flow through `walk_block_taint` (the
        // scope-aware stmt walker, at parity with the enforcing `analyze_stmts` merge discipline), then
        // read the tail's label in the block's post-statement scope. This closes the former `_ => {}`
        // fail-open where nested control-flow inside the block (`{ let r=0; if c { r=x; } r }`) was
        // dropped and a laundered value read clean.
        Expr::Block { stmts, tail } => {
            let mut local = scope.clone();
            walk_block_taint(stmts, &mut local, tainting_fns, param_return_taint, method_tainting_fns);
            tail.as_ref()
                .and_then(|t| expr_taint_source_m(t, &local, tainting_fns, param_return_taint, method_tainting_fns))
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let scrut = expr_taint_source_m(scrutinee, scope, tainting_fns, param_return_taint, method_tainting_fns);
            arms.iter().find_map(|arm| {
                let mut local = scope.clone();
                seed_taint_pattern(&mut local, &arm.pattern, &scrut);
                expr_taint_source_m(&arm.body, &local, tainting_fns, param_return_taint, method_tainting_fns)
            })
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let scrut = expr_taint_source_m(scrutinee, scope, tainting_fns, param_return_taint, method_tainting_fns);
            let mut local = scope.clone();
            seed_taint_pattern(&mut local, pattern, &scrut);
            expr_taint_source_m(then, &local, tainting_fns, param_return_taint, method_tainting_fns)
                .or_else(|| expr_taint_source_m(else_, scope, tainting_fns, param_return_taint, method_tainting_fns))
        }
        Expr::If { then, else_, .. } => {
            expr_taint_source_m(then, scope, tainting_fns, param_return_taint, method_tainting_fns)
                .or_else(|| expr_taint_source_m(else_, scope, tainting_fns, param_return_taint, method_tainting_fns))
        }
        // `expr?` unwraps Ok/Some to the inner value, which carries the same provenance (no binding).
        Expr::Try(inner) => expr_taint_source_m(inner, scope, tainting_fns, param_return_taint, method_tainting_fns),
        // A method/closure application (`x.clone()`, `f(a)`) may carry the taint of its receiver/callee
        // or any argument — conservatively surface the first tainted sub-expression (the symmetric
        // intra-procedural twin of the `CallExpr` arm in `expr_param_return_flow`, so `s.clone()` does
        // not launder a tainted value).
        Expr::CallExpr { callee, args } => {
            // #67: an impl method whose RETURN carries internally-minted taint makes
            // `r.tag()` a taint source. Bare-name keyed; inert when method_tainting_fns is empty.
            if let Expr::FieldAccess { field, .. } = callee.as_ref() {
                if method_tainting_fns.contains(field) {
                    return Some(format!("return value of method `{field}`"));
                }
            }
            expr_taint_source_m(callee, scope, tainting_fns, param_return_taint, method_tainting_fns).or_else(|| {
                args.iter()
                    .find_map(|a| expr_taint_source_m(a, scope, tainting_fns, param_return_taint, method_tainting_fns))
            })
        }
        _ => None,
    }
}

/// The sole confidentiality SEED builtin — the constructor of a private value (the dual of the taint
/// I/O sources). Corpus-inert: no committed program calls it, so the whole secrecy flow is silent on
/// the corpus and shadow-clean by construction (like the linear-capability builtins).
const SECRET_SOURCE_NAME: &str = "secret_source";

/// Whether a sink is an EGRESS — external communication (network / shell), the leg-3 surface. A secret
/// reaching one of THESE without declassify is exfiltration; a secret written to a LOCAL file
/// (`write_file`/`memcpy`) is out of scope for this slice (a named boundary — local persistence is a
/// weaker leak than network/shell egress, and folding it in would newly reject more corpus shapes).
/// This is a SUPERSET of the lethal-trifecta leg-3 egress: leg-3 reads `net.send`/`shell` off the
/// effect row (so `http_get`/`http_post`, present here, are NOT yet leg-3 — a named leg-3 residual),
/// whereas this value-flow egress set includes them.
fn is_egress_sink(callee: &str) -> bool {
    // Exact-match only (like `is_sink`) — a substring rule would false-positive on a user function
    // such as `compute_network_stats`. `net.send` builtins ∪ shell builtins = the leg-3 egress set.
    matches!(
        callee,
        "send"
            | "network_send"
            | "connect"
            | "http_get"
            | "http_post"
            | "shell"
            | "exec"
            | "system"
            | "target_run"
    )
}

/// Confidentiality (secrecy) provenance — the DUAL of `expr_taint_source`, and structurally its exact
/// mirror. Returns a source label if `expr` carries PRIVATE data: a `secret_source(..)` seed, a
/// binding whose `secret` flag is set, the return of an interprocedural secret function (`secret_fns`,
/// the dual of `tainting_fns`), or a value derived from any of these (binary / unary / index / field /
/// cast / a call whose secret arguments reach its return). A well-formed `declassify(x, policy,
/// reason)` clears it — the sanctioned release hatch, identical to the taint side, so Let/Assign
/// seeding needs no separate declassify test.
///
/// The `Call` arm mirrors `expr_taint_source` precisely: for a KNOWN user callee summarized in
/// `param_return_taint`, only the argument positions that VALUE-flow to the return carry the secret
/// (value-flow is label-agnostic, so the SAME summary serves both labels — this is what rules out the
/// discard-arg false positive: `send(ignore(secret))` with `fn ignore(x){ 0 }` does not fire); for a
/// builtin / unsummarized callee, any secret argument is conservative (fail-closed-safe — a
/// hash/encoding of a secret is not a release).
///
/// Method-aware form: `method_secret_fns` are impl methods whose RETURN carries a minted secret (#67);
/// the `CallExpr` arm recognizes `v.key()` when `key` is in the set. A caller with no method awareness
/// (e.g. the free-fn return-summary computation, which must stay byte-identical to keep the self-host
/// fixpoint) passes an empty set. (The former 4-arg `expr_secret_source` wrapper was removed once #70
/// routed the last summary caller through `_m` with an empty set — the taint side keeps its wrapper
/// because condition/declassify-trace reads still use it.)
fn expr_secret_source_m(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    secret_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_secret_fns: &BTreeSet<String>,
) -> Option<String> {
    match expr {
        Expr::Var(name) => scope.get(name).filter(|b| b.secret).map(|_| name.clone()),
        Expr::Call { callee, args } => {
            if callee == SECRET_SOURCE_NAME {
                Some(SECRET_SOURCE_NAME.to_string())
            } else if secret_fns.contains(callee) {
                Some(format!("return value of `{callee}`"))
            } else if let Some(rets) = param_return_taint.get(callee) {
                // Known user function: only params the summary says reach the return carry secrecy.
                rets.iter().find_map(|&i| {
                    args.get(i)
                        .and_then(|a| expr_secret_source_m(a, scope, secret_fns, param_return_taint, method_secret_fns))
                })
            } else {
                // Builtin / not-yet-summarized: any secret argument is conservative.
                args.iter()
                    .find_map(|a| expr_secret_source_m(a, scope, secret_fns, param_return_taint, method_secret_fns))
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_secret_source_m(lhs, scope, secret_fns, param_return_taint, method_secret_fns)
                .or_else(|| expr_secret_source_m(rhs, scope, secret_fns, param_return_taint, method_secret_fns))
        }
        Expr::Unary { expr, .. } => expr_secret_source_m(expr, scope, secret_fns, param_return_taint, method_secret_fns),
        Expr::Index { base, index } => {
            expr_secret_source_m(base, scope, secret_fns, param_return_taint, method_secret_fns)
                .or_else(|| expr_secret_source_m(index, scope, secret_fns, param_return_taint, method_secret_fns))
        }
        Expr::FieldAccess { base, .. } => {
            expr_secret_source_m(base, scope, secret_fns, param_return_taint, method_secret_fns)
        }
        Expr::Cast { expr, .. } => expr_secret_source_m(expr, scope, secret_fns, param_return_taint, method_secret_fns),
        // Faithful mirror of `expr_taint_source`'s `Tainted` arm: unwrap the marker to keep tracking
        // provenance (defensive — the parser does not currently emit `Expr::Tainted`, but the taint
        // dual has this arm, so the confidentiality side keeps it symmetric).
        Expr::Tainted { inner, .. } => {
            expr_secret_source_m(inner, scope, secret_fns, param_return_taint, method_secret_fns)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_secret_source_m(inner, scope, secret_fns, param_return_taint, method_secret_fns)
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                None // released — dual of the taint declassify clear
            } else {
                expr_secret_source_m(inner, scope, secret_fns, param_return_taint, method_secret_fns)
            }
        }
        // Composite / control-flow value propagation — the confidentiality dual of the same arms on
        // `expr_taint_source`: a secret stashed in a container, or a branch of a match/if, carries.
        Expr::ArrayLiteral { elements } => elements
            .iter()
            .find_map(|e| expr_secret_source_m(e, scope, secret_fns, param_return_taint, method_secret_fns)),
        Expr::StructLiteral { fields, .. } => fields
            .iter()
            .find_map(|(_, e)| expr_secret_source_m(e, scope, secret_fns, param_return_taint, method_secret_fns)),
        Expr::EnumConstruct { fields, .. } => fields
            .iter()
            .find_map(|e| expr_secret_source_m(e, scope, secret_fns, param_return_taint, method_secret_fns)),
        Expr::MapLiteral { entries, .. } => entries.iter().find_map(|(k, v)| {
            expr_secret_source_m(k, scope, secret_fns, param_return_taint, method_secret_fns)
                .or_else(|| expr_secret_source_m(v, scope, secret_fns, param_return_taint, method_secret_fns))
        }),
        // Control-flow value exprs — SCOPE-AWARE walk, the exact confidentiality dual of the same
        // arms on `expr_taint_source` (see the full note there): clone-per-arm/block, inner
        // bindings shadow, pattern vars inherit the whole scrutinee's secrecy, straight-line block
        // `Assign` applies the main analyzer's set/CLEAR discipline to the local clone.
        // Value-position block: confidentiality dual of the taint Block arm — thread through
        // `walk_block_secret` then read the tail's secret label in the post-statement scope, closing the
        // same value-position nested-control-flow fail-open on the confidentiality side.
        Expr::Block { stmts, tail } => {
            let mut local = scope.clone();
            walk_block_secret(stmts, &mut local, secret_fns, param_return_taint, method_secret_fns);
            tail.as_ref()
                .and_then(|t| expr_secret_source_m(t, &local, secret_fns, param_return_taint, method_secret_fns))
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let scrut =
                expr_secret_source_m(scrutinee, scope, secret_fns, param_return_taint, method_secret_fns).is_some();
            arms.iter().find_map(|arm| {
                let mut local = scope.clone();
                seed_secret_pattern(&mut local, &arm.pattern, scrut);
                expr_secret_source_m(&arm.body, &local, secret_fns, param_return_taint, method_secret_fns)
            })
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let scrut =
                expr_secret_source_m(scrutinee, scope, secret_fns, param_return_taint, method_secret_fns).is_some();
            let mut local = scope.clone();
            seed_secret_pattern(&mut local, pattern, scrut);
            expr_secret_source_m(then, &local, secret_fns, param_return_taint, method_secret_fns)
                .or_else(|| expr_secret_source_m(else_, scope, secret_fns, param_return_taint, method_secret_fns))
        }
        Expr::If { then, else_, .. } => {
            expr_secret_source_m(then, scope, secret_fns, param_return_taint, method_secret_fns)
                .or_else(|| expr_secret_source_m(else_, scope, secret_fns, param_return_taint, method_secret_fns))
        }
        Expr::Try(inner) => expr_secret_source_m(inner, scope, secret_fns, param_return_taint, method_secret_fns),
        // A method/closure application on a secret (`s.clone()`) may carry the secret — conservatively
        // surface the first secret sub-expression (twin of the `expr_taint_source` CallExpr arm).
        Expr::CallExpr { callee, args } => {
            // #67: an impl method whose RETURN carries an internally-minted secret makes
            // `v.key()` a secret source (the getter/accessor exfil). Bare-name keyed, so
            // fires before the receiver/arg recursion; inert when method_secret_fns is empty.
            if let Expr::FieldAccess { field, .. } = callee.as_ref() {
                if method_secret_fns.contains(field) {
                    return Some(format!("return value of method `{field}`"));
                }
            }
            expr_secret_source_m(callee, scope, secret_fns, param_return_taint, method_secret_fns).or_else(|| {
                args.iter()
                    .find_map(|a| expr_secret_source_m(a, scope, secret_fns, param_return_taint, method_secret_fns))
            })
        }
        _ => None,
    }
}

// ── Interprocedural return-taint summary (`ctx.tainting_fns`) ────────────────────────────────────
// A monotone fixpoint pre-pass (see `compute_tainting_fns`, run in `typecheck` before per-function
// analysis) marks each function whose RETURN value carries INTERNAL taint — a `taint_source()` /
// `tainted<T>` local returned directly, or a return of another already-marked function. Consumed by
// `expr_taint_source`'s `Call` arm so `sink(get_secret())` is flagged even with no tainted argument.
// It is deliberately whole-value + reassignment-insensitive + declassify-aware, exactly matching the
// intra-procedural analysis, and MONOTONE (only grows) so it needs no control-flow-merge join.

/// Whether an expression is a `return X` (Anubis models `return` as a call to the pseudo-function
/// named `"return"`, never a real function).
fn is_return_call(e: &Expr) -> bool {
    matches!(e, Expr::Call { callee, .. } if callee == "return")
}

/// Seed one `let` binding's taint into `scope`, mirroring the real let-seeding (annotation OR a
/// tainted, non-declassified initializer). Params are never seeded here — a returned parameter is
/// arg-flow, handled at each call site; this isolates taint a function produces INTERNALLY.
fn seed_one_let(
    name: &str,
    ty: Option<&str>,
    init: &Expr,
    scope: &mut BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_tainting_fns: &BTreeSet<String>,
) {
    let explicit = is_tainted_type(ty);
    let init_taint = expr_taint_source_m(init, scope, tainting_fns, param_return_taint, method_tainting_fns);
    let declassified = declassify_source(init, scope, tainting_fns, param_return_taint).is_some();
    let tainted = explicit || (init_taint.is_some() && !declassified);
    let taint_source = if explicit {
        Some(name.to_string())
    } else if tainted {
        init_taint
    } else {
        None
    };
    scope.insert(
        name.to_string(),
        ScopeBinding {
            info: BindingInfo {
                name: name.to_string(),
                ty: ty.map(str::to_string),
                mode: String::new(),
                tainted,
                taint_source,
                declassified,
                span: None,
            },
            closure_arity: None,
            // This scope feeds the INTEGRITY return summary only; secrecy has its own parallel
            // seeder (`seed_one_let_secret`, feeding `compute_secret_fns`), so it is not seeded here.
            secret: false,
        },
    );
}

/// Seed every name a pattern binds into `scope` with the given TAINT label — the whole-scrutinee /
/// whole-initializer label (conservative whole-value granularity, matching the `Index`/
/// `FieldAccess` arms: destructuring a tainted aggregate taints every bound part). Inserting
/// OVERWRITES an outer same-named binding — the pattern var SHADOWS it, which is the accept-bias
/// half of the scope-aware walk. Sets `tainted` and `taint_source` TOGETHER: the `Var` arm gates on
/// BOTH fields (`.filter(tainted)`), so a label written without the flag would be silently dropped
/// (a one-sided false negative the design review caught before it shipped).
fn seed_taint_pattern(
    scope: &mut BTreeMap<String, ScopeBinding>,
    pattern: &Pattern,
    label: &Option<String>,
) {
    for n in pattern.bound_names() {
        scope.insert(
            n.clone(),
            ScopeBinding {
                info: BindingInfo {
                    name: n,
                    ty: None,
                    mode: String::new(),
                    tainted: label.is_some(),
                    taint_source: label.clone(),
                    declassified: false,
                    span: None,
                },
                closure_arity: None,
                secret: false,
            },
        );
    }
}

/// Seed ONE `let` binding carrying BOTH labels (integrity + confidentiality) into an effect-analysis
/// scope. The single-label seeders (`seed_one_let` / `seed_one_let_secret`) each INSERT a whole
/// ScopeBinding and zero the other label, so calling them in sequence clobbers the first — but the
/// effect pass reads BOTH `info.tainted` (its sink check) and `.secret` (its egress check) off the
/// same binding. This mirrors the authoritative combined seed inline in `analyze_stmts` (the `let`
/// arm) minus the fn-symbol/taint-label/solver bookkeeping a value-block-local `let` never needs.
/// A well-formed `declassify` initializer clears (via `declassify_source`), exactly as at a real let.
#[allow(clippy::too_many_arguments)]
fn seed_effect_let(
    name: &str,
    ty: Option<&str>,
    init: &Expr,
    scope: &mut BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    secret_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_tainting_fns: &BTreeSet<String>,
    method_secret_fns: &BTreeSet<String>,
) {
    // #67: route to the method-aware `_m` variants so a value-block-local `let x = v.key()` inside an
    // effect-analyzed block is labelled from the method return (its sole caller is the enforcing
    // walk_block_effects, so method awareness is always wanted here).
    let init_taint = expr_taint_source_m(init, scope, tainting_fns, param_return_taint, method_tainting_fns);
    let init_secret =
        expr_secret_source_m(init, scope, secret_fns, param_return_taint, method_secret_fns).is_some();
    let declass = declassify_source(init, scope, tainting_fns, param_return_taint);
    let explicit = is_tainted_type(ty);
    // A `secret<T>` annotation on a value-position block-local `let` auto-labels it secret, mirroring
    // the statement-level let arm (keeps the two seeders byte-consistent for the qualifier).
    let explicit_secret = is_secret_type(ty);
    let tainted = explicit || (init_taint.is_some() && declass.is_none());
    let taint_source = if explicit {
        Some(name.to_string())
    } else if tainted {
        init_taint
    } else {
        None
    };
    scope.insert(
        name.to_string(),
        ScopeBinding {
            info: BindingInfo {
                name: name.to_string(),
                ty: ty.map(str::to_string),
                mode: String::new(),
                tainted,
                taint_source,
                declassified: declass.is_some(),
                span: None,
            },
            closure_arity: None,
            secret: init_secret || explicit_secret,
        },
    );
}

/// Seed every name a pattern binds with BOTH the scrutinee's integrity AND confidentiality labels
/// (whole-value granularity). The effect dual of `seed_taint_pattern`/`seed_secret_pattern`, written
/// as one insert so a destructured pattern var carries both labels the effect pass reads. Sets
/// `info.tainted` TOGETHER with `taint_source` (the taint `Var` arm gates on both).
fn seed_effect_pattern(
    scope: &mut BTreeMap<String, ScopeBinding>,
    pattern: &Pattern,
    taint: &Option<String>,
    secret: bool,
) {
    for n in pattern.bound_names() {
        scope.insert(
            n.clone(),
            ScopeBinding {
                info: BindingInfo {
                    name: n,
                    ty: None,
                    mode: String::new(),
                    tainted: taint.is_some(),
                    taint_source: taint.clone(),
                    declassified: false,
                    span: None,
                },
                closure_arity: None,
                secret,
            },
        );
    }
}

/// Whether any value a function body can RETURN carries internal taint, given the summary so far —
/// respecting LEXICAL BLOCK SCOPE. A `let` inside an `if`/loop body shadows an outer same-named
/// binding only within that block: the scope is snapshot/restored around every block, so a
/// `return x` AFTER the block sees the outer binding (an adversary found the flat version wrongly
/// marked `fn f(c){ let x=5; if c { let x=taint(); } return x; }`, which provably returns the clean
/// outer 5). `tail` marks whether this statement sequence is in the function's tail position, so a
/// bare trailing expression counts as an implicit return only when it truly is one (never a
/// mid-function side-effecting statement, nor a loop body's last statement). Declassify-before-return
/// reads clean automatically via `expr_taint_source`'s `Declassify` arm. Monotone in `tainting_fns`.
fn body_returns_taint(
    stmts: &[Stmt],
    scope: &mut BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_tainting_fns: &BTreeSet<String>,
    tail: bool,
) -> bool {
    let n = stmts.len();
    for (i, stmt) in stmts.iter().enumerate() {
        let stmt_is_tail = tail && i + 1 == n;
        match stmt {
            Stmt::Let { name, ty, init, .. } => {
                // A `return` can hide inside the initializer (a `match`/`if` arm).
                let mut rets = Vec::new();
                expr_returns(init, &mut rets);
                if rets.iter().any(|e| {
                    expr_taint_source_m(e, scope, tainting_fns, param_return_taint, method_tainting_fns).is_some()
                }) {
                    return true;
                }
                // #70: thread the method-return taint set (non-empty for the METHOD summary) so a
                // `let t = self.tag(); return t` chain is method-aware; the FREE-fn summary passes empty.
                seed_one_let(
                    name,
                    ty.as_deref(),
                    init,
                    scope,
                    tainting_fns,
                    param_return_taint,
                    method_tainting_fns,
                );
            }
            Stmt::If { then, else_, .. } => {
                // Branches inherit tail position; block-scoped `let`s must not leak past the `if`.
                let saved = scope.clone();
                if body_returns_taint(then, scope, tainting_fns, param_return_taint, method_tainting_fns, stmt_is_tail) {
                    return true;
                }
                *scope = saved.clone();
                if let Some(else_body) = else_ {
                    if body_returns_taint(
                        else_body,
                        scope,
                        tainting_fns,
                        param_return_taint,
                        method_tainting_fns,
                        stmt_is_tail,
                    ) {
                        return true;
                    }
                }
                *scope = saved;
            }
            Stmt::While { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                // A loop/research body is never the function's implicit return value (tail = false);
                // only an explicit `return` inside it counts. Its `let`s are block-scoped.
                let saved = scope.clone();
                if body_returns_taint(body, scope, tainting_fns, param_return_taint, method_tainting_fns, false) {
                    return true;
                }
                *scope = saved;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let saved = scope.clone();
                    if body_returns_taint(b, scope, tainting_fns, param_return_taint, method_tainting_fns, false) {
                        return true;
                    }
                    *scope = saved;
                }
            }
            _ => {
                // Explicit `return X` in this (non-block) statement — statement position or hidden in
                // an expression (match/if arm) — checked against the CURRENT lexical scope.
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                if rets.iter().any(|e| {
                    expr_taint_source_m(e, scope, tainting_fns, param_return_taint, method_tainting_fns).is_some()
                }) {
                    return true;
                }
                // Implicit tail return: a trailing expression in tail position. A control-flow
                // tail (`if`/`match`/`if let`/block) is tracked too — `expr_taint_source` walks
                // them scope-aware — so the old tail-`if`/`match` boundary is retired.
                if stmt_is_tail {
                    if let Stmt::ExprStmt(e) = stmt {
                        if !is_return_call(e)
                            && expr_taint_source_m(e, scope, tainting_fns, param_return_taint, method_tainting_fns)
                                .is_some()
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Seed a return-summary scope with a function's `tainted<T>` / `secret<T>` PARAMS. A qualifier-declared
/// param is UNCONDITIONALLY tainted/secret (declared, not arg-derived), so a function that RETURNS such a
/// param — or a value derived from it — is taint-/secret-returning regardless of the call argument. A
/// PLAIN param is left clean here: its label is arg-flow, resolved at each call site via
/// `param_return_taint`. This closes the interprocedural propagation gap the former empty-scope summary
/// left open (`fn get(x: secret<u64>){ return x; }` was not secret-returning, so `send(get(5))` slipped
/// through). Seeds BOTH labels so the one helper serves both the taint and secret summaries.
fn seed_qualifier_params(params: &[(String, String)], scope: &mut BTreeMap<String, ScopeBinding>) {
    for (name, ty) in params {
        let tainted = is_tainted_type(Some(ty));
        let secret = is_secret_type(Some(ty));
        if tainted || secret {
            scope.insert(
                name.clone(),
                ScopeBinding {
                    info: BindingInfo {
                        name: name.clone(),
                        ty: Some(ty.clone()),
                        mode: String::new(),
                        tainted,
                        taint_source: tainted.then(|| name.clone()),
                        declassified: false,
                        span: None,
                    },
                    closure_arity: None,
                    secret,
                },
            );
        }
    }
}

/// Collect `(name, typed-params, body)` for every free function (recursing into modules), so a
/// return-summary can honor a `secret<T>`/`tainted<T>` param qualifier via [`seed_qualifier_params`].
#[allow(clippy::type_complexity)]
fn collect_fn_typed_bodies<'a>(
    items: &'a [Item],
    out: &mut Vec<(String, &'a [(String, String)], &'a [Stmt])>,
) {
    for item in items {
        match item {
            Item::Fn {
                name, params, body, ..
            } => out.push((name.clone(), params.as_slice(), body.as_slice())),
            Item::Module { items, .. } => collect_fn_typed_bodies(items, out),
            _ => {}
        }
    }
}

/// Like `collect_fn_typed_bodies` but for IMPL METHODS (each `impl … { fn m(self, …) {…} }` method is an
/// `Item::Fn` inside `Item::Impl`). Returns TYPED params (unlike `collect_impl_method_params_bodies`,
/// which drops types) so `seed_qualifier_params` can honor a `secret<T>`/`tainted<T>` method param. Keyed
/// by bare method name (`self` at index 0), so same-named methods across impls share a key and the
/// return-summary UNIONS across them — fail-closed, matching the method_param_* keying.
#[allow(clippy::type_complexity)]
fn collect_impl_method_typed_bodies<'a>(
    items: &'a [Item],
    out: &mut Vec<(String, &'a [(String, String)], &'a [Stmt])>,
) {
    for item in items {
        match item {
            Item::Impl { methods, .. } => {
                for m in methods {
                    if let Item::Fn {
                        name, params, body, ..
                    } = m
                    {
                        out.push((name.clone(), params.as_slice(), body.as_slice()));
                    }
                }
            }
            Item::Module { items, .. } => collect_impl_method_typed_bodies(items, out),
            _ => {}
        }
    }
}

fn fn_returns_taint(
    params: &[(String, String)],
    body: &[Stmt],
    tainting_fns: &BTreeSet<String>,
    method_tainting_fns: &BTreeSet<String>,
) -> bool {
    let mut scope: BTreeMap<String, ScopeBinding> = BTreeMap::new();
    seed_qualifier_params(params, &mut scope);
    let empty: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    body_returns_taint(body, &mut scope, tainting_fns, &empty, method_tainting_fns, true)
}


/// Populate `ctx.tainting_fns` by a monotone fixpoint: repeatedly mark any not-yet-marked function
/// whose return carries taint under the current summary, until no function is added. Converges in at
/// most one round per function because the set only grows. Run once, before per-function analysis, so
/// every `Call` the analysis later sees consults a complete summary.
#[allow(clippy::type_complexity)]
fn compute_tainting_fns(items: &[Item], ctx: &mut SemanticContext) {
    let mut fns: Vec<(String, &[(String, String)], &[Stmt])> = Vec::new();
    collect_fn_typed_bodies(items, &mut fns);
    loop {
        let mut newly: Vec<String> = Vec::new();
        for (name, params, body) in &fns {
            if !ctx.tainting_fns.contains(name)
                && fn_returns_taint(params, body, &ctx.tainting_fns, &BTreeSet::new())
            {
                newly.push(name.clone());
            }
        }
        if newly.is_empty() {
            break;
        }
        ctx.tainting_fns.extend(newly);
    }
}

// ── Interprocedural return-SECRET summary (`ctx.secret_fns`) ──────────────────────────────────────
// The exact confidentiality dual of the return-taint summary above: a monotone fixpoint marking each
// function whose RETURN value carries a `secret_source(..)` secret — minted directly, through a
// `let`-chain, or returned from another already-marked function. Consumed by `expr_secret_source`'s
// `Call` arm (so `send(get_key())` fires) AND by the trifecta's leg-1 (a helper that
// returns a secret IS private-data access). Whole-value + reassignment-insensitive + declassify-aware,
// exactly mirroring the taint side, and MONOTONE (only grows) so it needs no control-flow-merge join.

/// Seed one `let` binding's SECRECY into `scope` for the return-secret walk (the dual of
/// `seed_one_let`). Secrecy originates from the initializer's value (`secret_source(..)`, a
/// secret-returning callee, or a derivation of one) OR from a `secret<T>` annotation on the let —
/// mirroring `seed_one_let`'s `is_tainted_type` term, so a locally-minted annotated secret that is
/// returned makes the function secret-returning (`compute_secret_fns`). A returned PLAIN param is
/// arg-flow, handled at each call site via `param_return_taint` (isolating the secrecy a function
/// produces INTERNALLY); a returned `secret<T>` param is now seeded unconditionally secret by
/// [`seed_qualifier_params`] in the return summary (that gap is closed).
fn seed_one_let_secret(
    name: &str,
    ty: Option<&str>,
    init: &Expr,
    scope: &mut BTreeMap<String, ScopeBinding>,
    secret_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_secret_fns: &BTreeSet<String>,
) {
    let secret =
        is_secret_type(ty) || expr_secret_source_m(init, scope, secret_fns, param_return_taint, method_secret_fns).is_some();
    scope.insert(
        name.to_string(),
        ScopeBinding {
            info: BindingInfo {
                name: name.to_string(),
                ty: ty.map(str::to_string),
                mode: String::new(),
                tainted: false,
                taint_source: None,
                declassified: false,
                span: None,
            },
            closure_arity: None,
            secret,
        },
    );
}

/// Seed every name a pattern binds into `scope` with the given SECRECY flag — the confidentiality
/// dual of `seed_taint_pattern` (single-field: the secret walker's `Var` arm gates only on
/// `.secret`). Inserting overwrites an outer same-named binding (shadow).
fn seed_secret_pattern(
    scope: &mut BTreeMap<String, ScopeBinding>,
    pattern: &Pattern,
    secret: bool,
) {
    for n in pattern.bound_names() {
        scope.insert(
            n.clone(),
            ScopeBinding {
                info: BindingInfo {
                    name: n,
                    ty: None,
                    mode: String::new(),
                    tainted: false,
                    taint_source: None,
                    declassified: false,
                    span: None,
                },
                closure_arity: None,
                secret,
            },
        );
    }
}

/// Whether any value a function body can RETURN carries a secret — the dual of `body_returns_taint`,
/// same lexical-block-scope discipline (snapshot/restore around every block, so a block-local `let`
/// shadow does not leak past its block and a `return` after the block sees the outer binding).
fn body_returns_secret(
    stmts: &[Stmt],
    scope: &mut BTreeMap<String, ScopeBinding>,
    secret_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    method_secret_fns: &BTreeSet<String>,
    tail: bool,
) -> bool {
    let n = stmts.len();
    for (i, stmt) in stmts.iter().enumerate() {
        let stmt_is_tail = tail && i + 1 == n;
        match stmt {
            Stmt::Let { name, ty, init, .. } => {
                let mut rets = Vec::new();
                expr_returns(init, &mut rets);
                if rets
                    .iter()
                    .any(|e| expr_secret_source_m(e, scope, secret_fns, param_return_taint, method_secret_fns).is_some())
                {
                    return true;
                }
                // #70: thread the (possibly non-empty for the METHOD summary) method-return set so a
                // `let k = self.key(); return k` chain is method-aware; the FREE-fn summary passes empty.
                seed_one_let_secret(
                    name,
                    ty.as_deref(),
                    init,
                    scope,
                    secret_fns,
                    param_return_taint,
                    method_secret_fns,
                );
            }
            Stmt::If { then, else_, .. } => {
                let saved = scope.clone();
                if body_returns_secret(then, scope, secret_fns, param_return_taint, method_secret_fns, stmt_is_tail) {
                    return true;
                }
                *scope = saved.clone();
                if let Some(else_body) = else_ {
                    if body_returns_secret(
                        else_body,
                        scope,
                        secret_fns,
                        param_return_taint,
                        method_secret_fns,
                        stmt_is_tail,
                    ) {
                        return true;
                    }
                }
                *scope = saved;
            }
            Stmt::While { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                let saved = scope.clone();
                if body_returns_secret(body, scope, secret_fns, param_return_taint, method_secret_fns, false) {
                    return true;
                }
                *scope = saved;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let saved = scope.clone();
                    if body_returns_secret(b, scope, secret_fns, param_return_taint, method_secret_fns, false) {
                        return true;
                    }
                    *scope = saved;
                }
            }
            _ => {
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                if rets
                    .iter()
                    .any(|e| expr_secret_source_m(e, scope, secret_fns, param_return_taint, method_secret_fns).is_some())
                {
                    return true;
                }
                if stmt_is_tail {
                    if let Stmt::ExprStmt(e) = stmt {
                        if !is_return_call(e)
                            && expr_secret_source_m(e, scope, secret_fns, param_return_taint, method_secret_fns).is_some()
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Whether a function's return value carries a secret (tail position) — the dual of `fn_returns_taint`.
/// The scope is seeded with the function's `secret<T>` PARAMS ([`seed_qualifier_params`]) but NOT with
/// plain params; a returned PLAIN param is arg-flow, resolved at each call site via `param_return_taint`
/// (hence the empty `param_return_taint` here), whereas a returned `secret<T>` param is unconditionally
/// secret-returning by declaration.
fn fn_returns_secret(
    params: &[(String, String)],
    body: &[Stmt],
    secret_fns: &BTreeSet<String>,
    method_secret_fns: &BTreeSet<String>,
) -> bool {
    let mut scope: BTreeMap<String, ScopeBinding> = BTreeMap::new();
    seed_qualifier_params(params, &mut scope);
    let empty: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    body_returns_secret(body, &mut scope, secret_fns, &empty, method_secret_fns, true)
}

/// Populate `ctx.secret_fns` by a monotone fixpoint — the exact dual of `compute_tainting_fns`.
#[allow(clippy::type_complexity)]
fn compute_secret_fns(items: &[Item], ctx: &mut SemanticContext) {
    let mut fns: Vec<(String, &[(String, String)], &[Stmt])> = Vec::new();
    collect_fn_typed_bodies(items, &mut fns);
    loop {
        let mut newly: Vec<String> = Vec::new();
        for (name, params, body) in &fns {
            if !ctx.secret_fns.contains(name)
                && fn_returns_secret(params, body, &ctx.secret_fns, &BTreeSet::new())
            {
                newly.push(name.clone());
            }
        }
        if newly.is_empty() {
            break;
        }
        ctx.secret_fns.extend(newly);
    }
}

/// Populate `ctx.method_secret_fns` — the impl-method twin of `compute_secret_fns`. A method is
/// return-secret iff `fn_returns_secret` holds over its body (self@0), reusing the SAME body walker,
/// which now consults BOTH the FROZEN free-fn set (`ctx.secret_fns`, at fixpoint — this runs after
/// `compute_secret_fns`) AND the GROWING method set (snapshotted each round). This is a COMBINED
/// fixpoint (#70): `fn alias(self){ return self.key() }` where `key` mints a secret is caught in round 2
/// (round 1 marks `key`; round 2, with `key` now in the snapshot, the body walk's `CallExpr` arm sees
/// `self.key()` as secret-returning → `alias` marked). Monotone (add-only over a finite name lattice) →
/// terminates. UNION across impls is automatic (same bare-name key).
#[allow(clippy::type_complexity)]
fn compute_method_secret_fns(items: &[Item], ctx: &mut SemanticContext) {
    let mut methods: Vec<(String, &[(String, String)], &[Stmt])> = Vec::new();
    collect_impl_method_typed_bodies(items, &mut methods);
    loop {
        let mut newly: Vec<String> = Vec::new();
        // Snapshot the growing method set so a method returning another method's secret chains.
        let known_method = ctx.method_secret_fns.clone();
        for (name, params, body) in &methods {
            if !ctx.method_secret_fns.contains(name)
                && fn_returns_secret(params, body, &ctx.secret_fns, &known_method)
            {
                newly.push(name.clone());
            }
        }
        if newly.is_empty() {
            break;
        }
        ctx.method_secret_fns.extend(newly);
    }
}

/// Populate `ctx.method_tainting_fns` — the integrity dual of `compute_method_secret_fns` (#70 combined
/// fixpoint): consults the frozen `ctx.tainting_fns` AND the growing method set (snapshotted per round),
/// so `fn relay(self){ return self.tag() }` where `tag` returns `input()` is chained. Runs after
/// `compute_tainting_fns`.
#[allow(clippy::type_complexity)]
fn compute_method_tainting_fns(items: &[Item], ctx: &mut SemanticContext) {
    let mut methods: Vec<(String, &[(String, String)], &[Stmt])> = Vec::new();
    collect_impl_method_typed_bodies(items, &mut methods);
    loop {
        let mut newly: Vec<String> = Vec::new();
        let known_method = ctx.method_tainting_fns.clone();
        for (name, params, body) in &methods {
            if !ctx.method_tainting_fns.contains(name)
                && fn_returns_taint(params, body, &ctx.tainting_fns, &known_method)
            {
                newly.push(name.clone());
            }
        }
        if newly.is_empty() {
            break;
        }
        ctx.method_tainting_fns.extend(newly);
    }
}

// ── Interprocedural param→sink summary (`ctx.param_sinks`) ───────────────────────────────────────
// Monotone fixpoint: for each function, which formal parameters flow to a sink without declassify
// (a builtin `is_sink`, or a call argument position another function's summary marks as sinking).
// Call sites then reject `log(tainted)` when `fn log(x){sink(x);}` — `ANUBIS_INTERPROC_SINK`.

/// Collect `(name, param_names, body)` for free functions (modules mangled the same way as calls).
pub(crate) fn collect_fn_params_bodies<'a>(
    items: &'a [Item],
    out: &mut Vec<(String, Vec<String>, &'a [Stmt])>,
) {
    for item in items {
        match item {
            Item::Fn {
                name, params, body, ..
            } => {
                out.push((
                    name.clone(),
                    params.iter().map(|(n, _)| n.clone()).collect(),
                    body.as_slice(),
                ));
            }
            Item::Module { items, .. } => collect_fn_params_bodies(items, out),
            _ => {}
        }
    }
}

/// Like `collect_fn_params_bodies` but for IMPL METHODS: each `impl … { fn m(self, …) {…} }` method is
/// an `Item::Fn` inside `Item::Impl`. Collects `(bare method name, param names INCLUDING self, body)`,
/// keyed by bare name (so same-named methods across impls share a key and their summaries UNION —
/// fail-closed). `self` is kept at parameter index 0, matching the runtime dispatch (run.rs builds
/// `call_args = [receiver, …]`), so a summary index p≥1 corresponds to call arg p-1.
fn collect_impl_method_params_bodies<'a>(
    items: &'a [Item],
    out: &mut Vec<(String, Vec<String>, &'a [Stmt])>,
) {
    for item in items {
        match item {
            Item::Impl { methods, .. } => {
                for m in methods {
                    if let Item::Fn {
                        name, params, body, ..
                    } = m
                    {
                        out.push((
                            name.clone(),
                            params.iter().map(|(n, _)| n.clone()).collect(),
                            body.as_slice(),
                        ));
                    }
                }
            }
            Item::Module { items, .. } => collect_impl_method_params_bodies(items, out),
            _ => {}
        }
    }
}

/// Parameter indices that flow through `expr` under the current param-flow scope. Declassify clears;
/// calls pass through argument taint (and, for known sink-params of the callee, only those positions
/// matter at the *call site* — here we only need "which params are in this value").
fn expr_param_flow(expr: &Expr, flow: &BTreeMap<String, BTreeSet<usize>>) -> BTreeSet<usize> {
    match expr {
        Expr::Var(name) => flow.get(name).cloned().unwrap_or_default(),
        Expr::Binary { lhs, rhs, .. } => {
            let mut s = expr_param_flow(lhs, flow);
            s.extend(expr_param_flow(rhs, flow));
            s
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Tainted { inner: expr, .. } => {
            expr_param_flow(expr, flow)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => expr_param_flow(inner, flow),
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                BTreeSet::new() // cleared
            } else {
                expr_param_flow(inner, flow)
            }
        }
        Expr::Call { args, .. } => {
            // Union of arg flows (conservative over-approx of the call's value). Sink detection
            // for user callees is handled separately at call sites via `known_param_sinks`.
            args.iter().fold(BTreeSet::new(), |mut acc, a| {
                acc.extend(expr_param_flow(a, flow));
                acc
            })
        }
        Expr::Index { base, index } => {
            let mut s = expr_param_flow(base, flow);
            s.extend(expr_param_flow(index, flow));
            s
        }
        Expr::FieldAccess { base, .. } => expr_param_flow(base, flow),
        // Composite / control-flow param-flow (same arms as `expr_param_return_flow`): a param wrapped
        // in a container or selected by a branch still flows, so `fn log(x){ sink([x]); }` marks
        // param 0 as sinking and `log(tainted)` is caught at the call site.
        Expr::ArrayLiteral { elements } => elements.iter().fold(BTreeSet::new(), |mut acc, e| {
            acc.extend(expr_param_flow(e, flow));
            acc
        }),
        Expr::StructLiteral { fields, .. } => {
            fields.iter().fold(BTreeSet::new(), |mut acc, (_, e)| {
                acc.extend(expr_param_flow(e, flow));
                acc
            })
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().fold(BTreeSet::new(), |mut acc, e| {
            acc.extend(expr_param_flow(e, flow));
            acc
        }),
        Expr::MapLiteral { entries, .. } => entries.iter().fold(BTreeSet::new(), |mut acc, (k, v)| {
            acc.extend(expr_param_flow(k, flow));
            acc.extend(expr_param_flow(v, flow));
            acc
        }),
        // Control-flow value exprs — SCOPE-AWARE walk (see the full note on `expr_taint_source`).
        // Set-walker semantics: UNION over arms/branches; pattern vars inherit the scrutinee's
        // whole index set; a straight-line block `Assign` OVERWRITES the local clone's entry
        // (sound straight-line, precise: `{ x = 42; x }` carries no params).
        Expr::Block { stmts, tail } => {
            let mut local = flow.clone();
            for stmt in stmts {
                match stmt {
                    Stmt::Let { name, init, .. } => seed_param_flow_let(name, init, &mut local),
                    Stmt::LetPattern { pattern, init, .. } => {
                        let set = expr_param_flow(init, &local);
                        seed_flow_pattern(&mut local, pattern, &set);
                    }
                    Stmt::Assign {
                        target: Expr::Var(name),
                        value,
                    } => {
                        let set = expr_param_flow(value, &local);
                        local.insert(name.clone(), set);
                    }
                    _ => {}
                }
            }
            tail.as_ref()
                .map(|t| expr_param_flow(t, &local))
                .unwrap_or_default()
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let scrut = expr_param_flow(scrutinee, flow);
            arms.iter().fold(BTreeSet::new(), |mut acc, arm| {
                let mut local = flow.clone();
                seed_flow_pattern(&mut local, &arm.pattern, &scrut);
                acc.extend(expr_param_flow(&arm.body, &local));
                acc
            })
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let scrut = expr_param_flow(scrutinee, flow);
            let mut local = flow.clone();
            seed_flow_pattern(&mut local, pattern, &scrut);
            let mut s = expr_param_flow(then, &local);
            s.extend(expr_param_flow(else_, flow));
            s
        }
        Expr::If { then, else_, .. } => {
            let mut s = expr_param_flow(then, flow);
            s.extend(expr_param_flow(else_, flow));
            s
        }
        Expr::Try(inner) => expr_param_flow(inner, flow),
        // #68 value-carry through a method/closure application (`m.wrap(x)`, `f(a)(b)`): before this
        // arm the result carried NO params (fell to `_ => empty`), truncating provenance so
        // `sink(m.wrap(x))` / `let y = m.wrap(x); sink(y)` lost `x`. Conservative union of the callee
        // expr (incl. a method receiver) and every arg — the exact mirror of the bare `Expr::Call` arm
        // and `expr_param_return_flow`'s CallExpr arm. This over-approximates a method that DISCARDS an
        // arg (the receiver + discarded arg still count) — fail-closed (only enlarges the param→sink
        // summary, never a false interproc reject beyond the free-fn `Call` arm's identical behavior),
        // and corpus-inert (no committed sink/egress takes a method-call argument).
        Expr::CallExpr { callee, args } => {
            let mut s = expr_param_flow(callee, flow);
            for a in args {
                s.extend(expr_param_flow(a, flow));
            }
            s
        }
        _ => BTreeSet::new(),
    }
}

/// Seed a let into the param-flow map (union of init's params; declassify clears).
fn seed_param_flow_let(name: &str, init: &Expr, flow: &mut BTreeMap<String, BTreeSet<usize>>) {
    // Mirror declassify_source: a full declassify clears param provenance.
    let cleared = matches!(
        init,
        Expr::Declassify {
            policy: Some(_),
            reason: Some(_),
            ..
        }
    );
    if cleared {
        flow.insert(name.to_string(), BTreeSet::new());
    } else {
        flow.insert(name.to_string(), expr_param_flow(init, flow));
    }
}

/// Seed every name a pattern binds into a param-flow map with the given index set (whole-value:
/// destructuring a param-carrying scrutinee gives every bound part the scrutinee's provenance).
/// Inserting overwrites an outer same-named entry (shadow) — including a same-named formal
/// parameter, which is exactly the lexical-scope semantics.
fn seed_flow_pattern(
    flow: &mut BTreeMap<String, BTreeSet<usize>>,
    pattern: &Pattern,
    set: &BTreeSet<usize>,
) {
    for n in pattern.bound_names() {
        flow.insert(n, set.clone());
    }
}

/// Walk a body collecting parameter indices that reach a sink under the current
/// `known_param_sinks` summary. Scope-aware (snapshot/restore around blocks).
fn body_param_sinks(
    stmts: &[Stmt],
    flow: &mut BTreeMap<String, BTreeSet<usize>>,
    sink_pred: fn(&str) -> bool,
    known_param_sinks: &BTreeMap<String, BTreeSet<usize>>,
    known_method_param_sinks: &BTreeMap<String, BTreeSet<usize>>,
    found: &mut BTreeSet<usize>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, init, .. } => {
                // A sink can hide inside the initializer (e.g. `let _ = sink(x)`).
                collect_param_sinks_in_expr(init, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                seed_param_flow_let(name, init, flow);
            }
            Stmt::If {
                then, else_, cond, ..
            } => {
                collect_param_sinks_in_expr(cond, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                let saved = flow.clone();
                body_param_sinks(then, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                *flow = saved.clone();
                if let Some(else_body) = else_ {
                    body_param_sinks(else_body, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                }
                *flow = saved;
            }
            Stmt::While { body, cond, .. } => {
                collect_param_sinks_in_expr(cond, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                let saved = flow.clone();
                body_param_sinks(body, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                *flow = saved;
            }
            Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                let saved = flow.clone();
                body_param_sinks(body, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                *flow = saved;
            }
            Stmt::For {
                body, source, var, ..
            } => {
                let saved = flow.clone();
                // Loop var inherits collection/range param flow (conservative).
                let src_flow = match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        let mut s = expr_param_flow(start, flow);
                        s.extend(expr_param_flow(end, flow));
                        s
                    }
                    crate::frontend::ForSource::Collection { expr } => expr_param_flow(expr, flow),
                };
                flow.insert(var.clone(), src_flow);
                body_param_sinks(body, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                *flow = saved;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let saved = flow.clone();
                    body_param_sinks(b, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                    *flow = saved;
                }
            }
            Stmt::Assign { target, value } => {
                // Reassignment-insensitive for taint *clearing*, but we DO propagate param flow
                // into an existing name when the RHS carries params (monotone add). Clearing is
                // never performed — same discipline as the main taint analysis.
                collect_param_sinks_in_expr(value, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                if let Expr::Var(name) = target {
                    let rhs = expr_param_flow(value, flow);
                    flow.entry(name.clone()).or_default().extend(rhs);
                }
            }
            Stmt::ExprStmt(e) => {
                collect_param_sinks_in_expr(e, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            }
            _ => {
                // Best-effort: scan any nested expressions in other stmt forms for sink calls.
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                for r in rets {
                    collect_param_sinks_in_expr(&r, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
                }
            }
        }
    }
}

/// If `expr` is (or contains) a sink of params, record those indices. `sink_pred` selects WHICH
/// builtins count as a leaf sink — `is_sink` for the integrity param→sink summary, `is_egress_sink`
/// for the confidentiality param→egress summary; the value-flow walk is otherwise identical, so both
/// summaries share this one traversal. Also handles calls to user functions whose own summary
/// (`known_param_sinks`, i.e. the map being computed) marks specific argument positions.
fn collect_param_sinks_in_expr(
    expr: &Expr,
    flow: &BTreeMap<String, BTreeSet<usize>>,
    sink_pred: fn(&str) -> bool,
    known_param_sinks: &BTreeMap<String, BTreeSet<usize>>,
    known_method_param_sinks: &BTreeMap<String, BTreeSet<usize>>,
    found: &mut BTreeSet<usize>,
) {
    match expr {
        Expr::Call { callee, args } => {
            if sink_pred(callee) {
                for arg in args {
                    found.extend(expr_param_flow(arg, flow));
                }
            }
            if let Some(sink_params) = known_param_sinks.get(callee) {
                for &i in sink_params {
                    if let Some(arg) = args.get(i) {
                        found.extend(expr_param_flow(arg, flow));
                    }
                }
            }
            for arg in args {
                collect_param_sinks_in_expr(arg, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_param_sinks_in_expr(lhs, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            collect_param_sinks_in_expr(rhs, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Tainted { inner: expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::FieldAccess { base: expr, .. } => {
            collect_param_sinks_in_expr(expr, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
        }
        Expr::Declassify { inner, .. } => {
            collect_param_sinks_in_expr(inner, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
        }
        Expr::Index { base, index } => {
            collect_param_sinks_in_expr(base, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            collect_param_sinks_in_expr(index, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
        }
        // Control-flow value exprs — SCOPE-AWARE, the param->sink SUMMARY dual of the enforcing
        // descent in `analyze_expr_effect`: a param that reaches a sink buried in a match arm / if
        // branch / block is summarized, so `fn log(x){ match c { _ => sink(x) } }` marks param 0 and
        // `log(tainted)` is caught at the call site. Pattern vars inherit the scrutinee's param set.
        Expr::If {
            cond, then, else_, ..
        } => {
            collect_param_sinks_in_expr(cond, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            collect_param_sinks_in_expr(then, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            collect_param_sinks_in_expr(else_, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_param_sinks_in_expr(scrutinee, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            let scrut = expr_param_flow(scrutinee, flow);
            for arm in arms {
                let mut local = flow.clone();
                seed_flow_pattern(&mut local, &arm.pattern, &scrut);
                if let Some(guard) = &arm.guard {
                    collect_param_sinks_in_expr(guard, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
                }
                collect_param_sinks_in_expr(&arm.body, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
            }
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            collect_param_sinks_in_expr(scrutinee, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            let scrut = expr_param_flow(scrutinee, flow);
            let mut local = flow.clone();
            seed_flow_pattern(&mut local, pattern, &scrut);
            collect_param_sinks_in_expr(then, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
            collect_param_sinks_in_expr(else_, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
        }
        Expr::Block { stmts, tail } => {
            let mut local = flow.clone();
            for stmt in stmts {
                match stmt {
                    Stmt::Let { name, init, .. } => {
                        collect_param_sinks_in_expr(init, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
                        seed_param_flow_let(name, init, &mut local);
                    }
                    Stmt::LetPattern { pattern, init, .. } => {
                        collect_param_sinks_in_expr(init, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
                        let set = expr_param_flow(init, &local);
                        seed_flow_pattern(&mut local, pattern, &set);
                    }
                    Stmt::Assign {
                        target: Expr::Var(name),
                        value,
                    } => {
                        collect_param_sinks_in_expr(value, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
                        let set = expr_param_flow(value, &local);
                        local.insert(name.clone(), set);
                    }
                    Stmt::ExprStmt(e) => {
                        collect_param_sinks_in_expr(e, &local, sink_pred, known_param_sinks, known_method_param_sinks, found)
                    }
                    // Deep-nested control-flow STATEMENTS in a value block are covered for the
                    // ENFORCING pass by walk_block_effects; here (a monotone, fail-open SUMMARY) the
                    // common linear shapes suffice — a missed nested sink under-approximates the
                    // param_sinks set (never a false interproc reject), a named residual.
                    _ => {}
                }
            }
            if let Some(t) = tail {
                collect_param_sinks_in_expr(t, &local, sink_pred, known_param_sinks, known_method_param_sinks, found);
            }
        }
        // #68 free-fn→method / method→method sink/egress laundering. A method call `recv.name(a,b)`
        // parses as `CallExpr{ callee: FieldAccess{ base, field }, args }`; before this arm it fell to
        // `_ => {}` and dropped the flow entirely, so `fn f(m,x){ m.snd(x) }` never marked its param.
        // Consult the method summary (`known_method_param_sinks`, keyed by bare method name — the joint
        // fixpoint keeps it mutually transitive with the free-fn map) with the #64 SELF-OFFSET: summary
        // index 0 → receiver `base`, p≥1 → arg p-1. Then recurse callee + args so a bare sink buried in
        // a method-call argument/receiver is still found.
        Expr::CallExpr { callee, args } => {
            if let Expr::FieldAccess { base, field, .. } = callee.as_ref() {
                if let Some(sink_params) = known_method_param_sinks.get(field) {
                    for &i in sink_params {
                        let target: Option<&Expr> = if i == 0 {
                            Some(base.as_ref())
                        } else {
                            args.get(i - 1)
                        };
                        if let Some(t) = target {
                            found.extend(expr_param_flow(t, flow));
                        }
                    }
                }
            }
            collect_param_sinks_in_expr(callee, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            for arg in args {
                collect_param_sinks_in_expr(arg, flow, sink_pred, known_param_sinks, known_method_param_sinks, found);
            }
        }
        _ => {}
    }
}

/// Compute the interprocedural param→sink (or param→egress, per `sink_pred`) summaries for free
/// functions AND impl methods in ONE COMBINED add-only fixpoint (#68). A free fn can sink a param
/// THROUGH a method (`fn f(m,x){ m.snd(x) }`) and a method can sink THROUGH another method
/// (`fn ship(self,p){ self.deliver(p) }`), so the free-fn map and the method map are MUTUALLY RECURSIVE
/// (they compose: free→method→free→…) — no staged order with one frozen before the other can express
/// that cycle, which is why the former four staged passes (`compute_param_sinks`/`_egress` +
/// `compute_method_param_*`) structurally missed both #68 directions. Each iteration snapshots BOTH maps,
/// re-walks every free-fn and method body (`body_param_sinks` now consults both maps; its `CallExpr` arm
/// applies the #64 self-offset at method-call sites), and accumulates growth into both maps until neither
/// grows. Monotone (union-only) over the PRODUCT of two finite lattices (names × index-sets ⊆ {0..arity})
/// → strictly ascends a finite lattice → terminates at the least joint fixpoint. `free_map`/`method_map`
/// are distinct ctx fields so the two `&mut` borrows split at the call site; `is_sink` and `is_egress_sink`
/// are independent runs of this one driver. Run before per-function analysis so the interproc call-site
/// checks (both the free-fn `Expr::Call` arms and the impl-method `Expr::CallExpr` arms in
/// `analyze_expr_effect`) see complete summaries.
#[allow(clippy::type_complexity)]
fn compute_sink_summaries_joint(
    fns: &[(String, Vec<String>, &[Stmt])],
    methods: &[(String, Vec<String>, &[Stmt])],
    sink_pred: fn(&str) -> bool,
    free_map: &mut BTreeMap<String, BTreeSet<usize>>,
    method_map: &mut BTreeMap<String, BTreeSet<usize>>,
) {
    loop {
        let mut changed = false;
        // Snapshot BOTH maps so a mid-iteration read-after-write can't defeat the monotone growth.
        let known_free = free_map.clone();
        let known_method = method_map.clone();
        for (name, params, body) in fns {
            // Free fn: parameter i is at flow index i (no receiver).
            let mut flow: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            for (i, p) in params.iter().enumerate() {
                flow.insert(p.clone(), BTreeSet::from([i]));
            }
            let mut found = BTreeSet::new();
            body_param_sinks(body, &mut flow, sink_pred, &known_free, &known_method, &mut found);
            let entry = free_map.entry(name.clone()).or_default();
            for i in found {
                if entry.insert(i) {
                    changed = true;
                }
            }
        }
        for (name, params, body) in methods {
            // Impl method: `self` is params[0], so summary index p≥1 ↔ call arg p-1 (the self-offset the
            // CallExpr arms apply). Keyed by bare name → UNIONED across impls (fail-closed, the #64 posture).
            let mut flow: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            for (i, p) in params.iter().enumerate() {
                flow.insert(p.clone(), BTreeSet::from([i]));
            }
            let mut found = BTreeSet::new();
            body_param_sinks(body, &mut flow, sink_pred, &known_free, &known_method, &mut found);
            let entry = method_map.entry(name.clone()).or_default();
            for i in found {
                if entry.insert(i) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

// ── Interprocedural param→return summary (`ctx.param_return_taint`) ──────────────────────────────
// Monotone fixpoint: which formal parameters of each function can flow to its return value without
// declassify. Call sites combine this with argument taint so `fn wrap(x){return x;}` makes
// `wrap(tainted)` a taint source — and only those params (not every arg) taint the call, fixing the
// `fn ignore(x){return 5;} ignore(secret)` false positive of the historical any-arg rule.

/// Param-flow through an expression, consulting `known_param_return` so a call only carries params
/// that the callee summary says reach its return (when the callee is known).
fn expr_param_return_flow(
    expr: &Expr,
    flow: &BTreeMap<String, BTreeSet<usize>>,
    known_param_return: &BTreeMap<String, BTreeSet<usize>>,
) -> BTreeSet<usize> {
    match expr {
        Expr::Var(name) => flow.get(name).cloned().unwrap_or_default(),
        Expr::Binary { lhs, rhs, .. } => {
            let mut s = expr_param_return_flow(lhs, flow, known_param_return);
            s.extend(expr_param_return_flow(rhs, flow, known_param_return));
            s
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Tainted { inner: expr, .. } => {
            expr_param_return_flow(expr, flow, known_param_return)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_param_return_flow(inner, flow, known_param_return)
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                BTreeSet::new()
            } else {
                expr_param_return_flow(inner, flow, known_param_return)
            }
        }
        Expr::Call { callee, args } => {
            if known_param_return.contains_key(callee) {
                // Known user function (entry may be empty): only summarized return-params.
                let mut s = BTreeSet::new();
                if let Some(rets) = known_param_return.get(callee) {
                    for &i in rets {
                        if let Some(arg) = args.get(i) {
                            s.extend(expr_param_return_flow(arg, flow, known_param_return));
                        }
                    }
                }
                s
            } else {
                // Unknown/builtin during bootstrap: conservative union of args.
                args.iter().fold(BTreeSet::new(), |mut acc, a| {
                    acc.extend(expr_param_return_flow(a, flow, known_param_return));
                    acc
                })
            }
        }
        Expr::Index { base, index } => {
            let mut s = expr_param_return_flow(base, flow, known_param_return);
            s.extend(expr_param_return_flow(index, flow, known_param_return));
            s
        }
        Expr::FieldAccess { base, .. } => expr_param_return_flow(base, flow, known_param_return),
        // Composite / control-flow param-flow (mirrors the same arms on `expr_taint_source`): a param
        // wrapped in a container, or returned via a match/if branch, still flows to the return — so a
        // pass-through helper `fn wrap(x){ return [x]; }` correctly summarizes {0}. Union over every
        // sub-expression (a param reaching ANY element/branch reaches the aggregate value).
        Expr::ArrayLiteral { elements } => elements.iter().fold(BTreeSet::new(), |mut acc, e| {
            acc.extend(expr_param_return_flow(e, flow, known_param_return));
            acc
        }),
        Expr::StructLiteral { fields, .. } => {
            fields.iter().fold(BTreeSet::new(), |mut acc, (_, e)| {
                acc.extend(expr_param_return_flow(e, flow, known_param_return));
                acc
            })
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().fold(BTreeSet::new(), |mut acc, e| {
            acc.extend(expr_param_return_flow(e, flow, known_param_return));
            acc
        }),
        Expr::MapLiteral { entries, .. } => {
            entries.iter().fold(BTreeSet::new(), |mut acc, (k, v)| {
                acc.extend(expr_param_return_flow(k, flow, known_param_return));
                acc.extend(expr_param_return_flow(v, flow, known_param_return));
                acc
            })
        }
        // Value-position block: delegate the block's STATEMENT flow to `body_param_returns` — THE single
        // stmt walker — so the value-position walker and the function-body walker share one implementation
        // and cannot re-diverge. `discard` is a throwaway `found`: a value-position block does not
        // accumulate returns (a `return` inside it is collected by the enclosing body walker's own
        // `expr_returns`), and `tail = false` so no block statement is mis-read as an implicit return.
        // This replaces a former hand-rolled loop whose `_ => {}` arm DROPPED nested control-flow (a
        // value-position under-approximation — a missed leak: `send({ let r=0; if c { r=x; } r })` slipped
        // through); the block now inherits the driver's branch may-union, so a param assigned only inside a
        // nested branch reaches the tail. The tail is then evaluated in the block's post-statement `local`.
        Expr::Block { stmts, tail } => {
            let mut local = flow.clone();
            let mut discard = BTreeSet::new();
            body_param_returns(stmts, &mut local, known_param_return, &mut discard, false);
            tail.as_ref()
                .map(|t| expr_param_return_flow(t, &local, known_param_return))
                .unwrap_or_default()
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let scrut = expr_param_return_flow(scrutinee, flow, known_param_return);
            arms.iter().fold(BTreeSet::new(), |mut acc, arm| {
                let mut local = flow.clone();
                seed_flow_pattern(&mut local, &arm.pattern, &scrut);
                acc.extend(expr_param_return_flow(&arm.body, &local, known_param_return));
                acc
            })
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let scrut = expr_param_return_flow(scrutinee, flow, known_param_return);
            let mut local = flow.clone();
            seed_flow_pattern(&mut local, pattern, &scrut);
            let mut s = expr_param_return_flow(then, &local, known_param_return);
            s.extend(expr_param_return_flow(else_, flow, known_param_return));
            s
        }
        Expr::If { then, else_, .. } => {
            let mut s = expr_param_return_flow(then, flow, known_param_return);
            s.extend(expr_param_return_flow(else_, flow, known_param_return));
            s
        }
        Expr::Try(inner) => expr_param_return_flow(inner, flow, known_param_return),
        // Closure/method application (`x.clone()`, `f(a)(b)`): the returned value MAY derive from the
        // callee expression or any argument — conservatively union them (a bare-name `f(a)` is
        // `Expr::Call`, handled precisely above via the return summary; this arm is only the
        // higher-order `CallExpr`). Without it, `fn fwd(x){ return x.clone(); }` summarized `{}` and a
        // forwarded secret leaked. (Residual: a returned `Lambda` capturing a param — no downstream
        // consumer models closure application, so a Lambda arm would only add over-rejections.)
        Expr::CallExpr { callee, args } => {
            let mut s = expr_param_return_flow(callee, flow, known_param_return);
            for a in args {
                s.extend(expr_param_return_flow(a, flow, known_param_return));
            }
            s
        }
        _ => BTreeSet::new(),
    }
}

/// Collect parameter indices that a function body can RETURN under `known_param_return`.
/// Union `src`'s param-flow into `dst` (per-name index-set union) — the may-merge used when joining
/// a branch/loop body's flow back into the outer flow for the param→return summary.
fn union_flow_into(
    dst: &mut BTreeMap<String, BTreeSet<usize>>,
    src: &BTreeMap<String, BTreeSet<usize>>,
) {
    for (k, v) in src {
        dst.entry(k.clone()).or_default().extend(v.iter().copied());
    }
}

fn body_param_returns(
    stmts: &[Stmt],
    flow: &mut BTreeMap<String, BTreeSet<usize>>,
    known_param_return: &BTreeMap<String, BTreeSet<usize>>,
    found: &mut BTreeSet<usize>,
    tail: bool,
) {
    let n = stmts.len();
    for (i, stmt) in stmts.iter().enumerate() {
        let stmt_is_tail = tail && i + 1 == n;
        match stmt {
            Stmt::Let { name, init, .. } => {
                // Hidden returns inside the initializer.
                let mut rets = Vec::new();
                expr_returns(init, &mut rets);
                for r in rets {
                    found.extend(expr_param_return_flow(&r, flow, known_param_return));
                }
                let cleared = matches!(
                    init,
                    Expr::Declassify {
                        policy: Some(_),
                        reason: Some(_),
                        ..
                    }
                );
                if cleared {
                    flow.insert(name.clone(), BTreeSet::new());
                } else {
                    flow.insert(
                        name.clone(),
                        expr_param_return_flow(init, flow, known_param_return),
                    );
                }
            }
            // A destructuring `let [a,b] = [x,0]; return a` propagates the init's param-flow to each
            // bound name — without this arm a destructured param that is returned was silently dropped.
            Stmt::LetPattern { pattern, init, .. } => {
                let mut rets = Vec::new();
                expr_returns(init, &mut rets);
                for r in rets {
                    found.extend(expr_param_return_flow(&r, flow, known_param_return));
                }
                let set = expr_param_return_flow(init, flow, known_param_return);
                seed_flow_pattern(flow, pattern, &set);
            }
            // A param assigned to a local inside a branch MAY reach a later return, so MERGE (union)
            // each branch-end flow into the outer flow rather than RESTORING the pre-branch flow — the
            // restore dropped `let r=0; if c { r=x; } return r`. `found` accumulates unconditionally
            // across the branch recursions (only the flow map is merged here).
            Stmt::If { then, else_, .. } => {
                let saved = flow.clone();
                let mut t = saved.clone();
                body_param_returns(then, &mut t, known_param_return, found, stmt_is_tail);
                let mut e = saved.clone();
                if let Some(else_body) = else_ {
                    body_param_returns(else_body, &mut e, known_param_return, found, stmt_is_tail);
                }
                *flow = saved;
                union_flow_into(flow, &t);
                union_flow_into(flow, &e);
            }
            Stmt::While { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                // Single-pass may-union: a param assigned in the body MAY reach a post-loop return.
                // (Residual: a loop-CARRIED flow that only manifests across iterations needs a
                // loop-body fixpoint — named, rarer.)
                let mut b = flow.clone();
                body_param_returns(body, &mut b, known_param_return, found, false);
                union_flow_into(flow, &b);
            }
            Stmt::For {
                body, var, source, ..
            } => {
                let src_flow = match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        let mut s = expr_param_return_flow(start, flow, known_param_return);
                        s.extend(expr_param_return_flow(end, flow, known_param_return));
                        s
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_param_return_flow(expr, flow, known_param_return)
                    }
                };
                let mut b = flow.clone();
                b.insert(var.clone(), src_flow);
                body_param_returns(body, &mut b, known_param_return, found, false);
                union_flow_into(flow, &b);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let mut bf = flow.clone();
                    body_param_returns(b, &mut bf, known_param_return, found, false);
                    union_flow_into(flow, &bf);
                }
            }
            // A straight-line whole-`Var` reassignment STRONG-overwrites: `r = <value>` unconditionally
            // replaces the entire binding, so `r`'s prior param-flow is dead and dropping it is EXACT
            // (not an under-approximation) — the value the tail/return reads derives solely from the RHS.
            // Soundness is confined to straight-line position BY CONSTRUCTION: a reassignment inside a
            // branch/loop is recursed on a CLONE and then may-unioned back (the If/While/For/Loop/Hybrid
            // arms above union the branch-post flow with the pre-branch flow), which demotes a conditional
            // clear to a MAY — the pre-branch flow always survives. This is the precision the value-walker
            // Block arm already had (fe44f35); lifting it into the one canonical stmt walker keeps the two
            // walkers from re-diverging. Verified sound + monotone in the `compute_param_return_taint`
            // fixpoint (add-only accumulation never defeats it: a clearing RHS is intrinsically small from
            // iteration 1).
            Stmt::Assign {
                target: Expr::Var(name),
                value,
            } => {
                let set = expr_param_return_flow(value, flow, known_param_return);
                flow.insert(name.clone(), set);
            }
            // A non-`Var` target (`buf[0]=k`, `obj.f=k`) is a MAY-update of the root binding, so weak-union
            // into the root (overwriting would drop the param-flow the sibling elements already carry).
            Stmt::Assign { target, value } => {
                if let Some(root) = assign_target_root(target) {
                    let rhs = expr_param_return_flow(value, flow, known_param_return);
                    flow.entry(root.to_string()).or_default().extend(rhs);
                }
            }
            _ => {
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                for r in rets {
                    found.extend(expr_param_return_flow(&r, flow, known_param_return));
                }
                if stmt_is_tail {
                    if let Stmt::ExprStmt(e) = stmt {
                        if !is_return_call(e) {
                            found.extend(expr_param_return_flow(e, flow, known_param_return));
                        }
                    }
                }
            }
        }
    }
}

/// Populate `ctx.param_return_taint` by a monotone fixpoint. Every free function gets an entry
/// (possibly empty) so the Call arm can distinguish "known, returns no params" from "unknown".
fn compute_param_return_taint(items: &[Item], ctx: &mut SemanticContext) {
    let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
    collect_fn_params_bodies(items, &mut fns);
    // Ensure every function is present so analysis sees an entry (even empty).
    for (name, _, _) in &fns {
        ctx.param_return_taint.entry(name.clone()).or_default();
    }
    loop {
        let mut changed = false;
        let known = ctx.param_return_taint.clone();
        for (name, params, body) in &fns {
            let mut flow: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            for (i, p) in params.iter().enumerate() {
                flow.insert(p.clone(), BTreeSet::from([i]));
            }
            let mut found = BTreeSet::new();
            body_param_returns(body, &mut flow, &known, &mut found, true);
            let entry = ctx.param_return_taint.entry(name.clone()).or_default();
            for i in found {
                if entry.insert(i) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn expr_is_declassified(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> bool {
    match expr {
        Expr::Declassify { policy, reason, .. } => policy.is_some() && reason.is_some(),
        Expr::Var(name) => scope
            .get(name)
            .is_some_and(|binding| binding.info.declassified && !binding.info.tainted),
        _ => false,
    }
}

fn is_sink(callee: &str) -> bool {
    // Phase-3 C4: I/O write/send paths are sinks (so an undeclassified read→send is
    // `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` via the existing machinery).
    // Phase-5: `target_run` is a process-exec sink (payload is attacker-controlled input to a
    // local binary — tracked the same way as write/send for dual-use discipline).
    matches!(
        callee,
        "sink"
            | "send"
            | "network_send"
            | "write"
            | "write_file"
            | "append_file"
            | "memcpy"
            | "exec"
            | "sql"
            | "target_run"
    )
}

/// Phase-3 C4: I/O reads (and stdin) are taint sources — their return value is untrusted input.
/// Phase-5: `env`/`getenv` seed taint (environment is untrusted input).
fn is_io_taint_source(callee: &str) -> bool {
    matches!(
        callee,
        "read_file" | "open" | "input" | "read_line" | "recv" | "net_recv" | "env" | "getenv"
    )
}

/// Inherit a callee's declared capability into the caller's effect set and enforce Safe-mode gates.
/// Maps raw `uses` tags to the same inferred tags builtins emit (`file_write`, `shell`, …).
fn apply_inherited_capability(
    raw: String,
    mode: Mode,
    effects: &mut Vec<String>,
    ctx: &mut SemanticContext,
) {
    let canon = normalize_effect_name(&raw);
    match canon.as_str() {
        "fs.read" => {
            effects.push("file_read".into());
        }
        "fs.write" => {
            effects.push("file_write".into());
            if mode == Mode::Safe && !safe_cap_allowed(ctx, "fs.write") {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                    message: format!(
                        "safe mode file_write (via callee `uses({raw})`) forbidden without `uses(fs.write)`"
                    ),
                    span: None,
                });
            }
        }
        "net.send" => {
            effects.push("network".into());
            if mode == Mode::Safe && !safe_cap_allowed(ctx, "net.send") {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                    message: format!(
                        "safe mode network (via callee `uses({raw})`) forbidden without `uses(net.send)`"
                    ),
                    span: None,
                });
            }
        }
        "shell" => {
            effects.push("shell".into());
            if mode == Mode::Safe && !safe_cap_allowed(ctx, "shell") {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                    message: format!(
                        "safe mode shell/exec (via callee `uses({raw})`) forbidden without `uses(shell)`"
                    ),
                    span: None,
                });
            }
        }
        "time.now" => effects.push("time".into()),
        "rand.gen" => effects.push("rand".into()),
        other => {
            // Unknown/custom tags still surface as inferred so verified-lane uses checks can see them.
            effects.push(other.to_string());
        }
    }
}

/// Normalize a declared `uses(...)` effect name (or an inferred effect tag) to a canonical
/// capability id used for declared ⊆ inferred checking.
pub(crate) fn normalize_effect_name(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "fs.read" | "file_read" | "read_file" | "open" => "fs.read".into(),
        "fs.write" | "file_write" | "write_file" | "append_file" => "fs.write".into(),
        "net.send" | "net.connect" | "network" | "send" | "connect" | "network_send" => {
            "net.send".into()
        }
        "shell" | "exec" | "system" | "proc.exec" | "target_run" => "shell".into(),
        "time.now" | "time" => "time.now".into(),
        "rand.gen" | "rand" | "random" => "rand.gen".into(),
        other => other.to_string(),
    }
}

/// If `inferred` is a capability effect that must be declared in `uses(...)`, return its canonical
/// name; otherwise `None` (analysis-only tags like taint/assume/loop are not gated).
fn capability_effect(inferred: &str) -> Option<String> {
    let base = inferred.split(':').next().unwrap_or(inferred);
    match base {
        "file_read" | "file_write" | "network" | "shell" => Some(normalize_effect_name(base)),
        "time" | "rand" => Some(normalize_effect_name(base)),
        // Direct sink tags are taint machinery, not I/O capabilities.
        _ => None,
    }
}

/// Safe-mode capability gate: a restricted effect is allowed only when this function's `uses(...)`
/// declares the matching capability. Empty `authorized_caps` means no uses clause → deny restricted
/// effects (write/network/shell). Research/Exploit modes do not consult this (caller already checked mode).
fn safe_cap_allowed(ctx: &SemanticContext, cap: &str) -> bool {
    let canon = normalize_effect_name(cap);
    !ctx.authorized_caps.is_empty() && ctx.authorized_caps.contains(&canon)
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Safe => "safe",
        Mode::Research => "research",
        Mode::Exploit => "exploit",
    }
}

fn bitwidth_of(ty: &str) -> u32 {
    ty::bitwidth(ty)
}

fn smt_bv_type(width: u32) -> String {
    format!("(_ BitVec {})", width)
}

fn qualified_name(module: Option<&str>, name: &str) -> String {
    module
        .map(|module| format!("{}::{}", module, name))
        .unwrap_or_else(|| name.to_string())
}

fn empty_ir() -> TypedIR {
    TypedIR {
        mode: BuildMode::Safe,
        taint_labels: vec![],
        constraints: vec!["(assert true)".into()],
        has_research: false,
        body: vec![],
        hir: Hir::default(),
        mir: vec![],
        solver_obligations: vec![],
        symbols: vec![],
        taint_traces: vec![],
        diagnostics: vec![],
        symbolic_defs: vec![],
        symbolic_widths: BTreeMap::new(),
    }
}
