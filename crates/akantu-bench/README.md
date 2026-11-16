# akantu-bench

Benchmark drivers of akantu-rs (`docs/BENCHMARKS.md`).

- `cohesive_extrinsic`: the port of the Akantu JOSS cohesive scalability
  test (`benchmarks/cohesive-perf/README.md`).
- `suite_solid`, `suite_models`, `suite_setup` and
  `python/benchmarks/suite_python.py`: the whole-library benchmark suite
  of the performance campaign (`docs/DECISIONS.md`, D-122), described
  below.

## Benchmark suite

One binary per area (and one Python script), the same command line for
all; this section is the contract every suite follows:

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
  with the `mpi` feature. A case that cannot run in the build or at the
  rank count (a missing feature, a serial-only case at P > 1) prints
  `skip: <case>: <reason>` on standard error and writes no JSON line.

Output: one JSON object per line and per (case, repetition, phase), on
standard output or appended to `--out`:

```json
{"suite":"solid","case":"explicit_tet4_ss","size":"m","rep":0,"phase":"solve_step",
 "calls":200,"seconds":2.1,"n_elements":162000,"n_nodes":29791,"n_dofs":89373,
 "ranks":1,"threads":1,"digest":"9f3c…","commit":"f81a63c8c","features":"",
 "profile":"release"}
```

Fields: `suite`, `case`, `size`, `rep`, `phase`, `calls`, `seconds`,
`n_elements`, `n_nodes`, `n_dofs`, `ranks`, `threads`, `digest`, `commit`
(the commit at build time, from `build.rs`), `features`; `suite_solid`
adds `profile`. Set-up phases (`mesh`, `init`, ...) are timed once
(`calls` = 1); loop phases are totals over `calls`, after three untimed
warm-up calls. The `digest` is the 64-bit FNV-1a hash of the bits of the
case's result arrays after its last phase; it must be equal before and
after an optimisation of the default path. Some cases add fields
(`newton_iterations`, `inserted`, `n_cohesive`, `nnz`, `sparse_solver`,
`partitioner`, `seconds_min`, ...); every case with a sparse solve names
its solver in `sparse_solver` and pins it, so that the digest does not
depend on the build's features. At P > 1 ranks every rank runs the case
and rank 0 writes the lines: `seconds` is the slowest rank's time (the
maximum over the ranks), counts are global, and the `--loop-seconds`
decision is rank 0's, broadcast to the others.

Meshes are generated in code (`suite_solid::mesh`: structured boxes of
triangle_3/6, quadrangle_4, tetrahedron_4/10 and hexahedron_8, Kuhn
tetrahedra, nodes numbered in order of first use). Material inputs are
embedded `.dat` text.

The shared harness is `src/suite.rs` (command line, phase timers, digest,
JSON lines); `suite_models` and `suite_setup` still have private harnesses
with the same contract. Scripts in `scripts/perf/suite/`:

- `run_ab.sh`: interleaved A/B series of two or more suite binaries over a
  case list, one process per run, each pinned with `taskset` (or started by
  an MPI launcher), the binaries of a case back to back with their order
  rotated per repetition; `--wrap "tjob LABEL --"` runs each such group as
  one job on the measurement machine; `--rep-offset K` numbers the
  repetitions of a series split over several calls.
- `summarize.py`: medians with [min, max], per-call and per-element times,
  paired ratios against a reference binary, and whether the digests agree,
  as a Markdown table.
- `profile.sh`: `perf record --call-graph dwarf` of one case with
  `--loop-seconds`, and the text report (top symbols, call chains).
- `run_mpi.sh`: the MPI cases at several rank counts in the project's MPI
  image (one container per run, ranks bound to distinct cores, the same
  output layout as `run_ab.sh`).

Results, profiles and the ranked hot-spot list of the solid area:
`docs/benchmarks/perf-campaign/suite-solid/`.

### Solid mechanics (`suite_solid`)

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
- `static_linear_mumps` runs on the MPI world only (it skips in the
  in-process test; a `mumps` test runs the binary as a singleton). Its
  digest repeats at 1 and 2 ranks, not above.

### Models (`suite_models`)

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
| `contact_cohesive_quad4` | solid–cohesive–contact coupler: insertion check, insertions and the explicit step (at most a handful of crack-face contacts at size `m`) |
| `structural_beam3_frame_static` | 3-D Bernoulli frame: stiffness assembly, one solve |
| `structural_beam3_frame_dynamic` | 3-D Bernoulli frame: implicit Newmark steps |
| `structural_dkt18_plate_static` | DKT18 plate: stiffness assembly, one solve |
| `embedded_rebar_tri3` / `_tet4` | embedded bars: intersection set-up, assemblies with the reinforcement, one solve |
| `ntn_friction_quad4` | split-node contact with slip-weakening friction, explicit steps |

Sizes: `s` (about a second, run twice by `tests/suite_models.rs`, which checks
that every digest repeats), `m` (the timing size), `l` (about 8× `m`). Cases
of serial-only models print a `skip:` line under `mpirun -n P` with
`P > 1`. The cases with a sparse solve pin the port's translation of Eigen's
SparseLU (`sparse_solver: "eigen"`; the others print `"none"`), which is
what `_auto` resolves to in the default build: with the `mumps` feature
`_auto` would resolve to MUMPS. Parallel runs need the `mpi` feature and
the project's MPI image (`docs/TESTING.md`). IGFEM has no case here: its enrichment set-up needs the
GPL-3.0-or-later sphere intersector, and a GPL switch in this crate is a
licensing decision (D-042).

Baseline, profiles and the ranked hot spots:
`docs/benchmarks/perf-campaign/suite-models/`.

### Set-up (`suite_setup`, `python/benchmarks/suite_python.py`)

Timing cases of everything a run does besides the models' own loops:
mesh reading and writing, the derived connectivities, groups, removal,
periodicity, cohesive insertion, partitioning and the centralised
distribution, the MPI layer's exchanges and reductions, the `.dat` parser,
the dumpers, and the cost of driving a model from Python.

```text
suite_setup --list
suite_setup --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
            [--loop-seconds S] [--threads N] [--local-ranks N]
```

Output: the JSON lines of the contract above, plus `seconds_min` and
`seconds_rank0` (at P ranks `seconds` is the slowest rank) and
case-specific counts (`n_facets`, `n_cohesive`, …).

- Meshes are generated in code: the unit cube in Kuhn tetrahedra (6 n³
  Tet4) or hexahedra with boundary triangles / quadrangles, the unit square
  in triangles, each with one physical group per side and `body`. They are
  written as Gmsh text and read back, so every case sees the groups and tags
  of a Gmsh file. Sizes: Tet4 cube n = 6 / 30 / 60 (1,296 / 162,000 /
  1,296,000 elements) for `s` / `m` / `l`; Hex8 cube n = 8 / 50 / 100.
- Set-up operations (reading, facets, distribution, insertion, …) are
  set-up phases, timed once per repetition on a fresh mesh; `--reps N`
  gives the samples. Per-step operations (exchanges, reductions, dumps,
  `solve_step`, parameter evaluation) are loop phases: three warm-up calls,
  then `calls` calls under one clock. With `--loop-seconds S` the loop
  phases run for at least S seconds and a case with set-up phases only is
  repeated until S seconds have passed (for `perf record`); the digest of a
  case whose state evolves per call (`explicit_step_dist_tet4`) then
  depends on the number of calls.
- `explicit_step_dist_tet4` reports, since commit 26d0f8365, the
  agreements of the collective error protocol per call and the time spent
  in them on the fastest and the slowest rank (`agreements_per_step`,
  `agree_us_per_step_min` / `_max`, the same for the energies), measured
  outside the timed phases; its digest is unchanged.
- Digest: FNV-1a (64-bit) over the case's result arrays (mesh nodes,
  connectivities and groups; partitions; exchanged values; displacements
  and velocities; dumped file bytes). The distribution cases hash each
  rank's local mesh and combine the per-rank digests in rank order: a
  distributed mesh depends on the rank count by construction. The exchange
  and step cases gather the owned nodes' values in global-id order. The
  exchange loops digest one exchange from the initial values, so the digest
  does not depend on the number of calls.
- Parallel cases (`[mpi]` in `--list`) run at any rank count: `mpirun -n P
  suite_setup …` with the feature `mpi` (Docker images), or `--local-ranks
  P` (the in-process ranks of `akantu-comm`, for tests). Rank 0 generates
  the mesh and partitions it (Scotch with the feature `scotch`, otherwise
  RCB); at one rank the mesh stays serial (no peer to exchange with).
  Serial cases print a `skip:` line at P > 1. Run Scotch builds with
  `SCOTCH_PTHREAD_NUMBER=1`: Scotch 7 otherwise partitions with one thread
  per core and its partition changes from run to run (3 different
  partitions in 5 runs of `partition_scotch_tet4`, seed 0; see
  `crates/akantu-mesh/src/scotch.rs`), and with it every distributed
  digest; its threads would also run on the other ranks' cores.
- `py_twin_explicit_tri3` is the Rust twin of the Python case
  `explicit_tri3`: same mesh, material, initial displacement, warm-up and
  step count, then the same `solve_step_tiny` loop on a 4 × 4 mesh; both
  hash the two models' displacement and velocity and print the same digest
  when the two drivers compute the same bits.
- `cargo test -p akantu-bench --test suite_setup_s` runs every case at size
  `s` twice (and the parallel ones on two in-process ranks) and checks that
  each completes and repeats its digest.

Builds on the measurement machine: `cargo build --release -p akantu-bench
--bin suite_setup` (serial cases); the parallel cases with
`AKANTU_RS_MPI_IMAGE=akantu-rs-scotch:dev scripts/mpi/in-docker.sh cargo
build --release -p akantu-bench --bin suite_setup --features mpi,scotch`.
The Python cases need the extension built as `docs/TESTING.md` says
(`maturin develop --release` in a virtual environment) and run headless:

```text
MPLBACKEND=Agg python python/benchmarks/suite_python.py --case all --size m --reps 5
```

Results, profiles and the ranked hot-spot list:
`docs/benchmarks/perf-campaign/suite-setup/`.
