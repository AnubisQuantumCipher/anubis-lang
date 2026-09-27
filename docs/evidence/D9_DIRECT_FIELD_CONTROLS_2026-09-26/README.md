# Direct-field loop and sibling controls — registration receipt

Fixture-only commit `59e4f1f94c4fd60d011b87eb6cb708fbcb84f4fa` registers the
frozen loop and sibling-projection controls before grading the direct-field
implementation. [Independent review](INDEPENDENT_REVIEW.md) approved their
source identities and intended semantics. The [inventory](registration-inventory.json)
binds the registry and each source; its complete source-file inventory matches
the registered forms. Historical matrix outcomes remain unchanged.

The [selected Safe baseline](baseline.json) uses the immutable compiler whose
SHA-256 is `d5b452c21c2a92d39199a8e93b2a5e9d1fc642fbbda493f45ef88a871448bcfd`,
built at code commit `2683669c2ae87ceea66d19ebb0e6995578c289eb`. Every selected
check returned a protocol-valid typed security refusal. The secret-loop
negative includes an IFC2 finding but no writer-located direct implicit-flow
finding. The public-loop, literal-false-loop and protected-sibling-write
ACCEPT controls remain precision defects. Their refusal is not a successful
implementation result.

Only `check` ran; no source was executed. The public-loop main varies a public
input alongside its protected input, so a future runtime noninterference
comparison must hold the public input fixed. This does not alter the frozen
source or its acceptance requirement.

The [verification record](verification.json) binds the original and published
baseline digests, normalization, independent review and documentation-drift
result. The original local records are retained. The registration drift check
passed with the existing scanner and floor unchanged; it is not a review of
every documentation claim or resolution of historical floor corrections.
The full matrix remains `INCOMPLETE`. The next dependency is independent
review of the coupled field transfer and egress consumer, followed by
source-matched candidate checks and direct writer-provenance tests.
