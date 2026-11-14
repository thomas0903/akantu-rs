<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Upstream inradius reference values

Copied verbatim from the Akantu reference checkout
(`test/test_fe_engine/_<type>.csv`, commit
`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later,
Copyright © EPFL/LSMS). Format: `element_id, inradius` with 6 significant
digits ("written in simple precision" per upstream). Used by
`tests/test_fe_engine_inradius.rs` (port of `test_fe_engine_inradius.cc`)
together with the meshes of `fixtures/mesh_io/test_fe_engine/*`.

| File | Upstream path | SHA-256 |
|---|---|---|
| `_hexahedron_20.csv` | `test/test_fe_engine/_hexahedron_20.csv` | `3c664cad035ecc57a74df35fe85cbf8e6b9bf6848b4d986c7c40bb6704ed244e` |
| `_hexahedron_8.csv` | `test/test_fe_engine/_hexahedron_8.csv` | `4c5bcd6bb527a848ca62820f438eafe72c2887d665315f023d81b65558cf4558` |
| `_pentahedron_15.csv` | `test/test_fe_engine/_pentahedron_15.csv` | `eb2bdd9159a71564505210258ff730ed8f35a212ba314a6c1eda1ae53d9b2ab7` |
| `_pentahedron_6.csv` | `test/test_fe_engine/_pentahedron_6.csv` | `1a7357cdd022160ba2069da90fec12ed8e614382c6fb89c97d4d139474ede9ed` |
| `_quadrangle_4.csv` | `test/test_fe_engine/_quadrangle_4.csv` | `55c41d3d7824170389bf7a9536f5eb300185104a07ccb02ef40af33e6f8b03f7` |
| `_quadrangle_8.csv` | `test/test_fe_engine/_quadrangle_8.csv` | `b702b676e105e8e4361b443280f95991335f088378046f799718176446918ea8` |
| `_segment_2.csv` | `test/test_fe_engine/_segment_2.csv` | `6c6230cc28578e88574a66033c9108a9423bd0eb2e4361aa5dfab9193e822a1e` |
| `_segment_3.csv` | `test/test_fe_engine/_segment_3.csv` | `8c64183bd13c6656c1ba7a678059b068184126074c479f4ccbbe2f1ae937a8b3` |
| `_tetrahedron_10.csv` | `test/test_fe_engine/_tetrahedron_10.csv` | `2139d2e745fd6c008005b06c822b5c85ba5830e95b983a749eb927be270fd58e` |
| `_tetrahedron_4.csv` | `test/test_fe_engine/_tetrahedron_4.csv` | `7e31cd0400107f61693f62f09e163609a372bd58d0e8e0385076ad8e2a0f19fb` |
| `_triangle_3.csv` | `test/test_fe_engine/_triangle_3.csv` | `971fab279667e882d5f5dbe8b6eff5e6ce7b950b9fc390ada153de6398ff55aa` |
| `_triangle_6.csv` | `test/test_fe_engine/_triangle_6.csv` | `a719fd09443349faa78ebabe0956ff9c93470324a7fcfc516dd7af82d2c42833` |

SHA-256 recorded in the upstream data manifest and checked against the copies by `akantu-validation/tests/upstream_data_manifest.rs`.
