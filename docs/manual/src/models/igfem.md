<!-- availability: available akantu::igfem -->
# IGFEM (interface-enriched finite elements)

<!-- BEGIN GENERATED: availability-status igfem -->
**Available behind the Cargo feature `igfem`**, off by default (facade module `akantu::igfem`, crate `akantu-igfem`).
<!-- END GENERATED: availability-status igfem -->

Upstream: the package `igfem` of `extra_packages/` (parity rows IGFEM-*,
ELEM-024…026). The port's crate `akantu-igfem` is reached as `akantu::igfem`
with the facade feature `igfem`, or directly as `akantu_igfem`. The feature
is off by default, as upstream's package, and it implies the feature
`cgal`: the interface generation uses CGAL's spherical kernel, which the
port keeps in the GPL-3.0-or-later crate `akantu-geometry-cgal` (decision
D-042). A build that enables `igfem` is therefore GPL-3.0-or-later as a
whole ([Geometry and the `cgal` feature](../geometry.md)).

```toml
[dependencies]
akantu = { path = "…/akantu-rs/crates/akantu", features = ["igfem"] }
```

The crate provides the IGFEM element types (`_igfem_segment_3`,
`_igfem_triangle_4`, `_igfem_triangle_5`), their FE engine, the materials
`igfem_elastic`, `igfem_saw_tooth_damage` and
`igfem_iterative_stiffness_reduction`, the growing-gel mesh generation and
the solid mechanics model `SolidMechanicsModelIgfem`.

The materials combine two sub-materials named in the input file
(`name_sub_mat_1`, `name_sub_mat_2`): each integration point of an IGFEM
element takes the elastic constants of the sub-material of its side of the
interface. The damage laws take their damage parameters from their second
sub-material, a `damage_iterative` or `iterative_stiffness_reduction` law of
the extra materials, and are driven between solves like those laws
(`norm_max_equivalent_stress`, `update_damage`). A sub-material must be an
elastic law or one built on it: the model recognises it by its real
parameters `lambda`, `mu` and `kapa` and refuses `neohookean` and laws
without them, which also lets through laws that upstream refuses but that
register the same three parameters (UD-1210).

`SolidMechanicsModelIgfem::update` creates the IGFEM elements of an
interface; later calls, and `move_interface`, move them on the live model:
the replaced elements' damage, strength and other transferred internals are
copied into the new elements (upstream's `transferInternalValues`), and the
enriched nodes of the previous position are removed with their degrees of
freedom. Each update is two mesh mutations: the additions with the erasure
of the replaced elements, then the removal of the previous enriched nodes.

The model registers the dumper `"igfem elements"` as upstream: it writes
every IGFEM element as its two sub-elements, with `real_displacement` (the
displacement at the interface nodes) and the IGFEM material fields per
sub-element. With a non-local law, the non-local manager counts the
integration points of both element kinds (upstream's
`NonLocalManagerIGFEM`). The IGFEM model runs on one process: a
distributed mesh is refused (the parallel part of upstream's package is an
approved exception, decision D-085).

The reference is the crate documentation of `akantu::igfem` and the ports
of upstream's tests in `crates/akantu-igfem/tests/`, which show the use of
the model (for example `test_solid_mechanics_model_igfem.rs`); the
departures from upstream's code are the entries of
[`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md) that name them.
