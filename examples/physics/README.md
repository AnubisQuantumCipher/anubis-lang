# Physics programs (Anubis)

## Orszag–Tang + Plasmoid Cascade (`orszag_tang_plasmoid_cascade.anb`)

**Domain:** MHD turbulence + plasmoid-mediated reconnection cascade  
**Problem attacked:** nonlinear energy cascade (Orszag–Tang) and multi-island / coalescence dynamics on a high-S Harris sheet

### Why this problem

- **Orszag–Tang vortex** is the standard 2D MHD turbulence test: coupled velocity and magnetic fields cascade energy across scales; spectral transfer and dissipation are still research-grade.
- **Plasmoid instability** (and subsequent **island coalescence**) is the leading route from slow Sweet–Parker sheets to fast reconnection at high Lundquist number. Multi-island counting is a first-class diagnostic.

### What it does

| Part | Content |
|------|---------|
| A | 2D incompressible resistive/viscous MHD (ψ, ω, Az), Jacobi Poisson, Ek/Em, 1D spectral shells k=0..6 |
| B | Dual multi-mode Harris sheet: LOW-S vs HIGH-S+anomalous; island extrema time series; coalescence drop |
| C | Regime labels + theory markers (S_crit ~ 10⁴) + evidence seal |

### Run

```bash
cd /Users/sicarii/anubis-lang
./target/release/anubis run examples/physics/orszag_tang_plasmoid_cascade.anb \
  --evidence --out out/physics_ot_plasmoid
```

**Success signal (representative):**

- last line `0`
- `turb.regime.label=CASCADE_ACTIVE`
- `plasmoid.regime.label=ISLAND_COALESCENCE` (or `PLASMOID_UNSTABLE`)
- `seal.verdict=RESEARCH_CONSISTENT`
- high-S island drop and higher magnetic dissipation than low-S

### Honest scope

Not 3D kinetic PIC, not a production tokamak code. Demo S is below literature S_crit≈10⁴; multi-mode coalescence is still a valid first-class control diagnostic of the cascade pathway.

---

## Fast Magnetic Reconnection Research Kernel (`fast_magnetic_reconnection.anb`)

**Domain:** plasma physics — solar flares, magnetospheric substorms, fusion confinement  
**Problem attacked:** the **Sweet–Parker vs fast reconnection discrepancy** (still open)

### Why this problem

Classical resistive MHD (Sweet–Parker) predicts a dimensionless reconnection rate

\[
R_{\mathrm{SP}} \sim S^{-1/2}, \qquad S = \frac{L V_A}{\eta}
\]

At coronal Lundquist numbers \(S \sim 10^{12}\)–\(10^{14}\), \(R_{\mathrm{SP}}\) is tiny. Solar flares and magnetotail substorms require rates closer to \(R \sim 0.01\)–\(0.1\). That gap is one of the central unsolved / actively researched problems in plasma physics (Petschek geometry, plasmoid instability for \(S \gtrsim 10^4\), anomalous resistivity, Hall/collisionless physics, stochastic reconnection). Multi-scale plasma turbulence is also a fusion bottleneck.

### What the program does

| Stage | Content |
|-------|---------|
| 1 | Harris current sheet + tearing seed in vector potential \(A_z\) |
| 2 | Reduced 2D resistive induction: \(\partial_t A_z = \eta(J)\,\nabla^2 A_z\) |
| 3 | **Classical** uniform \(\eta\) vs **anomalous** \(\eta=\eta_0(1+\alpha(|J|/J_c)^2)\) dual runs |
| 4 | Measure \(R=\lvert E\rvert/(B_{\mathrm{in}} V_A)\) at the X-point |
| 5 | Lundquist theory scan: global diffusion, SP, Petschek, stochastic upper bound |
| 6 | Solar-like \(S=10^{12}\) shortfall report vs target \(R=0.01\) |
| 7 | Magnetic energy decay vs cumulative ohmic heating residual |
| 8 | Sheet width, midplane island extrema, 3-band cascade proxy |
| 9 | Regime classify + evidence seal |

### Run

```bash
cd /Users/sicarii/anubis-lang
./target/release/anubis run examples/physics/fast_magnetic_reconnection.anb \
  --evidence --out out/physics_reconnection
```

**Success signal (representative):**

- last line `0`
- `seal.verdict=RESEARCH_CONSISTENT`
- `regime.label=FAST_ANOMALOUS`
- `compare.boost` ≳ 2 (anomalous faster than classical)
- `compare.classical_over_SP_theory` ~ O(1) (classical sits near SP)
- `science.solar_SP_shortfall_vs_0p01` ≫ 1 (the open-problem signature)

### Honest scope

This is **not**:

- a full 3D kinetic / PIC reconnection code
- a claim that Anubis “solved” solar flares or fusion
- cryptographically attested physics (ordinary `anubis run` evidence)

This **is**:

- a sealed, reproducible **research control kernel** for the rate-discrepancy problem
- dual-experiment proof that **anomalous resistivity opens a fast channel** on a Harris sheet
- a theory confrontation showing SP collapses at solar \(S\) while Petschek/stochastic stay finite
- evidence-native output auditors / CI can grep

### Key equations implemented

- Harris equilibrium: \(B_x = B_0\tanh(y/\delta)\), \(A_z = B_0\delta\ln\cosh(y/\delta)\)
- \(\mathbf{B}=\nabla\times(A_z\hat{z})\) ⇒ \(\nabla\cdot\mathbf{B}=0\) by construction of the \(A_z\) formulation (discrete ops approximate this)
- \(J_z = -\nabla^2 A_z\), \(E_z=\eta J_z\) in the diffusion region
- \(R_{\mathrm{SP}}=S^{-1/2}\), \(R_{\mathrm{Pets}}\approx\pi/(8\ln S)\)
- Stochastic upper-bound (LV-style scaling): \(R\sim (v_\ell/V_A)^2\min(\sqrt{L/\ell},\sqrt{\ell/L})\)
