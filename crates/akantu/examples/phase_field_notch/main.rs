// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/phase_field_model/
// phase_field_notch.cc (+ material_notch.dat, square_notch.geo).

//! `phase_field_notch` (EX-010): a single-edge notched plate in tension,
//! phase-field fracture (`CouplerSolidPhaseField`): both models static,
//! bottom blocked in y, left in x, the top displaced by `4e-6` per load
//! step (`IncrementValue`), 999 load steps of 10 staggered solid /
//! phase-field solves each, a dump every 100 steps
//! (`paraview/phase_notch_<n>.pvtu`: stress, grad_u, displacement,
//! damage). Afterwards the elements whose damage stays below 0.15 at some
//! quadrature point are clustered (`createClusters` with a
//! `PhaseFieldElementFilter`): the crack splits the plate into the groups
//! `crack_0` and `crack_1`, dumped through their own group dumpers
//! (`paraview/crack_<i>.pvtu`), and the number of clusters is printed.
//!
//! The progress line printed every 200 steps holds wall-clock timings
//! (upstream `std::chrono`), so it differs from run to run. `--upstream-compat`
//! selects every upstream defect of the coupled phase-field path
//! (coupler UD-382/619/620, phase field UD-377…380, the `phasefield`
//! material's tangent `LawOptions::upstream()`); the upstream solvers are
//! `_auto` (MUMPS in the oracle), `--solver eigen` pins the Eigen sparse LU
//! in both models. The defects are not reached here: **the default run
//! reproduces upstream** bitwise (with `--solver eigen`; the oracle
//! comparison is tested in both modes).
//!
//! ```text
//! cargo run -p akantu --release --example phase_field_notch [-- --upstream-compat --solver eigen]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;
use std::time::{Duration, Instant};

use akantu::core::enums::{AnalysisMethod, EventPriority, SpatialDirection};
use akantu::core::{Element, ElementType, GhostType, Result};
use akantu::couplers::CouplerSolidPhaseField;
use akantu::io::dumper::group::SharedMeshDumpers;
use akantu::io::dumper::{DumpBuffer, FieldDescriptor, FieldResolver};
use akantu::mesh::clusters::compute_clusters;
use akantu::mesh::{Mesh, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu::model::Model;
use akantu::model::boundary_condition::{FixedValue, IncrementValue};
use akantu::phase_field::PhaseFieldElementFilter;
use akantu::solid::material::LawOptions;
use common::Example;

/// Spatial dimension of the example (upstream `const Int spatial_dimension`).
pub const SPATIAL_DIMENSION: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The coupler (solid and phase-field models) in its final state.
    pub coupler: CouplerSolidPhaseField,
    /// Number of load steps run (upstream `nb_steps − 1`).
    pub nb_load_steps: usize,
    /// Number of clusters of unbroken elements (`Nb clusters: …`).
    pub nb_clusters: usize,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "phase_field_notch",
        "phase_field_model",
        "material_notch.dat",
    )
}

/// The progress line upstream prints every 200 steps (`\r[wheel] s/n
/// (ms/step - elapsed - ETA)` followed by `std::string(' ', 20)`, i.e. 32
/// characters `\x14`: the arguments of the `(count, char)` constructor are
/// swapped). Durations as upstream's `std::chrono` arithmetic: the time per
/// step is the elapsed nanoseconds divided by `s` (integer division).
#[must_use]
pub fn progress_line(s: usize, nb_steps: usize, elapsed: Duration) -> String {
    const WHEEL: [char; 4] = ['/', '-', '\\', '|'];
    let elapsed_ns = elapsed.as_nanos();
    let per_step_ns = elapsed_ns / s as u128;
    #[allow(clippy::cast_precision_loss)] // wall-clock durations
    let (ms_per_step, elapsed_s, eta_s) = (
        per_step_ns as f64 / 1e6,
        elapsed_ns as f64 / 1e9,
        ((nb_steps - s) as u128 * per_step_ns) as f64 / 1e9,
    );
    let idx = (s / 10) % 4;
    format!(
        "\r[{}] {s:>5}/{nb_steps} ({ms_per_step:>8.2}ms/step - elapsed: {elapsed_s:>8.2}s - ETA: \
         {eta_s:>8.2}s){}",
        WHEEL[idx],
        "\u{14}".repeat(32)
    )
}

/// A consumer stand-in for a mutation that records no event: it reports
/// the priorities of the consumer it stands for (the domain checks them)
/// and refuses every event (none is recorded).
struct NoEvents(Vec<EventPriority>);

impl MeshEventHandler for NoEvents {
    fn event_priority(&self) -> EventPriority {
        self.0[0]
    }
    fn event_priorities(&self) -> Vec<EventPriority> {
        self.0.clone()
    }
    fn on_mesh_event(&mut self, _mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        Err(akantu::core::Error::invalid(format!(
            "unexpected mesh event {event:?} in a group-only mutation"
        )))
    }
    fn check_mesh_event(&self, _mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        Err(akantu::core::Error::invalid(format!(
            "unexpected mesh event {event:?} in a group-only mutation"
        )))
    }
}

/// Upstream `mesh.createClusters(dim, prefix, PhaseFieldElementFilter(phase,
/// max_damage))` on the coupler's mesh: clusters of the elements of `dim`
/// accepted by the filter (connected through facets of a temporary facet
/// mesh), stored as element groups `<prefix>_<i>` (a group-only mutation of
/// the shared mesh: the models are its consumers). Returns the number of
/// clusters.
fn create_clusters(
    coupler: &CouplerSolidPhaseField,
    dim: usize,
    prefix: &str,
    filter: PhaseFieldElementFilter,
) -> Result<usize> {
    let unbroken = filter.unbroken_elements(&coupler.phase)?;
    let shared = coupler.solid.mesh().clone();
    let clusters = {
        let mesh = shared.read()?;
        let mut facets = akantu::mesh::utils::new_facet_mesh(&mesh, "mesh_facets_for_clusters")?;
        akantu::mesh::utils::build_all_facets(&mesh, &mut facets, dim, dim.saturating_sub(1))?;
        compute_clusters(&mesh, &facets, dim, &|e: Element| unbroken.contains(&e))?
    };
    let mut solid = NoEvents(coupler.solid.event_priorities());
    let mut phase = NoEvents(coupler.phase.event_priorities());
    shared.mutate(
        &mut [
            (coupler.solid.mesh_token(), &mut solid),
            (coupler.phase.mesh_token(), &mut phase),
        ],
        |tx| {
            let conn: Vec<Vec<akantu::core::Idx>> = clusters
                .iter()
                .flatten()
                .map(|&el| {
                    tx.mesh()
                        .connectivities()
                        .element_ref(el)
                        .map(|c| c.to_vec())
                })
                .collect::<Result<_>>()?;
            let groups = tx.groups_mut(MeshPart::Main)?;
            let mut k = 0;
            for (i, cluster) in clusters.iter().enumerate() {
                let name = format!("{prefix}_{i}");
                let mut g = groups.create_element_group_replace(&name, dim, true)?;
                for &el in cluster {
                    g.add(el, Some(&conn[k]), false)?;
                    k += 1;
                }
            }
            for i in 0..clusters.len() {
                groups
                    .element_group_mut(&format!("{prefix}_{i}"))?
                    .node_group
                    .optimize();
            }
            Ok(())
        },
        None,
    )?;
    Ok(clusters.len())
}

/// The fields of a group's default dumper: none (it writes the group's
/// mesh only).
struct NoFields;

impl FieldResolver for NoFields {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        _ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        Err(akantu::core::Error::invalid(format!(
            "the default dumper of a group has no field ({})",
            field.name
        )))
    }
}

/// Upstream `model.dumpGroup(group)`: the group's own default dumper
/// (`paraview_<group>`, files `<group>`), in the example's output
/// directory.
fn dump_group(ex: &Example, mesh: &SharedMesh, group: &str) -> Result<()> {
    let dumpers = SharedMeshDumpers::of(mesh)?;
    let m = mesh.read()?;
    let mut registry = dumpers.lock()?;
    let d = registry.element_group(&m, group)?;
    d.set_directory(&ex.paraview_dir())?;
    d.dump(&m, &NoFields, None, None)
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the models, the mesh, the solvers or the dumpers.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, |_, _| Ok(()))
}

/// [`run`] calling `on_step(s, &coupler)` after the staggered solves of
/// every load step `s` (before its dump).
///
/// # Errors
/// Any error of the models, the mesh, the solvers, the dumpers or
/// `on_step`.
pub fn run_with(
    ex: &Example,
    mut on_step: impl FnMut(usize, &CouplerSolidPhaseField) -> Result<()>,
) -> Result<Outcome> {
    ex.create_output_dir()?;
    // create mesh
    let mesh = ex.read_mesh(SPATIAL_DIMENSION, "square_notch.msh")?;

    // The model coupler contains the solid mechanics and the phase-field models
    let mut coupler = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        ex.parser()?,
    )?;
    if ex.upstream_compat {
        coupler.set_upstream_compat(true);
        coupler.solid.physics.law_options = Some(LawOptions::upstream());
    }

    // Each model can bet set separately
    coupler
        .solid
        .init_full(&ex.model_options(AnalysisMethod::Static))?;
    coupler
        .phase
        .init_full(&ex.model_options(AnalysisMethod::Static))?;

    // Dirichlet BC
    let fixed = |value, axis| FixedValue { value, axis };
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(0., SpatialDirection::Y), "bottom")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(0., SpatialDirection::X), "left")?;

    // Dumper settings (upstream: `model.setBaseName`, `model.addDumpField*`,
    // `model.dump()` with `model = coupler.getSolidMechanicsModel()`; the
    // coupler's series)
    coupler.solid.set_directory(&ex.paraview_dir())?;
    coupler.solid.set_base_name("phase_notch")?;
    coupler.solid.add_dump_field("stress")?;
    coupler.solid.add_dump_field("grad_u")?;
    coupler.solid.add_dump_field_vector("displacement")?;
    coupler.solid.add_dump_field("damage")?;
    if ex.dump {
        coupler.solid.dump(None, None)?;
    }

    let nb_steps = ex.steps.map_or(1000, |s| (s + 1).min(1000));
    let increment = 4e-6;
    let nb_staggered_steps = 10;

    let start_time = Instant::now();

    // Main loop over the loading steps
    for s in 1..nb_steps {
        if s.is_multiple_of(200) {
            ex.print_raw(progress_line(s, nb_steps, start_time.elapsed()));
        }
        coupler.solid.apply_dirichlet_bc(
            &IncrementValue {
                value: increment,
                axis: SpatialDirection::Y,
            },
            "top",
        )?;

        /* At each step, the two solvers are called alternately. Here a fixed number
         * of staggered iterations is set for simplicity but a convergence based on
         * the displacements, damage and/or energy can also be used to exit the
         * internal loop.*/
        for _ in 0..nb_staggered_steps {
            coupler.solve(None, None)?;
        }
        on_step(s, &coupler)?;

        if s.is_multiple_of(100) && ex.dump {
            coupler.solid.dump(None, None)?;
        }
    }

    /* Here a damage threshold is set and a mesh clustering function is called
     * using the phase-field element filter. This allows to cluster the mesh into
     * groups of elements separated by the mesh boundaries and "broken" elements
     * with damage above the threshold. */
    let damage_limit = 0.15;
    let global_nb_clusters = create_clusters(
        &coupler,
        SPATIAL_DIMENSION,
        "crack",
        PhaseFieldElementFilter::new(damage_limit),
    )?;

    if ex.dump {
        let mesh = coupler.solid.mesh().clone();
        for group in ["crack_0", "crack_1"] {
            // a reduced run (`--steps`) may not have split the plate yet:
            // its missing groups are skipped (upstream throws on them)
            let exists = mesh.read()?.groups().element_group_exists(group);
            if exists || ex.steps.is_none() {
                dump_group(ex, &mesh, group)?;
            }
        }
    }

    ex.say("");
    ex.say(format!("Nb clusters: {global_nb_clusters}"));

    Ok(Outcome {
        coupler,
        nb_load_steps: nb_steps - 1,
        nb_clusters: global_nb_clusters,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
