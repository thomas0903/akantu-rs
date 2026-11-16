# akantu-bench

Benchmark drivers of akantu-rs (`docs/BENCHMARKS.md`): the cohesive
scalability test (`cohesive_extrinsic`) and the timing suites of the
performance campaign (`docs/DECISIONS.md`, D-122).

## Set-up suite (`suite_setup`, `python/benchmarks/suite_python.py`)

Timing cases of everything a run does besides the models' own loops:
mesh reading and writing, the derived connectivities, groups, removal,
periodicity, cohesive insertion, partitioning and the centralised
distribution, the MPI layer's exchanges and reductions, the `.dat` parser,
the dumpers, and the cost of driving a model from Python.

```
suite_setup --list
suite_setup --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
            [--loop-seconds S] [--threads N] [--local-ranks N]
```

Output: one JSON object per line and per (case, repetition, phase), as the
other suites (`suite`, `case`, `size`, `rep`, `phase`, `calls`, `seconds`,
`n_elements`, `n_nodes`, `n_dofs`, `ranks`, `threads`, `digest`, `commit`,
`features`), plus `seconds_min` and `seconds_rank0` (at P ranks `seconds` is
the slowest rank) and case-specific counts (`n_facets`, `n_cohesive`, …).

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
  Serial cases print `skip:` at P > 1.
- `py_twin_explicit_tri3` is the Rust twin of the Python case
  `explicit_tri3`: same mesh, material, initial displacement, warm-up and
  step count; both print the same digest when the two drivers compute the
  same bits.
- `cargo test -p akantu-bench --test suite_setup_s` runs every case at size
  `s` twice (and the parallel ones on two in-process ranks) and checks that
  each completes and repeats its digest.

Builds on the measurement machine: `cargo build --release -p akantu-bench
--bin suite_setup` (serial cases); the parallel cases with
`AKANTU_RS_MPI_IMAGE=akantu-rs-scotch:dev scripts/mpi/in-docker.sh cargo
build --release -p akantu-bench --bin suite_setup --features mpi,scotch`.
The Python cases need the extension built as `docs/TESTING.md` says
(`maturin develop --release` in a virtual environment) and run headless:

```
MPLBACKEND=Agg python python/benchmarks/suite_python.py --case all --size m --reps 5
```

Results, profiles and the ranked hot-spot list:
`docs/benchmarks/perf-campaign/suite-setup/`.
