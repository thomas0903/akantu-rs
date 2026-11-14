# Viscoelastic laws

Two small-strain viscoelastic laws (upstream *Visco-Elastic*; code
`crates/akantu-solid/src/materials/sls_deviatoric.rs` and `maxwell.rs`).
Both read the model's time step `Δt`: set it with
`model.set_time_step(dt, None)` before the first solve, also in a static
analysis (a quasi-static loading history).

## Standard linear solid: `sls_deviatoric`

A spring `E∞` in parallel with a Maxwell arm (a spring `Ev` in series
with a dashpot `Eta`), acting on the deviatoric strain only; the
volumetric response is elastic with the long-term modulus. `E` is the
instantaneous modulus `E∞ + Ev`, so `E∞ = E − Ev` (readable as `Einf`,
recomputed whenever the parameters change). In 1-D the law is (upstream's
manual)

```text
dε/dt = (E∞ + Ev)⁻¹ [ dσ/dt + (Ev/η) σ − (E∞ Ev/η) ε ]
```

and the port integrates the arm with the exponential midpoint rule.
With `τ = η/Ev`, `γ∞ = E∞/E`, `γ_v = Ev/E`, the bulk coefficient `κ`
and shear modulus `μ` of `E`, `ν` (plane strain or plane stress in 2-D),
per quadrature point and step:

```text
ε = ½(∇uᵀ + ∇u),   Θ = tr ε,   ε_d = ε − (Θ/3) I,   s = 2μ ε_d
h ← e^{−Δt/τ} h + e^{−Δt/(2τ)} (s − s_old),   s_old ← s
σ = γ∞ κ Θ I + γ∞ s + γ_v h
```

`h` is the arm stress (internal `history_integral`), `s` the elastic
deviatoric stress (`stress_dev`). The midpoint factor `e^{−Δt/(2τ)}`
replaces the exact `(1 − e^{−x})/x` of a strain linear over the step
(`x = Δt/τ`), a second-order approximation. In 1-D and 2-D the deviator
keeps the 3-D `Θ/3` (uniaxial strain in 1-D, plane-strain kinematics in
2-D); with `Plane_Stress = true` only the bulk coefficient `κ` changes:
no `σ33 = 0` condition acts on the arm (UD-328). The tangent is the
instantaneous elastic tangent (modulus `E`).

<!-- law-keywords: sls_deviatoric inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `Eta` | viscosity `η` of the dashpot | stress·time | `1` |
| `Ev` | stiffness of the viscous arm | stress | `1` |

`E` must be larger than or equal to `Ev` (the defaults `E = 0`, `Ev = 1`
are refused, see below).

<!-- law-internals: sls_deviatoric -->
Internal fields: `stress_dev` (the elastic deviatoric stress s),
`history_integral` (the arm stress h) and `dissipated_energy` (per
point, accumulated).

Energies: `potential` is the free energy of the two springs,
`ψ = ½ σ∞:ε + γ_v/(4μ) h:B⁻¹:h` with `σ∞ = γ∞(κ tr ε I + 2μ ε_d)`
(`B⁻¹h = h + I tr h/(3 − D)` on the `D × D` block in 1-D and 2-D,
`B = I` on 3-D deviators), and `dissipated` the energy dissipated in the
dashpot, `Σ γ_v/(2μτ) h:B⁻¹:h Δt`.

Differences from upstream (switches of `LawOptions`):

* `sls_energy` (UD-161): upstream's potential energy is `½σ:∇u` with the
  total stress, which misses the energy of the arm spring; the default
  `SlsEnergy::TwoSpring` is the free energy above and refuses
  `finite_deformation` and `alpha ≠ 0` (upstream ignores both).
* `sls_dissipation` (UD-029): upstream sums the entries of the matrix
  product `(ε_d − αq)·q̇` (not rotation invariant); the default is the
  double contraction.
* `sls_committed_state` (UD-063): upstream advances `h`, `s_old` and the
  dissipation at every stress evaluation, so the repeated evaluations of a
  Newton step evolve the history several times; the default evaluates
  every trial from the state committed at the beginning of the step.
* `sls_degenerate_branch` (UD-762): upstream evaluates `Ev = 0` or
  `Eta = 0` as written (a NaN dissipated energy); the default takes the
  limits (no arm, instantaneous relaxation) and refuses negative or
  non-finite `Ev`, `Eta` and `Ev > E` (a negative `E∞`).

Tests: `crates/akantu-solid/tests/test_material_viscoelastic.rs`
(`test_material_standard_linear_solid_deviatoric_relaxation`,
`creep_converges_to_closed_form`, `ud029_sls_dissipation_frame_invariance`,
`ud063_sls_committed_state`, `viscoelastic_elastic_limits`),
`upstream_material_standard_linear_solid_deviatoric_relaxation.rs` (the
ports of upstream's relaxation tests) and `review_v40_regressions.rs`
(the 1-D and 2-D energy balance).

## Generalised Maxwell model: `viscoelastic_maxwell`

A spring `Einf` in parallel with `N` Maxwell arms (spring `Ev_k` in
series with a dashpot `Eta_k`), on the whole strain tensor. `Ev` and `Eta`
are vectors of the same length `N` (`Ev = [1e9, 2e9]`). `init_full`
overwrites `E` with the instantaneous modulus `Einf + Σ|Ev_k|` before the
Lamé constants are computed. With the unit-modulus isotropic stiffness
`C` (Voigt, engineering shears; plane strain in 2-D, whatever
`Plane_Stress` says; `C = 1` in 1-D), the relaxation times
`λ_k = Eta_k/Ev_k`, `e_k = exp(−Δt/λ_k)` and the effective arm
stiffnesses `E_k^eff = (1 − e_k) Ev_k λ_k/Δt` (`Ev_k` if `e_k = 1`):

```text
σ = Einf C ε + Σ_k ( E_k^eff C Δε + e_k σ_v,k )        Δε: strain increment of the step
tangent = (Einf + Σ_k E_k^eff) C
after a converged step:   σ_v,k ← e_k σ_v,k + E_k^eff C Δε
```

So a slow loading (`Δt ≫ λ_k`) sees the modulus `Einf` and a fast one
`Einf + Σ Ev_k`.

<!-- law-keywords: viscoelastic_maxwell inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `Einf` | long-term modulus (the parallel spring) | stress | `1` |
| `Eta` | viscosities of the arms (a vector) | stress·time | `[]` |
| `Ev` | stiffnesses of the arms (a vector, same length as `Eta`) | stress | `[]` |

`previous_dt` (the time step of the last evaluation) is readable only.

<!-- law-internals: viscoelastic_maxwell -->
Internal fields: `sigma_v` and `epsilon_v` (the stress and strain of every
arm: dim² · N components, registered at initialisation),
`dissipated_energy` and `mechanical_work` (per point, accumulated).

Energies: `potential` (the stored internal, refreshed after every
converged step: it lags by one step in the middle of a step, as
upstream), `dissipated` and `work`
(`W += ½(σ + σ_prev):Δε`, `dissipated = W − ψ`).

Differences from upstream (switches of `LawOptions`):

* `maxwell_update` (UD-029): upstream computes the new arm stresses
  after a converged step into a temporary that is discarded
  (`material_viscoelastic_maxwell.cc:253-260`), so the arm stresses stay
  at their initial zero and the arms keep no stress history (its manual
  describes the recurrence); the default
  `MaxwellUpdate::Recurrence` commits the recurrence above, and the stored
  arm strains are the tensorial strains with the compliance `C⁻¹`
  (upstream doubles the shear terms and mixes plane-stress compliance with
  plane-strain stiffness in 2-D).
* `maxwell_thermal` (UD-327): upstream adds the thermal stress of the
  instantaneous modulus to a law whose arms and energy ignore the
  thermal strain; the default refuses `alpha ≠ 0`.
* `maxwell_stiffness_invalidation` (UD-157): a parameter change flags the
  stiffness as changed (upstream only compares the time step).
* `Eta` and `Ev` of different lengths and `finite_deformation` are
  refused by `init_full` (upstream: debug assertions only).
* `Plane_Stress` has no effect on the stress and the tangent of this law
  (`C` is the plane-strain stiffness, in the port as upstream), where
  upstream's manual lists it as the plane-stress switch of the law
  (UD-1131).

Tests: `crates/akantu-solid/tests/test_material_viscoelastic.rs`
(`test_material_viscoelastic_maxwell_relaxation`,
`maxwell_relaxation_matches_closed_form_3d_two_branches`,
`maxwell_dissipation_and_energy_balance`,
`maxwell_tangent_and_stiffness_flag`,
`maxwell_rejects_inconsistent_branches`),
`upstream_material_viscoelastic_maxwell_relaxation.rs` (upstream's test,
which fails in the upstream mode as it does upstream) and the example
`material_viscoelastic_maxwell_energies`
(`crates/akantu/tests/examples_material_viscoelastic_maxwell_energies.rs`).

## Example

One step of `viscoelastic_maxwell` from rest with two arms: the
stress of a uniaxial strain `ε` is `(Einf + E_1^eff + E_2^eff) ε`
(`nu = 0`, so `C11 = 1`); and `sls_deviatoric` refuses `Ev > E`:

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::core::{ElementType, GhostType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::{ParamValue, ParameterRegistry, Parser};
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
let input = "material viscoelastic_maxwell [
    name = polymer
    rho  = 1
    nu   = 0
    Einf = 1
    Ev   = [1, 2]
    Eta  = [1, 4]
]";
let mut model = model_on("square.msh", input)?;
// init_full sets the instantaneous modulus E = Einf + Σ|Ev_k|
assert_eq!(model.material(0)?.get_param("E")?, ParamValue::Real(4.0));

let dt = 0.5;
model.set_time_step(dt, None)?;
let eps = 1e-3;
impose_gradient(&mut model, &[eps, 0.0, 0.0, 0.0])?; // u_x = ε x
model.assemble_internal_forces()?;

let effective = |ev: f64, eta: f64| {
    let lambda = eta / ev;
    (1.0 - (-dt / lambda).exp()) * ev * lambda / dt
};
let expected = (1.0 + effective(1.0, 1.0) + effective(2.0, 4.0)) * eps;
let stress = model.material(0)?.base().stress.get(ElementType::Quadrangle4, GhostType::NotGhost)?;
for s in stress.tuples() {
    // [σxx, σyx, σxy, σyy]
    assert!((s[0] - expected).abs() <= 1e-12 * expected);
    assert!(s[3].abs() <= 1e-12 * expected);
}
drop(stress); // release the read guard before building another model

// the standard linear solid: E∞ = E − Ev is readable; Ev > E is refused (UD-762)
let sls = model_on("square.msh", "material sls_deviatoric [\n name = a\n rho = 1\n E = 3\n nu = 0.3\n Ev = 1\n Eta = 2\n]\n")?;
assert_eq!(sls.material(0)?.get_param("Einf")?, ParamValue::Real(2.0));
assert!(model_on("square.msh", "material sls_deviatoric [\n name = a\n rho = 1\n E = 1\n nu = 0.3\n Ev = 2\n]\n").is_err());
# Ok::<(), akantu::core::Error>(())
```
