<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Upstream test inputs and reference outputs

Copied verbatim from the Akantu reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later, © EPFL/LSMS)
for the ported IGFEM tests in `crates/akantu-igfem/tests/`. The mesh of
`patch_tests/plate.geo` is generated at build time upstream: the tests read
the frozen Gmsh 4.13.1 mesh
`fixtures/meshes/gmsh-4.13.1/extra_packages/igfem/test/patch_tests/plate.msh`
(`common::frozen_patch_plate`), not a copy kept here.

| File | Upstream path | SHA-256 |
|---|---|---|
| `material.dat` | `extra_packages/igfem/test/patch_tests/material.dat` | `254f42c6f335e629f7bba09b6752a1fd733369170b31f31b2ee7c180e9f3a926` |
| `material_test_interface_position.dat` | `extra_packages/igfem/test/patch_tests/material_test_interface_position.dat` | `6d7dc51133e520563ee4eb74813292baf4c9a53466809fe18673b12a992d9d9e` |
| `mesh_info_igfem_triangle_4.txt` | `extra_packages/igfem/test/test_fe_engine/mesh_info_igfem_triangle_4.txt` | `d18c22c19150750babea169f2351dc95d7cb5bdd595b9966dc688f7e37802532` |
| `mesh_info_igfem_triangle_5.txt` | `extra_packages/igfem/test/test_fe_engine/mesh_info_igfem_triangle_5.txt` | `b727e88af885db81c4de624e4b82be964b94bb458189ed25cf2f53ba125d67a1` |
| `test_fe_engine_precomputation.verified` | `extra_packages/igfem/test/test_fe_engine/test_fe_engine_precomputation.verified` | `785086e5007648cebe1db49295afe33f5d7917b8d51fe5c6c19760041cdbe4f1` |
| `test_geometry_triangle.msh` | `extra_packages/igfem/test/test_geometry/test_geometry_triangle.msh` | `0a50d934f5e7f6827bd363b5bb62c42acba6968b270d2344856c685f78819c6d` |
| `test_gradient.verified` | `extra_packages/igfem/test/test_fe_engine/test_gradient.verified` | `c1650d070bfa76223009dbde81e22cde31bb2afad7c9d99cbafe6571c89006b7` |
| `test_integrate.verified` | `extra_packages/igfem/test/test_fe_engine/test_integrate.verified` | `2171d319d6a0af39bda01cc0b983b3b6a16686cd9a1f14cdb46df8b67747a24c` |
| `test_interface_position.msh` | `extra_packages/igfem/test/patch_tests/test_interface_position.msh` | `5214126d25ffd74fa484e1eb410826744b5bf04dfa3679304040cdc4439388ab` |
| `test_interpolate.verified` | `extra_packages/igfem/test/test_fe_engine/test_interpolate.verified` | `37ef6f23bb54bf4bf865fdd069a5cdc00205d2e0e21624f2fc294a0c10d6c1f7` |
| `test_mesh.msh` | `extra_packages/igfem/test/patch_tests/test_mesh.msh` | `691ec184cb731cacff9df821d9619fe6b272b37f9340406f60c1c46a97e51589` |
