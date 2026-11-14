# The phase-field material

Keyword `phasefield` (upstream `MaterialPhaseField`; code
`crates/akantu-solid/src/materials/phasefield.rs`). This is the
solid side of phase-field fracture: the law degrades the elastic
stress with a damage field `d` that the phase-field model computes. In a
simulation the solid–phase-field coupler `CouplerSolidPhaseField` writes
`d` into the law's `damage` internal field and reads the strain back
([Phase-field fracture](../models/phase-field.md), which also describes
the phase-field laws `quadratic` and `linear` of the `phasefield`
sections of the phase-field model, another section type with the same
name).

With the degradation function `g(d) = (1 − d)² + η` and the strain
`ε' = ε − ε_th I` (minus the thermal strain of `alpha`, `delta_T`):

```text
σ = g(d) σ⁺ + σ⁻
no_split (default option):   σ⁺ = 2μ ε' + λ tr ε' I,                 σ⁻ = 0
volumetric_deviatoric:       σ⁺ = κ ⟨tr ε'⟩₊ I + 2μ (ε' − (tr ε'/3) I),   σ⁻ = κ ⟨tr ε'⟩₋ I
```

(`κ = λ + 2μ/3`; with `finite_deformation` the strain is the
Green–Lagrange strain and the stress the second Piola–Kirchhoff stress).
The split is the option of the section: `material phasefield
volumetric_deviatoric [ … ]`. The volumetric–deviatoric split leaves the
compressive volumetric stress undegraded. The tangent differentiates
this stress exactly. The potential energy is `g ψ⁺(ε') + ψ⁻(ε')`, the
dissipated energy the damage base's `∫σ:dε − ψ`
([Damage](damage.md#isotropic-damage)). With `degrade_mass = true` the
density is degraded too, `ρ_q = min(ρ, ρ ((1 − d_q)² + η))` per
quadrature point, and the mass matrix is flagged as changed.

<!-- law-keywords: phasefield inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `eta` | residual stiffness `η` of the degradation function | 1 | `0` |
| `is_isotropic` | registered as upstream; no effect on the stress | bool | `false` |
| `degrade_mass` | degrade the density with `g(d)` | bool | `false` |

<!-- law-internals: phasefield -->
Internal fields: `damage` (d, with history), `damage dissipated
energy` and `integral of sigma` (the damage base).

`λ` and `μ` of the split are set from `E`, `nu` and `Plane_Stress` by
`init_full`, as upstream: a later change of `E` or `nu` does not reach
the split.

Differences from upstream (switches of `LawOptions`):

* `phasefield_tangent`: upstream's volumetric–deviatoric tangent uses
  `g_h E` in 1-D although the stress gives `g_h κ + g (4/3) μ`, and tests
  the sign of the trace of the total strain where the stress uses
  `tr ε'`; the default is consistent with the stress (converged results
  are the same).
* `thermal_energy` (UD-109): with a thermal strain the default potential
  energy is the stored energy of the split above, upstream's `½σ:∇u`
  otherwise.
* `finite_strain_work` (UD-158, UD-159) and `damage_thermal_work`
  (UD-160): the work and dissipation updates of the damage base.
* `degrade_mass` with several quadrature points per element: upstream
  scales every point by the first point's degradation (out-of-bounds
  writes); the port scales each point by its own damage.

Tests: `crates/akantu-solid/tests/test_material_phasefield.rs`
(`degradation_function_bounds`, `undamaged_split_recovers_elastic_stress`,
`volumetric_deviatoric_split_reconstruction`,
`potential_energy_is_degraded_split_energy`,
`tangent_matches_finite_differences`, `degraded_mass_per_quadrature_point`,
`phasefield_options`), the oracle fixtures of `material_fixtures_mat3.rs`
and, through the coupler, `crates/akantu-couplers/tests/phase_field_coupling.rs`.

## Example

The damage is set by hand (the coupler's job): with `d = 0.5` and
`eta = 0.001` the stress of a uniaxial strain is `g(d) E ε`
(`nu = 0`, no split):

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::core::{ElementType, GhostType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::constitutive_law::{ConstitutiveLaw, InternalFieldMut};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# fn model_on(mesh: &str, input: &str) -> akantu::core::Result<SolidMechanicsModel> {
#     let dim = if mesh == "cube.msh" { 3 } else { 2 };
#     let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#         .join("../../docs/manual/src/meshes")
#         .join(mesh);
#     let mut m = Mesh::new(dim, "mesh")?;
#     akantu::io::read_mesh(&mut m, &file, MeshIoType::Auto)?;
#     let parser = Parser::parse_str(input, "material.dat")?;
#     let mut model =
#         SolidMechanicsModel::with_parser(SharedMesh::new(m), None, "solid_mechanics_model", parser)?;
#     model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
#     Ok(model)
# }
# fn impose_gradient(model: &mut SolidMechanicsModel, h: &[f64]) -> akantu::core::Result<()> {
#     let dim = if h.len() == 9 { 3 } else { 2 };
#     let x: Vec<f64> = model.physics.base.mesh.read()?.nodes().as_slice().to_vec();
#     for (u, x) in model.displacement_mut()?.tuples_mut().zip(x.chunks(dim)) {
#         for i in 0..dim {
#             u[i] = (0..dim).map(|j| h[i * dim + j] * x[j]).sum();
#         }
#     }
#     Ok(())
# }
let input = "material phasefield [\n name = solid\n rho = 1\n E = 1\n nu = 0\n eta = 0.001\n]\n";
let mut model = model_on("square.msh", input)?;
let (quad, ng) = (ElementType::Quadrangle4, GhostType::NotGhost);
model.material_mut(0)?.for_each_internal_mut(&mut |name, field| {
    if let (true, InternalFieldMut::Real(f)) = (name == "damage", field) {
        f.get_mut(quad, ng)?.fill(0.5);
    }
    Ok(())
})?;
let eps = 1e-3;
impose_gradient(&mut model, &[eps, 0.0, 0.0, 0.0])?;
model.assemble_internal_forces()?;

let g = (1.0_f64 - 0.5).powi(2) + 0.001;
let stress = model.material(0)?.base().stress.get(quad, ng)?;
for s in stress.tuples() {
    assert!((s[0] - g * eps).abs() <= 1e-12 * g * eps);
}
# Ok::<(), akantu::core::Error>(())
```
