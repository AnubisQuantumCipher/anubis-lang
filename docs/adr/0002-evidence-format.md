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
