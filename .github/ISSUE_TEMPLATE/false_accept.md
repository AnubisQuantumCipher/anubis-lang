---
name: "🚨 False accept (soundness break)"
about: "anubis check accepts a program that anubis run then violates — the highest-severity bug"
title: "[false-accept] "
labels: ["soundness", "bug"]
---

<!--
STOP: if this leaks secrets or is otherwise sensitive, email sic.tau@pm.me instead
of filing publicly (see SECURITY.md). Otherwise, thank you — this is the report
that matters most to Anubis.
-->

## The program

```rust
// a MINIMAL .anb that check accepts but run violates
```

## What happens

```
$ anubis check the.anb      # → accepted / green
$ anubis run the.anb        # → traps requires/ensures/assert/invariant (paste output)
```

## The discriminator (please run this)

Negate the property (flip the predicate or a constant) and re-check:

- [ ] The **opposite is rejected** → this is a genuine **false accept** (directional bug).
- [ ] The opposite is **also accepted** → likely a fail-open completeness gap, not a soundness break (still useful — say so).

```
$ anubis check the_opposite.anb   # paste the verdict
```

## Environment

- `anubis --version`:
- `git rev-parse HEAD`:
