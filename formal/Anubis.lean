-- Anubis mechanized soundness (Phase 5). Root module: imports every proof file so
-- `lake build` machine-checks the entire formalization as one target.
import Anubis.Encoding
import Anubis.ArrayEncoding
import Anubis.Capability
import Anubis.CompareUnary
import Anubis.EffectSoundness
import Anubis.IntSigned
import Anubis.NonInterference
import Anubis.StringEncoding
import Anubis.UnsignedMask
