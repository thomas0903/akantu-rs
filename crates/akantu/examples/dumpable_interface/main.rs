// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/io/
// dumper/dumpable_interface.cc (+ locomotive_tools.{cc,hh},
// swiss_train.geo).

//! `dumpable_interface` (EX-026): the same moving train as
//! `dumper_low_level`, through the high-level [`Dumpable`] registry
//! (upstream: the `Dumpable` interface inherited by `Mesh` and
//! `ElementGroup`): a default ParaView dumper `train` on the whole mesh, the
//! group dumper of `wheels` (a ParaView dumper named after the group, on
//! the group's elements and nodes), external fields added to either, a
//! 2-component displacement also dumped padded to 3 components
//! (`paraview/dumpable/{train,wheels}_*`).
//!
//! **UD-474** (D-003 category B/C): upstream registers a wheel-filtered
//! colour on the whole-mesh dumper and unfiltered fields on the wheels
//! dumper, so its VTU files hold arrays whose lengths do not match the
//! numbers of points/cells. A dumper of the port writes every field on its
//! own mesh part (the wheels dumper writes the wheel nodes and elements)
//! and refuses a group field that does not cover its mesh part,
//! so the filtered `color` is not registered on `train`.
//!
//! `--upstream-compat` reproduces upstream's wheels that do not turn
//! (UD-473, see [`locomotive_tools`]).
//!
//! ```text
//! cargo run -p akantu --example dumpable_interface [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;
#[path = "../common/locomotive_tools.rs"]
pub mod locomotive_tools;

use std::process::ExitCode;

use akantu::core::{ElementKind, ElementType, GhostType, Real, Result};
use akantu::io::{Dumpable, DumperParaview, MeshFilter, MeshSelection, Padding};
use common::Example;
use locomotive_tools::{TrainFields, external_field, fill_colour, group_wheels, move_train};

/// Default configuration (upstream directory; no input file).
#[must_use]
pub fn example() -> Example {
    Example::new("dumpable_interface", "solid_mechanics_model/io/dumper", "")
}

/// The upstream `main` after `initialize`; returns the final displacement.
///
/// # Errors
/// Any error of the mesh or the dumpers.
pub fn run(ex: &Example) -> Result<Vec<Real>> {
    ex.create_output_dir()?;
    let mut mesh = ex.read_mesh(2, "swiss_train.msh")?;
    let wheels = group_wheels(&mut mesh)?;
    let nodes = mesh.nodes().as_slice().to_vec();
    let nb_nodes = mesh.nb_nodes();
    let spatial_dimension = mesh.spatial_dimension();

    // a 2-D displacement, padded to 3 components by one of the fields
    let mut displacement = vec![0.0; nb_nodes * spatial_dimension];
    let mut colour = vec![0; mesh.nb_element(ElementType::Triangle3, GhostType::NotGhost)];

    // ---- dumpers ----------------------------------------------------------
    let dir = ex.output("paraview/dumpable");
    let mut dumpable = Dumpable::new();
    // dumper of the complete mesh, registered as the default dumper
    dumpable.register_dumper(
        "train",
        Box::new(DumperParaview::new("train", &dir.to_string_lossy(), false)),
        true,
    );
    dumpable.add_dump_mesh(MeshSelection::new(
        None,
        GhostType::NotGhost,
        ElementKind::NotDefined,
    ))?;

    // the group dumper of `wheels` (upstream ElementGroup constructor:
    // ParaView dumper named after the group on the group's elements and
    // nodes), registered as dumper "wheels"
    dumpable.register_paraview("wheels", Some("wheels"), false);
    dumpable.add_dump_mesh_to_dumper(
        "wheels",
        MeshSelection::new(None, GhostType::NotGhost, ElementKind::NotDefined)
            .filtered(MeshFilter::ElementGroup("wheels".into())),
    )?;
    dumpable.set_directory_to_dumper("wheels", &dir)?;

    // external arrays as fields
    dumpable.add_dump_field(external_field(
        "displacement",
        "displacement",
        Padding::None,
    ))?;
    // UD-474: the wheel-filtered colour is not registered on `train`
    dumpable.add_dump_field_to_dumper(
        "wheels",
        external_field("displacement", "displacement", Padding::None),
    )?;
    dumpable
        .add_dump_field_to_dumper("wheels", external_field("colour", "colour", Padding::None))?;

    // the displacement padded to a 3-component ParaView vector
    let padded = external_field(
        "displacement_as_paraview_vector",
        "displacement",
        Padding::To3D,
    );
    dumpable.add_dump_field(padded.clone())?;
    dumpable.add_dump_field_to_dumper("wheels", padded)?;

    // ---- paint and move the train -----------------------------------------
    colour = {
        let c = fill_colour(&mesh)?;
        debug_assert_eq!(c.len(), colour.len());
        c
    };

    let nb_steps = ex.steps_or(100);
    let mut l_center = vec![0.0; spatial_dimension];
    let mut r_center = vec![0.0; spatial_dimension];
    for i in 0..spatial_dimension {
        l_center[i] = nodes[14 * spatial_dimension + i];
        r_center[i] = nodes[2 * spatial_dimension + i];
    }

    for i in 0..nb_steps {
        move_train(
            i,
            100,
            (&l_center, &r_center),
            &nodes,
            spatial_dimension,
            &wheels,
            &mut displacement,
            ex.upstream_compat,
        )?;
        // dumps through the Dumpable registry
        if ex.dump {
            let fields = TrainFields {
                displacement: &displacement,
                nb_component: spatial_dimension,
                colour: &colour,
            };
            dumpable.dump(&mesh, &fields, None, None)?;
            dumpable.dump_named("wheels", &mesh, &fields, None, None)?;
        }
    }
    Ok(displacement)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
