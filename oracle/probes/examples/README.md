# Oracle runs of the upstream examples

The Rust examples of `crates/akantu/examples/` port `examples/c++/**` of the
reference. Their integration tests (`crates/akantu/tests/examples_*.rs`)
compare the Rust runs with oracle runs recorded in `fixtures/examples/<case>/`.

## Cases (`cases.tsv`)

Each case runs either the **upstream example binary** of the oracle build
tree (`/build/<config>/examples/c++/<dir>/<name>`, unchanged) or an **oracle
variant** built here from a copy of the upstream source with the changes
marked `akantu-rs:`:

| Variant | Change | Why |
|---|---|---|
| `static_eigen`, `example_parser_eigen`, `implicit_dynamic_eigen` | `_solver_options` pins `SparseSolverType::_eigen` | upstream `_auto` resolves to MUMPS in the oracle; the port's SparseLU translation is bitwise against Eigen's |
| `implicit_dynamic_eigen` | `position_full.csv` = `position.csv` with 17 digits | bitwise comparison of every step |
| `implicit_dynamic_eigen_state` | `implicit_dynamic_eigen` + `state_sha256.txt`: per step, SHA-256 of the raw little-endian bytes of displacement, velocity and acceleration, concatenated (`akantu_rs_sha256.hh`, FIPS 180-4, self-tested at start) | exact comparison of the full state of every step |
| `explicit_dynamic_full` | `energy_full.csv` = `energy.csv` with 17 digits | bitwise energies of every step |
| `new_local_material_full` | `energy_full.csv`: `s, epot, ekin` of every step, 17 digits | bitwise energies (the example prints 6 digits every 100 steps) |
| `parallel_2d_partition` | `mesh.distribute()` written out (Scotch partition on rank 0, default weights, then `distribute(partition)`) and the partition saved to `partition.txt` | the port replays the oracle's partition (D-009) |
| `cohesive_extrinsic_full`, `cohesive_intrinsic_full`, `cohesive_extrinsic_ig_tg_full` | `Eigen::setCpuCacheSizes(16384, 524288, 524288)` at the start of `main` (D-050); `energy_full.csv`: after every step the number of cohesive elements and the dissipated, reversible, potential and kinetic energies, 17 digits | bitwise energies of every step; the variant's dumps must equal the upstream binary's (checked by the tests: the pin changes nothing in these explicit runs) |
| `cohesive_extrinsic_ig_tg_vel` | `cohesive_extrinsic_ig_tg_full` with the initial velocity `0.05 x` instead of `0.1 x` | the blocked side DOFs start at ±0.05, so the `Velocity` functor's write of ±0.1 changes the state (in the upstream example it rewrites the initial value bitwise) |
| `scalability_test_full` (cases `scalability_test_full`, `scalability_test_cohesive_full`) | Eigen cache sizes pinned (D-050); `energies.csv` with 17 digits; `cohesive_full.csv` = cohesive elements after every step; run with the input files `scalability_{elastic,cohesive}_full.dat` of this directory (`output_energy = true`, `sparse_solver_type = eigen` in the `static` solver section) | bitwise static solve, time step and energies; upstream `_auto` resolves to MUMPS |
| `scalability_test_full` with the input files `scalability_{elastic,cohesive}_paraview.dat` (cases `scalability_test_paraview`, `scalability_test_cohesive_paraview`) | `output_paraview = true` (both upstream input files switch the dumps off): the solid model's `extrinsic` series and the `cohesive elements` dumper's `solid_mechanics_model_cohesive` series, 52 dumps each | the example's ParaView output compared file by file (`examples_scalability_test`); the dumps change no value (`energies.csv` equals the `_full` cases') |
| `heat_diffusion_static_2d_eigen`, `contact_explicit_static_eigen`, `material_viscoelastic_maxwell_energies_eigen`, `phase_field_notch_eigen`, `phase_field_parallel_eigen` (case `phase_field_parallel_eigen_np1`, one rank) | Eigen sparse solver pinned (both models for the phase-field ones), cache sizes pinned (D-050); `newton_full.csv` (contact: contact elements, Newton iterations and error per step), `output_full.out` (viscoelastic: the output line with 17 digits plus iterations and error), `state_sha256.txt` (phase field: SHA-256 of displacement and damage after every load step) | bitwise solves and state; upstream `_auto` is MUMPS |
| `bernoulli_beam_2_example_static` | `initFull(_analysis_method = _static)` with the Eigen solver pinned (UD-514: upstream's `initFull()` is an explicit step with a zero mass); `values_full.txt`: the printed values, displacement, stresses and internal forces with 17 digits | the example's evident intent, bitwise |
| `contact_explicit_dynamic_full`, `cohesive_contact_explicit_dynamic_full`, `bloc_friction_full`, `steady_state_full` | cache sizes pinned; every 20th / 50th / 30th / every step: element counts, energies (17 digits) and the SHA-256 of the raw state arrays (`energy_full.csv`, `energies_full.csv`) | the full state along the run within the 1 MiB budget; the variants' dumps equal the upstream binaries' |
| `phase_field_parallel_partition` (cases `phase_field_parallel_partition`, `_np4`) | `mesh.distribute()` written out with the partition recorded (`partition.txt`, as `parallel_2d_partition`); per rank `state_sha256.proc<r>.txt` | references of the 2- and 4-rank runs (nondeterministic: distributed MUMPS) |
| `embedded`, `embedded_eigen` (`emb:` binaries) | the upstream example and its Eigen-pinned copy built against the patched embedded variant (D-060, `oracle/patches/embedded`, CMake project `embedded/`; the runner applies the source patches to copies) | **not promoted**: the upstream example crashes in `initFull` (UD-851); rows commented in `cases.tsv` |
| `embedded_eigen_p1` (`emb:embedded_eigen`) | `embedded_eigen` on the first-order version of the example's concrete mesh (input `probe:embedded_p1/concrete.msh`, written by `embedded_p1/derive_first_order.py` from the frozen second-order mesh); the variant computes UD-1140's memory-safety precondition (no regular 1-D element type other than `_segment_2` in the background mesh), prints it and stops (exit 3) when it fails | the oracle reference of EX-008 on the input where upstream is memory-safe (UD-851's root cause is UD-1140: the second-order mesh's `_segment_3` boundary elements) |

Every case runs in a fresh directory holding the upstream input files and
the frozen Gmsh meshes (`stderr.txt` without the cache-pin lines is
kept too: the contact examples print their energies there) (`fixtures/meshes/gmsh-4.13.1/examples/c++`, D-009),
**three times** (D-035), with `AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1`; 2-rank
cases under `mpirun -np 2`. additions: an input `probe:<file>` is
copied from this directory (variant input files), `-` in the meshes column
means none (upstream's committed meshes are inputs), an optional ninth column
gives the binary's arguments (e.g. `--aka_input_file material-cohesive.dat`),
`OPENBLAS_CORETYPE=Haswell` pins OpenBLAS (D-050), and the provenance of a
variant that pins Eigen's cache sizes says so.

**D-050 pins.** Eigen's cache sizes are pinned to
16384 / 524288 / 524288 bytes in every run: the variants call
`Eigen::setCpuCacheSizes` at the start of `main`, and every binary (variants
and unmodified upstream examples) runs with `LD_PRELOAD` of
`eigen_cache_pin.so` (`eigen_cache_pin.cc`), which pins before `main`,
aborts if the sizes did not take, and writes the effective sizes to stderr
(the runner fails a run whose stderr lacks that line). BLAS runs with
`OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1`. Both are recorded in
`provenance.txt`.

## Fixture layout (`fixtures/examples/<case>/`)

| File | Content |
|---|---|
| `digests.txt` | SHA-256 of **every** output file of run 1 (dumps, CSV files) |
| `NONDETERMINISTIC` | output files that differ between the three runs (absent if none) |
| `files/<path>` | output files kept verbatim (`keep` column; ≤ 1 MiB per case) |
| `stdout.txt`, `status.txt` | standard output of run 1; exit codes, command, counts |
| `provenance.txt` | platform, image id, oracle config and scripts commit, reference, generating command, date |

`fixtures/examples/SHA256SUMS` indexes the tree; the entries are also merged
into `fixtures/SHA256SUMS`.

## Running

On an x86_64 Linux host with the oracle image and its build volume
(`docs/ORACLE_BUILD.md`), in a clone of this repository:

```sh
AKANTU_SRC_HOST=/path/to/akantu oracle/probes/examples/run.sh --out ~/stage
# subsets / fewer runs while iterating:
EXAMPLES_CASES=static,static_eigen EXAMPLES_RUNS=1 AKANTU_SRC_HOST=/path/to/akantu \
  oracle/probes/examples/run.sh --out ~/stage
```

`EXAMPLES_STAGE_ON_FAILURE=1` copies the staged cases to
`<out>/examples-failed/` when a case fails (nothing is promoted otherwise).
`run.sh` refuses to replace an existing `<out>/examples/<case>` directory
unless `--force` is given: generate into a scratch `--out` and compare with
the committed fixtures (goldens are never replaced to hide a change).
