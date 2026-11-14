// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_cohesive_model/
// cohesive_extrinsic_ig_tg/cohesive_extrinsic_ig_tg.cc (+ material.dat,
// triangle.geo → square.msh).

//! `cohesive_extrinsic_ig_tg` (EX-016): extrinsic cohesive fracture with
//! **intergranular and transgranular** properties. The `[-1, 1]²` square
//! (`square.msh`, the frozen Gmsh mesh of `triangle.geo`) has two grains,
//! `btop` and `bbottom`, both elastic; a facet between the grains takes the
//! strong `tg_cohesive` law (`σ_c = 100`), a facet inside a grain the weak
//! `ig_cohesive` law (`σ_c = 20`), chosen by a `MaterialCohesiveRulesSelector`
//! chained, as upstream, with a `MeshDataMaterialSelector` on
//! `physical_names` (the bulk laws) and the model's default selector; the
//! input file restricts insertion to facets between the two zones
//! (`cohesive_zones = [btop, bbottom]`). The sides are blocked in their
//! normal direction (`FlagOnly`) and moved at the constant speed
//! `±0.1` by a user Dirichlet functor that also sets the velocity of its
//! nodes; the initial velocity is `0.1 x`. 1000 explicit steps
//! (`Δt = 0.05 Δt_crit`) with the insertion check before each, ParaView
//! dumps (with the material index) twice at the start and every 10 steps
//! (`paraview/extrinsic_*`).
//!
//! Upstream's functor writes `model.getVelocity()(node, axis)` from inside
//! `operator()`; a Rust [`DirichletFunctor`] has no access to the other
//! fields of the model it is applied to, so [`Velocity`] records the
//! velocities and [`apply_velocity`] writes them after the application —
//! a work-around for the functor API (a functor context exposing the other
//! nodal fields would allow upstream's form). The
//! state is the same: the boundary-condition application never reads the
//! velocity, each group is written before the next one is applied, and
//! nothing reads the velocity in between.
//!
//! In upstream's run the write changes no value: the side DOFs it writes
//! are blocked (`FlagOnly`), the Newmark predictor/corrector skip blocked
//! DOFs, and the initial condition already gives them `±0.1`, bitwise the
//! functor's value. [`Options::initial_velocity_rate`]
//! (the oracle variant `cohesive_extrinsic_ig_tg_vel`, initial velocity
//! `0.05 x`) makes the write observable: the side nodes start at `±0.05`
//! and reach `±0.1` (and the kinetic energy its upstream value) only
//! through the functor.
//!
//! ```text
//! cargo run -p akantu --release --example cohesive_extrinsic_ig_tg [-- --output-dir <dir>]
//! ```

#[path = "../cohesive_common/mod.rs"]
pub mod cohesive_common;
#[path = "../common/mod.rs"]
pub mod common;

use std::cell::RefCell;
use std::process::ExitCode;

use akantu::cohesive::selector::{MaterialCohesiveRules, MaterialCohesiveRulesSelector};
use akantu::cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::{Idx, Real, Result};
use akantu::mesh::SharedMesh;
use akantu::model::boundary_condition::{DirichletFunctor, FlagOnly};
use akantu::model::constitutive_law::{ElementDataSelector, LawIndexSelector, SelectorChain};
use cohesive_common::{EnergyRecord, Transcript};
use common::{Example, cout};

/// Spatial dimension of the example.
pub const DIM: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModelCohesive,
    /// The mesh (with the inserted cohesive elements).
    pub mesh: SharedMesh,
    /// Time step (`0.05 Δt_crit`).
    pub time_step: Real,
    /// Cohesive elements inserted by the check of each step.
    pub inserted: Vec<usize>,
    /// Per-step energies (only with [`Options::record_energies`]).
    pub energies: Vec<EnergyRecord>,
    /// Standard output.
    pub stdout: Vec<String>,
}

/// Run options beyond the common ones.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Record the energies after every step (the quantities of the oracle
    /// variants `cohesive_extrinsic_ig_tg_full` and
    /// `cohesive_extrinsic_ig_tg_vel`; read-only queries).
    pub record_energies: bool,
    /// Rate `r` of the initial velocity `v = r x` (`None`: upstream's
    /// loading rate `0.1`, which is also the speed the functor imposes on
    /// the sides). The oracle variant `cohesive_extrinsic_ig_tg_vel` uses
    /// `0.05`, so that the functor's velocity write changes the state.
    pub initial_velocity_rate: Option<Real>,
}

/// Default configuration (upstream directory and input file; the frozen
/// `square.msh`).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "cohesive_extrinsic_ig_tg",
        "solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg",
        "material.dat",
    )
}

/// Upstream's `Velocity` Dirichlet functor: `u(axis) += sign(x_axis) v Δt`
/// and `v(axis) = sign(x_axis) v` (`sign` from `std::signbit` of the
/// node's coordinate). The velocities are recorded and written by
/// [`apply_velocity`].
#[derive(Debug)]
pub struct Velocity {
    /// Axis.
    pub axis: usize,
    /// Speed.
    pub vel: Real,
    /// Displacement increment per step (`vel · Δt` at construction).
    pub disp: Real,
    /// `(node, velocity)` of the last application.
    pub velocities: RefCell<Vec<(Idx, Real)>>,
}

impl Velocity {
    /// Upstream `Velocity(model, vel, axis)`: the increment uses the time
    /// step at construction.
    #[must_use]
    pub fn new(time_step: Real, vel: Real, axis: usize) -> Self {
        Self {
            axis,
            vel,
            disp: vel * time_step,
            velocities: RefCell::new(Vec::new()),
        }
    }
}

impl DirichletFunctor for Velocity {
    fn apply(
        &self,
        node: Idx,
        _flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()> {
        let sign = if coord[self.axis].is_sign_negative() {
            -1.
        } else {
            1.
        };
        primal[self.axis] += sign * self.disp;
        self.velocities.borrow_mut().push((node, sign * self.vel));
        Ok(())
    }
}

/// Upstream `model.applyBC(functor, group)` with a [`Velocity`] functor.
///
/// # Errors
/// Unknown group, model errors.
pub fn apply_velocity(
    model: &mut SolidMechanicsModelCohesive,
    functor: &Velocity,
    group: &str,
) -> Result<()> {
    functor.velocities.borrow_mut().clear();
    model.smm.apply_dirichlet_bc(functor, group)?;
    let mut velocity = model.smm.velocity_mut()?;
    let v_nodal = velocity.as_mut_slice();
    for &(node, v) in functor.velocities.borrow().iter() {
        v_nodal[akantu::core::try_to_usize(node)? * DIM + functor.axis] = v;
    }
    Ok(())
}

/// Upstream's selector chain: `MaterialCohesiveRulesSelector(rules)` →
/// `MeshDataMaterialSelector<std::string>("physical_names")` → the model's
/// current selector (`DefaultMaterialCohesiveSelector` →
/// `DefaultMaterialSelector`).
#[must_use]
pub fn selector_chain(model: &SolidMechanicsModelCohesive) -> SelectorChain {
    let rule = |a: &str, b: &str, m: &str| ((a.to_owned(), b.to_owned()), m.to_owned());
    let rules: MaterialCohesiveRules = [
        rule("btop", "bbottom", "tg_cohesive"),
        rule("btop", "btop", "ig_cohesive"),
        rule("bbottom", "bbottom", "ig_cohesive"),
    ]
    .into_iter()
    .collect();
    let cohesive_material_selector = MaterialCohesiveRulesSelector {
        rules,
        mesh_data_id: "physical_names".into(),
        default: model.default_cohesive_selector(),
    };
    let bulk_material_selector = ElementDataSelector::new("physical_names");
    SelectorChain {
        selectors: vec![
            Box::new(cohesive_material_selector),
            Box::new(bulk_material_selector),
            Box::new(model.default_cohesive_selector()),
            Box::new(LawIndexSelector),
        ],
        fallback_value: 0,
    }
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, Options::default())
}

/// [`run`] with `options`.
///
/// # Errors
/// See [`run`].
pub fn run_with(ex: &Example, options: Options) -> Result<Outcome> {
    ex.create_output_dir()?;
    let mut out = Transcript::new(ex.verbose);
    let max_steps = ex.steps_or(1000);

    let mesh = SharedMesh::new(ex.read_mesh(DIM, "square.msh")?);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        ex.parser()?,
    )?;
    if ex.upstream_compat {
        // D-003: every upstream cohesive defect (UD-217, UD-040, …)
        model.ext.compat = CohesiveUpstreamCompat::upstream();
    }

    // model initialization
    let chain = selector_chain(&model);
    model.set_material_selector_chain(chain);
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass), true)?;

    let time_step = model.stable_time_step()? * 0.05;
    model.set_time_step(time_step, None)?;
    out.line(format!("Time step: {}", cout(time_step)));

    model.smm.assemble_mass_lumped()?;

    let flag = |axis| FlagOnly { axis };
    model
        .smm
        .apply_dirichlet_bc(&flag(SpatialDirection::Y), "top")?;
    model
        .smm
        .apply_dirichlet_bc(&flag(SpatialDirection::Y), "bottom")?;
    model
        .smm
        .apply_dirichlet_bc(&flag(SpatialDirection::X), "left")?;
    model
        .smm
        .apply_dirichlet_bc(&flag(SpatialDirection::X), "right")?;

    model.smm.set_directory(&ex.paraview_dir())?;
    model.smm.set_base_name("extrinsic")?;
    model.smm.add_dump_field_vector("displacement")?;
    model.smm.add_dump_field("velocity")?;
    model.smm.add_dump_field("acceleration")?;
    model.smm.add_dump_field("internal_force")?;
    model.smm.add_dump_field("stress")?;
    model.smm.add_dump_field("grad_u")?;
    model.smm.add_dump_field("material_index")?;
    if ex.dump {
        model.smm.dump(None, None)?;
    }

    // initial conditions
    let loading_rate = 0.1;
    // bar_height  = 2
    let vi = loading_rate * 2. * 0.5;
    let initial_rate = options.initial_velocity_rate.unwrap_or(loading_rate);
    {
        let position = mesh.read()?.nodes().as_slice().to_vec();
        let mut velocity = model.smm.velocity_mut()?;
        for (pos, vel) in position.iter().zip(velocity.as_mut_slice().iter_mut()) {
            *vel = initial_rate * pos;
        }
    }

    if ex.dump {
        model.smm.dump(None, None)?;
    }

    let vely = Velocity::new(model.smm.time_step(None)?, vi, 1);
    let velx = Velocity::new(model.smm.time_step(None)?, vi, 0);

    // Main loop
    let mut inserted = Vec::with_capacity(max_steps);
    let mut energies = Vec::new();
    for s in 1..=max_steps {
        apply_velocity(&mut model, &vely, "top")?;
        apply_velocity(&mut model, &vely, "bottom")?;

        apply_velocity(&mut model, &velx, "left")?;
        apply_velocity(&mut model, &velx, "right")?;

        inserted.push(model.check_cohesive_stress()?);

        model.solve_step(None)?;

        if options.record_energies {
            energies.push(EnergyRecord::of(&mut model, s)?);
        }
        if s % 10 == 0 {
            if ex.dump {
                model.smm.dump(None, None)?;
            }
            out.line(format!("passing step {s}/{max_steps}"));
        }
    }

    Ok(Outcome {
        model,
        mesh,
        time_step,
        inserted,
        energies,
        stdout: out.lines,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
