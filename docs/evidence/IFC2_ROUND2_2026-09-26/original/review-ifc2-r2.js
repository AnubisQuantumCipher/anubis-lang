export const meta = {
  name: 'review-ifc2-r2',
  description: 'Round-2 adversarial review of IFC v2 (pin anubis-ifc2-v8e, after the 107 round-1 findings were fixed): five lenses attack the new mechanisms and look for leaks, over-refusals and limits with runtime witnesses, then an independent verifier re-runs every finding',
  phases: [
    { title: 'Find', detail: 'five lenses probe `ifc2-report` on the v8e pin against runtime witnesses' },
    { title: 'Verify', detail: 'independent re-run of every reported finding' },
  ],
}

const BRIEF = `
You are an adversarial reviewer of a NEW information-flow analysis for the Anubis language: IFC v2, an abstract interpreter meant to mirror the runtime exactly. This is REVIEW ROUND 2. Round 1 found 107 problems; all were fixed (their programs are in ~/.cache/anubis-item21/ifc2/review1/src/, one .anb each, with the root causes in ~/.cache/anubis-item21/ifc2/review1/suite.json). Do NOT re-report those programs. Variants that still fail are welcome (a fix that covers the reported program but not its family is exactly what we want found). Your job: find where IFC v2 is still WRONG: programs it accepts that leak a secret at runtime (LEAKS), valid programs it refuses (OVER-REFUSALS), and crashes, limits or slowness (ROBUSTNESS). You are READ-ONLY with respect to source trees.

WHAT TO READ (the thing under review)
- ~/.cache/anubis-wt/ifc2/compiler/src/middle/ifc2/mod.rs, eval.rs, value.rs, builtins.rs (about 4,500 lines).
- The runtime it must mirror: ~/.cache/anubis-wt/ifc2/compiler/src/backends/run.rs. A factual map with file:line citations (written for round 1; the code decides where they differ): ~/.cache/anubis-item21/ifc2/SEMANTICS_MAP.md.

WHAT CHANGED SINCE ROUND 1 (attack these; each is new code)
- Loops: the header's decision lifts the next header (while, while-let; a break/continue inside a while-let scrutinee is honoured); a continue no longer lifts later iterations (only break and the header do); the loop cache is per call context; after 12 unsettled rounds a loop widens at depth 4.
- Values: V::select chooses between two alternatives raising only nodes whose shape differs (same struct type and fields, same enum variant, lists of one known length, maps of the same literal keys, the same function); presence tracking (MapV.maybe, V.maybe_fields: keys/fields that may be absent) decides whether a place write under a secret condition changes a container's shape; map literals fold a computed key into earlier known slots; map join/leq/merge handle keys known on one side; literal keys are the runtime display text (007 -> "7", 1.50 -> "1.5", true is position 1 for a struct index; a string index names a field); enum struct-variant payloads join by name when literal orders differ; leq is component-wise with top coverage; lists hash/compare by items only; widening first forgets list positions and merges map keys when a value exceeds 256 nodes.
- Control: truthiness (as_bool) decides if/while/&&/||/!: a scalar's content, a collection's emptiness, a struct/enum/closure is always true; method calls evaluate their arguments inside the chosen type arm (arity-truncated, padded with 0), the dispatch decision is secret only when the receiver may have more than one type name; match arms are decided progressively (an arm's pc is its own pattern plus earlier arms that are not disjoint from it, and their guards; enum sub-patterns read only that variant's payload); a destructuring let takes any enum's payload positionally (it does not test); exit(code) and panic(msg) end the program like a return (the continuation, in the function and its callers, is lifted by the pc they ran under); compose runs under the chosen function's label; nested compositions become a chain (any composition of their functions).
- Calls and summaries: after 12 precise contexts, or once a callee recurses (it is already on the solving stack), calls go to summary contexts keyed by (callee, pc, joined argument label); closure summaries past depth 3 are keyed by (lambda, reach label) where the reach label includes everything the closure's captures can reach; snapshots that join a summary are recorded through a pending list.
- Builtins: short-circuit HOFs (any all find position take_while drop_while) and sort_by run the callback under its own results; iteration counts use the length label (map klab included); apply spreads a list of unknown length to the callee's full arity; reduce puts the which-is-the-closure decision in the callbacks' pc and fails closed if it does not settle; char_at is index_get; repeat of a list returns a list; contains on a map tests keys; password_hash_phc_raw exists; files are a store (write_file/append_file/delete_file write it under the pc; read_file/open read it back); keychain_se_last_bind reads what cap_acquire_nonexportable wrote; type() reveals the kind; qualify is type-directed: Option<secret<T>>, Result<secret<T>, E>, list<secret<T>>, map<K, secret<V>> qualify only the payload/elements when the value has that shape.
- Runtime: fail-closed trap messages no longer print operand values (index, key, counts, paths, the unmatched match value). Type names of operands are still printed.
- Budgets: a step budget (1.5M statements) and a work budget (40M value operations) fail closed with ANUBIS_IFC2_LIMIT.

HARD RULES
- Never edit, build, or run cargo in ~/.cache/anubis-wt/* or ~/Projects/anubis-lang. Never kill processes. Do not use /tmp. Write only under your own probe directory.
- Binaries (pins, read-only): NEW=~/.cache/anubis-item21/pins/anubis-ifc2-v8e; IFC v2 alone is its subcommand \`ifc2-report\` (rc 1 and ANUBIS_* lines on a finding, rc 0 "no information flow found" otherwise). LANES=~/.cache/anubis-item21/pins/anubis-ord3x4l check (the current checker's lanes, no IFC v2). Runtime runners: NEW itself for programs its checker accepts; ~/.cache/anubis-item21/pins/anubis-whole10, ~/.cache/anubis-item21/pins/anubis-ordret2a and LANES for programs NEW's checker refuses (run refuses what its own checker refuses, so try them all). NOTE: the trap-message redaction is only in NEW's runtime; the older runners print operand values in trap messages.
- Always memory-capped, and read the exit status on the VERY NEXT statement:
  CAP=1500M timeout 60 ~/.cache/anubis-item21/capped.sh $NEW ifc2-report f.anb >out 2>&1 ; rc=$?
  CAP=1500M timeout 60 ~/.cache/anubis-item21/capped.sh $LANES check f.anb >out2 2>&1 ; rc2=$?
  CAP=2G timeout 90 ~/.cache/anubis-item21/capped.sh <runner> run --no-verify f.anb 2>&1 >runout ; rrc=$?
  NEVER run a pin outside capped.sh. At most 2 runs at once; other agents share the machine.
- 'ls' is aliased: use 'command ls'. At most 150 probes. Report FULL program text that runs as-is (no ellipses).

LANGUAGE NOTES (from the runtime): struct S { k: secret<i64>, pub_n: i64 } makes field k secret; let x: secret<i64> = 42; secret_source(v); declassify(v, "policy", "reason") releases (both strings non-empty). Taint sources: input(), read_line(), read_file(p), env(n), taint_source(x); integrity sinks include print/println/eprint/eprintln, send, shell, write_file, sql. Egress (confidentiality): print, println, eprint, eprintln, send, network_send, connect, http_get, http_post, shell, exec, system, target_run; IFC v2 also treats a secret panic message, a secret exit status and the proof journal as egress. Decision D1: an egress that runs under a condition a secret decides is refused (ANUBIS_IMPLICIT_FLOW); termination (divergence, a runtime trap) and timing channels are outside the claim, but exit and panic are explicit and are modelled. Runtime: pure value semantics (every read clones); closures snapshot their captures at creation; call-position name resolution is user function first, then a local closure, then builtins; a STATEMENT print(...) always prints; missing closure arguments are 0, extras ignored; a { in value position is a map literal (use if true { ... } else { ... } for a block); method calls dispatch on the receiver's runtime struct/enum type name and fall back to calling a closure stored in a field (or map key) of that name.

DEFINITIONS (verify each yourself before reporting)
- LEAK: NEW ifc2-report rc=0, and the program's runtime output (stdout and stderr) differs between two secret values (edit the secret literal, e.g. 42 to 123456, in a copy; run both on the same runner). Report whether LANES also accepts (lanes_rc).
- OVER-REFUSAL: NEW ifc2-report rc!=0 with an IFC finding, the program runs, and its output is IDENTICAL for two secret values (and it prints something). Report LANES rc: an over-refusal LANES accepts matters most (IFC v2 runs beside the lanes, so it would newly refuse it). D1 refusals of an egress that really runs under a secret condition are intended, not over-refusals.
- ROBUSTNESS: NEW crashes (signal, panic), exceeds 10 s, or reports ANUBIS_IFC2_LIMIT on a realistic program.
- Out of scope: contract/assertion obligations, capability/effect declarations (uses(...)), ANUBIS_ANALYSIS_LIMIT from the other lanes, termination/timing channels, a runtime trap's type name, programs that do not run (a build error is not a leak).
`

const FINDINGS = {
  type: 'object',
  properties: {
    findings: { type: 'array', items: { type: 'object', properties: {
      id: { type: 'string' }, kind: { type: 'string', enum: ['leak', 'overrefusal', 'robustness'] },
      program: { type: 'string' }, new_rc: { type: 'integer' }, lanes_rc: { type: 'integer' },
      new_code: { type: 'string' }, runner: { type: 'string' },
      output_k42: { type: 'string' }, output_other_k: { type: 'string' }, timing: { type: 'string' },
      root_cause: { type: 'string' }, explanation: { type: 'string' },
    }, required: ['id', 'kind', 'program', 'new_rc', 'lanes_rc', 'output_k42', 'output_other_k', 'root_cause', 'explanation'] } },
    probes_run: { type: 'integer' }, notes: { type: 'string' },
  },
  required: ['findings', 'probes_run', 'notes'],
}

const LENSES = [
  { key: 'control', prompt: 'LENS: implicit flows and control. Attack the loop changes (header lifting for while and while-let; a continue that does NOT lift the next iteration: find a program where it should; break/continue inside a while-let scrutinee; the per-context loop cache; coarse widening after 12 rounds), the progressive match decision and pattern disjointness (guards, or-patterns, nested sub-patterns, literals of different kinds, list patterns of other lengths, enum payload tests), truthiness (a struct/enum/closure is always true; a list/map/string by emptiness), method dispatch with per-arm argument evaluation (receivers of one type, of several types, enum receivers with several tags, the closure-field fallback, maps with a closure under the method name), exit/panic ending the program inside helpers, closures, callbacks of builtins, loops and match arms (and callers of those), compose under a secret choice, and returns/`?` in all of these.' },
  { key: 'values', prompt: 'LENS: values and places. Attack V::select (two alternatives that look the same shape but are not at runtime: struct literals of one type with different field orders or missing fields, lists whose length differs only at runtime, maps with the same literal keys but a computed key, closures of the same lambda), presence tracking (a key or field that is present on one path only, then written under a secret condition; remove then re-insert; maps from merge, map_values, joins in loops; struct fields added at runtime), computed map keys that collide with literal keys (numbers vs strings, 1 vs "1", 1.0 vs "1.0", true vs "true", keys built with str()), the literal display normalization, enum struct variants with different literal field orders, folded values (top) mixed with concrete kinds, the 256-node widening that forgets positions, and len/type/keys/has_key/for-in on all of these.' },
  { key: 'functions', prompt: 'LENS: functions, recursion and summaries. Attack the summary contexts (joined argument label: a public and a secret call of one helper past 12 contexts or in recursion; the on-stack recursion rule; mutual recursion), closure summaries keyed by reach label (closures nested past depth 3 in captures, pipelines that mix public and secret stages, closures stored in data), compose chains (compositions of compositions, compose in loops and reduce, a chain holding a closure that captures a secret or runs an egress), apply with lists of unknown length and with non-list arguments, reduce with a secret-chosen fold function or seed, the short-circuit HOFs under their own results, sort_by, min_by/max_by, and user higher-order functions passing callbacks through several levels.' },
  { key: 'parity', prompt: 'LENS: builtin and runtime parity, by code reading then witnesses. Compare builtins.rs entry by entry with run.rs (emit_builtin_call, the anubis_* helpers, specially lowered names): argument order, which argument is iterated or applied, what is returned (list vs string vs element vs scalar), which kinds panic, sources, sinks and STORES (any other runtime state a builtin writes and another reads back: files, environment, process-global statics, the keychain binding, randomness seeded by data). Check the type-directed qualify against declared types the runtime does not enforce (a value of another shape under Option<secret<T>>, Result<..>, list<..>, map<..>, nested generics, user struct types, tainted<..>). Check every runtime trap and panic message in run.rs and the included *.inc.rs runtimes for any that still prints operand data (values, keys, lengths, paths). For each divergence that could hide a flow or refuse a valid program, write a witness program and run it.' },
  { key: 'precision', prompt: 'LENS: precision and robustness on realistic programs. Take real programs (the corpus under ~/.cache/anubis-wt/ord3x/examples/ and ~/.cache/anubis-wt/ord3x/tests/fixtures/, and programs you write in the style of parsers, state machines, accumulators, config loaders, pipelines with named functions, recursive tree walks, vaults that compare a secret password and print a public result, record updates under secret conditions followed by printing public fields) and find valid programs IFC v2 refuses whose output does not depend on the secret, especially ones LANES accepts. Distinguish refusals decision D1 intends from genuine imprecision. Time NEW on large programs, deep recursion and nesting, long loops, wide maps and big records; report crashes, ANUBIS_IFC2_LIMIT, or more than 10 s.' },
]

phase('Find')
const results = await parallel(LENSES.map(l => () =>
  agent(`${BRIEF}\n\nYour probe directory: ~/.cache/anubis-review-esc/ifc2-r2/${l.key}/ (create it).\n\n${l.prompt}\n\nReturn every verified finding.`,
    { label: `find:${l.key}`, phase: 'Find', schema: FINDINGS })))
const all = []
results.forEach((r, i) => {
  if (!r) { log(`lens ${LENSES[i].key} returned nothing`); return }
  log(`lens ${LENSES[i].key}: ${r.findings.length} findings from ${r.probes_run} probes`)
  r.findings.forEach(f => all.push({ ...f, lens: LENSES[i].key }))
})
if (all.length === 0) return { findings: [], verdicts: [], notes: results.filter(Boolean).map(r => r.notes) }
phase('Verify')
const VERDICTS = { type: 'object', properties: { verdicts: { type: 'array', items: { type: 'object', properties: {
  id: { type: 'string' }, lens: { type: 'string' }, confirmed: { type: 'boolean' }, kind: { type: 'string' },
  new_rc: { type: 'integer' }, lanes_rc: { type: 'integer' }, output_k42: { type: 'string' }, output_other_k: { type: 'string' },
  duplicate_of_round1: { type: 'string' }, reason: { type: 'string' },
}, required: ['id', 'confirmed', 'new_rc', 'lanes_rc', 'reason'] } } }, required: ['verdicts'] }
const verified = await agent(`${BRIEF}\n\nYou are the INDEPENDENT VERIFIER. Directory: ~/.cache/anubis-review-esc/ifc2-r2/verify/ (create it). For EACH finding below: write the program exactly as given, run NEW ifc2-report and LANES check (rc on the next statement), and run it with the secret as given and with another secret value on a runner that accepts it. Confirm a LEAK only if NEW rc=0 and the outputs differ. Confirm an OVER-REFUSAL only if NEW rc!=0 with an IFC finding and the two outputs are identical and non-empty. Confirm ROBUSTNESS only if you reproduce the crash, limit or time. If the program is one of round 1's (~/.cache/anubis-item21/ifc2/review1/src/), say which in duplicate_of_round1. Default confirmed=false if anything does not reproduce exactly.\n\nFINDINGS:\n${JSON.stringify(all, null, 1)}`,
  { label: 'verify:all', phase: 'Verify', schema: VERDICTS })
return { findings: all, verdicts: verified ? verified.verdicts : null, notes: results.filter(Boolean).map(r => r.notes) }
