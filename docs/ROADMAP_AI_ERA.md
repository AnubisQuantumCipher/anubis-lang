# Anubis: the road to 1.0 and the class of its own

Status: proposal. Nothing here is a claim about the current tree.
Date: 2026-09-21.

This document answers one question: **what does done look like?** Not "better
than SPARK", which is not a criterion, but sixteen things a stranger can run.

It comes out of a twelve-dimension research pass covering the current tree, the
Ada/SPARK, Rust and Zig bar, the verification-language landscape, proof
certificate technology, runtime confinement, AI-era language requirements,
offensive-security positioning and language longevity — each dimension
adversarially fact-checked. The fact-checks killed several flattering claims and
those corrections are kept below, because a roadmap that starts from an
overstatement arrives somewhere false.

---

## 1. The thesis

Every widely-used systems language reports a verdict. Anubis can report **what
its verdict is worth.**

> Anubis is the language whose ordinary build output is an *audited* verdict.
> Every discharged obligation carries either a machine-checkable witness a
> stranger can re-check with no Anubis binary in the loop, or a named and
> counted admission that it has none. In the same compiler pass, the proved
> effect row becomes the kernel policy the emitted binary installs on itself
> before its first instruction, re-derivable from source and sealed in the
> signed evidence document. And the whole apparatus is aimed at deliberately
> adversarial code.

Three honest qualifications, from the fact-checkers:

- **Not first to export a certificate.** GNATprove emits `why3session.xml` for
  `--replay`; cvc5 emits Alethe, LFSC and Lean 4 proofs; certifying Boogie emits
  Isabelle certificates for Dafny, VCC and Viper. The defensible claim is about
  *defaults and grading*, not priority.
- **Not first to ship a verified checker.** `cake_lpr` is verified to x64
  machine code; GRAT is verified in Isabelle; Lean's `bv_decide` runs a verified
  LRAT checker by kernel reflection. Anubis's checker is unverified Rust and its
  own module says so. On that single axis Anubis is currently *behind*.
- **Not first to enforce declared authority.** WASI, `pledge`/`unveil` and
  seccomp profile generators all do a version of it. None *proves* the
  declaration complete.

What survives is the **conjunction**, and one clause nobody has attempted: no
verification language has ever proved anything about an artifact designed to
attack, because their cultures forbid writing one. That is the unoccupied
ground, and it is also the hardest verification benchmark anyone has proposed —
adversarial, crash-seeking, network-egressing code is strictly harder to bound
than a well-behaved control loop.

**The offensive surface stays.** It is not a liability to be apologised for; it
is the proof of power and the hardest test case. `docs/language/SEMVER_1_0_POLICY.md`
already has the right structure: a frozen Safe core and a declared, unfrozen
Research annex. ACATS grades whether an implementation processes constructs as
prescribed; DO-330 classifies a tool by the damage it can do. Neither restricts
what a language may *express*.

---

## 2. Definition of done

Sixteen criteria. Each is a thing you can run.

| # | Criterion | Measured by | Who sets this bar today |
|---|---|---|---|
| 1 | A forged certificate fails the bundle | Replace a `.drat` with `1 2 0\n0\n`, recompute `MANIFEST.sha256`, run `anubis evidence-verify`: must FAIL, no flag | Nobody |
| 2 | A third party re-checks every obligation with no Anubis binary | CI pulls a pinned `drat-trim` and `cake_lpr`, runs them over a fresh bundle | `cake_lpr`/GRAT for SAT solvers; no *language* toolchain |
| 3 | Certificate coverage is a number in the verdict | `PASS (certified 11/12, 1 trusted to z3: bvsdiv at src.anb:7)` | Nobody publishes how much of a proof you take on faith |
| 4 | No row has "trust us" as its witness | Every obligation, including vacuity queries, carries a witness resolving to a file | No verifier certifies its own SAT/vacuity direction |
| 5 | Same input, same verdict, independent of load | Full corpus, 10 runs each at 1x/4x/8x core saturation, byte-identical verdicts | Nobody claims load-independence |
| 6 | The kernel, not the checker, denies an undeclared effect | Build without `uses(fs.write)`, run as an ordinary user: `EACCES`/`EPERM` or `SIGSYS` | `pledge`/WASI enforce but do not prove completeness |
| 7 | The kernel policy is a pure function of the program | Add one `net.send`, rebuild, diff the emitted BPF bytes; hand-edit the sealed policy and verification fails | Docker/AppArmor profiles drift silently forever |
| 8 | An open effect row yields the *most* restrictive policy, proved | A Lean theorem beside `EffectSoundness.lean` | Nobody has mechanised sandbox-from-incomplete-analysis soundness |
| 9 | Every refusal is machine-actionable | `anubis check --message-format=json` against a published schema; an agent loop converges on a 20-program repair corpus from JSON alone | rustc has JSON diagnostics but no counterexamples |
| 10 | A vacuous contract is refused | `ensures(true)` fails; adequacy score is a field in `program-evidence.json` | Nobody. Not SPARK, Dafny, Verus, Frama-C |
| 11 | A conformance suite with a DEFER class | An implementation that *accepts* a DEFER test is non-conforming | Nobody. ACATS answers only accept-or-reject |
| 12 | Monotonicity is a conformance property | Accept-set of version N+1 ⊆ version N over a declared corpus | No language standard defines this |
| 13 | The Linux bootstrap fixpoint is sealed | Identical digest in three throwaway guests; CI job; Linux tarball in the release | Rust publishes no self-host fixpoint; Zig ships a WASM blob |
| 14 | Lean-to-Rust binding is stronger than a name grep | Rename a theorem to a stub and the gate fails; property-based differential Lean vs Rust blaster | No toolchain has a proof-to-implementation drift gate |
| 15 | One real implant written *in* Anubis | `uses(net.send, fs.read)`, `secret<T>` key material, kernel-locked to one port and directory, signed receipt the client verifies without installing Anubis | No C2 framework and no verification language. Empty square |
| 16 | An archived proof re-checks years later | A bundle sealed today verifies against a checker built from the published format alone | The only honest operationalisation of a 200-year claim |

---

## 3. Sequence

Ordered by what becomes impossible later, not by appeal.

1. **Irreversible decisions** — editions, governance, and the licence question.
   Days of writing, not engineering. Retrofitting editions forces a default for
   editionless manifests, which is Rust's permanent 2015 debt. *(editions
   landed; see §5)*
2. **Close the evidence loop** — a forged certificate must fail. *(started; see
   §5)*
3. **Deterministic verdict** — bound by work, never by clock. *(started; see §5)*
4. **Diagnostic identity and machine verdict** — `--json`, stable codes,
   `file:line:col` on every refusal. *(started; see §5)*
5. **Certificate upgrade and graded coverage** — real LRAT with hints, so a
   *verified* checker can accept Anubis output; coverage as a number.
   *(coverage landed; the LRAT upgrade has not — see §5)*
6. **Runtime confinement** — the effect row becomes the kernel policy.
7. **Linux first-class and sealed** — a reproducibility fixpoint on Linux.
8. **Close the fragment and bind Lean to Rust.**
9. **Systems substrate** — the things a systems language must have.
10. **Proof-carrying offense** — one real implant, written in the language.
11. **Normative spec, conformance suite, 1.0 freeze.**
12. **Specification adequacy** — refuse vacuous contracts.
13. **Evidence as a standard** — emit into DSSE rather than rebuilding SLSA.
14. **Concurrency with discharged obligations.**

---

## 4. What not to do

The research was blunt about the tempting mistakes. The sharpest:

- **Do not market `--evidence` as proof-carrying until `verify` re-checks.**
  Until then it is hash-carrying. *(This is now addressed; see §5.)*
- **Do not tune the z3 timeout to turn a flaky fixture green.** Move the budget
  into a declared implementation-defined parameter instead. *(Done that way; the
  reasoning and the measurements are in the constant's doc comment.)*
- **Do not let a green board substitute for a wired gate.** The most instructive
  defect in the tree is not a soundness bug: `scripts/verify_proof_bundle.py` is
  a genuine independent RUP checker, cited as evidence in
  `PROOF_CORRESPONDENCE.md`, invoked by nothing, printing `PASS` on bundles
  where a division contract was trusted to z3 alone. Every new gate goes on the
  `EXPECTED_GATES` roster with a floor, or it does not exist.
- **Do not present the Lean theorem count as assurance.** Four different totals
  appear across the project's own documents. Publish a theorem-to-construct map
  instead.
- **Do not add a 129th offensive subcommand.** There are ~128 already and no
  implant, listener or lateral module written *in* Anubis. The missing artifact
  is one beacon in the language, not more tooling around it.
- **Do not rebuild SLSA, in-toto or Sigstore.** Emit into a DSSE envelope and
  inherit transparency-log inclusion.
- **Do not claim any superlative without checking prior art.** One careless
  "first" costs more credibility than this whole roadmap buys.

---

## 5. Already landed

Three items from the sequence have first steps in the tree.

**Irreversible decisions — editions.** `[package] edition = "2026"` is parsed
and validated. An edition this compiler does not recognise is refused with
`ANUBIS_EDITION_UNKNOWN` rather than compiled under today's rules, which is what
makes adding a future edition safe. A missing edition resolves to 2026 now and
becomes an error at 1.0 — the window to avoid Rust's permanent 2015 default is
open only while no third-party code exists. `docs/language/EDITIONS.md` states
what an edition may change and what it may never change, including that a
soundness fix which only rejects more applies to every edition at once rather
than living on under an old one.

Still open in this workstream: governance, and the licence question the owner
has decided to leave as it stands.

**Close the evidence loop.** `anubis evidence-verify` gained a `.proofs` check
that re-derives every `rup_refutation` obligation by reverse unit propagation,
reading the published DIMACS and DRAT text rather than any solver struct.
`validate.sh` in each bundle replays the certificates too, prefers `sha256sum`
over `shasum` (absent on a stock Linux box, where it was silently reporting every
bundle as tampered), and exits non-zero when no checker is installed rather than
implying the proofs passed. Measured: forged certificate → FAIL exit 1;
truncated refutation → FAIL; honest bundle → PASS, replayed in-process and
independently by `drat-trim`.

Obligations with no certificate are not failures but are counted and named, so
the verdict states how much of itself rests on the solver's word. That is
criterion 3 in embryo.

**Deterministic verdict.** The native solver's wall clock is off by default:
search is bounded by conflicts, gates, clauses and certificate work, all
deterministic. z3 is bounded by `rlimit` rather than `-t`/`-T`. Measurement
found the exact mechanism behind the flapping fixtures: a QF_FP obligation costs
91,502,028 resource units and 8.18 s of CPU against a 10 s timeout, a margin of
1.2x, so under parallel load it crossed the line and a provable contract came
back UNDECIDED.

**Machine-readable verdict.** `anubis check --message-format=json` emits the
`anubis-diagnostics/1` stream: one JSON object per refusal, then a summary, on
stdout and nothing else. The format is specified in
`docs/language/DIAGNOSTICS_JSON.md`.

The design decision worth recording is what it refuses to do. SARIF was the
obvious carrier and was rejected as the normative format, because its `level`
enum collapses `disproved` (a counterexample exists; the obligation is false)
into the same value as `undecided` (no proof and no counterexample; it may still
be true). Those have different repairs, and an agent that cannot tell them apart
weakens a contract to clear a solver's undecided verdict — the laundering this
language exists to refuse, performed automatically. So the epistemic class is a
field, never a severity and never an exit code.

Three absences are deliberate and locked by tests. `suggestions` is always
empty, because the compiler's existing "possible fix" re-fails when applied
verbatim and a machine-readable suggestion would be applied rather than eyed.
`location` is omitted on semantic refusals, whose span is still start-of-file —
an absent field says "not tracked", a `1:1` says "here". And nothing anywhere in
the stream names a bypass, a suppression key or a confidence score.

An unknown `--message-format` value is refused rather than treated as `human`,
because an empty stdout reads exactly like a clean run to anything counting
findings.

**What this does not yet meet:** criterion 9 asks for an agent loop that
converges on a 20-program repair corpus from the JSON alone. That corpus does
not exist and the loop has not been run. What exists is the format, its schema,
and 30 tests over both. The criterion is unmet, and the flag shipping is not the
same as the criterion being met.

**Graded certificate coverage — criterion 3.** The ordinary verdict now states
how much of itself rests on a witness and how much on the solver's word:

```
check passed
certificates: 9/10 obligations carry a re-checkable witness; 1 trusted to the solver with none
  no witness (REG-002, out of the proven fragment): ensures:(bvsge (bvsdiv anb_a anb_b) (_ bv0 64))
```

The same numbers appear as a `coverage` object on the JSON summary, on a pass as
well as a failure. The obligations with no witness are **named**, not merely
counted, so the admission is specific.

The information was already being computed and thrown away. A native verdict of
`Unsat` requires all of the proven-fragment gate, a CDCL root refutation, and an
independent checker accepting that refutation; an obligation the native lane
declines falls through to z3 alone. That decision was made per obligation and
then discarded, so a `PASS` read identically whether every obligation carried a
machine-checked refutation or none did. It no longer does.

Two properties are worth stating because they are what makes the number worth
trusting. Coverage may understate itself and may never overstate it: an
obligation whose provenance the classifier does not recognise counts as neither,
rather than as certified. And a program with nothing to prove reports no
coverage at all rather than `0/0`, which would read as "nothing was witnessed"
instead of "nothing was attempted".

**What coverage is not.** A certified obligation means a stranger can re-check
that *this CNF is unsatisfiable*. It does not mean the program was verified end
to end, and no wording in the verdict or the schema says otherwise — locked by a
test that rejects "fully verified", "proven correct" and "guaranteed" in the
verdict line. The other half of criterion 5, the LRAT-with-hints upgrade that
would let a *verified* checker such as `cake_lpr` accept Anubis output, has not
been done; the emitted certificates are still DRAT.

**What is still open on both:** the certificate covers only the Lean-backed
fragment, so anything touching `bvsdiv`, `bvurem`, `bvsrem`, `bvudiv`, `bvashr`
or `sign_extend` is still discharged by z3 alone with no witness — REG-002.
And even a perfect replay proves only that *this CNF* is unsatisfiable. Nothing
yet binds the CNF to its `.smt2`, or the `.smt2` to the source. That chain is
still the compiler's word, `docs/PROOF_CORRESPONDENCE.md` says so plainly, and
closing it is the research programme rather than a sprint.
