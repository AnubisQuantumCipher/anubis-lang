# Shadowed match binder and guard-write controls: registration receipt

Fixture-only commit `2ca7afff0df115e9fcaf729ef2a34304e72830af`
registers a reached, statement-free `if let` call in a value-match arm,
its direct violated twin, a required valid call before a later binder
write, and a distinct stale-prior-nonmatch control. The source, binary,
diagnostic stream, runtime logs, and registry hashes are retained in a
local machine-readable manifest (SHA-256
`c8ae1fc2ced33cd71161bc636154017ea839aa551e3b539de75aca9e6b31c408`).
The registry SHA-256 at the fixture commit is
`514d9c2297f1b0ab17da8f490e82278a2d5a081558f1f7f373422500b3f9da95`.

The immutable `e7187a314db5385e2d34fd74dd3b773878112195` Safe CLI
(binary SHA-256
`8b85333dc2615b13d2d044cf19d421a2d40e7c5f6d4b63145465ee400516fe2b`)
produced these source-hashed baseline outcomes:

| Registered source | Required intent | Observed baseline |
|---|---|---|
| `r2arm_nested_iflet_binder_call_invalid` | REJECT | PASS: **silent accept** of reached `f(0)` |
| `r2arm_nested_iflet_binder_call_invalid.direct` | REJECT | `ANUBIS_ASSERTION_DISPROVED` |
| `r2arm_guard_call_before_binder_write_valid` | ACCEPT | PASS; must remain accepted after a repair |
| `r2arm_guard_write_stale_prior_nonmatch_invalid` | REJECT | `ANUBIS_ASSERTION_UNDECIDED`, explicitly unencoded, not a disproof |

The invalid carrier's source has outer `x=1`, but `match y` binds a new
`x=0` before an `if let` branch calls `f(x)` with `requires(v > 0)`.
A harmless source variant that prints its argument also passed `check`
on the same pinned binary, then a normal Safe `run` without `--no-verify`
printed `entered f with 0`. Its source SHA-256 is
`3aa08f5ed5f8eeaf5870aeb30ee58acd4a356b7c7b5794223d5e031d9d085db1`;
the complete runtime log SHA-256 is
`ae72851c3246f63725003119e9fc13fabd5ec24ef3d754afed4ec5c718b9a36d`.
The first wrapper attempt lacked the required user-bus environment and
failed before executing Anubis; that log is retained, and the corrected
environment is explicit in the local manifest. The stale-prior-nonmatch
case refuses normal execution as UNDECIDED; its separate `--no-verify`
Safe run printed `0` while clearly warning that execution was UNVERIFIED.
That development run is an executable witness, not proof evidence.

The earlier `r2arm_guard_write_stale_nonmatch_invalid` source was found
byte-identical to `r2arm_guard_write_failed_invalid`. Its existing source
and ID were preserved; the registry note now discloses that it is not an
independent stale-nonmatch test. The newly named prior-nonmatch case is
the distinct control. A registry preflight found unique IDs and all
declared forms present. Documentation drift passed after registration.

The proposed guard-write implementation is **not integrated**. Independent
review put its first two frozen drafts on HOLD for binder-identity holes
and a valid before-write over-refusal. A further draft is under review.
The full soundness matrix runner remains `INCOMPLETE`; no current all-case,
workspace, hosted, guest, Apple, or release pass is claimed here.
