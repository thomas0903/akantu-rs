# Build configuration study (performance campaign)

Question: which build configurations (codegen units, LTO, target CPU,
PGO, `panic = "abort"`, allocator) make the port faster, and which of
them keep every result bit. Summarised in `docs/BENCHMARKS.md`, "Build
configuration"; the user-facing statement is in the manual,
`docs/manual/src/getting-started.md`, "Optimised builds and the target
CPU". Tooling: `scripts/perf/build/`.

## Result

- **Release profile: no change.** One codegen unit, thin LTO, fat LTO and
  their combinations do not make the port faster: one codegen unit and
  fat LTO make the explicit step 6–10 % slower on tet4. Thin LTO is
  within ±1.5 % everywhere. Fat LTO without one codegen unit gains 2–5 %
  in the insertion check, the static solve and the mesh set-up. The
  clean build takes 1.3× (one codegen unit or thin LTO), 1.7× (both) and
  2.3–2.5× (fat LTO) as long. `Cargo.toml` keeps its profile.
- **Target CPU: bitwise at every level; `x86-64-v3` / `native` gain
  2–4 %, `x86-64-v4` loses up to 37 %.** The CI `test` and `release`
  tiers pass built with `x86-64-v2`, `-v3`, `-v4` and `znver4`; every
  result digest is identical. `-v3` and `znver4` take 0.96–0.98 of the
  explicit-step time and 0.98 of the cohesive driver's wall time,
  but 1.03–1.05 of the facet set-up's. `-v4` takes 1.19–1.37 times the
  hex8 step: LLVM's generic v4 cost model vectorises the strided loads of
  `fast::gradient` with AVX-512 gathers and scatters, which are slow on
  AMD Zen 4. The manual states this (`docs/manual/src/getting-started.md`).
- **PGO: the largest gain, bitwise, but only where the training reaches.**
  Trained on cases disjoint from the measured ones (second training set):
  cohesive driver 0.917 [0.914, 0.922] of the wall time (0.904 with fat
  LTO), insertion check 0.85–0.89, explicit step of the driver 0.84–0.85,
  explicit-step harness 0.94–0.98, the untrained mesh set-up 0.96. A
  training set without the cohesive material (first set) made the
  insertion check 1.35 times slower. PGO combined with `-v3` slowed the
  tet4 step (1.04–1.06). On the benchmark suites (not trained) PGO gains
  in the solvers and model set-up and loses in some untrained laws and
  writers (table below). Not adopted: it needs a training workflow in
  the packaging (candidate, report).
- **`panic = "abort"`: no measurable change** (0.98–1.00); not proposed
  (it would turn the Python bindings' `PanicException`, the agreed fatal
  error of a panicking native-solver callback in `akantu-solver`, and the
  PETSc callbacks' panic barrier into process aborts).
- **Allocators: no gain.** mimalloc and jemalloc (preloaded) slow the
  explicit step by 2–10 %; mimalloc lowers the driver's peak RSS by 11 %
  and makes element removal 0.65 of the time. Not proposed.
- **Bounds checks** in three kernels cost 10 % of the tet4 small-strain
  step (section "Bounds checks").
- `opt-level` per package: not measured. The profiles show no
  code-size-bound hot path, and the configurations that change inlining
  (one codegen unit, LTO) did not gain, so per-crate inlining thresholds
  have no measured case.

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
ratios within a series. The first repetition of series 1 ran before the
machine's wrappers took their present form (06:40 UTC), when a timing
job had the whole machine to itself.

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

## Timings

Paired ratio variant / `base` of the same series, median [min, max] over
5 repetitions (`overview.py`); main metric per case: `step` of the
explicit-step harness (ns per element and step), the driver's Chrono
phases and wall time, the sum of the five `facet_scaling` operations at
n = 32. Series: A (`cgu1` … `fat+cgu1`), B (`v2` … `znver4`), C
(`abort`, first-set PGO), D (second-set PGO), 1 (allocators, `nobc`).
Every phase, the absolute times and the peak RSS: `tables.md`.

| configuration | tet4 ss step | tet4 fd step | hex8 ss step | hex8 fd step | driver explicit step | driver insertion check | driver static solve | driver wall | facet set-up |
|---|---|---|---|---|---|---|---|---|---|
| `cgu1` | 1.098 [1.074, 1.103] | 1.061 [1.057, 1.073] | 1.019 [1.013, 1.031] | 0.999 [0.996, 1.009] | 1.034 [1.016, 1.037] | 0.971 [0.960, 0.988] | 0.980 [0.974, 0.993] | 0.981 [0.978, 0.993] | 0.984 [0.983, 0.984] |
| `thin` | 0.991 [0.986, 1.013] | 0.997 [0.987, 1.003] | 0.999 [0.982, 1.006] | 0.996 [0.991, 1.002] | 1.003 [0.997, 1.010] | 0.999 [0.991, 1.012] | 0.996 [0.988, 1.016] | 0.998 [0.994, 1.013] | 0.987 [0.987, 0.989] |
| `fat` | 1.101 [1.085, 1.112] | 1.072 [1.056, 1.080] | 1.025 [1.021, 1.032] | 1.007 [1.000, 1.024] | 1.036 [1.032, 1.060] | 0.948 [0.941, 0.970] | 0.975 [0.969, 0.989] | 0.970 [0.967, 0.981] | 0.968 [0.966, 0.970] |
| `thin+cgu1` | 1.081 [1.064, 1.098] | 1.059 [1.048, 1.063] | 1.010 [0.994, 1.012] | 1.008 [0.994, 1.012] | 1.027 [1.020, 1.046] | 0.990 [0.982, 1.018] | 0.986 [0.984, 1.001] | 0.992 [0.987, 1.012] | 0.984 [0.983, 0.986] |
| `fat+cgu1` | 1.088 [1.077, 1.108] | 1.073 [1.060, 1.083] | 1.012 [0.984, 1.019] | 1.012 [0.992, 1.015] | 1.031 [1.027, 1.056] | 1.043 [1.030, 1.046] | 0.986 [0.978, 1.014] | 1.012 [1.008, 1.028] | 0.973 [0.971, 0.977] |
| `v2` | 1.001 [0.987, 1.014] | 0.984 [0.981, 0.991] | 1.011 [1.007, 1.031] | 0.964 [0.938, 0.973] | 0.963 [0.956, 0.976] | 1.002 [0.997, 1.016] | 1.024 [1.010, 1.105] | 1.014 [1.001, 1.045] | 1.007 [1.006, 1.031] |
| `v3` | 0.964 [0.960, 1.022] | 0.977 [0.969, 0.994] | 0.964 [0.959, 0.965] | 0.965 [0.934, 0.968] | 0.972 [0.949, 0.974] | 0.970 [0.958, 0.987] | 0.995 [0.973, 1.017] | 0.984 [0.973, 0.997] | 1.052 [1.051, 1.089] |
| `v4` | 1.010 [0.983, 1.081] | 1.011 [1.006, 1.022] | 1.367 [1.356, 1.382] | 1.193 [1.177, 1.198] | 0.991 [0.987, 1.012] | 0.935 [0.932, 0.958] | 0.992 [0.985, 1.002] | 0.972 [0.966, 0.983] | 1.052 [1.050, 1.055] |
| `znver4` | 0.968 [0.930, 0.998] | 0.981 [0.968, 1.014] | 0.962 [0.945, 0.968] | 0.967 [0.939, 0.974] | 1.011 [0.969, 1.029] | 0.940 [0.922, 0.964] | 1.023 [1.014, 1.030] | 0.984 [0.979, 1.000] | 1.029 [1.027, 1.031] |
| `abort` | 0.996 [0.978, 1.004] | 0.995 [0.986, 1.002] | 0.996 [0.981, 1.000] | 1.003 [1.002, 1.005] | 0.995 [0.994, 1.000] | 0.978 [0.972, 0.992] | 0.997 [0.991, 1.031] | 0.988 [0.984, 1.015] | 0.977 [0.971, 1.029] |
| `base+pgouse` | 0.942 [0.923, 0.944] | 0.960 [0.935, 0.964] | 0.962 [0.960, 0.977] | 0.975 [0.970, 0.977] | 0.841 [0.837, 0.848] | 1.347 [1.332, 1.389] | 0.936 [0.926, 0.969] | 1.095 [1.089, 1.132] | 0.965 [0.963, 0.968] |
| `fat+cgu1+pgouse` | 0.931 [0.924, 0.943] | 0.940 [0.906, 0.943] | 0.960 [0.958, 0.982] | 0.969 [0.968, 0.976] | 0.837 [0.819, 0.842] | 1.230 [1.202, 1.253] | 0.948 [0.940, 0.951] | 1.053 [1.039, 1.059] | 0.935 [0.932, 0.937] |
| `pgo2-base` | 0.950 [0.938, 0.963] | 0.962 [0.956, 0.974] | 0.981 [0.975, 1.011] | 0.975 [0.973, 0.980] | 0.854 [0.845, 0.866] | 0.887 [0.852, 0.906] | 0.957 [0.934, 0.992] | 0.917 [0.914, 0.922] | 0.958 [0.955, 0.966] |
| `pgo2-fat` | 0.936 [0.921, 0.952] | 0.952 [0.943, 0.955] | 0.981 [0.974, 1.019] | 0.966 [0.960, 0.981] | 0.843 [0.834, 0.862] | 0.847 [0.811, 0.867] | 0.968 [0.953, 0.983] | 0.904 [0.882, 0.923] | 1.033 [1.031, 1.041] |
| `pgo2-v3` | 1.058 [1.047, 1.081] | 1.044 [1.030, 1.058] | 0.976 [0.931, 0.990] | 0.968 [0.959, 0.969] | 0.905 [0.895, 0.937] | 0.849 [0.811, 0.868] | 0.948 [0.930, 0.961] | 0.903 [0.878, 0.921] | 0.961 [0.958, 0.970] |
| `base@mimalloc` | 1.040 [1.020, 1.070] | 1.019 [1.010, 1.030] | 1.052 [1.027, 1.062] | 1.100 [1.089, 1.106] | 1.008 [0.952, 1.035] | 1.003 [0.990, 1.045] | 1.015 [0.995, 1.026] | 1.006 [0.996, 1.033] | 0.958 [0.949, 0.963] |
| `base@jemalloc` | 1.035 [1.019, 1.066] | 1.027 [1.013, 1.029] | 1.025 [1.007, 1.047] | 1.078 [1.039, 1.099] | 1.018 [0.993, 1.049] | 1.000 [0.986, 1.034] | 1.034 [1.019, 1.087] | 1.017 [1.003, 1.059] | 1.005 [0.999, 1.012] |
| `nobc` | 0.899 [0.880, 0.920] | 0.938 [0.934, 0.949] | 0.975 [0.971, 1.026] | 0.992 [0.981, 1.005] | 0.938 [0.919, 0.958] | 0.999 [0.995, 1.006] | 0.997 [0.994, 1.002] | 0.997 [0.986, 0.999] | 1.001 [0.994, 1.005] |

Per phase (`tables.md`): with `-v4` the hex8 `stress` phase (gradient and
constitutive law) takes 2.08–2.14 times the base time and `fint` 1.23–1.48;
with `-v3` `fint` takes 0.94–0.97 on tet4 small strain and hex8. The
peak RSS of the driver is unchanged by every build configuration
(±1 %); mimalloc lowers it to 0.890, jemalloc to 0.877.

Absolute medians of `base` (series D): tet4-200k step 93.4 ns (ss),
134.0 ns (fd); hex8-200k 610.6 ns (ss), 1,100 ns (fd) per element and
step; driver wall 16.90 s (static solve 7.65 s, 500 explicit steps
1.83 s, insertion checks 6.89 s); `facet_scaling` n = 32 2.335 s.

### Benchmark suites (not trained by PGO)

Fifteen cases of the benchmark suites (branches `perf/suite-solid`
`c698395e`, `perf/suite-models` `ee4140ce`, `perf/suite-setup`
`c4a87945`, size `m`; one case per area, chosen from a calibration of all
71 cases), each binary built as `base`, `v3` and PGO. The PGO build
reuses the profile of the second training set: the suites were not
trained, so this measures how far that training carries into the rest
of the library. `suite_series.sh` (one repetition of one suite per
`tjob`, the variants of a case adjacent, order rotated), 5 repetitions;
the total of each case's timed phases (seconds) and the paired ratios
(`suite_summary.py`; raw data `suite-series.jsonl`).

| case | base [s] | pgouse [s] | v3 [s] | pgouse / base | v3 / base | digests |
|---|---|---|---|---|---|---|
| cohesive_extrinsic_insertion | 7.911 [7.903, 8.194] | 7.748 [7.718, 8.001] | 7.898 [7.894, 8.277] | **0.978** [0.973, 0.981] | **0.999** [0.995, 1.010] | identical (5 reps) |
| cohesive_insert_tet4 | 6.393 [6.193, 7.446] | 6.469 [6.293, 7.548] | 6.402 [6.289, 7.469] | **1.012** [0.997, 1.018] | **1.014** [0.939, 1.016] | identical (5 reps) |
| contact_explicit_quad4 | 5.299 [5.249, 5.396] | 5.545 [5.520, 5.588] | 5.319 [5.284, 5.679] | **1.047** [1.023, 1.056] | **0.997** [0.997, 1.052] | identical (5 reps) |
| dump_paraview_tet4 | 3.366 [3.359, 3.869] | 3.596 [3.572, 4.104] | 3.422 [3.390, 3.919] | **1.064** [1.061, 1.072] | **1.014** [1.009, 1.064] | identical (5 reps) |
| embedded_rebar_tri3 | 10.162 [9.872, 11.777] | 9.536 [9.510, 10.986] | 9.701 [9.598, 11.579] | **0.938** [0.910, 0.964] | **0.981** [0.915, 0.983] | identical (5 reps) |
| explicit_mazars | 17.029 [16.683, 19.209] | 18.980 [18.874, 23.118] | 15.114 [14.990, 16.458] | **1.108** [0.988, 1.378] | **0.880** [0.792, 0.981] | identical (5 reps) |
| explicit_tet10_fd | 7.831 [7.723, 9.089] | 7.574 [7.504, 8.922] | 7.420 [7.351, 8.572] | **0.968** [0.965, 0.982] | **0.946** [0.943, 0.956] | identical (5 reps) |
| facets_tet4 | 2.025 [1.994, 2.294] | 1.937 [1.905, 2.198] | 2.130 [2.103, 2.455] | **0.955** [0.953, 0.960] | **1.054** [1.049, 1.070] | identical (5 reps) |
| heat_implicit_hex8_kvar | 1.371 [1.360, 1.615] | 1.273 [1.264, 1.474] | 1.358 [1.341, 1.590] | **0.928** [0.913, 0.929] | **0.989** [0.985, 0.992] | identical (5 reps) |
| implicit_dynamic | 5.742 [5.628, 5.941] | 4.601 [4.584, 5.205] | 5.671 [5.627, 6.448] | **0.800** [0.798, 0.888] | **0.990** [0.950, 1.100] | identical (5 reps) |
| msh41_read_tet4 | 0.152 [0.149, 0.170] | 0.133 [0.132, 0.152] | 0.153 [0.151, 0.171] | **0.890** [0.864, 0.896] | **1.009** [0.971, 1.027] | identical (5 reps) |
| parser_dat | 4.273 [4.231, 4.986] | 4.284 [4.159, 4.879] | 4.328 [4.207, 5.121] | **0.988** [0.973, 1.005] | **1.013** [0.977, 1.027] | identical (5 reps) |
| phasefield_staggered_tet4 | 2.533 [2.505, 2.999] | 2.294 [2.287, 2.723] | 2.616 [2.479, 2.892] | **0.911** [0.875, 0.947] | **0.990** [0.964, 1.033] | identical (5 reps) |
| static_linear_eigen | 12.149 [12.094, 13.151] | 11.401 [11.333, 12.715] | 12.050 [12.033, 13.649] | **0.938** [0.925, 0.967] | **0.995** [0.992, 1.038] | identical (5 reps) |
| structural_beam3_frame_dynamic | 5.253 [5.244, 6.064] | 4.686 [4.542, 4.749] | 5.231 [5.171, 5.365] | **0.882** [0.782, 0.903] | **0.989** [0.885, 0.998] | identical (5 reps) |

PGO speeds up the solvers and the model set-up it shares with the
training (implicit dynamics 0.80, the structural frame 0.88, the phase
field 0.91, the linear static solve 0.94, the heat Newton solve 0.93,
mesh reading 0.89). It slows down code the training never ran (Mazars
damage 1.11, the ParaView dump 1.06, explicit contact 1.05). `-v3` gains
in the eigenvalue-heavy Mazars law (0.88) and in tet10 (0.95), and loses
in the facet set-up (1.05).

### Build time and binary size

Clean build of the three timing binaries (with their dependencies) on
the measurement machine under `bjob` (4 cores, `CARGO_BUILD_JOBS=4`),
two rounds in rotated order, other agents' builds running beside them
(`buildtime.jsonl`): seconds per round and per-round ratio to `base`.
File size with `debug = 1`, and the `.text` section.

| variant | clean build [s] per round | / base (per round) | explicit_step_timing: file / .text [MB] | cohesive_extrinsic: file / .text [MB] | facet_scaling .text [MB] | load at start |
|---|---|---|---|---|---|---|
| base | 89, 92 | 1.00, 1.00 | 75.0 / 6.92 | 87.4 / 8.80 | 0.91 | 3.52, 3.71 |
| cgu1 | 119, 121 | 1.33, 1.31 | 62.9 / 6.25 | 72.9 / 7.92 | 0.80 | 2.78, 2.60 |
| thin | 121, 121 | 1.35, 1.30 | 59.6 / 7.18 | 76.8 / 9.12 | 0.91 | 4.30, 3.93 |
| fat | 220, 215 | 2.47, 2.33 | 49.1 / 6.60 | 62.2 / 8.24 | 0.79 | 2.13, 2.23 |
| thin+cgu1 | 153, 157 | 1.71, 1.70 | 50.3 / 6.55 | 64.4 / 8.29 | 0.82 | 3.34, 3.01 |
| fat+cgu1 | 220, 210 | 2.47, 2.27 | 46.7 / 6.20 | 59.5 / 7.80 | 0.77 | 2.23, 1.34 |

PGO costs an instrumented build, the training runs (about 3 minutes on
the sandbox for the second set) and a second build.

## Bitwise identity

Rust does not contract `a * b + c` into an FMA and does not reassociate
at any `opt-level`, LTO setting or target CPU; what can differ is the
lowering of operations whose IEEE 754 result is not unique. Four checks:

1. **CI tiers built with each configuration** (`ci_matrix.sh`: an
   untracked `.cargo/config.toml` in a scratch clone sets the rustflags or
   the release-profile settings; `ci/run-all.sh --backend docker` in the
   project image; boat.dev sandbox, AMD EPYC 4585PX (Zen 5), 2026-11-16).
   The `test` tier is the whole workspace test suite (3,769 tests in 462
   test binaries and 27 doc-test crates, the oracle replays and bitwise
   fixtures included, dev profile with the rustflags); the `release` tier is the optimised-build runs (cohesive,
   linalg, solid implicit dynamics, core and parser, dumpers, the facade
   examples).

   | configuration | `test` tier | `release` tier |
   |---|---|---|
   | `x86-64-v2` | pass (3,769 passed, 0 failed, 25 ignored) | pass |
   | `x86-64-v3` | pass (3,769 passed, 0 failed; rerun, see note) | pass |
   | `x86-64-v4` | pass (3,769 passed, 0 failed; rerun, see note) | pass |
   | `znver4` | pass (3,769 passed, 0 failed) | pass |
   | `fat` LTO + `codegen-units = 1` | (profile not used by the tier) | pass |
   | `thin` LTO + `codegen-units = 1` | (profile not used by the tier) | pass |

   Note: the first `-v3` and `-v4` runs used a git worktree as scratch
   checkout; three tree guards of `akantu-validation`
   (`public_tree_guard`, `upstream_data_manifest`) failed on its `.git`
   file (a host path, and a gitdir git cannot follow inside the
   container), all 3,766 other tests passed. `ci_variant.sh` now makes a
   standalone clone; the reruns of the `test` tier with it pass for both.

2. **Result digests of every timed binary** (`result_digests.sh`,
   `digests.tsv`): the explicit-step digest (FNV-1a of `u`, `v`, `a`,
   `f_int` after 30 steps) on tet4-200k and hex8-200k in small strain and
   finite deformation, and the cohesive driver run to 5,000 steps on the
   h = 0.12 mesh (11,736 cohesive elements inserted in 3,547 insertion
   steps; energies, displacement norms and sums, `insertions.csv`): the
   same for 16 binaries (`base`, `cgu1`, `thin`, `fat`, `thin+cgu1`,
   `fat+cgu1`, `v2`, `v3`, `v4`, `znver4`, `abort`, `base+pgouse`,
   `fat+cgu1+pgouse`, `pgo2-base`, `pgo2-fat`, `nobc`); for `pgo2-v3`
   only the driver's 500-step results of series D were compared (equal). Every timing run also hashed the driver's results: identical
   across the variants of every repetition (`tables.md`). The benchmark
   suites' 71 cases at size `m` give the same digest built as `base`,
   `v3` and PGO (`suite-digests.txt`).

3. **`f64::min`, `f64::max` and NaNs** (`minmax_probe.rs`,
   `minmax-probe.txt`): every pair of 19 special values (signed zeros,
   infinities, quiet and signalling NaNs with and without payload and of
   both signs, subnormals, extremes) through `min`, `max`, `+`, `-`, `*`,
   `/`, `<`-select, `|x|`-max, `sqrt` and `clamp`, scalar and in
   vectorised loops, built at baseline, `x86-64-v2`, `-v3`, `-v4` and
   `native` (Zen 4), at `-O0` and `-O3`. `min` and `max` give the same bits
   in every build: on x86 they return the second operand for equal
   operands, so `min(+0, -0) = -0` and `min(-0, +0) = +0` at every level.
   The only differences are results that are NaN in both builds: which of
   two NaN operands an addition, multiplication or clamp returns (20 of
   361 additions and multiplications, 28 clamps). SSE2's two-operand
   instructions make the compiler commute operands, AVX's three-operand
   forms do not. The same differences appear between `-O0` and `-O3` of
   the baseline. The default NaN of an invalid operation is
   `0xfff8000000000000` at every level. The project's exact rule (D-046)
   already treats two default NaNs that differ only in sign as equal, and
   a NaN with a payload exists only if an input carries one.

4. **Generated code** (`fma-holders.txt`): the `v3`, `v4` and `znver4`
   binaries hold 153 FMA instructions, all in functions that call
   `f64::mul_add` explicitly (the glibc-libm emulation of `akantu-core`:
   `sincos_checked`, `log`, `pow`, `exp`, `tan_checked`; the complex
   quotient of `akantu-linalg`; one non-local weight function into which
   `pow` is inlined). `mul_add` is a correctly rounded fused operation at
   every level: on the baseline it is a call to compiler-builtins' `fma`
   (2 FMA instructions in its run-time dispatched variant), on `-v3` and
   above one instruction, with the same result.

The C++ comparison is unchanged: the strict C++ install and the port
compute the same operations; no configuration here touches that.

## Bounds checks

Experiment (`nobc`, series 1): the checked indexing of three kernels
replaced by `get_unchecked` in a scratch build (`fast::gradient` and
`fast::btd` in `akantu-fe`, `scatter_add` of the internal-force assembly
in `akantu-solver`; `bounds-check-experiment.patch`, never applied). Same
results (digests above). Paired ratio `nobc` / `base`, 5 repetitions:

| case | `step` | `fint` | `stress` |
|---|---|---|---|
| tet4-200k ss | 0.899 [0.880, 0.920] | 0.864 [0.848, 0.888] | 0.874 [0.825, 0.895] |
| tet4-200k fd | 0.938 [0.934, 0.949] | 0.924 [0.919, 0.935] | 0.895 [0.877, 0.911] |
| hex8-200k ss | 0.975 [0.971, 1.026] | 0.972 [0.962, 0.993] | 0.982 [0.971, 1.014] |
| hex8-200k fd | 0.992 [0.981, 1.005] | 0.990 [0.977, 1.006] | 0.976 [0.973, 0.992] |
| cohesive driver `solve_step` | 0.938 [0.919, 0.958] | | |

The checks of these three kernels cost about 10 % of the tet4
small-strain step and 6 % of the finite-deformation step and of the
cohesive driver's explicit step; on hex8 (more arithmetic per element)
2 % or less. The insertion check, the static solve and `facet_scaling`
do not use these kernels and did not change.

Static count (`panic_calls.py`, `panic-calls.txt`): call sites of the
bounds-check panics (`panic_bounds_check`, `slice_index_fail`) per hot
function of the `base` binaries, with the function's share of the flat
profiles (`profiles/`, sandbox CPU):

| function | bounds-panic sites | other panic sites | profile share | where the checks come from | restructuring that removes them |
|---|---|---|---|---|---|
| `finite_deformation::fast::residuals` (`akantu-solid`) | 24 | 0 | 16 % tet4 fd, 29–33 % hex8 fd | per-point slices `gs[t*D*D..]`, `ps[..]`, `out[t*bt..]`, `dsh.at(e, q)` | `chunks_exact` over `gs`, `ps`, `out` zipped with the filter; the shape-derivative block of an element as one slice |
| `fast::btd` (`akantu-fe`) | 18 | 0 | 9 % tet4 ss, 21 % hex8 ss | `bs[(e*nq+q)*bsz..]` with `e` from `ids`, per-node `b[j*S..]`, `d[..]`, `o[..]` | the `ids` pass that already validates the range, then `chunks_exact(S)` over the point's block; `ds`/`os` by `chunks_exact` zipped with the points |
| `fast::gradient<E>` (`akantu-fe`) | 6 | 0 (plus a checked `block.node()` and `u.get()` per node) | 9–12 % | per-node connectivity lookup with its error path, `u[node*NBD..]`, per-point `dshapes`/`out` slices | one validation pass over the block's connectivity (every node index `< nb_nodes`), then gather without per-node checks; `chunks_exact` for `dshapes` and `out` |
| `assemble_elemental_array_local_view` / `scatter_add` (`akantu-solver`) | 6 | 3 | 8–9 % tet4, 2 % hex8 | `elementary[k*nbc..]`, `conn[e*nnpe..]`, data-dependent `out[n*D..]` | `chunks_exact(nbc)` over `elementary`; the connectivity validated once per block (the order of the sums into `out` stays) |
| `fast::integrate` (`akantu-fe`) | 3–5 | 1 | 6–9 % | `jac(pos)[..nq]`, per-element slices | already iterator-based; the per-element `f`/`o` slices by `chunks_exact` |
| `grad_u_in_place` (`akantu-solid`) | 0 | 5 | 4–6 % | panics on dimension checks | a dimension check before the loop |
| `Linear::check_insertion` (`akantu-cohesive`) | 6 | 16 | 8 % of the driver | per-facet indexing, `expect`s | one validation of the facet arrays per call |
| `interpolate_stress` (`akantu-cohesive`) | 4 | 3 | 11 % of the driver | the general product per point (`product_into`, 19 %) | a fixed-size kernel as in the explicit step (listed in `docs/BENCHMARKS.md`) |
| `eigen_sparse_lu::kernels::panel_bmod`, `dense::trsm_unit_lower`, `dense::sparselu_gemm` (`akantu-linalg`) | 25, 17, 5 | 1, 1, 0 | 3 %, 2 %, 29 % of the driver | supernode column slices of Eigen's layout | slices of a supernode taken once per panel; `sparselu_gemm` has few checks: its cost is arithmetic |

Ranked by expected gain: the explicit-step kernels `residuals`, `btd`,
`gradient` and the scatter (measured 6–10 % for three of them on tet4),
then the cohesive insertion check. Not done in this task (crate sources
are outside its files).

## Candidates (not done here)

Bitwise (default path), ranked by expected gain:

1. **Bounds checks of the explicit-step kernels** (`residuals`, `btd`,
   `gradient`, the scatter; 9–33 % of the step profiles each): measured
   0.90 (tet4 ss), 0.94 (tet4 fd, driver explicit step) for three of them
   (section "Bounds checks").
2. **PGO for the distributed artifacts** (wheels, benchmark binaries):
   cohesive driver 0.90–0.92, explicit step 0.94–0.98, bitwise. It needs
   a training set that covers every model. Untrained code can lose (first
   training set: insertion check 1.35; suites: see the table). The
   packaging would have to run the training (`pgo.sh` is the recipe).
3. **`fast::gradient` without AVX-512 gathers**: loads of `dshapes`
   contiguous in the inner loop (or the shape derivatives stored
   transposed). At `-v4` the gradient takes 39 % of the hex8 profile
   against 12.5 % at the baseline. Required before any AVX-512 variant of
   the step, and likely to help `-v3` too.
4. **Run-time multi-versioning (`-v3`) of the internal-force kernels**
   (the execution policy's run-time dispatch): `fint` (`btd`, `integrate`, the scatter) takes
   0.94–0.97 of its time at `-v3`, the small-strain `stress` phase
   (gradient, law) 0.94–0.95, the finite-deformation `stress` phase
   1.06–1.07 (not a candidate). A global `-v3` build is not the
   answer: the facet set-up (`plan_facets`) takes 1.05 at `-v3` and
   1.03 at `znver4`.
5. Fat LTO only for set-up-heavy programs: insertion check 0.95, static
   solve 0.98, facet set-up 0.97, but the explicit step 1.04–1.10. Not
   worth a profile.
6. mimalloc for mesh-editing workloads (element removal 0.65, peak RSS
   0.89) at a 2–10 % cost in the explicit step: a new dependency, not
   proposed.

Fast mode (not bitwise; after the precision switch exists): no build flag
gives FMA or reassociation in Rust (`-C target-cpu` and
`-C target-feature=+fma` do not contract). A fast mode has to write the
fused operations in the kernels. Candidates, gains not measured:
`f64::mul_add` in the inner products of `gradient`, `btd`, `integrate`
and `residuals` behind a `-v3` dispatch (about 50–70 % of the step
profiles), and vectorised (reassociated) reductions in place of
`tree_sum` / `seq_sum` of the fixed-size kernels.

## Files

| File | What |
|---|---|
| `series-1.jsonl`, `series-A.jsonl` … `series-D.jsonl` | raw `series.sh` output (one JSON line per run; round lines with order, load, CPU, UTC) |
| `tables.md`, `overview.md` | `summarize.py` over each series; `overview.py` (one row per configuration) |
| `buildtime.jsonl`, `buildtime.md` | clean build times on the measurement machine (`bjob`, 4 cores, two rounds in rotated order) and binary sizes; `buildtime_summary.py` |
| `suite-series.jsonl`, `suite-table.md` | the benchmark-suite series (one JSON line per case, phase and run, with `variant` and `series_rep`) and its table |
| `boat-builds.jsonl` | the sandbox builds of the timed binaries (their build times are not measurements) |
| `digests.tsv`, `suite-digests.txt` | `result_digests.sh`: explicit-step digests (30 steps) and the 5,000-step cohesive driver results per variant; the 71 suite cases per variant |
| `ci-matrix.md` | the CI tiers built with each configuration (`ci_matrix.sh`): verdicts, test counts, the summary files |
| `minmax-probe.txt` | `minmax_probe.rs` / `minmax_compare.py` at every level and at `-O0` / `-O3` |
| `fma-holders.txt` | the functions holding FMA instructions in the `v3`, `v4`, `znver4` binaries |
| `gather-v4.txt` | gathers and scatters per function in the `v4` binary; profile of `v4` against `base` |
| `panic-calls.txt` | `panic_calls.py`: bounds-check call sites per hot function of the `base` binaries |
| `profiles/` | flat `perf` profiles of `base` (self time per symbol, sandbox CPU; shares only) |
| `bounds-check-experiment.patch` | the `nobc` scratch change |
