// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No direct upstream
// counterpart: end-to-end checks of the non-local rebuild contract
// (UD-562) through the solid model's mesh-event route (level 100, upstream
// NonLocalManager as mesh-event handler,
// src/model/common/non_local_toolbox/non_local_manager.cc onElementsAdded).

//! A regular element added through `SharedMesh::mutate` invalidates the
//! neighbourhoods (the next assembly is refused); `reinitialize_non_local`
//! rebuilds them with the new points, the current options and the grid
//! centre of the changed mesh, exactly as a model created on the changed
//! mesh.

use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_io::read_mesh;
use akantu_mesh::{ConsumerToken, Mesh, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::non_local::{SummationOrder, WeightArithmetic};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

const Q4: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;
const DAT: &str = "material mazars_non_local [\n  name = body\n  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n]\n\
                   non_local body base_wf [\n  radius = 0.6\n]\n";

fn plate() -> Mesh {
    let root = akantu_validation::fixtures_root()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(
        &mut mesh,
        &root.join("oracle/probes/non_local/inputs/plate.msh"),
        MeshIoType::Gmsh,
    )
    .unwrap();
    mesh
}

fn model_on(mesh: SharedMesh) -> SolidMechanicsModel {
    let parser = Parser::parse_str(DAT, "nl.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(mesh, None, "solid_mechanics_model", parser).unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model
}

/// Append a quadrangle on four new nodes next to the plate (x in [1, 1.5],
/// which moves the bounding box), assigned to law 0.
fn append_quad(mesh_h: &SharedMesh, handlers: &mut [(&ConsumerToken, &mut dyn MeshEventHandler)]) {
    mesh_h
        .mutate(
            handlers,
            |tx| {
                let mut nodes = Vec::new();
                for (x, y) in [(1.0, 0.0), (1.5, 0.0), (1.5, 0.5), (1.0, 0.5)] {
                    nodes.push(tx.push_node(MeshPart::Main, &[x, y])?);
                }
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: nodes.clone(),
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                let e = tx.push_element(MeshPart::Main, Q4, NG, &nodes)?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: Some(vec![0]),
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
}

fn mutate_model(model: &mut SolidMechanicsModel) {
    let mesh_h = model.physics.base.mesh.clone();
    let (token, mut parts) = model.split_for_mutation();
    append_quad(
        &mesh_h,
        &mut [(token, &mut parts as &mut dyn MeshEventHandler)],
    );
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn regular_element_event_refuses_then_rebuild_includes_new_points() {
    let mut model = model_on(SharedMesh::new(plate()));
    model.assemble_internal_forces().unwrap();
    let points_before = model
        .non_local_manager()
        .unwrap()
        .neighborhood("body")
        .unwrap()
        .points()
        .len();
    let n_el = model.physics.base.mesh.read().unwrap().nb_element(Q4, NG);
    mutate_model(&mut model);
    assert_eq!(
        model.physics.base.mesh.read().unwrap().nb_element(Q4, NG),
        n_el + 1
    );
    let stale = model.assemble_internal_forces();
    assert!(stale.is_err(), "stale neighbourhoods must be refused");
    model.reinitialize_non_local().unwrap();
    model.assemble_internal_forces().unwrap();
    let nh = model
        .non_local_manager()
        .unwrap()
        .neighborhood("body")
        .unwrap();
    let new_el = i32::try_from(n_el).unwrap();
    let new_points = nh.points().iter().filter(|q| q.element == new_el).count();
    assert_eq!(nh.points().len(), points_before + 4);
    assert_eq!(new_points, 4);
}

/// The rebuild re-centres the grid on the changed bounding box and
/// then pairs and weighs the points exactly as a model created on the
/// changed mesh.
#[test]
fn rebuild_equals_a_model_created_on_the_changed_mesh() {
    let mut model = model_on(SharedMesh::new(plate()));
    model.assemble_internal_forces().unwrap();
    let center_before = model
        .non_local_manager()
        .unwrap()
        .neighborhood("body")
        .unwrap()
        .grid()
        .center()
        .to_vec();
    mutate_model(&mut model);
    model.reinitialize_non_local().unwrap();
    model.assemble_internal_forces().unwrap();

    let fresh_mesh = SharedMesh::new(plate());
    append_quad(&fresh_mesh, &mut []);
    let mut fresh = model_on(fresh_mesh);
    fresh.assemble_internal_forces().unwrap();

    let (a, b) = (
        model.non_local_manager().unwrap(),
        fresh.non_local_manager().unwrap(),
    );
    let (na, nb) = (
        a.neighborhood("body").unwrap(),
        b.neighborhood("body").unwrap(),
    );
    assert_ne!(
        na.grid().center(),
        center_before.as_slice(),
        "the bounding box moved"
    );
    assert_eq!(na.grid().center(), nb.grid().center());
    assert_eq!(
        na.pair_list(NG),
        nb.pair_list(NG),
        "pair lists (identity and order)"
    );
    let wa: Vec<Real> = na.pair_weights(NG).iter().flat_map(|w| *w).collect();
    let wb: Vec<Real> = nb.pair_weights(NG).iter().flat_map(|w| *w).collect();
    assert_eq!(bits(&wa), bits(&wb), "pair weights");
    assert_eq!(bits(a.volumes()), bits(b.volumes()), "volumes");
    let fa = model.internal_force().unwrap().as_slice().to_vec();
    let fb = fresh.internal_force().unwrap().as_slice().to_vec();
    assert_eq!(bits(&fa), bits(&fb), "internal force");
}

/// Options set after `init_full` take effect at the rebuild.
#[test]
fn rebuild_applies_options_set_after_init_full() {
    let mut model = model_on(SharedMesh::new(plate()));
    model.assemble_internal_forces().unwrap();
    assert_eq!(
        model.non_local_manager().unwrap().options().order,
        SummationOrder::UpstreamHalfPairs
    );
    model.physics.non_local_options.order = SummationOrder::KeyOrder;
    model.physics.non_local_options.weight_arithmetic = WeightArithmetic::UpstreamX86_64;
    model.reinitialize_non_local().unwrap();
    let o = model.non_local_manager().unwrap().options();
    assert_eq!(o.order, SummationOrder::KeyOrder);
    assert_eq!(o.weight_arithmetic, WeightArithmetic::UpstreamX86_64);
    model.assemble_internal_forces().unwrap();
}
