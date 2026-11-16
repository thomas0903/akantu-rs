# Where the time goes: solid mechanics and cohesive elements (`suite_solid`)

Sources: the baseline at size `m` (`baseline/`, an AWS `m7a.2xlarge`, AMD EPYC
9R14, one core, see `README.md` of this directory) for the phase times, and
the profiles of every case (`profiles/<case>.txt`, `perf record -F 499
--call-graph dwarf,16384`, the main loop extended to 10 s; and
`profiles/<case>.callers.txt`, `-F 299 --call-graph dwarf,65528` with the
library of each symbol, for cases whose shorter stack dumps lost frames).
The profiles were recorded in a boat.dev sandbox (AMD EPYC 4585PX, Zen 5;
same binary, commit `c698395e8`): shares, not times. The libc addresses
`0x1a14d3`, `0x1a16c6`…`0x1a1794`, `0x1a1949`… in the profiles are inside
glibc's AVX-512 `memmove` (disassembled; the small-copy path for
`0x1a14d3`), reached from `copy_nonoverlapping` / struct moves.

Shares are of the whole profiled run (the step loop is about 90 % of it
except where noted). "Bitwise-safe" means the fix computes every result
with the same operations in the same order (COMMON.md, section 1).

## Ranked list

Ranking: share of the affected cases × expected gain × how common the use
is (explicit elastic and implicit assembly are used by nearly everyone).

| # | hot spot | cases and share | reading | fix and estimate | kind |
|---|---|---|---|---|---|
| 1 | Sparse matrix assembly: one SipHash `HashMap` lookup per elemental entry (`SparseMatrixAij::resolve_block`, `add_to_profile`, `hash_one`, `write`; `crates/akantu-linalg/src/sparse.rs:239`, `:597`), also inside `add_matrix` (`:719`, the Jacobian `J` rebuilt from `K`/`M` at every Newton iteration) and `mat_vec_mul` (`:864`, `layout.local()` hashed for every non-zero on every call) | `setup_*`: 43 % (tet4), 40 % (tet10), 25 % (hex8) self in `resolve_block` + 5–10 % `add_to_profile`/`hash_one`/`write`; `cohesive_intrinsic_implicit` 25 %; `implicit_dynamic` 8 % `hash_one` + 10 % `mat_vec_mul` | algorithmic: the positions of an element block in the matrix do not change while the profile does not change, yet they are looked up through a cryptographic hash at every assembly | cache the resolved positions per (matrix, element type, profile release) and accumulate through the cached plan in the same loop order; or a non-cryptographic hasher (new dependency, or a hand-written one). Stiffness/mass assembly ×0.3–0.5, Newton iteration of small implicit problems ×0.7–0.8 | bitwise-safe |
| 2 | History copies at every step (`save_previous_state` → `InternalField::save_current_values`, `crates/akantu-model/src/internal_field.rs:593`: `stress`, `grad_u`, `sigma_th`, `epsilon_th` and the laws' internals copied into `previous_*`) | `explicit_tet4_ss` 19 % (memmove), `explicit_tri3_ss` 15 %, `explicit_hex8_ss` 9 %, all explicit cases 8–20 %; baseline: `step_hooks` = 16 % of the tet4 step (2 × 1.08 of 13.76 ms), 17 % of hex8 ss | memory-bound copies; upstream's `savePreviousState` does the same | skip internals whose values cannot have changed (`sigma_th`/`epsilon_th` of a law without temperature change; history fields nobody reads); double buffering is blocked by the Python views (D-047). Step ×0.85–0.9 | bitwise-safe (same values) |
| 3 | Generic Eigen-order reductions inside constitutive laws (`eigen_order::redux_tree`, `eigen_gebp::product_into`, `tr_matmul`, `inverse_impl`) | `explicit_maxwell` 42 % (`scaled_matvec`, `maxwell.rs:190`, in `update_int_variables` `:547` at the end of every step; baseline: `step_hooks` = 74 % of the step, 2 × 63.7 of 173.3 ms); `explicit_marigo` 30 % (`double_dot` in `energy_release_rate`, `marigo.rs:145`, and the energy update); `explicit_neohookean` 37 % (`tr_matmul` and `inverse_impl` in `neohookean_stress`, `neohookean.rs:108`); `explicit_plastic` 15 % (`double_dot`, `squared_norm`); `explicit_sls` 12 %; `explicit_orthotropic` 12 % (`product_into` in `stress_on_quad`, `anisotropic.rs:385`); `explicit_hex8_*` 3–5 % (`trace`); `cohesive_intrinsic_*` 10–12 % (generic `gradient_kernel` on cohesive elements) | generic kernels where a fixed-size one would do; the elastic path got fixed-size kernels with the same reduction trees in the explicit-step work, these laws did not | fixed-size kernels reproducing the same reduction trees (as `fast::gradient`/`btd`), each with a property test against the generic kernel. Maxwell step ×0.4, Marigo/Neo-Hookean ×0.6–0.7, plastic/SLS/orthotropic ×0.85 | bitwise-safe |
| 4 | Insertion check of extrinsic cohesive elements (`SolidMechanicsModelCohesive::interpolate_stress`, `crates/akantu-cohesive/src/model.rs:1486`, generic `product_into`, `check_insertion`, `find_elements_around_subfacet`) | `cohesive_extrinsic_check`: the check costs 4.1× the step (49.3 against 12.0 ms per call, 594 ns per element); `cohesive_extrinsic_insertion`: 88 % of the timed step is `check_insertion` (139.6 of 158.6 ms), of which `interpolate_stress` 18 %, `product_into` 18 %, `check_insertion` 13 %, `find_elements_around_subfacet` 8 % | generic product for the stress interpolation to the facets (named in `docs/BENCHMARKS.md`, item 7), copies of the interpolated stresses (`copy_nonoverlapping` 10 % of the check case), per-insertion neighbourhood searches | fixed-size interpolation kernel with the same order; reuse the facet-stress buffer; precompute facet → element adjacency once per insertion batch. Check ×0.4–0.6; the JOSS benchmark's insertion regime is dominated by it | bitwise-safe |
| 5 | Consistent mass and stiffness elemental matrices (`FeEngine::field_matrix_elemental`, `crates/akantu-fe/src/engine/assembly.rs:207`, and the stiffness `BᵀDB` path) | `setup_*`: generic `product_into` 11 % (tet4), 18 % (tet10), 29 % (hex8); baseline: `mass_consistent` 10.6 / 162 / 167 µs per tet4 / tet10 / hex8 element, `stiffness_assembly` 6.5 / 77 / 75 µs (the lumped mass: 0.2 / 0.1 / 0.7 µs) | two defects: (a) the point matrices `Nᵀ ρ N` of **all** elements and points are materialised (`ne · nq · (3 nn)²` reals) before the integration: 4.4 GB for 35 k tet10 elements, out of memory (8 GB sandbox) for 64 k hex8 elements; (b) each point matrix comes from the generic product | integrate per element as the point matrices are produced (same summation order) and a fixed-size `Nᵀ ρ N` kernel: memory O(1) instead of O(ne·nq), time ×0.2–0.4 | bitwise-safe |
| 6 | Eigenvalues of 3×3 strains (`eig3_fixed` → `real_schur`, `crates/akantu-linalg/src/dense/eigen/real_schur.rs`) | `explicit_mazars` 24 % small `memmove` + 12–14 % `real_schur` + 4 % `eig3_fixed`; `explicit_multi_material` 23 % + 8 %; `explicit_mazars_non_local` 12 % + 6 % | struct moves of the inline matrix buffers (Rust emits `memcpy` calls for them; libc's < 128-byte path) | keep the Schur iteration on one buffer in place (no moves). Mazars step ×0.7. A closed-form 3×3 eigen solver would change bits (fast mode) | bitwise-safe; closed form: fast mode |
| 7 | Non-local weights recomputed at every step (`NonLocalManager::compute_all_non_local_contribution` → `compute_weights`, `crates/akantu-model/src/non_local/manager.rs:1260`, `:1157`; `update_pair_list`, `weight`) | `explicit_mazars_non_local`: `compute_weights` 21 %, `update_pair_list` 12 %, `weight` 9 %, `weighted_average` 10 % | repeated set-up work per step: with the default `update_rate = 1` every stress computation recomputes all weights, also for `base_wf`, whose weights never change (upstream does the same, `non_local_neighborhood_tmpl.hh:237`) | cache the weights of weight functions that do not depend on the state (recomputing them gives the same bits). Non-local step ×0.6 | bitwise-safe |
| 8 | Per-step allocation and release of large scratch arrays in laws (`MaterialPlastic::j2_block`, `plastic.rs:833`; the same kernel signature in `sls_deviatoric` and `viscoelastic_maxwell`, whose allocating call was not identified) | `explicit_plastic` 12 % kernel time (`clear_page_erms`, page faults, `brk` shrink on `free` inside `j2_block`); `explicit_sls` 16 % and `explicit_maxwell` 7 % kernel time of the same kind | allocation: the buffer is freed every step, glibc returns it to the kernel, the next step faults it in again | keep the scratch in the law between steps (as the internal-force assembly buffers). Step ×0.85–0.9 | bitwise-safe |
| 9 | Energies (`kinetic_energy`, `energy("potential")`) | baseline, phase `energies` (one kinetic + one potential): 1.16× an explicit step for tet4 ss (16.0 against 13.8 ms), 1.5× for tet10, 1.7× for hex8 (115 against 68 ms); the upstream explicit example and the cohesive tests compute energies at every step | separate passes over the mesh, generic `double_dot` per point for the potential | fused element loop with a fixed-size kernel. Energies ×0.3–0.5: a run that logs energies every step spends half or more of its time there, so this ranks with items 2–3 for those users | bitwise-safe |
| 10 | Element filter converted to `Vec<usize>` on every kernel call (`akantu_fe::engine::resolve_filter`, `crates/akantu-fe/src/engine/mod.rs:252`) | 2–3 % of every explicit case (`tri3` 2.8 %, `tet4` 1.9 %) | allocation and copy per call of an unchanged filter | validate once per filter version and reuse | bitwise-safe |
| 11 | Explicit elastic kernels (`fast::btd`, `fast::gradient`, `fast::integrate`, the scatter `assemble_elemental_array_local_view`, `finite_deformation::fast::residuals`) | tet4 ss: 15 + 13 + 12 + 8 %; hex8 ss: 23 + 17 + 10 %; fd: `residuals` 20–32 % | already fixed-size and tight (the explicit-step work); memory-bound for tet4 | fuse `btd` with the integration (no per-point array), threads over elements with the scatter in element order (`docs/BENCHMARKS.md`, "What remains"). Step ×0.8 serial; threads up to ×(cores) on the element loops | bitwise-safe |
| 12 | Sparse direct solvers | `static_linear_eigen`: `sparselu_gemm` 71 % of the factorisation, `panel_bmod` 9 %; `static_linear_faer`: faer's **simplicial** numeric LU 92 %, factorisation 1.7× the SparseLU translation's on this 3-D problem (baseline: 4.73 against 2.77 s); `implicit_dynamic`, `static_newton_plastic`: the triangular solves (`gemv_product`, `solve`) 40–55 % | Eigen's SparseLU translated with its reduction order (bitwise default); faer runs its simplicial algorithm | default path: tighter `sparselu_gemm` kernel with the same order (SIMD across independent columns); faer: its supernodal LU (faer is an explicit, non-default choice qualified by residual, D-040); MUMPS for large problems | `sparselu_gemm` in the same order: bitwise-safe; anything reassociating: fast mode |
| 13 | Newton with plasticity keeps the elastic tangent | `static_newton_plastic`: 79 iterations per load step; the tangent is assembled once (`matrix` ≈ 0 s over 949 calls) | `MaterialPlastic` inherits the elastic law's "stiffness changed only before the first assembly" (upstream `material_elastic.hh:99`, same behaviour) | a consistent tangent changes the iteration path and results: a semantic decision, not a default-path optimisation | not bitwise (owner decision) |

## Per case: the phases and symbols that dominate

Phase times: the baseline (`baseline/summary.md`); "share" in the profile.

- `explicit_tet4_ss` / `_fd`, `explicit_tri3_*`: residual (stresses and the
  internal-force assembly) 75 % of the step, `step_hooks` (history copies)
  15 %, `step_rest` (lumped solve, integration updates, blocked-DOF
  bookkeeping) 6 %. Symbols: `fast::btd` 15 %, `fast::gradient` 13 %,
  `fast::integrate` 12 %, scatter 8 %, `grad_u_in_place` 7 %, elastic stress
  5 %, memmove 19 %, `resolve_filter` 2 %, `trace` reduction 2 %. Items 2,
  10, 11.
- `explicit_tet10_*`, `explicit_hex8_*`: `btd` 23 %, `gradient` 16–17 %,
  `integrate` 10–11 % (ss); `residuals` 28–32 % (fd); generic `trace`
  3–5 %; memmove 6–9 %. Items 3, 11.
- `explicit_plastic`, `_sls`, `_maxwell`, `_marigo`, `_neohookean`,
  `_orthotropic`: items 3 and 8 (shares above); `explicit_maxwell` spends
  70 % of its step in `step_hooks`.
- `explicit_mazars`, `explicit_multi_material`: item 6; `explicit_thermal`:
  as tet4 ss plus `compute_thermal` 2 %.
- `explicit_mazars_non_local`: items 6 and 7; `nonlocal_rebuild` 0.5 s per
  call (pair lists and weights for 48 k elements, radius 2 cells).
- `static_linear_*`: factorisation (item 12) dominates: 2.77 s per factorisation
  (Eigen translation) and 4.73 s (faer) against 63–84 ms per stiffness
  assembly and 10.6 ms per back-substitution (10,125 DOFs, 209,376
  non-zeros).
- `static_newton_plastic`: residual assembly (internal forces with the J2
  return) and the triangular solves of 949 iterations; item 13.
- `implicit_dynamic`: the factorisation and solves (58 %), `mat_vec_mul` and the hashed
  `add_matrix` (item 1).
- `setup_*`: item 1 (hashed assembly), item 5 (elemental matrices); the
  shape functions and the lumped mass are cheap (275 and 103 ns per tet4
  element). `init_full` costs 0.8 µs per tet4 element and 7.9 µs per hex8
  element (not analysed further).
- `cohesive_intrinsic_*`: bulk kernels plus `compute_traction_history` 8 %,
  generic `gradient_kernel` on cohesive elements 5 %, `libm::exp` 7 %
  (exponential law, the port's exact-libm translation); item 3.
- `cohesive_intrinsic_implicit`: factorisations 40 % and the hashed
  Jacobian assembly 25 % (item 1).
- `cohesive_extrinsic_check`, `_insertion`: item 4.

## Parallel cases (`parallel/summary.md`)

Times relative to one rank (median of the per-repetition ratios; the
8-rank runs ran in another session than the 1–4-rank runs):

| case, phase | 2 ranks | 4 ranks | 8 ranks |
|---|---|---|---|
| `par_explicit_tet4_ss`, `solve_step` (13.5 ms at 1 rank) | 0.75 | 0.59 | 0.62 |
| … `residual` (internal forces with the ghost exchanges) | 0.72 | 0.54 | 0.54 |
| … `step_hooks` (history copies; 1.05 ms per call at 1 rank) | 0.91 | 0.82 | 0.95 |
| … `step_rest` (lumped solve, synchronisations) | 0.97 | 1.23 | 1.29 |
| … `energies` (with their reductions) | 0.51 | 0.27 | 0.18 |
| `par_cohesive_extrinsic`, `step` (159 ms at 1 rank) | 0.63 | 0.37 | 0.27 |
| … `check_insertion` | 0.64 | 0.36 | 0.26 |
| … `solve_step` | 0.63 | 0.41 | 0.37 |
| `static_linear_mumps`, `solve_step` (331 ms at 1 rank) | 0.64 | 0.40 | – |

Readings:

- P1. The explicit step stops scaling after 4 ranks. Its `step_hooks`
  phase does not shrink with the rank count although each rank copies
  only its own elements' history (1/P of the data): the time is either a
  per-rank cost independent of the local size or waiting inside the
  hooks (the reported time is the largest over the ranks). `step_rest`
  grows with P (the nodal synchronisations of the lumped solve). Both are
  to be profiled per rank; the residual's plateau at 0.54 is consistent
  with the shared memory bandwidth of the element loops plus the ghost
  exchanges. Fixes of the copies (item 2) and of the exchanges are
  bitwise-safe when the summation order at shared nodes is kept.
- P2. The insertion run scales better (0.27 at 8 ranks), dominated by the
  insertion check, which is local work (item 4).
- P3. MUMPS solves the 10,125-DOF static problem in 0.33 s on one rank
  against 3.07 s for the first solve with the SparseLU translation (item
  12): for 3-D implicit problems of this size the default solver is the
  cost.

## Defects (described, not fixed in this task)

1. `FeEngine::field_matrix_elemental` (`crates/akantu-fe/src/engine/assembly.rs:207`)
   allocates `ne · nq` point matrices of `(nb_dof · nn)²` reals before
   integrating them: memory grows with elements × points × matrix size
   (4.4 GB for 35 k tetrahedron_10, out of memory at 64 k hexahedron_8 in
   8 GB). The integration needs only one element's points at a time.
2. `SparseMatrixAij` looks up every entry of every elemental block through
   a SipHash map at every assembly (`sparse.rs:597`), every entry of `B` in
   `add_matrix` (`:719`) and every non-zero's local equations in
   `mat_vec_mul` (`:864`), although the profile does not change between
   assemblies.
3. Non-local weights of constant weight functions are recomputed at every
   step with the default `update_rate = 1` (upstream behaviour,
   `NonLocalManager::compute_all_non_local_contribution`).
4. The plastic law's scratch buffer is allocated and freed at every step
   (`plastic.rs:833` `j2_block`), so its pages are faulted in again every
   step; the SLS and Maxwell profiles show the same kernel time.
5. `resolve_filter` (`crates/akantu-fe/src/engine/mod.rs:252`) rebuilds a
   `Vec<usize>` of the filter at every kernel call.

## Fast-mode candidates (not bitwise; for the fast mode, a later task)

- Closed-form 3×3 eigenvalues for Mazars (item 6): about ×0.6 of the Mazars
  step.
- SIMD/FMA `sparselu_gemm` and a supernodal ordering for the default direct
  solver (item 12): factorisation ×0.3–0.5.
- FMA in the fixed-size element kernels and vectorised per-point
  reductions (`trace`, `double_dot`): ×0.7–0.9 of the volume loops.
- Threaded scatter with per-thread partial sums (order depends on the
  thread count) for the explicit step.

## Upstream twins for C++ timings

Cases whose problem has an upstream example or test: the explicit elastic
cases (`examples/c++/solid_mechanics_model/explicit`, and the explicit
timing harness's C++ twin `scripts/perf/explicit-step/explicit_step_timing.cc`
with the same material and Δt; its meshes are Gmsh cubes), `static_linear_*`
(`examples/c++/solid_mechanics_model/static`), `implicit_dynamic`
(`examples/c++/solid_mechanics_model/implicit`), `static_newton_plastic`
(`test_material_elasto_plastic_linear_isotropic_hardening`),
`explicit_mazars_non_local` (`test_material_non_local`), the cohesive cases
(`test_cohesive_parallel_intrinsic` / `_extrinsic`, the cohesive scalability
benchmark `performance-testing-cohesive`), `par_*`
(`examples/c++/solid_mechanics_model/parallel`). A C++ twin needs the same
generated meshes (written as `.msh` by a later task) and the pulse of
`suite_solid::common::apply_pulse`.
