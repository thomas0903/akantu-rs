# Upstream test inputs

Verbatim copies of the input files of the upstream embedded-interface tests,
Akantu (EPFL/LSMS, LGPL-3.0-or-later) at commit
`35a851780d0b9471d258ed53a003ae98aaa95ea0`, directory
`test/test_model/test_solid_mechanics_model/test_embedded_interface/`:

| File | Used by (upstream test → Rust test) |
|---|---|
| `triangle.msh`, `embedded_element.dat` | `test_embedded_element_matrix` → `tests/test_embedded_element_matrix.rs` |
| `embedded_mesh.msh`, `material.dat`, `matrix` (expected stiffness, Matrix Market, 2015) | `test_embedded_interface_model` (+ `.sh` diff) → `tests/test_embedded_interface_model.rs` |
| `embedded_mesh_prestress.msh`, `embedded_mesh_prestress_reinforcement.msh`, `prestress.dat` | `test_embedded_interface_model_prestress` → `tests/test_embedded_interface_model_prestress.rs` |

The meshes are already meshed upstream (Gmsh 2.2 ASCII), so they are
platform-independent inputs. `sha256sum -c SHA256SUMS` in this directory
checks them.
