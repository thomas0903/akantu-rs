# Upstream test reference files

Copied verbatim from the Akantu reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`, LGPL-3.0-or-later,
© EPFL/LSMS) for the ported tests in `crates/akantu-io/tests/`:

| File | Upstream path | SHA-256 |
|---|---|---|
| `test_mesh_io_msh_physical_names.verified` | `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.verified` | `2c8e56a40a8de243d43fe01597884a7cccbe2f60a3d53a6c94cca20758629c54` |

The strict oracle reproduces this file with the frozen mesh
`fixtures/meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_io/cube_physical_names.msh`
(`oracle/results/verified-check-strict.json`: match).
