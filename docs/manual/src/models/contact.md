<!-- availability: available akantu::contact -->
# Contact mechanics

<!-- BEGIN GENERATED: availability-status contact -->
**Available in this release** (facade module `akantu::contact`, crate `akantu-contact`, feature `contact`, on by default).
<!-- END GENERATED: availability-status contact -->

Upstream: `ContactMechanicsModel` (`contactmechanicsmodel.rst`; parity
rows CONT-001…018, DOC-012). The port's model,
`akantu::contact::ContactMechanicsModel`, detects contact between a
master and a slave surface and computes penalty contact forces and
stiffnesses with Coulomb friction:

* surface selectors (`PhysicalSurfaceSelector` on the element groups
  named by the `master` and `slave` parameters of the `contact_detector`
  section, `CohesiveSurfaceSelector` on cohesive facets,
  `AllSurfaceSelector` for both);
* detection (`ContactDetector`, section `contact_detector`): a global
  search with bounding boxes and a spatial grid, then the closest master
  node, the orthogonal projection on the master facets and the gap,
  normal and natural coordinates of every slave node in contact;
* resolutions (sections `contact_resolution <type> [ … ]` of the
  model's section or of the root): the types registered by the model's
  factory are listed in [Materials](../materials.md#registered-laws)
  (`penalty_linear`, `penalty_quadratic`), with the penalties
  `epsilon_n`, `epsilon_t`, the friction coefficient `mu` and
  `is_master_deformable`.

This chapter describes the model on its own. The couplers
(solid–contact, solid–cohesive–contact) are in [Model couplers](couplers.md). The model is serial (a distributed mesh is
refused, see below), and only linear master facets (`_segment_2`,
`_triangle_3`, `_quadrangle_4`) are supported: quadratic facets make the
search return `Error::NotImplemented` where upstream throws (UD-373; test
`contact_fixtures.rs::contact_fixtures_adversarial_quadratic_master`).

## A detection and a penalty force

Two blocks touch along `y = 0`: the lower block's top edge is the master
surface, the upper block's bottom edge the slave surface. The upper block
is moved down by `δ`, so both slave nodes penetrate the master edge by
`δ`; with `penalty_linear`, a rigid master (`is_master_deformable =
false`) and no friction, each slave node gets the force `ε_n δ A n`
(`A` its nodal area on the slave surface, `n` the master normal):

```rust
use akantu::contact::{ContactMechanicsModel, PhysicalSurfaceSelector};
use akantu::core::enums::AnalysisMethod;
use akantu::core::{Element, ElementType, GhostType};
use akantu::mesh::utils::fill_element_to_subelements_data;
use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu::parser::Parser;

// lower block [−1, 1] × [−1, 0], upper block [−0.5, 0.5] × [0, 1]
let (q, s, g) = (ElementType::Quadrangle4, ElementType::Segment2, GhostType::NotGhost);
let mut mesh = Mesh::new(2, "blocks")?;
{
    let mut a = MeshAccessor::new(&mut mesh);
    for x in [[-1.0, -1.0], [1.0, -1.0], [1.0, 0.0], [-1.0, 0.0]] {
        a.push_node(&x)?; // nodes 0…3
    }
    for x in [[-0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [-0.5, 1.0]] {
        a.push_node(&x)?; // nodes 4…7
    }
    // write guards on the connectivities, released at the end of the block
    let mut quads = a.connectivity_mut(q, g);
    quads.push_tuple(&[0, 1, 2, 3]);
    quads.push_tuple(&[4, 5, 6, 7]);
    drop(quads);
    let mut segments = a.connectivity_mut(s, g);
    segments.push_tuple(&[2, 3]); // master: top edge of the lower block
    segments.push_tuple(&[4, 5]); // slave: bottom edge of the upper block
}
for (name, el) in [("master", 0), ("slave", 1)] {
    mesh.groups_mut().create_element_group(name, 1)?;
    mesh.element_group_add(name, Element::new(s, el, g), true, true)?;
}
fill_element_to_subelements_data(&mut mesh)?;

let eps_n = 1e6;
let input = format!(
    "contact_detector [
        type = explicit
        master = master
        slave = slave
        projection_tolerance = 1e-10
        max_iterations = 100
        extension_tolerance = 1e-5
    ]
    contact_resolution penalty_linear [
        name = contact
        mu = 0
        epsilon_n = {eps_n}
        epsilon_t = 0
        is_master_deformable = false
    ]"
);
let parser = Parser::parse_str(&input, "contact.dat")?;
let selector = PhysicalSurfaceSelector::new(&mesh, &parser)?;
let mesh = SharedMesh::new(mesh);
let mut model = ContactMechanicsModel::with_parser(mesh.clone(), None, "contact", parser)?;
model.init_full(AnalysisMethod::ExplicitLumpedMass)?; // reads the resolutions
model.set_surface_selector(Box::new(selector))?;
assert_eq!(model.resolutions().len(), 1);
// the selector's surface is mirrored as the mesh group `contact_surface`
// (upstream's selectors create it; a contact model alone on its mesh)
assert_eq!(mesh.read()?.groups().element_group("contact_surface")?.nb_nodes(), 4);

// move the upper block down by δ (the model's current positions)
let delta = 1e-3;
for n in 4..8 {
    model.positions_mut().tuple_mut(n)[1] -= delta;
}
model.search()?;
assert_eq!(model.contact_elements().len(), 2); // the two slave nodes
for ce in model.contact_elements() {
    let n = ce.slave as usize;
    assert!((model.gaps().tuple(n)[0] - delta).abs() <= 1e-12 * delta);
    assert_eq!(model.normals().tuple(n), &[0.0, 1.0]); // out of the master
    assert_eq!(model.nodal_area().tuple(n)[0], 0.5); // half the slave edge
}
model.assemble_internal_forces()?;
for ce in model.contact_elements() {
    let forces = model.internal_force(); // a read guard on the shared array
    let f = forces.tuple(ce.slave as usize);
    let expected = eps_n * delta * 0.5;
    assert!(f[0].abs() <= 1e-12 * expected);
    assert!((f[1] - expected).abs() <= 1e-12 * expected);
}
# Ok::<(), akantu::core::Error>(())
```

## Differences from upstream

The corrected defaults and their switches (`ContactUpstreamCompat`;
`ContactUpstreamCompat::all()` reproduces upstream wherever upstream is
defined) are listed in the crate documentation of `akantu::contact` and in
[`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md) (UD-001,
UD-002, UD-008, UD-362…369, UD-371, UD-372). By default the normal and
friction tangents are consistent (UD-368, UD-369), the normals are
oriented on the current configuration (UD-371) and a projection is valid
when it lies in its facet (UD-372). A selector owns its contact surface
and mirrors it as the mesh's element group `contact_surface` when it is
set, as upstream's selectors create that group (the example above checks
it): `set_surface_selector` of a contact model alone on its mesh, of
`CouplerSolidContact` or of `CouplerSolidCohesiveContact` writes it, and
the solid–cohesive–contact coupler rewrites it in the facet mesh after
each insertion. The remaining observable differences
are listed in `akantu::contact::selector`.

The detection grid keeps upstream's cell edge, `√2 ·` the largest
in-radius of every facet around a master node (side facets included). With
side facets much longer than the contact-surface facets the cells become
large and a search slows down towards `O(slaves × masters)` (UD-1297).
`ContactMechanicsModel::set_grid_spacing(GridSpacing::ContactSurface)` (or
`ContactDetector::grid_spacing`) takes the contact-surface facets only; the
neighbourhood of a slave then covers another region, so the closest master
node found can differ from upstream's when it lies farther than one cell.

Contact mechanics on a distributed mesh is not part of this release and
is not planned (upstream has no distributed support for it either):
`init_full` refuses a mesh distributed over several processes with
`Error::Unsupported` (test
`manual_claims_2.rs::doc012_contact_model_refuses_a_distributed_mesh`).

Tests: `crates/akantu-contact/tests/`, ports of upstream's contact tests
(`contact_model.rs`: `test_explicit_resolution`, resolution
instantiation; `contact_detection.rs`: `test_detection` with analytical
gaps, normals and nodal areas), the friction tangents
(`contact_friction_tangent.rs`), the stiffness (`contact_stiffness.rs`,
finite differences) and the oracle replays of parity rows CONT-*
(`contact_fixtures.rs`, `contact_explicit.rs`); the availability of the
model and its resolution names:
`manual_claims_2.rs::doc012_contact_model_is_available`.
