# Named residual table closeout — 2026-07-25T16:05:18Z

**Branch:** `a-plus-maturity/20260705-1649`  
**Pre-fix tip:** `a4dbdc5b2e759a7b8e9d570b7a3ffe00c2a0d0f2`  
**Posture:** residual-honest (closed = fail-closed/proven OR HARD permanent residual named)

## Verdict table

| Residual | Disposition | Evidence this stamp | Still residual? |
|----------|-------------|---------------------|-----------------|
| free×free × wrap-safety | **CLOSED** (offline interval product; never SMT smul hang) | `r1_wrap_safety.txt` — 6/6 ok incl. `wrap_safety_free_mul_fail_closed_offline` + bounded prove | Free `ensures(result == x*y)` posts can still be slow (separate from wrap-safety); complex factor shapes offline → Risk fail-closed |
| Softnet DNS rebind | **HARD residual sealed** (not closed as L7) | `r2_softnet_dns_pin.txt` — `softnet_dns_pin_records_hard_rebind_residual` ok; applied field `dns_pin_residual=rebind_after_pin` | Yes: pin is apply-time only; re-`vz apply` after DNS change. Policy: document only, no silent re-resolve |
| Keychain / SE linear caps | **PERMANENT residual** | `r3_entitlement.txt` — `no_apple_enforced_claim_on_any_key` ok (7/7); `r3_capability.txt` — 35/35 static non-exportable + causal spend | Yes forever under language TCB: `apple_enforced_claim: false`; no hardware item bind claimed |
| Loop / complex wrap paths | **Showcase closed** (clamp-first / requires) | `r4_nexus_*.txt`, `r4_vz_confine.txt` — all `check passed` | More programs may trip WRAP_RISK; pattern is clamp-first as they red (not a silent prove) |

## Adjacent IFC slice sealed this stamp (not in original table)

| Item | Disposition | Evidence |
|------|-------------|----------|
| Intermediate mid-bind `let mid = outer[0]; mid[0](0)` | **CLOSED** fail-closed | `mid_bind_nested.txt` — unit includes mid lit/sym/clean; `project_field_closures` + symbolic first-segment union |
| Flat symbolic index non-regression | **CLOSED** | `symbolic_index.txt` ok |

## Residual that remains (honest)

- if-expr-built containers may not seed `field_closures` (named fail-open under-report in CLAIMS / SPEC §5)
- Softnet rebind after pin (HARD)
- Keychain/SE hardware bind (permanent)
- Free `ensures(x*y)` solver slowness under native-authoritative

## Re-run

```bash
cargo test -p anubis-compiler --lib wrap_safety
cargo test -p anubis softnet_dns_pin
cargo test -p anubis-compiler entitlement
cargo test -p anubis-compiler capability
cargo test -p anubis-compiler --lib nested_container_closure_application_fails_closed
./target/debug/anubis check examples/showcase/nexus/nexus_cognitive_kernel.anb
./target/debug/anubis check examples/showcase/nexus/nexus_checker_security.anb
./target/debug/anubis check examples/showcase/vz_confine_demo.anb
```
