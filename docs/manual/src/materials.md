<!-- availability: available akantu::solid -->
# Materials

This chapter corresponds to *Material section* (`io.rst`) and to the
elastic part of *Constitutive laws* (`constitutive-laws.rst`) of the
upstream manual: the material sections, the assignment of materials to
elements and the elastic family (`elastic` with its thermal part,
`elastic_orthotropic`, `elastic_anisotropic`) in detail. The other laws
are in its sub-chapter [Constitutive laws](constitutive-laws.md).

## Registered laws

The laws registered in this release, generated from the factories of
every model of the facade crate that has one (the input file selects
them by these names; `tests/manual.rs::registered_laws_cover_every_model_factory`
fails when a model with a new factory is added without being listed
here):

<!-- BEGIN GENERATED: registered-laws -->
| Model | Section | Registered laws |
|---|---|---|
| `SolidMechanicsModel` | `material` | `anisotropic_damage`, `elastic`, `elastic_anisotropic`, `elastic_orthotropic`, `marigo`, `marigo_non_local`, `mazars`, `mazars_non_local`, `neohookean`, `phasefield`, `plastic_drucker_prager`, `plastic_linear_isotropic_hardening`, `plastic_mazars`, `plastic_mazars_non_local`, `sls_deviatoric`, `viscoelastic_maxwell` |
| `SolidMechanicsModelCohesive` (in addition) | `material` | `cohesive_bilinear`, `cohesive_exponential`, `cohesive_linear`, `cohesive_linear_fatigue`, `cohesive_linear_friction`, `cohesive_linear_uncoupled` |
| `ContactMechanicsModel` | `contact_resolution` | `penalty_linear`, `penalty_quadratic` |
| `PhaseFieldModel` | `phasefield` | `linear`, `quadratic` |
| `HeatTransferModel` | `constitutive_law` | `heat_diffusion` |
<!-- END GENERATED: registered-laws -->

The laws beyond the elastic family are described law by law in
[Constitutive laws](constitutive-laws.md): equations, keywords with
their defaults and units, internal fields, energies and the differences
from upstream (plasticity, damage, the non-local laws and their
`non_local` section, viscoelasticity, `neohookean` and the `phasefield`
material). The cohesive laws belong to the [cohesive
model](models/cohesive.md#cohesive-laws), the `phasefield` laws of the
phase-field model to [Phase-field fracture](models/phase-field.md);
custom laws are described in [Writing new constitutive
laws](new-constitutive-laws.md).

Two of these laws need a choice before they compute anything (details
in [Damage](laws/damage.md#anisotropic-damage-anisotropic_damage) and
[Viscoelastic laws](laws/viscoelastic.md#standard-linear-solid-sls_deviatoric)):

- `anisotropic_damage` refuses `computeStress` by default: upstream's code
  is not a usable law (UD-031, UD-058). `LawOptions::anisotropic_damage`
  selects `Upstream` (the reference code with its undefined values
  defined, for comparisons with the oracle) or the corrected model
  `Corrected` (an opt-in until its independent review; specification
  decision D-066 in `docs/DECISIONS.md`). Limitation of the
  corrected model: once a principal damage has reached `Dc`, a load that
  opens that direction again stops the damage evolution in every
  direction for as long as it lasts (the increment is scaled to keep all
  principal damages at most `Dc`); loading orthogonal to the saturated
  direction keeps damaging. Plane stress and `finite_deformation` are
  refused by it.
- `sls_deviatoric` refuses `Ev > E` by default (its long-term modulus
  `Einf = E − Ev` would be negative; UD-762); `Ev = 0` and `Eta = 0` take
  the limits of the viscous element instead of upstream's NaN dissipated
  energy.

## Material sections

A material is a `material <law> [ … ]` section. Materials are read from the
model's `model solid_mechanics_model [ … ]` section when the file has one
(preferred), else from the top level of the file:

```dat
model solid_mechanics_model [
    material elastic [
        name = steel
        rho  = 7800
        E    = 210e9
        nu   = 0.3
    ]
]
```

```dat
# no model section: top-level materials
material elastic [
    name = steel
    rho  = 7800
    E    = 210e9
    nu   = 0.3
]
```

When the file has a model section, top-level materials are ignored (as
upstream). Every material needs a `name`; a section without one is an
error ("A constitutive_law of type 'elastic' in the input file has been
defined without a name!"). Materials are numbered in file order; the
option word of the section header (`material elastic flavor [`) is passed
to the law's factory, which the elastic laws ignore.

Parameters unknown to the law are errors, unless the file sets
`permissive_parser = true` (then warnings). Parameters that the law
registers as not parsable (the derived quantities `lambda`, `mu`, `kapa`,
…) are errors in both modes.

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::{ParamValue, Parser};
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mesh = || -> akantu::core::Result<SharedMesh> {
#     let mut m = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut m, &mesh_file, MeshIoType::Auto)?;
#     Ok(SharedMesh::new(m))
# };
let model_with = |input: &str| -> akantu::core::Result<SolidMechanicsModel> {
    let parser = Parser::parse_str(input, "material.dat")?;
    let mut model = SolidMechanicsModel::with_parser(mesh()?, None, "solid_mechanics_model", parser)?;
    model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
    Ok(model)
};

let model = model_with("material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n")?;
let steel = model.material_by_name("steel")?;
assert_eq!(steel.get_param("E")?, ParamValue::Real(210e9));
// derived parameters are readable
let mu = match steel.get_param("mu")? { ParamValue::Real(x) => x, _ => unreachable!() };
assert!((mu - 210e9 / 2.6).abs() < 1e-6 * mu);

assert!(model_with("material elastic [\n rho = 1\n E = 1\n nu = 0.3\n]\n").is_err()); // no name
assert!(model_with("material elastic [\n name = a\n E = 1\n nu = 0.3\n Young = 1\n]\n").is_err());
assert!(model_with("permissive_parser = true\nmaterial elastic [\n name = a\n E = 1\n nu = 0.3\n Young = 1\n]\n").is_ok());
assert!(model_with("permissive_parser = true\nmaterial elastic [\n name = a\n E = 1\n nu = 0.3\n mu = 1\n]\n").is_err());
# Ok::<(), akantu::core::Error>(())
```

## Assigning materials to elements

By default every element is assigned to the first material. To assign
materials by mesh data (typically the Gmsh physical names) use the
element-data selector (upstream `MeshDataMaterialSelector<std::string>`):
the material whose `name` equals the element's value is chosen.

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::core::{Element, ElementType, GhostType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::constitutive_law::ElementDataSelector;
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

// the four quadrangles of square.msh belong to the physical group "body"
let input = "\
material elastic [\n name = other\n rho = 1\n E = 1\n nu = 0.3\n]
material elastic [\n name = body\n rho = 2\n E = 2\n nu = 0.3\n]";
let parser = Parser::parse_str(input, "material.dat")?;
let mut model = SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;

let el = Element::new(ElementType::Quadrangle4, 0, GhostType::NotGhost);
assert_eq!(model.material_of(el)?.name(), "body");
assert_eq!(model.material_index("body")?, 1);
# Ok::<(), akantu::core::Error>(())
```

Materials can also be created from code (`register_new_material(name,
law, option)`) and selectors written in Rust (trait
`akantu::model::constitutive_law::ConstitutiveLawSelector`).

## Parameters of the elastic family

Generated from the laws of the port (`r` readable, `w` writable from code,
`p` parsable from an input file; upstream's access flags):

<!-- BEGIN GENERATED: elastic-parameters -->
#### `elastic`

| Parameter | Type | Access | 2-D | 3-D |
|---|---|---|---|---|
| `name` | ByteString | r-p | ✓ | ✓ |
| `rho` | Real | rwp | ✓ | ✓ |
| `finite_deformation` | Bool | r-p | ✓ | ✓ |
| `inelastic_deformation` | Bool | --- | ✓ | ✓ |
| `eigen_grad_u` | MatrixReal | --p | ✓ | ✓ |
| `Plane_Stress` | Bool | rwp | ✓ | — |
| `E` | Real | rwp | ✓ | ✓ |
| `nu` | Real | rwp | ✓ | ✓ |
| `alpha` | Real | rwp | ✓ | ✓ |
| `delta_T` | Real | rwp | ✓ | ✓ |
| `lambda` | Real | r-- | ✓ | ✓ |
| `mu` | Real | r-- | ✓ | ✓ |
| `kapa` | Real | r-- | ✓ | ✓ |

#### `elastic_orthotropic`

| Parameter | Type | Access | 2-D | 3-D |
|---|---|---|---|---|
| `name` | ByteString | r-p | ✓ | ✓ |
| `rho` | Real | rwp | ✓ | ✓ |
| `finite_deformation` | Bool | r-p | ✓ | ✓ |
| `inelastic_deformation` | Bool | --- | ✓ | ✓ |
| `eigen_grad_u` | MatrixReal | --p | ✓ | ✓ |
| `n1` | VectorReal | rwp | ✓ | ✓ |
| `n2` | VectorReal | rwp | ✓ | ✓ |
| `C11` | Real | rwp | ✓ | ✓ |
| `C12` | Real | rwp | ✓ | ✓ |
| `C13` | Real | rwp | ✓ | ✓ |
| `C22` | Real | rwp | ✓ | ✓ |
| `C23` | Real | rwp | ✓ | ✓ |
| `C33` | Real | rwp | ✓ | ✓ |
| `E1` | Real | rwp | ✓ | ✓ |
| `E2` | Real | rwp | ✓ | ✓ |
| `nu12` | Real | rwp | ✓ | ✓ |
| `G12` | Real | rwp | ✓ | ✓ |
| `n3` | VectorReal | rwp | — | ✓ |
| `C14` | Real | rwp | — | ✓ |
| `C15` | Real | rwp | — | ✓ |
| `C16` | Real | rwp | — | ✓ |
| `C24` | Real | rwp | — | ✓ |
| `C25` | Real | rwp | — | ✓ |
| `C26` | Real | rwp | — | ✓ |
| `C34` | Real | rwp | — | ✓ |
| `C35` | Real | rwp | — | ✓ |
| `C36` | Real | rwp | — | ✓ |
| `C44` | Real | rwp | — | ✓ |
| `C45` | Real | rwp | — | ✓ |
| `C46` | Real | rwp | — | ✓ |
| `C55` | Real | rwp | — | ✓ |
| `C56` | Real | rwp | — | ✓ |
| `C66` | Real | rwp | — | ✓ |
| `E3` | Real | rwp | — | ✓ |
| `nu13` | Real | rwp | — | ✓ |
| `nu23` | Real | rwp | — | ✓ |
| `G13` | Real | rwp | — | ✓ |
| `G23` | Real | rwp | — | ✓ |

#### `elastic_anisotropic`

| Parameter | Type | Access | 2-D | 3-D |
|---|---|---|---|---|
| `name` | ByteString | r-p | ✓ | ✓ |
| `rho` | Real | rwp | ✓ | ✓ |
| `finite_deformation` | Bool | r-p | ✓ | ✓ |
| `inelastic_deformation` | Bool | --- | ✓ | ✓ |
| `eigen_grad_u` | MatrixReal | --p | ✓ | ✓ |
| `n1` | VectorReal | rwp | ✓ | ✓ |
| `n2` | VectorReal | rwp | ✓ | ✓ |
| `C11` | Real | rwp | ✓ | ✓ |
| `C12` | Real | rwp | ✓ | ✓ |
| `C13` | Real | rwp | ✓ | ✓ |
| `C22` | Real | rwp | ✓ | ✓ |
| `C23` | Real | rwp | ✓ | ✓ |
| `C33` | Real | rwp | ✓ | ✓ |
| `n3` | VectorReal | rwp | — | ✓ |
| `C14` | Real | rwp | — | ✓ |
| `C15` | Real | rwp | — | ✓ |
| `C16` | Real | rwp | — | ✓ |
| `C24` | Real | rwp | — | ✓ |
| `C25` | Real | rwp | — | ✓ |
| `C26` | Real | rwp | — | ✓ |
| `C34` | Real | rwp | — | ✓ |
| `C35` | Real | rwp | — | ✓ |
| `C36` | Real | rwp | — | ✓ |
| `C44` | Real | rwp | — | ✓ |
| `C45` | Real | rwp | — | ✓ |
| `C46` | Real | rwp | — | ✓ |
| `C55` | Real | rwp | — | ✓ |
| `C56` | Real | rwp | — | ✓ |
| `C66` | Real | rwp | — | ✓ |

<!-- END GENERATED: elastic-parameters -->

Defaults (upstream `registerParam` defaults, checked by
`crates/akantu/tests/manual_claims.rs`): `rho = 0`, `E = 0`,
`nu = 0.5`, `alpha = 0`, `delta_T = 0`, `Plane_Stress = false`,
`finite_deformation = false`; the orthotropic moduli and the anisotropic
coefficients `Cij` default to 0; the axes `n1`, `n2`, `n3` to the global
axes.

### `elastic`

Linear isotropic elasticity, `σ = λ tr(ε) I + 2 μ ε` with the Lamé
coefficients computed from `E` and `nu`. In 2-D the default is plane
strain; `Plane_Stress = true` selects plane stress. With
`finite_deformation = true` the law is evaluated on the Green–Lagrange
strain and returns the second Piola–Kirchhoff stress (Saint
Venant–Kirchhoff), and the model assembles the total-Lagrangian forms
([Solid mechanics model](solid-mechanics.md#finite-deformation)).

A temperature change `delta_T` with expansion coefficient `alpha` adds the
thermal strain `ε_th` and the stress offset `σ_th`: `ε_th = α ΔT` in 1-D,
in 2-D plane stress and in 3-D (upstream multiplies the 3-D value by
`1 + ν`, UD-038, corrected by default, `ThermalStrain::Upstream` restores
it), and `ε_th = (1 + ν) α ΔT` in 2-D plane strain (the in-plane
eigenstrain of the plane-strain reduction, the same upstream and in the
port); `σ_th` is `−E α ΔT` (1-D), `−E α ΔT / (1 − ν)` (plane stress) and
`−E α ΔT / (1 − 2ν)` (plane strain, 3-D) in both (checked by
`akantu-solid` `tests/elastic_family_properties.rs::ud038_thermal_eigenstrain_and_free_expansion`
and the `material_thermal` unit tests); the potential energy is the stored elastic
energy `½ (ε − ε_th) : C : (ε − ε_th)` (upstream integrates `½ σ : ∇u`,
which is not a potential of the computed stress, UD-109).

### `elastic_orthotropic`

Orthotropic elasticity defined by `E1`, `E2`, `nu12`, `G12` (2-D) and
additionally `E3`, `nu13`, `nu23`, `G13`, `G23` (3-D) in the material
axes `n1`, `n2` (, `n3`). The stiffness coefficients `Cij` it computes are
readable (and writable) as for `elastic_anisotropic`.

### `elastic_anisotropic`

General linear elasticity given by the symmetric Voigt matrix `Cij`
(`i ≤ j`, 3 × 3 in 2-D, 6 × 6 in 3-D) in the material axes `n1`, `n2`
(, `n3`).

The material axes are the directions `n1`, `n2`, `n3` themselves: a
stiffness `E1` along `n1 = (0.8, 0.6)` acts in the direction
`(0.8, 0.6)`. Upstream applies the inverse rotation (UD-104: its stiff
direction is `(0.8, −0.6)`). The stable time step uses an upper bound of
the wave speeds (the largest eigenvalue of the stiffness in Mandel form);
upstream takes the first eigenvalue in Eigen's unsorted order (UD-033).

## Upstream compatibility switches

The corrected behaviours above are the defaults. `LawOptions::upstream()`
reproduces upstream for comparisons with the C++ library (category B of
decision D-003); set it on the model before `init_full`:

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::material::LawOptions;
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
let parser = Parser::parse_str("material elastic [\n name = a\n rho = 1\n E = 1\n nu = 0.3\n]\n", "m.dat")?;
let mut model = SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser)?;

assert!(LawOptions::default().material_axes_as_directions); // corrected
model.physics.law_options = Some(LawOptions::upstream()); // every switch as upstream
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
# Ok::<(), akantu::core::Error>(())
```

| Field of `LawOptions` | Default (corrected) | `upstream()` | Defect |
|---|---|---|---|
| `thermal_strain` | `ThermalStrain::Corrected` (`α ΔT`) | `ThermalStrain::Upstream` (`(1 + ν) α ΔT` in 3-D) | UD-038 |
| `thermal_energy` | `ThermalEnergy::StoredElastic` | `ThermalEnergy::Upstream` (`½ σ : ∇u`) | UD-109 |
| `celerity` | `CelerityMode::KelvinBound` | `CelerityMode::UpstreamFirstEigenvalue` | UD-033 |
| `orthotropic_parent_update` | `true` (parsed `eigen_grad_u` applied, stiffness flagged changed) | `false` | UD-105 |
| `material_axes_as_directions` | `true` | `false` (inverse rotation) | UD-104 |
| `damage_thermal_refresh` | `true` (`marigo`, `mazars`, `plastic_mazars` refresh the thermal state first) | `false` (thermal stress never refreshed) | UD-038 |
| `plastic_thermal_increment` | `true` (`Δσ_th = σ_th − σ_th,prev` in the trial and the final stress) | `false` (wrong sign in the trial, none in the final stress) | UD-038 |
| `finite_strain_plasticity` | `FiniteStrainPlasticity::Unsupported` (refused) | `FiniteStrainPlasticity::UpstreamLegacy` | UD-030 |
| `neohookean` | `NeohookeanMode::Documented` | `NeohookeanMode::UpstreamNotImplemented` (the error) | UD-026, UD-057 |
| `j2_reduced_return` | `J2ReducedReturn::Consistent` | `J2ReducedReturn::UpstreamTruncated` | UD-137 |
| `committed_trial_state` | `true` (trials from the committed hardening or damage) | `false` | UD-138 |
| `plastic_mazars_effective_stress` | `true` | `false` (J2 parent continues from the damaged stress) | UD-139 |
| `mazars_mechanical_strain` | `true` | `false` (total minus plastic strain) | UD-140 |
| `mazars_out_of_plane_strain` | `true` | `false` (`ε33 = 0`, plane-stress `λ`) | UD-141 |
| `finite_strain_damage` | `FiniteStrainDamage::Unsupported` (refused) | `FiniteStrainDamage::UpstreamLegacy` | UD-142 |
| `drucker_prager_return` | `DruckerPragerReturn::ClosedForm` | `DruckerPragerReturn::UpstreamIteration` | UD-144, UD-145, UD-146 |
| `sls_dissipation` | `SlsDissipation::Contraction` | `SlsDissipation::UpstreamMatrixProduct` | UD-029, UD-329 |
| `sls_committed_state` | `true` | `false` (history advanced at every evaluation) | UD-063 |
| `sls_energy` | `SlsEnergy::TwoSpring` | `SlsEnergy::UpstreamElastic` (`½ σ:∇u`) | UD-161, UD-329 |
| `sls_degenerate_branch` | `SlsDegenerateBranch::Limit` | `SlsDegenerateBranch::Upstream` | UD-762 |
| `maxwell_update` | `MaxwellUpdate::Recurrence` | `MaxwellUpdate::Upstream` (the stresses of the Maxwell elements never updated) | UD-029, UD-162, UD-163 |
| `maxwell_stiffness_invalidation` | `true` | `false` | UD-157 |
| `maxwell_thermal` | `MaxwellThermal::Unsupported` (`alpha ≠ 0` refused) | `MaxwellThermal::Upstream` | UD-327 |
| `anisotropic_damage` | `AnisotropicDamageModel::Unsupported` (refused) | `AnisotropicDamageModel::Upstream` | UD-031, UD-058 |
| `phasefield_tangent` | `PhaseFieldTangent::Consistent` | `PhaseFieldTangent::Upstream` | UD-164, UD-165 |
| `finite_strain_work` | `FiniteStrainWork::Conjugate` | `FiniteStrainWork::UpstreamMixed` | UD-158, UD-159 |
| `damage_thermal_work` | `true` | `false` | UD-160 |

The table lists every field of `LawOptions`. The law chapters explain what
each switch changes: [Plasticity](laws/plasticity.md),
[Damage](laws/damage.md), [Viscoelastic laws](laws/viscoelastic.md),
[The phase-field material](laws/phasefield.md) and
[Neo-Hookean hyperelasticity](laws/neohookean.md). The refused
configurations are listed in
[Differences from the C++ library](differences.md#refused-by-default).

The model-level switch `SmmUpstreamCompat` and the solver switches
`akantu::solver::UpstreamCompat` are described in
[Differences from the C++ library](differences.md).
