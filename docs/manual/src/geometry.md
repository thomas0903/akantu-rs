<!-- availability: available akantu::geometry -->
<!-- availability: available akantu::geometry_cgal -->
# Geometry and the `cgal` feature

Upstream's `src/geometry` (package `CGAL`, built on the CGAL library)
computes intersections between a mesh and segments or spheres; the
embedded-reinforcement and IGFEM packages use it. The upstream manual has
no chapter on it. In the port it is split in two crates because of
CGAL's licences (decision D-042):

| Crate | Facade | Licence | Built |
|---|---|---|---|
| `akantu-geometry` | `akantu::geometry` | LGPL-3.0-or-later | always |
| `akantu-geometry-cgal` | `akantu::geometry_cgal` | GPL-3.0-or-later | only with the feature `cgal` (off by default) |

## `akantu::geometry` (LGPL)

* the inexact `Simple_cartesian<double>` kernel of CGAL (points, vectors,
  segments, rays, triangles, tetrahedra, predicates) and its intersection
  constructions (`kernel`, `intersections`; CGAL's LGPL packages), and the
  exact number types of upstream's spherical kernel (`number`);
* the mesh geometry factory (`MeshGeomFactory`: triangle primitives of the
  elements of a mesh) and the segment intersector `MeshSegmentIntersector`
  (upstream `MeshSegmentIntersector<dim, type>`), which cuts query segments
  by a mesh of `_triangle_3` (2-D) or `_tetrahedron_4` (3-D) elements into
  `_segment_2` elements of a result mesh, over a pluggable search structure
  (`TriangleIndex`; the LGPL ones are `LinearScan`, which tests every
  primitive, and `GridIndex`, a uniform grid that tests only the
  primitives near the query, used by the embedded model);
* the dynamic spatial grid `SpatialGrid` (upstream `aka_grid_dynamic.hh`)
  and `save_grid_as_mesh`.

```rust
use akantu::core::{Element, ElementType, GhostType};
use akantu::geometry::kernel::{Point3, Segment3};
use akantu::geometry::{LinearScan, MeshSegmentIntersector, SegmentIntersectorOptions};
use akantu::mesh::{Mesh, MeshAccessor};

// the unit square cut along its diagonal into two triangles
let mut mesh = Mesh::new(2, "square")?;
{
    let mut a = MeshAccessor::new(&mut mesh);
    for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
        a.push_node(&x)?;
    }
    // write guard on the connectivity, dropped at the end of this block
    let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
    c.push_tuple(&[0, 1, 2]); // below the diagonal
    c.push_tuple(&[0, 2, 3]); // above the diagonal
}
let intersector = MeshSegmentIntersector::<LinearScan>::new(
    &mesh,
    ElementType::Triangle3,
    SegmentIntersectorOptions::default(),
)?;

// a horizontal bar at y = 0.25 crosses the diagonal at x = 0.25
let bar = Segment3::new(Point3::new(0.0, 0.25, 0.0), Point3::new(1.0, 0.25, 0.0));
let mut bars = Mesh::new(2, "bars")?;
intersector.compute_intersection_query(&mesh, &bar, &mut bars)?;
assert_eq!(bars.nb_element(ElementType::Segment2, GhostType::NotGhost), 2);
let centre = |e| bars.barycenter(Element::new(ElementType::Segment2, e, GhostType::NotGhost));
// pieces in lexicographic order of their end points (UD-408)
assert_eq!(centre(0)?, [0.125, 0.25]); // in the upper triangle
assert_eq!(centre(1)?, [0.625, 0.25]); // in the lower triangle
# Ok::<(), akantu::core::Error>(())
```

Differences from upstream (D-003; `SegmentIntersectorOptions::upstream()`
selects upstream's behaviour for both switches):

| UD | Upstream | Port default | Switch |
|---|---|---|---|
| UD-407 | a `_tetrahedron_4` becomes the single flat triangle of its first three nodes (z dropped) | its four faces | `tetrahedra: TetrahedronPrimitives::UpstreamTriangle` |
| UD-408 | the pieces of a query are kept in a `std::set` with an invalid comparator (undefined behaviour): duplicates are not reliably removed and the order depends on the AABB traversal | exact lexicographic order on the end points, exact duplicates removed; independent of the search structure | `order: SegmentOrder::Upstream` |
| UD-072 | throws when the result mesh has no `_segment_2` connectivity | the connectivity is created | — |

Tests: `crates/akantu-geometry/tests/test_segment_intersection.rs` (ports
of upstream's `test_segment_intersection_triangle_3` and
`_tetrahedron_4`, which throw upstream because of UD-072),
`test_geometry_predicates.rs`, `test_grid.rs`, and the oracle replays of
the kernel (`oracle_kernel.rs`). The libstdc++ algorithms whose
unspecified behaviour upstream observes (`std::nth_element`, the
red–black tree of `std::set`) are reproduced clean-room from black-box
experiments (`akantu::geometry::cxx`; `crates/akantu-geometry/CLEANROOM_LOG.md`,
D-030).

## The `cgal` feature (GPL-3.0-or-later)

`akantu-geometry-cgal` contains Rust translations of CGAL code that CGAL
distributes under `GPL-3.0-or-later OR LicenseRef-Commercial` (packages
`AABB_tree`, `Circular_kernel_3`, `Algebraic_kernel_for_spheres`), used
under the GPL:

* `AabbTree`: CGAL's AABB tree with upstream's build and traversal order;
  `MeshSegmentIntersector<AabbTree>` with `SegmentOrder::Upstream`
  (`akantu::geometry_cgal::UpstreamSegmentIntersector`) reproduces
  upstream's segment intersector bit for bit (tests
  `crates/akantu-geometry-cgal/tests/oracle_geometry.rs::segment_intersector_*_matches_the_oracle`);
* the exact spherical kernel subset of upstream (line arcs, spheres,
  circular-arc points) and `MeshSphereIntersector`, upstream's
  sphere–mesh intersector of the IGFEM package (tests
  `oracle_geometry.rs::sphere_intersector_*_matches_the_oracle`). Upstream's
  class does not compile (UD-409); the port implements its evident intent,
  and the oracle data come from a probe-local copy with compile fixes only.

It is enabled with

```toml
[dependencies]
akantu = { path = "…/akantu-rs/crates/akantu", features = ["cgal"] }
```

and is then `akantu::geometry_cgal`. The facade feature `igfem` implies
`cgal`; no other feature does, and none of the default features.

Licence consequence. A program or library built with the feature
`cgal` (or `igfem`) includes GPL-3.0-or-later code and is therefore
distributed under the **GNU General Public License, version 3 or later,
as a whole**. Without the feature no GPL code is compiled into a program
that uses the facade (a `cargo build --workspace` of the repository
compiles the GPL crate too, as a member of the workspace, but links it
into nothing): the port's crates in such a program are LGPL-3.0-or-later and the third-party dependencies
keep their own (permissive or LGPL-compatible) licences
([`docs/DEPENDENCIES.md`](../../DEPENDENCIES.md)). The test suite checks
the separation on the resolved dependency graph
(`crates/akantu-geometry/tests/license_separation.rs`): the facade with its
default features, with each other feature alone and with all of them
together never reaches the GPL crate, nor does any other LGPL crate of the
workspace with all its features; `cgal` and `igfem` do, and `igfem`
implies `cgal`. Every file of the GPL crate carries the GPL SPDX header and
the CGAL notices (`crates/akantu-geometry-cgal/tests/license_headers.rs`).
The crate's README (`crates/akantu-geometry-cgal/README.md`) lists the CGAL
source files and copyright holders.

Without `cgal`, the segment intersector of `akantu::geometry` gives the
corrected (UD-408) result, which does not depend on the search structure
(test `test_segment_intersection_aabb.rs::default_output_is_independent_of_the_search_structure`
in the GPL crate compares `LinearScan` with `AabbTree`); only the
upstream-ordered output and the sphere intersector need the GPL crate.
