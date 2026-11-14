// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/scalability_test/
// cohesive_extrinsic.cc (+ material-elastic.dat, material-cohesive.dat,
// cube.msh).

//! `scalability_test` (EX-018): upstream's weak/strong-scaling benchmark
//! (JOSS paper). A unit cube of linear tetrahedra (upstream's committed
//! `cube.msh`, copied next to this file), elastic with finite deformation
//! and an extrinsic `cohesive_linear` law, is loaded by a hydrostatic
//! compression and a shear traction (`FromHigherDim` on the faces); a
//! static linear solve (solver `static` of the input file) gives the
//! initial state, then explicit dynamics (`Δt = 0.05 Δt_crit`) where the
//! shear grows by `inc_shear` every 100 steps. Everything is set by the
//! `user` section of the input file: `material-elastic.dat` (default, no
//! insertion: the upstream ctest) or `material-cohesive.dat`
//! (`--aka_input_file`, insertion checked every step); `max_steps`,
//! `output_energy` (`energies.csv`), `output_paraview` (dumps every 100
//! steps). Timings of every phase go to the standard error as upstream's
//! CSV (`Chrono`). `energies.csv` is written line by line, flushed as
//! upstream's `std::endl`.
//!
//! **Dissipated energy (UD-803).** `Ed` (`energies.csv`, and the final
//! `Cohesive info: dissipated energy` line) is upstream's
//! `getEnergy("dissipated")`, a trapezoidal bookkeeping of the cohesive
//! work, reproduced bitwise. It is **not a physical dissipation** for this
//! benchmark: the explicit time step does not resolve the cohesive
//! softening (`δc,eff` ≈ 0.2 µm), about half of the cohesive points go from
//! their insertion traction to fully broken within one step, and the
//! trapezoid then books `½ t_ins·Δδ`, which has no sign. With
//! `material-cohesive.dat` upstream ends with `Ed = −15.67` J (the port's
//! default, UD-217 corrected: +19.72 J) while the law dissipates
//! `½ σc,eff min(δmax, δc,eff)` per point (`Gc = ½ σc δc` per broken point):
//! +260.06 J (+237.53 J).
//!
//! Static solve then explicit solver: the explicit solver is created after
//! the static solve (`init_new_solver`), whose current-position cache the
//! port keeps (UD-352, D-064), so the stable time step is computed on the
//! deformed mesh. `--upstream-compat` keeps that rule too (it does not set
//! `SmmUpstreamCompat::init_solver_resets_current_position`): upstream's own
//! run never meets the defect (no reader refreshes the position between its
//! static solve and `initNewSolver`), while the port's cohesive model still
//! refreshes it during the static solve; with the switch the compat run
//! would compute the time step on the undeformed mesh and leave the oracle.
//! The switch can be set here once the cohesive model refreshes the position
//! only when a cohesive law holds elements, as upstream.
//!
//! Ranks: upstream reads the mesh on rank 0 and calls `mesh.distribute()`.
//! The port does the same on its world communicator (MPI with the `mpi`
//! feature) on several ranks, where the cohesive model runs on the
//! distributed mesh (the example's multi-rank runs are not
//! compared with the oracle, EX-049); on one rank it keeps the mesh serial
//! (upstream's one-rank distribution changes no value, and a distributed
//! mesh would only add collectives with nothing to synchronise).
//!
//! Dumps (`output_paraview = true`, off in both input files): as upstream,
//! the solid model's default dumper (base name `extrinsic`: displacement,
//! forces, stress, blocked DOFs, `grad_u`, then velocity and acceleration)
//! and the cohesive model's dumper `cohesive elements` (base name the model
//! id `solid_mechanics_model_cohesive`: displacement and the cohesive
//! tractions, then velocity), both written before the static solve, before
//! the first explicit step and every 100 steps (`paraview/`). Until
//! The port refused the second dumper as not ported;
//! the library has had it since COH-013 (`akantu::cohesive::COHESIVE_DUMPER`).
//! While no cohesive element exists, the `cohesive elements` files hold
//! points and no cell: their empty arrays are valid base64 by default,
//! upstream's invalid stream with `--upstream-compat` (UD-114, the
//! dumpers' compat layout).
//!
//! ```text
//! cargo run -p akantu --release --example scalability_test [-- --aka_input_file crates/akantu/examples/scalability_test/material-cohesive.dat]
//! ```

#[path = "../cohesive_common/mod.rs"]
pub mod cohesive_common;
#[path = "../common/mod.rs"]
pub mod common;

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::Instant;

use akantu::cohesive::{COHESIVE_DUMPER, CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu::comm::{Communicator, ReduceOp};
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::tensor::MatrixX;
use akantu::core::{Error, Real, Result};
use akantu::mesh::distribution::distribute_with;
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::boundary_condition::{FixedValue, FromHigherDim};
use akantu::parser::SearchScope;
use cohesive_common::{LineFile, Transcript, nb_cohesive_elements};
use common::{Example, cout};

/// Spatial dimension of the example.
pub const DIM: usize = 3;

/// The `user` section of the input file.
#[derive(Clone, Debug, PartialEq)]
pub struct UserParameters {
    /// Hydrostatic stress of the static load (`compression`).
    pub compression: Real,
    /// Initial shear stress (`shear`).
    pub shear: Real,
    /// Shear increment every 100 steps (`inc_shear`).
    pub inc_shear: Real,
    /// Write `energies.csv` (`output_energy`, default true).
    pub output_energy: bool,
    /// Write ParaView dumps (`output_paraview`, default true).
    pub output_paraview: bool,
    /// Check the insertion of cohesive elements (`cohesive_insertion`,
    /// default true).
    pub cohesive_insertion: bool,
    /// Number of explicit steps (`max_steps`).
    pub max_steps: usize,
    /// Mesh file (`mesh`).
    pub mesh: String,
}

/// Energies after a step: `[Ed, Ep, Ek, Ew, Et]` (upstream `energies.csv`
/// columns `ed, ep, ek, ew, et`; `Ew` accumulated, `Et = Ed + Ep + Ek −
/// Ew`).
pub type Energies = [Real; 5];

/// Result of a run on one rank.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModelCohesive,
    /// The mesh.
    pub mesh: SharedMesh,
    /// The user parameters read.
    pub user: UserParameters,
    /// Time step of the explicit phase (`0.05 Δt_crit` after the static
    /// solve).
    pub time_step: Real,
    /// Energies of step 0 (after the static solve) and of every step (with
    /// `output_energy`).
    pub energies: Vec<Energies>,
    /// Cohesive elements inserted by the check of each step (with
    /// `cohesive_insertion`).
    pub inserted: Vec<usize>,
    /// Final number of cohesive elements (summed over ranks).
    pub nb_cohesive_elements: usize,
    /// Final dissipated energy.
    pub dissipated: Real,
    /// Global DOFs at the start and at the end of the explicit phase.
    pub nb_dofs: (usize, usize),
    /// Standard output (rank 0).
    pub stdout: Vec<String>,
    /// Upstream `printself_csv` of the chronometer, written to the standard
    /// error at the end of the run on rank 0 (`psize` and, per phase, the
    /// seconds and the repetitions; read by `plot.py`, EX-049).
    pub timings_csv: String,
}

/// Default configuration (upstream directory and default input file; the
/// mesh is upstream's committed `cube.msh`, copied next to this file).
#[must_use]
pub fn example() -> Example {
    let mut ex = Example::new(
        "scalability_test",
        "scalability_test",
        "material-elastic.dat",
    );
    ex.mesh_dir = ex.source_dir.clone();
    ex
}

/// The `user` section of `parser` (upstream `getUserParser()`).
///
/// # Errors
/// A missing section or parameter, a conversion error.
pub fn user_parameters(parser: &akantu::parser::Parser) -> Result<UserParameters> {
    let usersect = parser.user_section()?;
    let cur = SearchScope::Current;
    let max_steps: u64 = usersect.get_parameter_value("max_steps", cur)?;
    Ok(UserParameters {
        compression: usersect.get_parameter_value("compression", cur)?,
        shear: usersect.get_parameter_value("shear", cur)?,
        inc_shear: usersect.get_parameter_value("inc_shear", cur)?,
        output_energy: usersect.get_parameter_or("output_energy", true, cur)?,
        output_paraview: usersect.get_parameter_or("output_paraview", true, cur)?,
        cohesive_insertion: usersect.get_parameter_or("cohesive_insertion", true, cur)?,
        max_steps: usize::try_from(max_steps)
            .map_err(|_| Error::invalid(format!("max_steps = {max_steps}")))?,
        mesh: usersect.get_parameter_value("mesh", cur)?,
    })
}

/// Upstream's `Chrono`: accumulated wall-clock time per phase.
#[derive(Debug)]
struct Chrono {
    prank: i32,
    psize: usize,
    start: Instant,
    /// phase → (seconds, repetitions); `std::map` order.
    measures: BTreeMap<String, (f64, u32)>,
}

impl Chrono {
    fn new(prank: i32, psize: usize) -> Self {
        Self {
            prank,
            psize,
            start: Instant::now(),
            measures: BTreeMap::new(),
        }
    }

    fn start(&mut self) {
        self.start = Instant::now();
    }

    /// Upstream `store_time(type)`: the first measure of a phase is
    /// printed on rank 0.
    fn store_time(&mut self, ty: &str, out: &mut Transcript) {
        let elapsed = self.start.elapsed().as_secs_f64();
        match self.measures.get_mut(ty) {
            Some(m) => {
                m.0 += elapsed;
                m.1 += 1;
            }
            None => {
                self.measures.insert(ty.to_owned(), (elapsed, 1));
                if self.prank == 0 {
                    out.line(format!(
                        "Passing the first {ty} chrono! [{}]",
                        cout(elapsed)
                    ));
                }
            }
        }
        self.start = Instant::now();
    }

    /// Upstream `printself_csv`.
    fn csv(&self) -> String {
        let mut header = String::from("\"psize\"");
        let mut values = self.psize.to_string();
        for (name, (secs, n)) in &self.measures {
            header.push_str(&format!(", \"{name}\", \"{name} nb_rep\""));
            values.push_str(&format!(", {}, {n}", cout(*secs)));
        }
        format!("{header}\n{values}\n")
    }
}

/// `[[c, 0, 0], [0, c, 0], [0, 0, c]]`.
fn compression_matrix(c: Real) -> Result<MatrixX> {
    MatrixX::from_col_major(3, 3, vec![c, 0., 0., 0., c, 0., 0., 0., c])
}

/// `[[0, 0, s], [0, 0, 0], [s, 0, 0]]` (symmetric).
fn shear_matrix(s: Real) -> Result<MatrixX> {
    MatrixX::from_col_major(3, 3, vec![0., 0., s, 0., 0., 0., s, 0., 0.])
}

/// Upstream `model.applyBC(BC::Neumann::FromHigherDim(m), group)` on every
/// group of `groups`.
fn apply_traction(
    model: &mut SolidMechanicsModelCohesive,
    m: &MatrixX,
    groups: &[&str],
) -> Result<()> {
    let functor = FromHigherDim::new(m.clone());
    for g in groups {
        model.smm.apply_neumann_bc(&functor, g)?;
    }
    Ok(())
}

/// Upstream `std::cout << bool` (no `boolalpha`).
fn bool01(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// The upstream `main` after `initialize`, on the ranks of `comm`.
///
/// # Errors
/// Any error of the parser, the mesh, the model or the outputs.
pub fn run_on(comm: Communicator, ex: &Example) -> Result<Outcome> {
    let parser = ex.parser()?;
    let prank = comm.rank();
    let psize = comm.size();
    let mut out = Transcript::new(ex.verbose && prank == 0);
    let mut chrono = Chrono::new(prank, psize);

    let user = user_parameters(&parser)?;
    let max_steps = ex.steps_or(user.max_steps);

    if prank == 0 {
        ex.create_output_dir()?;
        out.line("Paramters:");
        out.line(format!(" - output_energy: {}", bool01(user.output_energy)));
        out.line(format!(
            " - output_paraview: {}",
            bool01(user.output_paraview)
        ));
        out.line(format!(
            " - cohesive_insertion: {}",
            bool01(user.cohesive_insertion)
        ));
        out.line(format!(" - max_steps: {}", user.max_steps));
        out.line(format!(" - mesh_filename: {}", user.mesh));
    }
    chrono.start();

    let start_time = Instant::now();

    let mut mesh = Mesh::with_communicator(DIM, "mesh", comm.clone())?;
    if prank == 0 {
        akantu::io::read_mesh(
            &mut mesh,
            &ex.mesh_path(&user.mesh),
            akantu::core::enums::MeshIoType::Auto,
        )?;
        chrono.store_time("read_mesh", &mut out);
    }
    // upstream `mesh.distribute()`: Scotch partition on rank 0 and one part
    // per rank (no mesh consumer exists yet: the distribution events have
    // no receiver). On one rank upstream only builds empty synchronizers
    // and flags the mesh distributed, which changes no value; the port
    // keeps the mesh serial there (a distributed mesh would only add the
    // collectives of the parallel insertion, with nothing to
    // synchronise).
    if psize > 1 {
        let mut partitioner = default_partitioner(prank);
        let _events = match partitioner.as_mut() {
            Some(p) => distribute_with(&mut mesh, Some(p.as_mut()), 0)?,
            None => distribute_with(&mut mesh, None, 0)?,
        };
    }
    chrono.store_time("dist_mesh", &mut out);

    let mesh = SharedMesh::new(mesh);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )?;
    if ex.upstream_compat {
        // D-003: every upstream cohesive defect (UD-217, UD-040, …); not the
        // `init_solver` reset of UD-352 (see the module documentation)
        model.ext.compat = CohesiveUpstreamCompat::upstream();
    }

    // model initialization
    model.init_full(&ex.model_options(AnalysisMethod::Static), true)?;
    chrono.store_time("init_full", &mut out);

    // boundary conditions
    let fixed = |axis| FixedValue { value: 0.0, axis };
    model
        .smm
        .apply_dirichlet_bc(&fixed(SpatialDirection::Z), "bottom")?; // face
    model
        .smm
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "right line")?; // line
    model.smm.blocked_dofs_mut()?.as_mut_slice()[3 * DIM + 1] = true; // point

    let compression = compression_matrix(user.compression)?;
    let shear = shear_matrix(user.shear)?;

    model.smm.external_force_mut()?.as_mut_slice().fill(0.0);

    apply_traction(&mut model, &compression, &["top", "bottom"])?;
    apply_traction(
        &mut model,
        &shear,
        &["top", "bottom", "side left", "side right"],
    )?;

    let paraview = user.output_paraview && ex.dump;
    if paraview {
        // upstream writes both dumpers below its run directory's
        // `paraview/` (the dumpers' default directory)
        model.smm.set_directory(&ex.paraview_dir())?;
        model.set_directory_to_dumper(COHESIVE_DUMPER, &ex.paraview_dir())?;
        model.smm.set_base_name("extrinsic")?;
        model.smm.add_dump_field_vector("displacement")?;
        model.smm.add_dump_field("internal_force")?;
        model.smm.add_dump_field("external_force")?;
        model.smm.add_dump_field("stress")?;
        model.smm.add_dump_field("blocked_dofs")?;
        model.smm.add_dump_field("grad_u")?;

        model.add_dump_field_to_dumper(COHESIVE_DUMPER, "displacement")?;
        model.add_dump_field_to_dumper(COHESIVE_DUMPER, "tractions")?;
        if ex.upstream_compat {
            // UD-114: upstream's base64 stream of an empty array (the
            // `cohesive elements` dumper before the first insertion: no
            // cell) is invalid; the compat layout reproduces those bytes
            for dumper in ["solid_mechanics_model", COHESIVE_DUMPER] {
                model
                    .smm
                    .dumpers
                    .dumpable
                    .dumper_as::<akantu::io::DumperParaview>(dumper)?
                    .set_compat_layout(true);
            }
        }

        model.dump(None, None)?;
        model.dump_named(COHESIVE_DUMPER, None, None)?;
    }

    chrono.store_time("initial_conditons", &mut out);

    model.solve_step(Some("static"))?;
    chrono.store_time("static_solve", &mut out);

    model
        .smm
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)?;

    // upstream `fout << … << std::endl`: every line is on disk once written
    let mut fout = if user.output_energy && prank == 0 {
        let mut f = LineFile::create(ex.output("energies.csv"))?;
        f.line("step, ed, ep, ek, ew, et")?;
        Some(f)
    } else {
        None
    };

    let (mut ed, mut ep, mut ek, mut ew) = (0.0, 0.0, 0.0, 0.0);
    if user.output_energy {
        ep = model.energy("potential")?;
        ek = model.energy("kinetic")?;
        ew += model.energy("external work")?;
    }
    let mut et = ed + ep + ek - ew;
    let mut energies = Vec::new();
    if user.output_energy {
        energies.push([ed, ep, ek, ew, et]);
    }
    if let Some(f) = fout.as_mut() {
        f.line(format!(
            "0, {}, {}, {}, {}, {}",
            cout(ed),
            cout(ep),
            cout(ek),
            cout(ew),
            cout(et)
        ))?;
    }

    let time_step = model.stable_time_step()? * 0.05;
    model.set_time_step(time_step, None)?;
    if prank == 0 {
        out.line(format!("Time step: {}", cout(time_step)));
    }

    if paraview {
        model.smm.add_dump_field("velocity")?;
        model.smm.add_dump_field("acceleration")?;
        model.add_dump_field_to_dumper(COHESIVE_DUMPER, "velocity")?;

        model.dump(None, None)?;
        model.dump_named(COHESIVE_DUMPER, None, None)?;
    }

    let new_shear = shear_matrix(user.inc_shear)?;

    let init_time = start_time.elapsed().as_secs_f64();
    chrono.store_time("before_step", &mut out);

    let nb_dofs_start = mesh.read()?.nb_global_nodes() * DIM;

    let loop_start = Instant::now();
    let mut inserted = Vec::new();
    // Main loop
    for s in 1..=max_steps {
        if s % 100 == 0 && s < 10000 {
            apply_traction(
                &mut model,
                &new_shear,
                &["top", "bottom", "side left", "side right"],
            )?;
            chrono.store_time("boundary_conditions", &mut out);
        }
        if user.cohesive_insertion {
            inserted.push(model.check_cohesive_stress()?);
            chrono.store_time("check_cohesive_stress", &mut out);
        }
        model.solve_step(Some("explicit_lumped"))?;
        chrono.store_time("solve_step", &mut out);

        if user.output_energy {
            ed = model.energy("dissipated")?;
            ep = model.energy("potential")?;
            ek = model.energy("kinetic")?;
            ew += model.energy("external work")?;

            et = ed + ep + ek - ew;
            energies.push([ed, ep, ek, ew, et]);

            if let Some(f) = fout.as_mut() {
                f.line(format!(
                    "{s}, {}, {}, {}, {}, {}",
                    cout(ed),
                    cout(ep),
                    cout(ek),
                    cout(ew),
                    cout(et)
                ))?;
            }
            chrono.store_time("energies", &mut out);
        }

        if paraview && s % 100 == 0 {
            model.dump(None, None)?;
            model.dump_named(COHESIVE_DUMPER, None, None)?;

            if prank == 0 {
                // upstream `milliseconds` = `duration<double, std::milli>`
                let loop_ms = loop_start.elapsed().as_secs_f64() * 1e3;
                let local = mesh.read()?;
                #[allow(clippy::cast_precision_loss)] // upstream `loop_time.count() / s`
                let per_step = loop_ms / s as Real;
                out.partial(format!(
                    "passing step {s}/{max_steps} - nb_cohesive_element: {} - nb_dofs: {} - {}\t\t \r",
                    nb_cohesive_elements(&local, DIM),
                    local.nb_global_nodes() * DIM,
                    cout(per_step)
                ));
            }
            chrono.store_time("dumpers", &mut out);
        }
    }
    let nb_dofs_end = mesh.read()?.nb_global_nodes() * DIM;

    let loop_time = loop_start.elapsed().as_secs_f64();
    let mut nb_cohesive = [u64::try_from(nb_cohesive_elements(&*mesh.read()?, DIM))
        .map_err(|_| Error::invalid("number of cohesive elements"))?];
    comm.all_reduce(&mut nb_cohesive, ReduceOp::Sum)?;
    let nb_cohesive = usize::try_from(nb_cohesive[0])
        .map_err(|_| Error::invalid("number of cohesive elements"))?;
    let dissipated = model.energy("dissipated")?;
    if prank == 0 {
        out.line("");
        out.line(format!(
            "Cohesive info: dissipated energy: {} - nb_cohesive_element: {nb_cohesive}",
            cout(dissipated)
        ));
        out.line(format!("Nb proc: {psize}"));
        out.line(format!("Full time: {}", cout(init_time + loop_time)));
        out.line(format!("Init time: {}", cout(init_time)));
        #[allow(clippy::cast_precision_loss)] // upstream `loop_time / max_steps`
        let per_step = loop_time / max_steps as Real;
        out.line(format!(
            "Step time: {} - nb_steps: {max_steps} - time_per_step: {}",
            cout(loop_time),
            cout(per_step)
        ));
        out.line(format!(
            "Ns DOFs - start: {nb_dofs_start} - end: {nb_dofs_end}"
        ));
        if ex.verbose {
            eprintln!("{}", chrono.csv());
        }
    }
    drop(fout);
    let timings_csv = chrono.csv();

    Ok(Outcome {
        model,
        mesh,
        user,
        time_step,
        energies,
        inserted,
        nb_cohesive_elements: nb_cohesive,
        dissipated,
        nb_dofs: (nb_dofs_start, nb_dofs_end),
        stdout: out.lines,
        timings_csv,
    })
}

/// [`run_on`] on the world communicator (MPI with the `mpi` feature, else
/// serial).
///
/// # Errors
/// See [`run_on`].
pub fn run(ex: &Example) -> Result<Outcome> {
    run_on(world()?, ex)
}

/// The partitioner of rank 0 on several ranks (Scotch with the `scotch`
/// feature, else recursive coordinate bisection; seed 1 as upstream with
/// `AKA_SEED=1`).
fn default_partitioner(prank: i32) -> Option<Box<dyn akantu::mesh::distribution::MeshPartitioner>> {
    if prank != 0 {
        return None;
    }
    #[cfg(feature = "scotch")]
    {
        Some(Box::new(akantu::mesh::scotch::ScotchPartitioner::new(1)))
    }
    #[cfg(not(feature = "scotch"))]
    {
        Some(Box::new(akantu::mesh::distribution::RcbPartitioner))
    }
}

/// The world communicator (MPI with the `mpi` feature, else serial).
fn world() -> Result<Communicator> {
    #[cfg(feature = "mpi")]
    {
        Communicator::mpi_world(akantu::comm::MpiOptions::default())
    }
    #[cfg(not(feature = "mpi"))]
    {
        Ok(Communicator::serial())
    }
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
