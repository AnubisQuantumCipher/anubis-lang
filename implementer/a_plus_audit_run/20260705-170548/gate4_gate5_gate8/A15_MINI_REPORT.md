# A15 Mini Audit - Gates 4,5,8

Stamp: 20260705-170548

## Gate 4: Safe Tainted Sink
- Fixture: examples/taint_reject.anb (taint_source + sink)
- Check: FAIL with "safe mode tainted flow from `password` to sink `sink` requires declassify() or research boundary"
- No "research lowering requires assume" error
- Bundle: FAIL verdict, taint-traces, sarif with ANUBIS_TAINTED_SINK..., source copy, etc.
- No native artifact emitted for check path
- Verdict: YES

## Gate 5: Declass Policy
- Missing: FAIL (via sink flow after bare declass)
- Pass: PASS on check, artifact on build
- Traces include declassified true + policy in steps when provided
- Verdict: YES (partial on exact bare declass message, but enforcement works)

## Gate 8: Evidence Schema
- Bundles contain required files + manifest.json
- verify_bundle and check_schema PASS
- Tamper test: detects hash mismatch
- Verdict: YES

## SARIF
- Has ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY etc in results
- Verdict: PARTIAL (locations basic, but ruleIds and messages correct)

## Old Gate Avoided
- Safe taint paths no longer hit assume bound requirement (scoped is_research)

All commands run fresh, artifacts copied.

A15 reproduction: COMPLETE for this slice.
