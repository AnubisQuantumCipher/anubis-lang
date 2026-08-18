-- Anubis mechanized soundness (Phase 5+). Root module: imports every proof file so
-- `lake build` machine-checks the entire formalization as one target.
import Anubis.Encoding
import Anubis.BitBlast
import Anubis.ContractComposition
import Anubis.LoopInvariant
import Anubis.PathCondition
import Anubis.ArrayEncoding
import Anubis.Capability
import Anubis.CompareUnary
import Anubis.DeclassifyWellFormed
import Anubis.EffectSoundness
import Anubis.IntSigned
import Anubis.ModeAggregation
import Anubis.NonInterference
import Anubis.StringEncoding
import Anubis.UnsignedMask
-- Completion Blueprint Phase 8, Slice 1 — production-linked SecurityLabel abstraction.
-- The observer module intentionally re-imports SecurityLabel; declaring both keeps the
-- `lake build` target verifying the theorems AND compiling the executable's dependency graph.
import Anubis.SecurityLabel
import Anubis.SecurityLabelObserver
