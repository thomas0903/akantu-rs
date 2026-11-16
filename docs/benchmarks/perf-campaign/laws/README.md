<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Constitutive laws and the heat law on fixed-size kernels

Branch `perf/laws` (from `perf/wave2-base` 78a330615). Every change keeps
every bit of the default path (D-122): the suite digests of all 71
runnable cases at size `s` are unchanged after every step (`digests-s.txt`),
and so are those of every timed case at size `m` (column "digest" below).

## What changed

| Change | Where | Order argument |
|---|---|---|
| `EigenSolver` of fixed-size matrices (`N ≤ 4`): Hessenberg reduction, Francis double-shift iteration and eigenvalue extraction on `[[f64; N]; N]` stack arrays, the active-window indices passed as literals so that the steps unroll | `akantu-linalg/src/dense/eigen/fixed.rs`, used by `eig_fixed`, `eig3_fixed`, `eig_vectors_fixed` (Mazars, anisotropic damage) | a transcription of `real_schur` function by function: same operands, same order, same branches, same convergence test and iteration limit; only the run-time buffers (`Buf`, `Mat`) and their moves are gone |
| Maxwell: Voigt products of the law's sizes (1, 3, 6) with a constant size; the branch factors (`exp(−Δt/λ)`, `E_eff`) once per block instead of once per point | `akantu-solid/src/materials/maxwell.rs` | the same halving trees (`product_coeff` with a literal depth); the factors depend only on `Δt` and the branch |
| Anisotropic / orthotropic: `C·ε` through a constant-size kernel of the coefficient-based sequential route | `akantu-solid/src/materials/anisotropic.rs` | `r = c(i,0)·ε₀; r += c(i,k)·ε_k`, the route of the dynamic `C` times the fixed Voigt vector |
| Fixed-size inverse (`N ≤ 4`) writes into its result instead of a `Vec` allocated per call (neo-Hookean inverts `C` at every point) | `akantu-core/src/tensor.rs` | same closed forms |
| Heat law: `∇u` computed into its block when it has the gradient's shape; `D∇u` into its block with constant-dimension kernels; the internal-flow assembly keeps its two arrays between steps and borrows the filter | `akantu-diffusion` (`law.rs`, `heat_diffusion.rs`, `model.rs`, `model/assembly.rs`) | same kernels; the reused arrays are fully rewritten by `computeBtD` and `integrate` |
| Heat law: the diffusivity `k + var·(T_q − T_ref)` without the interpolation of `T` when the term vanishes bit for bit, and `D∇u` from the `d × d` conductivity in that case | `heat_diffusion.rs` (`temperature_term_vanishes`) | see below |
| Linear cohesive law: the point kernel always inlined and called with the dimension as a literal | `akantu-cohesive/src/laws/linear.rs` | same function, same arguments |

**The vanishing temperature term.** Every diffusivity entry is `k + s`
with `s = var·(T_q − T_ref)`. When `var` is `±0`, `T_ref` and every nodal
temperature are finite with magnitude at most `2^960`, `T_q` (a sum of
products of nodal values with shape functions of magnitude below `2^4`)
stays below `2^970`, `T_q − T_ref` is finite and `s` is `±0`. Then
`k + s = k` for every `k` that is not a NaN and not `−0` (`−0 + +0 = +0`).
Under these conditions the law writes `k` into every point (no
interpolation); otherwise it computes the term as before. The block is
still written at every recomputation (a reader of the diffusivity
internal sees what it saw before). With `conductivity_variation ≠ 0`
(temperature-dependent conductivity) nothing changes: the term is
computed at every recomputation, as upstream does.

Tests (bitwise, D-046/D-051 rule, inputs from
`akantu_core::bitwise_testing`: signed zeros, infinities, default NaNs of
both signs, subnormals, extremes, cancellation partners, arbitrary bit
patterns; payload mode with one NaN carrying a payload):

- `dense::eigen::fixed::tests`: `T`, `U` (with and without), the status
  and the eigenvalues of the fixed path against the general `real_schur`
  for `N = 1…4`, arbitrary, symmetric, symmetric with repeated
  eigenvalues and general finite matrices (68 000 cases), payload mode,
  matrices that reach the exceptional shifts and inputs that end in
  `NoConvergence`. In payload mode the one input NaN spreads through the
  iteration and meets itself (once negated) in later additions, where the
  operand IEEE 754 returns is unspecified (x86_64 returned the other
  sign than the general path for the same payload): the payload must
  match, the sign is free.
- `materials::maxwell::tests::constant_size_matvec_matches_the_general_product_bitwise`,
  `materials::anisotropic::tests::constant_size_voigt_stress_matches_the_general_product_bitwise`,
  `heat_diffusion::tests::fixed_dimension_d_grad_u_matches_the_general_kernel_bitwise`
  (also the constant-conductivity kernel): each kernel against the
  general product with a run-time size.
- `heat_diffusion::tests::vanishing_temperature_term_gives_the_conductivity_bitwise`:
  for 200 000 random draws where the predicate holds, `k + var·(T_q −
  T_ref) = k` bit for bit for interpolations with weights up to 8 in
  magnitude over 27 nodes; the predicate refuses non-zero variations,
  NaN or `−0` conductivity entries, non-finite or unbounded temperatures.

## Measurements

Machine: a Google Cloud VM, AMD EPYC 9B45 (Zen 5, 8 cores, one thread per
core), Ubuntu 24.04, the agent's own machine. Release profile, one process
per run, `scripts/perf/suite/run_ab.sh` (the two variants interleaved,
order rotated per repetition), 5 repetitions, size `m`, pinned to core 2,
nothing else running. "before" = the suite binaries of `perf/wave2-base`
78a330615, "final" = f25a10053 (the later commits change a test,
documents, and revert the `#[inline]` marks of four `akantu-core` matrix
reductions, which an interleaved A/B showed to have no measurable effect:
`screens/inline-ab/`), both built on the machine. Date 2026-11-16, 14:07–14:35
UTC. Median and the paired ratio per repetition, median [min, max]; a
range that contains 1 is no measurable change. Raw JSON lines and run
metadata: `final/solid/`, `final/models/`; full tables `final/solid.md`,
`final/models.md`.

| case | phase | before | final | final / before | digest |
|---|---|---|---|---|---|
| explicit_maxwell | solve_step | 3.194 s | 1.500 s | 0.470 [0.465, 0.471] | same |
| explicit_mazars | solve_step | 5.070 s | 2.881 s | 0.568 [0.540, 0.601] | same |
| explicit_multi_material | solve_step | 3.637 s | 2.431 s | 0.669 [0.661, 0.693] | same |
| explicit_mazars_non_local | solve_step | 5.029 s | 4.079 s | 0.811 [0.791, 0.829] | same |
| explicit_neohookean | solve_step | 1.489 s | 1.197 s | 0.800 [0.797, 0.813] | same |
| explicit_neohookean | energies | 176.82 ms | 126.25 ms | 0.714 [0.687, 0.727] | same |
| explicit_orthotropic | solve_step | 1.742 s | 1.373 s | 0.790 [0.782, 0.797] | same |
| explicit_marigo | solve_step | 2.178 s | 2.123 s | 0.975 [0.974, 0.978] | same |
| explicit_plastic | solve_step | 2.573 s | 2.507 s | 0.976 [0.961, 0.984] | same |
| explicit_sls | solve_step | 2.340 s | 2.377 s | 1.004 [0.991, 1.036] | same |
| explicit_sls | energies | 105.58 ms | 85.22 ms | 0.807 [0.807, 0.808] | same |
| explicit_thermal | solve_step | 1.805 s | 1.836 s | 1.016 [1.014, 1.028] | same |
| static_newton_plastic | solve_step | 1.887 s | 1.837 s | 0.994 [0.964, 1.016] | same |
| cohesive_intrinsic_bilinear | solve_step | 3.040 s | 2.926 s | 0.960 [0.959, 0.963] | same |
| cohesive_intrinsic_exponential | solve_step | 4.614 s | 4.700 s | 1.020 [1.013, 1.028] | same |
| cohesive_intrinsic_implicit | solve_step | 6.295 s | 6.211 s | 0.989 [0.985, 0.997] | same |
| heat_explicit_tet4 | solve_step | 1.977 s | 976.39 ms | 0.495 [0.488, 0.503] | same |
| heat_explicit_quad4 | solve_step | 2.388 s | 1.237 s | 0.518 [0.515, 0.542] | same |
| heat_implicit_hex8_kvar | solve_step | 878.03 ms | 870.38 ms | 0.993 [0.989, 0.997] | same |
| heat_static_tri6 | solve_step | 2.988 s | 2.961 s | 0.993 [0.966, 0.999] | same |
| phasefield_staggered_tri3 | solid_solve + phase_solve | 7.041 s + 1.325 s | 7.028 s + 1.312 s | 0.998, 0.995 (both ranges contain 1) | same |
| phasefield_staggered_tet4 | solid_solve + phase_solve | 1.390 s + 91.31 ms | 1.386 s + 91.20 ms | 1.000, 0.996 (both ranges contain 1) | same |

`explicit_thermal` (×1.016) and `cohesive_intrinsic_exponential` (×1.020)
run code this branch does not change (the elastic law with a thermal
stress; the exponential cohesive law); their step differs by 1–2 %, and a
second interleaved series of the final binary against a build without
the `#[inline]` commit gave 1.010 [0.985, 1.022] and 0.988 [0.974, 1.004]
for them: a code-placement effect of the size of the spread between
sessions, not a change of the work done. The SLS step (no change of its
own; its energy phase ×0.81 from the inverse) and the plastic and Marigo
steps (×0.98) gain only through shared kernels: their per-point kernels
were already fixed-size code (see below).

### Profiles: what is left

`perf record` (frame pointers, 999 Hz) of the same binaries with
`--loop-seconds 6`, flat top symbols and inclusive shares:
`profiles/before/`, `profiles/final/`.

Self time of the law's own code (the point kernel and its block loop)
in the step, before → final, and what dominates the rest:

| case | law's own share before → final | the rest |
|---|---|---|
| explicit_mazars | `real_schur` + Householder + `Buf` moves + `make_householder` ≈ 51 % → `real_schur_fixed` 34.7 % | exact `pow` / `exp` / `fma` / `round` of the damage law 14 %, FE kernels and scatter 13 % |
| explicit_maxwell | `scaled_matvec` 38.3 % + per-point `exp` 3.8 % → `stress_on_quad` + `update_int_variables` 13.6 % | per-step copies of the internal fields and their page faults (kernel time 28 %), FE kernels |
| explicit_neohookean | `inverse_impl` 10.2 % (with a heap allocation per call) + `kinematics` 7.1 % → `kinematics` 10.1 % (inverse inlined) | finite-deformation residual kernel 19 %, FE kernels, exact `log` / `fma` |
| explicit_orthotropic | generic `eval_product` + `route` 14.4 % → 0 % (the stress kernel is now part of `anisotropic_compute_stress`, 11.1 % with its loop and copy) | FE kernels 39 %, scatter 10 % |
| heat_explicit_tet4 | `D∇u` 8.7 % + diffusivity 3.1 % + interpolation of `T` 7.7 % + temporaries (`try_resize`, copies) ≈ 6 % → `D∇u` 4.1 % + diffusivity < 1 % | FE kernels (`integrate`, `gradient`, `btd`) 60 %, scatter 14 %, the FE filter copy (`Vec<usize>`) 4 % |
| heat_explicit_quad4 | 11.8 % + 3.4 % + 8.4 % + ≈ 7 % → 5.1 % + 2.6 % | FE kernels 61 %, scatter 8 % |
| explicit_marigo | `double_dot` call 10.4 % + `energy_release_rate` 1.8 % → `energy_release_rate` 6.9 % (`double_dot` inlined) | FE kernels, copies |
| cohesive_intrinsic_bilinear | `compute_traction_history` 10.4 % → 6.8 % | bulk FE kernels, cohesive assembly, `plan_facets` 3–4 % per step |
| cohesive_intrinsic_exponential | `compute_traction` + `coupled_traction` 14.4 % → 13.2 % | exact `exp` / `fma` 13 % |

The implicit cases (`static_newton_plastic`, `heat_implicit_hex8_kvar`,
`heat_static_tri6`, `cohesive_intrinsic_implicit`) and the phase-field
cases spend 55–90 % in the sparse factorisation, the triangular solves and
the hashed assembly: the laws' share there is below 5 %.

## Fast mode (numbers and a proposal; not in the default path)

**Closed-form 3 × 3 eigenvalues** (Mazars, the damage laws' principal
strains): `cargo run --release -p akantu-linalg --example
eig3_closed_form 200000 9` (trigonometric solution of the characteristic
cubic of a symmetric matrix against `eig3_fixed`, 200 000 strain-like
matrices with entries of order `10⁻⁴`, a fifth with a zero off-diagonal
pair, a tenth with a repeated diagonal entry), pinned to core 2 of the
same machine, three runs (`micro-eig3.txt`):

| variant | ns per matrix |
|---|---|
| `eig3_fixed` (bitwise, Eigen's order) | 280–290 |
| closed form | 58–60 |

Ratio 0.21. 89 % of the eigenvalues differ in their bits; the largest
difference is `9.8·10⁻¹⁴` of the largest eigenvalue magnitude. With the
eigen solver at 35 % of the final Mazars step, a closed form would bring
the step to about 0.72 of its final time (0.41 of "before"). Its
behaviour differs on degenerate input (a NaN or infinite entry: the
closed form returns NaN where `EigenSolver` may return `NoConvergence`)
and the eigenvalue order is ascending instead of Eigen's Schur order:
Mazars' damage is symmetric in the principal strains, other callers would
have to sort. Proposal: a `Precision::Fast` branch in `eig3_fixed`'s
callers (the damage laws), selected by the run-time switch of the
execution policy (not merged yet; `akantu-core::exec`), with a
tolerance test against `eig3_fixed` at `1e-12` relative.

**FMA in the law kernels.** The remaining law kernels are short
dependency chains of a few multiply-adds per point (`C·ε` of 6 × 6,
the Maxwell branch updates, the J2 return, `D∇u`) ending in divisions,
square roots and exact `exp` / `log` / `pow`. FMA would shorten each chain
by roughly one rounding step per term; the products of
`docs/benchmarks/perf-campaign/small-products/` measured at most 1.5× on
multiply-add-bound column kernels, and the law kernels are not
multiply-add-bound. Expected gain below 10 % of the laws' share. Not
measured; it needs the `fma` target feature with run-time dispatch
(`unsafe` calls), which the law crates forbid.

## Left as it is

- **Mazars' eigen solver** (35 % of its step): every operation is fixed
  by Eigen's order; the remaining cost is the latency of the iteration
  (square roots and divisions in a dependency chain, data-dependent
  iteration counts). SIMD across independent quadrature points would be
  bitwise (each lane the scalar sequence) but needs masked lanes for the
  different iteration counts and deflation branches; not done.
- **Per-point kernels already fixed-size**: the J2 return
  (`j2_small_strain_quad`, 8 % of `explicit_plastic`), the SLS point update
  (12 % of `explicit_sls`), Marigo's energy release rate (7 %), the linear
  and exponential cohesive tractions: their cost is the arithmetic itself,
  including per-entry divisions whose order the bitwise rule fixes, and
  the exact `exp` / `log` / `pow` translations.
- **Per-step copies and allocations of the laws' blocks** (whole-block
  clones of `grad_u`, `sigma`, the internals, `Vec`s of energy densities
  in `elastic_damage_potential_energy`, the per-step arrays of the
  plastic, viscoelastic and damage laws): kernel time (page
  faults: 31 %, 33 %, 28 % of the final profiles) in `explicit_plastic`,
  `explicit_sls`, `explicit_maxwell`.
  These belong to the laws' scratch and history storage, which another
  branch reworks; not touched here.
- **FE kernels** (`fast::gradient`, `btd`, `integrate`, the scatter, the
  filter copy in `resolve_filter`) now dominate the heat cases (75 %) and
  half of the elastic-family law cases; they are outside the laws.
- **`akantu_mesh::utils::plan_facets` inside the intrinsic cohesive step**
  (3–4 % of `cohesive_intrinsic_*` at every step): a facet plan rebuilt per
  step looks like repeated set-up work; not analysed (mesh crate).
- **Phase field**: the law's own work is below 2 % of the staggered step;
  the sparse solves and the hashed assembly dominate.
