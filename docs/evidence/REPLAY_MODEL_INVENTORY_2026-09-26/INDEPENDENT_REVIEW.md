# Replay inventory: independent applied-diff review

**GO for this test-only integration. HOLD for production replay authority.** No blocking final-diff issue remains in the bounded inventory scaffold. This is not an A-EVID-1 closure or a verified-counterexample result.

Reviewed frozen artifact: `lead-v3-final/applied-staged.patch`, SHA-256 `cc62d25e9d503f66ec6917edbc1cc2f988224aa72388762b1645082fab9ec04d`. Exact base middle SHA-256: `b33ce29df925fd251585568354d14844a0c9ef83102f4e0c51de48c1b08c4ee0`. Applied middle SHA-256: `08b072c4129d876337222ad93d36d47e2944de5abe30c6b8ffa71d97b88b8e54`. Applied `replay_inventory.rs` SHA-256: `fd99112dca06d72d7295acce8caaccca92b217454e8e80489a3af4fe73fee992`.

I read the complete added module, the earlier independent v3 review, and the actual applied diff. File comparison verified that the added module's bytes exactly match the new-file body in the frozen patch. Removing only the added `#[cfg(test)] mod replay_inventory;` declaration from the applied middle file produces the exact base bytes. No production function or consumer changed. The existing production `replay_counterexample` does not call this inventory; the new module is compiled only for tests.

Relative to the earlier independently reviewed v3 candidate, the lead made these bounded corrections:

- The bare `as-array absent` test now expects the bounded grammar's unsupported-symbol refusal. A separate quoted `|absent|` control preserves coverage of a syntactically admitted but missing helper. Neither change makes a missing helper acceptable.
- The atom predicate's equivalent conditional was simplified for strict Clippy; the reserved bare-operator refusal still precedes acceptance as a formal or bit-vector literal.
- The quote reader combines predicates that formerly advanced by the same amount in distinct branches. The predicates have no side effects; the quote/backslash paths and fallthrough cases are preserved.
- A literal replaced an unnecessary format expression, and rustfmt reordered/wrapped source. No value typing or replay operation was introduced.

The inventory still checks declared/model symbol coverage, declared sorts and model arity; rejects duplicate/extra/missing entries; and follows the bounded array-helper dependency graph with missing/cyclic-helper refusals. Quoted application heads cannot enter the bare builtin allowlist. The module retains raw value bodies rather than presenting them as typed, evaluated or independently checked values.

I inspected the lead's completed final gate commands, complete logs and exit files in `lead-v3-final/`, after the final source bytes were frozen:

| Witness | Observed result | Log SHA-256 |
|---|---|---|
| `cargo fmt --all -- --check` | exit `0` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `cargo test --locked -p anubis-compiler --lib replay_inventory` | exit `0`; `8 passed; 0 failed; 0 ignored; 0 measured; 1065 filtered out` | `57ff2d53e9b55af8e8878040e433d761abfb425701d33d30bc8112a5801176fa` |
| `cargo clippy --locked -p anubis-compiler --all-targets -- -D warnings` | exit `0` | `5e509fb5483afeec2035dfbd24d75f58c2f8b66dfd40e418d8f1e0e5fea8e695` |

These are lead-executed local gates that I reviewed, not independent rebuilds. The test and lint commands used the recorded memory-capped user scope. Earlier test/format/lint failures remain in `lead-v3/`; their replacements followed concrete source/test fixes. No earlier passing log is substituted for the final witness. The reviewed test filter is not a whole-workspace test result.

The earlier review's authority limits remain: valid alternate quoted/bare formal spelling is conservatively refused; leading solver warning lines require a separate normalized process-outcome boundary; raw BV/FP/string/array bodies are not typechecked; builtin helper arity is not validated here; no model value is pinned into a new obligation; source-to-formula correctness is not established. Some synthetic inventory fixtures intentionally contain terms that a later semantic checker must reject. Passing these tests does not make those terms valid solver evidence.

Before production use, implement and review typed value parsing/pinning, complete source-bound model validation and honest outcome propagation, including real supported-model positives and malformed/tampered negatives. Preserve a refusal as unresolved/tool failure where appropriate rather than turning inventory acceptance into a disproof. The bounded parser also needs dedicated resource-boundary evidence before a hostile-input robustness claim.

This reviewer performed no git operation, build, compiler execution, shared-source edit, pin publication or SIA change. The final patch is approved only within the test-only boundary recorded above.
