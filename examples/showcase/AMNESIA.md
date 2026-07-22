# AMNESIA — Transitive Machine-Unlearning Deletion Witness

AMNESIA is real data-governance infrastructure written entirely in Anubis. It verifies whether a post-erasure AI artifact manifest has removed every source, augmentation, mixed batch, checkpoint, and metric transitively derived from a revoked subject.

The core problem is lineage laundering: deleting `raw-alice-001` is meaningless if `checkpoint-009` still depends on a mixed batch that depended on Alice. AMNESIA computes the full monotone erasure closure before it examines the candidate state.

## Run the passing case

```bash
./target/release/anubis run \
  --allow-research \
  -o out/amnesia/run-clean --evidence \
  examples/showcase/amnesia_unlearning_witness.anb -- \
  examples/showcase/amnesia/before.manifest \
  examples/showcase/amnesia/after-clean.manifest \
  examples/showcase/amnesia/revocations.txt \
  out/amnesia/erasure-certificate.txt
```

## Run the retained-data attack

```bash
./target/release/anubis run \
  --allow-research \
  -o out/amnesia/run-retained --evidence \
  examples/showcase/amnesia_unlearning_witness.anb -- \
  examples/showcase/amnesia/before.manifest \
  examples/showcase/amnesia/after-retained.manifest \
  examples/showcase/amnesia/revocations.txt \
  out/amnesia/rejected-certificate.txt
```

The second command must exit nonzero with `verdict=FAIL` and name `raw-alice-001` under `residual_ids`.

The current Anubis native runner classifies `declassify(...)` as a research-lane construct, so `--allow-research` is required. AMNESIA uses it only at the final local `fs.write` boundary: input files are integrity-tainted, fully parsed and validated, then a certificate containing canonical ids, counts, roots, and proof paths is explicitly declassified. The tool performs no shell, network, process, or exploit operation.

## Manifest protocol

Each non-comment line has five pipe-delimited fields:

```text
artifact_id|kind|subject_or_dash|comma_separated_parents_or_dash|content_sha256
```

The before-manifest must be a unique-id acyclic DAG. The after-manifest must be an unchanged subset: no new identifiers, renames, altered parent sets, or content-hash swaps are accepted. Unrelated collateral deletion is recorded rather than hidden. Dangling retained children are rejected.

## What a PASS means

- **REAL:** every requested subject occurs in the before-manifest;
- **REAL:** every directly associated artifact and every transitive descendant is absent from the after-manifest;
- **REAL:** retained artifacts are byte-for-byte identical at the canonical manifest-record level and retain all declared parents;
- **REAL:** before and after states are committed by domain-separated binary Merkle roots;
- **REAL:** the certificate carries a before-state inclusion path and an after-state sorted-set non-membership proof for a challenged revoked artifact;
- **REAL:** both proofs are verified in-program, and reinserting that artifact flips the assessor to FAIL.

## Strict boundary

- **PARTIAL:** AMNESIA proves purge completeness over the declared artifact lineage. It cannot prove that an opaque checkpoint semantically forgot data unless the checkpoint and all of its derivations are represented faithfully in the manifest.
- **UNSUPPORTED:** AMNESIA does not claim that hashing a model file proves machine unlearning, and it does not claim that a dishonest manifest is truthful.
- **REAL:** the Anubis compiler evidence bundle can separately preserve the exact source, command, IR, findings, and artifact hashes used to run the witness.

No compiler or language implementation changes are part of AMNESIA.
