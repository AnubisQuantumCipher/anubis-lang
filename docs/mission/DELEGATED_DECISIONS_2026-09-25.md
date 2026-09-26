# Decisions made under the owner's delegation (2026-09-25)

On 2026-09-25 the owner handed the mission to the lead with: "wherever I'm not sure, you figure
out and make it happen." Four questions the handoff had reserved for the owner are therefore
decided here, by the lead, with the reason and the evidence for each. Each is reversible: the
owner may override any of them, and an override is recorded below it rather than by editing it.

These are decisions about what the mission will do. None of them marks anything fixed.

## D1. IFC-PC-EGRESS: egress executed under a secret-decided condition is refused in Safe mode

**Question.** Should a `print`/`send` (any egress) that runs only when a secret decides a
condition be refused? (The assignment and `return` forms are already refused in Safe mode by
SPEC_1_0_FREEZE §5 and are ordinary fixes where a lane misses them.)

**Decision.** Yes. In Safe mode an egress whose execution is controlled by a condition that a
secret (or a whole struct holding one) decides is refused with `ANUBIS_IMPLICIT_FLOW`, in every
lane. The explicit escape is `declassify(<condition>)`, which is visible, auditable, and already
part of the language. Termination, timing and resource channels stay outside the confidentiality
claim and must be stated as such (mandate §8); this decision covers the presence, absence, count
and order of egress events.

**Why.** It is not a one-bit question. Measured on the head checker (pin `anubis-ord3b`,
e7b56507) on 2026-09-25:

```
fn main() {
    let k: secret<i64> = 42
    for j in 0..8 {
        if (k >> j) & 1 == 1 { print("1") } else { print("0") }
    }
}
```

`anubis check` accepts it (rc 0) and `anubis run` prints `0 1 0 1 0 1 0 0`, the bits of the
secret. Any secret can be printed this way with no assignment under the secret condition, so an
information-flow guarantee that allows it does not bound what a program discloses. Refusing it
is the standard noninterference rule, matches the language's existing rejection of assignments
and returns under a secret condition, and keeps `declassify` as the one place disclosure
happens. The probe set is in the mission scratch (`claude-s2/pcegress/`) and becomes matrix
cases with the fix.

**Compatibility.** A program the checker accepted before can be refused after. Every such flip
on the corpus and the examples is classified when the fix lands: a real disclosure gets an
explicit `declassify` only where the program's purpose is to reveal that fact (and the change is
listed), never silently.

## D2. L-SHADOW-1: lexical shadowing, reached through a diagnostic first and an edition second

**Question.** A local or formal does not shadow a same-named user function in call position
(`let check = strict; check(x)` calls the global `check`), although it does in value position.

**Decision.** The intended rule is lexical: the innermost binding wins in every position. Because
changing which function an existing program calls is a semantic change, it is reached in two
steps. (1) In the current edition, a call whose name resolves to a user function while a
same-named local or formal is in scope gets a diagnostic naming both bindings (the program's
meaning does not change, and the checker keeps modelling what the runtime does). (2) The next
edition adopts lexical scoping, with a migration that renames the local, and compatibility
fixtures for both editions. The second L-SHADOW-1 row (`open_whole8_r31_r31i_s1`, a branch write
followed by a shadow) is a lane defect, not part of this question: it is fixed as a leak if the
round-39 units do not close it.

**Why.** The current rule surprises readers and agents in exactly the security-relevant case
(calling a different checker than the one named), but the checker already models the runtime,
so there is no soundness gap to close by silently changing meaning; the mandate (section 7)
requires an edition, a diagnostic and a migration for deliberate semantic changes.

## D3. Which completion bar governs

**Decision.** The owner's mandate (`docs/mission/EXECUTION_MANDATE_2026-09-23.md`) and the two
finish lines in `docs/MISSION.md` govern, with the mandate's section 19 as the definition of
done. As already recorded in `docs/mission/REQUIREMENTS.md` (authority reconciliation): the
blueprint's phase stops do not require a separate approval for each routine phase, and
production-linked correspondence and full self-hosting are mission outcomes despite the older
"unscheduled research" wording. `docs/ROADMAP_AI_ERA.md`'s sixteen criteria are not a competing
bar: each is mapped into the requirement map under finish line A or B and must hold there.

**Why.** The mandate is the owner's most recent and most specific statement, and it states the
finish lines, the continuation authority and the treatment of correspondence explicitly. Keeping
three bars live invites reporting against whichever is easiest.

**Not changed.** Merge, release, tag, force-push and security-policy changes still need the
owner's explicit authorization. The disposable-guest rule for research, crash-PoC, fuzz and
exploit execution is unchanged (see D5).

## D4. Worktree `modreq`: keep

**Decision.** Keep it. Its uncommitted diff (a measurement prototype that models integer
parameters of functions calling contracted functions, and turns an unencodable direct-call
`requires` into an explicit unresolved obligation) addresses mandate section 5's "direct-call
`requires` paths that emit no obligation when modeling fails". It is preserved byte-for-byte at
`~/.cache/anubis-item21/handoff/modreq-prototype-at-2047df10.diff` (sha256
14980588bb51aac9d0289302b97d242d9b9279ebe49ebdacc825fffe55a0a48a) and is an input to that unit,
not a landed fix.

## D5. What the disposable-guest rule covers

**Question (raised by the 2026-09-25 continuation, which stopped on it).** The alias candidate's
workspace test suite aborted with a stack overflow in `closure_analysis_limit`. Is re-running and
debugging that test "crash execution" that must happen only in a disposable Tart guest?

**Decision.** No. The rule in AGENTS.md covers research-gated, crash-PoC, fuzz, exploit and
agent/C2-class *programs* run through Anubis (`anubis fuzz`, `anubis run --allow-research`,
`exploit-run`). The compiler's own unit and integration tests are the ordinary gate that every
unit runs (verify.sh and the hosted gate run them); a test that aborts is a compiler defect found
by that gate. It is run memory-capped (`capped.sh`, no core dumps) like every other check.
Deliberately adversarial memory- and stack-exhaustion probing of the checker (the checker-limits
reviews' memory-pressure runner) is closer to the rule's intent; those runs stay capped, their
isolation is reported as "host, memory-capped scope", and they are never described as isolated.

## Later decisions (2026-09-26)

Three more questions came up while landing IFC v2 (mandate section 6). They are decided the same
way, and are equally reversible.

## D6. `exit` and `panic` end the program like a `return`

**Question.** D1 keeps termination channels outside the confidentiality claim. Is
`if secret { exit(0) } print("after")` (or the same with `panic`) such a channel, or an implicit
flow D1 covers?

**Decision.** An implicit flow. Both runs terminate normally and print different things: the
presence of the later egress depends on the secret exactly as it does after
`if secret { return }` in `main`, which every lane already refuses. `exit(code)` and `panic(msg)`
are explicit statements the program chose to execute, so IFC v2 treats them as ending the program
under the program counter they run at: the rest of the function, and of every caller, runs under
that label (their message and status are egress as before). What stays outside the claim is the
implicit abort of a runtime trap (an out-of-range index, a missing key, a failed assertion) and
divergence.

**Evidence.** The adversarial review of IFC v2 (round 1) witnessed `exit` under a secret condition
followed by a print (CTL-L12, `ifc2r1_ctl_l12_exit_under_secret`).

## D7. A runtime trap's message carries no operand value

**Question.** The runtime's fail-closed traps printed their operands to stderr (the out-of-range
index and the list's length, the missing key, a negative count, the unmatched match value, a file
path). A secret operand then reaches stderr although no `print` in the program touches it. Model
every trap as egress in the checker, or change the messages?

**Decision.** Change the messages: a trap names its error code and what went wrong, never the
program's data. The checker cannot sensibly treat every index expression as an egress of its
operand, and the diagnostic value of the operand in the message is small next to what it leaks.
Operand type names (`expected a list, got int`) are still printed: a type-dependent trap already
discloses the type by trapping, and they carry most of the diagnostic value; that residual is
stated with the termination channel.

**Evidence.** Review round 1 witnessed `xs[k]` and `m[str(k)]` printing the secret `k` in the trap
message (L17, L21); the runtime test `runtime_traps_do_not_print_operand_values` holds the fix for
the core runtime (e516b1f3). Review round 2 reported further messages that still print an
operand (in the crypto and exploit-kit runtimes, and the arity trap of a builtin used as a
value); they are the next unit, not covered by this entry's evidence.

## D8. How IFC v2 is enforced

**Question.** The mandate asks for a semantic foundation for information flow (an analysis that
mirrors the runtime's value semantics) rather than more syntactic special cases. When it lands,
does it replace the existing flow lanes, run beside them, or run in a shadow mode?

**Decision.** Beside them, always on in Safe mode: a program is refused if any lane, IFC v2
included, finds a flow. Replacing the lanes would require IFC v2 to be at least as strict on every
registered case, and measured over the matrix it is not (117 registered rejections are contract,
capability or effect cases that are not information flow, and IFC v2 reports only information
flow). A shadow mode would leave the 58 registered leaks the lanes accept open while IFC v2 already
refuses them. The union adds no over-refusal on the corpus (968 programs, 0 verdict changes) and
none on the matrix's valid cases. `anubis ifc2-report` shows IFC v2's findings alone for
measurement. Retiring flow lanes that IFC v2 subsumes is a later decision, taken on the matrix.
