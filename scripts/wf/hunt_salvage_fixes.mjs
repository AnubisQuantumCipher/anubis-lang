export const meta = {
  name: 'hunt-salvage-fixes',
  description: 'Adversarial false-accept hunt on the 4 salvaged checker fixes (call-site block discharge, assert modeling, HO taint, list-type parse)',
  phases: [
    { title: 'Hunt', detail: 'adversarial agents construct check-accepts/run-traps programs per changed surface' },
    { title: 'Verify', detail: 'independent discriminator re-verification of each candidate false-accept' },
  ],
}

const BIN = './target/release/anubis'

// Each hunter targets a surface the salvaged fixes touched. The prime-directive bug is a FALSE ACCEPT:
// `anubis check f` prints verified/green BUT `anubis run f` traps a requires/ensures/assert (exit 101)
// or a secret/tainted value reaches a sink. The DISCRIMINATOR separates a genuine false accept from a
// benign fail-open: negate the asserted predicate (or flip the leak to clean); if the OPPOSITE program
// is ALSO accepted, the checker models nothing there (fail-open, safe) — only if the opposite is
// REJECTED is the original a genuine directional false accept.
const HUNTERS = [
  {
    key: 'callsite-block-discharge',
    prompt: `You are hunting FALSE ACCEPTS in the Anubis checker's NEW call-site \`requires\`-discharge inside BLOCK-bodied branches (\`compiler/src/middle/mod.rs\` \`discharge_calls_in_expr\` \`Expr::Block\`/\`Expr::IfLet\` arms). This code DESCENDS into a block that is the then/else of an \`if\` EXPRESSION (or a match-arm body), models the block's never-reassigned/never-shadowed \`let\` bindings as SCOPED solver facts (int/float/string), then discharges any contracted call under those facts + the inherited branch guard. THE RISK: a modeled block-let fact that is NOT valid on the path the runtime takes → the solver "proves" a callee \`requires\` that is actually false → green check, runtime traps.

Construct adversarial \`.anb\` programs (write to /tmp/hunt_cb/*.anb) that try to make \`${BIN} check\` ACCEPT while \`${BIN} run\` TRAPS an \`ANUBIS_ASSERT_FAILED\`/requires. Specifically probe:
 - a block-let whose value the discharge models, but the call uses a DIFFERENT value (reassignment the guard should have caught, shadowing an outer var of the same name, a let bound from a mutated/aliased source).
 - the shadow-guard: a block-let named the SAME as a param/outer modeled var — does discharge use the wrong facts?
 - a value-position \`if { let t=…; g(t) } else { let t=…; g(t) }\` where one branch's fact leaks to the other (scope restore bug).
 - the \`IfLet\` scrutinee/else discharge — a call in the scrutinee or else that should NOT discharge under enclosing facts.
 - branch-guard vacuity: \`if false { g(bad) }\` — the guard makes it dead, but does the discharge wrongly prove it via a vacuous guard?
 - nested blocks, let-in-let, a block-let initialized from a call.

A useful callee: \`fn g(x: u32) requires(x >= 10) { }\`. Then a block that binds \`t\` and calls \`g(t)\` where t<10 at runtime but the checker proves t>=10.

For EACH candidate: run \`${BIN} check f.anb\` (record accept/reject) and \`${BIN} run f.anb\` (record trap/clean). Then build the OPPOSITE (flip the constant so the requires is genuinely satisfiable-vs-not) and check it. Classify: false_accept (check accepts + run traps + opposite rejects), fail_open (opposite also accepts), or correct. Report ONLY genuine false_accepts plus your 2 most-suspicious fail-opens. Include the exact .anb source.`,
  },
  {
    key: 'noncontract-assert-modeling',
    prompt: `You are hunting FALSE ACCEPTS in the Anubis checker's NEW non-contract integer-param assert modeling (\`compiler/src/middle/mod.rs\` \`analyze_function\`: \`model_int_params = has_contract || body_asserts_over_int_params(body, params)\`, and \`body_asserts_over_int_params\`). A function WITHOUT a contract, whose body has \`assert(P)\` over its INTEGER params, now models those params as UNCONSTRAINED i64 (plus the uN runtime mask) and statically discharges the assert. THE INTENDED direction is fail-CLOSED (an assert not provable for all param values is REJECTED). THE RISK: modeling the param could instead let the solver PROVE an assert that the runtime actually TRAPS → false accept; or an interaction (the modeled param feeding another lane: array index, field, call-site) proves something false.

Construct adversarial \`.anb\` programs (write to /tmp/hunt_as/*.anb) making \`${BIN} check\` ACCEPT while \`${BIN} run\` TRAPS. Probe:
 - a non-contract fn \`fn g(x: u32) { assert(EXPR); }\` where EXPR is provable-looking but false for some masked u32 value the runtime hits (overflow: \`assert(x + 1 > x)\` — wraps at u32::MAX; the fn is param-opaque unbounded i64 with a [0,2^32) mask, so x=2^32-1 wraps).
 - the uN mask interaction: does \`assert(x < 4294967296)\` get proved (always true under the mask) while a wider op traps?
 - the modeled param used in a SECOND assert or a call-site discharge in the same body (fact leakage).
 - float/string param asserts (should stay FAIL-OPEN — verify they don't newly false-accept): \`fn g(x: f64) { assert(x > 0.0); }\` then \`run\` with the value that traps.
 - \`body_asserts_over_int_params\` scope: an assert in a match-arm body (NOT descended) vs a while/for body (descended) — an isomorphic-form gap.

Call these fns from \`main\` with an argument that makes the runtime assert trap. For each candidate: \`check\` + \`run\`, then the OPPOSITE (flip the predicate) discriminator. Report genuine false_accepts + top 2 fail-opens with exact source.`,
  },
  {
    key: 'ho-taint-underreport',
    prompt: `You are hunting FALSE ACCEPTS (compiling LEAKS) in the Anubis checker's NEW higher-order taint summaries (\`compiler/src/middle/mod.rs\`: \`collect_param_sinks_in_expr\` Block arm now delegates to \`body_param_sinks\`; new \`lambda_body_return_flow\`; \`body_param_sinks\` \`LetPattern\` arm). These are meant to be monotone add-only (over-approximate taint = fail-closed). THE RISK: a bug in the delegation/flattening DROPS a taint/secret edge, so a secret or tainted value reaches a sink (send/shell/write/print/network) with a GREEN check — a compiling leak.

A leak = \`${BIN} check f.anb\` ACCEPTS but the program actually flows a \`secret(...)\`/\`taint_source(...)\`/\`input()\` value to an egress/sink without \`declassify\`. Construct adversarial \`.anb\` programs (write to /tmp/hunt_ht/*.anb) that route a secret/tainted param through:
 - a nested-in-block local lambda applied in the block tail: \`fn leak(x){ let g = ||{ let h = ||{ send(x) }; h() }; g(); }\` and deeper.
 - a lambda-body \`return\` forwarder composition: \`fn fwd(x){ let g = ||{ return x; }; return g(); }\` then \`send(fwd(secret))\`; and the nested \`let h = || x; h()\` inner-return form.
 - a destructuring \`let [a,b] = [secretparam, 0]; send(a)\` (the new LetPattern sink arm).
 - method-forwarders, containers holding an arg-capturing closure, if/while/for value-blocks that hide the sink.
 - the TWIN of each: closure defined-but-not-applied, applied-in-different-scope, returned-then-sunk.

The DIRECT forms (\`let g=||{send(x)}; g()\`) are already rejected — you are looking for a COMPOSED/nested form that slips. For each: confirm \`check\` ACCEPTS and reason precisely why the value reaches the sink at runtime (the discriminator here: the same program with the secret replaced by a public literal must ACCEPT, and with a \`declassify\` must ACCEPT — so an accept of the SECRET-egress form is the leak). Report genuine leaks + top 2 suspicious with exact source.`,
  },
  {
    key: 'list-type-parse',
    prompt: `You are hunting FALSE ACCEPTS around the Anubis parser's NEW list-type handling (\`compiler/src/frontend/mod.rs\` \`collect_type_until\` now emits \`[T]\` instead of collapsing to bare \`T\`). Before the fix a \`[int]\` field/param/return was stored as \`"int"\`, so a list could be modeled as a scalar integer (a latent false-accept the fix closes). THE RISK to check: does the NEW \`[T]\` string cause any DOWNSTREAM lane to now PROVE something false — e.g. a \`[int]\` treated as numeric in a contract/assert, an array-read/write lane mis-modeling a bracketed type, or a struct-field contract over a list field?

Construct adversarial \`.anb\` programs (write to /tmp/hunt_lt/*.anb) with \`[int]\`/\`[u32]\`/\`[string]\` params, fields, returns, and let-bindings, used in \`requires\`/\`ensures\`/\`assert\`/array-index/field-access contexts, trying to make \`${BIN} check\` ACCEPT a contract/assert that \`${BIN} run\` TRAPS. Also verify the fix's INTENT holds (a list value can NOT pass into a scalar u32 slot as an accept). Probe struct fields \`struct S { xs: [int] }\` with \`s.xs\`-based contracts, and the Map-inside-list case \`[Map<int,string>]\`. For each candidate apply the flip-the-constant discriminator. Report genuine false_accepts + top 2 fail-opens with exact source. If the surface is inert (lists are opaque, no contract lane models them), say so explicitly.`,
  },
  {
    key: 'isomorphic-twins-general',
    prompt: `You are running a general adversarial false-accept sweep across the Anubis checker, seeded by this repo's recurring lesson: ISOMORPHIC-FORM gaps. A fix closes one form; its twin often stays open. The 4 changes just landed touch: call-site discharge in \`if\`-EXPRESSION block branches, non-contract int-param assert modeling, HO-taint value-blocks/lambda-returns, list-type parsing. Hunt the TWINS the fixes may NOT cover:
 - call-site discharge: block branch is now covered — but is the \`match\`-ARM body? the \`if let\` THEN branch? a \`while\`/\`for\` body inside a value block? a lambda body? Construct \`fn g(x:u32) requires(x>=10){}\` called from those positions with x<10, check-accepts + run-traps.
 - assert modeling: covered for while/for/loop/block — is a match-arm-body or if-let-arm assert a false accept now (modeled param + arm assert)?
 - taint: read vs write, push(xs, secret) vs xs[i]=secret, if-let vs while-let binder.
Construct programs (write /tmp/hunt_tw/*.anb), run \`${BIN} check\` + \`${BIN} run\`, apply the opposite-predicate discriminator. Report genuine false_accepts + top 3 fail-opens (twins worth noting as completeness residuals) with exact source.`,
  },
]

const CANDIDATE_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['surface', 'ran_to_completion', 'candidates'],
  properties: {
    surface: { type: 'string' },
    ran_to_completion: { type: 'boolean', description: 'true only if you actually built + ran check/run on every candidate' },
    candidates: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['classification', 'anb_source', 'check_verdict', 'run_verdict', 'opposite_verdict', 'note'],
        properties: {
          classification: { type: 'string', enum: ['false_accept', 'fail_open', 'correct'] },
          anb_source: { type: 'string', description: 'the exact .anb program text' },
          check_verdict: { type: 'string', enum: ['accept', 'reject'] },
          run_verdict: { type: 'string', enum: ['trap', 'leak', 'clean', 'na'] },
          opposite_verdict: { type: 'string', description: 'accept/reject of the flipped/negated program (the discriminator)' },
          note: { type: 'string' },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['is_genuine_false_accept', 'reproduced_check_accept', 'reproduced_run_violation', 'discriminator_opposite_rejected', 'reasoning'],
  properties: {
    is_genuine_false_accept: { type: 'boolean' },
    reproduced_check_accept: { type: 'boolean' },
    reproduced_run_violation: { type: 'boolean' },
    discriminator_opposite_rejected: { type: 'boolean' },
    reasoning: { type: 'string' },
  },
}

phase('Hunt')
const hunted = await pipeline(
  HUNTERS,
  h => agent(h.prompt, { label: `hunt:${h.key}`, phase: 'Hunt', schema: CANDIDATE_SCHEMA, agentType: 'general-purpose' }),
  (report, h) => {
    if (!report) return { surface: h.key, candidates: [], ran_to_completion: false }
    const fas = (report.candidates || []).filter(c => c.classification === 'false_accept')
    if (fas.length === 0) return { ...report, verifiedFindings: [] }
    // Verify each claimed false_accept with 3 independent discriminator votes.
    return parallel(
      fas.flatMap(c =>
        [0, 1, 2].map(v => () =>
          agent(
            `Independently VERIFY this claimed Anubis false accept on surface "${h.key}". Write the program to a temp .anb, run \`${BIN} check\` and \`${BIN} run\`, and apply the discriminator (flip the asserted constant / predicate and re-check). A GENUINE false accept requires: check ACCEPTS the original, run VIOLATES (traps a contract/assert OR flows a secret/tainted value to a sink), AND the flipped program is REJECTED (proving the checker models the lane directionally, so the accept is a real lie — not a fail-open where everything accepts). Program:\n\n${c.anb_source}\n\nClaimed: check=${c.check_verdict} run=${c.run_verdict} opposite=${c.opposite_verdict}. Note: ${c.note}`,
            { label: `verify:${h.key}:${v}`, phase: 'Verify', schema: VERDICT_SCHEMA, agentType: 'general-purpose' },
          ),
        ),
      ),
    ).then(votes => {
      const byCand = []
      let i = 0
      for (const c of fas) {
        const cv = votes.slice(i, i + 3).filter(Boolean)
        i += 3
        const yes = cv.filter(v => v.is_genuine_false_accept).length
        byCand.push({ candidate: c, votes_genuine: yes, votes_total: cv.length, confirmed: yes >= 2, vote_detail: cv })
      }
      return { ...report, verifiedFindings: byCand }
    })
  },
)

const confirmed = hunted.flatMap(r => (r?.verifiedFindings || []).filter(f => f.confirmed))
const failOpens = hunted.flatMap(r => (r?.candidates || []).filter(c => c.classification === 'fail_open'))
const incomplete = hunted.filter(r => !r?.ran_to_completion).map(r => r?.surface)

return {
  surfaces_hunted: HUNTERS.length,
  confirmed_false_accepts: confirmed.map(f => ({ surface: f.candidate.classification, note: f.candidate.note, votes: `${f.votes_genuine}/${f.votes_total}`, source: f.candidate.anb_source })),
  confirmed_count: confirmed.length,
  fail_open_residuals_count: failOpens.length,
  fail_open_residuals: failOpens.slice(0, 12).map(c => ({ note: c.note, source: c.anb_source.slice(0, 400) })),
  inconclusive_surfaces: incomplete,
}
