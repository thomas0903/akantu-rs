<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Contact detection, spatial grid and embedded intersection

Bitwise-identical speed-ups of the embedded-reinforcement intersection
set-up, the contact detector, the shared spatial grid and the node-to-node
contact callback. Every suite digest is unchanged.

## What changed

| Change | Files | What it removes |
|---|---|---|
| `GridIndex`: a uniform grid over the triangle primitives of the segment intersector; each query tests only the primitives near it, in primitive order, with the predicates of `LinearScan` | `crates/akantu-geometry/src/index.rs` | the `O(segments × elements)` scan of the embedded set-up (40–49 % of the embedded profiles before) |
| The embedded intersector uses `GridIndex` (`exhaustive_search = true` keeps `LinearScan`) | `crates/akantu-embedded/src/intersector.rs` | |
| Spatial grid cells in hashed slots (open addressing on the ids, lookups without a `CellId` allocation), lexicographic order built on demand | `crates/akantu-core/src/spatial_grid.rs` | `BTreeMap` walks with `Vec` key comparisons in `NeighborhoodItems::next` and `insert` |
| Outward directions of the facets memoised within one detection (`OutwardFrame::CurrentCached`) | `crates/akantu-contact/src/{geometry,detector}.rs` | repeated barycentres of a facet and its neighbour for every projection |
| In-radius of each facet taken once per detection | `crates/akantu-contact/src/detector.rs` | the in-radius of a facet once per master node around it |
| Reference barycentres of the nodal areas from node arrays read once | `crates/akantu-contact/src/{geometry,model}.rs` | a node-array lock per barycentre |
| Node flags read once in the node-to-node work increment | `crates/akantu-ntn-contact/src/solver_callback.rs` | two checked flag lookups per node and step |
| Opt-in `GridSpacing::ContactSurface` (default: upstream's rule) | `crates/akantu-contact/src/detector.rs` | degenerate detection grids with elongated side facets (opt-in: the closest master found can differ) |

### Why the grid reports the scan's intersections

`GridIndex` hands a query every primitive whose bounding box comes within
a quarter cell of the query (the query is walked in pieces about one cell
long; the cell coordinate is a monotone function of the position, so the
selection is conservative). The candidates are sorted by primitive order and
tested with the scan's predicates, so the output is the scan's output
restricted to them, in the scan's order. The scan also reports primitives
farther away when the query lies, up to rounding, in the plane of a distant
face or on the line of a distant edge. The `Simple_cartesian<double>`
orientations of such nearly coplanar points are rounding noise, and the
report is a point or piece attached to a distant element. Upstream's AABB
tree does not produce it either (its box test rejects the primitive).
`tests/grid_index.rs` and `tests/grid_index_intersector.rs` of
`akantu-geometry` check this on structured meshes with inexact coordinates
(multiples of 0.1), jittered meshes and degenerate queries (along grid lines
and edges, in face planes, through vertices, zero length, partly outside):

- the grid's reports are always the scan's reports in the scan's order;
- every report the grid omits is from a primitive whose true distance to
  the query exceeds the grid's margin (216 such reports among 43 653 hits
  and 147 399 ray hits on a 4³-cube Kuhn mesh with `h = 0.1`, none on the
  jittered triangle meshes);
- with the embedded model's default assembly (`PieceAssembly::Tiled`) the
  pieces of every query are identical: the omitted reports do not lie in
  their element and the tiling drops them. With `PieceAssembly::Collected`
  (upstream's raw set, `remove_near_duplicates = false`) the scan keeps a
  piece attached to a distant element, which the grid omits, in 4 of 16 727
  pieces (2-D, 7 × 7 cells) and 6 of 25 459 pieces (3-D, 5³ cubes).

## Measurements

An AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB), Ubuntu 24.04, release
profile, one process per run pinned to core 2, `scripts/perf/suite/run_ab.sh`
with 5 interleaved repetitions, order rotated per repetition, 2026-11-16
12:54–13:05 UTC. Before: `suite_models` / `suite_solid` at 24f73b94e (the
base of this branch). After: c534e539f. Size `m`. Raw data: `final/`,
`final-solid/`; the full table (all phases, ns per element and call,
digests): `final-summary.md`.

| Case | Phase | Before (s) | After (s) | Ratio after/before, median [min, max] | Digest |
|---|---|---|---|---|---|
| `embedded_rebar_tri3` | `intersect` (5 calls) | 7.827 [7.489, 8.597] | 0.0628 [0.0621, 0.0632] | 0.008 [0.007, 0.008] | same |
| `embedded_rebar_tri3` | `init` | 1.585 [1.517, 1.831] | 0.0320 [0.0313, 0.0337] | 0.021 [0.018, 0.021] | same |
| `embedded_rebar_tet4` | `intersect` (5 calls) | 7.175 [6.992, 7.458] | 0.121 [0.119, 0.136] | 0.017 [0.017, 0.018] | same |
| `embedded_rebar_tet4` | `init` | 1.421 [1.417, 1.446] | 0.0400 [0.0393, 0.0459] | 0.028 [0.027, 0.032] | same |
| `contact_search_quad4` | `search` (60 calls) | 5.378 [5.345, 5.405] | 4.474 [4.430, 4.498] | 0.832 [0.829, 0.835] | same |
| `contact_search_tet4` | `search` (30 calls) | 5.098 [5.068, 5.170] | 4.155 [4.139, 4.188] | 0.817 [0.804, 0.819] | same |
| `contact_explicit_quad4` | `solve_step` (200) | 4.642 [4.187, 4.846] | 4.413 [4.102, 4.664] | 0.962 [0.938, 0.988] | same |
| `ntn_friction_quad4` | `solve_step` (100) | 3.465 [3.176, 3.836] | 3.178 [3.024, 3.390] | 0.944 [0.829, 0.955] | same |
| `contact_implicit_static_quad4` | `solve_step` (5) | 2.135 [2.085, 2.147] | 2.100 [2.071, 2.137] | 0.993 [0.976, 1.000] | same |
| `contact_cohesive_quad4` | `solve_step` (300) | 8.093 [7.533, 8.357] | 7.870 [7.322, 9.369] | 0.972 [0.959, 1.121] (no measurable change) | same |
| `explicit_mazars_non_local` (`suite_solid`) | `nonlocal_rebuild` (2) | 1.647 [1.636, 1.648] | 1.613 [1.593, 1.677] | 0.979 [0.967, 1.025] (no measurable change) | same |
| `explicit_mazars_non_local` (`suite_solid`) | `init` | 0.884 [0.884, 0.895] | 0.864 [0.864, 0.906] | 0.978 [0.978, 1.014] (no measurable change) | same |

The other phases of these cases (mesh generation, model set-up, the
embedded assemblies and solve, the non-local step) show no measurable
change (`final-summary.md`).

**Scaling of the embedded set-up.** `embedded_rebar_tri3` at size `l`
(157 k triangles, 41 k bar segments, about 8× the elements and segments of
`m`), after only (`large/`): `intersect` 0.658 s for 5 calls (0.13 s per
call, 10.5× the per-call time at `m`). The scan grows with the product,
about 64× the 1.57 s per call at `m` (100 s); the model-suite calibration
measured 113 s at 9× on a development VM.

## Profiles after the changes

`profiles/<case>.txt`: `perf record --call-graph dwarf` of the binary at
faf1b35c7 (before the node-to-node change; the contact and embedded code
is that of c534e539f), size `m`, `--loop-seconds 6`, core 5 of the same
machine. Call chains did not unwind inside the hot loops on this machine,
so only self shares are reliable:

- `contact_search_quad4` / `_tet4`: the small dense products of the
  projections and nodal areas (`eigen_gebp::product_into`, `route`,
  `gemv_into`, `eigen_order::redux_tree`, `fe::eigen::product_slices`)
  take 29 % / 38 % of self time. The other remaining items are the
  barycentres of the current positions (`barycenter_on`, 6 % / 4.2 %, the
  first orientation of each facet per detection), the reference
  barycentres of the nodal areas (`barycenter_of`, 5 % / 2.4 %), the FE
  boundary engine re-initialised on every boundary facet by the nodal
  areas (`normals_on_natural`, `jacobian`, `compute_jacobians`, about 5 %),
  and the grid walk (`NeighborhoodItems::next`, 2.7 % / 3.5 %, 11 % before).
- `contact_explicit_quad4`, `ntn_friction_quad4`: the solid explicit step
  (`fast::btd`, `fast::gradient`, `fast::integrate`, the residual scatter,
  the elastic law, libc `memset`/`memmove` of the solid's temporaries). No
  contact-detection symbol reaches the top 25 (each below 0.9 %); the
  node-to-node callback's `work_increment` was 3.3 % before its change.
- `embedded_rebar_tri3` / `_tet4`: the intersection is no longer in the
  top symbols of the assemblies; inside `intersect` the grid build over the
  81 k faces of the 3-D case is about a third of a call (once per set-up in
  real use).
