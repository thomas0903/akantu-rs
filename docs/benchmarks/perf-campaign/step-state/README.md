# Repeated work in the solid model's step: history copies, scratch arrays, non-local weights

Work items and their effect on the solid suite (`suite_solid`,
`crates/akantu-bench/README.md`, "Benchmark suite"). Every result keeps
every bit: the digests of all 73 cases of the three suites at size `s`, and
of the timed cases at size `m`, are equal before and after.

## What changed

1. **Deferred history saves** (`akantu_model::InternalField`).
   `save_current_values` (upstream `saveCurrentValues`, called by
   `savePreviousState` before every step) copied every block with history
   into its previous values: for the elastic law `stress`, `grad_u`,
   `sigma_th` and `epsilon_th`, 14–15 % of the tet4 small-strain step. A
   block whose current values are not exported is no longer copied at the
   save: the field records that the previous values are the current ones.
   - The copy is made before the current values change: every mutable
     accessor (`get_mut`, `values_mut`, `current_and_previous_mut`,
     `previous_mut`), `resize`, `reset`, `set_default_value`,
     `remove_integration_points`, `inflate` performs it first.
   - When nothing writes the block before the next save, nothing is copied
     (e.g. the ghost blocks of a serial run, the thermal fields of the SLS
     law).
   - A writer that rewrites the whole block announces it
     (`InternalField::prepare_overwrite`, or commits a block computed
     elsewhere with `commit_overwrite`): the save becomes an exchange of the
     current and previous allocations. The solid model announces `grad_u`
     in `compute_all_stresses`; laws announce their stress and thermal
     blocks through `Material::prepare_stress_overwrite` (elastic, plastic,
     Maxwell) after checking the shapes their `compute_stress` checks, so
     the rewrite cannot stop half way.
   - Readers of the previous values get the current block of a deferred
     save (`InternalField::previous`), which holds exactly what the copy
     would have held; `restore_previous_values` leaves such a block alone.
   - Python views (D-047): a view of a block is an export lease of its
     allocation. A block that is exported at the save is copied at once,
     as before, and never exchanged; a block can only be exported through
     `InternalField::values_mut`, which performs the deferred saves first;
     previous values reach Python only as copies. The Python bindings keep
     the lease of every array Python fetched for the life of the model, so
     a field Python has read is copied at every save, as upstream does; the
     saving applies to the fields Python never fetched (in a pure-Rust run,
     all of them). `python/tests/test_history_views.py` runs the same
     explicit problem holding views of the current values for the whole run
     and exporting nothing until the end, and requires the same bits at
     every step (previous values, potential energy, final values), the held
     views at their address and the previous values of a step equal to the
     current values of the step before; with `previous()` made to ignore
     deferred saves (a mutation run on the machine below) the test fails.
   - In the test-only export-all mode (`akantu-core/export-all`) every
     shared array counts as exported: saves are copies there.
   - Upstream copies at every save. The port may differ because the copy
     is not observable: every reader of the previous values and every
     writer of the current values goes through the field, and foreign code
     sees only exported blocks, which are copied as before.
2. **Scratch blocks kept by the laws** (`akantu_solid::materials::scratch`).
   The plastic, Marigo, standard linear solid and Maxwell laws cloned
   their input and output blocks at every step (nine
   clones per step in the J2 return of the plastic law; arrays above
   glibc's mmap threshold are returned to the kernel when freed and faulted
   in again at the next step: 12 % of the plastic case was page clearing).
   They now read their inputs through guards, compute into scratch blocks
   kept between steps (loaded with the current values only where those are
   read first) and commit them with `commit_overwrite`, which exchanges the
   allocations; nothing is written when the computation fails, as before.
   The elastic potential energy does the same; the Neo-Hookean and
   anisotropic stress kernels and the anisotropic energy read the gradient
   in place. The same change in the Mazars damage block executed fewer
   instructions (−0.17 %) but took 8–10 % more cycles on the Mazars case
   on this CPU (the eigen-solver code it calls is the same machine code at
   other addresses), and gave no measurable change for non-local Mazars:
   the Mazars laws keep their clones.
3. **Non-local weights kept while their inputs are unchanged**
   (`akantu_model::non_local::NonLocalNeighborhood`). With the default
   `update_rate = 1` every stress computation recomputed all pair weights
   and the volumes (upstream `computeWeights`), 30 % of
   `explicit_mazars_non_local`. A weight function whose weights depend only
   on the distance and the radius (`WeightFunction::
   weights_depend_only_on_distance`: `base_wf`) lets the neighbourhood keep
   the weights and the volumes it produced; the computation is skipped
   when the radius, the point-identity mode, the summation route, the
   slots, the integration weights at the slots and the incoming volumes
   are bitwise those of the stored computation. Every change of points,
   pairs, slots, radius, weight function or parameters drops the stored
   weights. Damage- and stress-dependent weight functions are recomputed
   at every update as before. The volumes are compared on entry because
   neighbourhoods sharing slots accumulate into the same volumes.
4. **Element filter**: `compute_all_stresses` moves the filter block out of
   the law for the gradient and back, instead of copying it per step.

## Correctness evidence

- Unit tests: `akantu-model` `internal_field::tests::
  deferred_saves_match_immediate_copies` (200 random sequences of saves,
  restores, partial writes, announced rewrites, commits, resizes and resets
  against two vectors that copy at every save; reads of current and
  previous values after every operation and of a clone),
  `deferred_save_and_overwrite`,
  `exported_blocks_are_copied_and_keep_their_views` (an exported block is
  copied, never exchanged; the lease stays attached and aliases the live
  values); `non_local::neighborhood::tests::
  stored_weights_equal_recomputed_weights` (pair weights, key weights and
  volumes bitwise against a neighbourhood that recomputes, also with
  changed incoming volumes).
- Python: `python/tests/test_history_views.py` (above); the whole Python
  suite (`pytest tests` from `python/`) with a wheel of the branch:
  298 passed, 13 skipped, 2 failed: the two
  `structural_mechanics_dynamics*` example comparisons, which need
  matplotlib (`ci/tiers/wheel.sh`); with matplotlib installed both pass.
- Digests: `digests/size-s-threads1.tsv` and `size-s-threads4.tsv`
  (suite, case, `AKANTU_NUM_THREADS`, digest of the base `11d783342`,
  digest of the branch at `12633c666`, status) for every case of
  `suite_solid`, `suite_models` and `suite_setup` at size `s`: 74 equal at
  1 and at 4 threads, and the same digests at both thread counts (the two
  cases `static_linear_mumps` and `partition_scotch_tet4` print no digest
  without the `mumps` / `scotch` features; the same held for each earlier
  base and branch pair at one thread); the timed cases at size `m` in
  `ab/` (every run of a case, both binaries, the same digest).
- CI on the machine below at `84310238f` (the branch with its base
  `b3f6c78aa` merged): `ci/run-all.sh --backend docker` (fast preset:
  lint, test, fixtures, parity, licence), and the `exportall`, `par` and
  `docs` tiers: all passed; after the last merge of the base, at
  `d74b9f36d`, the fast preset passed again. Python tests with a wheel of that commit
  (`test_history_views`, `test_zero_copy_views`,
  `test_constitutive_laws`, `test_views_runtime`, `test_exec_policy`): 64
  passed.

## Measurements

- Machine: a 4-core AMD EPYC Milan (Zen 3) development VM, Ubuntu 24.04;
  one process per run pinned to core 1, nothing else running. (A
  different CPU from the campaign's measurement machine, an AWS
  `m7a.2xlarge`: the numbers decided what was kept; the campaign
  re-measures there.)
- Threads: one. `AKANTU_NUM_THREADS` was not set (default 1) and the
  suites' `--threads` option was not used; the JSON lines record
  `"threads":1`.
- Binaries: `suite_solid` of the base `b3f6c78aa` ("base") and of this
  branch at `84310238f` ("after", the base merged in), release profile,
  default flags.
- Command: `scripts/perf/suite/run_ab.sh --out ab --size m --reps 5 --core 1
  --cases <22 cases> base=<base> after=<after>` (interleaved, order
  rotated per repetition), 2026-11-16 (`ab/meta.*.json`). Raw JSON lines
  `ab/<variant>/<case>.r<R>.jsonl`; `ab/summary.md` (all phases,
  `scripts/perf/suite/summarize.py`), `ab/compact.md` (the phases below).
- ms per call, median [min, max] over 5 repetitions; ratio = after/base
  paired per repetition, median [min, max]. Every run of a case, both
  binaries, printed the same digest.

### The explicit step (`solve_step`, 200 steps at size `m`)

| case | base [ms/step] | after [ms/step] | after/base | `step_hooks` base → after [ms/step] |
|---|---|---|---|---|
| explicit_tet4_ss | 16.06 [15.91, 16.65] | 14.03 [13.84, 14.20] | 0.874 [0.853, 0.878] | 1.25 → 0.00 |
| explicit_tet4_fd | 23.17 [22.93, 23.55] | 19.57 [19.46, 19.98] | 0.848 [0.844, 0.849] | 1.89 → 0.00 |
| explicit_tet10_ss | 52.40 [50.30, 52.71] | 46.86 [45.66, 47.80] | 0.901 [0.889, 0.912] | 2.48 → 0.00 |
| explicit_tet10_fd | 88.71 [88.44, 89.70] | 81.94 [81.11, 83.52] | 0.918 [0.914, 0.941] | 3.37 → 0.00 |
| explicit_hex8_ss | 77.76 [76.67, 79.22] | 68.30 [66.79, 70.22] | 0.883 [0.868, 0.887] | 4.76 → 0.00 |
| explicit_hex8_fd | 141.44 [140.07, 144.60] | 126.51 [125.98, 127.46] | 0.895 [0.871, 0.902] | 6.89 → 0.00 |
| explicit_tri3_ss | 12.44 [12.34, 12.49] | 11.10 [11.05, 11.22] | 0.896 [0.885, 0.899] | 0.68 → 0.00 |
| explicit_tri3_fd | 14.03 [13.96, 14.08] | 12.05 [12.00, 12.30] | 0.860 [0.857, 0.873] | 0.98 → 0.00 |
| explicit_thermal | 10.33 [10.32, 10.51] | 9.21 [9.16, 9.37] | 0.890 [0.887, 0.893] | 0.77 → 0.00 |
| explicit_plastic | 51.51 [50.87, 51.62] | 17.97 [17.87, 18.02] | 0.349 [0.347, 0.353] | 1.82 → 0.57 |
| explicit_sls | 47.54 [47.09, 47.74] | 14.11 [13.93, 14.20] | 0.297 [0.294, 0.297] | 1.85 → 0.00 |
| explicit_maxwell | 170.21 [169.83, 171.53] | 104.43 [104.05, 106.58] | 0.615 [0.612, 0.624] | 64.27 → 33.38 |
| explicit_marigo | 19.30 [19.01, 19.53] | 15.60 [15.51, 15.84] | 0.811 [0.807, 0.818] | 2.58 → 1.86 |
| explicit_neohookean | 24.13 [24.07, 24.73] | 21.72 [21.35, 23.21] | 0.885 [0.878, 0.963] | 1.10 → 0.00 |
| explicit_orthotropic | 16.00 [15.86, 16.07] | 14.01 [13.93, 14.04] | 0.874 [0.874, 0.878] | 0.72 → 0.00 |
| explicit_mazars | 96.38 [95.10, 106.91] | 97.36 [96.61, 98.83] | 1.016 [0.911, 1.025] (no measurable change) | 2.54 → 1.89 |
| explicit_multi_material | 47.25 [47.01, 49.20] | 43.78 [43.53, 44.67] | 0.927 [0.890, 0.945] | 1.78 → 0.85 |
| explicit_mazars_non_local | 226.18 [223.60, 227.13] | 101.57 [100.40, 102.25] | 0.448 [0.442, 0.456] | 1.04 → 0.85 |
| cohesive_intrinsic_bilinear | 17.88 [17.76, 18.13] | 17.20 [17.09, 17.28] | 0.962 [0.953, 0.966] | – |
| cohesive_intrinsic_exponential | 24.86 [24.73, 25.07] | 24.24 [24.02, 24.43] | 0.973 [0.967, 0.985] | – |

Readings:

- `step_hooks` (the history saves before the step and the energy updates
  after it) falls to 0.00 ms for every law whose blocks are rewritten in
  full; what remains for plastic, Marigo, Maxwell and the Mazars laws is
  their energy and branch updates after the step (arithmetic).
- The `residual` phase of the elastic cases is 1–5 % slower (tet4 ss
  1.038 [1.016, 1.045], thermal 1.048 [1.047, 1.051], hex8 ss 1.009
  [1.003, 1.014]): the copy of the save used to bring the current blocks
  into the last-level cache just before the step rewrote them; after an
  exchange the step writes into the other allocation, untouched since the
  step before. The saved copy is 2–12 times that (tet4 ss: 1.25 ms of
  hooks saved against 0.52 ms more `residual` per step; hex8 ss: 4.76
  against 0.38 ms).
- Plastic and SLS: the per-step clones and their page faults are gone
  (×0.31–0.34 of the `residual`). Maxwell: ×0.52 of `step_hooks`, the
  branch update in place; the rest is its generic `scaled_matvec`
  (65 % of the profile, a fixed-order kernel is another task's item).
- Non-local Mazars: the weights are computed once per rebuild instead of
  at every step (×0.44 of the `residual`).

### After the last merge of the base (`ab-merge/`)

The base `72337cb4e` brought fixed-size kernels for several laws (Maxwell,
anisotropic). Branch `d74b9f36d` (that base merged) against it, same
method, four cases (the digests of that pair at size `s`: 73 equal):

| case | base [ms/step] | after [ms/step] | after/base | `step_hooks` base → after [ms/step] |
|---|---|---|---|---|
| explicit_maxwell | 90.64 [89.82, 90.75] | 24.76 [24.71, 25.08] | 0.276 [0.272, 0.278] | 35.22 → 4.41 |
| explicit_orthotropic | 12.11 [12.07, 12.28] | 10.29 [10.24, 10.36] | 0.848 [0.838, 0.859] | 0.70 → 0.00 |
| explicit_plastic | 50.94 [50.59, 51.56] | 18.10 [18.00, 18.63] | 0.355 [0.355, 0.365] | 1.81 → 0.58 |
| explicit_tet4_ss | 16.01 [15.76, 16.72] | 13.73 [13.67, 14.53] | 0.864 [0.851, 0.869] | 1.24 → 0.00 |

With the Maxwell products fixed-size, its per-step clones were most of
what was left of its step.

### Energies (`energies`: one kinetic and one potential energy per call)

| case | base [ms/call] | after [ms/call] | after/base |
|---|---|---|---|
| explicit_tet4_ss | 8.61 [8.39, 8.69] | 6.76 [6.70, 6.91] | 0.795 [0.781, 0.803] |
| explicit_tet10_ss | 56.03 [55.19, 57.08] | 7.27 [7.23, 7.49] | 0.130 [0.127, 0.134] |
| explicit_hex8_ss | 98.30 [98.17, 98.82] | 10.87 [10.76, 11.27] | 0.111 [0.109, 0.115] |
| explicit_hex8_fd | 102.52 [102.09, 107.59] | 17.38 [17.24, 17.43] | 0.170 [0.160, 0.171] |
| explicit_orthotropic | 4.78 [4.77, 4.84] | 4.08 [4.05, 4.14] | 0.849 [0.843, 0.868] |

The elastic potential energy cloned four blocks per call (for hex8 ss at
size `m`, 884,736 points × 20 reals: about 140 MB written and faulted in
per call); with the
scratch block, a potential energy costs less than an explicit step again
(hex8 ss: 10.9 against 68.3 ms). The remaining time is the ordered
`Math::reduce` of the integration (a sort, `akantu-core`) and the
gradient pass.

### `init_full` on hexahedra (`setup_hex8`, `setup_tet4`)

`init_explicit` is unchanged (setup_hex8 0.988 [0.961, 1.003]). Per
element, hex8 costs 5.6 µs (13,824 elements, 77.95 ms) against 0.69 µs
for tet4 (162,000 elements, 112.04 ms); per quadrature point the two are
close (0.71 against 0.69 µs: hex8 has 8 points). Of the hex8 time 42 %
is the shape-function precomputation (`shape_functions`, 32.9 ms;
before the base's fixed-order small products, 6.3 µs per element on this
machine: generic products and the LU inverse of the Jacobian in
`akantu-fe`); the rest is the allocation and first fill of the laws'
internals (about 50 reals per point) and the solver set-up. Nothing
there is repeated or quadratic.

### Profiles after (`profiles/`)

`perf record -F 499 --call-graph dwarf` of `suite_solid` built at
`dbb3ca2c5` (this branch before the Mazars files were restored and before
the last two merges of the base, which brought the execution policy and
the sparse assembly; `scripts/perf/suite/profile.sh`, 6 s loops; shares,
recorded while other work ran on the other cores). No `memmove`, page-clearing or allocator
symbol is left in the top 25 of any of the eight cases; the explicit
elastic cases are the FE kernels (gradient, `BᵀD`, integration, scatter:
60–65 %), the stress loop and `grad_u` (13–16 %), `resolve_filter`'s per-call
filter copy (2.8 %, `akantu-fe`) and `Math::reduce`'s sort (2 %, in the
energies).
