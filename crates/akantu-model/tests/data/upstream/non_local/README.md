# Upstream non-local toolbox test inputs and reference outputs

Copied verbatim from the Akantu reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later,
© EPFL/LSMS), directory `test/test_model/test_common/test_non_local_toolbox/`,
for the ported tests `crates/akantu-model/tests/upstream_non_local_toolbox.rs`
(TEST-075, TEST-076) and `crates/akantu-solid/tests/upstream_non_local_*.rs`
(TEST-077, TEST-078, TEST-080) and `crates/akantu-solid/tests/non_local_parallel.rs`
(TEST-079, TEST-080 at 2 / 4 ranks). The `.verified` files are compared as the
upstream test driver does (`diff -biw`: case and white space ignored, line
by line; `cmake/akantu_test_driver.sh`).

| File | Upstream path | SHA-256 |
|---|---|---|
| `material.dat` | `test/test_model/test_common/test_non_local_toolbox/material.dat` | `6043d36261155bd535c1eb3a10f960e4a9f0929aed67e4086b68d0d366a82082` |
| `material_avg.dat` | `test/test_model/test_common/test_non_local_toolbox/material_avg.dat` | `6d3cc5fceaf7168a3f8bbe47314eb97e769649ff16f760cd93706677c2653b78` |
| `material_remove_damage.dat` | `test/test_model/test_common/test_non_local_toolbox/material_remove_damage.dat` | `001fe159cfab7ff9cdac9c3b7c2503042ce6737c018c3d69dd7078e2f16fcaf6` |
| `material_weight_computation.dat` | `test/test_model/test_common/test_non_local_toolbox/material_weight_computation.dat` | `361efcf1665ec74cfe74f666d31800c88c826f2f707724ae6ca47ae9bfce3ef2` |
| `material_parallel_test.dat` | `test/test_model/test_common/test_non_local_toolbox/material_parallel_test.dat` | `e012f1beedaf25f4baf02ae330b4d8711a8bd7957fd9269e7ee66e590b3028df` |
| `pair_test.msh` | `test/test_model/test_common/test_non_local_toolbox/pair_test.msh` | `c95cb0828314b3cbb23468b83490948123bf6ff7e898af8f7e86c94b59f7ac1d` |
| `parallel_test.msh` | `test/test_model/test_common/test_non_local_toolbox/parallel_test.msh` | `8fc792ac308ba25fc2aeffcf7d92bb90803bff0610def621b39e9dd16c644e7b` |
| `plate.msh` | `test/test_model/test_common/test_non_local_toolbox/plate.msh` | `f7957922eeb0b7dbc4269928f622b90cde5f8699eefb6b8d1f29277bb891c888` |
| `test_non_local_neighborhood_base.verified` | `test/test_model/test_common/test_non_local_toolbox/test_non_local_neighborhood_base.verified` | `5f5074176d14041cd067cd871e2d694121a9130591008e9048b32d4a432b41ea` |
| `test_remove_damage_weight_function.verified` | `test/test_model/test_common/test_non_local_toolbox/test_remove_damage_weight_function.verified` | `cf3dddc77d5764b2740bf9b56b7857429d63f4e9c9129312e77c9dd5f15cf84c` |
| `test_weight_computation.verified` | `test/test_model/test_common/test_non_local_toolbox/test_weight_computation.verified` | `79afb3078609a89749f47587e42f5ca64c5ed5ea317f00380a4be934913f6d5b` |
