<!-- availability: available akantu::embedded -->
# Embedded interfaces

<!-- BEGIN GENERATED: availability-status embedded -->
**Available in this release** (facade module `akantu::embedded`, crate `akantu-embedded`, feature `embedded`, on by default).
<!-- END GENERATED: availability-status embedded -->

Upstream: the embedded interface model (`EmbeddedInterfaceModel`,
reinforcement laws; parity rows EMB-*).
The port's crate `akantu-embedded` is reached as `akantu::embedded`
(facade feature `embedded`, on by default as upstream's package) or
directly as `akantu_embedded`.

Per-feature descriptions are kept in the EMB rows of
[`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md); the departures from
upstream's code are the entries of
[`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md) that name them.

## Search structure of the intersection

The interface mesh is built by cutting every reinforcement segment by the
background elements. The port searches the elements near each segment with
a uniform grid (`akantu_geometry::GridIndex`), where upstream uses CGAL's
AABB tree; `EmbeddedInterfaceIntersector::exhaustive_search = true` tests
every element instead (`LinearScan`, quadratic cost). Both use upstream's
predicates. They differ only where the exhaustive scan reports a distant
element through rounding noise (a segment lying, up to rounding, on the line
of a distant edge or in the plane of a distant face): the grid, like
upstream's tree, does not test that element. With the default assembly of
the pieces (`remove_near_duplicates = true`) no difference was found; with
upstream's collected set the scan can attach a piece to the distant element
where the grid and upstream keep the element containing it. The
documentation of `akantu_embedded::intersector` gives the details and the
tests.
