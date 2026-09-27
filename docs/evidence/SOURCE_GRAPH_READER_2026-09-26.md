# Bounded source-byte reader: private local result

Local code commit `4ba7455f1fbb2f696685a0ad202c43ebf51e04dc`
adds a private `CapturedTree` reader beside the
[caller-supplied graph identity primitive](../mission/VERIFIED_SOURCE_GRAPH_DESIGN_2026-09-26.md).
It retains exact bytes under portable source-relative paths and makes no
package admission, compiler correspondence, or evidence claim. No production
CLI or package resolver calls it yet; verified transitive package admission
still refuses valid inputs.

On Linux and Android, traversal holds each entry with `O_PATH|O_NOFOLLOW`
before classifying its inode. It reopens held regular files and directories
through a checked `/proc/self/fd` view, compares inode/device and type, and
refuses symlinks, nonregular entries, duplicate names, unportable names, and
resource-limit overflows. It includes ordinary files even under directories
named `out`, `target`, or `evidence`. A source-name swap after a descriptor is
held cannot redirect that descriptor's bytes. The external tree traversal is
not an atomic snapshot. Linux/Android filesystem, libc, and a genuine
current-process procfs in a trusted mount namespace remain external
assumptions; the guard check alone does not prove that procfs cannot be
hostilely bind-mounted. This reader explicitly returns `UnsupportedPlatform`
on Apple and other targets until an equivalent acquisition route is reviewed.
Android and Apple platform execution was not run here.

Independent read-only review held the first patch for Android errno, name
budget, duplicate-entry, and stat/open race errors. The revised exact final
diff SHA-256 is
`200b4a4e6225bd3c12275ebe4d881ee148f4d98971445cb8fea090b6bffe0b86`.
The reviewer found those issues corrected and gave a scoped GO for the final
diff, preserving the procfs assumption and Apple limitation. The lead's first
focused build found a Rust borrow/move compile error; the final diff fixes it
without changing the duplicate refusal. That failed compile log remains in
the local validation artifact, not a passing witness.

The lead's focused Safe reader test run passed its race, mutation, symlink,
nonregular, path, budget, and duplicate controls. Workspace Clippy with
`-D warnings`, format, diff and documentation-drift checks passed. The local
validation manifest SHA-256 is
`f106375edf92fa3b859ddb37ec6299b29728de8dcea238ce94fd2e7354f0301e`.
No source-bound production binary, full workspace, independent rebuild,
guest, hosted or release gate is claimed for this code head.

The next required unit is resolver/checker correspondence: resolve every
import from captured bytes with the existing candidate precedence, compare
parsed import occurrences to source-qualified edges, preserve per-module
sidecars, and make checking and lowering consume the same immutable graph.
Only a later versioned package/evidence closure producer and independent
consumer can safely restore verified multi-module and transitive admission.
