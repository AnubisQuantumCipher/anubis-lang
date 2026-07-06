# Anubis Security Language Competitive Radar (2026)

**Date:** 2026-07-06  
**Context:** Anubis on branch `a-plus-maturity/20260705-1649` after Gate 12/13/14 portable RC tranche.  
**Thesis:** Anubis aims to become a **policy-enforced, proof-aware, evidence-native security programming language** that unifies taint, contracts, fuzzing, symbolic reasoning, RISC0 receipts, CPU/GPU execution, SARIF, and tamper-verifiable disclosure bundles.

This document compares Anubis against leading tools and languages in security analysis, formal verification, fuzzing, binary research, GPU programming, and proof/receipt systems. Every claim is grounded in cited sources.

---

## 1. Static Analysis & Security Query Languages

### CodeQL (GitHub)
- **Strengths:** Extremely powerful dataflow + variant analysis, path queries, taint tracking across languages (C/C++, Java, JavaScript, Python, etc.). Excellent for finding complex vulnerabilities. Integrated with GitHub code scanning. Strong SARIF support.
- **Weaknesses:** Query language (QL) has a learning curve. Primarily source-code focused (limited binary). Not a general-purpose programming language for writing harnesses or PoCs. No built-in fuzzing or proof generation.
- **Anubis comparison:** Anubis has native taint + declassify with policy/reason + evidence bundles as first-class. CodeQL is query-after-the-fact; Anubis embeds security reasoning in the program itself with proof (RISC0) and fuzz integration.
- **What Anubis should learn:** Rich path-query expressiveness and variant analysis ideas.
- **What Anubis must not claim:** "We replace CodeQL for large-scale enterprise scanning" (Anubis is a programming + proof language, not a query engine over millions of lines of legacy code).

**Citations:** https://codeql.github.com/docs/ , GitHub Security Lab publications.

### Semgrep
- **Strengths:** Fast, lightweight, easy-to-write rules (YAML + pattern matching). Excellent for CI. Good taint tracking in recent versions. Broad language support.
- **Weaknesses:** Shallower analysis than CodeQL for complex flows. Limited symbolic reasoning.
- **Anubis:** Deeper (symbolic + proof) but currently narrower language surface. Semgrep wins on "run this rule on any codebase in 2 seconds."

### Joern / Infer / others
- Joern: Code property graphs for C/C++/Java — strong for taint and reachability.
- Infer (Facebook): Static analysis for null derefs, resource leaks, etc.
- Anubis advantage: Combines static + dynamic (fuzz) + proof in one evidence-producing language.

**SARIF pipelines:** All modern tools emit SARIF. Anubis must produce high-quality SARIF with security-specific rules (see Task 7 taxonomy).

---

## 2. Formal Verification & Proof Languages

### Dafny
- Excellent for writing verified algorithms with preconditions/postconditions.
- No native fuzzing or taint-to-sink security modeling for bug bounty workflows.
- Anubis can learn from its contract style for `@proof` and `@defensive` modes.

### F*, SPARK Ada, Why3, Frama-C, Prusti/Creusot
- Strong for memory safety and functional correctness.
- Steep learning curve; not designed for rapid PoC/harness writing by security researchers.
- Anubis positions itself as "security researcher first" — easier entry for taint + fuzz + evidence while still supporting RISC0 proofs.

### KLEE / CBMC
- Excellent symbolic execution and bounded model checking for C.
- Anubis already has a symbolic engine (Gate 7) + plans to integrate it with fuzz and proof lanes.
- Opportunity: Use Anubis as a higher-level language that can emit or drive these backends for security properties.

**Lean/Coq ecosystems:** For mechanized proofs. Anubis uses RISC0 (which has its own verification story) for practical receipt-based assurance rather than full theorem proving for every program.

---

## 3. Systems Programming Languages

**Rust** — Gold standard for memory safety without GC. Excellent ecosystem for fuzzing (cargo-fuzz, libFuzzer bindings). Anubis can learn from its ownership + effect-like thinking.

**Zig** — Great for low-level control and cross-compilation. Lacks Rust's safety guarantees.

**C/C++** — The attack surface. Anubis models problems in C/C++ targets safely (via harnesses and taint) without being C/C++ itself.

**Go, Swift, Nim, D** — Various safety/performance tradeoffs. None combine taint+symbolic+fuzz+RISC0+Metal+evidence bundles the way Anubis is designed to.

Anubis is **not** trying to replace Rust for general systems programming. It is a specialized security research language that can target or analyze programs written in those languages.

---

## 4. Fuzzing & Vulnerability Discovery

### libFuzzer, AFL++, honggfuzz, syzkaller, OSS-Fuzz
- Mature, coverage-guided, highly effective.
- Anubis V1 fuzz harnesses will initially be a safe, evidence-producing wrapper around local execution (deterministic or libFuzzer-style). Later tranches can drive external engines.
- Sanitizers (ASan/UBSan/MSan/TSan) are gold for crash triage. Anubis evidence bundles should capture sanitizer output when the harness is run under them.

**Grammar-based & differential testing:** Anubis can model protocol frames and generate differential test harnesses (compare multiple implementations of the same spec).

Anubis strength: Every fuzz run is expected to produce a signed, tamper-evident evidence bundle with authorization metadata.

---

## 5. Binary & Security Research Tools

**angr, Ghidra, Binary Ninja, radare2/rizin, eBPF, QEMU user-mode:**
- These excel at reverse engineering and dynamic instrumentation of opaque binaries.
- Anubis is primarily a **source-level** security programming language that can also model and harness binary parsers/formats.
- Future: Anubis harnesses can drive QEMU or eBPF for deeper tracing while still emitting unified evidence.

Anubis must not claim to replace Ghidra for interactive RE.

---

## 6. GPU / Accelerator Programming

**CUDA, Metal, Vulkan compute, SYCL, WGSL, Mojo:**
- Performance for parallel workloads.
- RISC0 Metal-hybrid proving (via the canonical `/Users/sicarii/Desktop/metal-hybrid-prover` + `r0-metal-doctor`) is a key differentiator for Anubis proof generation on Apple Silicon.
- Anubis should document how `@proof` + Metal lane can accelerate certain security proof workloads (e.g., large symbolic or fuzz-derived verification tasks) while keeping the receipt path post-quantum inner / classical outer as previously classified.

**r0-metal-doctor integration:** The doctor companion to the metal-hybrid-prover should be invoked or referenced by `anubis doctor --require-metal` and in security proof evidence when Metal lanes are used.

---

## 7. Proof / Receipt Systems

**RISC0 (primary for Anubis):**
- In-process proving, real receipts, ImageID, journal.
- Metal-hybrid acceleration on Apple Silicon via the pinned reference.
- Anubis already achieves fresh receipt + verify (Gate 10) and parity (Gate 11).
- Gate 15 extends this to security-specific proofs (policy compliance, crash reproduction, differential results) with full authorization + evidence metadata.

**SP1 and other zkVMs:** Similar receipt model. Anubis can remain RISC0-native while documenting compatibility paths.

**in-toto / SLSA / reproducible builds / attestations:**
- Anubis evidence bundles (with MANIFEST.sha256 + security block) are a natural fit for supply-chain security attestations of analysis results and PoCs.

---

## Summary Positioning (What Anubis Claims)

Anubis is uniquely positioned as:

- A **programming language** (not just a query engine or fuzzer) where security properties are expressed in code.
- **Evidence-native** by default (bundles, SARIF, reports, receipts).
- **Proof-aware** via RISC0 (with Metal acceleration option).
- **Policy-enforced** (capability modes + effects + authorization metadata).
- Designed for **authorized bug bounty / defensive / research** workflows with built-in guardrails.

**What we do not claim:**
- Replacement for CodeQL/Semgrep at massive scale on arbitrary legacy codebases.
- Full interactive binary RE workstation (Ghidra/Binary Ninja territory).
- General-purpose safe systems language (Rust's job).
- Weaponized exploitation framework.

All future claims must be backed by the evidence produced by the language itself.

---

**Next steps in this tranche:** Implement the capability model, effects, fuzz, bounty reports, taxonomy, examples, and runner while keeping every prior gate green.

*This radar will be updated as Gate 15 and later tranches land concrete capabilities.*
