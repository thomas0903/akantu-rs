<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# akantu-geometry-cgal — GPL-3.0-or-later

This crate is the **GPL-licensed** part of the akantu-rs geometry port
(decision record D-042, user decision 2026-11-11). It contains Rust
translations of code from **CGAL 5.6.2** packages that CGAL distributes under
`GPL-3.0-or-later OR LicenseRef-Commercial`; this crate uses them under the
GNU General Public License, version 3 or (at your option) any later version
(`COPYING` at the repository root):

| Module | Derived from (CGAL 5.6.2) | CGAL package (license) |
|---|---|---|
| `aabb` | `AABB_tree/include/CGAL/AABB_tree.h`, `AABB_traits.h`, `AABB_tree/internal/AABB_node.h`, `AABB_tree/internal/AABB_traversal_traits.h` | AABB_tree (GPL-3.0-or-later) |
| `spherical` | `Circular_kernel_3/include/CGAL/Circular_kernel_3/{Line_arc_3.h, internal_functions_on_line_arc_3.h, internal_functions_on_sphere_3.h, internal_function_has_on_spherical_kernel.h}`, `Algebraic_kernel_for_spheres/include/CGAL/{Root_for_spheres_2_3.h, Polynomials_for_line_3.h, Algebraic_kernel_for_spheres/internal_functions_on_roots_and_polynomial_1_3_and_2_3.h, Algebraic_kernel_for_spheres/internal_functions_comparison_root_for_spheres_2_3.h}` | Circular_kernel_3, Algebraic_kernel_for_spheres (GPL-3.0-or-later) |
| `sphere_intersector` | Akantu `src/geometry/mesh_sphere_intersector*.hh`, the spherical part of `mesh_geom_factory_tmpl.hh`, `aabb_primitives/line_arc.hh`, `geom_helper_functions.hh` (LGPL-3.0-or-later Akantu code, conveyed here under GPL-3.0-or-later), built on `spherical` | — |

CGAL copyright holders of these files: INRIA Sophia-Antipolis (France),
GeometryFactory (France), ETH Zurich (Switzerland); authors Camille Wormser,
Pierre Alliez, Stéphane Tayeb, Monique Teillaud, Sylvain Pion, Pedro Machado,
Julien Hazebrouck, Damien Leroy (see the header of each source file).
Every source file starts with `SPDX-License-Identifier: GPL-3.0-or-later`,
names the CGAL file(s) it derives from and marks the GPL-derived parts.

## Consequences for users

* The crate is **opt-in**: the umbrella crate `akantu` enables it only with
  the Cargo feature `cgal` (OFF by default, like upstream's `CGAL` option),
  which the feature `igfem` implies (IGFEM needs the sphere intersector;
  `igfem` is OFF by default too). `embedded` (default) never implies it.
  A binary or library that includes this crate is distributed under
  **GPL-3.0-or-later as a whole**.
* No LGPL crate reaches it, in any feature set or dependency kind, except
  through the declared, off-by-default GPL switches: the umbrella's `cgal`
  and `igfem`, and `akantu-igfem`'s `cgal` (the sphere intersector of the
  growing-gel mesh generation, turned on by `igfem`). The guard
  `akantu-geometry/tests/license_separation.rs` checks the resolved
  dependency graph (an allow-list of declared GPL switches).
  Without it,
  `akantu-geometry` (LGPL) still provides the segment intersector with the
  corrected, traversal-independent segment ordering (`LinearScan`);
  this crate adds upstream's exact AABB traversal order (`AabbTree`,
  needed for `SegmentOrder::Upstream` bit-identity) and the sphere
  intersector of the IGFEM package.
* CGAL's LGPL parts (kernel, intersections, number types) are ported in the
  LGPL crate `akantu-geometry`; the libstdc++ behaviour (`std::nth_element`,
  `std::set`) is reproduced clean-room there (D-030).
