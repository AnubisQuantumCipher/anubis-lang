# GATE HARNESS INTEGRITY — round 1 (task #22)

**Date:** 2026-07-28 · **By:** lead (w19:p1) · **Scope:** all 48 `scripts/run_*.sh` + 19 support
gates and libraries they call = **67 scripts, 11,605 lines**.

**Method:** 13 parallel read-only auditors, one per bucket of ~900 lines, each answering the five
integrity questions with `path:LINE` + verbatim quote; every red claim then handed to an independent
adversarial verifier instructed to REFUTE and to default to refuted. 46 red claims verified:
**35 CONFIRMED, 11 REFUTED.** The lead then re-verified the headline findings firsthand.

**Bottom line: the answer to "can a gate report PASS while testing nothing" is YES, and one of them
is doing it right now, on the real board, with no flag set.**

---

## 0. The claim this audit was testing

`docs/CLAIMS.md` says:

> **Harness integrity + instrument fact — CLOSED 2026-07-27.** ... a fixture with no `EXPECT:`
> header was graded expected-to-pass ... and the seal itself printed **SEAL_PASS** with two required
> gates SKIPPED ... Both are closed with microbenches that show each guard FIRING rather than merely
> present.

Those two specific defects are genuinely closed and I confirmed the mechanism (see §4). **The CLASS
is not closed.** The item should be reworded from CLOSED to a named residual.

---

## 1. Ranked red list — LEAD-VERIFIED (I ran these myself)

### R1 — `run_docs_drift_gate.sh` prints PASS having checked ZERO stamps. **DEMONSTRATED.**

The gate's final verdict reads only the failure counter. Coverage is never asserted:

```
scripts/run_docs_drift_gate.sh:344   if [[ "$FAILS" -eq 0 ]]; then
scripts/run_docs_drift_gate.sh:345     echo "DOCS_DRIFT_GATE: PASS"
scripts/run_docs_drift_gate.sh:346     echo "Overall: PASS ($STAMPS_CHECKED stamps checked, 0 drift)"
```

`$STAMPS_CHECKED` is interpolated into the headline and never compared to anything.

**Counterexample, run against the real gate:**

```
$ mkdir -p scratchpad/fleet_20260726/w19/empty_docs
$ bash scripts/run_docs_drift_gate.sh --scan-root scratchpad/fleet_20260726/w19/empty_docs \
      --out scratchpad/fleet_20260726/w19/out_empty
stamps_checked=0 claim_guards_checked=0 scan_fails=0
DOCS_DRIFT_GATE: PASS
Overall: PASS (0 stamps checked, 0 drift)
GATE_EXIT_CODE=0
```

A gate that checked **zero stamps and zero claim guards** exits 0 and prints its PASS token.

**And this is not flag-only.** The scanner iterates a hardcoded list of 15 owned docs and skips
missing ones with no counter:

```
scripts/lib/docs_drift_scan.py:163   for rel in LIVE_FILES:
scripts/lib/docs_drift_scan.py:164       path = root / rel
scripts/lib/docs_drift_scan.py:165       if not path.is_file():
scripts/lib/docs_drift_scan.py:166           continue
```

Second counterexample — real docs, renamed, nothing else changed:

```
$ cp docs/CLAIMS.md .../renamed_docs/docs/CLAIMS_RENAMED.md
$ bash scripts/run_docs_drift_gate.sh --scan-root .../renamed_docs --out .../out_renamed
stamps_checked=0 claim_guards_checked=0 scan_fails=0
DOCS_DRIFT_GATE: PASS
Overall: PASS (0 stamps checked, 0 drift)
```

**LIVE INSTANCE ON THIS REPO RIGHT NOW.** Two of the 15 declared owned-live docs do not exist and
are being silently skipped every run:

```
MISSING: SPEC_1_0_FREEZE.md
MISSING: TUTORIAL.md
```

So the `33 stamps` on the inherited board and the `35 stamps` I committed today are both numbers
that silently exclude 2/15 of the gate's own declared corpus. The gate cannot distinguish
*"scanned 15 files and all agreed"* from *"scanned nothing"*.

**Impact chain:** the seal consumes this gate by matching its PASS token —
`scripts/run_seal_checklist.sh:741  '^DOCS_DRIFT_GATE: PASS\b'` — so a docs rename propagates a
vacuous green into `SEAL_PASS`.

**Severity: HIGH.** Reachable by an ordinary file rename, no flag, no env var.

---

### R2 — `run_shadow_diff.sh` is vacuous BY CONSTRUCTION today. **VERIFIED.**

It harvests `ANUBIS_SHADOW:` lines from stderr and passes when it finds no disagreement:

```
scripts/run_shadow_diff.sh:69   ANUBIS_SHADOW_TYPES=1 timeout 3600 "$BIN" check "$f" >/dev/null 2>"$err" || true
scripts/run_shadow_diff.sh:72   while IFS= read -r _l; do ... done < <(grep -E '^ANUBIS_SHADOW: ' "$err" ...)
```

There are **zero** call sites in the compiler that emit that prefix:

```
$ grep -rn 'ANUBIS_SHADOW' compiler/src/ --include=*.rs | grep -c 'eprintln\|println'
0
```

Every run harvests an empty set and prints PASS. This gate has no failing input. **Severity: HIGH**
— it is green today for the same reason an empty corpus is green.

---

### R3 — `run_offensive_platform_gate.sh`: one of the "34/34" cannot fail. **VERIFIED.**

```
scripts/run_offensive_platform_gate.sh:423   if [[ -S "$eng/aop.sock" ]] || grep -q 'uds listener' "$out/listen.log"; then
scripts/run_offensive_platform_gate.sh:424     record "t3_uds" "PASS" "uds transport"
scripts/run_offensive_platform_gate.sh:425   else
scripts/run_offensive_platform_gate.sh:426     record "t3_uds" "PASS" "uds configured (listener lifecycle ended)"
scripts/run_offensive_platform_gate.sh:427   fi
```

PASS in both branches. The headline `34/34` verifies at most 33 things.

**Scoped honestly:** I swept the whole file for this shape and `t3_uds` is the ONLY instance — the
immediately adjacent check is its own control (`:431 record "t3_dns" "FAIL" "no dns"`). This is one
vacuous check, not a sham gate.

**Severity: HIGH** (it is an isolation-lane check, and it is the one that cannot fail).

**Partly REFUTED — recorded because the auditor overreached.** The same finding claimed
`ANUBIS_OFFENSIVE_GATE_IN_GUEST=1` on a bare host yields a forged
`isolation=tart-disposable-guest`. There IS a guard (`:727-731` stamps `isolation="host-misuse"`
when the var is absent), and host-forgeable isolation markers are ALREADY a documented residual
(CLAIMS boundary item 4: *"VZ isolation is SAFETY, not SECURITY — host-forgeable markers"*). I am
dropping that sub-claim as not-novel. The **debug-binary fallback at `:279`** is real and separate.

---

### R4 — `run_keychain_se_gate.sh` validates by substring; it passes today by luck. **VERIFIED.**

```
scripts/run_keychain_se_gate.sh:14  cargo test -p anubis-compiler --lib package::entitlements::tests::nonexportable_cap_derives_keychain --quiet
scripts/run_keychain_se_gate.sh:17  cargo test -p anubis-compiler --lib middle::capability::tests::nonexportable_token_as_print --quiet
```

libtest filters are substring matches, and a filter matching zero tests **exits 0**
(`0 passed; 0 failed; N filtered out`). Neither call passes `--exact`, and nothing asserts a nonzero
test count. The filters are PREFIXES of the real names:

```
compiler/src/package/entitlements.rs:464   fn nonexportable_cap_derives_keychain_and_se_keys()
compiler/src/middle/capability.rs:3172     fn nonexportable_token_as_print_arg_is_export()
```

Rename either test's prefix and the gate runs **nothing** and reports PASS. **Severity: HIGH.**
Same defect confirmed independently in `run_vz_apply_gate.sh` and three checks of `run_dx_gate.sh`,
which grade on the string `test result: ok` — printed for a zero-match filter too.

---

## 2. CONFIRMED by the audit, NOT yet lead-verified

Reported as agent-confirmed. I have not personally reproduced these; treat accordingly.

| gate | condition | sev |
|---|---|---|
| `run_seal_checklist.sh` | SEAL_PASS against a stale manually-published pin; no freshness check | HIGH |
| `run_native_shadow_gate.sh` | `out/` is gitignored and never created ⇒ stderr redirect fails on fresh checkout ⇒ every `check` silently never runs | HIGH |
| `run_for_in_gate.sh`, `run_lang_trio_gate.sh` | hardcoded `/Users/sicarii/Desktop/metal-hybrid-prover`; absent ⇒ prove/verify leg silently skipped, still PASS | HIGH |
| `run_essence_spine_gate.sh` | `ESSENCE_SPINE_FAST=1` skips the native-authoritative pillar, still prints "N pillars green" | HIGH |
| `run_selfhost_repro_gate.sh` | grades a stale `stage2.rs` predating current sources, no freshness check | HIGH |
| `run_stdlib_gate.sh` | `poc_kit/bin/` gitignored ⇒ `std.pwn` lane skips without incrementing any counter | HIGH |
| `build_release_candidate.sh` | prints "Final Verdict: PASS" with Gate 5 a no-op placeholder | HIGH |
| `run_capset_selfhost_gate.sh` | builtin-drift sub-check PASSes on an empty candidate list if `run.rs` moves | HIGH |
| `gate10_a15_reproduce.sh` | treats infrastructure failure of `verify_bundle.sh` as successful tamper-detection | HIGH |
| `check_gate_common_adoption.sh` | candidate detection is a grep heuristic; 11 `.anb`-referencing scorers are invisible to it | MED |
| 9 more | `run_check_run_parity`, `run_walker_completeness`, `run_poc_kit`, `check_metal_parity`, `run_nexus`, `run_metal_prove`, `run_author_diversity`, `run_selfhost_ddc`, `run_package` | MED/LOW |

**Instrument provenance is the single most common defect.** Nine gates grade whatever sits at
`target/release/anubis` with no freshness or digest check, ignoring the content-addressed pin
mechanism that exists precisely to prevent this: `run_security_fixtures`, `run_language_fixtures`,
`run_power_gate`, `run_parameterized_proof_gate`, `run_named_journal_gate`,
`run_capset_corpus_failclosed`, `run_selfhost_fulllang_gate`, `run_formal_kernel_gate`,
`run_metal_prove_gate`.

**Deferred, not dropped** — 9 gates had red claims that exceeded this round's verify budget and were
logged rather than silently truncated: `verify_bundle.sh`, `run_multi_field_journal_gate.sh`,
`run_proof_binding_gate.sh`, `run_enum_match_gate.sh`, `repro_language_core.sh`,
`run_check_confine_run_gate.sh`, `check_evidence_schema.sh`, `run_prove_gate.sh`,
`run_native_authoritative_gate.sh`, `check_declaration_seam.sh`.

---

## 3. Answers to the five questions, across 67 scripts

| Q | finding |
|---|---|
| **Q1 vacuous PASS** | **YES, live.** R1 demonstrated; R2 vacuous by construction. `gate_common`'s `require_nonempty_corpus`/`finalize` are correct and *do* protect the 13 gates that call them — but 35 gates call neither. |
| **Q2 exit code** | Largely sound. `rc=$?` is correctly placed in every gate I checked; the one `$?`-after-pipeline is `smoke_host_exec_guard.sh:12`, harmless there. The real Q2 defect is different: **`test result: ok` and libtest exit 0 accepted as proof a test ran** (R4). |
| **Q3 binary provenance** | **Weakest area.** 9 gates grade an unpinned binary with no freshness check; `run_security_fixtures.sh:34-43` silently cascades release→debug→`cargo run`. |
| **Q4 missing tool** | Mixed. `tart`/`z3`/`python3` mostly fail closed. Hardcoded absent paths and unset env vars degrade to skip-and-pass (R1, `run_for_in_gate`, `run_essence_spine_gate`). |
| **Q5 skip accounting** | **Systematically invisible.** `34/34`, `N pillars green`, `stdlib gate: PASS (N pass, 0 fail)` all omit a skip counter. A skip is booked as a pass in R3. |

---

## 4. The negative result — what IS sound, and why

`scripts/lib/gate_common.sh` is well built and I want that recorded:

- `parse_expectation` (`:42-72`) reads `// EXPECT:` **only from the leading comment block**, rejects
  symlinks, empty, unreadable, missing, duplicate and conflicting headers, and binds `_accepts`/
  `_rejects` naming to the header. The headerless-fixture defect is genuinely dead.
- `require_nonempty_corpus` (`:133`) refuses count 0 and rejects non-canonical integers.
- `finalize` (`:157`) refuses on empty corpus, on counters that do not sum to total, and returns
  INCOMPLETE rather than PASS.

This is the right abstraction. **The defect is adoption, not design** — 13 of 48 `run_*.sh` call it.

---

## 5. Proposed diffs — NOT applied (matrix before patches, per the brief)

```diff
--- a/scripts/run_docs_drift_gate.sh
+++ b/scripts/run_docs_drift_gate.sh
@@ -343,6 +343,14 @@
+# A gate over an empty corpus must FAIL. Coverage is part of the verdict, not decoration.
+if [[ "$STAMPS_CHECKED" -eq 0 || "$CLAIM_GUARDS_CHECKED" -eq 0 ]]; then
+  echo "DOCS_DRIFT_GATE: FAIL"
+  echo "Overall: FAIL (vacuous: stamps=$STAMPS_CHECKED guards=$CLAIM_GUARDS_CHECKED)" >&2
+  exit 1
+fi
+if [[ "$SCAN_RC" -ne 0 ]]; then          # captured at :113 and currently never read
+  echo "DOCS_DRIFT_GATE: FAIL"
+  echo "Overall: FAIL (scanner exited $SCAN_RC)" >&2
+  exit 1
+fi
 if [[ "$FAILS" -eq 0 ]]; then
```

```diff
--- a/scripts/lib/docs_drift_scan.py
+++ b/scripts/lib/docs_drift_scan.py
@@ -163,7 +163,9 @@
     for rel in LIVE_FILES:
         path = root / rel
         if not path.is_file():
-            continue
+            missing.append(rel)      # a doc this gate CLAIMS to own has moved or gone
+            continue
```
…and fail the gate when `missing` is non-empty and `root` is the repo root.

```diff
--- a/scripts/run_offensive_platform_gate.sh
+++ b/scripts/run_offensive_platform_gate.sh
@@ -423,7 +423,7 @@
   if [[ -S "$eng/aop.sock" ]] || grep -q 'uds listener' "$out/listen.log"; then
     record "t3_uds" "PASS" "uds transport"
   else
-    record "t3_uds" "PASS" "uds configured (listener lifecycle ended)"
+    record "t3_uds" "FAIL" "no uds socket and no 'uds listener' in listen.log"
   fi
```

```diff
--- a/scripts/run_keychain_se_gate.sh
+++ b/scripts/run_keychain_se_gate.sh
@@ -14 +14 @@
-cargo test -p anubis-compiler --lib package::entitlements::tests::nonexportable_cap_derives_keychain --quiet
+cargo test -p anubis-compiler --lib package::entitlements::tests::nonexportable_cap_derives_keychain_and_se_keys --exact --quiet
```
`--exact` everywhere, plus assert `1 passed` rather than accepting `test result: ok`.

**Systemic fix, worth more than all four:** a shared `assert_tested N` in `gate_common.sh` that every
gate calls with its own coverage counter, and an adoption check that fails the seal for any
`run_*.sh` that prints a PASS token without calling it. R1 and R2 are the same bug in two gates;
patching them individually is how this class survived to round 1 of this audit.

---

## 6. What this means for the board

The six numbers I measured today on pin `anubis-cf98ccebb4c1` — security 311/311, language 244/244,
compiler lib 745/745, anubis 200/200, stdlib 104/104 — come from gates whose **corpus and scoring
logic are fail-closed** (all use `gate_common`). Those numbers stand.

What does **not** stand is the docs-drift stamp count, and the general claim that a green board means
every gate tested something. `docs/CLAIMS.md` should move "Harness integrity — CLOSED" to a named
residual citing R1–R4.
