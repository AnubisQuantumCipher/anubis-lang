export const meta = {
  name: 'anubis-soundness-hunt',
  description: 'Exhaustive adversarial false-accept hunt across the whole Anubis checker/solver surface, real-binary adjudicated + discriminator-confirmed',
  phases: [
    { title: 'Hunt', detail: 'one grounded hunter per surface — write adversarial .anb, run check+run, apply the discriminator' },
    { title: 'Verify', detail: 'independent skeptic reproduces each candidate false-accept from scratch' },
    { title: 'Synthesize', detail: 'rank confirmed false-accepts, pick the highest-value fix' },
  ],
}

// The release binary to adjudicate probes. Override by passing the path as the Workflow `args` value;
// defaults to the repo-relative build output (agents' Bash cwd is the repo root). Build it first:
//   cargo build --release -p anubis
const BIN = (typeof args === 'string' && args.trim()) ? args.trim() : 'target/release/anubis'

// The soundness contract + the DISCRIMINATOR oracle, given verbatim to every hunter so findings are grounded.
const PROTOCOL = `
ANUBIS SOUNDNESS CONTRACT (the ONLY thing you are hunting to break):
  A green \`anubis check <f.anb>\` (exit 0) must NEVER certify a contract that the runtime \`anubis run <f.anb>\` violates.
  \`requires\`/\`ensures\` are COMPILE-TIME ONLY (no runtime check emitted). Only a body \`assert(...)\` traps at runtime.
  Runtime is a transpile-to-Rust using i64::wrapping_* arithmetic; u8/u16/u32 PARAMS are masked to [0,2^w) at entry;
  returns/locals/fields are NOT masked. Floats are f64/RNE. Strings/arrays have runtime semantics.

BINARY: ${BIN}
  Check:  ${BIN} check <f.anb>      (exit 0 = ACCEPT/no-disproof ; non-zero = REJECT)
  Run:    ${BIN} run <f.anb> --out /tmp/<uniq>/o    (exit 0 = ran ; ANUBIS_ASSERT_FAILED in output = a body assert trapped)
  Use a UNIQUE scratch dir per file to avoid collisions with sibling agents: mkdir -p /tmp/hunt_<surface>_<n>/ .

THE DISCRIMINATOR (mandatory — a raw "check accepts + run traps" is AMBIGUOUS):
  A body \`assert(P)\` ALWAYS runs at runtime regardless of compile-time proof, so "check accepts + run traps" can just be a
  normal deferred runtime guard catching a real violation — NOT a soundness bug. A genuine FALSE ACCEPT is: the checker
  PROVED a contract at compile time that the runtime then violates. To tell them apart, for a candidate property P:
    1. Program A asserts/ensures P. Does \`check A\` ACCEPT?  (if it REJECTS -> no false accept, discard.)
    2. Program B is A with the property NEGATED to !P (same position). Does \`check B\` ACCEPT?
       - If \`check B\` is REJECTED  -> the checker genuinely PROVED P (it disproved !P). If \`run\` then exhibits !P
         (a value/print or a runtime assert(!P) that passes), this is a CONFIRMED FALSE ACCEPT (false proof).
       - If \`check B\` is ACCEPTED  -> the checker proved NEITHER (both deferred to runtime) = FAIL-OPEN deferral = BENIGN,
         NOT a false accept. Classify as FAIL_OPEN.
  Only "the checker proved P (rejected !P) AND runtime exhibits !P" counts as FALSE_ACCEPT.

You must actually RUN the binary (Bash) for every probe and paste the real exit codes / output into your evidence.
Do NOT speculate — a finding with no real check+run+discriminator transcript is worthless and will be discarded.
`

const HUNT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['surface', 'probes_run', 'findings'],
  properties: {
    surface: { type: 'string' },
    probes_run: { type: 'integer', description: 'how many distinct .anb probes you actually executed' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['title', 'program', 'property', 'classification', 'transcript'],
        properties: {
          title: { type: 'string' },
          program: { type: 'string', description: 'the full .anb source of program A' },
          property: { type: 'string', description: 'the property P the checker proved that runtime violates' },
          negation_program: { type: 'string', description: 'program B asserting !P (the discriminator twin)' },
          classification: { type: 'string', enum: ['FALSE_ACCEPT', 'FAIL_OPEN', 'CORRECT_REJECT', 'CORRECT_ACCEPT', 'OVER_REJECT'] },
          transcript: { type: 'string', description: 'real check/run exit codes + output for A, B, and the runtime witness' },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['reproduced', 'confirmed_false_accept', 'reasoning'],
  properties: {
    reproduced: { type: 'boolean', description: 'did you independently re-run the binary and reproduce the transcript?' },
    confirmed_false_accept: { type: 'boolean', description: 'true ONLY if checker proved P (rejected !P) AND runtime exhibits !P' },
    reasoning: { type: 'string' },
    corrected_classification: { type: 'string', enum: ['FALSE_ACCEPT', 'FAIL_OPEN', 'CORRECT_REJECT', 'CORRECT_ACCEPT', 'OVER_REJECT'] },
  },
}

const SURFACES = [
  { key: 'int-overflow', focus: 'i64 wrapping vs solver bvadd/bvsub/bvmul near i64::MIN/MAX; requires/ensures/asserts over values close to the wrap boundary; signed div/rem MIN/-1; shift amounts mod 64.' },
  { key: 'u32-boundary-A1', focus: 'the RECENT A1 change: u8/u16/u32 PARAMS masked to [0,2^w) and range-injected, vs returns/locals/fields NOT masked. Hunt a value the CHECKER treats as range-injected uN but the RUNTIME leaves negative/out-of-range: reassigned params, arg from an unmasked return exceeding 2^32, mixed widths, method vs free-fn params, call-site coerce_uint_arg mismatches.' },
  { key: 'float-coercion', focus: 'int->f64 param/return coercion rounding (>2^53), the #40/#41 call-site+return coercion, a raw int arg into an f64 param proving the callee requires on the RAW int, f64-call-result at a caller binding.' },
  { key: 'float-arith', focus: 'QF_FP fp.div/fp.rem/signed-zero/NaN; the <= must be (or (fp.leq a b)(isNaN a)(isNaN b)) to match run.rs partial_cmp; an integer-arith subexpr flipped to float by +0.0/*1.0; is_float_modelable gates.' },
  { key: 'string-qfs', focus: 'QF_S faithfulness: str.len, ++, contains/starts_with/ends_with, indexof, substr — vs runtime; len(a)+len(b) overflow with a literal addend near i64::MAX; z3 \\u{} decoding of literals; builtin-name shadowing (user fn/local shadows len/contains).' },
  { key: 'array-abv', focus: 'QF_ABV select/store: array-literal cell facts, symbolic-index read under a loop guard, concrete-index write a[k]=v (in-place vs de-model), sequence length facts, out-of-bounds, aliasing (seq copy is Rc-COW at runtime).' },
  { key: 'struct-field', focus: 'field read/write (int/float/string), nested p.a.b, numeric-kind smuggle (a field classified by f64-parseability while int-kinded), param-field write surviving a shadowing let, prefix-conflict gate, struct is by-value (Vec not Rc).' },
  { key: 'loop-invariant', focus: 'while/for havoc soundness: a mutable range bound (0..e with e mutated in body), the auto i>=start invariant, invariant proved over body-mutated state, quantified array-fill, let-propagation into the loop body.' },
  { key: 'call-site-discharge', focus: 'callee requires/ensures composition at the call site: beta-substitution of params (substitute_vars must recurse every modelable form or a param name re-binds to caller scope), CLOSED constant-arg discharge at all depths, && decomposition/mixed-lane, match-arm body calls, nested/conditional call positions, ensures-composition launder after a mutation.' },
  { key: 'taint-laundering', focus: 'Safe-mode tainted-flow: containers (push/insert/index), while-let/if-let/match binders, method returns (getter exfil), interproc param->sink summaries, destructuring lets, aggregate arms, control-flow value blocks — find a tainted value reaching a sink that check ACCEPTS.' },
  { key: 'effect-capability', focus: 'undeclared fs.write/net/shell egress or a capability use that check ACCEPTS: buried-in-branch/aggregate/cast, method-arg laundering, closure via HOF/user-fn-param (#47/#48-A just landed — probe #48-B struct-field + nested-lambda-in-return + closure through TWO user fns), capability double-use.' },
  { key: 'cross-lane', focus: 'lane INTERACTIONS: int x float x string kind confusion, a value modeled in two lanes, a float fact poisoning an int obligation coverage gate, a string fact in a numeric obligation, sort-partition mistakes — the seams between lanes where a fact leaks into the wrong solver.' },
]

phase('Hunt')
log(`Hunting ${SURFACES.length} surfaces for false-accepts (real-binary adjudicated + discriminator).`)

const results = await pipeline(
  SURFACES,
  (s, _orig, i) =>
    agent(
      `You are an adversarial SOUNDNESS hunter for the Anubis compile-time contract checker. Your ONE job: find a program the
checker ACCEPTS (green \`check\`) whose contract the runtime VIOLATES — a FALSE ACCEPT / false proof.

${PROTOCOL}

YOUR SURFACE: "${s.key}"
FOCUS: ${s.focus}

Write 8-15 distinct adversarial .anb probes targeting this surface. For EACH, actually run \`${BIN} check\` and \`${BIN} run\`
(unique scratch dir), and for any that \`check\` ACCEPTS, run the DISCRIMINATOR (the negation twin) to classify. Prefer
contracts (requires/ensures) and body asserts that SHOULD be disprovable. Report every genuine FALSE_ACCEPT with a complete
real transcript (exit codes + output for A, B, and the runtime witness). If a surface is clean, report probes_run and an
empty findings list honestly — a clean result is a valid, valuable outcome. Do NOT invent findings.`,
      { label: `hunt:${s.key}`, phase: 'Hunt', schema: HUNT_SCHEMA }
    ),
  // Verify each FALSE_ACCEPT finding independently, as soon as this surface's hunt returns.
  (huntResult, s) => {
    if (!huntResult) return []
    const suspects = (huntResult.findings || []).filter((f) => f.classification === 'FALSE_ACCEPT')
    if (suspects.length === 0) return []
    return parallel(
      suspects.map((f) => () =>
        agent(
          `You are a skeptical independent VERIFIER. A hunter claims the following is a FALSE ACCEPT in the Anubis checker
(green \`check\` certifying a contract the runtime violates). Default to DISBELIEF — most claims are actually FAIL_OPEN
deferrals or the hunter mis-read a runtime assert. Reproduce it FROM SCRATCH with the real binary.

${PROTOCOL}

SURFACE: ${s.key}
CLAIMED FINDING: ${f.title}
PROPERTY the checker allegedly proved-but-runtime-violates: ${f.property}

PROGRAM A (asserts/ensures P):
\`\`\`
${f.program}
\`\`\`

DISCRIMINATOR TWIN B (asserts !P), as reported:
\`\`\`
${f.negation_program || '(hunter did not supply — construct it yourself)'}
\`\`\`

Re-run \`${BIN} check\` on A and B and \`${BIN} run\` on A yourself (unique scratch dir). Confirm confirmed_false_accept=true
ONLY if: \`check A\` ACCEPTS, \`check B\` is REJECTED (the checker genuinely proved P), AND runtime exhibits !P. If \`check B\`
also ACCEPTS, it is FAIL_OPEN (benign) — set confirmed_false_accept=false. Paste your real transcript in reasoning.`,
          { label: `verify:${s.key}:${f.title.slice(0, 24)}`, phase: 'Verify', schema: VERDICT_SCHEMA }
        ).then((v) => ({ surface: s.key, finding: f, verdict: v }))
      )
    )
  }
)

const flat = results.flat().filter(Boolean)
const confirmed = flat.filter((r) => r.verdict && r.verdict.confirmed_false_accept)

log(`Hunt complete. ${confirmed.length} independently-CONFIRMED false accepts across ${SURFACES.length} surfaces.`)

phase('Synthesize')
const summaryInput = confirmed.length
  ? confirmed
      .map(
        (r, i) =>
          `#${i + 1} [${r.surface}] ${r.finding.title}\nPROPERTY: ${r.finding.property}\nPROGRAM:\n${r.finding.program}\nVERIFIER: ${r.verdict.reasoning}`
      )
      .join('\n\n---\n\n')
  : '(no confirmed false accepts)'

const synthesis = await agent(
  `You are the lead soundness engineer for Anubis. Below are the INDEPENDENTLY-CONFIRMED false accepts from an exhaustive
multi-surface hunt (each: checker proved a property the runtime violates, discriminator-confirmed, real-binary reproduced).

${summaryInput}

Produce a crisp report: (1) how many genuine false accepts, (2) rank them by severity (a SOUNDNESS false-accept that
silently certifies a violated contract is CRITICAL; a security laundering accept is CRITICAL; a completeness/fail-open gap
is LOW), (3) for the single highest-value one, name the exact root cause and the minimal sound fix (which file/function,
which gate, and how to validate — verdict-diff + hunt). If ZERO confirmed, state that plainly: the surface is sound to the
depth probed, and name the residual/deferred classes worth a future deeper pass. Be honest — do not manufacture a finding.`,
  { label: 'synthesize', phase: 'Synthesize' }
)

return { confirmed_count: confirmed.length, confirmed, synthesis }
