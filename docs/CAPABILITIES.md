# What Anubis can do — the full capability surface

**Tier 2 · reference.** The [README](../README.md) gives a seven-line summary; this is the detail
behind it. Read it with [`docs/CLAIMS.md`](CLAIMS.md) open — that file is the project's declared
single source of truth for current status, and where this page and that file disagree, **that file
wins**.

## How to read the status column

This project bans a freestanding "REAL" stamp. A claim here is one of:

| Mark | Means |
|---|---|
| ✅ **gated** | backed by a re-runnable command named in the row; the claim is only as strong as re-running it |
| 🟡 **partial — named residual** | real slices work, and the gap is written down rather than papered over; the residual is named in [`docs/CLAIMS.md`](CLAIMS.md) |
| 🔴 **known-open** | a measured defect is currently published against this surface |
| ⬜ **planned** | not built; stated so it is not mistaken for shipped |

**A green gate is an empty published residual inventory, not a proof of total soundness.** Absence
of a red row is not evidence of absence.

---

## 🛡️ Verify — prove your contracts, or get the counterexample

| | Status | |
|---|---|---|
| **Contract checking** | ✅ | `requires` / `ensures` / `assert` discharged by SMT, with real solver counterexamples; `--suggest-contracts` infers clauses for you |
| **Verified build front door** | ✅ | Without the explicit `--no-verify` escape hatch, `anubis build` runs the same checker and refuses the currently modeled unproven-contract cases |
| **Contract lanes** | 🟡 | integer (exact i64) · float **comparison** · string **equality/length** · bounded arrays · loop invariants · struct fields. Outside the modeled fragment the checker **defers** — see the scoped promise below |
| **Native SMT solver** | ✅ | a zero-dependency, Lean-verified QF_BV solver; **default-authoritative** on the proven integer fragment (opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0`); Z3 cross-checks when present |
| **Mechanized components** | 🟡 | 162 Lean 4 theorems across 15 modules cover the stated encoding, bit-blast, non-interference, and effect lemmas; `run_formal_gate.sh` checks those files and rejects `sorry`/`admit`/`axiom`. **Not** a proof of total language soundness. Hosted CI installs the pinned Lean toolchain and requires this gate; see [CI reality](#ci-reality) below |

### What a green `check` actually promises

The original promise sentence was found false in its second clause on 2026-07-28 and has been
replaced. The current scoped sentence, quoted from [`docs/CLAIMS.md`](CLAIMS.md):

> `anubis check` PASS means: every obligation class listed as **proved** was discharged or the check
> failed; every class listed as **deferred** produced a **visible residual** — a diagnostic or a
> report field — not a silent accept; and the source bytes were **fully tokenized** (unknown
> characters refuse). Deferred classes are **not** "proved absent."

A deferral is an accept: the program compiles and runs. The three-tier policy (MUST REFUSE /
deferred-but-named / runtime-enforced) and the list of what sits in each tier is in
[`docs/CLAIMS.md`](CLAIMS.md). Do not quote the older, unscoped sentence.

### It proves its own math

Most verifiers lean on **Z3** — a large, external, unverified C++ trusted base. Anubis is removing
it from the loop.

The [`solver/`](../solver/) crate is a **from-scratch QF_BV decision procedure with zero external
dependency** (`std` only, empty `[dependencies]`): an SMT-LIB2 parser, a Tseitin bit-blaster, and a
CDCL SAT engine (watched literals, 1-UIP learning, VSIDS, Luby restarts). Every bit-blast the
authoritative path relies on is **machine-checked in Lean 4 core** (no Mathlib) — the ripple-carry
adder, all eight signed/unsigned comparators, equality, bitwise `& | ^ ~`, negation, both shifts,
and the structural ops — the operation surface a real integer contract emits, **except division**,
each proven equal to the runtime's `i64` semantics.

```bash
anubis check <int-contract>.anb                                 # native-authoritative by default
ANUBIS_NATIVE_AUTHORITATIVE=0 anubis check <int-contract>.anb   # opt out → z3-only authority
bash scripts/run_native_authoritative_gate.sh                   # cert + ≡ Z3 corpus + TCB-drop + fragment danger
bash scripts/run_formal_gate.sh                                 # Lean theorem check, no sorry/admit/axiom
```

> **Honest boundary.** Unsat only after a **verified pure RUP certificate** (`solver/src/lrat.rs`),
> Sat only after independent model replay. Z3 **cross-checks every native verdict when present**,
> failing closed on disagreement. **Division / remainder** (`bvsdiv`/`bvsrem`/`bvudiv`/`bvurem`) stay
> z3-deferred — the only op class a real integer contract emits that the native lane declines.
> Variable×variable multiply is **not** deferred: it is machine-checked (`mulVar_correct`,
> `formal/Anubis/BitBlast.lean`) and admitted by the fragment gate (`MulVar` in `PROVEN_OP_TAGS`).

Deeper: [`docs/SOLVER_PIPELINE_MAP.md`](SOLVER_PIPELINE_MAP.md) · [`solver/README.md`](../solver/README.md)

---

## 🔒 Secure by construction — types that stop data from leaking

| | Status | |
|---|---|---|
| **Information flow** | 🟡 | `tainted<T>` (integrity) + `secret<T>` (confidentiality) are enforced across the currently instrumented carriers; the named sink fixtures reject unless routed through `declassify(value, policy, reason)`. Composition completeness is not claimed |
| **The lethal trifecta** | 🟡 | the named direct and summarized forms that *read private data*, *take untrusted input*, **and** *can exfiltrate* reject with `ANUBIS_LETHAL_TRIFECTA`; residual composition shapes remain governed by [`docs/CLAIMS.md`](CLAIMS.md) |
| **Effects & capabilities** | 🔴 | transitive effect inference (`fs.read` `fs.write` `net.send` `shell` `time.now` `rand.gen`) and linear **use-once** capability tokens (`cap_acquire`/`cap_use`, reuse is `ANUBIS_CAPABILITY_REUSE`) both work — **but a capability-carrying function reaching an application site through a container passed as a parameter is currently accepted.** Open, measured, published: [`docs/CLAIMS.md` § Container-PARAM carrier](CLAIMS.md) |
| **Implicit-flow rejection** | 🟡 | named assignment-to-public forms under a secret program counter reject with `ANUBIS_IMPLICIT_FLOW`, covering the cited `if`/`match`/guard/loop/`if let` fixtures in statement and value position. Full Jif/FlowCaml-style PC labelling at every join is not implemented, so behavior outside those fixtures is a named residual |

> **Known-open, worth knowing before you rely on this lane.** The container-PARAM carrier above is a
> measured false accept in the capability lane as of 2026-07-28: `app([leak])` where the callee does
> `xs[0](…)` is ACCEPTed, while the same value *returned* and applied locally is REJECTed. Check
> [`docs/CLAIMS.md`](CLAIMS.md) for whether it has closed since.

Deeper: [`docs/language/INFORMATION_FLOW.md`](language/INFORMATION_FLOW.md) — including how to
*construct* a `secret<T>`.

---

## 🧾 Prove (zero-knowledge) — attest a computation without revealing it

| | Status | |
|---|---|---|
| **Program-bound RISC Zero proving** | ✅ | `anubis prove --backend risc0` lowers `main()` to a real zkVM guest; `proof_assert` is an in-circuit constraint (a false one yields *no valid receipt*) |
| **Parameterized proofs + named journals** | ✅ | `--input-json`/`--input-file`; `proof_commit_u32`/`_bool` name public outputs; ImageID binds the *program*, the journal binds the *inputs* |
| **Private witnesses** | ✅ | inputs read via `proof_input_*` stay off the journal — prove `lo <= x <= hi` without revealing `x` |
| **Standalone receipt verification** | ✅ | `anubis verify-receipt --receipt … --image-id …` cold-verifies against ImageID |
| **Metal-hybrid rv32im lane** | 🟡 | vendored `risc0-circuit-rv32im` + CPU fallback; works on Tier-2 Apple Silicon, `ANUBIS_REQUIRE_METAL=1` fails closed elsewhere (no speed claim is made) |

Where proving is architecturally young — boxed values in the zkVM, whole-program guest cost — is
stated plainly in [`docs/language/PROOF_SCALING.md`](language/PROOF_SCALING.md).

Deeper: [`docs/proof/RISC0_PARAMETERIZED_INPUT_ABI.md`](proof/RISC0_PARAMETERIZED_INPUT_ABI.md) ·
[`docs/METAL_BACKEND.md`](METAL_BACKEND.md)

---

## 🧱 Confine — hardware isolation derived from the proof

| | Status | |
|---|---|---|
| **Effect-derived confinement** | 🟡 | `anubis vz confine <program>` derives a manifest from the checker's emitted effect set; the named bundle/tamper controls re-derive and byte-compare it on verify. Evidence for the declared schema, not proof that effect discovery is complete |
| **VZ apply network posture** | ✅ | No open NAT by default; `--allow-host` → Softnet default-deny + `/32` allows when `softnet` is on PATH; `--allow-open-nat` is an explicit, named residual |
| **VZ apply mount posture** | 🟡 | the named apply-gate cases show `none` denying and `read-only` forcing `:ro`; unenumerated apply combinations are not covered by that observation |
| **Effect-derived entitlement profile** | 🟡 | `anubis entitlements <program>` derives a profile from the emitted effect set; named verification controls re-derive it. **Derived profile, not enforced until signed** (`apple_enforced_claim: false`) |
| **Non-exportable linear caps + Keychain/SE** | ✅ | Static export-seal; macOS Keychain bind (`kc:`) under signed Development path; optional SE (`se:`); soft fallback; gate `scripts/run_keychain_se_gate.sh` |
| **VM lifecycle (tart lane)** | ✅ | `anubis vz` create / boot / exec / snapshot / stop / delete — the Virtualization.framework lifecycle behind one CLI, on Apple Silicon |
| **Native VZ backend** | 🟡 | `vz native-preflight` validates the generated configuration and its named negative control; the net-free configuration contains zero network devices. Per-hostname egress is substrate-staged, so no broader air-gap claim follows |

> **Isolation is SAFETY, not SECURITY — and the two claims share one word.** The receipt field
> `"isolation": "tart-disposable-guest"` is written by the **host** orchestrator as a hardcoded
> string literal (`tools/anubis/src/vz.rs:1392`), not derived from guest attestation or any hardware
> measurement. The receipt chain protects against *post-hoc tampering*, not against *fabrication at
> write time* — a host process with access to the engagement directory can mint a byte-identical
> receipt with no guest involved. The native VZ air-gap is structural; the tart marker is not.
> Measured 2026-07-28, full write-up in [`docs/CLAIMS.md`](CLAIMS.md).

Deeper: [`docs/APPLE_NATIVE.md`](APPLE_NATIVE.md) · [`docs/TRUST_BOUNDARIES.md`](TRUST_BOUNDARIES.md)

---

## ⚔️ Research — an accountable offensive toolchain (authorized use)

Anubis carries an **engagement-scoped** offensive platform for authorized security work — because a
proof-of-concept is *also* evidence, and every offensive action is logged as a hash-chained receipt
you can verify. It runs, by design, inside disposable, crash-isolated VZ guests. The Tart wrapper
uses shared NAT; true zero-NIC isolation is a separate native-VZ path.

| | Status | |
|---|---|---|
| **Bounty-grade PoC kit** | ✅ | cyclic patterns (`pattern-create`/`pattern-offset`), `p64` packing, `gadget-search`, a `target_run` harness, and **mutation fuzzing of local binaries** (`anubis fuzz`, real process crashes → crash evidence) |
| **Engagement platform (AOP)** | ✅ | scoped workspaces (`engage-init`, authorization charter), an HTTP/JSON C2 listener, beacon `agent-generate`, task queue, and a fail-closed action-receipt hash chain (`receipt-verify`) |
| **Isolated execution** | 🟡 | Host control plane `vz-status`/`vz-start`/`vz-exec`/… drives **Tart shared-NAT guests**. Omitted `vz-start --network` requests the fail-closed default `off`, which Tart refuses because it cannot remove the NIC; pass `--network nat` explicitly for Tart. True zero-NIC requires `anubis vz native-boot`. Tart inventory reports `unknown` when Tart exposes no launch-mode evidence; `unknown` must never be read as `off`. Live offensive work is crash-isolated from the host, but the *receipt's* isolation marker is host-written and forgeable (see the box above), so treat it as an operational safety control, not an attestation |
| **Reporting** | ✅ | `anubis bounty-report` turns an evidence bundle into a structured responsible-disclosure report |
| **High-risk primitives** | 🟡 | process injection is **PLAN_ONLY by default**; live inject requires double authorization. SMB/WinRM lateral remains **PLAN_ONLY** (never executes) |

Running any of this is governed by [`SECURITY.md`](../SECURITY.md) — authorized engagements only.

Deeper: [`docs/language/OFFENSIVE_PLATFORM.md`](language/OFFENSIVE_PLATFORM.md) ·
[`docs/language/POC_KIT.md`](language/POC_KIT.md)

---

## 📦 Evidence, packages & crypto — sign the truth, ship it, re-check it

| | Status | |
|---|---|---|
| **Proof-Carrying Artifacts** | ✅ | `anubis build --evidence` → tamper-evident bundle: source Merkle root, HIR/MIR, taint traces, solver output, SARIF, hashes, Markdown report. `verify` re-derives the claim and fails closed on tamper; `keygen`/`sign` add Ed25519 signatures |
| **Proof-carrying packages** | ✅ | `anubis package` — `Anubis.toml`/`Anubis.lock` with content-`sha256` pins; a dependency's effect/taint/**contract** summaries are re-derived and enforced at the consumer's call sites; a signer `trust` store |
| **Crypto surface** | ✅ | boring primitives, RustCrypto-backed where a vetted crate exists (`sha2`, `aead`/`aes-gcm`, `ed25519-dalek`): SHA-256, HMAC (constant-time verify), AEAD, PBKDF2/Argon2, Ed25519 — via `import std.crypto`; never a novel construction. Post-quantum (ML-KEM/ML-DSA) is ⬜ a documented future path, never hand-rolled |
| **Standard library** | 🟡 | 13 content-locked Anubis-source modules (`compiler/stdlib/std/`): `math` `collections` `iter` `result` `option` `io` `str` `crypto` `net` `rand` `time` `testing`, and `pwn` for the offensive lane. Fail-closed behavior is instrumented for the collection matrix and sealed cells — **the crypto/hash/KDF/random slice is unmeasured** |

Deeper: [`docs/language/PACKAGES.md`](language/PACKAGES.md) ·
[`docs/language/CRYPTO.md`](language/CRYPTO.md) ·
[`docs/language/STDLIB_CORE.md`](language/STDLIB_CORE.md)

---

## 🧰 Run, tool & self-host — a real language, day to day

| | Status | |
|---|---|---|
| **Executable core** | ✅ | Turing-complete: loops, recursion, mutation, enums + `match`, `for x in xs` / `for i in a..b`, structs, maps, closures, `Option`/`Result`/`?`, **213 builtins** (inventory: [`docs/language/BUILTINS.md`](language/BUILTINS.md)) — native Apple-Silicon executables |
| **Type system** | 🟡 | bidirectional inference, traits + coherence. **Generics are decided by a string heuristic** (`compiler/src/middle/ty.rs:258` treats any annotation ≤2 chars and all-uppercase, or containing `<`, as an erased generic) — two measured defects in opposite directions, currently open. Multi-file `import` resolution is in progress |
| **Developer experience** | 🟡 | `fmt` (self-verifying), `test` (`// EXPECT: PASS\|FAIL`), `doc` (Contracts section), `repl`, `lsp` (contract hovers), tree-sitter grammar + VS Code extension — gate: `bash scripts/run_dx_gate.sh out/dx`. **Semantic diagnostics carry no source location**: a lexer error gives `file:line:col` with a caret, while a security-lane refusal is a bare string. Open |
| **Self-hosting spine** | 🟡 | `selfhost/` implements a stage0→stage3 bootstrap plus Anubis-authored effect, type, and taint engines. The named differential gates report the corpus comparison; the post-registry VM fixpoint is currently **unsealed** and must not be represented as current proof |

Deeper: [`LANGUAGE.md`](../LANGUAGE.md) · [`docs/CLI.md`](CLI.md) ·
[`docs/language/SELFHOST.md`](language/SELFHOST.md)

---

## CI reality

What the hosted CI workflow is configured to execute on this branch:

- The canonical roster contains **29 named gates**. Hosted CI installs the pinned Lean toolchain,
  runs the formal gate explicitly, then runs `scripts/audit_unified.sh --profile hosted`.
- A successful hosted result requires **28 PASS plus exactly `G9_poc_kit=EXTERNAL`**. G14 is only
  its non-executing 5-check host-isolation witness. The verdict is `HOSTED_PASS`, never a full seal.
- G9, the full 34-check G14 battery, and require-Metal parity are separately approved operator-run
  evidence outside public CI. No persistent self-hosted runner is authorized by this design.
- This text describes workflow intent, not a current GitHub result. Re-derive the live status:

```bash
gh run list --workflow anubis-ci --status completed --limit 1 \
  --json conclusion,displayTitle,headBranch
bash scripts/audit_unified.sh --profile hosted  # same bounded hosted contract, locally
```

---

## What is deliberately *not* here

Every 🟡 and 🔴 above is a published gap rather than a silent one. The authoritative, dated list of
open items is [`docs/CLAIMS.md` § Known open issues](CLAIMS.md) — this page must not restate it, and
if the two disagree, that file is correct and this one is stale. The per-feature "we do not support
this" record is [`docs/language/UNSUPPORTED.md`](language/UNSUPPORTED.md).
