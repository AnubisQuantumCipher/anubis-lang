# Anubis Language — 1.0 Frozen Surface

**Freeze date:** 2026-07-22  
**Sealed commit at freeze:** see git tag intent `v1.0.0` on branch `a-plus-maturity/20260705-1649`  
**Normative companions:** [`SPEC.md`](SPEC.md), [`LANGUAGE.md`](../../LANGUAGE.md), [`SEMVER_1_0_POLICY.md`](SEMVER_1_0_POLICY.md)

This document freezes the **production 1.0 claim surface** — what you may treat as stable.
Anything not listed here is experimental / research unless later promoted by MINOR.

## 1. Toolchain commands (stable)

| Command | 1.0 guarantee |
|---------|----------------|
| `anubis check` | Safe mode types, taint, secrets, effects, contracts (SMT) |
| `anubis check --verified` | Linear capability authorization for privileged effects |
| `anubis run` | Native execution of the Safe run subset + crypto builtins |
| `anubis build` / `build --evidence` | Fail-closed on unproven contracts; PCA bundle |
| `anubis verify` / `report` | Re-derive claims; tamper detection |
| `anubis package` / lock / verify | Dependency trust surface (see package gate) |
| `anubis vz confine` | Confinement manifest from proven effects |
| `anubis doctor` / `fmt` / LSP / `repl` / `doc` | DX gate surfaces |
| `anubis prove --backend risc0` | ZK path (receipt verify API); performance not frozen |

## 2. Language surface (stable)

- Program structure: `fn`, `let`/`let mut`, `struct`, `enum`, `match`, `if`/`while`/`for`/`loop`, closures, traits/impl, generics (runtime-checked), `Option`/`Result`/`?`
- Types: integers (`u8`/`u16`/`u32`/`u64`/int spelling), `bool`, `f64`, strings, lists, maps, `secret<T>` / `tainted<T>` qualifiers
- Effects: `uses(...)` with `fs.read`/`fs.write`/`net.send`/`shell`/`time.now`/`rand.gen`
- Capabilities: `cap_acquire` / `cap_use` (check + run)
- Contracts: `requires` / `ensures` / `invariant` / `assert` / `assume`
- Information flow: taint sinks, secret exfiltration, lethal trifecta (verified lane)
- I/O builtins: `read_file`/`write_file`/`append_file`/`delete_file`/`open`/`args`/`env`/print family
- Crypto builtins: as in [`CRYPTO.md`](CRYPTO.md) (Argon2id, AEAD, HMAC, HKDF, Ed25519, …)
- Modules: multi-file `import` + embedded `import std.*`

## 3. Trust spine (stable gates)

| Gate | Script | 1.0 status |
|------|--------|------------|
| Self-host fixpoint | `scripts/run_selfhost_gate.sh` | PASS 9/9 |
| External repro | `scripts/run_selfhost_repro_gate.sh` | PASS 6/6 (Docker) |
| DDC | `scripts/run_selfhost_ddc_gate.sh` | PASS 34/34 |
| Formal | `scripts/run_formal_gate.sh` | PASS (no sorry/admit/axiom) |
| Language fixtures | `scripts/run_language_fixtures.sh` | PASS |
| Package | `scripts/run_package_gate.sh` | PASS 9/9 |
| DX | `scripts/run_dx_gate.sh` | PASS 15/15 |
| Independent strangers | `docs/language/phase9_independent_witness/` | 2 parties, hash agreement |

## 4. Showcase systems (1.0 evidence of production use)

| System | Path |
|--------|------|
| NEXUS secure agent | `examples/showcase/nexus/` |
| Anubis Vault password manager | `examples/showcase/anubis_vault/` |
| Verified private settlement | `examples/showcase/verified_private_settlement.anb` |
| Security reject/accept corpus | `examples/security/` |

## 5. Explicit residuals (not 1.0-blocking; not “closed forever”)

- Escaping-closure class: **map-entry application closed 2026-07-22** (`map_entry_closure_application_is_enforced`); remaining symbolic-index / deep-nested forms are fail-closed residuals (SH ⊆ Rust under-report where still open)
- Native SMT default flip **DONE 2026-07-25** (native-authoritative by default + verified RUP Unsat cert; opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0`). **General** integer division / var×var mul still deferred in the fragment; nonneg **const** dividends with divisor `2^k` rewrite to proven `bvlshr`/`bvand` (encoder identity, not native `bvsdiv` authority)
- VZ slice-2 **live apply DONE 2026-07-25**: `anubis vz apply` / `vz run --confine` emit `anubis.confinement.applied.v1` and pass tart args (e.g. `--net-host`). DNS-pinned egress **policy compiled** (`vz_egress_gateway`; empty = deny-all); live frame pump attaches at native-boot (kernel/initrd still optional host dep)
- `http_get`/`http_post` **run lowering DONE 2026-07-25**: cleartext pure-std TCP; **HTTPS via host `curl`** (system TLS TCB, same honesty as package registry)
- Hosted CI Metal *proving*: workflow `.github/workflows/metal-prove.yml` on self-hosted labels `self-hosted,macOS,ARM64,metal` + `scripts/run_metal_prove_gate.sh`. Stock GHA remains cold-verify; claim only when that job observes `metal-hybrid`
- Author-diversity: DDC toolchain diversity REAL; **architecture lane** `selfhost/backend_independent/token_scan.c` + `run_author_diversity_gate.sh` PASS. **TT-total still not claimed** (same-human residual)

## 6. Definition of “production-grade” for 1.0

Anubis 1.0 is production-grade for **Safe-mode verified systems programming** on the frozen
surface above: check/run/build/evidence/package/confine with independent reproduction and
dual-toolchain DDC. It is **not** a claim of universal general-purpose language completeness
or infinite multi-party audit forever.
