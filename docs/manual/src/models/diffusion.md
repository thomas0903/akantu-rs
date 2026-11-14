<!-- availability: available akantu::diffusion -->
# Diffusion (heat transfer)

<!-- BEGIN GENERATED: availability-status diffusion -->
**Available in this release** (facade module `akantu::diffusion`, crate `akantu-diffusion`, feature `diffusion`, on by default).
<!-- END GENERATED: availability-status diffusion -->

This chapter corresponds to *Diffusion model* (`diffusion.rst`) of the
upstream manual. The model is `akantu::diffusion::HeatTransferModel`: a
`DiffusionModel` whose degree of freedom is the temperature (crate
`akantu-diffusion`, feature `diffusion`, on by default), driven by the law
`heat_diffusion` (parameters `density`, `capacity`, `conductivity`, and the
optional `conductivity_variation` and `temperature_reference`). It solves
`ρc Ṫ = ∇·(k ∇T)` with the three analysis methods of upstream: `Static`,
`ImplicitDynamic` and `ExplicitLumpedMass`. It also runs on a distributed
mesh (upstream throws there, UD-275).

## Set-up and one explicit step

The example builds a bar of three segments, reads the law from an input
string, starts from a hot middle node and makes one explicit step at half
the stable time step. The rate of the insulated bar satisfies
`Σ M_i Ṫ_i = 0`, so the lumped thermal content `Σ M_i T_i` is unchanged by
the step. Guards on the model's arrays are scoped by a block and never
kept across a `solve_step`.

```rust
use akantu::core::enums::AnalysisMethod;
use akantu::core::{ElementType, GhostType, Idx, Real};
use akantu::diffusion::HeatTransferModel;
use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu::parser::Parser;

# fn main() -> akantu::core::Result<()> {
let mut mesh = Mesh::new(1, "bar")?;
{
    let mut a = MeshAccessor::new(&mut mesh);
    for i in 0..4 {
        a.push_node(&[0.25 * i as Real])?;
    }
    let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
    for i in 0..3 {
        c.push_tuple(&[i as Idx, (i + 1) as Idx]);
    }
}
mesh.compute_bounding_box()?;
let parser = Parser::parse_str(
    "model heat_transfer_model [\n\
     constitutive_law heat_diffusion [\n\
     name = copper\n density = 8940\n capacity = 385\n conductivity = [[401]]\n\
     ]\n]\n",
    "heat.dat",
)?;
let mut model =
    HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)?;
model.init_full_with(AnalysisMethod::ExplicitLumpedMass)?;
{
    // the write guard lives until the end of this block
    let mut t = model.temperature_mut()?;
    t.as_mut_slice().copy_from_slice(&[20.0, 100.0, 20.0, 20.0]);
}
let dt = 0.5 * model.stable_time_step()?;
model.set_time_step(dt, None)?;
model.solve_step(None)?;
let m = model.lumped_capacity()?;
let content = |t: &[Real]| t.iter().zip(m.as_slice()).map(|(t, m)| t * m).sum::<Real>();
let t1 = model.temperature()?.as_slice().to_vec();
let rate = model.temperature_rate()?.as_slice().to_vec();
// the hot node cools, its neighbours warm, the insulated bar keeps its heat
assert!(rate[1] < 0.0 && rate[0] > 0.0 && rate[2] > 0.0);
let before = content(&[20.0, 100.0, 20.0, 20.0]);
assert!((content(&t1) - before).abs() <= 1e-12 * before);
# Ok(())
# }
```

## Implicit schemes and their default

`ImplicitDynamic` uses backward Euler with the temperature as unknown, as
upstream's `getDefaultSolverOptions` does. Upstream's generalized
trapezoidal predictor keeps the rate of the previous step, so upstream's
backward Euler stops evolving after its first step under constant loads
and its trapezoidal rule 1 is not Crank–Nicolson (UD-287). The port
corrects the predictor by default; the switch
`generalized_trapezoidal_keeps_rate_in_predictor` of
`akantu::solver::UpstreamCompat` (set on the DOF manager) reproduces
upstream. The corrected trapezoidal rule 1 is second order in time from a
consistent initial rate `M Ṫ⁰ = f − K T⁰` and first order from the default
`Ṫ⁰ = 0` (what upstream always starts from);
`DiffusionModel::set_consistent_initial_rate` sets the consistent rate.

The other differences from upstream (each with a default that corrects it
and, where it changes results, a switch of `DiffusionUpstreamCompat`) are
listed with the entries UD-275…290 and UD-811 of
`docs/UPSTREAM_DEFECTS.md`.
