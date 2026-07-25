# Anubis Claims (1.0 freeze — evidence-first)

See `MATURITY_CLAIM_MATRIX.md` for historical gate rows. Living freeze:
[`docs/language/SPEC_1_0_FREEZE.md`](language/SPEC_1_0_FREEZE.md) ·
[`docs/language/SEMVER_1_0_POLICY.md`](language/SEMVER_1_0_POLICY.md).

**Status vocabulary (2026-07-24):** freestanding **REAL** / production-grade / fully proven
stamps are banned. A claim is either (a) re-runnable with a command + observation, (b) sealed
under a dated artifact path that is not re-run this session, (c) **partial** with the gap named,
or (d) **not claimed**.

## Portable 1.0 surface (grounded)

| Claim | Evidence (command + observation) | Boundary |
|-------|----------------------------------|----------|
| Evidence-native compiler/toolchain | `cargo build -p anubis` (workspace); CI sealed suite on branch | Not a claim about every possible target triple |
| Safe taint enforcement | security fixtures + `cargo test -p anubis-compiler` taint suite; see matrix rows for interproc slices | Reassignment residual **closed** (UNSUPPORTED.md); remaining: field-granular taint + deep escaping-closure forms (fail-closed / documented) |
| Declassification policy | declassify accept/reject fixture pairs under `tests/fixtures` / security fixtures | Lab policy surface, not a full IFC type system |
| Solver correctness (supported int fragment) | **re-run 2026-07-25:** `bash scripts/run_native_authoritative_gate.sh` → `NATIVE_AUTHORITATIVE_GATE: PASS` (cert + corpus ≡ + TCB-drop + opt-out control + fragment danger) | Division deferred; **var×var mul claimed** (see row below); opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0` |
| Wrap-safety VCs (AoRTE-lite) + CEX possible fix | **CLAIMED 2026-07-25; free×free closed 2026-07-25** | On modelable ints: auto wrap-safety for `+`/`-`, **var×const `*`**, and **free×free `*`** via **offline interval product** (no SMT smul hang): bounded factors → prove; unbounded → `ANUBIS_WRAP_RISK` + possible fix; opt-out `ANUBIS_WRAP_SAFETY=0`; unit `cargo test -p anubis-compiler --lib wrap_safety` → 6+; see [`SPARK_VS_ANUBIS.md`](SPARK_VS_ANUBIS.md) | Residual: free `ensures(result == x*y)` posts can still be slow under native-authoritative (separate from wrap-safety); compound factors only offline-proved for simple `bvadd`/`bvsub`/const/var shapes |
| Implicit secret→public (PC) + explicit secret→public (Safe) | **CLAIMED 2026-07-25** | `ANUBIS_IMPLICIT_FLOW` + `ANUBIS_SECRET_TO_PUBLIC` including **method** formals; fixtures `implicit_flow_*` + `secret_to_public_*`; unit `secret_to_public_store_and_return_rejected`; security fixtures **149/149** | Residual: full PC-label join (not Jif-total); untyped formals still interproc |
| Symbolic-index secret-capturing closure application | **CLAIMED 2026-07-25** | `arr[idx](…)` with non-literal `idx` fail-closed when container holds secret/taint-capturing element (j1 twin of `let g = arr[i]`); unit `symbolic_index_secret_capturing_list_application_fails_closed`; clean symbolic still accepts | Residual: full PC-join; untyped formals still interproc |
| Nested container closure application (`outer[0][0]`, `b.fs[i]`, bind + mid-bind) | **CLAIMED 2026-07-25** | Nested Index/FieldAccess CallExpr + **bind** (`let g = outer[i][0]; g(0)`) + **intermediate mid-bind** (`let mid = outer[0]; mid[0](0)` re-keys `field_closures`; symbolic mid union-projects first segments fail-closed); unit `nested_container_closure_application_fails_closed` (apply + bind + mid lit/sym/clean); clean nested still accepts | Residual: full PC-join not claimed |
| if-expr-built containers seed `field_closures` (incl. nested `Stmt::If` + let-inner) | **CLAIMED 2026-07-25** | `collect_container_closures` walks `Expr::If`/`Match`/`Block`; nested bare `if` as `Stmt::If`; unit `nested_container_closure_application_fails_closed` | Residual: full PC-join not claimed |
| `push`/`insert` seed capturing lambdas into `field_closures` (free + method) | **CLAIMED 2026-07-25** | `apply_container_mutation_taint` seeds pushed/inserted lambdas; concrete path miss fail-closes via `any_capturing_field_closure`; free `push(arr, lam)` + method `arr.push(lam)`; unit cases in `nested_container_closure_application_fails_closed` | Residual: full PC-join; HO rebind beyond push/insert seed |
| Verified causal capability spend | **CLAIMED 2026-07-25** | Verified privileged builtins require a **live matching-kind** token at the effect (`cap_acquire("kind")` → effect spends it); wrong kind / no token → `ANUBIS_EFFECT_UNAUTHORIZED`; double-spend → `ANUBIS_CAPABILITY_REUSE`; fixtures `cap_causal_*`; unit `middle::capability::tests::*` | Safe remains declaration-gated (`uses`); interprocedural / HO cap flow residual |
| Non-exportable linear capabilities (shared visitor + store-then-project + push/insert) | **CLAIMED 2026-07-25** | Local mint + export sinks → `ANUBIS_CAPABILITY_EXPORT`; causal spend without token-as-arg OK; `cap_export` peels; **interproc** formals + headers; **closure capture** export-seal; **store-then-project** including **literal store, free/method `push`/`insert`, place-write** (`container_ne`); symbolic index into NE container also EXPORT; dual matrix units; `cargo test -p anubis-compiler --lib capability` (75) | Residual: Keychain/SE hardware isolation **not claimed**; peel-of-param; deep HO rebind beyond sealed container_ne table |
| Native CDCL Unsat RUP certificate | **re-run 2026-07-25:** `cargo test -p anubis-solver lrat` → **16 passed**; `check_proof` required for every `NativeVerdict::Unsat` | Pure independent RUP; division deferred |
| Native solver as compiler **default** (no env) | **CLAIMED 2026-07-25** | `native_authoritative()` default ON; soak `out/native_default_flip_soak_20260725/`; decision `out/native_default_flip_seal_20260725/DECISION.md`; gate PASS post-flip |
| Native authoritative **var×var mul** | **CLAIMED 2026-07-25** | `mulVar_correct` in BitBlast.lean + schoolbook `blast.rs::var_mul` + fragment admits; `run_native_authoritative_gate.sh` PASS |
| Native authoritative **division** (`bvsdiv`/`bvsrem`) | **partial CLAIMED 2026-07-25** | **Const/const** `/` `%` fold to a single `(_ bv… 64)` (native-authoritative, matches wrapping_div); nonneg + power-of-two → proven `bvlshr`/`bvand`; general free/signed non-pow2 still deferred (native declines; z3 may decide) |
| check → confine → run vertical | **re-run 2026-07-25:** `bash scripts/run_check_confine_run_gate.sh` | Net-free showcase; applied confinement + Safe run |
| Evidence bundle + tamper detection | package gate path `scripts/run_package_gate.sh` (seal history); unit evidence/tamper tests | Re-run package gate for live CI claims |
| RISC0 receipt path (in-process) | prove/verify path + A15 gate history; shape + `Receipt::verify` API | Hosted Metal proving **not claimed** |
| Metal parity (local Apple Silicon) | local Tier-2 parity history in A15 / doctor | Not hosted GPU prove |
| Language core (fixtures + repro) | language fixture runner history; Turing-core fixtures | General-purpose “all features forever” **partial** (SPEC §5) |
| Backend portability / doctor / CLI | `anubis doctor`; DX gate history 15/15 | — |
| Ordinary `anubis run` Safe subset | SPEC_1_0 frozen surface; e.g. hello fixtures | Research/exploit needs `--allow-research` + VZ where required |
| Runtime planning (probe) | plan surfaces exist (`runtime-plan`); **plan-only** | Plan-observed exec enforcement **deferred** |
| In-repo package / PCA ecosystem | package gate history; `import` + evidence deps | Public package registry **not claimed** |
| Third-party / multi-party reproduction | Phase 9 witness docs: [`phase9_independent_witness/`](language/phase9_independent_witness/) | Two recorded strangers + hashes; not infinite multi-party |
| DDC toolchain diversity | DDC gate history 34/34 + Phase 9 hashes | Residual: same-author C sources (not TT-total) |
| Hosted CI front door (15-gate, no Metal prove) | `.github/workflows/ci.yml` on `macos-latest` | Metal prove not in hosted CI |
| A+ front door (2026-07-24 A15 re-seal) | **sealed:** `out/a_plus_a15_frontdoor_20260724-154145/gate_report.json` → pass=15 fail=0 skip=0; G14 VZ **34/34** tart guest | Re-run `bash scripts/audit_a_plus.sh` for a new seal date |
| A+ hostile audit package | **sealed:** `implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md` + STEP_STATUS | Independent of freestanding maturity adjectives |
| Lean formal core | **re-run 2026-07-24:** `bash scripts/run_formal_gate.sh` → `FORMAL_GATE: PASS` | Lean 4.32.0; no Mathlib |
| Pure-Anubis formal SAT kernel demo | **re-run 2026-07-25:** `bash scripts/run_formal_kernel_gate.sh` → `FORMAL_KERNEL_GATE: PASS` (kernel + hard tests + independent Python oracle 12/12) | Demo / education surface; not the production native SMT (`solver/`) |
| `http_get` / `http_post` native `run` | **re-run 2026-07-25:** `cargo test -p anubis-compiler http_` → 3 passed | Cleartext TCP; HTTPS via host `curl` (system TLS TCB) |
| VZ slice-2 apply (tart args + applied artifact) | **re-run 2026-07-25:** `bash scripts/run_vz_apply_gate.sh` → `VZ_APPLY_GATE: PASS` | Applied schema separate from sealed `anubis.confinement.v1` |
| VZ apply mount posture fail-closed | **CLAIMED 2026-07-25** | Engagement `--dir` filtered by proven mount posture: `none` → `ANUBIS_APPLY_MOUNT_DENIED`; `read-only` forces `:ro`; unit + gate mount-deny | Residual: live tart boot not required for gate |
| VZ apply network fail-closed (hostname staged) | **CLAIMED 2026-07-25** | Dual of mounts: host-only refuses `--allow-host`/`--allow-open-nat`; `net.send` defaults to host-only (not open NAT); `--allow-host` DNS-pins + records; `--allow-open-nat` explicit residual; gate net-deny | Superseded for Softnet path by row below when softnet on PATH |
| VZ apply Softnet CIDR from DNS-pinned allow-host | **CLAIMED 2026-07-25** | With `softnet` on PATH: `--allow-host` → tart `--net-softnet-block=0.0.0.0/0` + `--net-softnet-allow=<ip>/32`; mode `hostname-softnet`; without softnet → `hostname-policy-staged` host-only fallback; applied field `dns_pin_residual=rebind_after_pin` + HARD RESIDUAL notes; unit `cargo test -p anubis softnet_dns_pin` + `vz_apply` | **HARD residual sealed:** Softnet `/32` is apply-time DNS pin only — post-pin DNS rebind not enforced (not L7). Re-`vz apply` after DNS change. Not Keychain; live tart boot not in gate |
| Effect-derived entitlement / sandbox profile | **CLAIMED 2026-07-25** | `anubis entitlements <file.anb>`; `package::entitlements` derive + seal `entitlement_profile.json` + `program.entitlements` plist; re-derive on PCA verify (`ANUBIS_ENTITLEMENT_DRIFT`); unit `cargo test -p anubis-compiler entitlement` → 7 passed | Residual: OS enforcement only after codesign (`apple_enforced_claim: false`); **Keychain/SE-backed linear caps permanently residual** (language TCB stops at static non-exportable + causal spend — no hardware item bind claimed); path-level sandbox rules **not claimed** |
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

Agreed hashes: binary fixpoint `9030e24b…`, macOS repro `c94fd5b1…`, Linux hermetic `6211f8c9…`, DDC output `3830edc6…`.  
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
