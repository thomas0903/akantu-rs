<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Upstream golden files: coverage list

Taken 2026-11-11. Reference: upstream Akantu @
`35a851780d0b9471d258ed53a003ae98aaa95ea0`; paths are relative to the upstream
repository root. Parity rows
GOLD-001…015 and MDATA-001…006 (`docs/FEATURE_PARITY.md`); inventory
`docs/inventory/tests_examples_python.md` §C.4/§C.5.

This file is the coverage list written before the remaining golden files
were ported, updated with the results (§6). The *Coverage* column records
the state of that day; `docs/FEATURE_PARITY.md` gives the current status.

Legend (column *Coverage*):

- **main** — already reproduced by a Rust test at the time; this list only
  adds the provenance record (SHA-256 of the copy = upstream, see
  "Accounting" below).
- **taken** — the Rust test was written with this list.
- **owed (area)** — the test belonged to an area whose port was not yet
  integrated; the golden file went with that area.
- **dead** — no registered upstream test produces it, or it documents a
  removed API; reason given.

Comparison column = how upstream's harness compares the file (the Rust test
must compare the same way): `diff -biwI` is `cmake/akantu_test_driver.sh:158`
(`diff -biwI "([0-9]\+ ms\( total\)\?)" <lastout> <reference>`, never run
upstream because the driver is not given `-r`, UD-018 — the oracle checker
`oracle/scripts/check-verified.sh` reproduces 14 of them in the strict
configuration).

## 1. `.verified` files (GOLD-001, GOLD-003, GOLD-004, GOLD-005)

| # | Upstream file (SHA-256 prefix) | Test (row) | Area | Comparison upstream | Coverage | Rust test / copy |
|---|---|---|---|---|---|---|
| 1 | `test/test_io/test_parser/test_parser.verified` (`309d165d0c19`) | `test_parser` (TEST-031) | parser | `diff -biwI` | **main** | `akantu-parser/tests/test_parser.rs::test_parser_matches_verified_output`; copy `crates/akantu-parser/tests/fixtures/test/test_io/test_parser/test_parser.verified` |
| 2 | `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.verified` (`2c8e56a40a8d`) | `test_mesh_io_msh_physical_names` (TEST-038) | mesh I/O | `diff -biwI` | **main** | `akantu-io/tests/mesh_io_gmsh.rs::test_mesh_io_msh_physical_names`; copy `crates/akantu-io/tests/data/upstream/` |
| 3 | `test/test_mesh_utils/test_buildfacets/test_buildfacets_gtest.verified` (`f8b6d814c18e`) | `test_buildfacets_gtest` (TEST-039) | mesh utils | `diff -biwI` | **main** | `akantu-mesh/tests/upstream_mesh_utils.rs::test_buildfacets`; copy `fixtures/mesh_utils/buildfacets/_triangle_3/inputs/` |
| 4 | `test/test_model/test_common/test_model_solver/test_dof_manager_default.verified` (`ad573df50b3b`) | TEST-072 | solver | `diff -biwI` | **main** | `akantu-solver/tests/test_model_solver.rs`; copy `fixtures/solver_model/dof_manager_default/inputs/` |
| 5 | `…/test_model_solver/test_model_solver_mumps.verified` (`007cdbdf80c4`) | TEST-073 | solver | `diff -biwI` | **main** | `test_model_solver.rs::test_model_solver_static`; copy `fixtures/solver_model/static_model_solver/inputs/` |
| 6 | `…/test_model_solver/test_model_solver_dynamic_explicit.verified` (`e955798103f6`) | TEST-074 | solver | `diff -biwI` | **main** | `test_model_solver.rs::test_model_solver_dynamic_explicit`; copy `fixtures/solver_model/dynamic_explicit/inputs/` |
| 7 | `…/test_model_solver/test_model_solver_dynamic_implicit.verified` (`44faf9d83f5a`) | TEST-074 | solver | `diff -biwI` | **main** | `test_model_solver.rs::test_model_solver_dynamic_implicit`; copy `fixtures/solver_model/dynamic_implicit/inputs/` |
| 8 | `test/test_solver/test_sparse_matrix_product.verified` (`78c1debd314b`) | TEST-157 | sparse LA | `diff -biwI` | **main** (+ `akantu-core/tests/array_printself.rs`, CORE-013, compares the `Array<double>` block on a second copy in `crates/akantu-core/tests/data/upstream/`; with its README) | `akantu-solver/tests/upstream_sparse_matrix_product.rs`; copy `crates/akantu-solver/tests/data/upstream/` |
| 9 | `…/test_model_solver/test_model_solver_petsc.verified` (`007cdbdf80c4`, **byte-identical to #5**) | TEST-073 (PETSc) | PETSc backend | `diff -biwI` | **main** (PETSc backend merged) | `akantu-solver/tests/petsc_mpi.rs::test_model_solver_petsc` (PETSc tier; the comparison of #5 applies unchanged) |
| 10 | `…/test_model_solver/test_model_solver_dynamic_petsc.verified` (`44faf9d83f5a`, **byte-identical to #7**) | TEST-074 (PETSc) | PETSc backend | `diff -biwI` | **main** | `akantu-solver/tests/petsc_mpi.rs::test_model_solver_dynamic_petsc` (PETSc tier) |
| 11 | `test/test_model/test_common/test_non_local_toolbox/test_non_local_neighborhood_base.verified` (`5f5074176d14`) | TEST-075 | non-local | `diff -biwI` | **main** | `akantu-model/tests/upstream_non_local_toolbox.rs::test_non_local_neighborhood_base` |
| 12 | `…/test_non_local_toolbox/test_remove_damage_weight_function.verified` (`cf3dddc77d57`) | TEST-078 | non-local | `diff -biwI` | **main** | `akantu-solid/tests/upstream_non_local_toolbox.rs::test_remove_damage_weight_function` |
| 13 | `…/test_non_local_toolbox/test_weight_computation.verified` (`79afb3078609`) | TEST-076 | non-local | `diff -biwI` | **main** | `akantu-model/tests/upstream_non_local_toolbox.rs::test_weight_computation` |
| 14 | `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_standard_linear_isotropic_hardening.verified` (`51b70bf1acda`) | TEST-146 | inelastic materials | `diff -biwI` | **main** | `akantu-solid/tests/upstream_material_elasto_plastic_linear_isotropic_hardening.rs::test_material_standard_linear_isotropic_hardening` |
| 15 | `extra_packages/extra-materials/test/test_material_damage/test_material_iterative_stiffness_reduction.verified` (`1d3ab7136ff1`) | TEST-188 | extra materials | `diff -biwI` (package does not compile upstream) | **main** | `akantu-extra-materials/tests/upstream_damage_iterative.rs::test_material_iterative_stiffness_reduction` |
| 16 | `test/test_fe_engine/test_interpolate_bernoulli_beam_2.verified` (empty, `e3b0c44298fc` = SHA-256 of nothing) | TEST-211 (unregistered) | structural FE | none (not registered) | **dead** — source uses the removed `ShapeLinked` / `interpolateOnControlPoints` API, prints nothing (writes two *pointer addresses* to `out.txt`); the empty file is its empty stdout | UD-698; D-028 exception proposed for TEST-211, since approved (D-076); intent covered by `akantu-fe/tests/structural_matches_oracle.rs` |
| 17 | `test/test_solver/test_petsc_matrix_profile.verified` (`74ddd63ef985`) | TEST-228 (unregistered) | PETSc | none | **dead / owed (PETSc)** | — |
| 18 | `test/test_solver/test_petsc_matrix_profile_parallel.verified` (`ecb8076d4695`) | TEST-228 (unregistered) | PETSc | none | **dead / owed (PETSc)** | — |
| 19–22 | `extra_packages/igfem/test/test_fe_engine/{test_fe_engine_precomputation,test_gradient,test_integrate,test_interpolate}.verified` | TEST-193…196 | IGFEM | (package does not compile upstream) | **main** | `akantu-igfem/tests/test_fe_engine.rs::{test_fe_engine_precomputation, test_gradient, test_integrate, test_interpolate}` (feature tier `--features cgal`; copies in `crates/akantu-igfem/tests/data/upstream/`) |
| 23 | `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.verified` | TEST-200 | IGFEM | as above | **main** | `akantu-igfem/tests/test_solid_mechanics_model_igfem.rs::test_solid_mechanics_model_igfem` (feature tier `--features cgal`) |
| 24 | `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_compression.verified` (`2777b9b2abbf`) | none (orphan; GOLD-005) | TASN | none | **main** | `akantu-ntn-contact/tests/upstream_ntn_friction.rs` (TestCompression; the orphan history equals the test's hard-coded references, GOLD-005) |

## 2. Other reference data compared by an upstream test (GOLD-002, -006, -007, -008)

| Upstream file(s) | Test (row) | Area | Comparison upstream | Coverage |
|---|---|---|---|---|
| `test/test_fe_engine/_<type>.csv` ×12 (GOLD-002) | `test_fe_engine_gtest: Inradius` (TEST-016) | FE | `EXPECT_NEAR` rel 1e-6 per element | **main**: `akantu-fe/tests/test_fe_engine_inradius.rs`, copies `crates/akantu-fe/tests/data/inradius/*.csv` (all 12 byte-identical). This list adds only the SHA-256 record |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/K_matrix_verified.dat` (GOLD-006, `0a857e340105`) | `test_assembling_K_cohe_elements` (TEST-222; `add_akantu_test` commented out upstream) | cohesive (merged) | the program itself: entries matched by (row, col), `|ΔK|/|K_ver| ≤ 1e-13` (or both `< 1e-13`), `EXIT_FAILURE` otherwise | **taken → done**: `akantu-cohesive/tests/upstream_assembling_k_cohe_elements.rs`; the driver does not compile at the pinned commit (UD-697) and runs on the oracle with a one-line API patch; `K_matrix_test.dat` byte-identical to the oracle's, same failing verdict (7/99 rejected; stale 2015 golden) |
| `…/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_{0,1}_out_of_2.out`, `…_{0..3}_out_of_4.out` (GOLD-007) | `test_cohesive_parallel_intrinsic_implicit_insertion` (TEST-242, a successor of the superseded TEST-226; POSTPROCESS `verify_insertion.sh`: exact `diff`). The inventory appendix attributes the `test_parallel_cohesive` files to TEST-226 only; the manifest adds all its successors TEST-235…244 as consumers, so the six `.out` files become owed as soon as TEST-242 is ported | parallel cohesive (**not merged**: the cohesive model refuses > 1 rank, `akantu-cohesive/src/lib.rs`) | `diff` (exact) | **owed (parallel cohesive insertion)**; later an approved exception for the byte comparison (D-085) |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/matrix` (GOLD-008) | `test_embedded_interface_model` (TEST-126, `.sh`: `diff`) | embedded (not merged; does not compile upstream) | `diff` (exact) | **owed (embedded model)** |

## 3. Non-file references and data without a consumer (GOLD-009…015)

| Row | Content | Coverage |
|---|---|---|
| GOLD-009 | hard-coded reference values in test sources | mapped with the TEST rows; not duplicated |
| GOLD-010 | independent oracle code: `test/python_fe/*` (truss FE: static, Newmark), `test_model_solver{,_dynamic}.py`, `py_engine`, `py_mazars` | `py_engine`: main (`test_fe_engine_precomputation.rs`). **taken → done: `python_fe` + `test_model_solver.py`** (`akantu-solver/tests/upstream_python_fe.rs`; upstream's script is dead and fails its own 1e-14 on the oracle output, UD-699; the port gives the same verdict on a bit-identical input). `py_mazars`: owed (MAT) |
| GOLD-011 | analytical solutions | per TEST row (other work packages) |
| GOLD-012 | example-produced data without reference (`energy.csv`, `position.csv`, `energies.csv`, `modes.txt`) | owed (examples) |
| GOLD-013 | `joss/results/timmings_*.csv` (JOSS scaling study) | **follows EX-018 and DOC-019, no exception**: the CSVs are written by the scalability example (`examples/c++/scalability_test/cohesive_extrinsic.cc`, `Chrono::printself_csv`, EX-018) on EPFL's "jed" cluster and read by `joss/results/plot.py` and `examples/c++/scalability_test/plot.py` (DOC-019). Done when the EX-018 port writes timing CSVs in the same column format, upstream's `plot.py` reads them, and `docs/BENCHMARKS.md` compares the scaling shape (efficiency) with these CSVs; absolute timings are hardware-specific (a documented non-reproducible quantity). `Not started` until EX-018 is ported |
| GOLD-014 | `extra_packages/igfem/test/test_fe_engine/mesh_info_igfem_triangle_{4,5}.txt` | inputs of IGFEM tests: owed (IGFEM) |
| GOLD-015 | unused data: 3 `material_ntn_friction_rate_and_state_revised_n_shaped_*.dat` (TASN, tests commented out), `examples/c++/diffusion_model/cube_tet.geo`, `test/test_solver/1D_bar.geo` | accounted for in the data manifest (below) as "no consumer"; `1D_bar.msh` is frozen anyway (`fixtures/meshes/frozen_meshes.json`) |

## 4. Meshes and input files (MDATA-001…006)

- Generated meshes: all 121 `add_mesh()` outputs of `test/`, `examples/`,
  `extra_packages/` are frozen on main (`fixtures/meshes/gmsh-4.13.1/`,
  `frozen_meshes.json`, D-009; Gmsh 4.13.1 of the oracle image).
- Committed meshes: golden `Mesh::read` fixtures of every committed mesh
  (`fixtures/mesh_io/**`) on main.
  **MDATA-004** (cohesive / contact straight-interface geometries, cohesive
  now merged) is **taken**.
- **The accounting** — a manifest of every upstream test
  mesh and input file (`.msh`, `.geo`, `.dat`, golden files) with its
  SHA-256, every verbatim copy in this repository (crate test data and
  fixture `inputs/`), the frozen outputs of each `.geo`, and the consumer
  rows. The Rust test re-derives every
  repository-dependent part of it (copies by a SHA-256 scan of the
  git-visible files, frozen meshes, consumers with the successors of
  superseded rows, ported consumers, statuses, the `tests/data/upstream`
  READMEs) and fails when the committed manifest is stale; every commit that
  changes copies, frozen meshes or TEST/EX/PY statuses regenerates it
  (`docs/TESTING.md`).

## 5. Summary

| Coverage | `.verified` (24) | other reference data |
|---|---|---|
| main | 8 (#1–#8) | GOLD-002 (12 CSV) |
| taken by this list | — (#16 documented as dead) | GOLD-006; accounting of all copies (GOLD-001/002 SHA-256, MDATA); MDATA-004; GOLD-010 `python_fe` (stretch) |
| owed to unmerged areas | 2 (#17–#18, PETSc; #9–#15 and #19–#24 have consumers on main) | GOLD-007, GOLD-008, GOLD-012, GOLD-013 (EX-018), GOLD-014 |
| dead | #16 (and #17–#18 unregistered) | — (GOLD-013 follows EX-018/DOC-019) |

## 6. Results

| Item | Result |
|---|---|
| GOLD-006 | done: port of `test_assembling_K_cohe_elements`, oracle output `fixtures/gold/test_assembling_K_cohe_elements/` (probe `oracle/probes/gold`, API-patched driver, `"defect": "UD-697"`), UD-697; plus a port-only test of the golden's scenario after a forced insertion check (the oracle aborts there, UD-197) |
| GOLD-010 `python_fe` | done: `upstream_python_fe.rs`, oracle-side run `fixtures/gold/test_model_solver_py/` (`oracle/probes/gold/python_fe_check.sh`; re-verified: three runs identical modulo the scratch-directory name, OpenBLAS pin), UD-699 (a stale check) |
| GOLD-003 bernoulli | documented as not portable, UD-698; TEST-211 later approved as an exception (D-076) |
| GOLD-001/002 copies | SHA-256 of every copy checked by `akantu-validation/tests/upstream_data_manifest.rs` (8 `.verified` + 12 CSV + `K_matrix_verified.dat`) |
| MDATA-001…006, GOLD-015 | accounting manifest at the time of writing: 438 upstream data files (now including `third-party/iohelper/test/cube.{geo,msh}`) — 282 copied (1015 (upstream file, copy) **pairs**, 793 distinct copy files), 110 `.geo` frozen (121 meshes), 22 `.geo` without a build rule (commented-out `#add_mesh` lines no longer count: `test_materials/barre_trou.geo`, `cohesive_{extrinsic,intrinsic}/triangle.geo`), 24 not needed by a ported test (areas not merged, or dead), **0 owed**. The repository had gained copies since the first count (phase-field, geometry, core); the fresh scan found one without a README row (`crates/akantu-core/tests/data/upstream/`), fixed |
| owed to unmerged areas | `.verified` #17–#18 (#9–#15 and #19–#24 have consumers on main); GOLD-007/008/012 (owners in the tables above) |
