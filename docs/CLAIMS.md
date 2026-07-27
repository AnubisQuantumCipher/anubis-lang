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

**The roadmap claim that phases 0–10 are DONE / At DoD is FALSE as a current soundness claim.**
The language's promise — *`anubis check` passing means the program cannot violate its stated
contracts, effects, capabilities, or information-flow policy at runtime* — is still violated by
the **three known-red witnesses** below (one multi-candidate mechanism + one interproc factory
summary). Fixture green elsewhere does **not** empty that residual.

### The disease (read this first) — most honest thing we can say about this project

> **Every class closed in this arc was the same disease: a user writes something down, or a
> producer computes a label, and a consumer ignores it or recomputes it independently.**

Declared returns, R1 field types, (R)/PCA re-derivation, M1 stored-callables, M2 multi-candidate
denotation, M3 nested discharge — all instances of that sentence. Prefer making the correct
binding reachable over adding a parallel consumer.

It is **not** an open-ended unknown. Live security reds are a **bounded, named residual**
(live-enumerated 2026-07-27 round 6): **one mechanism** (multi-candidate closure denotation)
**plus one interprocedural factory summary** — **not three unknown bugs**.

| Class | Disease form | Status |
|---|---|---|
| Declared `-> secret/tainted` returns (named/aliased) | Consumer ignored declaration | **CLOSED** `6fb055f` |
| Declared struct field secret/tainted (R1) | Consumer ignored field type | **CLOSED** `f4d2f37` |
| (R) preflight + PCA twin | Consumer re-derived taint policy | **CLOSED** PTAH `6fb055f` |
| M3 value-block stmt `if` contract discharge | Consumer never walked nested stmt | **CLOSED** `2168bf1` |
| M2-A branch-expression forwarder registration | Consumer only saw statement `return` | **CLOSED** `ae1fa17` |
| M2 alias normalize (`let a = f; return a`) | Identity-let alias ignored at registration | **CLOSED** `fb45278` |
| M1 push / for / named-fn-in-container | Apply site lost stored callable identity | **CLOSED** `47ab408` / `ed24cba` / `07eb1db` |
| Stdlib silent wrong values | Producer returned wrong value | **CLOSED** `bdfd21f` — **32/32** |
| **Multi-candidate closure denotation** (If/Match bind → apply) | Expression denotes more than one lambda | **OPEN** — 2 witnesses |
| **Interprocedural factory field-callable summary** | Factory return does not seed field-callable denotation | **OPEN** — 1 witness |

**Headline:** reds went **11 → 3**. The project knows the shape of what remains.

### Currently green (re-stamped 2026-07-27 GROK-MAAT round 6 — not a soundness completion claim)

**Commit tip of this stamp:** `07eb1db` (M1 push) atop M1 for/named-fn, M2 alias, M2-A, M3, R1,
stdlib, declared-return/PTAH. Live instrument: `./target/release/anubis` (mtime 2026-07-27 00:58).

| Surface | Observation | Repro / boundary |
|---|---|---|
| **Language core** | **244/244 PASS** | pin `ANUBIS_BIN` (§6) |
| **Security fixtures** | Disk **219** `.anb`. **216 PASS / 3 FAIL** | Honest residual inventory — **not rot** |
| **Stdlib fail-closed** | **32/32 PASS** | `ANUBIS_BIN=./target/release/anubis bash scripts/run_stdlib_failclosed_gate.sh --out out/…` |
| **Formal gate** | **PASS** — every theorem machine-checked; **no `sorry` / `admit` / free `axiom`** | `bash scripts/run_formal_gate.sh` → `FORMAL_GATE: PASS`; Lean **162 theorems / 15 modules** (comment-stripped) |
| **Native authoritative** | **PASS over 681 files, 0 mismatches** | `bash scripts/run_native_authoritative_gate.sh` (lead-verified) |
| **Taint / type / effect selfhost** | **All PASS, 0 disagreements** | lead-verified differentials |
| Capset selfhost | **Known FAIL** on `c05_open_param_call` | See §3 — pre-existing; root-cause in flight (GROK-HORUS). **Do not hide.** |
| Ordinary Safe `run` | Vault contacts EXIT=0 post-PTAH | Proof/shell non-run by design (§2 B) |
| VM seal of post-registry fixpoint | **Pending** | Do not publish host fixpoint as sealed |

#### Honest-number methodology (read before quoting 216/219)

**`216/219` is an honest published defect inventory, not a failing product grade.**

The corpus **deliberately contains red witness fixtures**. The **3** failures are a **published
residual list** for **one multi-candidate mechanism + one factory interproc summary** (§1), not
mystery rot and not three unknown bugs. A reader who sees 216/219 without this frame assumes
decay; with it, they see a nearly-empty bounded map.

- **Do not** rewrite as "≈99% secure" or claim Safe soundness total.
- **Do not** quote older stamps (207/215, 203/214, 199/210, …) as current.
- **Do** re-enumerate after any checker change.
- **Do** seal with one pinned `ANUBIS_BIN` (§6).

Counting rules: **Lean = 162 / 15**. **Builtins ≈ 213** (five-function union).

### Open — load-bearing (blocks honest completion)

1. **Three known-red security witnesses = one mechanism + one interproc summary (completion residual).**  

   Not three independent mysteries. Live on release binary (this pass):

   #### Mechanism A — multi-candidate closure denotation (**2 witnesses**)

   **Product question:** when `let f = if … { λ₁ } else { λ₂ }` (or `match`), what does `f`
   denote at apply? (Set/union denotation at bind; charge all candidates at apply, with
   documented over-reject if needed.)

   | Witness | Sub-hole |
   |---|---|
   | `if_expr_write_closure_apply_rejects.anb` | If-expr multi-lambda bind then apply (H18) |
   | `match_expr_write_closure_apply_rejects.anb` | Match-expr multi-lambda bind then apply (H18) |

   #### Mechanism B — interprocedural factory field-callable summary (**1 witness**)

   **Product question:** when a factory free-fn returns a struct whose field is a write/secret
   closure, does the **caller** still know what that field denotes?

   | Witness | Sub-hole |
   |---|---|
   | `factory_struct_field_write_closure_rejects.anb` | Return/param summary empties field-callable denotation |

   **Corpus: 216/219.** Re-enumerate:
   ```bash
   for f in examples/security/*_rejects.anb; do
     ./target/release/anubis check "$f" >/dev/null 2>&1 && echo "RED $f"
   done
   # expect 3 lines; green = total − 3 → 216 when total is 219
   find examples/security -name '*.anb' | wc -l   # 219
   ```

2. **check/run divergence — (R) CLOSED; (B) residual named.**  
   **(A)=0, (B)=7, (R)=3**; (R)+PCA **CLOSED** (PTAH). **(B)** non-run by design.

3. **Capset selfhost gate — known FAIL (document, do not hide).**  
   `scripts/run_capset_selfhost_gate.sh` fails on
   `tests/fixtures/capset_selfhost/c05_open_param_call.anb`. **Verified pre-existing** relative
   to this arc's security closes; **not** introduced by M1/M2/R1. Root-cause analysis **in
   flight with GROK-HORUS**. Do not grade the language "capset selfhost green" until this row
   is closed or explicitly scoped. Self-host **taint / type / effect** engines remain
   **0 disagreements**.

4. **Self-host registry — HOST-FIXED; VM seal pending.**  
   Do not publish post-drift host fixpoint as sealed.

### Open — boundary honesty / process (not silent overclaims)

5. **VZ isolation is SAFETY, not SECURITY** — host-forgeable markers; operator is trust root.  
6. **Research / Safe seam** — dual-use by design, not Safe escape.  
7. **Harness integrity + instrument fact:** language fixtures defaulted **DEBUG** while security
   graded **RELEASE**. Both accept **`ANUBIS_BIN`**. Seals must pin one binary. Quoting
   244/244 and 216/219 from mixed instruments is fabrication.

### Resolved this arc (do not re-open without new evidence)

8. ~~Self-host harness / research host ban~~ **RESOLVED** (`14f5e14`).  
9. ~~P0 `var×var` solver hang~~ **RESOLVED**.  
10. ~~Tier 1 H2–H5-unanimity + W8 total~~ **RESOLVED**.  
11. ~~Declared returns named/aliased~~ **RESOLVED** `6fb055f`.  
12. ~~(R) + PCA twin~~ **RESOLVED** PTAH.  
13. ~~R1 declared field secret/tainted~~ **RESOLVED** `f4d2f37`.  
14. ~~Stdlib 27-builtin silent wrong~~ **RESOLVED** `bdfd21f` — **32/32**.  
15. ~~M3 value-block nested stmt discharge~~ **RESOLVED** `2168bf1`.  
16. ~~M2-A branch-expression forwarder~~ **RESOLVED** `ae1fa17`.  
17. ~~M2 identity-let alias forwarder registration~~ **RESOLVED** `fb45278`.  
18. ~~M1 named-fn-in-container / for-loop seed / push seed~~ **RESOLVED**
    `47ab408` / `ed24cba` / `07eb1db`.

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
| Safe taint enforcement | security + taint/type/effect selfhost **0 disagreements**; R1 + declared returns + M1 bulk | **PARTIAL** — remaining FA = **multi-candidate + factory summary** only (**3 reds / 216/219**). Stdlib 32/32 |
| Declassification policy | declassify accept/reject fixture pairs under `tests/fixtures` / security fixtures | Lab policy surface, not a full IFC type system; shell declassify accept is check-policy only (`run` non-run by design — CLAIMS open §2) |
| Solver correctness (supported int fragment) | **lead-verified:** `bash scripts/run_native_authoritative_gate.sh` → **PASS, 681 files, 0 mismatches** | Division deferred; var×var mul claimed; opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0` |
| Wrap-safety VCs (AoRTE-lite) + CEX possible fix | **CLAIMED 2026-07-25; free×free closed 2026-07-25** | On modelable ints: auto wrap-safety for `+`/`-`, **var×const `*`**, and **free×free `*`** via **offline interval product** (no SMT smul hang): bounded factors → prove; unbounded → `ANUBIS_WRAP_RISK` + possible fix; opt-out `ANUBIS_WRAP_SAFETY=0`; unit `cargo test -p anubis-compiler --lib wrap_safety` → 6+; see [`SPARK_VS_ANUBIS.md`](SPARK_VS_ANUBIS.md) | Residual: free `ensures(result == x*y)` posts can still be slow under native-authoritative (separate from wrap-safety); compound factors only offline-proved for simple `bvadd`/`bvsub`/const/var shapes |
| Implicit secret→public (PC) + explicit secret→public (Safe) | **CLAIMED 2026-07-25 for cited fixtures; PARTIAL as total IFC** | Method formals + declared returns + R1 + M1 bulk; **live security 216/219** / **3 known-red** | Residual: full PC-join; **multi-candidate If/Match apply + factory field-callable summary** only |
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
| Phases 0–10 "DONE / At DoD" as total soundness | **not claimed as current** | Historical phase narrative in `docs/language/ROADMAP.md`; living STATUS banner + phase table State column rewritten 2026-07-26 pass 3 to refuse present-tense COMPLETE | **Named residual:** open false-accept class + walker parity (CLAIMS open §1) contradict any freestanding COMPLETE stamp |
| Program-wide mode aggregation + explicit Safe enclaves | **under Command 2026-07-25:** `cargo test -p anubis --test safe_mode_program_gate` plus CLI `program_mode_` units; Lean lattice in `formal/Anubis/ModeAggregation.lean` | Highest privilege wins across source order/modules/impls; explicit `@safe` stays Safe. Lean proves the abstract lattice, while Rust tests cover traversal correspondence |
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
