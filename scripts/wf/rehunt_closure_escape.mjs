export const meta = {
  name: 'rehunt-closure-escape',
  description: 'Focused re-hunt: confirm the 6 closure-escaping false accepts are closed + probe residual escaping-closure binding forms',
  phases: [
    { title: 'Rehunt', detail: 'probe residual escaping-closure forms for green-check leaks' },
    { title: 'Verify', detail: 'discriminator re-verification' },
  ],
}

const BIN = './target/release/anubis'

const HUNTERS = [
  {
    key: 'residual-binding-forms',
    prompt: `A soundness fix just landed in the Anubis interproc taint/secret summary (compiler/src/middle/mod.rs): a NEW helper \`collect_capturing_closures\` registers closures bound via destructuring (\`let [g]=[||..]\`), container element (\`let c=[||..]; c[0]()\`), and if/match-init (\`let g=if b {||..} else {..}\`), so their buried sink/egress/return is now summarized. It covers these value forms in the initializer: Lambda, If, Match, Block-tail, ArrayLiteral, Cast, Tainted, MapLiteral values, StructLiteral fields, EnumConstruct fields. Application is descended for \`Call{name}\` and \`CallExpr{callee: Var(name) | Index{base: Var(name)}}\`.

Your job: find a RESIDUAL escaping-closure form that STILL launders a secret/tainted param to a sink with a GREEN check — a form NOT covered above. Probe (write /tmp/reh_a/*.anb, run \`${BIN} check\` + \`${BIN} run\`):
 - closure stored in a STRUCT FIELD then applied: \`let s = S { f: ||{send(x)} }; s.f()\` (CallExpr callee = FieldAccess — NOT covered by the Var/Index application match).
 - closure in a MAP then applied by key: \`let m = {"k": ||{send(x)}}; m["k"]()\`.
 - closure REASSIGNED: \`let mut g = ||{0}; g = ||{send(x)}; g()\` (Stmt::Assign, not Let — does registration see it?).
 - closure returned from a LOCAL helper then applied: \`let g = mk(); g()\` where \`fn mk() { return ||{send(x)} }\` — interproc closure value.
 - double-nested container \`let c = [[||{send(x)}]]; c[0][0]()\`.
 - closure passed to a HOF (map/each/filter) that applies it: \`each([secretparam], ||{send(x)})\` styles.
 - the TWIN in the RETURN lane for each (\`return\`-forwarder through the same binding).

For each candidate: does \`${BIN} check\` ACCEPT while the value provably reaches the sink at runtime (a secret/tainted param flows to send/shell/println/write/network without declassify)? The discriminator: the SAME program with the secret replaced by a public literal (or wrapped in declassify) must ACCEPT — so an accept of the SECRET-egress form is the leak. Report genuine leaks (classification: false_accept) + the top 3 suspicious as fail_open, with exact .anb source and which binding form escaped.`,
  },
  {
    key: 'confirm-6-closed',
    prompt: `Confirm a soundness fix is COMPLETE. These 6 Anubis programs were confirmed false accepts (green \`check\`, runtime leak) and should now ALL be REJECTED by \`${BIN} check\`:
1. \`fn fwd(x) { let [g] = [ || { return x; } ]; return g(); }\nfn main() uses(net.send) { send("host", 80, fwd(secret_source("api_key"))); }\`
2. \`fn leak(x) uses(io.print) { let [g] = [ || { println(x) } ]; g(); }\nfn main() uses(io.print) { leak(secret_source("api_key")); }\`
3. \`fn leak(x) uses(io.print) { let c = [ || { println(x) } ]; c[0](); }\nfn main() uses(io.print) { leak(secret_source("api_key")); }\`
4. \`fn leak(x) uses(io.print) { let g = if true { || { println(x) } } else { || { println(0) } }; g(); }\nfn main() uses(io.print) { leak(secret_source("api_key")); }\`
5. \`fn run(c) uses(proc.exec) { let [g] = [ || { shell(c) } ]; g(); }\nfn main() uses(proc.exec, io.read) { run(input()); }\`
6. \`fn fwd(x) { let [g] = [ || { return x; } ]; return g(); }\nfn main() uses(proc.exec, io.read) { shell(fwd(input())); }\`

Write each to a .anb (/tmp/reh_b/*.anb) and confirm \`${BIN} check\` now REJECTS all 6. THEN construct 8-10 close VARIATIONS of the same forms (e.g. tuple destructure \`let (g,)=(||..,)\`, 2-element \`let [a,b]=[||{send(x)},||{0}]; a()\`, container with 3 elements applied at c[2](), a match-init instead of if-init, the return-forwarder with an extra hop) and confirm they ALSO reject (or find one that still ACCEPTS = an incomplete fix). Report any variation that still check-ACCEPTS while leaking as classification: false_accept, with exact source. If all reject, report a single classification: correct candidate summarizing "6 closed + N variations all reject".`,
  },
  {
    key: 'monotone-no-new-fa',
    prompt: `A monotone-add-only change to the Anubis interproc taint summary could in principle cause an OVER-rejection (a valid program now wrongly rejected) — the opposite of a false accept, but still a bug. Find a CLEAN program (no secret/tainted value ever reaches a sink) that \`${BIN} check\` now WRONGLY REJECTS because the new \`collect_capturing_closures\` over-charged a closure. Probe (write /tmp/reh_c/*.anb): a destructured/container/if-init closure that captures a param but is applied in a NON-leaking way, or whose captured value is a PUBLIC literal / declassified, or a closure bound but never actually applied to a sink. For each: does \`${BIN} check\` REJECT a program that \`${BIN} run\` completes cleanly (no ANUBIS_* violation, exit 0)? That is an over-rejection (classification: false_accept is the wrong label here — use classification: correct with note 'OVER-REJECTION' and the source, and set run_verdict: clean). Report the top over-rejections. If the checker correctly accepts all your clean programs, say so (the fix is precise).`,
  },
]

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['surface', 'ran_to_completion', 'candidates'],
  properties: {
    surface: { type: 'string' }, ran_to_completion: { type: 'boolean' },
    candidates: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['classification', 'anb_source', 'check_verdict', 'run_verdict', 'note'],
      properties: {
        classification: { type: 'string', enum: ['false_accept', 'fail_open', 'correct'] },
        anb_source: { type: 'string' }, check_verdict: { type: 'string', enum: ['accept', 'reject'] },
        run_verdict: { type: 'string', enum: ['trap', 'leak', 'clean', 'na'] }, note: { type: 'string' },
      } } },
  },
}
const VSCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['is_genuine_false_accept', 'is_over_rejection', 'reasoning'],
  properties: { is_genuine_false_accept: { type: 'boolean' }, is_over_rejection: { type: 'boolean' }, reasoning: { type: 'string' } },
}

phase('Rehunt')
const hunted = await pipeline(
  HUNTERS,
  h => agent(h.prompt, { label: `rehunt:${h.key}`, phase: 'Rehunt', schema: SCHEMA, agentType: 'general-purpose' }),
  (report, h) => {
    if (!report) return { surface: h.key, candidates: [], ran_to_completion: false }
    const susp = (report.candidates || []).filter(c => c.classification === 'false_accept' || (c.note || '').includes('OVER-REJECT'))
    if (!susp.length) return { ...report, verified: [] }
    return parallel(susp.flatMap(c => [0, 1].map(v => () =>
      agent(`Independently verify this Anubis finding. Write to a .anb, run \`${BIN} check\` and \`${BIN} run\`. Set is_genuine_false_accept=true iff check ACCEPTS and run LEAKS a secret/tainted value to a sink (with the public/declassified twin accepting). Set is_over_rejection=true iff check REJECTS but run completes CLEAN (exit 0, no ANUBIS_* violation). Program:\n\n${c.anb_source}\n\nClaimed: ${c.classification} check=${c.check_verdict} run=${c.run_verdict}. ${c.note}`,
        { label: `verify:${h.key}:${v}`, phase: 'Verify', schema: VSCHEMA, agentType: 'general-purpose' })
    ))).then(votes => {
      const out = []; let i = 0
      for (const c of susp) { const cv = votes.slice(i, i + 2).filter(Boolean); i += 2
        out.push({ c, fa: cv.filter(v => v.is_genuine_false_accept).length, or: cv.filter(v => v.is_over_rejection).length, n: cv.length }) }
      return { ...report, verified: out }
    })
  },
)
const newFA = hunted.flatMap(r => (r?.verified || []).filter(x => x.fa >= 1))
const overRej = hunted.flatMap(r => (r?.verified || []).filter(x => x.or >= 1))
return {
  new_false_accepts: newFA.map(x => ({ note: x.c.note, votes: `${x.fa}/${x.n}`, source: x.c.anb_source })),
  new_false_accept_count: newFA.length,
  over_rejections: overRej.map(x => ({ note: x.c.note, votes: `${x.or}/${x.n}`, source: x.c.anb_source })),
  over_rejection_count: overRej.length,
  inconclusive: hunted.filter(r => !r?.ran_to_completion).map(r => r?.surface),
}
