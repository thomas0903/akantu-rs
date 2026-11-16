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

Rust does not contract `a * b + c` into an FMA and does not reassociate
at any `opt-level`, LTO setting or target CPU; what can differ is the
lowering of operations whose IEEE 754 result is not unique. Four checks:

1. **CI tiers built with each configuration** (`ci_matrix.sh`: an
   untracked `.cargo/config.toml` in a scratch clone sets the rustflags or
   the release-profile settings; `ci/run-all.sh --backend docker` in the
   project image; boat.dev sandbox, AMD EPYC 4585PX (Zen 5), 2026-11-16).
   The `test` tier is the whole workspace test suite (3,769 tests in 462 binaries and 27 doc-test crates, the
   oracle replays and bitwise fixtures included, debug profile with the
   rustflags); the `release` tier is the optimised-build runs (cohesive,
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
   same for all 17 binaries (`base`, `cgu1`, `thin`, `fat`, `thin+cgu1`,
   `fat+cgu1`, `v2`, `v3`, `v4`, `znver4`, `abort`, the five PGO builds,
   `nobc`). Every timing run also hashed the driver's results: identical
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
