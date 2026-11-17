# The phase-field material

Keyword `phasefield` (upstream `MaterialPhaseField`; code
`crates/akantu-solid/src/materials/phasefield.rs`). This is the
solid side of phase-field fracture: the law degrades the elastic
stress with a damage field $d$ that the phase-field model computes. In a
simulation the solid–phase-field coupler `CouplerSolidPhaseField` writes
$d$ into the law's `damage` internal field and reads the strain back
([Phase-field fracture](../models/phase-field.md), which also describes
the phase-field laws `quadratic` and `linear` of the `phasefield`
sections of the phase-field model, another section type with the same
name).

In the variational theory of brittle fracture that the phase-field model
regularises (Francfort and Marigo, 1998; Bourdin et al., 2000), the
material is linear elastic with small strains, with the elastic energy
density

$$
\psi(\boldsymbol{\varepsilon}) = \frac{\lambda}{2} \left(\operatorname{tr} \boldsymbol{\varepsilon}\right)^2
  + \mu\, \boldsymbol{\varepsilon} : \boldsymbol{\varepsilon} ,
$$

and the phase field $d$ (0 in the intact material, 1 in the crack)
degrades this energy, so the stress is the derivative of the degraded
energy:

$$
\boldsymbol{\sigma} = \left((1 - d)^2 + \eta\right) \frac{\partial \psi}{\partial \boldsymbol{\varepsilon}} .
$$

$\eta$ (`eta`) is a small residual stiffness that keeps the stiffness
matrix regular where the material is fully broken. The law generalises
this with an energy split: only the part $\psi^+$ of the energy that
drives the crack is degraded, the part $\psi^-$ is not. The phase-field
laws `quadratic` and `linear` take the same split for their driving
energy (option of their section, [Phase-field fracture](../models/phase-field.md)).

With the degradation function $g(d) = (1 - d)^2 + \eta$ and the strain
$\boldsymbol{\varepsilon}' = \boldsymbol{\varepsilon} - \varepsilon_\text{th} \boldsymbol{I}$
(minus the thermal strain of `alpha`, `delta_T`):

$$
\boldsymbol{\sigma} = g(d)\, \boldsymbol{\sigma}^+ + \boldsymbol{\sigma}^-
$$

where, without split (`no_split`, the default option),

$$
\boldsymbol{\sigma}^+ = 2\mu\, \boldsymbol{\varepsilon}' + \lambda \operatorname{tr} \boldsymbol{\varepsilon}'\, \boldsymbol{I},
\qquad \boldsymbol{\sigma}^- = \boldsymbol{0},
$$

and with the split `volumetric_deviatoric`

$$
\boldsymbol{\sigma}^+ = \kappa \langle \operatorname{tr} \boldsymbol{\varepsilon}' \rangle_+ \boldsymbol{I}
  + 2\mu \left(\boldsymbol{\varepsilon}' - \tfrac{1}{3} \operatorname{tr} \boldsymbol{\varepsilon}'\, \boldsymbol{I}\right),
\qquad \boldsymbol{\sigma}^- = \kappa \langle \operatorname{tr} \boldsymbol{\varepsilon}' \rangle_- \boldsymbol{I}
$$

($\kappa = \lambda + 2\mu/3$, $\langle x \rangle_- = \min(x, 0)$; with `finite_deformation` the strain is the
Green–Lagrange strain and the stress the second Piola–Kirchhoff stress).
The stresses derive from the energies
$\psi^\pm = \tfrac{1}{2}\, \boldsymbol{\sigma}^\pm : \boldsymbol{\varepsilon}'$,
i.e. $\psi^+ = \psi(\boldsymbol{\varepsilon}')$ and $\psi^- = 0$ without
split, and

$$
\psi^+ = \frac{\kappa}{2} \langle \operatorname{tr} \boldsymbol{\varepsilon}' \rangle_+^2
  + \mu\, \boldsymbol{\varepsilon}'_\text{dev} : \boldsymbol{\varepsilon}'_\text{dev},
\qquad
\psi^- = \frac{\kappa}{2} \langle \operatorname{tr} \boldsymbol{\varepsilon}' \rangle_-^2
$$

with the volumetric–deviatoric split
($\boldsymbol{\varepsilon}'_\text{dev} = \boldsymbol{\varepsilon}' - \tfrac{1}{3} \operatorname{tr} \boldsymbol{\varepsilon}'\, \boldsymbol{I}$).
The split is the option of the section: `material phasefield
volumetric_deviatoric [ … ]`. The volumetric–deviatoric split leaves the
compressive volumetric stress undegraded. The tangent differentiates
this stress exactly. The potential energy is
$g\, \psi^+(\boldsymbol{\varepsilon}') + \psi^-(\boldsymbol{\varepsilon}')$, the
dissipated energy the damage base's
$\int \boldsymbol{\sigma} : \mathrm{d}\boldsymbol{\varepsilon} - \psi$
([Damage](damage.md#isotropic-damage)). With `degrade_mass = true` the
density is degraded too, $\rho_q = \min\left(\rho,\ \rho\, ((1 - d_q)^2 + \eta)\right)$ per
quadrature point, and the mass matrix is flagged as changed.

<!-- law-keywords: phasefield inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `eta` | residual stiffness `η` of the degradation function | 1 | `0` |
| `is_isotropic` | registered as upstream; no effect on the stress | bool | `false` |
| `degrade_mass` | degrade the density with `g(d)` | bool | `false` |

<!-- law-internals: phasefield -->
Internal fields: `damage` ($d$, with history), `damage dissipated
energy` and `integral of sigma` (the damage base).

$\lambda$ and $\mu$ of the split are set from `E`, `nu` and `Plane_Stress` by
`init_full`, as upstream: a later change of `E` or `nu` does not reach
the split.

Differences from upstream (switches of `LawOptions`):

* `phasefield_tangent`: upstream's volumetric–deviatoric tangent uses
  $g_h E$ in 1-D although the stress gives $g_h \kappa + g\, \tfrac{4}{3} \mu$
  ($g_h = g$ in volumetric tension, else 1), and tests
  the sign of the trace of the total strain where the stress uses
  $\operatorname{tr} \boldsymbol{\varepsilon}'$; the default is consistent with the stress (converged results
  are the same).
* `thermal_energy` (UD-109): with a thermal strain the default potential
  energy is the stored energy of the split above, upstream's
  $\tfrac{1}{2}\, \boldsymbol{\sigma} : \nabla\boldsymbol{u}$ otherwise.
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

The damage is set by hand (the coupler's job): with $d = 0.5$ and
`eta = 0.001` the stress of a uniaxial strain is $g(d)\, E \varepsilon$
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

## References

* Bourdin, B., Francfort, G. A. and Marigo, J.-J. (2000). Numerical
  experiments in revisited brittle fracture. *Journal of the Mechanics and
  Physics of Solids*, 48(4), 797–826.
* Francfort, G. A. and Marigo, J.-J. (1998). Revisiting brittle fracture
  as an energy minimization problem. *Journal of the Mechanics and Physics
  of Solids*, 46(8), 1319–1342.
