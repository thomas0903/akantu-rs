# Damage

Small-strain continuum damage laws (upstream *Damage*; code
`crates/akantu-solid/src/materials/damage.rs`, `marigo.rs`, `mazars.rs`,
`anisotropic_damage.rs`).

## Isotropic damage

`marigo`, `mazars` and `plastic_mazars` share upstream's isotropic damage
base: a scalar damage `d ∈ [0, 1]` (internal `damage`, with history)
degrades the stress of the parent law,

```text
σ = (1 − d) σ_parent
```

(`σ_parent` is the elastic stress, or the von Mises stress for
`plastic_mazars`), `d` never decreases, and the tangent is the parent's
tangent scaled by `1 − d` (the secant stiffness; the stiffness is
reassembled at every step). After every converged step the base
accumulates the work `∫σ:dε` (internal `integral of sigma`, with the
trapezoidal rule `½(σ + σ_n):Δε`) and stores the dissipated energy
density `∫σ:dε − ψ` (internal `damage dissipated energy`); `energy("dissipated")`
integrates it.

Common to the three laws (switches of `LawOptions`):

* `finite_strain_damage` (UD-142): upstream evaluates `finite_deformation`
  with the small-strain kernel on the raw `∇u` (a rotation creates
  stress); the default refuses it (`Error::Unsupported`),
  `FiniteStrainDamage::UpstreamLegacy` reproduces upstream (`plastic_mazars`
  also needs `finite_strain_plasticity`, UD-030).
* `committed_trial_state` (UD-138): the damage of a trial is computed from
  the damage committed at the beginning of the step (upstream: from the
  previous evaluation, so rejected Newton trials influence later ones).
* `damage_thermal_refresh` (UD-038): with a temperature change upstream
  never refreshes the thermal stress of these laws; the default does,
  and with `thermal_energy = StoredElastic` (UD-109) the stored energy and
  `damage_thermal_work` (UD-160) the thermal work are consistent with it.

### `marigo`

Energy-based damage (Marigo). With the elastic stress `σ_el` of the
strain `ε`, the energy release rate is `Y = ½ σ_el:ε`; the damage grows
when it exceeds the threshold:

```text
Y ← (1 − d_n) Y            if damage_in_y
Y ← min(Y, Yc)             if epsilon_c ≠ 0,  Yc = ½ E epsilon_c²
F = Y − Yd − Sd d_n
d = min(max((Y − Yd)/Sd, d_n), 1)   if F > 0,   d = d_n otherwise
σ = (1 − d) σ_el
```

`Yd` is a random internal field: a value per quadrature point drawn at
`init_full` (`Yd = 50 uniform [0, 10]`; [Input files](../input-files.md)
for the distributions).

<!-- law-keywords: marigo inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `Yd` | damage threshold on `Y` (random parameter) | stress | `0` |
| `Sd` | damage softening: `Y − Yd` needed per unit of damage | stress | `5000` |
| `epsilon_c` | critical strain of the cap `Yc = ½ E epsilon_c²` (0: no cap) | 1 | `0` |
| `damage_in_y` | use `(1 − d_n) Y` | bool | `false` |

<!-- law-internals: marigo -->
Internal fields: `damage`, `damage dissipated energy`, `integral of sigma`
(the damage base), `Y` (energy release rate) and `Yd` (the threshold at
each point).

### `mazars`

Mazars' damage for concrete, driven by the equivalent strain
`ε̂ = √(Σ_i ⟨ε_i⟩₊²)` (`ε_i` the principal strains, `⟨·⟩₊` the positive
part). When `ε̂ > K0`:

```text
d_t = 1 − K0 (1 − At)/ε̂ − At exp(−Bt (ε̂ − K0))
d_c = 1 − K0 (1 − Ac)/ε̂ − Ac exp(−Bc (ε̂ − K0))
σ̃_i = C_d ε_i + λ (ε_j + ε_k),    C_d = E(1 − ν)/((1 + ν)(1 − 2ν))     (principal effective stresses)
ε_t,i = ((1 + ν) ⟨σ̃_i⟩₊ − ν Σ_j ⟨σ̃_j⟩₊) / E                       (strains of the tensile stresses)
α_t = min(Σ_i ε_t,i ⟨ε_i⟩₊ / ε̂², 1),   α_c = 1 − α_t
d = min(max(α_t^β d_t + α_c^β d_c, d_n), 1)
```

and `σ = (1 − d) σ_el`. `K0` is a random internal field. The strain is
the mechanical strain (total minus thermal `α ΔT`, and minus plastic for
`plastic_mazars`), and in 2-D it is embedded in the 3-D strain it
represents (plane strain `ε33 = 0`; plane stress
`ε33 = −ν/(1 − ν)(ε11 + ε22)`) before the principal strains are computed.

<!-- law-keywords: mazars inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `K0` | damage threshold on `ε̂` (random parameter) | 1 | `0` |
| `At` | tension: asymptotic parameter | 1 | `0.8` |
| `Bt` | tension: decay parameter | 1 | `12000` |
| `Ac` | compression: asymptotic parameter | 1 | `1.4` |
| `Bc` | compression: decay parameter | 1 | `1900` |
| `beta` | shear parameter `β` (exponent of `α_t`, `α_c`) | 1 | `1.06` |

<!-- law-internals: mazars -->
Internal fields: `damage`, `damage dissipated energy`, `integral of sigma`,
`K0` (the threshold at each point) and `Ehat` (ε̂ of the last
evaluation; upstream keeps it as one scalar member, UD-067).

Differences from upstream: `LawOptions::mazars_mechanical_strain`
(UD-140; upstream: total minus plastic strain, so free heating damages)
and `mazars_out_of_plane_strain` (UD-141; upstream: `ε33 = 0` and the
plane-stress `λ` in 2-D). Upstream's manual describes the weights `α_t`
with "the strains of the undamaged material" and without the cap at 1;
the code (upstream and port) uses the strains of the tensile principal
effective stresses and the cap (UD-1132).

### `plastic_mazars`

Mazars' damage on the von Mises plasticity of
`plastic_linear_isotropic_hardening` (upstream `MaterialVonMisesMazars`):
the radial return of [Plasticity](plasticity.md) gives the undamaged
(effective) stress, the Mazars damage is evaluated on the elastic strain
`ε − εᵖ`, and the stress is `(1 − d) σ_eff`. The potential energy is
`(1 − d) ½ σ_eff:(ε − εᵖ)`; both `dissipated` and `plastic` are defined.

<!-- law-keywords: plastic_mazars inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `sigma_y` | initial yield stress | stress | `0` |
| `h` | hardening modulus | stress | `0` |
| `K0` | damage threshold on `ε̂` (random parameter) | 1 | `0` |
| `At` | tension: asymptotic parameter | 1 | `0.8` |
| `Bt` | tension: decay parameter | 1 | `12000` |
| `Ac` | compression: asymptotic parameter | 1 | `1.4` |
| `Bc` | compression: decay parameter | 1 | `1900` |
| `beta` | shear parameter | 1 | `1.06` |

<!-- law-internals: plastic_mazars -->
Internal fields: those of the von Mises law (`iso_hardening`,
`inelastic_strain`, `plastic_energy`, `d_plastic_energy`), those of
Mazars' damage (`damage`, `damage dissipated energy`, `integral of
sigma`, `K0`, `Ehat`) and `effective_stress` (the undamaged stress, with
history).

Difference from upstream: `LawOptions::plastic_mazars_effective_stress`
(UD-139): upstream's next return mapping starts from the damaged
stress (the stress decays at a fixed strain); the default runs the
return on the undamaged stress history `effective_stress`.

Tests of the isotropic damage laws:
`crates/akantu-solid/tests/inelastic_properties.rs`
(`damage_monotone_bounded_and_dissipation_nonnegative`,
`damage_unloading_is_secant_elastic_with_constant_dissipation`,
`plastic_mazars_uses_the_elastic_strain`,
`ud038_thermal_state_of_inelastic_laws`), `test_damage_materials.rs`
(Mazars in 1-D, 2-D and 3-D, compression softening),
`upstream_material_mazars.rs` (upstream's test, and its bitwise
reproduction in the upstream mode), `review_v32_regressions.rs`
(UD-138…142) and the oracle fixtures of `inelastic_material_fixtures.rs`.

## Anisotropic damage: `anisotropic_damage`

A second-order damage tensor `D` for quasi-brittle materials (upstream
`MaterialAnisotropicDamage`, options `mazars` (the default) and
`mazars-drucker-prager`, which adds the keyword `k`). By default the
law refuses to compute a stress: upstream's code is not a usable
constitutive law (UD-031: it never writes the stress it degrades, so its
stress is zero, and its threshold is met at every evaluation; UD-058 for
the specification). `LawOptions::anisotropic_damage` selects:

* `AnisotropicDamageModel::Unsupported` (default): the stress and the
  tangent are `Error::Unsupported`;
* `AnisotropicDamageModel::Upstream`: the reference code, with its
  uninitialised entries defined as zero (for comparisons with the
  oracle);
* `AnisotropicDamageModel::Corrected`: the corrected model of the UD-058
  specification (decision D-066 in `docs/DECISIONS.md`), an
  opt-in until its independent review. Its damage grows in the direction
  of the positive strains, `D_{n+1} − D_n` is positive semi-definite and
  the principal damages stay at most `Dc`. Limitation: once a principal
  damage has reached `Dc`, a load that opens that direction again stops
  the damage evolution in every direction for as long as it lasts;
  loading orthogonal to the saturated direction keeps damaging. Plane
  stress and `finite_deformation` are refused by it;
* `AnisotropicDamageModel::DesmoratExperimental`: a
  formulation rejected at review (its damage is not monotone), kept as
  an explicitly selected opt-in.

The threshold function of the accumulated damage trace `T`: with
`Corrected` (and `DesmoratExperimental`) it is
`κ(T) = a tan(T/(a A) + atan(K0/a))` (so `κ(0) = K0`); upstream's code
(`Upstream`) uses `K(T) = a tan(atan(T/a) − atan(K0/a))`, so
`K(0) = −K0` (UD-031). The equivalent
strain `√(ε⁺:ε⁺)` (plus `k tr ε` with `mazars-drucker-prager`).

<!-- law-keywords: anisotropic_damage inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `Dc` | critical (maximal) principal damage | 1 | `0.99` |
| `a` | threshold function: strain scale `a` | 1 | `0.000293` |
| `A` | threshold function: damage scale `A` | 1 | `5000` |
| `K0` | initial threshold `κ(0)` | 1 | `0.00005` |

<!-- law-keywords: anisotropic_damage option=mazars-drucker-prager inherits=elastic -->
| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `k` | Drucker–Prager coefficient of `tr ε` in the equivalent strain | 1 | `0` |
| `Dc` | as above | 1 | `0.99` |
| `a` | as above | 1 | `0.000293` |
| `A` | as above | 1 | `5000` |
| `K0` | as above | 1 | `0.00005` |

<!-- law-internals: anisotropic_damage -->
Internal fields: `damage` (the tensor D, dim² components),
`elastic_stress` (dim² components), `equivalent_strain` and
`trace_damage` (T, with history).

Tests: `crates/akantu-solid/tests/test_material_anisotropic_damage.rs`
(`mv01_default_is_unsupported`, `upstream_mode_defects`,
`drucker_prager_variant_through_the_factory`,
`anisotropic_damage_parameters`, and the `ud058_*` checks of the
corrected model: irreversibility, saturation, dissipation on random
paths), `review_v100_regressions.rs` and the fixtures of the UD-031
oracle variant (`fixtures/material/anisotropic_damage_mazars/variant_ud031`).

## Example

`marigo` under the uniaxial strain `ε_xx = 0.5` (`nu = 0`, plane strain:
`σ_el,xx = E ε_xx = 8`) has `Y = ½ σ_el:ε = 2`, so with `Yd = 1` and
`Sd = 4` the damage is `d = (Y − Yd)/Sd = 0.25` and `σ_xx = 6`; the
default `anisotropic_damage` refuses to compute:

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::core::{ElementType, GhostType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::constitutive_law::{ConstitutiveLaw, InternalFieldRef};
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
# fn internal(model: &SolidMechanicsModel, name: &str, ty: ElementType) -> akantu::core::Result<Vec<f64>> {
#     let mut out = None;
#     model.material(0)?.for_each_internal(&mut |n, f| {
#         if let (true, InternalFieldRef::Real(f)) = (n == name, f) {
#             out = f.get(ty, GhostType::NotGhost).ok().map(|a| a.as_slice().to_vec());
#         }
#     });
#     out.ok_or_else(|| akantu::core::Error::invalid(format!("no internal {name}")))
# }
let input = "material marigo [
    name = concrete
    rho  = 1
    E    = 16
    nu   = 0
    Yd   = 1     # a plain number: no random part
    Sd   = 4
]";
let mut model = model_on("square.msh", input)?;
impose_gradient(&mut model, &[0.5, 0.0, 0.0, 0.0])?;
model.assemble_internal_forces()?;

let quad = ElementType::Quadrangle4;
for y in internal(&model, "Y", quad)? {
    assert!((y - 2.0).abs() <= 1e-12 * 2.0);
}
for d in internal(&model, "damage", quad)? {
    assert!((d - 0.25).abs() <= 1e-12 * 0.25);
}
let stress = model.material(0)?.base().stress.get(quad, GhostType::NotGhost)?;
for s in stress.tuples() {
    assert!((s[0] - 6.0).abs() <= 1e-12 * 6.0);
}
drop(stress);

// anisotropic_damage: the default model refuses (UD-031, UD-058)
let aniso = "material anisotropic_damage [\n name = a\n rho = 1\n E = 1\n nu = 0.2\n]\n";
let mut model = model_on("square.msh", aniso)?;
impose_gradient(&mut model, &[1e-3, 0.0, 0.0, 0.0])?;
assert!(matches!(model.assemble_internal_forces(), Err(akantu::core::Error::Unsupported(_))));
# Ok::<(), akantu::core::Error>(())
```
