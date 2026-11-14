<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Upstream test reference files

Copied verbatim from the Akantu reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later,
© EPFL/LSMS) for the ported tests in `crates/akantu-core/tests/`:

| File | Upstream path | SHA-256 | Used by |
|---|---|---|---|
| `test_sparse_matrix_product.verified` | `test/test_solver/test_sparse_matrix_product.verified` | `78c1debd314b7b7d09be3ff31706a70ae52ab645a01cd456a5b90ed1b9476728` | `array_printself.rs` (the `Array<double>` block, CORE-013) |

The same file is copied for the whole-program comparison in
`crates/akantu-solver/tests/data/upstream/` (TEST-157). Every file under a
`tests/data/upstream/` tree has a row with its name, upstream path and SHA-256, checked by
`akantu-validation/tests/upstream_data_manifest.rs`.
