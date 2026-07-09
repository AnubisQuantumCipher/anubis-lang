# RISC0 Parameterized Input ABI (v1)

**Status:** design + implementation target for crown-jewel tranche  
**Prime law:** no claim without evidence.

## Goal

Prove: **program P**, given **input I**, computed **output O**.

| Artifact | Binds |
|----------|--------|
| ImageID | program P (guest ELF from P) |
| Journal | O = P(I) as u32 (v1) |
| input_sha256 | canonical JSON of I |
| receipt | verifies under ImageID |

## Language (Option B — builtins)

```anubis
// examples/proof/proof_factorial_input.anb
fn factorial(n: u32) {
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}
fn main() {
    let n = proof_input_u32("n");
    return factorial(n);
}
```

Builtins (v1):

| Builtin | Type | Guest behavior |
|---------|------|----------------|
| `proof_input_u32("key")` | u32 | lookup key in loaded map; panic if missing |
| `proof_input_bool("key")` | bool | 0/1 i64 in map |

Only meaningful under `prove --backend risc0`. In `anubis run`, builtins fail closed unless a future sim mode is added.

## CLI

```bash
anubis prove path.anb --backend risc0 \
  --input-json '{"n":5}' \
  --evidence --out out/dir

anubis prove path.anb --backend risc0 \
  --input-file examples/proof/inputs/factorial_5.json \
  --evidence --out out/dir
```

Rules:

- At most one of `--input-json` / `--input-file`.
- If neither is set: empty input map (n=0). Programs that call `proof_input_*` fail inside guest.
- Malformed JSON → `ANUBIS_PROOF_INPUT_INVALID_JSON`
- Missing file → `ANUBIS_PROOF_INPUT_FILE_MISSING`
- Unsupported JSON types (objects/arrays nested) → `ANUBIS_PROOF_INPUT_UNSUPPORTED_TYPE`
- Canonicalization: parse JSON → `BTreeMap<String, i64>` (bool→0/1, u32/i64 numbers) → re-serialize sorted keys for hash.

## Wire encoding (host → guest)

Host (`risc0-prove-child`) writes via `ExecutorEnv`:

```
u32 n_entries
for (key, value) in BTreeMap order:
    String key   // env::write / env::read
    i64 value
```

Guest `anubis_load_proof_inputs()` runs before `anb_main()`, fills `OnceLock<HashMap<String,i64>>`.

## Journal (v1 scalar + v2 multi-field + v3 named)

```rust
let r = anb_main();
anubis_commit_journal(r);
// proof_commit_u32("name", v) → env::commit(u32) immediately (names extracted from guest source)
// if any named commits: return is ignored
// else scalar return  → one env::commit(u32)   (v1-compatible; journal.bin length 4)
// else list return    → one env::commit(u32) per element (multi-field; length 4*N)
```

Host decodes `journal.bin` as LE u32 sequence and writes:

- `backend/risc0/journal_decoded.json`
- `risc0_metadata.json` → `journal_fields` (`name`, `value_u32`, `named`)

Example named fields (`examples/proof/proof_named_fields.anb`):

```anubis
fn main() {
    let a = proof_input_u32("a");
    let b = proof_input_u32("b");
    proof_commit_u32("sum", a + b);
    proof_commit_u32("product", a * b);
    return 0;
}
```

Gates:

```bash
bash scripts/run_multi_field_journal_gate.sh
bash scripts/run_named_journal_gate.sh
```

## Metadata fields (`risc0_metadata.json`)

| Field | Meaning |
|-------|---------|
| `guest_binding` | `"anubis-program"` |
| `input_mode` | `none` \| `json` \| `file` |
| `input_source` | path or `"--input-json"` |
| `input_sha256` | SHA-256 of canonical JSON bytes |
| `input_redacted` | false in v1 (no private split yet) |
| `input_schema_version` | `"1"` |
| `committed_journal_sha256` | journal file hash |
| `image_id` | from risc0-build |

## Evidence binding chain

`source_sha256` + `input_sha256` + `guest_source_sha256` + `guest_elf_sha256` + `image_id` + `receipt_sha256` + `committed_journal_sha256`  
must all be present for a parameterized REAL claim.

## Tests of honesty

1. Same program, n=5 → journal 120; n=6 → journal 720.  
2. Same input, different programs → different ImageIDs.  
3. Receipt verifies for both.  
4. Tamper of receipt fails verify.
