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

![Two meshes with matching nodes along their interface; in the close-up,
the normals of the two surfaces and the normal and frictional forces on a
pair of matching nodes, with the area of influence A of the
pair](../figures/upstream/node_to_node.png)
*Figure: the interface between a bottom body ($-$, black) and a top body
($+$, brown) with matching nodes; the normal forces
$\boldsymbol{f}_{N,\pm}$ and the frictional forces
$\boldsymbol{f}_{T,\pm}$ act on each pair, whose area of influence is $A$
(figure of the Akantu manual).*

The contact forces enter the explicit central-difference scheme through
the residual. For a node of the interface, the acceleration of step $i$
is

$$
\ddot{\boldsymbol{u}}^{i} = \boldsymbol{M}^{-1} \left( \boldsymbol{f}_\text{ext}^{i} - \boldsymbol{f}_\text{int}^{i} + \boldsymbol{f}_N^{i} + \boldsymbol{f}_T^{i} \right),
$$

with $\boldsymbol{f}_N$ and $\boldsymbol{f}_T$ the contact and the
frictional forces, zero on the nodes that are not on the interface.
Below, $[\![\cdot]\!]$ is the jump of a field between a node of the top
body and its matching node of the bottom body, the subscript $N$ its
normal and $T$ its tangential part.

### Contact forces

The pair must not overlap at the next step,
$[\![\boldsymbol{x}]\!]_N^{i+1} \geq 0$. Expanding the next position with
the central-difference update, replacing the velocity by the predicted
velocity $\dot{\boldsymbol{u}}^P$ plus the acceleration increment, and the
acceleration by the residual above, gives

$$
[\![\boldsymbol{x}]\!]_N + [\![\boldsymbol{u}]\!]_N^{i} + \Delta t \, [\![\dot{\boldsymbol{u}}^P]\!]_N^{i} - \frac{\Delta t^2}{2} [\![\ddot{\boldsymbol{u}}]\!]_N^{i-1} + \Delta t^2 [\![\ddot{\boldsymbol{u}}^\text{free}]\!]_N^{i} - 2 \Delta t \, Z^{-1} T_N^{i} \geq 0,
$$

where $[\![\ddot{\boldsymbol{u}}^\text{free}]\!]^{i} = \boldsymbol{M}^{-1} \big( [\![\boldsymbol{f}_\text{ext}]\!]^{i} - [\![\boldsymbol{f}_\text{int}]\!]^{i} \big)$
is the acceleration of the bodies without the contact forces, the contact
forces are written $\boldsymbol{f}_N = A \, \boldsymbol{T}_N$ with the
areas of the contact nodes, the tractions on the two nodes of a pair are
equal and opposite ($[\![\boldsymbol{T}]\!]_N = -2 \boldsymbol{T}_N$), and
the *impedance* $Z$ of the pair is

$$
Z^{-1} = \frac{\Delta t}{2} \left( \frac{A_+}{m_+} + \frac{A_-}{m_-} \right).
$$

All terms but the last one are independent of the contact; together they
are the *free interpenetration* $I^\text{free}$. If
$I^\text{free} \geq 0$ no contact traction is needed; otherwise the
traction that closes the gap exactly is

$$
T_N^{i} = \frac{Z \, I^\text{free}}{2 \Delta t} .
$$

In the port's notation: for every pair (slave node $s$ on one surface,
master node $m$ on the other) with the normal $\boldsymbol{n}$ of the
slave surface, lumped boundary areas $A_s$, $A_m$ and lumped masses
$m_s$, $m_m$, the impedance is

$$
Z = \frac{2}{\Delta t \left( \dfrac{A_m}{m_m} + \dfrac{A_s}{m_s} \right)}
$$

and, at each residual assembly, the free interpenetration of the pair

$$
I = [x]_N + \Delta t \, [v]_N + \Delta t^2 [a^\text{free}]_N - \tfrac12 \Delta t^2 [a^\text{old}]_N
$$

(relative normal position, velocity, acceleration of the residual without
contact forces, previous acceleration) decides the contact: the pair is in
contact when $I \leq 0$ and then gets the normal traction
$T_N = Z I / (2 \Delta t)$, applied as $-A_s T_N$ on the slave and
$+A_m T_N$ on the master. Against a rigid plane (`new_ntrf`) the impedance
has the slave term only, $Z = 2 / (\Delta t \, A_s / m_s)$, and the
mode-II antisymmetric contact (`new_miiasym`) takes half of that
(`NtnBaseContact::update_impedance`).

### Friction forces

The tangential condition is that the friction holds the pair together, so
that the relative tangential velocity at the mid-step vanishes,
$[\![\dot{\boldsymbol{u}}]\!]_T^{i+1/2} = 0$, with
$\dot{\boldsymbol{u}}^{i+1/2} = \dot{\boldsymbol{u}}^{i} + \tfrac{\Delta t}{2} \ddot{\boldsymbol{u}}^{i}$.
The same substitutions give the traction that keeps the pair stuck,

$$
\boldsymbol{T}_T^{i} = \frac{Z \, \boldsymbol{R}^\text{free}}{2},
\qquad
\boldsymbol{R}^\text{free} = [\![\dot{\boldsymbol{u}}^P]\!]_T^{i} - \frac{\Delta t}{2} [\![\ddot{\boldsymbol{u}}]\!]_T^{i-1} + \Delta t \, [\![\ddot{\boldsymbol{u}}^\text{free}]\!]_T^{i},
$$

that is, in the port's notation,
$\boldsymbol{T}_T = \tfrac12 Z \big( \boldsymbol{v}_t + \Delta t \, \boldsymbol{a}_t^\text{free} - \tfrac12 \Delta t \, \boldsymbol{a}_t^\text{old} \big)$.
The interface can carry this traction only up to its frictional strength
$S$, given by the friction law (below): with

$$
\boldsymbol{T}_T \leftarrow
\begin{cases}
\boldsymbol{T}_T, & \lVert \boldsymbol{T}_T \rVert \leq S \quad \text{(stick)},\\
\dfrac{S}{\lVert \boldsymbol{T}_T \rVert} \, \boldsymbol{T}_T, & \lVert \boldsymbol{T}_T \rVert > S \quad \text{(slip)},
\end{cases}
$$

a pair whose stick traction exceeds the strength slides, and its traction
is scaled to $S$. These are the expressions of the
upstream manual (its $Z^{-1} = \frac{\Delta t}{2} \big( \boldsymbol{M}_+^{-1} A_+ + \boldsymbol{M}_-^{-1} A_- \big)$); the port's code is
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
predicts the displacement
($\boldsymbol{u} + \Delta t \, \dot{\boldsymbol{u}} + \tfrac12 \Delta t^2 \ddot{\boldsymbol{u}}$)
and then assembles the
residual: the contact forces computed in a step close the gap of the
*next* predicted state. In the example the first step therefore leaves
the slider $\Delta t \, v$ inside the base; from the second step on no pair
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
relaxes); both take `t_star`. With $\alpha = \Delta t / t^*$, at every
step, Rubin–Ampuero updates the frictional pressure $\tilde p$ of a pair
in contact from the norm of its contact pressure $p$, and simplified
Prakash–Clifton updates the strength $S$ from the strength $S^\text{law}$
that the friction law gives:

$$
\tilde p \leftarrow \frac{\tilde p + \alpha \, p}{1 + \alpha} \quad (\texttt{rubin\_ampuero}),
\qquad
S \leftarrow \frac{S + \alpha \, S^\text{law}}{1 + \alpha} \quad (\texttt{simplified\_prakash\_clifton}).
$$

With $t^* = 0$ (to the package's tolerance) both take the unregularised
value, $\tilde p = p$ and $S = S^\text{law}$ (UD-397 for the latter);
a pair out of contact has $\tilde p = 0$. Registered laws and their parameters (with
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
`rate_and_state:n_shaped:<state>`. Strengths, with $p$ the frictional
contact pressure (the norm of the contact pressure of a pair in contact,
or its regularised value below) and $S = 0$ on a pair that is not in
contact:

* Coulomb (`coulomb`, $\mu$ = `mu`): $S = \mu \, p$;
* linear slip weakening (`linear_slip_weakening`, $\mu_s$ = `mu_s`,
  $\mu_k$ = `mu_k`, $d_c$ = `d_c`): $S = \mu \, p$ with, on a sliding node
  of slip $s$,
  $$
  \mu =
  \begin{cases}
  \mu_k + \left( 1 - \dfrac{s}{d_c} \right) (\mu_s - \mu_k), & s < d_c,\\
  \mu_k, & s \geq d_c,
  \end{cases}
  $$
  while a sticking node gets $\mu_s$ and its slip restarts from 0
  (healing); the `_no_healing` variant uses the cumulative slip, which a
  sticking node keeps, and evaluates the formula on sticking nodes too;
* linear cohesive (`linear_cohesive`, $G_c$ = `G_c`, $\tau_c$ = `tau_c`,
  $\tau_r$ = `tau_r`), a strength that does not depend on the pressure,
  with $s$ the cumulative slip:
  $$
  S = \max\!\left( \tau_c - \frac{(\tau_c - \tau_r)^2}{2 G_c} \, s, \; \tau_r \right)
  $$
  ($S = \tau_r$ when $G_c = 0$);
* rate and state: $S = \mu(V, \theta) \, p$ with $V$ the norm of the
  slip velocity and $\theta$ the state (`friction_state` is its initial
  value), below.

The closed forms are checked by
`independent_checks.rs::{linear_slip_weakening_closed_form_with_and_without_healing,
linear_cohesive_closed_form, rate_and_state_steady_state_closed_forms,
rubin_ampuero_relaxation_closed_form}`.

### Rate-and-state coefficients and state laws

`rate_and_state:<coefficient>:<state>` combines one of five coefficients
with one of three state evolution laws (the code of upstream and of the
port, `akantu_ntn_contact::friction::{RsCoefficient, RsState}`):

| Coefficient | $\mu(V, \theta)$ |
|---|---|
| `dieterich` | $\mu_0 + a \ln(V/V^*) + b \ln(\theta/\theta^*)$ |
| `revised_dieterich` | $\mu_0 + a \ln(1 + V/V^*) + b \mu_0 \ln(\theta/\theta^*)$ |
| `regularized_dieterich` | $\mu_0 + a \ln(1 + V/V^*) + b \mu_0 \ln(1 + \theta/\theta^*)$ |
| `n_shaped` | $\big(1 + b \ln(1 + \theta/\theta^*)\big) \left( \dfrac{\mu_0}{\sqrt{1 + (V/V^*)^2}} + a \ln(V/V^*) \right)$ |
| `revised_n_shaped` | $\big(1 + b \ln(1 + \theta/\theta^*)\big) \left( \dfrac{\mu_0}{\sqrt{1 + (V^*/V)^2}} + a \ln(1 + V/V^*) \right)$ |

| State law | $\mathrm{d}\theta/\mathrm{d}t$ |
|---|---|
| `ageing` | $1 - V\theta/d_c$ |
| `slip` | $-(V\theta/d_c) \ln(V\theta/d_c)$ |
| `ageing_regularized` | $1 - (\theta/d_c) \sqrt{V^2 + v_0^2}$ |

($V^*$ = `v_star`, $\theta^*$ = `phi_star`, $\mu_0$ = `mu_0`, $v_0$ = `v0`.)
The upstream manual prints
the `n_shaped` coefficient with $\sqrt{1 + (V^*/V)^2}$; upstream's code, which
the port follows, uses $\sqrt{1 + (V/V^*)^2}$ for `n_shaped` and $(V^*/V)^2$ only
for `revised_n_shaped`. The `dieterich` and `n_shaped` coefficients are
singular at rest ($\ln 0$); the three others are finite at $V = 0$ (unit
test `friction::laws::tests::dieterich_steady_state_matches_the_test_data`).

The flash-heating law `rate_and_state:flashheating` (parameters $\mu_w$ =
`mu_w`, $v_w$ = `v_w` in addition) weakens the Dieterich coefficient
towards $\mu_w$ at high slip velocity. A sticking node gets
$\mu = \mu_0$; on a slipping node with $V < V^*$ the state is set to
$\theta = d_c / V^*$ and $\mu = \mu_0 + a \ln(V^*/V^*) + b \ln(\theta/\theta^*)$
(the first logarithm is kept as upstream writes it); otherwise the state
takes one ageing step and

$$
\theta \leftarrow \theta + \left( 1 - \frac{\theta V}{d_c} \right) \Delta t,
\qquad
\mu_\text{rs} = \mu_0 + a \ln(V/V^*) + b \ln(\theta/\theta^*),
\qquad
\mu = \mu_w + \frac{\mu_\text{rs} - \mu_w}{1 + d_c / (\theta \, v_w)} .
$$

The state is advanced by one explicit Euler step per time step,
$\theta \leftarrow \theta + \Delta t \, \mathrm{d}\theta/\mathrm{d}t$, on slipping nodes. For the ageing law this is
$\theta \leftarrow \theta (1 - V\Delta t/d_c) + \Delta t$: with $V\Delta t/d_c \leq 1$ the new state is at
least $\Delta t > 0$ whatever the old one; with $V\Delta t/d_c > 1$ a large enough
state becomes negative and the logarithms of the coefficient are no longer
defined. Upstream's manual only recommends small time steps (a safety
factor $\alpha_F = \mathcal{O}(0.01)$, citing Rezakhani et al., 2020); the condition above is what the
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
of $V\Delta t/d_c$ (tests `independent_checks.rs::ud039_singular_rate_and_state_at_rest`,
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

## References

* Rezakhani, R., Barras, F., Brun, M. and Molinari, J.-F. (2020). Finite
  element modeling of dynamic frictional rupture with rate and state
  friction. *Journal of the Mechanics and Physics of Solids*, 141, 103967.
