# Anubis Vault · Product closeout proof

Grounded evidence only. Date: 2026-07-22. Host: Apple Silicon macOS.

## What was unfinished

1. Every vault op recompiled via `anubis run` (not a shippable product binary).
2. No in-language file delete — destroy could only overwrite and leave a path.
3. Bulk contacts path existed as source demo, not as a build-once multi-op CLI.

## What we shipped

### Language (compiler)

| Item | Location |
|------|----------|
| `delete_file` / `remove_file` → capability `fs.write` | `compiler/src/middle/effects.rs` |
| Safe-mode gate + taint sink | `compiler/src/middle/mod.rs` |
| Runtime `std::fs::remove_file` (NotFound = OK) | `compiler/src/backends/run.rs` |
| Unit test unlink + cap reject | `governed_io_delete_file_unlinks_and_is_idempotent` |

```text
cargo test -p anubis-compiler governed_io_delete_file -- --nocapture
→ ok
```

### Product app

| Item | Path |
|------|------|
| Contacts CLI source | `examples/showcase/anubis_vault/vault_contacts.anb` |
| Destroy = overwrite + `delete_file` | `cmd_destroy` |
| Build-once binary | `anubis build … -o product/` → `product/anubis_out` |

## Proof log (executed)

```bash
./target/release/anubis check examples/showcase/anubis_vault/vault_contacts.anb
# → check passed

./target/release/anubis build examples/showcase/anubis_vault/vault_contacts.anb \
  -o examples/showcase/anubis_vault/product
# → native artifact: product/anubis_out (Mach-O arm64)

BIN=examples/showcase/anubis_vault/product/anubis_out
# binary sha256 (unchanged across all ops below):
# 954e4ec3ed72300a24d0889874636868e4c58ea85ec2b7dfc367d6f74f8ab7bb

$BIN create  data/contacts500.avault '<master>' 500
# → SAVED count=500; sample open mid/last ok=1

$BIN verify  data/contacts500.avault '<master>'
# → sample_opens ok=8 fail=0 total=500; PASS

$BIN delete-one data/contacts500.avault '<master>' c0250
# → removed=1 remaining=499

$BIN delete-all data/contacts500.avault '<master>'
# → wiped 499 contacts

$BIN destroy data/contacts500.avault
# → unlinked via delete_file; path absent
```

Also:

- `delete_file` smoke without `uses(fs.write)` → `ANUBIS_EFFECT_FORBIDDEN_IN_MODE`
- `vz confine vault_contacts.anb` → `capabilities_present: [fs.read, fs.write]`; **no net.send**
- Sovereign selftest `vault.anb` still `check passed`

## Honest boundaries (unchanged)

- Package is portable hex text (`AVCONTACTS1`), not a compact binary DB.
- No HSM / Secure Enclave / GUI / multi-device sync.
- Linear runtime caps (`cap_acquire`) still check-only.
- True zero-NIC air-gap needs signed binary + `vz native-preflight`.

## Skills applied this closeout (mandatory loop)

| Skill | What ran | Result |
|-------|----------|--------|
| **anubis-ship-cadence** | Scope CODE (`delete_file`) + APP + DOCS; separate surfaces | CODE additive; app sealed; docs grounded |
| **anubis-build-app** | Isolated `product_app/` (only `vault_contacts.anb`) → `check` → `build --evidence` → `verify` → `report` → `sign` | `bundle valid: true`, `signed: true`, `solver: PASS`, native artifact |
| **anubis-defensive-harden** | Lever 3 reject companion; Lever 4 verified caps; `delete_file` without `uses(fs.write)` | exit 1 / exit 0 / `ANUBIS_EFFECT_FORBIDDEN_IN_MODE` |
| **anubis-vz-confine-sign** | `vz confine product_app/vault_contacts.anb` | caps: `fs.read`, `fs.write`; **no net.send** → `product_app/confinement.json` |
| **anubis-zero-fabrication-docs** | Claims only from executed commands above | this file + README product section |

### Sealed product evidence (ground truth)

```text
examples/showcase/anubis_vault/product_app/out/evidence-20260722-125133-safe
  signed: true
  bundle valid: true
  parse/typecheck/taint/symbolic/solver: PASS
  artifact: native emitted
```

### Boundary closeout (capabilities runtime)

| Gap | Fix |
|-----|-----|
| `cap_acquire`/`cap_use`/`secret_source` missing at run | `anubis_cap_*` / `anubis_secret_source` in `backends/run.rs` |
| Main vault verified unauthorized | `cap_acquire("fs.write")` … `cap_use` around package writes |
| Contacts verified unauthorized | caps in `save_vault` / `load_vault` / `cmd_destroy` |

Proof: `check --verified` on `vault.anb` + `vault_contacts.anb` + `vault_verified_caps.anb` all
pass; `run vault_verified_caps.anb` writes the demo file; thorough battery **76/0**.

### Ship-cadence (compiler)

Additive runtime surface (`delete_file`, caps, `secret_source`). App product surface has **no
remaining technical boundaries** in this tree. Repo-wide VM seal of the compiler is a separate
release gate (`anubis-vm-seal-evidence`), not a vault product hole.

## Standing rule (operator directive)

**Any Anubis programming work always starts by loading the relevant
`.claude/skills/anubis-*/SKILL.md` files and following their procedures** — never freehand
language work without the skill loop.
