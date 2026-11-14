# Cohesive scalability test (Akantu JOSS benchmark): C++ harness

The C++ side of the Akantu JOSS scalability test
([performance-testing-cohesive](https://gitlab.com/akantu/performance-testing-cohesive)
at `483bace8a03850ae49ee2feb68636d2f64eae3ee`, `docs/REFERENCE.md`), built
against the oracle installs of the reference Akantu
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, `docs/ORACLE_BUILD.md`). The
port's driver is `crates/akantu-bench` (binary `cohesive_extrinsic`, same
input format). Results: `docs/BENCHMARKS.md`; data: `docs/benchmarks/`.

The driver `src/cohesive_extrinsic.cc` keeps its upstream LGPL-3.0-or-later
header. The upstream repository has no licence file, and the copied data
files (`results/published-jed/*.csv`, `inputs/cube.geo`,
`inputs/material*.dat`) carry no licence notice: they are reproduced with
attribution as the Akantu team's published reference measurements and
inputs, no licence is asserted for them, and they will be removed at the
authors' request (`NOTICE.md`).

## The test

`SolidMechanicsModelCohesive`, extrinsic insertion, on a 2 × 2 × 2 cube
(`inputs/cube.geo`): linear elastic material with `finite_deformation = true`
(E = 70 GPa, ν = 0.22, ρ = 2500) and `cohesive_linear` facets (σ_c = 70 MPa,
G_c = 7.6, β = 3) everywhere. Bottom face fixed in z, "right line" fixed in
x, local node 3 of every rank fixed in y. Compression (−0.1 MPa) on top and
bottom, shear (−60 MPa) on top, bottom and both sides. One static linear
solve (default sparse solver, MUMPS), then explicit central differences with
lumped mass, Δt = 0.05 Δt_crit, `max_steps` steps; every 100 steps the shear
grows by −1 MPa on all four surfaces; with `cohesive_insertion = true`,
`checkCohesiveStress()` runs before every step. Rank 0 prints the
accumulated time of each phase as one CSV row (the Chrono CSV) on stderr.

Upstream's `blocked_dofs(3, _y) = true` uses the local node numbering, so
with N ranks N different nodes are blocked, chosen by the partition, and
results of different rank counts are different problems (UD-349). The
driver has two modes: `blocked_node = local` (default, upstream, for JOSS
comparisons) and `blocked_node = global` (the node of global id 3: the
1-rank problem at every rank count). The comparisons with the port use
`global`.

## Layout

| Path | Content |
|---|---|
| `src/cohesive_extrinsic.cc` | upstream driver, LGPL header kept; changes marked `akantu-rs:` (below) |
| `src/CMakeLists.txt` | build against an oracle install tree (upstream file kept as `CMakeLists.upstream.txt`) |
| `src/joss_fnte_interposer.cc` | the JOSS-patch emulation (below), compiled into the executable |
| `inputs/material-{elastic,cohesive}.dat`, `inputs/material.dat` | upstream inputs, verbatim (each run's `input.dat` changes only `mesh`, `max_steps` and optional parameters) |
| `inputs/cube.geo` | upstream geometry; line 1 `h = .002;` replaced by `DefineConstant[ h = 0.002 ];` so that `gmsh -setnumber h <h>` sets it |
| `meshes/cube-h<h>.json` | statistics of the generated meshes (counts, edge lengths, SHA-256, Gmsh version and command); `cube-h<h>.<machine>.json` for a mesh generated on another architecture (Gmsh on x86_64 gives other meshes than on aarch64; frozen meshes are never replaced) |
| `scripts/` | `gen-meshes.sh`, `build.sh`, `run-series.sh` (+ `in-container/run-one.sh`), `rerun-invalid.sh`, `collect.py`, `analyze.py`, `gen-fixtures.sh` + `make_fixtures.py`, `gen-fixtures-eigen.sh` + `make_fixtures_eigen.py`, `mesh_stats.py`, `in-container/fnte_sites.py`, `in-container/build_record.py`, `in-container/validate.sh` (argument validation shared by host and container), `perfcheck.py` (run validation shared by `collect.py` and `make_fixtures.py`), `shell.sh`, `common.sh` |
| `scripts/tests/` | unit tests on synthetic runs, no Docker |
| `results/published-jed/` | the published CSVs of the upstream repository (Akantu v4.0.1 and v5.0.4 on EPFL's jed), verbatim; read by the parity tests |
| `results/<campaign>/` | where `collect.py` writes a campaign (created by the scripts; the kept campaigns are under `docs/benchmarks/`) |
| `../../fixtures/perf_cohesive/` | the correctness reference for the port's driver (strict build) and the frozen `cube-h0.12.msh` |

### Driver changes (marked `akantu-rs:` in the source)

1. Optional user parameters, inert by default: `export_partition` (true =
   the same Scotch partition with the same unit weights, built by the
   driver and written to `partition.csv`), `energy_precision` (default 6,
   upstream's stream default) and `blocked_node` (`local` or `global`).
2. `checkCohesiveStress()` returns the global number of inserted elements
   (discarded upstream); the driver keeps it (one integer comparison per
   step inside the timed loop) and writes the insertion steps to
   `insertions.csv` after the loop.
3. After the upstream Chrono output: the same CSV in `chrono.csv` (Open
   MPI's stderr forwarding once lost half a row), `summary.json` (rank 0:
   energies, displacement norms, counts, insertion steps, Δt, modes) and
   `summary_rank<r>.json` (local counts, blocked nodes, interposer
   statistics).
4. `joss_fnte_interposer.cc` (a separate file linked into the executable).

The timed phases and the Chrono CSV are unchanged, and the driver compiles
unchanged against v5.0.7-post1.

### JOSS-patch emulation (`fillNodesToElements`)

The published JOSS runs used Akantu v4.0.1 and v5.0.4 patched
(`akantu-v*.patch` of the upstream repository) to comment out
`this->fillNodesToElements()` in `Mesh::sendEvent<NewElementsEvent>` and
`Mesh::sendEvent<RemovedElementsEvent>`, a whole-mesh rebuild of the
node-to-element map on every insertion pass. The reference still calls it.
Without rebuilding Akantu, the benchmark executable defines and exports the
symbol `_ZN6akantu4Mesh19fillNodesToElementsEi`; libakantu calls it through
its PLT, so the call reaches the executable, which forwards it to the real
function (`dlsym(RTLD_NEXT)`) unless the mode says otherwise. The call site
is identified by the return address: `scripts/in-container/fnte_sites.py`
lists, at build time, every call and tail call to the PLT entry in the
installed `libakantu.so` and classifies it by its innermost inlined function
from the DWARF data. At the reference commit (aarch64 build) 10 sites are
`sendEvent<NewElementsEvent|RemovedElementsEvent>` instances (removed by the
patch) and 3 are direct calls from contact mechanics (kept).

The table fails closed: a site must resolve to one of the two `sendEvent`
instances or to one of the three named direct callers
(`ContactDetector::search`, `CohesiveSurfaceSelector::onNodesAdded`,
`AllSurfaceSelector::onNodesAdded`); anything else stops the build. The
table is bound to its library: its SHA-256 and GNU build-id are written into
the generated header, the interposer aborts at start-up when the loaded
`libakantu` has another build-id, and `run-one.sh` re-hashes the library
before every run. The emulation's own overhead (`dladdr`, a binary search
and two clock reads per call, inside the timed check) has not been measured
against a truly patched build. The v4 and v5.0.4 patches also change build
files, empty-field handling and error reporting; only the
`fillNodesToElements` part is emulated.

`AKANTU_RS_FILL_NODES_TO_ELEMENTS=upstream` (default): every call runs, and
event calls are counted and timed; `=joss` (`run-series.sh --fnte joss`):
event calls return at once. On the h = 0.12 fixture the two modes give
bitwise identical energies and insertion sequences
(`fixtures/perf_cohesive/determinism.json`).

### Invalid runs (upstream's MUMPS retry defect, UD-216)

When MUMPS stops the static factorisation with INFO(1) = −9 (workspace too
small), Akantu's `SparseSolverMumps::printError()` raises ICNTL(14) and
calls `solve()` recursively from inside `factorize()`; the outer
`solveInternal()` then solves a second time with the solution as
right-hand side, so the static solution is wrong. It happens at random from
run to run, and the run still exits with status 0. Any of Akantu's MUMPS
messages (`MUMPS dynamic memory is insufficient`, `The MUMPS workarray is
too small`, `Error/Warning in mumps during solve process`) makes a run
invalid: `run-one.sh` stops it as soon as the message appears, `collect.py`
and `make_fixtures.py` reject it, `analyze.py` excludes it and
`scripts/rerun-invalid.sh <campaign>` repeats it (the attempt is kept as
`rep<k>.invalid-<n>`; `collection.json` counts the rejected attempts and
their wall time). Timings are therefore conditional on a retry-free solve.

### Validation, provenance, completeness

- Arguments of every entry point are validated before any path is built
  (`in-container/validate.sh`): campaign names `[A-Za-z0-9][A-Za-z0-9._-]{0,63}`,
  positive counts, `h` like `0.12`, enums, `key=value` with plain-token
  values. Results are written only to `results/<campaign>/`
  (`published-jed` is reserved).
- Build records (`build.sh` → `/perf/builds/<config>/build-record.json`):
  executable, `libakantu` SHA-256 and build-id, every resolved library,
  harness and site-table digests, compile commands and flags, the sparse
  solver `_auto` selects, MPI, image id. `run-one.sh` verifies the record
  before every run (refused otherwise, `refused.txt`), copies it into the
  run directory, and `run.json` records host CPU, container CPU count,
  environment, image id and launch line.
- Plans and completeness: `run-series.sh` first writes the list of expected
  runs. `collect.py` reports as failures (exit 1, `collection.json`
  `complete: false`) every planned run that is missing, invalid or
  different from its plan; runs with different measurement identities (mesh
  digest, steps, seed, Scotch threads, interposer mode, parameters, build
  record, image, host CPU) are never combined in one table, and results of
  different machines are never ratioed.

### Single node only

Every run is one `mpiexec --oversubscribe -n N` inside one container on one
host, with the image's Open MPI transports (TCP and shared memory, no
binding). There is no hostfile and no remote launch, so N beyond the host's
cores only oversubscribes it; `run.json` records `oversubscribed` and
`collect.py` keeps such runs out of the timing tables. The port's
multi-node runs used `crates/akantu-bench/scripts/mn_cluster.py`.

### Reproducibility settings

Akantu seeds Scotch and its random generator from `time(nullptr)` unless
`AKA_SEED` is set, and the image's Scotch 7.0.7 partitions with several
threads by default, which makes partitions differ from run to run even with
a fixed seed. Every run of this harness sets `AKA_SEED=1` and
`SCOTCH_PTHREAD_NUMBER=1` (`--seed` changes the seed). With these settings a
2-rank run repeats its partition and insertion sequence exactly; its
energies still differ at the 1e-14 level from step 0, because the parallel
MUMPS static solve is not bitwise reproducible (1-rank runs are).

## Reproducing

Requirements: Docker with the oracle image `akantu-oracle:dev` and the
volume `akantu-oracle-build` holding `/build/install-default` and
`/build/install-strict` (`docs/ORACLE_BUILD.md`). Comparisons with the port
were made on x86_64. Meshes, driver builds and run directories live in the
scratch volume `akantu-perf-cohesive` (`docker volume rm
akantu-perf-cohesive` frees it). Host scripts need bash 3.2 or later and
python3 ≥ 3.9 (standard library only).

```sh
cd benchmarks/cohesive-perf
scripts/gen-meshes.sh 0.12 0.06 0.04           # Gmsh 4.13.1 in the image, twice each (byte-identical check)
scripts/build.sh default strict                # /perf/builds/<config>/cohesive_extrinsic + build-info.txt
# before insertion (one container per run, repetitions interleaved):
scripts/run-series.sh --campaign baseline --h 0.12 --steps 500
scripts/run-series.sh --campaign baseline --h 0.06 --steps 500
scripts/run-series.sh --campaign baseline --h 0.04 --steps 300 --memory 14g
# the full upstream schedule, upstream's behaviour and the JOSS emulation:
scripts/run-series.sh --campaign full-schedule --h 0.12 --steps 5000 --materials cohesive
scripts/run-series.sh --campaign full-schedule --h 0.12 --steps 5000 --materials cohesive --fnte joss
scripts/rerun-invalid.sh baseline               # repeat runs hit by UD-216 (above)
python3 -I scripts/analyze.py baseline --published    # Markdown tables
python3 -I scripts/analyze.py full-schedule
scripts/gen-fixtures.sh --steps 1500 --reps 3   # correctness reference (strict), fixtures/perf_cohesive/
scripts/shell.sh                                # shell in the image with /build, /perf, /harness
python3 -I -m unittest discover -s scripts/tests   # harness unit tests (no Docker)
bash scripts/tests/test_validate.sh             # shell validators
python3 -I scripts/make_fixtures.py --check     # offline check of fixtures/perf_cohesive/
```

`run-series.sh` options: `--config default|strict`, `--materials "elastic
cohesive"`, `--ranks "1 2 4 8"`, `--reps 3`, `--memory 8g` (container RAM
limit, no swap), `--fnte upstream|joss`, `--seed N`, and `-- key=value ...`
for user-section parameters (for example `-- output_energy=true` or `--
blocked_node=global`). A variant lands in
`timmings_<material>_<config>[-fnte_joss][-global]_h<h>.csv`. Each run:
`mpiexec --oversubscribe -n <N>` with `OMP_NUM_THREADS=1` and the image's
Open MPI settings, GNU `time`, a 0.5 s memory trace of the container and
the cgroup memory peak. A failed run does not stop the series; `collect.py`
reports it, and `run-series.sh` then exits with status 1.

The port against this harness on the same machine, mesh and partition: the
campaign tools in `crates/akantu-bench/scripts/` (`timing_ab.sh` for serial
pairs, `c1prep_campaign.sh` for parallel pairs with MUMPS, `scaling_series.sh`
for the scaling series; usage in their headers and `docs/TESTING.md`).

The JOSS mesh (`cube.msh`, a Git-LFS object of the upstream repository:
734,594 nodes, 4,392,183 tetrahedra, SHA-256 `5ad845c7…`) corresponds to
h = 0.02 with this Gmsh (100 segments per cube edge), not to the `h = .002`
written in the upstream `cube.geo`; this Gmsh does not reproduce it, so the
file itself is used.
