<!-- availability: available akantu::couplers -->
# Model couplers

<!-- BEGIN GENERATED: availability-status couplers -->
**Available in this release** (facade module `akantu::couplers`, crate `akantu-couplers`, feature `couplers`, on by default).
<!-- END GENERATED: availability-status couplers -->

Upstream: the solid–contact, solid–cohesive–contact and
solid–phase-field couplers of `model_couplers` (parity rows CPL-*), and
the sections *Coupling with SolidMechanicsModel* and *Coupling with
SolidMechanicsModelCohesive* of upstream's contact chapter
(`contactmechanicsmodel.rst`; parity row DOC-012). The port's crate
`akantu-couplers` is reached as `akantu::couplers` with the facade
feature `couplers`, which is on by default.

| Upstream class | Type of the port | Coupling |
|---|---|---|
| `CouplerSolidContact` | `CouplerSolidContact` | monolithic: one solver for the solid and the contact mechanics model |
| `CouplerSolidCohesiveContact` | `CouplerSolidCohesiveContact` | monolithic: the contact part lives on the facet mesh of the cohesive model |
| `CohesiveContactSolverCallback` | `CohesiveContactSolverCallback` | the stand-alone callback of the cohesive contact coupler |
| `CouplerSolidPhaseField` | `CouplerSolidPhaseField` | staggered: the solid and the phase-field model alternate (strain and damage transfer) |

Every coupler owns its component models, as public fields (upstream's
`getSolidMechanicsModel()`, `getContactMechanicsModel()`,
`getPhaseFieldModel()` return references to them): `solid` and `contact`
for the contact couplers, `solid` and `phase` for the phase-field
coupler. The models are set up through these fields (material
selector, contact surface selector, boundary conditions, dumpers) and
solved through the coupler.

## Coupling the solid and the contact mechanics models

`CouplerSolidContact::with_parser(mesh, dim, id, parser)` creates the
solid mechanics model (`solid`, id `solid_mechanics_model`) and the
contact mechanics model (`contact`, id `contact_mechanics_model`) on the
same mesh, from the same input file: the solid reads its `material`
sections, the contact model its `contact_detector` section (required)
and its `contact_resolution` sections ([Contact
mechanics](contact.md)), and the coupler the solvers of its
`model coupler_solid_contact [ … ]` section if the file has one.
`init_full(options)` creates the coupler's default solver for the
analysis method (the solid's default solver type and options), then
initialises the solid and the contact model. The contact surface
selector is set on `contact` (`set_surface_selector`), the material
selector on `solid`, the boundary conditions through the coupler
(`apply_dirichlet_bc`, `apply_neumann_bc`, forwarded to the solid).

The coupling is monolithic: one solver, on the solid's DOF manager,
drives a combined solver callback that calls the parts in upstream's
order:

* explicit dynamics (`ExplicitLumpedMass`): every `solve_step`
  runs the solid's predictor; then, for the residual, the positions of
  the contact detector are set to the solid's current positions
  (`X + u`), the contact search runs, the solid's and the contact
  model's internal forces are assembled and the contact forces enter the
  solid's residual; then the solid's corrector;
* statics and implicit dynamics: the stiffness is the solid's plus
  the contact stiffness; the contact search runs once before the first
  residual of the step and after every Newton correction, on the current
  positions; the friction history of the contact model is committed once
  per converged step (upstream commits it at every Newton iterate, so the
  friction residual changes from one iteration to the next and a sliding
  step does not converge: UD-374;
  `SolidContactCompat::per_iteration_friction_history`, set with
  `set_upstream_compat`, reproduces it).

The search before the first residual is a correction of the port
(UD-1291). Upstream searches in the Newton corrector only, and Newton
with the `residual` criterion tests the residual of the predictor before
any corrector: a step whose first residual already converges keeps the
contact state of the previous step (two bodies pushed into each other by
prescribed displacements end the step with a penetration and no contact
force), and every first residual is assembled on the previous step's
contact set. The port runs the search on the predicted positions (after
the time-step solver's integration-scheme predictor, at the first
assembly of the step: the residual, or the stiffness when a solver
assembles it first), without touching the friction history.
`SolidContactCompat::search_in_corrector_only` restores upstream's order;
`SolidContactCompat::upstream()` sets it with the UD-374 switch, and
`SolidContactCompat::all()` (for the cohesive coupling below) also with
the UD-772 one. Two properties of the Newton criteria remain as upstream
has them: the `solution` criterion (the default) declares convergence on
the last increment and never tests the residual of the contact set that
the last correction's search found, so prefer `convergence_type =
residual` with contact (UD-1293); and the `residual` criterion tests later
iterations relative to the first residual, so a step whose first residual
is tiny but above the threshold may not reach `threshold × ‖r₀‖` above
the rounding floor (UD-1294). The `linear` non-linear solver is not a contact
solver: the couplers never compute a contact force inside its step, in either
code (UD-1295), so use Newton–Raphson. The couplers are serial in both codes.

The stable time step is the solid's (`stable_time_step`), and
`set_time_step` sets the shared solver's. `contact_force()` returns, as
upstream's `getContactForce()`, the solid's internal force; the contact
forces themselves are `contact.internal_force()` (UD-623).

```rust
use akantu::contact::PhysicalSurfaceSelector;
use akantu::core::enums::AnalysisMethod;
use akantu::core::{Element, ElementType, GhostType};
use akantu::couplers::CouplerSolidContact;
use akantu::mesh::utils::fill_element_to_subelements_data;
use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;

// the two blocks of the contact chapter: lower block [−1, 1] × [−1, 0]
// (master: its top edge), upper block [−0.5, 0.5] × [0, 1] (slave: its
// bottom edge)
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
    let mut quads = a.connectivity_mut(q, g);
    quads.push_tuple(&[0, 1, 2, 3]);
    quads.push_tuple(&[4, 5, 6, 7]);
    drop(quads);
    let mut segments = a.connectivity_mut(s, g);
    segments.push_tuple(&[2, 3]);
    segments.push_tuple(&[4, 5]);
}
for (name, el) in [("master", 0), ("slave", 1)] {
    mesh.groups_mut().create_element_group(name, 1)?;
    mesh.element_group_add(name, Element::new(s, el, g), true, true)?;
}
fill_element_to_subelements_data(&mut mesh)?;

let (rho, eps_n) = (2.0, 1e6);
let input = format!(
    "material elastic [
        name = block
        rho  = {rho}
        E    = 1e9
        nu   = 0.3
    ]
    contact_detector [
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
let parser = Parser::parse_str(&input, "coupler.dat")?;
let selector = PhysicalSurfaceSelector::new(&mesh, &parser)?;
let mut coupler =
    CouplerSolidContact::with_parser(SharedMesh::new(mesh), None, "coupler_solid_contact", parser)?;
coupler.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
coupler.contact.set_surface_selector(Box::new(selector))?;
let dt = 0.1 * coupler.stable_time_step()?;
coupler.set_time_step(dt, None)?;

// the upper block starts δ into the lower one (a rigid translation: no
// stress in the solid, only the contact pushes)
let delta = 1e-4;
for n in 4..8 {
    coupler.solid.displacement_mut()?.tuple_mut(n)[1] = -delta;
}
coupler.solve_step(None)?;

// each slave node carries the penalty force ε_n δ A (nodal area A = 0.5)
// and, with its lumped mass ρ·area/4 = ρ/4, the acceleration (ε_n δ A)/(ρ/4)
let force = eps_n * delta * 0.5;
for n in [4, 5] {
    let f = coupler.contact.internal_force().tuple(n)[1];
    assert!((f - force).abs() <= 1e-12 * force);
    let a = coupler.solid.acceleration()?.tuple(n)[1];
    assert!((a - force / (rho / 4.0)).abs() <= 1e-12 * force / (rho / 4.0));
}
// the rigid master gets no contact force
assert_eq!(coupler.contact.internal_force().tuple(2), &[0.0, 0.0]);
# Ok::<(), akantu::core::Error>(())
```

## Contact between cohesive crack faces

`CouplerSolidCohesiveContact` couples the cohesive model
`SolidMechanicsModelCohesive` (`solid`, id
`solid_mechanics_model_cohesive`) with a contact mechanics model built
on the facet mesh (`contact`), so that the faces of the cracks the
cohesive elements open can come into contact. Its use is that of
`CouplerSolidContact` with three differences:

* `init_full(options, is_extrinsic)` takes the kind of cohesive
  insertion ([Cohesive elements](cohesive.md#using-the-model));
* the contact surface selector is a `CohesiveSurfaceSelector` (the
  cohesive facets) or an `AllSurfaceSelector` (the cohesive facets and
  the physical surfaces);
* the coupler's `check_cohesive_stress()` inserts the cohesive elements
  (it calls the cohesive model's) and gives the new ones to the contact
  surface selector (upstream: the selector receives them as a mesh
  event); `solve_step` does the same for elements inserted otherwise. A
  program that inserts through `solid` directly calls
  `forward_new_cohesive_elements()` before reading the contact surface.
  Upstream's manual and its example call `checkCohesiveStress` after
  `solveStep`; in explicit dynamics calling it before the next
  `solve_step` is equivalent (the check uses the bulk stresses of the last
  residual assembly).

The contact stiffness of a static or implicit run is added to the
stiffness that is solved; upstream assembles it into the contact model's
own DOF manager, a matrix that is never solved, so its solved stiffness
is the solid's alone (UD-772; `SolidContactCompat::contact_stiffness_in_own_dof_manager`
reproduces it).

```rust
use akantu::contact::CohesiveSurfaceSelector;
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::couplers::CouplerSolidCohesiveContact;
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
#     "../../fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
#      test_cohesive/_cohesive_2d_4_triangle_3.msh",
# );
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Gmsh)?;
// the plate of the cohesive chapter (2 × 1, the physical line `insertion` at y = 0)
let input = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = body
    rho  = 1e3
    E    = 1e9
    nu   = 0.2
  ]
  material cohesive_linear [
    name    = interface
    beta    = 1
    G_c     = 10
    sigma_c = 1e6
  ]
]
contact_detector [
  type = explicit
  master = insertion
  slave = insertion
  projection_tolerance = 1e-10
  max_iterations = 100
  extension_tolerance = 1e-5
]
contact_resolution penalty_linear [
  name = interface
  mu = 0
  epsilon_n = 1e10
  epsilon_t = 0
  is_master_deformable = true
]";
let parser = Parser::parse_str(input, "coupler.dat")?;
let mesh = SharedMesh::new(mesh);
let mut coupler =
    CouplerSolidCohesiveContact::with_parser(mesh.clone(), None, "coupler_solid_contact", parser)?;
coupler.contact.set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))?;
coupler.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)?;
let dt = 0.1 * coupler.stable_time_step()?;
coupler.set_time_step(dt, None)?;

// stretch the plate vertically above the strength: the coupler inserts the
// five cohesive elements of `insertion` and hands them to the contact surface
let x: Vec<f64> = mesh.read()?.nodes().as_slice().to_vec();
for (u, x) in coupler.solid.smm.displacement_mut()?.tuples_mut().zip(x.chunks(2)) {
    u[1] = 1e-3 * x[1];
}
coupler.solid.assemble_internal_forces()?;
assert_eq!(coupler.check_cohesive_stress()?, 5);
coupler.solve_step(None)?; // the crack is open: no contact element
assert!(coupler.contact.contact_elements().is_empty());
# Ok::<(), akantu::core::Error>(())
```

## Output of the coupled models

The three couplers have the model-level dumper calls of upstream's
models: `set_base_name`, `set_directory`, `add_dump_field`,
`add_dump_field_vector`, `add_dump_field_tensor`, their `…_to_dumper`
forms, `remove_dump_field`, `dump`, `dump_named` and `dump_group`
(module `akantu::couplers::dump`; [Output and dumpers](../output.md)
describes the fields and the files). A field name is resolved
type-major, as upstream's coupler does: the real nodal fields (the
contact model's, then the solid's), then the integer, the boolean and
the elemental ones, then the mesh data; so `blocked_dofs`, which both
models know, is the contact model's. A name that nothing resolves, an
unknown dumper and a field that the part dumping alone cannot read are
errors, never silent omissions.

Upstream registers every model's dumper on the mesh and makes the last
one the mesh's default; `setBaseName`, `addDumpField*` and `dump()`
act on that default whichever model they are called on. The port's
models own their dumpers, so each coupler's calls act on the dumper of
the part that upstream's mesh default belongs to (UD-982, category C;
the series' file names start with the base name, or with the id of the
part when no base name is set):

| Coupler | Default series of the coupler's calls | File base name by default |
|---|---|---|
| `CouplerSolidContact` | the contact model's dumper | `contact_mechanics_model` |
| `CouplerSolidCohesiveContact` | the cohesive solid's dumper (upstream builds the contact model on the facet mesh, so the bulk mesh's default is the solid's) | `solid_mechanics_model_cohesive` |
| `CouplerSolidPhaseField` | the solid's dumper, renamed to the phase-field model's id | `phase_field_model` |

The phase-field part of a `CouplerSolidPhaseField` keeps its own series
under the base name `phase_field_model_part`, so its `dump()` never
overwrites the coupler's files (upstream has one series; a field that
the phase-field part registers is not in the coupler's series, upstream:
it is). The part calls that upstream sends to the mesh default reach the
coupler's series through `dump_contact` (the contact model's dump with
the solid's fields), `solid_set_time_step` (the series' time step, which
writes the `.pvd`), `solid_add_dump_field` and, for the cohesive coupler,
`dump_solid`; the calls of the parts themselves (`coupler.solid.…`,
`coupler.contact.…`) act on the part's own dumper. The couplers'
`set_time_step` is the solver's: it gives the dumper no time step, so no
`.pvd`, as upstream. The tests are in
`crates/akantu-couplers/tests/coupler_dump.rs`:
`solid_contact_default_series_is_the_contact_models_dumper`,
`solid_cohesive_contact_default_series_is_the_cohesive_solid_models`,
`phase_field_default_series_is_named_after_the_phase_field_model`,
`solid_part_calls_reach_the_coupler_series` and
`solid_contact_refusals_are_errors_never_silent`.

```rust
# use akantu::contact::PhysicalSurfaceSelector;
# use akantu::core::enums::AnalysisMethod;
# use akantu::core::{Element, ElementType, GhostType};
# use akantu::couplers::CouplerSolidContact;
# use akantu::mesh::utils::fill_element_to_subelements_data;
# use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# let (q, s, g) = (ElementType::Quadrangle4, ElementType::Segment2, GhostType::NotGhost);
# let mut mesh = Mesh::new(2, "blocks")?;
# {
#     let mut a = MeshAccessor::new(&mut mesh);
#     for x in [[-1.0, -1.0], [1.0, -1.0], [1.0, 0.0], [-1.0, 0.0]] {
#         a.push_node(&x)?;
#     }
#     for x in [[-0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [-0.5, 1.0]] {
#         a.push_node(&x)?;
#     }
#     let mut quads = a.connectivity_mut(q, g);
#     quads.push_tuple(&[0, 1, 2, 3]);
#     quads.push_tuple(&[4, 5, 6, 7]);
#     drop(quads);
#     let mut segments = a.connectivity_mut(s, g);
#     segments.push_tuple(&[2, 3]);
#     segments.push_tuple(&[4, 5]);
# }
# for (name, el) in [("master", 0), ("slave", 1)] {
#     mesh.groups_mut().create_element_group(name, 1)?;
#     mesh.element_group_add(name, Element::new(s, el, g), true, true)?;
# }
# fill_element_to_subelements_data(&mut mesh)?;
# let input = "material elastic [
#         name = block
#         rho  = 2
#         E    = 1e9
#         nu   = 0.3
#     ]
#     contact_detector [
#         type = explicit
#         master = master
#         slave = slave
#         projection_tolerance = 1e-10
#         max_iterations = 100
#         extension_tolerance = 1e-5
#     ]
#     contact_resolution penalty_linear [
#         name = contact
#         mu = 0
#         epsilon_n = 1e6
#         epsilon_t = 0
#         is_master_deformable = false
#     ]";
# let parser = Parser::parse_str(input, "coupler.dat")?;
# let selector = PhysicalSurfaceSelector::new(&mesh, &parser)?;
# let mut coupler =
#     CouplerSolidContact::with_parser(SharedMesh::new(mesh), None, "coupler_solid_contact", parser)?;
# coupler.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
# coupler.contact.set_surface_selector(Box::new(selector))?;
let dir = std::env::temp_dir().join(format!("akantu-manual-coupler-dump-{}", std::process::id()));
std::fs::create_dir_all(&dir)?;
coupler.set_directory(&dir)?;
coupler.set_base_name("blocks")?;
coupler.add_dump_field("gaps")?; // the contact model's field
coupler.add_dump_field_vector("displacement")?; // the solid's field
coupler.dump(None, None)?;
let mut written: Vec<String> = std::fs::read_dir(&dir)?
    .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
    .collect::<Result<_, _>>()?;
written.sort();
// the series is named after the base name: `blocks_0000.pvtu` and its directory `blocks-VTU`
assert_eq!(written, ["blocks-VTU", "blocks_0000.pvtu"]);
// a name that nothing resolves is an error, never silently omitted
assert!(coupler.add_dump_field("no_such_field").is_err());
std::fs::remove_dir_all(&dir)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## The solid–phase-field coupler

`CouplerSolidPhaseField` is staggered: the solid and the phase-field
model keep their own solvers and time steps, and `solve(…)` runs a solid
solve, the transfer of the strain to the phase-field model, a phase-field
solve and the transfer of the damage to the solid's `phasefield`
material, in that order (upstream `coupler_solid_phasefield.cc`). It is
described with the phase-field model in [Phase-field
fracture](phase-field.md); it runs on a distributed mesh, as its two
models do; upstream's coupler tags `_csp_damage` and `_csp_strain` carry
no data (UD-901). By default the damage is transferred only to the
elements of the phase-field laws' filters (upstream transfers it
unfiltered, UD-382).

## Distributed meshes

The contact couplers run on one process: on a distributed mesh the
contact mechanics model refuses its entry points, as it does on its own
([Contact mechanics](contact.md)).

The upstream defects the port corrects by default in this package, each
with a switch that reproduces upstream, are entries of
[`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md) (UD-374,
UD-382, UD-617…624, UD-772…776). Tests:
`crates/akantu-couplers/tests/`: `contact_coupler_fixtures.rs` and
`coupler_fixtures.rs` (replays of upstream-built oracle cases),
`contact_static.rs` (the static coupling: contact search in every Newton
correction, contact stiffness in `K`), `friction_history.rs` (UD-374),
`cohesive_contact.rs` (crack faces in contact), `cohesive_static.rs` and
`cohesive_implicit_dynamic.rs`, `phase_field_coupling.rs`; and the
compiled examples `contact_explicit_static`, `contact_explicit_dynamic`,
`cohesive_contact_explicit_dynamic` (`crates/akantu/tests/examples_*.rs`).
