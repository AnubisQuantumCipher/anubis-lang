# Anubis Claims (1.0 freeze — evidence-first)

See `MATURITY_CLAIM_MATRIX.md` for historical gate rows. Living freeze:
[`docs/language/SPEC_1_0_FREEZE.md`](language/SPEC_1_0_FREEZE.md) ·
[`docs/language/SEMVER_1_0_POLICY.md`](language/SEMVER_1_0_POLICY.md).

## Known open issues (2026-07-27)

Any A15/A+ seal dated 2026-07-24 or earlier (`ROADMAP_A_PLUS.md`, `A_PLUS_CLOSEOUT.md`,
`A_PLUS_FINAL_REPORT.md`, and the tail of `MATURITY_CLAIM_MATRIX.md`) predates every item below —
read those files' "CLAIMED"/"DONE"/"PASS"/"COMPLETE" language as *what was true on that seal
date*, not as current. **This section is the single source of truth for current status.** Other
owned docs link here; they must not restate the list.

**A green board is when a claim surface is most dangerous.** Read the disease theme, the green
table, and "green = no KNOWN defects" with equal weight.

### The disease — proven across eight separate classes

> **A user writes something down, or a producer computes a label, and a consumer ignores it or
> recomputes it independently.**

That sentence is now **proven** across **at least eight** closed classes this arc (not eight
unrelated bugs — one disease, eight surfaces):

| # | Class | Consumer failure | Closed |
|---|---|---|---|
| 1 | Declared `-> secret/tainted` returns | Summary body-derived only | `6fb055f` |
| 2 | Declared struct **field** qualifier (R1) | Field type ignored at project | `f4d2f37` |
| 3 | (R) runtime + PCA twin | Policy re-derived from assignment provenance | PTAH / `6fb055f` |
| 4 | Stored-callable pipeline (M1) | Apply lost stored/returned callable identity | `47ab408`…`eb5be1f` |
| 5 | Multi-candidate denotation (M2) | Expression not a single Var/Lambda | `ae1fa17`…`c491237` |
| 6 | Value-block nested discharge (M3) | Stmt-`if` in value block not walked | `2168bf1` |
| 7 | **D1/D2/D3** field qualifier **through CALL result** | Place type ignored return type of free-fn/method | `f9fc7a7` |
| 8 | **D4** enum-payload qualifier at match binder | Payload declaration ignored at bind | `c9415b7` |

**Same disease, additional closed surfaces this stamp:**

| Class | Failure | Closed |
|---|---|---|
| **Research authorization bypass** | Bare `@research { … }` elevated out of Safe with **no** authorization — any Safe program could obtain research capabilities by wrapping code | `e6ebfd2` |
| **Unknown attributes** | Silently ignored (fail-open on the declaration surface) | `ec65724` fail-closed |

Prefer making the correct binding reachable over a parallel consumer.

The original **eleven** published red witnesses closed via **four mechanisms** (M1, M2-reg,
M2-B, M3) — not eleven fixes. D1–D4 and research/attr closes extend the same disease map.

### Green means no KNOWN defects — not no defects

**Green corpus = no defects in the published residual inventory. It does not mean no defects.**

Do not stamp "false-accept class closed forever," "roadmap soundness complete," or "Safe is
total." Residual composition shapes may still exist (e.g. D5 generic field instantiation, D6
struct-lit IF-at-construction from earlier HORUS census) without a current published red row.
Absence of a red row is **not** evidence of absence.

### Currently green (re-stamped 2026-07-27 GROK-MAAT round 8 — not a total-soundness claim)

**Tip commits:** `ec65724` (unknown attr fail-closed) · `e6ebfd2` (research auth bypass) ·
`c9415b7` (D4) · `f9fc7a7` (D1/D2/D3). Live instrument: `./target/release/anubis` (mtime
2026-07-27 02:14 this pass).

| Surface | Observation | Repro / boundary |
|---|---|---|
| **Security fixtures** | Lead gate **294/294 PASS**. Live disk inventory **294** `.anb`; **published red list EMPTY** (0 `EXPECT: FAIL` still check-PASS this pass) | Green ≠ no bugs. Re-enumerate command below. |
| **Language core** | **244/244 PASS** | pin `ANUBIS_BIN` (§6) |
| **Stdlib fail-closed** | **86/86 PASS** | `ANUBIS_BIN=./target/release/anubis bash scripts/run_stdlib_failclosed_gate.sh --out out/…` |
| **Capset selfhost** | **5/5 PASS** | `bash scripts/run_capset_selfhost_gate.sh` |
| **Taint / type / effect selfhost** | **0 disagreements** each | lead-verified |
| **Formal gate** | **PASS** — every theorem machine-checked; **no `sorry` / `admit` / free `axiom`** | `bash scripts/run_formal_gate.sh`; Lean **162 theorems / 15 modules** (comment-stripped) |
| **Native authoritative** | **PASS over 842 files, 0 mismatches** | `bash scripts/run_native_authoritative_gate.sh` |
| Research elevation | Bare `@research` **without** authorization → REJECT | Live: `research_block_without_authorization_rejects.anb` EXIT=1 |
| Unknown attributes | **Fail closed** | Live: `unknown_attribute_rejects.anb` EXIT=1 |
| Ordinary Safe `run` | Vault contacts EXIT=0 post-PTAH | Proof/shell non-run by design (§2 B) |
| VM seal of post-registry fixpoint | **Pending** | Do not publish host fixpoint as sealed |

#### Honest-number methodology

**A full green gate is an empty published residual inventory, not a proof of total Safe soundness.**

- **Do not** rewrite as "100% secure."
- **Do not** quote older stamps (219/219, 216/219, …) as current without re-run.
- **Do** re-enumerate after any checker change.
- **Do** seal with one pinned `ANUBIS_BIN` (§6).

```bash
# expect ZERO lines if published red inventory still empty
for f in examples/security/*_rejects.anb; do
  ./target/release/anubis check "$f" >/dev/null 2>&1 && echo "RED $f"
done
find examples/security -name '*.anb' | wc -l
```

Counting rules: **Lean = 162 / 15**. **Builtins ≈ 213** (five-function union).

### Open — load-bearing (blocks honest "complete")

1. **Composition residuals — D1–D6 closed; the class is NOT claimed total.**
   D1–D4 closed earlier this arc. **D5** (generic field instantiation) and **D6**
   (IF-at-construction) now have fixtures and are closed (2026-07-27), which is what this entry
   demanded — "not claimed closed unless fixtures + mechanism land":

   | shape | fixture | verdict |
   |---|---|---|
   | D5 `Box<secret<i64>>` field via instantiation | `d5_generic_field_instantiation_rejects` | REJECT |
   | D5 instantiation in a PARAMETER's declared type | `d5_generic_field_via_param_rejects` | REJECT |
   | D6 construction inside a value-position `if` | `d6_if_at_construction_rejects` | REJECT |
   | D6 construction inside a `match` | `d6_match_at_construction_rejects` | REJECT |

   Each ships an over-rejection guard (`d5_generic_field_plain_accepts`,
   `d6_if_at_construction_public_accepts`) so a future instantiation fix that marks EVERY generic
   field, or every join-constructed struct, is caught rather than shipped.

   **Still not a totality claim.** These six are the shapes that were NAMED. The
   function-identity carrier family closed the same day (bare name, alias chain, if/match join,
   list/struct/map/enum element, return, identity forwarder, pass-through builtin, argument
   position) with one carrier still open — see item 7. Green board does not invent completeness.

2. **check/run divergence — (R) CLOSED; (B) residual named.**  
   **(A)=0, (B)=7, (R)=3**; (R)+PCA **CLOSED**. **(B)** non-run by design — do not equate
   `check` PASS with ordinary `run` for shell/symbolic.

3. **Self-host registry — HOST-FIXED; VM seal pending.**  
   Do not publish post-drift host fixpoint as sealed.

### `anubis run` fail-closed — the bounded residual (2026-07-27)

Adopted verbatim from the agent that measured it, in preference to any whole-surface green number:

> **`anubis run` fail-closed is instrumented for the collection-first-argument matrix and the sealed
> domain/wrong-type cells under `tests/fixtures/stdlib/*should_fail_closed.anb` (86 fixtures after
> A–L + M–Z merge + elevation corpus), plus DOC_OK locks under `tests/fixtures/stdlib/doc_ok/` (18 fixtures). It is NOT
> fail-closed over the full 213-builtin surface: crypto/hash/KDF/random/x25519, unenumerated
> arity/IO/capability matrices, and intentional soft conversions (`int`/`float`/`parse_*`, string
> auto-stringify, IEEE NaN/inf on real floats, position-predicate −1, empty sum/product monoids)
> remain outside that claim.**

Coverage is a three-way union, stated so it is auditable rather than assumed:

| Slice | Names | Status |
|---|---:|---|
| A–L | 107 | sealed, 15 fail-closed fixtures |
| M–Z non-crypto | 87 | sealed, 16 fail-closed fixtures |
| crypto / hash / KDF / random / x25519 / `pwn.anb` | 19 | **UNMEASURED** |

**213** is derived by command from `run.rs` (the deduplicated union of `emit_builtin_call`, its
inline `matches!`, `is_proof_input_builtin`, `is_poc_kit_builtin`, `is_non_run_builtin`), not from the
README's "~150".

31 builtins were returning a plausible WRONG value at `rc=0` rather than refusing — `factorial("5")`
→ `120`, `pow("2", 3)` → `8`, `len(42)` → `0`, `substr("hello", -1, 2)` → `he`,
`times("2", |i| i)` → `[0, 1]`. That is the failure mode that corrupts a proof instead of stopping
it: the value reaches a `requires`/`ensures` and makes the contract hold for the wrong reason. All 31
now fail closed; documented leniency is unchanged and locked by must-stay-PASS fixtures, verified by
the distinction `sqrt("x")` FAILS while `sqrt(-1.0)` still returns NaN.

### Open — boundary honesty / process (not silent overclaims)

4. **VZ isolation is SAFETY, not SECURITY** — host-forgeable markers; operator is trust root.  
5. **Research elevation requires authorization** — bypass **CLOSED** (`e6ebfd2`); dual-use
   research remains intentional with explicit authorization, not a Safe free ride.  
6. **Harness integrity + instrument fact — CLOSED 2026-07-27.** Language fixtures defaulted
   **DEBUG** while security graded **RELEASE**, so the two headline numbers described two different
   compilers. Both gates now use one instrument cascade and `audit_unified.sh` exports `ANUBIS_BIN`
   after the build, so CI cannot publish two builds as one number.

   Two further instrument defects were found and closed while verifying this: a fixture with no
   `EXPECT:` header was graded **expected-to-pass** (a headerless `*_rejects.anb` containing a real
   leak scored GREEN — demonstrated, not inferred), and the seal itself printed **SEAL_PASS** with
   two required gates SKIPPED, a constituent grading `/tmp/WRONG-BINARY`, and a recorded snapshot
   hash that did not match the artifact. Both are closed with microbenches that show each guard
   FIRING rather than merely present.

   Binaries are now published as content-addressed read-only pins (`scripts/publish_pin.sh`) so a
   rebuild cannot mutate the instrument an agent is mid-measurement on.

7. **Function-identity carrier — CLOSED 2026-07-27 (`0eb5977`).** A function reference reaching an
   application site through a container built by `push`/`insert` rather than a literal
   (`let fs = []; push(fs, key); app(fs)`) accepted and printed the secret at runtime.

   Cause: the push seeder computed labels with `expr_secret_source_m` / `expr_taint_source_m`, which
   recognise a `Var` only when the LOCAL BINDING carries the label — sound for VALUES, but a bare
   reference to a top-level `secret<T>`-returning function has no local binding, so the pushed
   element read clean. It now uses `container_element_secret` / `container_element_taint`, the same
   helpers the literal twin (`[key]`) and eta twin (`push(fs, || key())`) already routed through.

   An earlier candidate fix was measured INERT and REVERTED rather than shipped: a synthesized
   `|| key()` captures nothing and has no effect, so the push resolver's capturing/effectful
   fallbacks both missed it. Dead code that looks like a fix is worse than a stated gap.

   `let fs = push(e, key); app(fs)` still accepts and is NOT a false accept: `push` returns `0`
   rather than the container, so the program panics before reaching any sink. That is a deferral on
   an unrunnable program — and separately a footgun, tracked as item 8.

8. **`push` expression-position return — CLOSED 2026-07-27.** `let ys = push(xs, 3); len(ys)` had
   `check` rc=0 and `run` rc=1 (panic): the expression lowering returned `AnubisValue::Int(0)`, a
   placeholder, so functional-style use silently bound a non-container. It now returns the container,
   matching its siblings (`pop`, `insert`, `remove` all return something meaningful). Statement-position
   `push(xs, v)` is lowered separately and unaffected; both are pinned by
   `push_expression_returns_container_doc_ok.anb`. Opened and closed the same day.

### Resolved this arc (do not re-open without new evidence)

7. ~~Published security red inventory (the eleven + D/research witnesses in corpus)~~ **EMPTY**
   this pass — live zero known-red.  
8. ~~D1/D2/D3 field qualifier through CALL result~~ **RESOLVED** `f9fc7a7`.  
9. ~~D4 enum-payload qualifier at match binder~~ **RESOLVED** `c9415b7`.  
10. ~~Research-block authorization bypass~~ **RESOLVED** `e6ebfd2` — bare `@research` no longer
    elevates Safe.  
11. ~~Unknown attributes silently ignored~~ **RESOLVED** `ec65724` fail-closed.  
12. ~~Four mechanisms for the original eleven~~ **RESOLVED** (M1, M2-reg, M2-B, M3).  
13. ~~Declared returns / R1 fields / (R)+PCA / stdlib 45/45 / capset 5/5~~ **RESOLVED**.  
14. ~~P0 mul hang / Tier 1 / self-host harness~~ **RESOLVED**.

**Status vocabulary:** freestanding **REAL** / production-grade / fully proven / "roadmap
complete" stamps are banned unless the same line cites a re-runnable command + observation (or a
dated seal path that is not re-read as current). A claim is (a) re-runnable with command +
observation, (b) sealed under a dated artifact path, (c) **partial** with the gap named
(**named residual**), or (d) **not claimed**. Aspirational work is a named residual — never a
silent DONE.

## Portable 1.0 surface (grounded)

**Read with § Known open issues.** Rows marked **CLAIMED** below are *true for their cited
command/fixture shape* (or a dated seal). They are **not** a claim that Safe-mode soundness is
total: the open false-accept / walker-parity class (item 1 above) is an explicit **named residual
on every taint / secret / capability / effect row** until OPUS5's queue is empty and re-hunted.

| Claim | Evidence (command + observation) | Boundary |
|-------|----------------------------------|----------|
| Evidence-native compiler/toolchain | `cargo build -p anubis` (workspace); CI sealed suite on branch | Not a claim about every possible target triple |
| Safe taint enforcement | security **294/294** (lead) / red list empty live; D1–D4 closed; taint selfhost **0 disagreements** | **PARTIAL as total** — green = **no KNOWN defects**, not no defects. Stdlib **86/86** |
| Declassification policy | declassify accept/reject fixture pairs under `tests/fixtures` / security fixtures | Lab policy surface, not a full IFC type system; shell declassify accept is check-policy only (`run` non-run by design — CLAIMS open §2) |
| Solver correctness (supported int fragment) | **lead-verified:** `bash scripts/run_native_authoritative_gate.sh` → **PASS, 842 files, 0 mismatches** | Division deferred; var×var mul claimed; opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0` |
| Wrap-safety VCs (AoRTE-lite) + CEX possible fix | **CLAIMED 2026-07-25; free×free closed 2026-07-25** | On modelable ints: auto wrap-safety for `+`/`-`, **var×const `*`**, and **free×free `*`** via **offline interval product** (no SMT smul hang): bounded factors → prove; unbounded → `ANUBIS_WRAP_RISK` + possible fix; opt-out `ANUBIS_WRAP_SAFETY=0`; unit `cargo test -p anubis-compiler --lib wrap_safety` → 6+; see [`SPARK_VS_ANUBIS.md`](SPARK_VS_ANUBIS.md) | Residual: free `ensures(result == x*y)` posts can still be slow under native-authoritative (separate from wrap-safety); compound factors only offline-proved for simple `bvadd`/`bvsub`/const/var shapes |
| Implicit secret→public (PC) + explicit secret→public (Safe) | **CLAIMED 2026-07-25 for cited fixtures; PARTIAL as total IFC** | Method formals + declared returns + R1 + D1–D4 call/match places; **security 294/294** lead / red list empty | Residual: full PC-join; composition shapes may remain (D5/D6 family) |
| Symbolic-index secret-capturing closure application | **CLAIMED 2026-07-25** | `arr[idx](…)` with non-literal `idx` fail-closed when container holds secret/taint-capturing element (j1 twin of `let g = arr[i]`); unit `symbolic_index_secret_capturing_list_application_fails_closed`; clean symbolic still accepts | Residual: full PC-join; untyped formals still interproc |
| Nested container closure application (`outer[0][0]`, `b.fs[i]`, bind + mid-bind) | **CLAIMED 2026-07-25** | Nested Index/FieldAccess CallExpr + **bind** (`let g = outer[i][0]; g(0)`) + **intermediate mid-bind** (`let mid = outer[0]; mid[0](0)` re-keys `field_closures`; symbolic mid union-projects first segments fail-closed); unit `nested_container_closure_application_fails_closed` (apply + bind + mid lit/sym/clean); clean nested still accepts | Residual: full PC-join not claimed |
| if-expr-built containers seed `field_closures` (incl. nested `Stmt::If` + let-inner) | **CLAIMED 2026-07-25** | `collect_container_closures` walks `Expr::If`/`Match`/`Block`; nested bare `if` as `Stmt::If`; unit `nested_container_closure_application_fails_closed` | Residual: full PC-join not claimed |
| `push`/`insert` seed capturing lambdas into `field_closures` (free + method) | **CLAIMED 2026-07-25** | `apply_container_mutation_taint` seeds pushed/inserted lambdas; concrete path miss fail-closes via `any_capturing_field_closure`; free `push(arr, lam)` + method `arr.push(lam)`; unit cases in `nested_container_closure_application_fails_closed` | Residual: full PC-join; HO rebind beyond push/insert seed |
| Verified causal capability spend | **CLAIMED 2026-07-25 for cited units; PARTIAL as total Safe capability** | Verified privileged builtins require a **live matching-kind** token at the effect (`cap_acquire("kind")` → effect spends it); wrong kind / no token → `ANUBIS_EFFECT_UNAUTHORIZED`; double-spend → `ANUBIS_CAPABILITY_REUSE`; **ambient interproc caller-pays** (units `interproc_caller_pays_*`); fixtures `cap_causal_*` | Safe declaration-gated (`uses`); Cluster F inheritance closed for its mechanism — **other capability false accepts remain** (CLAIMS open §1); map/struct-field linear-closure residual |
| Non-exportable linear capabilities (shared visitor + store-then-project + interproc container stores + peel-of-param + deep HO linear closures) | **CLAIMED 2026-07-25** | Local mint + export sinks → `ANUBIS_CAPABILITY_EXPORT`; causal spend without token-as-arg OK; `cap_export` peels; **interproc** formals + headers; **closure capture** export-seal; **store-then-project** + **interproc formal-container mutation**; **peel-of-param**; **deep HO linear closures** (named + **list containers** `arr[0](…)`; free Live caps MOVE into binding/container; double apply / use-after-move → `ANUBIS_CAPABILITY_REUSE`; units `linear_closure_*`); dual matrix; `cargo test -p anubis-compiler --lib middle::capability::tests` | Residual: map/struct-field linear-closure containers |
| Keychain / Secure Enclave bind for NE caps (macOS) | **CLAIMED 2026-07-25 (signed Keychain path)** | Soft: `__anubis_cap_ne_soft:…` (`ANUBIS_KEYCHAIN_CAPS=0`); Keychain: `__anubis_cap_ne_kc:…` via Security.framework; SE: `__anubis_cap_ne_se:…` when `ANUBIS_KEYCHAIN_SE=1` and hardware allows; `keychain_se_probe` 0/1/2; **signed path** `compile_sign_and_run_source` (codesign with Apple Development or ad-hoc + safe CLI entitlements, no restricted SE key that AMFI-kills); unit `keychain_se_signed_run_binds_keychain` requires `kc:`/`se:` under Development identity; gate `bash scripts/run_keychain_se_gate.sh`; entitlement derive for packaging profiles | Residual: **App Store / notarized .app packaging**; restricted `com.apple.developer.secure-enclave` provisioning UX (CLI signed path omits that key deliberately); zkVM guest soft-only |
| Native CDCL Unsat RUP certificate | **re-run 2026-07-25:** `cargo test -p anubis-solver lrat` → **16 passed**; `check_proof` required for every `NativeVerdict::Unsat` | Pure independent RUP; division deferred |
| Native solver as compiler **default** (no env) | **CLAIMED 2026-07-25** | `native_authoritative()` default ON; soak `out/native_default_flip_soak_20260725/`; decision `out/native_default_flip_seal_20260725/DECISION.md`; gate PASS post-flip |
| Native authoritative **var×var mul** | **CLAIMED 2026-07-25** | `mulVar_correct` in BitBlast.lean + schoolbook `blast.rs::var_mul` + fragment admits; `run_native_authoritative_gate.sh` PASS |
| Native authoritative **division** (`bvsdiv`/`bvsrem`) | **partial CLAIMED 2026-07-25** | **Const/const** `/` `%` fold to a single `(_ bv… 64)` (native-authoritative, matches wrapping_div); nonneg + power-of-two → proven `bvlshr`/`bvand`; general free/signed non-pow2 still deferred (native declines; z3 may decide) |
| check → confine → run vertical | **re-run 2026-07-25:** `bash scripts/run_check_confine_run_gate.sh` | Net-free showcase; applied confinement + Safe run |
| Evidence bundle + tamper detection | package gate path `scripts/run_package_gate.sh` (seal history); unit evidence/tamper tests | Re-run package gate for live CI claims |
| RISC0 receipt path (in-process) | prove/verify path + A15 gate history; shape + `Receipt::verify` API | Hosted Metal proving **not claimed** |
| Metal parity (local Apple Silicon) | local Tier-2 parity history in A15 / doctor | Not hosted GPU prove |
| Language core (fixtures + repro) | **244/244** on pinned instrument; `scripts/run_language_fixtures.sh` | Seal must set `ANUBIS_BIN` to same binary as security (CLAIMS §7); default is still DEBUG `cargo run` |
| Backend portability / doctor / CLI | `anubis doctor`; DX gate history 15/15 | — |
| Ordinary `anubis run` Safe subset | SPEC_1_0 frozen surface; e.g. hello fixtures; vault contacts `run` EXIT=0 post-PTAH | Research/exploit needs `--allow-research` + VZ where required; **proof/shell constructs are non-run by design** (CLAIMS open §2 (B)); (R) preflight false-rejects **closed**; *check ≠ run for proof/shell* is a named product residual, not a checker gap |
| Phases 0–10 "DONE / At DoD" as total soundness | **not claimed as current** | Historical narrative in `docs/language/ROADMAP.md` | **Named residual:** published reds empty ≠ Class D / D1–D6 closed; green board is not COMPLETE |
| Program-wide mode aggregation + explicit Safe enclaves | **under Command 2026-07-25** + research auth bypass closed 2026-07-27 | Highest privilege wins; bare `@research` **without authorization REJECTS** (`e6ebfd2`). Lean lattice + Rust tests |
| Honest automatic rejection evidence | **under Command 2026-07-25:** `cargo test -p anubis --test safe_mode_program_gate` | Failed `check` auto-emits and `build --evidence` emits artifact-free `FAIL` bundles; PCA tier is `rejected`, not a proof claim |
| Runtime planning (probe) | plan surfaces exist (`runtime-plan`); **plan-only** | Plan-observed exec enforcement **deferred** |
| In-repo package / PCA ecosystem | package gate history; `import` + evidence deps | Public package registry **not claimed** |
| Third-party / multi-party reproduction | Phase 9 witness docs: [`phase9_independent_witness/`](language/phase9_independent_witness/) | Two recorded strangers + hashes; not infinite multi-party |
| DDC toolchain diversity | DDC gate history 34/34 + Phase 9 hashes | Residual: same-author C sources (not TT-total) |
| GitHub hosted witness | `scripts/audit_unified.sh --profile hosted` → 14 host-verifiable gates plus `G9=EXTERNAL`, G14 non-executing host isolation witness, verdict `HOSTED_PASS` | Not a full seal. Only default `audit_a_plus.sh` on the dedicated Tart/VZ runner may claim G9 PoC execution and the full G14 34-check battery |
| A+ front door (2026-07-24 A15 re-seal) | **sealed:** `out/a_plus_a15_frontdoor_20260724-154145/gate_report.json` → pass=15 fail=0 skip=0; G14 VZ **34/34** tart guest | Re-run `bash scripts/audit_a_plus.sh` for a new seal date |
| A+ hostile audit package | **sealed:** `implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md` + STEP_STATUS | Independent of freestanding maturity adjectives |
| Lean formal core | **lead-verified:** `bash scripts/run_formal_gate.sh` → `FORMAL_GATE: PASS`; every theorem machine-checked; no sorry/admit/axiom in core | Lean 4.32.0; no Mathlib; **162 theorems / 15 modules** (comment-stripped) |
| Pure-Anubis formal SAT kernel demo | **re-run 2026-07-25:** `bash scripts/run_formal_kernel_gate.sh` → `FORMAL_KERNEL_GATE: PASS` (kernel + hard tests + independent Python oracle 12/12) | Demo / education surface; not the production native SMT (`solver/`) |
| `http_get` / `http_post` native `run` | **re-run 2026-07-25:** `cargo test -p anubis-compiler http_` → 3 passed | Cleartext TCP; HTTPS via host `curl` (system TLS TCB) |
| VZ slice-2 apply (tart args + applied artifact) | **re-run 2026-07-25:** `bash scripts/run_vz_apply_gate.sh` → `VZ_APPLY_GATE: PASS` | Applied schema separate from sealed `anubis.confinement.v1` |
| VZ apply mount posture fail-closed | **CLAIMED 2026-07-25** | Engagement `--dir` filtered by proven mount posture: `none` → `ANUBIS_APPLY_MOUNT_DENIED`; `read-only` forces `:ro`; unit + gate mount-deny | Residual: live tart boot not required for gate |
| VZ apply network fail-closed (hostname staged) | **CLAIMED 2026-07-25** | Dual of mounts: host-only refuses `--allow-host`/`--allow-open-nat`; `net.send` defaults to host-only (not open NAT); `--allow-host` DNS-pins + records; `--allow-open-nat` explicit residual; gate net-deny | Superseded for Softnet path by row below when softnet on PATH |
| VZ apply Softnet CIDR from DNS-pinned allow-host | **CLAIMED 2026-07-25** | With `softnet` on PATH: `--allow-host` → tart `--net-softnet-block=0.0.0.0/0` + `--net-softnet-allow=<ip>/32`; mode `hostname-softnet`; without softnet → `hostname-policy-staged` host-only fallback; applied field `dns_pin_residual=rebind_after_pin` + HARD RESIDUAL notes; unit `cargo test -p anubis softnet_dns_pin` + `vz_apply` | **HARD residual sealed:** Softnet `/32` is apply-time DNS pin only — post-pin DNS rebind not enforced (not L7). Re-`vz apply` after DNS change. Not Keychain; live tart boot not in gate |
| Effect-derived entitlement / sandbox profile | **CLAIMED 2026-07-25** | `anubis entitlements <file.anb>`; `package::entitlements` derive + seal `entitlement_profile.json` + `program.entitlements` plist; re-derive on PCA verify (`ANUBIS_ENTITLEMENT_DRIFT`); when source uses `cap_acquire_nonexportable`, derives `keychain-access-groups` + `com.apple.developer.secure-enclave` (still `apple_enforced_claim: false`); unit `nonexportable_cap_derives_keychain_and_se_keys` | Residual: OS enforcement only after codesign; path-level sandbox rules **not claimed** |
| Hostname egress policy (DNS pin / deny-all) | **re-run 2026-07-25:** `cargo test -p anubis vz_egress` → pass | Policy compiled; live fd pump at native-boot |
| Hosted Metal prove (local AS + self-hosted job) | **re-run 2026-07-25:** `ANUBIS_REQUIRE_METAL=1 bash scripts/run_metal_prove_gate.sh` → **METAL_PROVE_GATE: PASS** (Gate11 overall_verdict=PASS, metal-hybrid) | Stock GHA still cold-verify; hosted claim needs self-hosted Metal runner labels |
| VZ native-boot + egress pump | **landed** `anubis vz native-boot --kernel …` | Needs signed binary + bootable kernel; pump enforces DNS-pinned policy |
| Author-diversity architecture lane | **re-run 2026-07-25:** `bash scripts/run_author_diversity_gate.sh` → PASS | TT-total **not claimed** (same-human residual) |
| Hosted CI Metal **proving** | **not claimed** | Needs Apple Silicon GPU runners |
| “Production-grade” / industry-ready blanket | **not claimed** as a freestanding stamp | 1.0 freeze is scoped (SPEC_1_0 + showcases); residuals in freeze §5 |
| General-purpose language (all features forever) | **partial** | 1.0 freeze scoped; residuals in SPEC_1_0 §5 |

### Session proof log (2026-07-24)

Recorded under `out/never_oversell_prove_20260724/`:

```text
python3 tools/host_exec_guard.py   # allow exit 0; malware/destructive denylist exit 2
cargo test -p anubis-solver lrat   # 16 passed; 0 failed (re-run 2026-07-25)
bash scripts/run_native_authoritative_gate.sh
  # NATIVE_AUTHORITATIVE cert suite: PASS
  # equivalence 539 files mismatches=0 disagreements=0
  # NATIVE_AUTHORITATIVE_GATE: PASS
bash scripts/run_formal_gate.sh    # FORMAL_GATE: PASS
jq gate_report.json                # pass=15 fail=0 (A15 frontdoor seal on disk)
```

## Independent reproduction (Phase 9)

| Party | Commit | Selfhost | Repro | DDC |
|-------|--------|----------|-------|-----|
| Stranger 1 | `4b19c48` / witness set | 9/9 | 6/6 | 34/34 |
| Stranger 2 | `7c5bf06` | 9/9 | 6/6 | 34/34 |

Agreed hashes (Phase 9 witness date only): binary fixpoint `9030e24b…`, macOS repro `c94fd5b1…`, Linux hermetic `6211f8c9…`, DDC output `3830edc6…`.  
**Post-2026-07-26 registry work deliberately re-baselined the self-host binary; that new host value is unsealed — do not cite it as a seal.** Re-seal under VM before any new public fixpoint claim.  
See [`language/phase9_independent_witness/WITNESS.md`](language/phase9_independent_witness/WITNESS.md) and [`WITNESS_2.md`](language/phase9_independent_witness/WITNESS_2.md).

### Essence spine (identity re-check)

```bash
bash scripts/run_essence_spine_gate.sh          # full (incl. native + formal)
ESSENCE_SPINE_FAST=1 bash scripts/run_essence_spine_gate.sh   # flagships + IFC only
```

**2026-07-25:** secret-PC + secret→public (incl. method formals); **Verified causal capability spend** at privileged effects. Safe = declaration-gated; Verified = live matching-kind token at use.

## Forbidden overclaims

- Freestanding **REAL** / “production-grade” / “fully proven” without a re-runnable command on the same claim
- “Trusting-trust closed” / “backdoor-free”
- “Hosted Metal proving”
- “Public package registry”
- Native solver **default flip residual** (closed 2026-07-25 — default-authoritative; not listed as open)
- Infinite multi-party coverage beyond recorded witnesses
