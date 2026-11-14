<!-- availability: available akantu::model -->
# Solid mechanics model

This chapter corresponds to the *Solid Mechanics Model* chapter of the
upstream manual (`solidmechanicsmodel.rst`) without its cohesive part
(the cohesive model is described in [Cohesive elements](models/cohesive.md)). `SolidMechanicsModel` solves
the equations of motion `M ü + f_int(u) = f_ext` (dynamics) or of
equilibrium `f_int(u) = f_ext` (statics) on a mesh, with the materials of
the [Materials](materials.md) chapter.

All examples of this chapter use the mesh `square.msh` (unit square, groups
`left`, `right`, `bottom`, `top`, `body`) and a steel-like material; the
helper hidden in each example reads them:

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;

fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
    let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/manual/src/meshes/square.msh");
    let mut mesh = Mesh::new(2, "mesh")?;
    akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
    let parser = Parser::parse_str(
        "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
        "material.dat",
    )?;
    // upstream: SolidMechanicsModel model(mesh);
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
    // upstream: model.initFull(_analysis_method = method);
    model.init_full(&ModelOptions::new(method))?;
    Ok(model)
}

let model = square_model(AnalysisMethod::Static)?;
assert_eq!(model.spatial_dimension(), 2);
assert_eq!(model.nb_materials(), 1);
# Ok::<(), akantu::core::Error>(())
```

`SolidMechanicsModel::new(mesh, dim, id)` creates a model without an input
file (materials are then registered from code); `with_parser` passes the
parsed input file, as upstream's static parser. The second argument is the
spatial dimension of the problem (`None`: the mesh's).

## Nodal arrays and initial conditions

After `init_full` the model holds, per node and component:

| Upstream | Rust | Meaning |
|---|---|---|
| `getDisplacement()` | `displacement()`, `displacement_mut()` | `u` (imposed on blocked DOFs) |
| `getVelocity()` | `velocity()`, `velocity_mut()` | `u̇` |
| `getAcceleration()` | `acceleration()`, `acceleration_mut()` | `ü` |
| `getBlockedDOFs()` | `blocked_dofs()`, `blocked_dofs_mut()` | `true`: Dirichlet DOF |
| `getExternalForce()` | `external_force()`, `external_force_mut()` | `f_ext` |
| `getInternalForce()` | `internal_force()`, `internal_force_mut()` | `f_int` |
| `getMass()` | `mass()` | lumped mass (after its assembly) |
| `getIncrement()` | `increment()` | last increment of `u` |

All are zero after initialisation. Initial conditions are set by writing
into the arrays (upstream's loop over the nodes):

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
let mut model = square_model(AnalysisMethod::ExplicitLumpedMass)?;

// upstream: disp(node, 0) = 0.1; velo(node, 1) = 1.;
for x in model.displacement_mut()?.tuples_mut() {
    x[0] = 0.1;
}
for v in model.velocity_mut()?.tuples_mut() {
    v[1] = 1.0;
}
assert_eq!(model.velocity()?.tuple(4), &[0.0, 1.0]);
# Ok::<(), akantu::core::Error>(())
```

## Reading and writing model arrays

The nodal arrays above, the mesh's `nodes()` and `connectivity(…)` and the
internal fields of the constitutive laws are storage that the Python
interface can share with NumPy, so their accessors return guards
(`akantu::core::array_shared`) instead of references:

| Guard | Returned by | Gives |
|---|---|---|
| `ArrayRef<'_, T>` | `displacement()`, `velocity()`, …, `mesh.nodes()`, `law.base().stress.get(…)` | read access: dereferences to `Array<T>` (`tuple`, `tuples`, `as_slice`, `len`, …) |
| `ArrayMut<'_, T>` | `displacement_mut()`, `velocity_mut()`, …, `get_mut(…)` | write access: `as_mut_slice`, `tuple_mut`, `tuples_mut`, `fill`, `copy_from`, … |
| `SliceRef<'_, T>` | per-element accessors (`element_connectivity`, `elements_of`) | a read-only slice, without copy |

A guard borrows the model (or the mesh) it comes from until it is
dropped, so write every access with an explicit scope: bind the guard in
an inner block or `drop` it, and never keep one across `solve_step` or
any other call that changes the model. Values needed later are copied out
(`to_vec()`). A use within one statement needs no scope; the temporary
guard is dropped at the end of the statement (`model.displacement()?.tuple(8)[0]`,
or a `for` loop over `model.velocity_mut()?.tuples_mut()` as above):

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
use akantu::core::Real;
use akantu::core::array_shared::{ArrayMut, ArrayRef};

let mut model = square_model(AnalysisMethod::ExplicitLumpedMass)?;
{
    // write guard: released at the end of the block
    let mut v: ArrayMut<'_, Real> = model.velocity_mut()?;
    v.as_mut_slice().fill(1.0);
}
let dt = model.stable_time_step()?;
model.set_time_step(0.8 * dt, None)?;
model.solve_step(None)?; // no guard is alive here
let u_before: Vec<Real> = {
    // read guard: released at the end of the block, the values copied out
    let u: ArrayRef<'_, Real> = model.displacement()?;
    u.as_slice().to_vec()
};
model.solve_step(None)?;
let moved = model.displacement()?.as_slice()[0] - u_before[0]; // one statement
assert!(moved > 0.0); // the rigid motion at 1 m/s continued
# Ok::<(), akantu::core::Error>(())
```

The compiler refuses a guard that is still used after a call that
changes the model:

```rust,compile_fail
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
let mut model = square_model(AnalysisMethod::Static)?;
let u = model.displacement()?; // read guard borrows `model`
model.solve_step(None)?;       // error: `model` is borrowed by `u`
println!("{}", u.len());
# Ok::<(), akantu::core::Error>(())
```

and a slice taken from a temporary guard (bind the guard first):

```rust,compile_fail
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
let mut model = square_model(AnalysisMethod::Static)?;
let v = model.velocity_mut()?.as_mut_slice(); // error: temporary guard dropped
v[0] = 1.0;
# Ok::<(), akantu::core::Error>(())
```

With the Python interface. `model.getDisplacement()` and the other
getters of the [Python interface](models/python.md) return NumPy arrays
that view the same memory: writes from Python reach the model and a view
held by Python sees the values of later steps (Python tests
`test_mesh_model_views.py::test_model_views_are_writable_and_reach_the_model`
and `::test_a_held_view_sees_the_values_after_each_call`). An array that
has been handed to Python is checked at run time: native code that calls
back into Python (a boundary-condition functor written in Python, for
example) while it still holds a guard of such an array gets an error
instead of a NumPy view that could change under the guard
(`test_views_runtime.py::test_callback_refused_while_a_native_guard_is_alive`).
The explicit scopes above avoid both problems.

## Boundary conditions

Dirichlet and Neumann conditions are applied on element groups (for
example those created from Gmsh physical names, see [Meshes](meshes.md))
with functors, as upstream's `model.applyBC(functor, "group")`:

| Upstream | Rust (`akantu::model::boundary_condition`) | Effect |
|---|---|---|
| `BC::Dirichlet::FixedValue(v, _x)` | `FixedValue { value: v, axis: SpatialDirection::X }` | block the component, set it to `v` |
| `BC::Dirichlet::FlagOnly(_x)` | `FlagOnly { axis: SpatialDirection::X }` | block the component, keep its value |
| `BC::Dirichlet::IncrementValue(v, _x)` | `IncrementValue { value: v, axis: … }` | block the component, add `v` |
| `BC::Dirichlet::Increment(vec)` | `Increment { value: vec }` | block every component, add `vec` |
| `BC::Neumann::FromTraction(t)` | `FromSameDim::new(t)` | add the traction vector `t` integrated over the group to `f_ext` |
| `BC::Neumann::FromStress(σ)` | `FromHigherDim::new(σ)` | add the traction `σ n` integrated over the group to `f_ext` |
| `BC::Neumann::FreeBoundary()` | `FreeBoundary` | zero traction |

The Dirichlet functors are applied to the nodes of the group, the Neumann
functors integrated on its elements (the group's elements must be of
dimension `dim − 1`). The model methods are `apply_dirichlet_bc(&functor,
group)` and `apply_neumann_bc(&functor, group)`; `…_all` apply a functor
to every element group. As upstream, a Neumann condition is added to
the external force.

### Static analysis

A uniaxial traction `t` on the right side of the square, with the left
side blocked in `x` and the bottom in `y`, gives `σ_xx = t` and, in plane
strain, `u_x(x = 1) = (1 − ν²) t / E`, reproduced exactly by bilinear
quadrangles:

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
use akantu::core::enums::SpatialDirection::{X, Y};
use akantu::core::{ElementType, GhostType};
use akantu::model::boundary_condition::{FixedValue, FlagOnly, FromSameDim};
use akantu::parser::ParamValue;

let mut model = square_model(AnalysisMethod::Static)?;
model.apply_dirichlet_bc(&FlagOnly { axis: X }, "left")?;
model.apply_dirichlet_bc(&FixedValue { value: 0.0, axis: Y }, "bottom")?;
let t = 1e6;
model.apply_neumann_bc(&FromSameDim::new(vec![t, 0.0]), "right")?;

// upstream: solver.set("max_iterations", 2); solver.set("convergence_type", _residual);
let newton = model.solver.non_linear_solver_mut(None)?;
newton.set_param("max_iterations", ParamValue::Int(2))?;
newton.set_param("convergence_type", ParamValue::Enum("residual".into()))?;
model.solve_step(None)?;

let (e, nu) = (210e9, 0.3);
let ux = model.displacement()?.tuple(8)[0]; // node at (1, 1)
let exact = (1.0 - nu * nu) * t / e;
assert!((ux - exact).abs() <= 1e-12 * exact);

// the Cauchy stress of the material at every quadrature point: [σxx, σyx, σxy, σyy]
let stress = model.material(0)?.base().stress.get(ElementType::Quadrangle4, GhostType::NotGhost)?;
for s in stress.tuples() {
    assert!((s[0] - t).abs() <= 1e-12 * t && s[3].abs() <= 1e-12 * t);
}
# Ok::<(), akantu::core::Error>(())
```

For a linear problem one Newton iteration solves the system; the default
non-linear solver of a static analysis is `newton_raphson` with the
`solution` criterion, threshold `1e-10` and at most 10 iterations
(upstream's `registerParam` defaults; the upstream manual says 100). The
1e-12 relative bound above is three orders of magnitude above the
round-off of an 18-unknown solve.

### Custom functors

A functor is any type implementing `DirichletFunctor` or `NeumannFunctor`
(upstream: a class deriving from `BC::Dirichlet::DirichletFunctor` with an
`operator()`):

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
use akantu::core::{Idx, Real, Result};
use akantu::model::boundary_condition::DirichletFunctor;

/// Shear: u_x = γ · y on the group's nodes.
struct Shear {
    gamma: Real,
}

impl DirichletFunctor for Shear {
    fn apply(&self, _node: Idx, flags: &mut [bool], primal: &mut [Real], coord: &[Real]) -> Result<()> {
        flags[0] = true;
        primal[0] = self.gamma * coord[1];
        Ok(())
    }
}

let mut model = square_model(AnalysisMethod::Static)?;
model.apply_dirichlet_bc(&Shear { gamma: 1e-3 }, "right")?;
let blocked = model.blocked_dofs()?;
assert!(blocked.tuple(8)[0] && !blocked.tuple(8)[1]); // node (1, 1)
assert_eq!(model.displacement()?.tuple(8)[0], 1e-3);
# Ok::<(), akantu::core::Error>(())
```

## Implicit dynamics

`AnalysisMethod::ImplicitDynamic` creates a `dynamic` solver with the
trapezoidal rule (`trapezoidal_rule_2`, i.e. Newmark with Akantu's
`alpha = beta = 1/2`, textbook β = ¼, γ = ½: the average-acceleration
scheme, [Solver configuration](solver-configuration.md#integration_scheme-type---))
and Newton–Raphson. The time step is set with
`set_time_step` and every `solve_step` advances one step. For a linear
problem the trapezoidal rule conserves the total energy exactly:

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
use akantu::core::enums::SpatialDirection::{X, Y};
use akantu::model::boundary_condition::FixedValue;

let mut model = square_model(AnalysisMethod::ImplicitDynamic)?;
model.apply_dirichlet_bc(&FixedValue { value: 0.0, axis: X }, "left")?;
model.apply_dirichlet_bc(&FixedValue { value: 0.0, axis: Y }, "left")?;
// initial state: a uniform stretch released at t = 0
let x: Vec<f64> = model.physics.base.mesh.read()?.nodes().as_slice().to_vec();
for (u, x) in model.displacement_mut()?.tuples_mut().zip(x.chunks(2)) {
    u[0] = 1e-4 * x[0];
}

let dt = model.stable_time_step()?; // the implicit scheme is stable for any Δt
model.set_time_step(0.8 * dt, None)?;
let mut energies = Vec::new();
for _ in 0..50 {
    model.solve_step(None)?;
    energies.push(model.energy("kinetic")? + model.energy("potential")?);
}
let e0 = energies[0];
assert!(energies.iter().all(|e| (e - e0).abs() <= 1e-12 * e0));
# Ok::<(), akantu::core::Error>(())
```

(In exact arithmetic the average-acceleration scheme conserves this
energy exactly; what remains is the round-off of the linear solves,
which the bound `1e-12` covers with a wide margin.)

## Explicit dynamics

`AnalysisMethod::ExplicitLumpedMass` (the default of `ModelOptions`, as
upstream's `initFull()`) uses the central-difference scheme with a lumped
mass matrix: every step is explicit and the time step must stay below the
stable time step `stable_time_step()` (upstream `getStableTimeStep()`),
the minimum over the elements of the element size divided by the wave
speed. `AnalysisMethod::ExplicitConsistentMass` uses the consistent mass
matrix.

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn square_model(method: AnalysisMethod) -> akantu::core::Result<SolidMechanicsModel> {
#     let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes/square.msh");
#     let mut mesh = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(
#         "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#         "material.dat",
#     )?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(method))?;
#     Ok(model)
# }
let mut model = square_model(AnalysisMethod::ExplicitLumpedMass)?;

// a rigid translation at 1 m/s: kinetic energy ½ ρ V v², no deformation
for v in model.velocity_mut()?.tuples_mut() {
    v[0] = 1.0;
}
let critical = model.stable_time_step()?;
model.set_time_step(0.8 * critical, None)?; // safety factor, as upstream examples
for _ in 0..10 {
    model.solve_step(None)?;
}
let ek = model.energy("kinetic")?;
assert!((ek - 0.5 * 7800.0).abs() <= 1e-12 * 7800.0);
assert!(model.energy("potential")?.abs() <= 1e-12);

// the lumped mass of the x components sums to ρ V
let mass: f64 = model.mass()?.tuples().map(|m| m[0]).sum();
assert!((mass - 7800.0).abs() <= 1e-12 * 7800.0);
# Ok::<(), akantu::core::Error>(())
```

Upstream v3's `explicitPred`, `updateResidual`, `updateAcceleration` and
`explicitCorr`, still mentioned by the upstream manual, do not exist in
upstream v5 nor in the port: `solve_step` performs the predictor, the
residual, the solve and the corrector.

## Energies

| Upstream `getEnergy(id)` | Rust | Meaning |
|---|---|---|
| `"kinetic"` | `energy("kinetic")`, `kinetic_energy()` | `½ vᵀ M v` (lumped or consistent mass) |
| `"potential"` | `energy("potential")` | elastic energy summed over the materials |
| `"external work"` | `energy("external work")`, `external_work()` | power of `f_ext` on free DOFs and of the reactions on blocked DOFs, times `Δt` (the increment in statics) |
| any other id | `energy(id)` | summed over the materials (e.g. `dissipated` for laws that define it; for cohesive laws upstream's trapezoidal estimate, not a physical dissipation when the time step does not resolve the softening: UD-803) |
| `getEnergy(id, element)` | `energy_of_element(id, element)` | one element |
| `getEnergy(id, group)` | `energy_of_group(id, group)` | one element group |

`energy(id)` (including `kinetic_energy()` and `external_work()`) and
`energy_of_group(id, group)` are summed over all MPI ranks (in rank order,
D-039), so every rank gets the same value; `energy_of_element(id,
element)` is the value of one local element, without reduction, as
upstream (`solid_mechanics_model.cc:642-699`).

## Finite deformation

A material with `finite_deformation = true` makes the model assemble the
total-Lagrangian internal forces and tangent (geometric stiffness
included); the law computes the second Piola–Kirchhoff stress from the
Green–Lagrange strain. The problem is non-linear, so the static solve
takes several Newton iterations:

```rust
use akantu::core::enums::SpatialDirection::{X, Y};
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::boundary_condition::FixedValue;
use akantu::model::ModelOptions;
use akantu::parser::{ParamValue, Parser};
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

let input = "material elastic [\n name = rubber\n rho = 1\n E = 1\n nu = 0.3\n finite_deformation = true\n]\n";
let parser = Parser::parse_str(input, "material.dat")?;
let mut model =
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
model.apply_dirichlet_bc(&FixedValue { value: 0.0, axis: X }, "left")?;
model.apply_dirichlet_bc(&FixedValue { value: 0.0, axis: Y }, "bottom")?;
model.apply_dirichlet_bc(&FixedValue { value: 0.5, axis: X }, "right")?; // 50 % stretch
model.solve_step(None)?;

let newton = model.solver.non_linear_solver(None)?;
let ParamValue::Int(iterations) = newton.get_param("nb_iterations")? else { unreachable!() };
assert!(iterations > 1);
assert_eq!(newton.get_param("converged")?, ParamValue::Bool(true));
// the PK2 stress is the law's stress; the Cauchy `stress` internal is
// computed when the model is dumped (upstream onDump)
assert!(model.material(0)?.base().piola_kirchhoff_2.is_some());
# Ok::<(), akantu::core::Error>(())
```

## Upstream compatibility switch

`model.physics.upstream_compat = SmmUpstreamCompat {
stiffness_grad_u_ignores_eigen_grad_u: true }` reproduces upstream's
stiffness assembly, which stores `∇u` instead of `∇u − ε⁰` in the
materials' `grad_u` when an eigenstrain is present (UD-208). The default
is the corrected behaviour; it only matters for laws whose tangent depends
on the strain and for reads of `grad_u` between a stiffness assembly and
the next residual.
