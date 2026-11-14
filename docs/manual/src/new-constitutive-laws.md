# Writing new constitutive laws

This chapter corresponds to *Adding a New Constitutive Law*
(`new-constitutive-laws.rst`) of the upstream manual. Upstream adds a law
by deriving a C++ class from `Material` (or another material), registering
its parameters in the constructor and overriding `initMaterial`,
`computeStress`, `computeTangentModuli` and the wave speeds. The port has
no class inheritance: a law is a Rust value behind the trait
`akantu::solid::Material`, created by a factory under its name, and
the input file selects it by that name like a built-in law.

There are two ways to write one:

* runtime registration (`akantu::solid::ForeignMaterial`): the law's
  behaviour is a type implementing `ForeignMaterialMethods`, and its
  parameters and internal fields are registered by name when the law is
  created. This is the path for laws written outside the library (the
  Python subclasses of materials of the [Python interface](models/python.md)
  use it); the worked example below uses it;
* built-in laws implement `Material` (and the traits it requires:
  `ConstitutiveLaw`, `ParameterRegistry`, `Parsable`) on a struct with
  static parameter tables, e.g. `crates/akantu-solid/src/materials/elastic.rs`.
  The helper macros they use are internal to `akantu-solid` (hidden from
  the API documentation), so this path is for laws added to the library.

## Worked example: a runtime-registered law

The law `scaled_elastic` is linear elastic (plane strain) with its
stiffness multiplied by a parameter `factor`. It registers three
parameters, computes the stress and the tangent, and gives the P-wave
speed used by the stable time step. Registered on a model before
`init_full`, it is used from the input file and solved statically; with
`factor = f` it must behave as the built-in `elastic` law with Young's
modulus `f·E`, which the example checks against the closed form
`u_x = (1 − ν²) t / (f E)` of the uniaxial traction test of
[Solid mechanics model](solid-mechanics.md#static-analysis):

```rust
use std::sync::Arc;

use akantu::core::enums::SpatialDirection::{X, Y};
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::core::{Array, Element, ElementType, GhostType, Real, Result};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::model::boundary_condition::{FixedValue, FlagOnly, FromSameDim};
use akantu::model::constitutive_law::{LawSpec, ModelServices};
use akantu::parser::Parser;
use akantu::parser::param::ParameterAccess;
use akantu::solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SolidMechanicsModel,
};

/// σ = f (λ tr ε I + 2μ ε) in plane strain.
#[derive(Debug, Default)]
struct ScaledElastic {
    lambda: Real,
    mu: Real,
    factor: Real,
}

impl ForeignMaterialMethods for ScaledElastic {
    // upstream: initMaterial(), called once the parameters are parsed and
    // the elements assigned
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        let e = law.state.params.get_real("E")?;
        let nu = law.state.params.get_real("nu")?;
        self.factor = law.state.params.get_real("factor")?;
        self.lambda = nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu));
        self.mu = e / (2.0 * (1.0 + nu));
        law.base_init_material() // upstream: Material::initMaterial()
    }

    // upstream: computeStress(type, ghost) with the
    // MATERIAL_STRESS_QUADRATURE_POINT_LOOP_BEGIN/END macros
    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let base = &mut law.state.base;
        // a read guard on the displacement gradient and a write guard on
        // the stress: two different internal fields, so both guards can be
        // held at once; they are released when this function returns
        let grad_u = base.grad_u.get(ty, ghost)?;
        let mut sigma = base.stress.get_mut(ty, ghost)?;
        // one 2 × 2 column-major tuple per quadrature point
        for (s, gu) in sigma.tuples_mut().zip(grad_u.tuples()) {
            let (exx, eyy, exy) = (gu[0], gu[3], 0.5 * (gu[1] + gu[2]));
            let tr = exx + eyy;
            s[0] = self.factor * (self.lambda * tr + 2.0 * self.mu * exx);
            s[3] = self.factor * (self.lambda * tr + 2.0 * self.mu * eyy);
            s[1] = self.factor * 2.0 * self.mu * exy;
            s[2] = s[1];
        }
        Ok(())
    }

    // upstream: computeTangentModuli(type, tangent, ghost); Voigt notation
    // (xx, yy, xy), one 3 × 3 column-major matrix per quadrature point
    fn compute_tangent_moduli(
        &mut self,
        _law: &mut ForeignLaw<'_, '_>,
        _ty: ElementType,
        _ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let (l, m, f) = (self.lambda, self.mu, self.factor);
        let c = [f * (l + 2.0 * m), f * l, 0.0, f * l, f * (l + 2.0 * m), 0.0, 0.0, 0.0, f * m];
        for t in tangent.tuples_mut() {
            t.copy_from_slice(&c);
        }
        Ok(())
    }

    // upstream: getPushWaveSpeed (the default celerity)
    fn push_wave_speed(&self, state: &ForeignLawState, _element: Element) -> Result<Real> {
        Ok((self.factor * (self.lambda + 2.0 * self.mu) / state.base.rho).sqrt())
    }
}

/// Allocator registered under the law's name (upstream:
/// MaterialFactory::getInstance().registerAllocator("scaled_elastic", …)).
fn register(model: &mut SolidMechanicsModel) -> Result<()> {
    model.physics.factory.register(
        "scaled_elastic",
        Arc::new(|spec: &LawSpec, _model: &dyn ModelServices| -> Result<Box<dyn Material>> {
            let mut law =
                ForeignMaterial::new(spec, "scaled_elastic", Box::new(ScaledElastic::default()))?;
            // upstream: registerParam("E", E, _pat_parsmod, "…")
            let access = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
            law.register_param_real("E", access, "Young's modulus")?;
            law.register_param_real("nu", access, "Poisson's ratio")?;
            law.register_param_real("factor", access, "stiffness factor")?;
            Ok(Box::new(law))
        }),
    )
}

let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("../../docs/manual/src/meshes/square.msh");
let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
let input = "
material scaled_elastic [
    name   = soft
    rho    = 1
    E      = 1
    nu     = 0.3
    factor = 0.5
]";
let parser = Parser::parse_str(input, "material.dat")?;
let mut model =
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
register(&mut model)?; // before init_full, which creates the materials
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
assert_eq!(model.material(0)?.parameter_names(), ["name", "rho", "E", "nu", "factor"]);

model.apply_dirichlet_bc(&FlagOnly { axis: X }, "left")?;
model.apply_dirichlet_bc(&FixedValue { value: 0.0, axis: Y }, "bottom")?;
let t = 1e-3;
model.apply_neumann_bc(&FromSameDim::new(vec![t, 0.0]), "right")?;
model.solve_step(None)?;

let (e, nu, f) = (1.0, 0.3, 0.5);
let ux = model.displacement()?.tuple(8)[0]; // node at (1, 1)
let exact = (1.0 - nu * nu) * t / (f * e);
assert!((ux - exact).abs() <= 1e-12 * exact);
# Ok::<(), akantu::core::Error>(())
```

The repository's tests of this mechanism are
`crates/akantu-solid/tests/runtime_registered_law.rs` (parameters,
runtime internal fields, history, resizing, parsing of the `material`
section: a transcription of upstream's Python example
`custom-material/bi-material.py`) and
`crates/akantu-solid/tests/review_v41_regressions.rs` (a runtime
incremental law driven by the Newton–Raphson solver).

## What a law provides

| Port (`ForeignMaterialMethods`) | Upstream (`Material`) | Required |
|---|---|---|
| `init_material` (default: `base_init_material`) | `initMaterial` | no |
| `compute_stress(law, type, ghost)` | `computeStress` | yes |
| `compute_tangent_moduli(law, type, ghost, tangent)` | `computeTangentModuli` | for implicit solves (default: `Error::NotImplemented`) |
| `compute_potential_energy(law, type)` | `computePotentialEnergy` | for `energy("potential")` (default: no-op) |
| `push_wave_speed(state, element)` | `getPushWaveSpeed` (default `getCelerity`) | yes (stable time step) |
| `shear_wave_speed(state, element)` | `getShearWaveSpeed` | no (default: `Error::NotImplemented`) |

Every method receives `ForeignLaw { state, ctx }`: `state` is the law's own
data (`state.base`: the common internals `grad_u`, `stress`,
`potential_energy`, `eigen_grad_u` with their element filter and density
`rho`; `state.params`: the runtime parameters; `state.internals`: the
runtime internal fields), `ctx` gives read access to the model (mesh,
quadrature-point coordinates, nodal fields) for the current phase.
`grad_u` holds the displacement gradient `∇u` (as upstream), not the
strain; a law that needs the strain symmetrises it itself.

Internal fields (upstream `InternalField<Real> damage("damage", *this)` +
`damage.initialize(n)`) are `ForeignMaterial::register_internal_real(name,
nb_component)`; `base_init_material` sizes them for the law's elements,
and they follow the same history (`save_previous_state`,
`restore_previous_state`) and resizing rules as the internals of built-in
laws (test
`runtime_registered_law.rs::runtime_internals_follow_the_history_and_resize_contract`).

### Internals on another FE engine or element filter

`ForeignMaterial::register_internal_real_on(name, nb_component,
fe_engine_id, element_filter)` is upstream's `registerInternal(id,
nb_component, fe_engine_id[, element_filter])` (row CL-001): the field
lives on the integration points of the model's FE engine `fe_engine_id`
(registered with the model's `fems`) and on `element_filter` when one is
given, else on the law's elements. Initialisation, the dumper,
`extrapolate_internal` and the Python `flattenInternal` /
`inflateInternal` size and read it on that support, as upstream. Two
differences from upstream:

* The field owns a copy of its filter. Upstream keeps a reference to
  the caller's array, so a later change of that array is seen by the
  field; in the port a change after registration goes through
  `InternalField::element_filter_mut`.
* Element removal and reassignment are refused before any change
  when such a field cannot follow them (UD-1083): removing an element of
  the law (its internals would be compacted: upstream does it correctly
  for a field on another engine with the law's filter, which the port
  does not implement, and misapplies the law's renumbering to a field on
  its own filter), and removing or renumbering an element of a field's own
  filter (upstream never renumbers that filter). The refusal happens in
  the mesh mutation's capability check, so the mesh and the model are
  left unchanged; a removal that only renumbers the law's elements in
  the mesh is accepted (tests `cl001_internal_on_engine.rs`,
  `cl001_own_support_model.rs`).

### Parameters

`register_param_real(name, access, description)` registers a real
parameter; `name` and `rho` are registered by `ForeignMaterial::new`.
Access flags (`akantu::parser::param::ParameterAccess`) are upstream's:

| Upstream | Port |
|---|---|
| `_pat_internal` | `INTERNAL` |
| `_pat_writable` | `WRITABLE` |
| `_pat_readable` | `READABLE` |
| `_pat_modifiable` | `MODIFIABLE` (= `READABLE \| WRITABLE`) |
| `_pat_parsable` | `PARSABLE` |
| `_pat_parsmod` | `PARSMOD` (= `MODIFIABLE \| PARSABLE`) |

Differences from upstream's `registerParam(name, member, default, access,
description)`: a runtime parameter has no default value: reading one
that the input file did not set is an error at `init_material`, not an
uninitialised member (test
`runtime_registered_law.rs::unset_runtime_parameter_is_an_error_not_garbage`);
values in the input file can be expressions of the enclosing scope, and
an unknown parameter name is an error, a warning with a permissive parser
(test `runtime_registered_law.rs::runtime_law_parses_its_material_section`).

## Writing a non-local law

This section corresponds to *Adding a New Non-Local Constitutive Law* of
the upstream chapter. Upstream derives `MaterialXXXNonLocal` from the local
law `MaterialXXX` and from `MaterialNonLocal`, adds the internal fields
for the averaged quantities, and implements four functions. The port has
no inheritance; a non-local law is a value that implements `Material` for
its local behaviour and `MaterialNonLocal`
(`akantu::solid::materials::non_local`) for the non-local part, and that
the model finds through `Material::as_non_local`:

| Upstream (`MaterialXXXNonLocal`) | Port |
|---|---|
| `initMaterial`: `MaterialNonLocal::initMaterial()` and `getNonLocalManager().registerNonLocalVariable(local, non_local, nb_component)` | `MaterialNonLocal::non_local_variables`: the list of `NonLocalVariable { local, non_local, nb_component }` that the manager averages; the model creates the non-local manager when a law returns `Some` from `Material::as_non_local` (upstream: the `dynamic_cast` to the non-local interface); the law's `is_non_local` flag (`base_mut().is_non_local = true`, set by upstream's `MaterialNonLocal` base class) only makes the local part of a built-in law skip its own damage update |
| the neighbourhood of the non-local parent (the law's name, `getNeighborhoodName()`) | `MaterialNonLocal::neighborhood_name` (the built-in laws use the law's `name`); the `non_local <name> <weight function> [ … ]` section of the input file gives the weight function and its parameters |
| `computeStress(el_type, ghost)`: compute all local quantities | `Material::compute_stress`: the law computes, and stores in its internals, the quantities that are averaged |
| `computeNonLocalStresses(ghost)` (loop over the elements) and `computeNonLocalStress(el_type, ghost)` (the law on each point) | `MaterialNonLocal::compute_non_local_stresses(ctx, ghost)`: one call per ghost type, after the averages were copied into the non-local internals of the law; the loop over the element types is the law's own |
| the internal field `InternalField<Real> grad_u_nl` | any `InternalField<Real>` of the law, listed in `for_each_internal` and `for_each_internal_mut` like the internals of a local law |

During `assemble_internal_forces` the model computes the local stresses of
every law, averages the registered variables of each neighbourhood (the
weights come from the weight function of the `non_local` section) and then
calls `compute_non_local_stresses`. The law below is the port of upstream's
`CustomNonLocalTestMaterial` (test `test_material_non_local`, ported in
`crates/akantu-solid/tests/upstream_material_non_local.rs`): it extends
`elastic` by the local field `local_damage`, averages it into `damage` and
completes the stress as `σ ← σ (1 − damage)`. The lines that only forward
the local behaviour to the parent law are hidden in the rendered page (they
are compiled and run as part of this page). The example checks that the
model creates the non-local manager and that a uniform local quantity is
reproduced by the averaging, so that `σ = 0.75 σ_el` for
`local_damage = 0.25`:

```rust
use std::sync::Arc;

use akantu::core::enums::{AnalysisMethod, MatrixType, MeshIoType};
use akantu::core::tensor::{Matrix, MatrixExt, as_matrix_mut};
use akantu::core::{Array, Element, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::model::InternalField;
use akantu::model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu::model::non_local::NonLocalVariable;
use akantu::parser::param::{ParamInfo, ParamValue};
use akantu::parser::{Parsable, ParameterRegistry, Parser, ParserType};
use akantu::solid::material::{LawOptions, MaterialBase};
use akantu::solid::materials::elastic::MaterialElastic;
use akantu::solid::materials::non_local::MaterialNonLocal;
use akantu::solid::{Material, SolidMechanicsModel};

/// The local law (here `elastic`) extended by two internal fields: the
/// local quantity `local_damage` that is averaged, and `damage`, its
/// non-local counterpart. Upstream: the members of
/// `CustomNonLocalTestMaterial` (`test_material_non_local`).
#[derive(Clone, Debug)]
struct CustomNonLocal {
    elastic: MaterialElastic<2>,
    local_damage: InternalField<Real>,
    damage: InternalField<Real>,
}

impl CustomNonLocal {
    fn new(spec: &LawSpec, mesh: Option<&Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<2>::new(spec, mesh)?;
        elastic.base_mut().law_type = "custom_non_local".into();
        // upstream: the `MaterialNonLocal` base class sets `is_non_local`
        // (the local part then skips its own update); the model creates its
        // non-local manager because `as_non_local` returns `Some` below
        elastic.base_mut().is_non_local = true;
        Ok(Self {
            elastic,
            local_damage: InternalField::new("local_damage", 1, 0.0)?,
            damage: InternalField::new("damage", 1, 0.0)?,
        })
    }
}

// upstream: `registerNonLocalVariable(local, non_local, nb_component)` in
// `initMaterial`, and `computeNonLocalStresses`
impl MaterialNonLocal for CustomNonLocal {
    // the neighbourhood (and the `non_local <id> <weight function> [ … ]`
    // section of the input file) is named after the law
    fn neighborhood_name(&self) -> String {
        self.elastic.name().to_owned()
    }

    // the variables the non-local manager averages before the stress is
    // completed: (local field, non-local field, number of components)
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        vec![NonLocalVariable {
            local: "local_damage".into(),
            non_local: "damage".into(),
            nb_component: 1,
        }]
    }

    // called after the local stress pass and the averaging, with the
    // non-local `damage` filled in: here σ ← σ (1 − damage)
    fn compute_non_local_stresses(&mut self, _ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        for ty in self.elastic.base().filter_types(ghost) {
            let damage = self.damage.get(ty, ghost)?.clone();
            let mut stress = self.elastic.base_mut().sigma_field_mut()?.get_mut(ty, ghost)?;
            for (s, d) in stress.as_mut_slice().chunks_exact_mut(4).zip(damage.as_slice()) {
                let s: &mut Matrix<2, 2> = as_matrix_mut(s)?;
                *s = s.scale(1.0 - d);
            }
        }
        Ok(())
    }
}

// `Material`: everything local is delegated to the parent law; the
// non-local part is the pair `as_non_local` / `as_non_local_mut`
impl Material for CustomNonLocal {
    fn as_non_local(&self) -> Option<&dyn MaterialNonLocal> {
        Some(self)
    }
    fn as_non_local_mut(&mut self) -> Option<&mut dyn MaterialNonLocal> {
        Some(self)
    }
#     fn base(&self) -> &MaterialBase {
#         self.elastic.base()
#     }
#     fn base_mut(&mut self) -> &mut MaterialBase {
#         self.elastic.base_mut()
#     }
#     fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
#         self.elastic.init_material(ctx)?;
#         let blocks = self.elastic.base().blocks(ctx);
#         self.local_damage.resize_blocks(&blocks)?;
#         self.damage.resize_blocks(&blocks)
#     }
#     fn compute_stress(&mut self, ctx: &LawContext<'_>, ty: ElementType, ghost: GhostType) -> Result<()> {
#         self.elastic.compute_stress(ctx, ty, ghost)
#     }
#     fn compute_tangent_moduli(
#         &mut self,
#         ctx: &LawContext<'_>,
#         ty: ElementType,
#         ghost: GhostType,
#         tangent: &mut Array<Real>,
#     ) -> Result<()> {
#         self.elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)
#     }
#     fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
#         self.elastic.compute_potential_energy(ctx, ty)
#     }
#     fn push_wave_speed(&self, element: Element) -> Result<Real> {
#         self.elastic.push_wave_speed(element)
#     }
#     fn shear_wave_speed(&self, element: Element) -> Result<Real> {
#         self.elastic.shear_wave_speed(element)
#     }
#     fn tangent_type(&self) -> MatrixType {
#         self.elastic.tangent_type()
#     }
#     fn options(&self) -> Option<&LawOptions> {
#         self.elastic.options()
#     }
#     fn options_mut(&mut self) -> Option<&mut LawOptions> {
#         self.elastic.options_mut()
#     }
#     fn as_any(&self) -> Option<&dyn std::any::Any> {
#         Some(self)
#     }
#     fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
#         Some(self)
#     }
}

# impl ConstitutiveLaw for CustomNonLocal {
#     fn name(&self) -> &str {
#         self.elastic.name()
#     }
#     fn law_id(&self) -> &str {
#         self.elastic.law_id()
#     }
#     fn law_type(&self) -> &str {
#         self.elastic.law_type()
#     }
#     fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
#         self.elastic.element_filter()
#     }
#     fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
#         self.elastic.element_filter_mut()
#     }
#     // the internal fields of the law, in registration order
#     fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
#         self.elastic.for_each_internal(f);
#         f("local_damage", InternalFieldRef::Real(&self.local_damage));
#         f("damage", InternalFieldRef::Real(&self.damage));
#     }
#     fn for_each_internal_mut(
#         &mut self,
#         f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
#     ) -> Result<()> {
#         self.elastic.for_each_internal_mut(f)?;
#         f("local_damage", InternalFieldMut::Real(&mut self.local_damage))?;
#         f("damage", InternalFieldMut::Real(&mut self.damage))
#     }
# }
# impl ParameterRegistry for CustomNonLocal {
#     fn get_param(&self, name: &str) -> Result<ParamValue> {
#         self.elastic.get_param(name)
#     }
#     fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
#         self.elastic.set_param(name, value)
#     }
#     fn param_names(&self) -> Vec<&'static str> {
#         self.elastic.param_names()
#     }
#     fn update_internal_parameters(&mut self) -> Result<()> {
#         self.elastic.update_internal_parameters()
#     }
#     fn param_info(&self, name: &str) -> Option<ParamInfo> {
#         self.elastic.param_info(name)
#     }
#     fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
#         self.elastic.set_param_from_input(name, value)
#     }
# }
# impl Parsable for CustomNonLocal {
#     fn section_type(&self) -> ParserType {
#         ParserType::Material
#     }
#     fn parsable_id(&self) -> &str {
#         &self.elastic.base().id
#     }
# }

let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("../../docs/manual/src/meshes/square.msh");
let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
// the `non_local` section names the neighbourhood (= the law) and gives
// its weight function (`base_wf`) and radius
let input = "
material custom_non_local [
    name = soft
    rho  = 1
    E    = 1
    nu   = 0
]

non_local soft base_wf [
    radius = 0.4
]";
let parser = Parser::parse_str(input, "material.dat")?;
let mut model =
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
// registered before init_full, like any user law
model.physics.factory.register(
    "custom_non_local",
    Arc::new(|spec: &LawSpec, services: &dyn ModelServices| -> Result<Box<dyn Material>> {
        Ok(Box::new(CustomNonLocal::new(spec, Some(services.mesh()))?))
    }),
)?;
model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
// a law that is non-local makes the model create the non-local manager
assert!(model.is_non_local());
assert!(model.non_local_manager().is_some());

// the manager follows `as_non_local`, not the `is_non_local` flag: the
// same law with the flag cleared still gets one
let mut square = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut square, &mesh_file, MeshIoType::Auto)?;
let mut unflagged = SolidMechanicsModel::with_parser(
    SharedMesh::new(square),
    None,
    "solid_mechanics_model",
    Parser::parse_str(input, "material.dat")?,
)?;
unflagged.physics.factory.register(
    "custom_non_local",
    Arc::new(|spec: &LawSpec, services: &dyn ModelServices| -> Result<Box<dyn Material>> {
        let mut law = CustomNonLocal::new(spec, Some(services.mesh()))?;
        law.elastic.base_mut().is_non_local = false;
        Ok(Box::new(law))
    }),
)?;
unflagged.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
assert!(!unflagged.material(0)?.base().is_non_local);
assert!(unflagged.non_local_manager().is_some());

// a uniform eigenstrain gives a non-zero elastic stress; a uniform local
// quantity is reproduced by the averaging (the weights are normalised),
// so `damage` = 0.25 everywhere and σ = 0.75 σ_el
let (ty, ng) = (ElementType::Quadrangle4, GhostType::NotGhost);
for g in model.material_mut(0)?.base_mut().eigen_grad_u.get_mut(ty, ng)?.as_mut_slice().chunks_exact_mut(4) {
    g.copy_from_slice(&[1e-3, 0.0, 0.0, 2e-3]);
}
model.assemble_internal_forces()?;
let elastic_stress = model.material(0)?.base().stress.get(ty, ng)?.clone();
assert!(elastic_stress.as_slice().iter().any(|s| *s != 0.0));
for d in model.material_mut(0)?.as_any_mut().unwrap().downcast_mut::<CustomNonLocal>().unwrap()
    .local_damage.get_mut(ty, ng)?.as_mut_slice()
{
    *d = 0.25;
}
model.assemble_internal_forces()?;
let law = model.material(0)?.as_any().unwrap().downcast_ref::<CustomNonLocal>().unwrap();
let damage = law.damage.get(ty, ng)?;
assert!(damage.as_slice().iter().all(|d| (d - 0.25).abs() <= 1e-14));
let stress = law.base().stress.get(ty, ng)?;
for (s, e) in stress.as_slice().iter().zip(elastic_stress.as_slice()) {
    assert!((s - 0.75 * e).abs() <= 1e-14 * e.abs().max(1e-3));
}
# Ok::<(), akantu::core::Error>(())
```

The non-local laws of the library (`mazars_non_local`,
`plastic_mazars_non_local`, `marigo_non_local`) are built the same way (see
`crates/akantu-solid/src/materials/non_local/`).

## Differences from upstream's procedure

* Registration is per model and before `init_full`. Upstream's
  `MaterialFactory` is a process-wide singleton; the port's factory is the
  model's (`model.physics.factory`, built-in laws already registered), so a
  law registered on one model is unknown to another. Registering a name
  that exists is an error (upstream `registerAllocator`), so a user law
  cannot replace a built-in one. The law is registered before `init_full`,
  as upstream's code registers an allocator with
  `MaterialFactory::registerAllocator` before `initFull`; the sequence
  `initFull` → `registerNewCustomMaterials` → `initMaterials` of the
  upstream manual does not exist in upstream's code ([upstream manual
  errata](differences.md#upstream-manual-errata)).
* No printing operator. Upstream asks for an `operator<<` calling
  `printself`; Rust laws print with `Debug`.
* No quadrature-point loop macros: the law iterates the tuples of its
  arrays (`tuples()`, `tuples_mut()`), one per quadrature point of the
  element type and ghost type asked for.
* Non-local laws (upstream's `MaterialNonLocal` mix-in, second half of
  the upstream chapter, see [above](#writing-a-non-local-law)) are
  available in the default build: the non-local
  manager is in `akantu::model::non_local` and the laws
  `mazars_non_local`, `plastic_mazars_non_local` and `marigo_non_local`
  (upstream package `damage_non_local`) are registered on the solid
  mechanics model (test
  `manual_claims_2.rs::doc_non_local_laws_are_available`).
