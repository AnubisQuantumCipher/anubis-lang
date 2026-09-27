# Editions

**Status:** proposal, implemented in the manifest parser. Not yet frozen.

SemVer versions the *implementation*. An edition versions the *language*.

That distinction is what lets a language change its mind without breaking code
that already exists. A package declares the edition it was written for, the
compiler keeps supporting every edition it has ever shipped, and a later edition
is free to reject what an earlier one accepted. Rust's editions are the model,
and they are the single mechanism most responsible for Rust being able to evolve
after 1.0 without a Python-3-style schism.

```toml
[package]
edition = "2026"
name = "my-app"
```

## The two rules that matter

### An unrecognised edition is refused, never assumed

A compiler that meets `edition = "2031"` does not know what that means.
Compiling it under today's rules would hand the program semantics its author
never asked for, silently. So it refuses:

```
ANUBIS_EDITION_UNKNOWN: `edition = "2031"` is not an edition this compiler
understands (supported: 2026). A newer edition is refused rather than compiled
under older rules, because that would give the program semantics its author did
not ask for. Upgrade the compiler, or declare a supported edition.
```

This is the same fail-closed reflex the rest of the language uses, applied to
its own evolution. It is also what makes adding a future edition *safe*: nothing
built today can be misinterpreted by a compiler that predates it.

### A missing edition is a warning now and an error at 1.0

Rust defaulted editionless manifests to 2015 and carries that default
permanently, because by the time the problem was visible there was too much code
in the wild to change it.

Anubis can still avoid that debt, for exactly as long as essentially no
third-party code exists. After 1.0 ships it becomes impossible. So the decision
is made now: today a missing edition resolves to 2026 and callers may warn; at
1.0 the key is mandatory and its absence is an error.

## What an edition may and may not change

An edition **may**:

- change the grammar, including removing syntax;
- change what `anubis check` accepts or rejects for the Safe surface;
- change the meaning of an existing construct, if the old meaning remains
  available under the old edition;
- promote a warning to an error, or a deferral to a refusal.

An edition **may not**:

- change the meaning of code that declares an *earlier* edition;
- change evidence-bundle or receipt verification for artifacts already sealed —
  a proof archived under one edition must still verify under every later one
  (see `docs/ROADMAP_AI_ERA.md`, criterion 16);
- be used to weaken a soundness fix. A fix that only *rejects more* is a PATCH
  under `SEMVER_1_0_POLICY.md` and applies to every edition at once. Editions
  exist to evolve the language, never to keep an unsound acceptance alive.

## Relationship to the 1.0 freeze

`SPEC_1_0_FREEZE.md` freezes a surface; `SEMVER_1_0_POLICY.md` governs changes
to it. Editions are the third leg: they are how the frozen surface is eventually
*succeeded* rather than broken. The frozen 1.0 surface is edition 2026. A future
edition 20NN may diverge from it, and a 2026 package keeps compiling under 2026
rules for as long as the compiler exists.

Editions apply to the Safe core. The Research annex is already declared
unstable in `SEMVER_1_0_POLICY.md` and changes without either a MAJOR bump or an
edition.

## Cross-edition dependencies

A package may depend on a package written under a different edition. Editions
are a property of the package, not of the build, and the compiler applies each
package's own edition rules to its own source. This is the property that makes
editions usable at all: without it, adopting a new edition would require the
whole dependency graph to move at once, which is precisely the failure mode
editions exist to prevent.

Proof-carrying composition across editions is a Phase 6 question and is not
settled here.
