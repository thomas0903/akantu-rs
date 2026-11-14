# Upstream test input and reference files

Copied verbatim from the Akantu reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later,
© EPFL/LSMS) for the ported tests of `crates/akantu-solver/tests/`:

| File | Upstream path | SHA-256 | Used by |
|---|---|---|---|
| `bar.msh` (subdirectory `test_synchronizer/`) | `test/test_synchronizer/bar.msh` (MSH 2.1, 25 nodes, 16 `_quadrangle_4`; `FILES_TO_COPY` of `test_dof_synchronizer`) | `8ed19a97fedee6fa252a5143d7bf9134fd9cbe49dc0613d967b3e25687d0ea1e` | `tests/dof_sync_scenario/mod.rs` → `test_dof_synchronizer.rs` (local world 1/2/4) and `akantu-solid` `mpi_smm` (real MPI 1/2/4, scripts/mpi/run-par-tests.sh) |
| `test_sparse_matrix_product.verified` | `test/test_solver/test_sparse_matrix_product.verified` | `78c1debd314b7b7d09be3ff31706a70ae52ab645a01cd456a5b90ed1b9476728` | `tests/upstream_sparse_matrix_product.rs` |

Not to be confused with `test/test_solver/bar.msh` (121 nodes), used by the
`fixtures/solver/aij` oracle cases.
