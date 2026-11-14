<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Upstream test reference files

Copied verbatim from the Akantu reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later,
© EPFL/LSMS) for the ported tests in `crates/akantu-cohesive/tests/`:

| File | Upstream path | SHA-256 | Used by |
|---|---|---|---|
| `K_matrix_verified.dat` | `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/K_matrix_verified.dat` | `0a857e3401058fbabfb24e6962508317fd40c6c6838b87271f2ec915cb471221` | `upstream_assembling_k_cohe_elements.rs` (golden of upstream's comparison, GOLD-006) |
| `quadrangle.msh` | `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/quadrangle.msh` | `f6f040ce29c0c00a6f298565792694de565197d5fe8a524477c8159570635103` | `upstream_assembling_k_cohe_elements.rs` (input) |
| `material.dat` | `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/material.dat` | `41b54be9ddd4b62594a1ebffc5e753fe2e30c6f55c4e9c44fc20c79c573c905c` | `upstream_assembling_k_cohe_elements.rs` (input) |

`K_matrix_verified.dat` dates from 2015 and is stale: the upstream driver no
longer compiles at the pinned commit (UD-697) and, with the minimal API fix,
produces a `K` without cohesive elements that fails the driver's own
comparison with this file; the oracle output and verdict are in
`fixtures/gold/test_assembling_K_cohe_elements/`.
