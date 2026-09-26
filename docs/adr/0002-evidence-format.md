# ADR 0002: Evidence bundle format

## Status
Accepted

## Context
Sovereign reproducibility and bounty submissions require tamper-evident artifacts. Modelled on (and improved from) user's risc0-metal-hybrid evidence/ bundles.

## Decision
Timestamped dir: evidence-YYYYMMDD-HHMMSS-mode/
- evidence.json : manifest with source_hash (sha256), checks[], verdict, tool, timestamp
- source.anubis snapshot
- build.log
- artifact (binary)
- Optional traces, receipts.

Validation: all checks PASS + shasum of key files + manifest matches.

`anubis build --bounty` and `anubis verify <bundle>` .

## Consequences
First-class feature. Every build can produce it. Used for self-audit gates.

## Current-format addendum (2026-09-26)

The original decision above describes the initial bundle. Current bundles add
`pca.json` for a source-bound claim that `anubis verify` re-derives, plus solver,
proof-index, and replay artifacts. Bundle validity and the recorded program
verdict are separate facts; verification of a bundle is not a declaration that
its program passed. Manifest hashes detect inconsistency within the bundle.
Without a signature from a separately trusted key, they do not authenticate
its origin or age because an editor can rehash an unsigned bundle.

New PCA version 3 counts real solver obligations. The exact sole
`solver:no-obligations` marker is reporting metadata and contributes zero
obligations and no proof. Version 2 remains readable using its historical
counting convention; its solver discharge is re-derived under the current typed
rule. An unsigned version-2 label does not establish that a bundle was
historically produced by a version-2 compiler. Version 1 claims are refused.

For a new multi-file bundle, `source-merkle-leaves.json` uses schema
`anubis-source-merkle-leaves-v2` and names sorted, manifest-covered
`source-leaves/*.bin` files. Verification checks each supplied leaf's length and
digest, re-derives the path-and-content Merkle root, and checks its relation to
the sealed `source.anubis` snapshot. A single-file bundle retains its historical
source hash. Older multi-file descriptors that contain only paths and digests
cannot satisfy the current PCA or PASS-artifact validation: their original source
bytes must be recovered and new evidence issued. Their hash inventory remains
historical data, not an accepted source-closure proof. This format binds the
*supplied leaves*. It does not establish that a resolver selected every imported
module, dependency, or standard-library implementation, nor that the compiler
analyzed their semantics. Package admission therefore still refuses unsupported
multi-module and transitive graphs.

The optional `pca.sig` is checked when present, even when the caller supplies
no expected public key. `verify` can accept an intact, re-derived `FAIL` claim
and prints the recorded verdict; `validate` requires an intact PASS artifact.
Both check a claimed ZK receipt cryptographically and refuse that claim if the
required verifier is unavailable. A `build --no-verify` envelope has only
integrity meaning: `verify` labels it `UNVERIFIED`, while `validate` and
`evidence-verify` refuse it as assurance. The evidence verifier's top-level
`ok` reports its evidence checks; an honest FAIL PCA can still require a separate
program-verdict decision. A producer-reported build-log PASS is not independently
replayed, and current `--strict` does not require certification of every solver
obligation.

The exact source-to-obligation, SMT-to-CNF, compiled-behavior, and signature
trust boundaries remain governed by [proof correspondence](../PROOF_CORRESPONDENCE.md)
and the current [claims registry](../CLAIMS.md).
