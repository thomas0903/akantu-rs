<!-- availability: available akantu::ntn_contact -->
# Split-node contact and rate-and-state friction

<!-- BEGIN GENERATED: availability-status ntn-contact -->
**Available behind the Cargo feature `ntn-contact`**, off by default (facade module `akantu::ntn_contact`, crate `akantu-ntn-contact`).
<!-- END GENERATED: availability-status ntn-contact -->

This chapter corresponds to *Node To Node Contact* (`nodetonodecontact.rst`)
of the upstream manual. It documents the port of upstream's extra package
`traction-at-split-node-contact`: crate `akantu-ntn-contact`, reached as
`akantu::ntn_contact` with the facade feature `ntn-contact` (off by
default, as upstream's package), or directly as `akantu_ntn_contact`:

```toml
[dependencies]
akantu = { path = "…/akantu-rs/crates/akantu", features = ["ntn-contact"] }
```

The examples below use the crate name `akantu_ntn_contact`; with the
facade the same items are under `akantu::ntn_contact`.

The package couples matching ("split") node pairs of two surfaces of one
`SolidMechanicsModel` in explicit dynamics (central differences with a
lumped mass). On a distributed mesh, call `NtnBaseContact::init_parallel`
(upstream `initParallel`) after the contact nodes were added: the contact
then exchanges the nodal data of the elements around its nodes (parity
rows MPI-051/052); without it a contact step on a distributed mesh is
refused with `Error::Unsupported` (UD-904; see
[Parallel computations](../parallel.md); test
`manual_claims_2.rs::doc013_split_node_contact_on_a_distributed_mesh`).
The pairs are formed on each process from its own nodes, so both sides of
the interface must lie on the same processes; `init_parallel` refuses a
partition that separates them (UD-1105).

## Formulation

For every pair (slave node `s` on one surface, master node `m` on the
other) with the normal `n` of the slave surface, lumped boundary areas
`A_s`, `A_m` and lumped masses `m_s`, `m_m`, the impedance is

```text
Z = 2 / ( Δt (A_m/m_m + A_s/m_s) )
```

At each residual assembly the free interpenetration of the pair,
`I = [x]_N + Δt [v]_N + Δt² [a_free]_N − ½ Δt² [a_old]_N` (relative normal
gap, velocity, acceleration of the residual without contact forces, previous
acceleration), decides the contact: the pair is in contact when `I ≤ 0`
and then gets the normal traction `T_N = Z I / (2Δt)`, applied as `−A_s T_N`
on the slave and `+A_m T_N` on the master. The tangential traction that
keeps the pair stuck is `T_T = ½ Z (v_t + Δt a_t,free − ½ Δt a_t,old)`; if
it exceeds the frictional strength `S` of the friction law the pair slides
and the traction is scaled to `S`. These are the expressions of the
upstream manual (its `Z⁻¹ = Δt/2 (M₊⁻¹A₊ + M₋⁻¹A₋)`); the port's code is
`NtnBaseContact::update_impedance`, `compute_contact_pressure` and
`NtnFriction::compute_friction_traction`, checked against hand
computations (tests `independent_checks.rs::contact_quantities_hand_computed`,
`contact_pressure_closed_form_and_momentum_of_assembly`,
`coulomb_stick_and_slip_regimes`) and against upstream bit for bit (49
oracle cases of parity rows TASN-*, `oracle_ntn_fixtures.rs`,
`oracle_ntn_adversarial.rs`).

## Using the contact

Upstream's
`NTNContactSolverCallback coupler(solid, slave, master, surface_normal_dir, time_step_factor)`
is `NtnContactSolverCallback::new` with one more argument, the
upstream-defect switches. The constructor initialises the model for
explicit dynamics, assembles the lumped mass, pairs the nodes of the two
surfaces (element groups), sets the time step to `time_step_factor` times
the stable time step and reads the friction law from the first `friction`
section of the input file. A step is then `solve_step` of the callback:

```rust
use akantu::core::Real;
use akantu::core::enums::MeshIoType;
use akantu::mesh::{Mesh, SharedMesh};
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;
use akantu_ntn_contact::{NtnContactSolverCallback, TasnUpstreamCompat};

// slider [0, 1] × [0, 0.25] on base [0, 1] × [−0.25, 0] (upstream test mesh)
let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("../../docs/manual/src/meshes/ntn_2d.msh");
let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
// the materials of upstream's test input; both are equal, and without a
// selector every element uses the first one (see the Materials chapter)
let input = "
friction coulomb [
    mu = 0.3
]
material elastic [
    name = slider
    rho  = 1200
    E    = 7.98e9
    nu   = 0.33
    Plane_Stress = 1
]
material elastic [
    name = base
    rho  = 1200
    E    = 7.98e9
    nu   = 0.33
    Plane_Stress = 1
]";
let parser = Parser::parse_str(input, "ntn.dat")?;
let mut model =
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
let mut ntn = NtnContactSolverCallback::new(
    &mut model,
    "slider_bottom", // slave surface
    "base_top",      // master surface
    1,               // the normal is along y
    0.5,             // Δt = 0.5 × stable time step
    TasnUpstreamCompat::default(),
)?;
assert_eq!(ntn.nb_contact_nodes(), 3); // pairs at x = 0, 0.5, 1
let pairs = ntn.contact().node_pairs()?;

// throw the slider onto the base
let slider: Vec<usize> = {
    let mesh = model.physics.base.mesh.read()?;
    let group = mesh.groups().element_group("slider")?;
    group.nodes().iter().map(|&n| n as usize).collect()
};
{
    // one write guard on the velocity, released at the end of this block,
    // before the first `solve_step` (see "Reading and writing model arrays"
    // in the Solid mechanics chapter)
    let mut v_guard = model.velocity_mut()?;
    let v = v_guard.as_mut_slice();
    for &n in &slider {
        v[2 * n] = 0.5;
        v[2 * n + 1] = -1.0;
    }
}
let dt = model.time_step(None)?;
let mut contacts = 0;
for step in 0..50 {
    ntn.solve_step(&mut model, None)?;
    contacts += ntn.contact().is_in_contact().as_slice().iter().filter(|&&c| c).count();
    // normal gap u_y(slave) − u_y(master) of every pair; the read guard
    // lives in this block only, never across the next `solve_step`
    {
        let u_guard = model.displacement()?;
        let u = u_guard.as_slice();
        let scale = u.iter().fold(0.0 as Real, |m, x| m.max(x.abs()));
        for &(s, m) in &pairs {
            let gap = u[2 * s as usize + 1] - u[2 * m as usize + 1];
            if step == 0 {
                // the predictor moved the slider by Δt·v before any contact force
                assert!((gap + dt * 1.0).abs() <= 1e-12 * dt, "{gap:e}");
            } else {
                // from then on, no interpenetration beyond rounding
                assert!(gap >= -1e-12 * scale, "step {step}, pair ({s}, {m}): gap {gap:e}");
            }
        }
    }
}
assert!(contacts > 0);
# Ok::<(), akantu::core::Error>(())
```

The forces act through the residual of the explicit step, which first
predicts the displacement (`u + Δt v + ½ Δt² a`) and then assembles the
residual: the contact forces computed in a step close the gap of the
*next* predicted state. In the example the first step therefore leaves
the slider `Δt · v` inside the base; from the second step on no pair
interpenetrates beyond rounding.

Other contact kinds of the package: a surface against a rigid plane
(`NtnBaseContact::new_ntrf`, upstream `NTRFContact`, with
`set_reference_point` and `set_normal`; test
`independent_checks.rs::ntrf_impact_no_penetration_and_energy_decay`) and
the mode-II antisymmetric contact (`new_miiasym`, upstream
`MIIASYMContact`; test `independent_checks.rs::miiasym_relations_to_rigid_flat`).
The module `akantu_ntn_contact::boundary_conditions` has the package's
`ForceBasedDirichlet`, `SpringBc` and `InclinedFlatDirichlet`,
`boundary_functions` the `boundaryFix` and `integrateResidual` helpers,
`dump` the contact dumper, `manual_restart` and `sync_array` the restart
files, and `parameter_reader` upstream's `ParameterReader`.

## Friction laws

The first `friction` section of the input file selects the law, and its
option the regularisation:

```dat
friction rate_and_state:dieterich:ageing rubin_ampuero [
    mu_0     = 0.28
    a        = 0.005
    b        = 0.075
    v_star   = 1e-7
    phi_star = 0.00033
    d_c      = 5e-7
    v0       = 1e-7
    t_star   = 1e-6
    friction_state = 0.00025
]
```

Regularisations: `no_regularisation` (the default), `rubin_ampuero` (the
frictional pressure relaxes towards the contact pressure over the time
scale `t_star`) and `simplified_prakash_clifton` (the strength itself
relaxes); both take `t_star`. Registered laws and their parameters (with
`no_regularisation`; generated from the code):

<!-- BEGIN GENERATED: ntn-friction-laws -->
| Friction laws | Parameters |
|---|---|
| `rate_and_state:dieterich:ageing`, `rate_and_state:dieterich:slip`, `rate_and_state:dieterich:ageing_regularized`, `rate_and_state:revised_dieterich:ageing`, `rate_and_state:revised_dieterich:slip`, `rate_and_state:revised_dieterich:ageing_regularized`, `rate_and_state:regularized_dieterich:ageing`, `rate_and_state:regularized_dieterich:slip`, `rate_and_state:regularized_dieterich:ageing_regularized`, `rate_and_state:n_shaped:ageing`, `rate_and_state:n_shaped:slip`, `rate_and_state:n_shaped:ageing_regularized`, `rate_and_state:revised_n_shaped:ageing`, `rate_and_state:revised_n_shaped:slip`, `rate_and_state:revised_n_shaped:ageing_regularized`, `rate_and_state_n_shaped_ageing`, `rate_and_state_n_shaped_slip`, `rate_and_state_n_shaped_ageing_regularized` | `mu_0`, `a`, `b`, `v_star`, `phi_star`, `friction_state`, `d_c`, `v0` |
| `rate_and_state:flashheating` | `mu_0`, `a`, `b`, `v_star`, `phi_star`, `friction_state`, `d_c`, `v0`, `mu_w`, `v_w` |
| `coulomb` | `mu` |
| `linear_slip_weakening`, `linear_slip_weakening_no_healing` | `mu_s`, `mu_k`, `d_c` |
| `linear_cohesive` | `G_c`, `tau_c`, `tau_r` |
<!-- END GENERATED: ntn-friction-laws -->

The legacy keys `rate_and_state_n_shaped_<state>` are aliases of
`rate_and_state:n_shaped:<state>`. Strengths: Coulomb `μ p`; linear slip
weakening `μ = μ_k + (1 − s/d_c)(μ_s − μ_k)` on a sliding node until the
slip `s` reaches `d_c`, then `μ_k`, while a sticking node gets `μ_s` and
its slip restarts from 0 (healing); the `_no_healing` variant uses the
cumulative slip, which a sticking node keeps; linear cohesive `max(τ_c − (τ_c − τ_r)² s/(2G_c), τ_r)`;
rate and state `μ(V, θ) p` with `V` the slip velocity and `θ` the state
(`friction_state` is its initial value). The closed forms are checked by
`independent_checks.rs::{linear_slip_weakening_closed_form_with_and_without_healing,
linear_cohesive_closed_form, rate_and_state_steady_state_closed_forms,
rubin_ampuero_relaxation_closed_form}`.

### Rate-and-state coefficients and state laws

`rate_and_state:<coefficient>:<state>` combines one of five coefficients
with one of three state evolution laws (the code of upstream and of the
port, `akantu_ntn_contact::friction::{RsCoefficient, RsState}`):

| Coefficient | `μ(V, θ)` |
|---|---|
| `dieterich` | `μ₀ + a ln(V/V*) + b ln(θ/θ*)` |
| `revised_dieterich` | `μ₀ + a ln(1 + V/V*) + b μ₀ ln(θ/θ*)` |
| `regularized_dieterich` | `μ₀ + a ln(1 + V/V*) + b μ₀ ln(1 + θ/θ*)` |
| `n_shaped` | `(1 + b ln(1 + θ/θ*)) (μ₀/√(1 + (V/V*)²) + a ln(V/V*))` |
| `revised_n_shaped` | `(1 + b ln(1 + θ/θ*)) (μ₀/√(1 + (V*/V)²) + a ln(1 + V/V*))` |

| State law | `dθ/dt` |
|---|---|
| `ageing` | `1 − Vθ/d_c` |
| `slip` | `−(Vθ/d_c) ln(Vθ/d_c)` |
| `ageing_regularized` | `1 − (θ/d_c) √(V² + v0²)` |

(`V* = v_star`, `θ* = phi_star`, `μ₀ = mu_0`.) The upstream manual prints
the `n_shaped` coefficient with `√(1 + (V*/V)²)`; upstream's code, which
the port follows, uses `√(1 + (V/V*)²)` for `n_shaped` and `(V*/V)²` only
for `revised_n_shaped`. The `dieterich` and `n_shaped` coefficients are
singular at rest (`ln 0`); the three others are finite at `V = 0` (unit
test `friction::laws::tests::dieterich_steady_state_matches_the_test_data`).

The state is advanced by one explicit Euler step per time step,
`θ ← θ + Δt · dθ/dt`, on slipping nodes. For the ageing law this is
`θ ← θ (1 − VΔt/d_c) + Δt`: with `VΔt/d_c ≤ 1` the new state is at
least `Δt > 0` whatever the old one; with `VΔt/d_c > 1` a large enough
state becomes negative and the logarithms of the coefficient are no longer
defined. Upstream's manual only recommends small time steps (a safety
factor `O(0.01)`, citing Rezakhani 2020); the condition above is what the
explicit update needs:

```rust
use akantu_ntn_contact::friction::{RsArgs, RsState};

let mut args = RsArgs { d_c: 5e-7, time_step: 1e-6, state: 1e-2, ..Default::default() };
// V Δt / d_c = 2 > 1: one step makes the state negative
args.slip_velocity = 1.0;
assert!(args.state + RsState::Ageing.increment(&args) < 0.0);
// V Δt / d_c = 0.5 ≤ 1: θ (1 − 0.5) + Δt > 0
args.slip_velocity = 0.25;
assert!(args.state + RsState::Ageing.increment(&args) > 0.0);
# Ok::<(), akantu::core::Error>(())
```

What happens when the coefficient leaves its domain (UD-039, D-003
category B): upstream lets the non-finite value through, and a NaN
strength silently makes the node stick; the port stops with
`Error::Numerical` naming the node and the cause: a zero slip velocity
with a coefficient singular at rest, or a negative state with the value
of `VΔt/d_c` (tests `independent_checks.rs::ud039_singular_rate_and_state_at_rest`,
`ud039_error_names_the_negative_state_of_an_unstable_update`; oracle case
`adv_3d_ras_reversal`, where upstream's strengths turn NaN from step 96,
`oracle_ntn_adversarial.rs::oracle_adv_3d_ras_reversal_corrected_mode`).
The switch `rate_and_state_non_finite_propagates` reproduces upstream
(`oracle_adv_3d_ras_reversal_upstream_mode`). Three of upstream's own test
inputs (`revised_n_shaped` with ageing, slip and regularised ageing) are
disabled upstream and fail the same way (UD-405; characterisation test
`upstream_ntn_friction.rs::revised_n_shaped_cases_disabled_upstream_characterisation`).

## Differences from upstream

Corrected defaults with a compatibility switch (the fields of
`TasnUpstreamCompat`, passed to the contact; `TasnUpstreamCompat::upstream()`
sets them all):

| UD | Upstream behaviour | Port default | Switch |
|---|---|---|---|
| UD-039 | every regularisation option but `simplified_prakash_clifton` (including the default and misspellings) gives Rubin–Ampuero | the option is honoured, unknown names refused | `regularisation_fallback_rubin_ampuero` |
| UD-039 | a non-finite rate-and-state coefficient enters the strength | domain error (above) | `rate_and_state_non_finite_propagates` |
| UD-392 | pairing keeps searching after a match (a slave can get several masters) | first match | `pairing_keeps_searching` |
| UD-394 | reading a restart file leaves the values untouched | values restored, header checked | `restart_read_ignores_values` |
| UD-395 | the friction restart does not read `slip` | read | `friction_restart_skips_slip` |
| UD-397 | simplified Prakash–Clifton with `t_star = 0` divides by zero (NaN strength) | unregularised limit | `spc_zero_t_star_nan` |
| UD-398 | `boundaryFix` fails on existing groups and creates no missing one | creates the missing groups | `boundary_fix_inverted` |

```rust
use akantu_ntn_contact::TasnUpstreamCompat;

let corrected = TasnUpstreamCompat::default();
assert!(!corrected.regularisation_fallback_rubin_ampuero && !corrected.rate_and_state_non_finite_propagates);
let upstream = TasnUpstreamCompat::upstream();
assert!(upstream.regularisation_fallback_rubin_ampuero && upstream.pairing_keeps_searching);
# Ok::<(), akantu::core::Error>(())
```

Refused without a switch (category A, the upstream behaviour is a crash or
meaningless): 1-D pairing (UD-393), noise on the state of laws without
state (UD-396), restart arrays whose header differs (UD-402). Kept and
documented (category C): UD-399 (`initializeNTNFriction(contact, law, reg)`
is declared but not defined upstream; the port provides
`initialize_ntn_friction_with`), UD-400 (pressure assigned, friction
accumulated at a node shared by several pairs), UD-401 (`ParameterReader`
conversions), UD-403 (`InclinedFlatDirichlet` does not compile upstream;
the port implements its body), UD-404 (inverted position filters), UD-406
(impedances before the time step is set are infinite). Each entry and its
tests are in [`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md).
