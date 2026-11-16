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
   The plastic, Marigo, Mazars, non-local Mazars, standard linear solid and
   Maxwell laws cloned their input and output blocks at every step (nine
   clones per step in the J2 return of the plastic law; arrays above
   glibc's mmap threshold are returned to the kernel when freed and faulted
   in again at the next step: 12 % of the plastic case was page clearing).
   They now read their inputs through guards, compute into scratch blocks
   kept between steps (loaded with the current values only where those are
   read first) and commit them with `commit_overwrite`, which exchanges the
   allocations; nothing is written when the computation fails, as before.
   The elastic potential energy does the same; the Neo-Hookean and
   anisotropic stress kernels and the anisotropic energy read the gradient
   in place.
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
  suite with the branch's wheel: 300 passed, 13 skipped (the two
  `structural_mechanics_dynamics*` example comparisons need matplotlib).
- Digests: `digests/size-s.tsv` (suite, case, digest before, digest after,
  status) for every case of `suite_solid`, `suite_models` and
  `suite_setup` at size `s`: 73 equal (two cases print no digest without
  the `mumps` / `scotch` features); the timed cases at size `m` in
  `ab/` (every run of a case, both binaries, the same digest).
- CI on the machine below, branch merged with its base: `ci/run-all.sh
  --backend docker` (fast preset) passed; `par` and `docs` passed;
  `exportall` passed except the two deferred-save unit tests, whose
  structural checks assumed deferred saves (fixed: in that mode they check
  the values only).

## Measurements

PLACEHOLDER
