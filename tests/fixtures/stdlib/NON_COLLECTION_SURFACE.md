# Non-collection argument surface — systematic fail-closed matrix

**Purpose:** Stop discovering “`reverse(42)` returns 42” one at a time.  
Every collection/string/map builtin is classified here. New builtins **must** get a row.

**Exercise path:** `anubis run` only. `anubis check` does not enforce these.

**Runner:**

```bash
# Fail-closed non-collection + prior empty/domain corpus:
bash scripts/run_runtime_fixtures.sh \
  --dir tests/fixtures/stdlib --glob '*should_fail_closed.anb'

# DOC_OK IEEE (must stay green; must never be "fixed" to panic):
bash scripts/run_runtime_fixtures.sh \
  --dir tests/fixtures/stdlib/doc_ok --glob '*.anb'
```

---

## Classification legend

| Class | Meaning |
|---|---|
| **FAIL_CLOSED** | Non-collection (or wrong-shape) argument panics `ANUBIS_TYPE_ERROR` (or more specific) |
| **LOCK_IN** | Fail-closed via `anubis_iter` residual (scalar not list/str/map) |
| **DOC_OK** | Documented leniency — **do not patch** |
| **OPEN** | Still soft at time of writing (should not remain) |

---

## Matrix (collection / string / map first-arg surface)

| Builtin | Accepts | Non-collection today (post-27, pre-R5) | Target | Fixture |
|---|---|---|---|---|
| `map`/`filter`/HOF via `anubis_iter` | list/str/map | **FAIL_CLOSED** (iter residual) | keep | (many `*_scalar_*`) |
| `sort` | list | **FAIL_CLOSED** | keep | `sort_scalar_should_fail_closed.anb` |
| `sum` | list | **FAIL_CLOSED** | keep | `sum_non_list_should_fail_closed.anb` |
| `keys`/`values`/`has_key` | map | **FAIL_CLOSED** | keep | `keys_*` / `values_*` / `has_key_*` |
| `push`/`pop`/`insert` | list | **FAIL_CLOSED** | keep | `push_*` / `pop_*` / `insert_*` |
| `first`/`last` empty | coll | **FAIL_CLOSED** empty | keep | `first_empty_*` / `last_empty_*` |
| `flatten`/`zip`/`concat`/`take`/… | via iter | **LOCK_IN** TYPE_ERROR | keep | `flatten/zip/concat_non_collection_*` |
| **`reverse`** | list/str | soft `other=>other` | **FAIL_CLOSED** | `reverse_non_collection_should_fail_closed.anb` |
| **`join`** | list | soft auto-stringify | **FAIL_CLOSED** | `join_non_list_should_fail_closed.anb` |
| **`index_of`** | list/str | soft `-1` | **FAIL_CLOSED** | `index_of_non_collection_should_fail_closed.anb` |
| **`contains`** | list/str/map | soft `false` | **FAIL_CLOSED** | `contains_non_collection_should_fail_closed.anb` |
| **`slice`** | list/str | soft passthrough | **FAIL_CLOSED** | `slice_non_collection_should_fail_closed.anb` |
| **`entries`** | map | soft `[]` | **FAIL_CLOSED** | `entries_non_map_should_fail_closed.anb` |
| **`merge`** | map,map | soft empty / ignore | **FAIL_CLOSED** both args | `merge_non_map_{first,second}_*` |
| **`remove` map missing key** | map | soft `Int(0)` | **`ANUBIS_MISSING_KEY`** | `remove_map_missing_key_should_fail_closed.anb` |
| `get(m,k,default)` | any + default | fail-**soft** by design | **DOC_OK** | (no fail-closed fixture) |
| `position` no match | list | `-1` sentinel | **DOC_OK** | (not a defect) |
| `starts_with`/`ends_with`/`replace`/`upper`/… | stringy | auto-stringify | **DOC_OK** (language-wide) | (not a defect) |

Patch that closes the SEKHMET four + same-class twins:  
`scratchpad/fleet_20260726/grok_ptah_round5/non_collection_failclosed.patch`

---

## § DOC_OK — IEEE float leniency (DO NOT PATCH)

| Case | Behavior | Fixture |
|---|---|---|
| Float `/0` | `±inf` | `doc_ok/float_div_zero_is_inf_doc_ok.anb` **EXPECT: PASS** |
| `sqrt(-1.0)` | `NaN` | `doc_ok/sqrt_negative_is_nan_doc_ok.anb` **EXPECT: PASS** |
| Integer `1/0` | `ANUBIS_DIV_BY_ZERO` | already fail-closed (control) |

**Rule for the next agent:** If a float op yields NaN/inf and LANGUAGE documents the IEEE model, it is **not** a fail-closed defect. Do not add `*_should_fail_closed` for those.

---

## How to extend systematically (not case-by-case)

When adding a stdlib builtin that takes a collection:

1. Add a row to this matrix **before** landing the impl.  
2. Decide Accepts = list | str | map | combinations.  
3. Default non-accepting residual → `panic!("ANUBIS_TYPE_ERROR: …")` (never `other => other`, never invent `[]`/`0`/`false`/`-1` for wrong type).  
4. Add `tests/fixtures/stdlib/<name>_non_collection_should_fail_closed.anb` with EXPECT FAIL + ERROR_CONTAINS.  
5. If the residual is intentional soft (`get`, DOC_OK IEEE, position `-1`), mark DOC_OK and add an EXPECT PASS twin under `doc_ok/` if easy to misread.

**Search heuristic for audits** (re-run anytime):

```bash
rg -n 'other => other|_ => anubis_mk_list\(vec!\[\]\)|_ => false|_ => AnubisValue::Int\(-1\)|_ => AnubisValue::Int\(0\)' \
  compiler/src/backends/run.rs
```

Any hit on a user-facing collection builtin that is not listed DOC_OK above is a candidate defect.
