// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (same intent)
// of test/test_model/test_contact_mechanics_model/test_resolution/
// test_explicit_resolution.cc (model construction + initFull with a
// resolution) and test_resolution.cc (TestComputeN, shape function matrix of
// a contact element; the empty TestComputeDalpha/Nalpha are covered by the
// derivative-matrix unit tests), plus model-level behaviour: resolution
// instantiation and its error paths, solver ids/options, the solver
// callback, dump fields and the cohesive surface selector.

mod common;

use akantu_contact::resolution::shape_function_matrix;
use akantu_contact::{CohesiveSurfaceSelector, ContactMechanicsModel, SurfaceSelector};
use akantu_core::enums::{AnalysisMethod, TimeStepSolverType};
use akantu_core::{Element, ElementType, ElementTypeMapArray, GhostType, Real};
use akantu_io::dumper::{
    FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType, Padding,
};
use akantu_mesh::data::MeshDataValues;
use akantu_mesh::utils::SUBELEMENT_TO_ELEMENT;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_parser::Parser;
use akantu_solver::{DofManager, SolverCallback};
use common::*;

fn model_with(dat: &str) -> akantu_core::Result<ContactMechanicsModel> {
    let mesh = two_blocks_2d(2, 1.0, 1, 0.5);
    let parser = Parser::parse_str(dat, "options.dat").unwrap();
    let mut m = ContactMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "contact_mechanics_model",
        parser,
    )?;
    m.init_full(AnalysisMethod::Static)?;
    Ok(m)
}

/// test_explicit_resolution.cc: a contact model initialised for a static
/// analysis with one penalty resolution; printing works.
#[test]
fn test_explicit_resolution() {
    let dat = contact_dat("master", "slave", 0.0, 10.0, 0.0, false);
    let m = model_with(&dat).unwrap();
    assert!(m.are_resolutions_instantiated());
    assert_eq!(m.resolutions().len(), 1);
    assert_eq!(
        m.resolutions()[0].id(),
        "contact_mechanics_model:0:penalty_linear"
    );
    assert_eq!(
        m.resolutions()[0]
            .get_param("epsilon_n")
            .unwrap()
            .as_real()
            .unwrap(),
        10.0
    );
    assert!(!format!("{m:?}").is_empty());
    assert_eq!(m.fields.gaps.len(), 4 + 6);
}

/// test_resolution.cc, TestComputeN: the shape function matrix of every
/// supported master facet is `[I, −N₁I, …]` at the natural projection.
#[test]
fn test_resolution_compute_n() {
    for (ty, dim, xi) in [
        (ElementType::Segment2, 2, vec![0.3]),
        (ElementType::Segment3, 2, vec![-0.2]),
        (ElementType::Triangle3, 3, vec![0.2, 0.3]),
        (ElementType::Triangle6, 3, vec![0.1, 0.4]),
        (ElementType::Quadrangle4, 3, vec![0.2, -0.7]),
        (ElementType::Quadrangle8, 3, vec![-0.5, 0.25]),
    ] {
        let nn = ty.nb_nodes_per_element();
        let mut n = vec![0.0; nn];
        akantu_contact::geometry::shapes_at(ty, &xi, &mut n).unwrap();
        let mut a = vec![0.0; 3 * 27];
        let nd = shape_function_matrix(ty, dim, &xi, &mut a).unwrap();
        assert_eq!(nd, dim * (nn + 1));
        for c in 0..nd {
            for r in 0..dim {
                let (node, comp) = (c / dim, c % dim);
                let expected = if comp != r {
                    0.0
                } else if node == 0 {
                    1.0
                } else {
                    -n[node - 1]
                };
                assert_eq!(a[r + c * dim], expected, "{ty:?} ({r}, {c})");
            }
        }
        // rigid-body translation: A · [t; t; …] = 0 (partition of unity)
        let t: Vec<Real> = (0..nd).map(|c| [0.3, -1.1, 0.7][c % dim]).collect();
        for r in 0..dim {
            let v: Real = (0..nd).map(|c| a[r + c * dim] * t[c]).sum();
            assert!(v.abs() < 1e-15, "{ty:?}: A t = {v}");
        }
    }
}

#[test]
fn resolution_instantiation_errors() {
    // no contact_detector section (upstream dereferences an end iterator)
    let e = model_with("contact_resolution penalty_linear [\n name = c\n]\n");
    assert!(e.is_err());
    // no resolution although a file was parsed
    let d = "contact_detector [\n type = explicit\n master = master\n slave = slave\n projection_tolerance = 1e-10\n max_iterations = 10\n extension_tolerance = 1e-5\n]\n";
    let e = model_with(d).unwrap_err();
    assert!(e.to_string().contains("No contact resolutions"), "{e}");
    // unknown resolution type, missing name, duplicate names, bad parameter
    for (extra, what) in [
        (
            "contact_resolution penalty [\n name = c\n]\n",
            "unknown type",
        ),
        (
            "contact_resolution penalty_linear [\n epsilon_n = 1\n]\n",
            "missing name",
        ),
        (
            "contact_resolution penalty_linear [\n name = c\n]\ncontact_resolution penalty_quadratic [\n name = c\n]\n",
            "duplicate name",
        ),
        (
            "contact_resolution penalty_linear [\n name = c\n epsilon = 1\n]\n",
            "unknown parameter",
        ),
    ] {
        assert!(model_with(&format!("{d}{extra}")).is_err(), "{what}");
    }
    // unknown detection type
    let bad = d.replace("type = explicit", "type = magic");
    assert!(
        model_with(&format!(
            "{bad}contact_resolution penalty_linear [\n name = c\n]\n"
        ))
        .is_err()
    );
    // model section resolutions come first
    let ms = format!(
        "{d}model contact_mechanics_model [\n contact_resolution penalty_quadratic [\n  name = inner\n ]\n]\ncontact_resolution penalty_linear [\n name = outer\n]\n"
    );
    let m = model_with(&ms).unwrap();
    let types: Vec<&str> = m
        .resolutions()
        .iter()
        .map(|r| r.resolution_type())
        .collect();
    assert_eq!(types, vec!["penalty_quadratic", "penalty_linear"]);
}

#[test]
fn solver_ids_and_options() {
    let id = |m| ContactMechanicsModel::default_solver_id(m);
    assert_eq!(
        id(AnalysisMethod::ExplicitLumpedMass),
        ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
    );
    assert_eq!(
        id(AnalysisMethod::Static),
        ("static".into(), TimeStepSolverType::Static)
    );
    assert_eq!(
        id(AnalysisMethod::ImplicitDynamic),
        ("implicit".into(), TimeStepSolverType::Dynamic)
    );
    assert_eq!(
        id(AnalysisMethod::ExplicitContact).1,
        TimeStepSolverType::NotDefined
    );
    let o = ContactMechanicsModel::default_solver_options_for(TimeStepSolverType::Static).unwrap();
    assert_eq!(
        o.non_linear_solver_type,
        Some(akantu_core::enums::NonLinearSolverType::NewtonRaphsonContact)
    );
    assert!(
        ContactMechanicsModel::default_solver_options_for(TimeStepSolverType::NotDefined).is_err()
    );
}

/// The solver callback assembles the contact force into the residual of
/// the `displacement` family and the stiffness into `K`; dump fields
/// resolve.
#[test]
fn solver_callback_and_dump_fields() {
    let mesh = two_blocks_2d(2, 1.0, 1, 0.5);
    let dat = contact_dat("master", "slave", 0.0, 1e3, 0.0, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    translate_group(&mut model, &shared, "upper_body", &[0.0, -0.01]);
    model.search().unwrap();
    assert_eq!(model.contact_elements().len(), 2);
    let mut dofs = DofManager::new("dm", akantu_comm::Communicator::serial());
    dofs.register_nodal_dofs(&shared.read().unwrap(), "displacement", 2)
        .unwrap();
    model.assemble_residual(&mut dofs).unwrap();
    let r = dofs.residual().values().to_vec();
    assert_eq!(r.as_slice(), model.internal_force().as_slice());
    model.assemble_matrix(&mut dofs, "K").unwrap();
    let k = dofs.matrix("K").unwrap();
    assert!(k.values().iter().any(|v| *v != 0.0));
    assert!(model.assemble_lumped_matrix(&mut dofs, "M").is_err());
    let fd = |name: &str, source: FieldSource| FieldDescriptor {
        name: name.into(),
        source,
        support: FieldSupport::Nodal,
        value_type: FieldValueType::Real,
        group: None,
        padding: Padding::None,
        homogenize: false,
    };
    let m = shared.read().unwrap();
    for name in [
        "contact_force",
        "normal_force",
        "tangential_force",
        "normals",
        "tangents",
        "gaps",
        "areas",
        "tangential_traction",
        "contact_state",
        "blocked_dofs",
    ] {
        let b = model.resolve(
            &m,
            &fd(name, FieldSource::ModelNodal(name.into())),
            None,
            GhostType::NotGhost,
        );
        assert!(b.is_ok(), "{name}");
        assert_eq!(b.unwrap().len(), m.nb_nodes(), "{name}");
    }
    assert!(
        model
            .resolve(
                &m,
                &fd("x", FieldSource::ModelNodal("x".into())),
                None,
                GhostType::NotGhost
            )
            .is_err()
    );
}

/// Cohesive selector: the two facets of every new cohesive element (from
/// `subelement_to_element` of the facet mesh) join the surface, with their
/// nodes; master = slave = the surface nodes.
#[test]
fn cohesive_surface_selector_collects_facets() {
    let mut facets = Mesh::new(2, "mesh_facets").unwrap();
    {
        let mut a = MeshAccessor::new(&mut facets);
        for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 0.0], [1.0, 0.0], [2.0, 0.0]] {
            a.push_node(&x).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[2, 3]);
        c.push_tuple(&[1, 4]);
    }
    let mut s2e: ElementTypeMapArray<Element> = ElementTypeMapArray::new("subelement_to_element");
    let blk = s2e
        .alloc(
            1,
            2,
            ElementType::Cohesive2d4,
            GhostType::NotGhost,
            Element::NULL,
        )
        .unwrap();
    blk.tuple_mut(0).copy_from_slice(&[
        Element::new(ElementType::Segment2, 0, GhostType::NotGhost),
        Element::new(ElementType::Segment2, 1, GhostType::NotGhost),
    ]);
    facets
        .data_mut()
        .insert_elemental(SUBELEMENT_TO_ELEMENT, MeshDataValues::Element(s2e));
    let mut sel = CohesiveSurfaceSelector::new();
    assert!(sel.master_list(&facets).unwrap().is_empty());
    let coh = Element::new(ElementType::Cohesive2d4, 0, GhostType::NotGhost);
    let regular = Element::new(ElementType::Segment2, 2, GhostType::NotGhost);
    sel.on_elements_added(&facets, &[regular, coh]).unwrap();
    let s = sel.contact_surface();
    assert_eq!(
        s.elements_of(ElementType::Segment2, GhostType::NotGhost),
        &[0, 1]
    );
    assert_eq!(sel.master_list(&facets).unwrap(), &[0, 1, 2, 3]);
    assert_eq!(sel.slave_list(&facets).unwrap(), &[0, 1, 2, 3]);
    assert_eq!(sel.part(), akantu_mesh::MeshPart::Facets);
}

/// Contact on the facet mesh (`MeshPart::Facets`, upstream contact model on
/// `mesh.getMeshFacets()` with `AllSurfaceSelector`): the interface facets
/// of the facet mesh are grouped as `master`/`slave`, and the penetrating
/// slaves are detected with the parent mesh giving the outward directions.
#[test]
fn contact_on_the_facet_mesh_with_all_surface_selector() {
    use akantu_contact::AllSurfaceSelector;
    use akantu_mesh::MeshPart;
    let mut mesh = two_blocks_2d(4, 1.0, 2, 0.5);
    // set-up phase: no mesh consumer exists yet, nothing to dispatch to
    let _events = akantu_mesh::utils::init_mesh_facets(&mut mesh).unwrap();
    // interface facets: segments at y = 0 of the facet mesh, split by the
    // body their nodes belong to
    let upper_nodes = nodes_of_physical(&mesh, "upper_body");
    let facets = mesh.mesh_facets_mut().unwrap();
    let conn = facets
        .connectivity(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .clone();
    let x = facets.nodes().clone();
    let (mut master, mut slave) = (Vec::new(), Vec::new());
    for (e, c) in conn.as_slice().chunks(2).enumerate() {
        let on_interface = c.iter().all(|&n| x.tuple(n as usize)[1] == 0.0);
        if !on_interface {
            continue;
        }
        let el = Element::try_new(ElementType::Segment2, e, GhostType::NotGhost).unwrap();
        if c.iter().all(|n| upper_nodes.contains(n)) {
            slave.push(el);
        } else {
            master.push(el);
        }
    }
    assert_eq!((master.len(), slave.len()), (4, 2));
    add_group(facets, "master", 1, &master);
    add_group(facets, "slave", 1, &slave);
    let dat = contact_dat("master", "slave", 0.0, 1e3, 0.0, false);
    let parser = Parser::parse_str(&dat, "contact.dat").unwrap();
    let selector = AllSurfaceSelector::new(mesh.mesh_facets().unwrap(), &parser).unwrap();
    assert_eq!(selector.part(), MeshPart::Facets);
    assert_eq!(selector.contact_surface().nb_elements(), 6);
    let shared = SharedMesh::new(mesh);
    let mut model = ContactMechanicsModel::on_part(
        shared.clone(),
        MeshPart::Facets,
        None,
        "contact_mechanics_model",
        parser,
    )
    .unwrap();
    model.init_full(AnalysisMethod::ExplicitLumpedMass).unwrap();
    assert!(
        model
            .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
            .is_ok()
    );
    model.set_surface_selector(Box::new(selector)).unwrap();
    // AllSurfaceSelector: master = slave = every surface node (two-pass)
    for n in upper_nodes {
        model.positions_mut().tuple_mut(n as usize)[1] -= 0.01;
    }
    model.search().unwrap();
    // the 3 upper-interface nodes and the 3 lower-interface nodes under the
    // upper block (x = ±1 lie outside it: no valid projection)
    assert_eq!(model.contact_elements().len(), 3 + 3);
    for ce in model.contact_elements() {
        let g = model.gaps().tuple(ce.slave as usize)[0];
        assert!((g - 0.01).abs() < 1e-15, "gap {g}");
    }
    model.assemble_internal_forces().unwrap();
    assert!(model.internal_force().as_slice().iter().any(|f| *f != 0.0));
}
