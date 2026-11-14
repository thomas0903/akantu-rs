// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/io/
// dumper/dumper_low_level.cc (+ locomotive_tools.{cc,hh}, swiss_train.geo).

//! `dumper_low_level` (EX-025): the low-level dumper API — two ParaView
//! dumpers created directly, one on the whole "swiss train" mesh, one on
//! a filtered mesh (the elements and nodes of the four wheel groups merged
//! into the element group `wheels`), external fields served from the
//! example's own arrays (a 3-component nodal displacement, an elemental
//! colour from the physical names), and 100 dumps of the train moving to
//! the right while its wheels turn (`paraview/dumper/{train,wheels}_*`).
//!
//! Upstream dumpers keep references to the arrays; akantu-rs dumpers store
//! field descriptors and read the live values at dump time from a
//! [`akantu::io::FieldResolver`] ([`locomotive_tools::TrainFields`]).
//!
//! `--upstream-compat` reproduces upstream's wheels that do not turn
//! (UD-473, see [`locomotive_tools`]).
//!
//! ```text
//! cargo run -p akantu --example dumper_low_level [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;
#[path = "../common/locomotive_tools.rs"]
pub mod locomotive_tools;

use std::process::ExitCode;

use akantu::core::{ElementKind, ElementType, GhostType, Real, Result};
use akantu::io::{Dumper, DumperParaview, MeshFilter, MeshSelection, Padding};
use common::Example;
use locomotive_tools::{TrainFields, external_field, fill_colour, group_wheels, move_train};

/// Default configuration (upstream directory; no input file).
#[must_use]
pub fn example() -> Example {
    Example::new("dumper_low_level", "solid_mechanics_model/io/dumper", "")
}

/// The upstream `main` after `initialize`; returns the final displacement.
///
/// # Errors
/// Any error of the mesh or the dumpers.
pub fn run(ex: &Example) -> Result<Vec<Real>> {
    ex.create_output_dir()?;
    let spatial_dimension = 2;
    let mut mesh = ex.read_mesh(spatial_dimension, "swiss_train.msh")?;
    let wheels = group_wheels(&mut mesh)?;
    let nodes = mesh.nodes().as_slice().to_vec();
    let nb_nodes = mesh.nb_nodes();
    let dim = mesh.spatial_dimension();

    // three components to warp the train deformation in ParaView
    let mut displacement = vec![0.0; nb_nodes * 3];

    // ---- dumpers ----------------------------------------------------------
    // one for the full mesh, one for the filtered mesh
    let dir = ex.output("paraview/dumper");
    let dir = dir.to_string_lossy();
    let mut dumper = DumperParaview::new("train", &dir, false);
    let mut wheels_dumper = DumperParaview::new("wheels", &dir, false);

    // register the full mesh (upstream registerMesh defaults:
    // _all_dimensions, _not_ghost, _ek_not_defined)
    dumper.register_mesh_selection(MeshSelection::new(
        None,
        GhostType::NotGhost,
        ElementKind::NotDefined,
    ))?;

    // register a filtered mesh limited to the nodes and elements of the
    // wheel groups (upstream registerFilteredMesh with the group's arrays)
    let (elements, wheel_nodes) = {
        let g = mesh.groups().element_group("wheels")?;
        (Box::new(g.elements.to_plain()?), g.nodes().to_vec())
    };
    wheels_dumper.register_mesh_selection(
        MeshSelection::new(None, GhostType::NotGhost, ElementKind::NotDefined).filtered(
            MeshFilter::Explicit {
                elements,
                nodes: wheel_nodes,
            },
        ),
    )?;

    let mut colour = vec![0; mesh.nb_element(ElementType::Triangle3, GhostType::NotGhost)];

    // a first output of the two meshes, without fields
    if ex.dump {
        let fields = TrainFields {
            displacement: &displacement,
            nb_component: 3,
            colour: &colour,
        };
        dumper.dump(&mesh, &fields, None, None)?;
        wheels_dumper.dump(&mesh, &fields, None, None)?;
    }

    // nodal displacement and elemental colour; the dumper of the filtered
    // mesh writes them on its nodes and elements
    dumper.register_field(external_field(
        "displacement",
        "displacement",
        Padding::None,
    ))?;
    dumper.register_field(external_field("colour", "colour", Padding::None))?;
    wheels_dumper.register_field(external_field(
        "displacement",
        "displacement",
        Padding::None,
    ))?;
    wheels_dumper.register_field(external_field("colour", "colour", Padding::None))?;

    // ---- paint and move the train -----------------------------------------
    colour = fill_colour(&mesh)?;

    let nb_steps = ex.steps_or(100);
    let mut l_center = vec![0.0; 3];
    let mut r_center = vec![0.0; 3];
    for i in 0..dim {
        l_center[i] = nodes[14 * dim + i];
        r_center[i] = nodes[2 * dim + i];
    }

    for i in 0..nb_steps {
        move_train(
            i,
            100,
            (&l_center, &r_center),
            &nodes,
            dim,
            &wheels,
            &mut displacement,
            ex.upstream_compat,
        )?;
        // output after each step for both dumpers
        if ex.dump {
            let fields = TrainFields {
                displacement: &displacement,
                nb_component: 3,
                colour: &colour,
            };
            dumper.dump(&mesh, &fields, None, None)?;
            wheels_dumper.dump(&mesh, &fields, None, None)?;
        }
    }
    Ok(displacement)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
