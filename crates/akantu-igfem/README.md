<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# akantu-igfem — LGPL-3.0-or-later

Port of Akantu's `extra_packages/igfem` (interface-enriched generalized FEM):
IGFEM element classes (`_igfem_segment_3`,
`_igfem_triangle_4`, `_igfem_triangle_5`), the IGFEM FE engine,
`SolidMechanicsModelIGFEM` (moving interfaces on a live model, the transfer
of internals, the `"igfem elements"` dumper, the IGFEM point source of the
non-local manager), the laws `igfem_elastic`, `igfem_saw_tooth_damage` and
`igfem_iterative_stiffness_reduction`, and the spherical growing-gel mesh
generation. Serial (parallel IGFEM: approved exception D-085). The
upstream package does not compile at the reference commit:
verification is by independent checks and upstream's `.verified` outputs
(decision D-007; `docs/NUMERICAL_VALIDATION.md`).

## Licensing

Every source file of this crate is **LGPL-3.0-or-later** (Akantu code; no
GPL-derived code). The sphere–mesh intersection used by the growing-gel
mesh generation is a port of CGAL's GPL spherical kernel and lives in the
GPL-3.0-or-later crate `akantu-geometry-cgal` (decision D-042). This crate
depends on it **optionally**, through its feature `cgal`:

* without `cgal`: element classes, FE engine, model, materials and the
  interface-geometry trait (LGPL build);
* with `cgal`: also `growing_gel` and the spherical enrichment
  (`SphereGeometry`, `SphericalEnrichment`, `SolidMechanicsModelIgfem::{update,
  move_interface, register_geometry_object}`). **A build with `cgal` is
  distributed under GPL-3.0-or-later as a whole.**

The umbrella crate's feature `igfem` enables `akantu-igfem/cgal` and implies
`cgal`; it is OFF by default, like upstream's `IGFEM`
CMake package. A build that enables `cgal` or `igfem` is GPL-3.0-or-later as
a whole (D-042). `akantu-igfem/cgal`, `akantu/cgal` and `akantu/igfem` are
the declared GPL switches of the licence guard
(`crates/akantu-geometry/tests/license_separation.rs`):
none may be a default feature or be turned on by another crate's default
features, and no other feature may reach the GPL crate. The inside test of
the spheres (`has_on_unbounded_side`, CGAL Kernel_23 semantics, LGPL) is
evaluated with the LGPL number types of `akantu-geometry`.

Tests that need the mesh generation are `[[test]]` targets with
`required-features = ["cgal"]`: run them with
`cargo test -p akantu-igfem --features cgal`.
