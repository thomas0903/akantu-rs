# akantu-bench

Benchmark drivers of akantu-rs.

- `cohesive_extrinsic`: the port of the Akantu JOSS cohesive scalability
  test (`benchmarks/cohesive-perf/README.md`, `docs/BENCHMARKS.md`).
- `suite_solid` (and the other `suite_*` binaries): the whole-library
  benchmark suite described below.

## Benchmark suite

One binary per area, the same command line for all:

```text
suite_<area> --list
suite_<area> --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
             [--loop-seconds S] [--threads N]
```

- `--size s`: every case under about a second (the crate's test runs each
  case twice at `s` and checks that its digest repeats); `m`: the timing
  size; `l`: about 8× `m`, for scaling studies.
- `--reps N`: each repetition runs the whole case again from a new mesh
  and model, so the digest does not depend on `N`.
- `--loop-seconds S`: the case's main loop runs for at least `S` seconds
  (for `perf record`); the digest then describes that longer run.
- `--threads N`: recorded only (the library has no threads yet).
- MPI cases take their rank count from `mpirun -n`; the binary must be built
  with the `mpi` feature. A case that needs a feature the binary lacks
  prints `skip: <case>: <reason>` on standard error.

Output: one JSON object per line and per (case, repetition, phase), on
standard output or appended to `--out`:

```json
{"suite":"solid","case":"explicit_tet4_ss","size":"m","rep":0,"phase":"solve_step",
 "calls":200,"seconds":2.1,"n_elements":162000,"n_nodes":29791,"n_dofs":89373,
 "ranks":1,"threads":1,"digest":"9f3c…","commit":"f81a63c8c","features":"",
 "profile":"release"}
```

Set-up phases (`mesh`, `init`, ...) are timed once (`calls` = 1); loop
phases are totals over `calls`, after three untimed warm-up calls. The
`digest` is the 64-bit FNV-1a hash of the bits of the case's result arrays
after its last phase; it must be equal before and after an optimisation of
the default path. Some cases add fields (`newton_iterations`, `inserted`,
`n_cohesive`, `nnz`, `sparse_solver`, `partitioner`). At P > 1 ranks each
phase reports the largest time over the ranks, and counts are global.

Meshes are generated in code (`suite_solid::mesh`: structured boxes of
triangle_3/6, quadrangle_4, tetrahedron_4/10 and hexahedron_8, Kuhn
tetrahedra, nodes numbered in order of first use). Material inputs are
embedded `.dat` text.

The shared harness is `src/suite.rs` (command line, phase timers, digest,
JSON lines). Scripts in `scripts/perf/suite/`:

- `run_ab.sh`: interleaved A/B series of two or more suite binaries over a
  case list, one process per run, each pinned with `taskset` (or started by
  an MPI launcher), the binaries of a case back to back with their order
  rotated per repetition; `--wrap "tjob LABEL --"` runs each such group as
  one job on the measurement machine.
- `summarize.py`: medians with [min, max], per-call and per-element times,
  paired ratios against a reference binary, and whether the digests agree,
  as a Markdown table.
- `profile.sh`: `perf record --call-graph dwarf` of one case with
  `--loop-seconds`, and the text report (top symbols, call chains).

### Benchmark suite: solid mechanics (`suite_solid`)

The cases cover the explicit step per element type and per constitutive
law, the implicit solvers, the model set-up, cohesive elements and two
parallel runs. `suite_solid --list` prints them with the cost each one
exposes. Elastic parameters are those of the explicit timing harness
(`rho = 2500`, `E = 70e9`, `nu = 0.22`, `Δt = 0.8 Δt_crit`).

| case | problem at size `m` | main phases | upstream example with the same problem |
|---|---|---|---|
| `explicit_tet4_ss`, `_fd` | tetrahedron_4, 30³ cells: 162,000 elements, 89,373 DOFs; 200 / 150 steps | solve_step (+ split), internal_forces, stresses, energies, stable_time_step | examples/c++/solid_mechanics_model/explicit (explicit_dynamic.cc), the explicit timing harness |
| `explicit_tet10_ss`, `_fd` | tetrahedron_10, 26³ cells: 105,456 elements, 446,631 DOFs; 50 / 40 steps | as above | as above (tet10 mesh of the harness) |
| `explicit_hex8_ss`, `_fd` | hexahedron_8, 48³ cells: 110,592 elements, 352,947 DOFs; 50 / 30 steps | as above | as above (hex8 mesh of the harness) |
| `explicit_tri3_ss`, `_fd` | triangle_3, 300² cells: 180,000 elements, 181,202 DOFs; 300 / 200 steps | as above | – |
| `explicit_plastic`, `_marigo`, `_mazars`, `_sls`, `_maxwell`, `_neohookean`, `_orthotropic`, `_thermal` | tetrahedron_4, 26³ cells: 105,456 elements, 59,049 DOFs; 40–300 steps under the stretch ramp | as above | test/test_model/test_solid_mechanics_model/test_materials (the laws' inputs) |
| `explicit_multi_material` | the same mesh, elastic / plastic / Mazars in three slabs of x; 120 steps | as above | – |
| `explicit_mazars_non_local` | tetrahedron_4, 20³ cells: 48,000 elements; base_wf radius 2 cells; 40 steps | as above + nonlocal_rebuild | test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local, test/test_model/test_common/test_non_local_toolbox |
| `static_linear_eigen`, `_faer`, `_mumps` | clamped tetrahedron_4 cube, 14³ cells: 16,464 elements, 10,125 DOFs (209,376 non-zeros) | solve_step (+ split), stiffness_assembly, analysis, factorize, back_substitution | examples/c++/solid_mechanics_model/static (static.cc) |
| `static_newton_plastic` | clamped tetrahedron_4 cube, 10³ cells, plasticity, 12 load steps (949 Newton iterations) | solve_step (+ split) | test_material_elasto_plastic_linear_isotropic_hardening |
| `implicit_dynamic` | free tetrahedron_4 cube, 14³ cells, trapezoidal rule, Δt = 4 Δt_crit, 60 steps | mass_consistent, solve_step (+ split) | examples/c++/solid_mechanics_model/implicit (implicit_dynamic.cc) |
| `setup_tet4`, `_tet10`, `_hex8` | 162,000 tet4 / 10,368 tet10 / 13,824 hex8 elements | mesh, init_explicit, shape_functions, mass_lumped, init_implicit, mass_consistent, stiffness_assembly | – |
| `cohesive_intrinsic_bilinear`, `_exponential` | tetrahedron_4, 20³ cells, 19,680 cohesive elements in the slab 0.4 ≤ x ≤ 0.6; 300 steps | init, solve_step, energies | test_cohesive_parallel_intrinsic (materials) |
| `cohesive_intrinsic_implicit` | triangle_6 on [0, 1] × [0, 2], 30 × 60 cells, cohesive line y = 1; 30 load steps | solve_step | fixtures/model/cohesive/implicit_static_bilinear_triangle_6 (probe) |
| `cohesive_extrinsic_check` | tetrahedron_4, 24³ cells: 82,944 elements, every facet checked, no insertion; 100 steps | check_insertion, solve_step | the cohesive scalability benchmark (cohesive_step_timing) |
| `cohesive_extrinsic_insertion` | tetrahedron_4, 20³ cells: 48,000 elements; 60 preload steps, then 16 timed steps with about 15,000 insertions | preload, step = check_insertion + solve_step | performance-testing-cohesive cohesive_extrinsic.cc (laws), test_cohesive_parallel_extrinsic |
| `par_explicit_tet4_ss` | explicit_tet4_ss on the distributed cube (mpirun -n P) | mesh, distribute, init, solve_step (+ split), energies | examples/c++/solid_mechanics_model/parallel (with explicit_dynamic.cc) |
| `par_cohesive_extrinsic` | cohesive_extrinsic_insertion on the distributed cube (mpirun -n P) | mesh, distribute, init, preload, step, check_insertion, solve_step | performance-testing-cohesive (JOSS scalability) |

Notes:

- Explicit elastic cases start from a sine–Gaussian displacement pulse
  along x spread over a fifth of the cube (the explicit example's pulse is
  narrower than one element of these meshes); the other laws run under a
  stretch ramp (both x faces moved every step, linear initial velocity)
  that takes them into their inelastic regime during the run.
- Every solver step of the solid model is split into the solver callbacks
  through the model's intercepting callback (`solve_step_intercepted`, which
  delegates every call): `predictor`, `residual` (internal forces and the
  residual assembly), `matrix` (stiffness, mass, Jacobian), `lumped_matrix`,
  `corrector`, `step_hooks` (before/after the step) and `step_rest` (the
  step minus those: the linear or lumped solve and the solver's own work).
  The crate's test checks that this split computes the bits of
  `solve_step`. The cohesive model's step is timed whole.
- `static_linear_*` time the first solve (`solve_step`, assembly included),
  then drive the direct solver directly on the assembled matrix `J`:
  `analysis`, `factorize`, `back_substitution` (Eigen's SparseLU
  translation and faer; MUMPS only through `solve_step`).
- The parallel cases (`par_*`) build the cube on rank 0 and distribute it
  (RCB, or Scotch with the `scotch` feature). Their digests gather the
  owned nodes' `u` and `v` in global-id order: they repeat at a fixed rank
  count and differ between rank counts (sums at shared nodes, numbering of
  doubled nodes).
Benchmark drivers of akantu-rs (`docs/BENCHMARKS.md`): the cohesive
scalability test (`cohesive_extrinsic`) and the timing suites of the
performance campaign (`docs/DECISIONS.md`, D-122).

## Model suite (`suite_models`)

Timing cases for the models other than plain solid mechanics and cohesive
elements. Every case generates its mesh in code (structured grids written as
Gmsh MSH 2.2 text and read with `akantu_io::read_mesh_from_str`, so the
models see the element groups and `physical_names` data of a Gmsh mesh), runs
through the public API, prints one JSON object per phase and hashes its
results (FNV-1a over the result arrays after the last step).

```text
cargo build --release -p akantu-bench --bin suite_models
suite_models --list
suite_models --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
             [--loop-seconds S] [--threads N]
SUITE_VERBOSE=1 suite_models ...   # counts that show what a case did (stderr)
```

| Case | Exposes |
|---|---|
| `heat_explicit_tet4` | explicit lumped heat steps, 3-D tet4 (distributed under `mpirun`) |
| `heat_explicit_quad4` | explicit lumped heat steps, 2-D quad4 |
| `heat_implicit_hex8_kvar` | backward Euler, `T`-dependent conductivity: Newton, assembly and solve per iteration |
| `heat_static_tri6` | conductivity assembly and one static solve, 2-D tri6 |
| `phasefield_staggered_tri3` | phase field + solid staggered step, each piece a phase; both stiffness assemblies (distributed under `mpirun`) |
| `phasefield_staggered_tet4` | the same in 3-D |
| `contact_search_quad4` / `_tet4` | contact detection alone, 2-D / 3-D, many surface elements |
| `contact_explicit_quad4` | solid–contact coupler, explicit penalty steps |
| `contact_implicit_static_quad4` | solid–contact coupler, static Newton steps with the contact stiffness |
| `contact_cohesive_quad4` | solid–cohesive–contact coupler: extrinsic insertion, then crack-face contact |
| `structural_beam3_frame_static` | 3-D Bernoulli frame: stiffness assembly, one solve |
| `structural_beam3_frame_dynamic` | 3-D Bernoulli frame: implicit Newmark steps |
| `structural_dkt18_plate_static` | DKT18 plate: stiffness assembly, one solve |
| `embedded_rebar_tri3` / `_tet4` | embedded bars: intersection set-up, assemblies with the reinforcement, one solve |
| `ntn_friction_quad4` | split-node contact with slip-weakening friction, explicit steps |

Sizes: `s` (about a second, run twice by `tests/suite_models.rs`, which checks
that every digest repeats), `m` (the timing size), `l` (about 8× `m`). Cases
of serial-only models print `skip: <reason>` under `mpirun -n P` with
`P > 1`. Parallel runs need the `mpi` feature and the project's MPI image
(`docs/TESTING.md`). IGFEM has no case here: its enrichment set-up needs the
GPL-3.0-or-later sphere intersector, and a GPL switch in this crate is a
licensing decision (D-042).

Baseline, profiles and the ranked hot spots:
`docs/benchmarks/perf-campaign/suite-models/`.
