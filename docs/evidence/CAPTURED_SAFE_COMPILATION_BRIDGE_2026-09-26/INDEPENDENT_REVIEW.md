# Independent final-diff review: captured Safe compilation bridge

Date: 2026-09-26. Verdict: **GO for a code-only commit** of
`compiler/src/resolve/captured.rs` at SHA-256
`2bc88a033a72f146f5495e32f965914f867a6b354a90497e0720662c2c97ab9c`.
This is a static source review, not approval for CLI, package, evidence, proof,
native-artifact, or release admission.

## Reviewed input and method

I read the applied file, its caller and source-graph validation paths, and the
retained focused-test/lint logs. I compared its bytes with the frozen v2
candidate and the preserved base using filesystem diffs, without Git. The
preserved base file has SHA-256
`bd505e0aa690d5d126577d400366ebd78473dc70dd124f2d0d08c375e5ccb59d`;
the frozen v2 candidate has SHA-256
`2a096587626c64828713426546ae1fe2d7c856aec93447ab23295b3dbf36e47d`.
The applied file differs from that candidate only by the non-executing
Research/Exploit refusal test at `compiler/src/resolve/captured.rs:725-742`.
I did not edit the shared source, run Git, build, execute tests, or execute
Research/Exploit code.

## Source assessment

- `PreparedCapturedProject` has private AST and source-graph fields
  (`compiler/src/resolve/captured.rs:247-254`). Its constructor loads one
  `CapturedTree`, combines that parsed graph, and derives the compilation
  identity from the same graph (`:294-310`). Callers may borrow the graph or
  entry bytes, but this API does not expose a way to replace the stored AST or
  graph independently (`:256-265`). The graph's nodes, import occurrences,
  and source bytes are taken from that parsed capture (`:312-365`). The
  source-graph constructor checks endpoints, namespace agreement, spans,
  collisions, topology, and compilation reachability
  (`compiler/src/package/source_graph.rs:257-345,413-481`). These checks do not
  prove that the parser or compiler is correct.
- The method rejects a parsed Research or Exploit program before typecheck or
  lowering (`compiler/src/resolve/captured.rs:267-290`). Its new test uses a
  Safe `main` followed by a Research-attributed function and a separate
  Exploit-body case; it calls capture, parse/combine, and classification only,
  and requires the specific Safe-only refusal (`:725-742`). No Research or
  Exploit execution is part of this test.
- For the default Safe comparison, the method runs `typecheck`, the taint pass,
  solver checks and `solver_stream_refusals`, then Rust-source lowering with
  research lowering disabled (`compiler/src/resolve/captured.rs:274-290`). This
  follows the core default-build sequence
  (`tools/anubis/src/main.rs:2625-2656`). `typecheck` delegates to
  `typecheck_ex(..., false)` (`compiler/src/middle/mod.rs:5266-5268`); its
  Safe request runs IFC v2 alongside the existing taint, secret, effect, and
  capability analysis (`compiler/src/middle/mod.rs:5424-5496`). I found no
  newly omitted default-Safe enforcing lane in this private bridge.
- The changed-body test keeps the import name constant, mutates the file after
  capture, checks the retained graph bytes, and checks the old string value in
  emitted Rust (`compiler/src/resolve/captured.rs:681-699`). The imported
  trait control reaches the same check/lower method (`:701-723`). Trait-name
  collision and parsed-byte-limit refusals remain covered (`:744-772`). These
  are focused controls, not a proof of complete resolution or lowering.
- Only `captured.rs` is changed. `resolve/mod.rs:20-23` still keeps this a
  private staged module without a production CLI or proof caller. The graph
  uses `GraphCoverage::Compilation`, which identifies reachable parsed imports;
  it is not package-surface closure (`compiler/src/resolve/captured.rs:357-364`).

## Validation boundary and remaining work

The retained focused-test log reports `17 passed; 0 failed` for the captured
resolver tests, including the new mode refusal. Those tests were **lead-run**,
not reviewer-run; the log does not itself bind the execution to the source
SHA above. The retained Clippy and format logs are empty and have no adjacent
exit-code receipts. Their successful exits are **lead-reported**, not
independently established by this review. No full workspace or final-candidate
gate is claimed here.

This API represents the default Safe comparison only. It does not implement
the stricter `--verified` capability policy, dependency/package admission,
evidence verification, native compilation, runtime confinement, or proof of
source-to-executable correspondence. Those boundaries need explicit treatment
before any CLI consumer uses the prepared input as an admission decision.
