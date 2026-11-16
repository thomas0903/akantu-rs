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
SIZES_TABLE

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
