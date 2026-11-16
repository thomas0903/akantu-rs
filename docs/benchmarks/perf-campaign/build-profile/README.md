# Build configuration study (performance campaign, WP-BUILD)

Question: which build configurations (codegen units, LTO, target CPU,
PGO, `panic = "abort"`, allocator) make the port faster, and which of
them keep every result bit. Summarised in `docs/BENCHMARKS.md`, "Build
configuration"; the user-facing statement is in the manual,
`docs/manual/src/getting-started.md`, "Optimised builds and the target
CPU". Tooling: `scripts/perf/build/`.

## Result

@@RESULT@@

## Machine and method

Timing: an AWS `m7a.2xlarge` (AMD EPYC 9R14, Zen 4, 8 cores, no SMT,
32 GiB, Ubuntu 24.04), shared by several agents. Every timing job ran
under the machine's `tjob` wrapper (timing jobs one at a time on cores
0–3, builds of other agents on cores 4–7), each process pinned to core 2
(`taskset -c 2`). The `series.sh` series interleave the variants: each
repetition runs every case with every variant, adjacent, in an order
rotated by one per repetition; one process per run; 5 repetitions, each
repetition its own `tjob` of 3 to 5 minutes. Tables: median [min, max]
over the repetitions, and the median [min, max] of the per-repetition
paired ratios variant / base (`summarize.py`). A ratio whose range
contains 1 is "no measurable change". Builds running on the other four
cores share the memory system and the L3 cache with the timed process:
absolute times of different series are not compared, only the paired
ratios within a series.

Cases (`series.sh`):

| Case | Program | What |
|---|---|---|
| `es-tet4-ss`, `es-tet4-fd` | `crates/akantu-solid/examples/explicit_step_timing.rs` | explicit step on tet4-200k (178,772 tetrahedra), small strain / finite deformation; phases `step`, `fint`, `stress` (ns per element and step), 6 calls per phase after 3 warm-up steps |
| `es-hex8-ss`, `es-hex8-fd` | same | hex8-200k (195,112 hexahedra), 3 calls per phase |
| `coh` | `crates/akantu-bench` `cohesive_extrinsic` | the cohesive benchmark driver, frozen h = 0.12 mesh (`fixtures/perf_cohesive/meshes/cube-h0.12.msh`, 23,156 tetrahedra), 500 steps, static preload solved with the port's SparseLU translation; Chrono phases `static_solve`, `solve_step` (explicit step), `check_cohesive_stress` (insertion check), `init_full`; wall time; peak RSS |
| `facet` | `crates/akantu-mesh/examples/facet_scaling.rs` | mesh set-up operations at n = 32 (196,608 tetrahedra): median of 5 runs per operation as the example prints it |

Meshes: gmsh 4.12.1 (Ubuntu 24.04 package) from
`scripts/perf/explicit-step/cube_{tet,hex}.geo`; their SHA-256 equal
`docs/benchmarks/explicit-step/final/meshes-SHA256SUMS`.

Binaries: built from commit `59dd2229` (the branch point; the branch
changes only tooling and documents) with `build_variant.sh`, toolchain
1.99.1. The variants of series A to D were built in a boat.dev sandbox
(x86_64 Ubuntu 24.04, same toolchain and path) with their debug
information stripped from the copies (`strip --strip-debug`, code and
symbols unchanged) and copied to the measurement machine; series 1 used
binaries built on the measurement machine. Configurations
(`variants.sh`):

| Name | Setting on top of the workspace release profile |
|---|---|
| `base` | none (`opt-level = 3`, `debug = 1`, `lto = false`, 16 codegen units, x86-64 baseline) |
| `cgu1` | `codegen-units = 1` |
| `thin`, `fat` | `lto = "thin"`, `lto = "fat"` |
| `thin+cgu1`, `fat+cgu1` | both |
| `v2`, `v3`, `v4` | `-C target-cpu=x86-64-v2` / `-v3` / `-v4` |
| `znver4` | `-C target-cpu=znver4` (what `native` selects on the measurement machine) |
| `abort` | `panic = "abort"` |
| `base+pgouse`, `fat+cgu1+pgouse` | PGO (first training set, below) |
| `pgo2-base`, `pgo2-fat`, `pgo2-v3` | PGO (second training set) on `base`, `fat+cgu1`, `v3` |
| `@mimalloc`, `@jemalloc` | the `base` binary with `LD_PRELOAD` of Ubuntu 24.04's `libmimalloc2.0` 2.1.2 / `libjemalloc2` 5.3.0 |
| `nobc` | `base` with the bounds checks of three kernels removed by hand (`bounds-check-experiment.patch`, never merged) |

PGO (`pgo.sh`, raw `-C profile-generate` / `-C profile-use`, the
toolchain's `llvm-profdata`): one merged profile per configuration from
a training set disjoint from the measured cases. First set: the
explicit-step harness on tet4-20k, tet10-20k, hex8-20k (small strain and
finite deformation) and the cohesive driver with the `elastic` material
on the h = 0.12 mesh. It never ran the insertion check with a cohesive
material, and the measured `check_cohesive_stress` became 1.2–1.4 times
slower (series C). Second set: the same explicit-step runs and the
cohesive driver on another mesh (h = 0.15 from
`benchmarks/cohesive-perf/inputs/cube.geo`, 300 steps with the
`cohesive` material, 100 with `elastic`). `facet_scaling` is in neither
set.

@@TABLES@@

## Bitwise identity

@@BITWISE@@

## Bounds checks

@@BOUNDS@@

## Files

| File | What |
|---|---|
| `series-1.jsonl`, `series-A.jsonl` … `series-D.jsonl` | raw `series.sh` output (one JSON line per run; round lines with order, load, CPU, UTC) |
| `tables.md` | `summarize.py` over each series |
| `buildtime.jsonl`, `buildtime.md` | clean build times on the measurement machine (`bjob`, 4 cores, two rounds in rotated order) and binary sizes; `buildtime_summary.py` |
| `boat-builds.jsonl` | the sandbox builds of the timed binaries (their build times are not measurements) |
| `digests.tsv` | `result_digests.sh`: explicit-step digests (30 steps) and the 5,000-step cohesive driver results per variant |
| `ci-matrix.md` | the CI tiers built with each configuration (`ci_matrix.sh`): verdicts, test counts, the summary files |
| `minmax-probe.txt` | `minmax_probe.rs` / `minmax_compare.py` at every level and at `-O0` / `-O3` |
| `fma-holders.txt` | the functions holding FMA instructions in the `v3`, `v4`, `znver4` binaries |
| `gather-v4.txt` | gathers and scatters per function in the `v4` binary; profile of `v4` against `base` |
| `panic-calls.txt` | `panic_calls.py`: bounds-check call sites per hot function of the `base` binaries |
| `profiles/` | flat `perf` profiles of `base` (self time per symbol, sandbox CPU; shares only) |
| `bounds-check-experiment.patch` | the `nobc` scratch change |
