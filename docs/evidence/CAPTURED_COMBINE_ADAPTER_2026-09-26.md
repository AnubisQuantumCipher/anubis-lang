# Captured project combine adapter: scoped local receipt

Code commit `ebe4f3edf0ae4b9a93c04b26e467de2ca0f8a05b` extracts the
existing module-item rewrite into one combiner and adds a private adapter
that combines parsed modules from captured source bytes. The adapter carries
each module's trait sidecar into the returned project and refuses a
cross-module trait-name collision with a typed error. The current public
checker and package admission do **not** call this adapter. It has no
evidence or verified-dependency authority.

An independent read-only reviewer gave the frozen implementation patch a
static GO before integration (patch SHA-256
`dcda0013339731fd17a197608d0e9bc070e40fa432203884d46b211da9fc342b`).
The lead then ran the focused captured-resolver and legacy-resolver Safe
tests, workspace Clippy with `-D warnings`, formatting, and the
documentation-drift gate; all passed. The clean-head Linux AArch64 release
CLI SHA-256 is
`6d1bc9bb8a9fc5ecfb60e91bb4059a4d173277cd02a9153eae05c8fa1a9c4aed`.
The retained local build and comparison manifest SHA-256 is
`ecba9df9d6eb14739998fd6c5e82755ba6dab6214a47955dcb44e23b5f4dfc76`.
That build reused a local Cargo target and is not an independent rebuild.

A separate source-bound Safe CLI comparison used the clean-head binary and
the prior `e7187a31` binary on the same tracked multi-file inputs. The
comparison manifest SHA-256 is
`aea943585c98aa2a1055e0d0abdd5e7f6c2380385f29a16c764d934583e8e8f8`;
the reproducible local harness SHA-256 is
`420f00e7b93c48e27a5200ea09d4cfc0423262b07a24366fb720603e2fd06788`.
Both binaries returned byte-identical JSON diagnostics and stderr for
`mathlib`, `enum_vs_mod`, and `private_reject`. The valid examples passed;
the private call was refused as `ANUBIS_PRIVATE_ITEM`, never counted as a
contract disproof. These observations protect the production combiner
refactor on those examples. They do not exercise the new adapter through
the public CLI or prove all module behavior equivalent.

The remaining admission dependency is one immutable captured compilation
input shared by resolution, checking, lowering, package claims, and offline
verification. In particular, the current CLI still reopens/reparses entry
source and does not preserve imported trait sidecars through its production
path. The valid transitive package fixture remains refused at
`ANUBIS_DEP_PROOF_UNVERIFIED`; this precision/product gate is open. No full
matrix, workspace, disposable-guest, hosted, Apple, clean-room, or release
gate is claimed for this commit.
