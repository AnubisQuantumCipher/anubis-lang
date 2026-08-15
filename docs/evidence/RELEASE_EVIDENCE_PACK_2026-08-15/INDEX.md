# Anubis — product-release evidence pack (2026-08-15)

Completion Blueprint **Phase 7** deliverable (blueprint §59: *produce the product-release evidence
pack*). This pack is the **agent's half**: every artifact that substantiates a release is assembled,
hashed, and indexed here so a verifier can confirm integrity in one pass. **Cutting a tagged release
is deliberately not performed** — that is the operator's ratified act (see "Boundary" below).

Assembled on `main` HEAD `7464ff7f` (the sealed technical epoch); this document lands on the HEAD
that carries it. Point-in-time snapshot — a later release produces a new dated pack; this one is not
rewritten.

## What this pack proves

- **Completion Phases 0–7 (agent-reachable half) are done and receipted.** Phases 0–4 each carry a
  signed `PHASE_<n>_COMPLETION` receipt; Phase 5 is OPTIONAL-complete (surface gated, deferrals
  published); Phase 6 is MET (permanence controls committed + enforced); Phase 7 is this pack.
- **The self-host seal PASSED** on a re-provisioned throwaway guest: 0 gate failures, in-VM fixpoint
  `46ddce145e96a8971f5988bc8ef1b49c3af20544f62cb2822df67a1f9447ba60` == `scripts/vm/EXPECTED_FIXPOINT_VM`,
  disposable guest torn down, no host substitution (`PHASE_3_VM_SEAL_2026-08-15.md`).
- **The product promise is the only shipping promise** and remains honestly bounded: green means no
  KNOWN defects on the measured surface. Open residuals are named in `docs/CLAIMS.md`, not implied
  away.

## Sealed-pin provenance (verified this pack build)

The binary the seal graded, re-verified byte-for-byte at pack assembly:

| field | value |
|---|---|
| pin path | `vm/pins/anubis-2c9ee4079c7a-src-2e3c2bd71d5c` |
| pin sha256 | `2c9ee4079c7a9cd434231bd64f881f2fb5a3e0ed539d3806c342121dc412b53c` |
| meta sha256 | `f78712d1707e325b105169aa4d85752813ae99da5e898a37929406fde62c519d` |
| source tree sha256 | `2e3c2bd71d5c76dc427662b41e884c1c788a690552702a43aa105a879f9812c9` |
| `publish_pin.sh --verify` | `pin matches tree` (source-current at HEAD) |

Pin **binaries are gitignored** (`.gitignore:81 vm/pins/anubis-*`) and are republished locally per
slice by the active lead; the tracked `vm/pins/CURRENT` pointer is therefore a local pointer, not a
clone-portable artifact. The authoritative pin identity for THIS pack is the table above, which the
seal receipt records independently.

## Toolchain provenance

- Rust `nightly-2026-05-10` (`rustc 1.97.0-nightly 82bee9650`) — `rust-toolchain.toml`.
- Lean `leanprover/lean4:v4.32.0` — `formal/lean-toolchain`.
- z3 `4.15.4`, cmake, coreutils in the sealed guest (`PHASE_3_VM_SEAL_2026-08-15.md`).

## Indexed evidence (see `MANIFEST.sha256`)

`MANIFEST.sha256` lists the sha256 of each artifact below (repo-relative paths). Manifest self-sha256:
`3bc713c198dc1f2e4a7a7b2fbffb024e768c73ccf369eb00af4f574e150c3f5c`.

- **Completion receipts:** `PHASE_0`/`PHASE_1`/`PHASE_1.5`/`PHASE_2`/`PHASE_3`/`PHASE_4_COMPLETION_*.md`
- **Self-host seal:** `PHASE_3_VM_SEAL_2026-08-15.md`, `PHASE_3_VM_SEAL_ATTEMPT_2026-08-15.md`, `scripts/vm/EXPECTED_FIXPOINT_VM`
- **Phases 5–8 status + convergence:** `COMPLETION_PHASES_5_8_STATUS_2026-08-15.md`, `PHASE_METRICS_LEDGER.md`
- **Authorities:** `docs/COMPLETION_BLUEPRINT.md`, `docs/CLAIMS.md`, `docs/language/ROADMAP.md`, `MATURITY_CLAIM_MATRIX.md`
- **Permanence controls (Phase 6):** `.github/workflows/ci.yml`, `.gate_floors/native_authoritative.floor`, `examples/security/.corpus_expect_floor`, `examples/security/.fixture_count_floor`
- **Toolchain provenance:** `rust-toolchain.toml`, `formal/lean-toolchain`

## Verify this pack

```sh
# From the repo root, at the HEAD that carries this pack:
cd "$(git rev-parse --show-toplevel)"
shasum -a 256 -c docs/evidence/RELEASE_EVIDENCE_PACK_2026-08-15/MANIFEST.sha256
# Re-verify the sealed pin (if the local pin binary is present):
bash scripts/publish_pin.sh --verify
```

A mismatch means an indexed artifact changed after this pack was cut — expected only if the release
is re-cut (then a new dated pack supersedes this one).

## Boundary — what is NOT in this pack, and why

- **The tagged release / GitHub release / crates publish is NOT performed.** Per `AGENTS.md` ("only
  the active lead may build, publish pins, commit, or push") and `skill://human-gate-at-send-button`,
  a public release is a human-ratified action. This pack is the evidence a release would cite; the
  operator presses publish.
- **Phase 8** (mechanized-correspondence research) is **not** represented as done: the blueprint
  (§25–30) defines it as unscheduled research with no ship date, never presented as a current
  guarantee. It is not a product-completion gate and is left open and unclaimed.
- **Open soundness residuals** are NOT closed by this pack; they are the named entries in
  `docs/CLAIMS.md` § "Open — load-bearing" (item 21 rows 1/2/3/8/9/10, REG-002 full cert replay,
  permanent OS/hardware/author-diversity items). A pack indexes evidence; it does not upgrade a
  residual to closed.

---

`Phase 7 pack PRODUCED — Phases 0–7 agent-half receipted, seal PASS, evidence hashed + indexed; tagged release remains operator-gated`
