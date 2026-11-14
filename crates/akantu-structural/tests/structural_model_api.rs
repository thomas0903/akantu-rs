// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): API, invalidation and
// error-path tests of the structural model: materials
// (STRUCT-002, UD-023 live handle, UD-508 reassembly), solver configuration
// (STRUCT-008, UD-509), energies (STRUCT-011, UD-510), line-load input
// checks (STRUCT-012), dumps (STRUCT-013), refused configurations
// (serial-only, mixed dimensions UD-511, mesh events UD-342), no panics on
// user input.

mod structural_model_common;

use akantu_core::enums::{AnalysisMethod, EventPriority, NonLinearSolverType, TimeStepSolverType};
use akantu_core::{Array, Element, ElementType, Error, Idx, Real};
use akantu_mesh::{MeshEvent, MeshEventHandler, MeshPart};
use akantu_model::{Model, ModelOptions};
use akantu_solver::SolverCallback;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

fn bar_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 2e3,
        a: 0.5,
        i: 0.1,
        rho: 3.,
        ..StructuralMaterial::default()
    }
}

/// Axial bar along x, 2 elements of length 1, clamped at node 0, axial
/// force `p` at node 2 (2-D beams).
fn bar(method: AnalysisMethod) -> StructuralMechanicsModel {
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        2,
        2.0,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = StructuralMechanicsModel::new(shared(mesh), None, "bar").unwrap();
    m.add_material(bar_material(), "steel").unwrap();
    m.init_full_with(method).unwrap();
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..3].fill(true);
    m.external_force_mut().unwrap().as_mut_slice()[6] = 10.0;
    m
}

#[test]
fn materials_api() {
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        2,
        2.0,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = StructuralMechanicsModel::new(shared(mesh), None, "mats").unwrap();
    assert_eq!(m.add_material(bar_material(), "").unwrap(), 0);
    assert_eq!(m.add_material(bar_material(), "b").unwrap(), 1);
    assert_eq!(m.add_material(bar_material(), "").unwrap(), 2);
    assert_eq!(m.material_index("material_0").unwrap(), 0);
    assert_eq!(m.material_index("material_2").unwrap(), 2);
    assert!(matches!(
        m.add_material(bar_material(), "b"),
        Err(Error::AlreadyExists { .. })
    ));
    assert!(m.material_by_name("nope").is_err());
    assert!(m.material(3).is_err());
    assert_eq!(m.nb_materials(), 3);
    m.init_full_with(AnalysisMethod::Static).unwrap();
    m.element_material_mut(ElementType::BernoulliBeam2, G)
        .unwrap()
        .as_mut_slice()[1] = 1;
    let e = Element::new(ElementType::BernoulliBeam2, 1, G);
    assert_eq!(*m.material_by_element(e).unwrap(), bar_material());
    // a second init_full keeps the assignment (upstream initialize resizes)
    m.init_full_with(AnalysisMethod::Static).unwrap();
    assert_eq!(
        m.element_material(ElementType::BernoulliBeam2, G)
            .unwrap()
            .as_slice(),
        &[0, 1]
    );
}

/// UD-023 / UD-508: a material changed after a first solve (live handle)
/// and an element reassigned to another material are taken into account
/// (upstream assembles `K` once and its Python binding returns a copy).
#[test]
fn material_changes_are_reassembled() {
    let mut m = bar(AnalysisMethod::Static);
    m.solve_step(None).unwrap();
    let u1 = m.displacement().unwrap().as_slice()[6];
    let mat = bar_material();
    assert!((u1 - 10. * 2. / (mat.e * mat.a)).abs() <= 1e-12 * u1);
    m.material_mut(0).unwrap().e /= 2.;
    m.solve_step(None).unwrap();
    let u2 = m.displacement().unwrap().as_slice()[6];
    assert!((u2 - 2. * u1).abs() <= 1e-12 * u2, "{u2} vs {}", 2. * u1);
    // second element on a stiffer material
    let stiff = StructuralMaterial {
        e: 4. * mat.e,
        ..mat
    };
    let i = m.add_material(stiff, "stiff").unwrap();
    m.element_material_mut(ElementType::BernoulliBeam2, G)
        .unwrap()
        .as_mut_slice()[1] = i as u32;
    m.solve_step(None).unwrap();
    let u3 = m.displacement().unwrap().as_slice()[6];
    let want = 10. / (mat.e / 2. * mat.a) + 10. / (4. * mat.e * mat.a);
    assert!((u3 - want).abs() <= 1e-12 * want, "{u3} vs {want}");
}

#[test]
fn bad_material_indices_are_errors_not_panics() {
    let mut m = bar(AnalysisMethod::Static);
    m.element_material_mut(ElementType::BernoulliBeam2, G)
        .unwrap()
        .as_mut_slice()[0] = 7;
    assert!(m.solve_step(None).is_err());
    assert!(m.assemble_stiffness_matrix().is_err());
    assert!(m.compute_stresses().is_err());
    // no material at all
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        1,
        1.0,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = StructuralMechanicsModel::new(shared(mesh), None, "empty").unwrap();
    m.init_full_with(AnalysisMethod::Static).unwrap();
    let err = m.assemble_stiffness_matrix().unwrap_err();
    assert!(format!("{err}").contains("add_material"), "{err}");
}

#[test]
fn solver_configuration_follows_upstream() {
    let mut m = bar(AnalysisMethod::Static);
    let defaults = [
        (
            TimeStepSolverType::Static,
            AnalysisMethod::Static,
            NonLinearSolverType::NewtonRaphson,
        ),
        (
            TimeStepSolverType::DynamicLumped,
            AnalysisMethod::ExplicitLumpedMass,
            NonLinearSolverType::Lumped,
        ),
        (
            TimeStepSolverType::Dynamic,
            AnalysisMethod::ImplicitDynamic,
            NonLinearSolverType::NewtonRaphson,
        ),
    ];
    for (tss, method, nls) in defaults {
        m.physics.base.method = method;
        let o = m.physics.default_solver_options(tss);
        assert_eq!(o.non_linear_solver_type, Some(nls));
    }
    let ids = [
        (AnalysisMethod::Static, "static"),
        (AnalysisMethod::ImplicitDynamic, "implicit"),
        (AnalysisMethod::ExplicitLumpedMass, "explicit_lumped"),
        (AnalysisMethod::ExplicitConsistentMass, "explicit"),
        (AnalysisMethod::ExplicitContact, "unknown"),
    ];
    for (method, id) in ids {
        assert_eq!(m.physics.default_solver_id(method).0, id);
    }
    // the matrix type of every id is symmetric (C included)
    for id in ["K", "M", "C", "J"] {
        assert_eq!(
            m.physics.matrix_type(id),
            akantu_core::enums::MatrixType::Symmetric
        );
    }
    // switching to an implicit solver keeps the static one
    m.init_new_solver(AnalysisMethod::ImplicitDynamic).unwrap();
    assert!(m.solver.has_solver("static") && m.solver.has_solver("implicit"));
    assert!(m.velocity().is_ok());
}

/// UD-509: an analysis method the model does not know is refused (upstream
/// prints "UNKOWN." and throws later).
#[test]
fn unknown_analysis_method_is_refused() {
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        1,
        1.0,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = StructuralMechanicsModel::new(shared(mesh), None, "u").unwrap();
    assert!(m.init_full_with(AnalysisMethod::ExplicitContact).is_err());
    assert!(m.init_new_solver(AnalysisMethod::ImplicitContact).is_err());
}

/// STRUCT-011: kinetic energy 0 without mass (static), unknown ids 0
/// (UD-510), potential energy `½ fᵀu` after a static solve.
#[test]
fn energies() {
    let mut m = bar(AnalysisMethod::Static);
    assert_eq!(m.energy("kinetic").unwrap(), 0.0);
    assert_eq!(m.energy("external work").unwrap(), 0.0);
    m.solve_step(None).unwrap();
    let u = m.displacement().unwrap().as_slice()[6];
    let ep = m.energy("potential").unwrap();
    assert!((ep - 0.5 * 10. * u).abs() <= 1e-12 * ep, "{ep}");
    // a dynamic model with a velocity field: ½ vᵀ M v with the consistent
    // mass, rigid translation → ½ m v²
    let mut d = bar(AnalysisMethod::ImplicitDynamic);
    for n in 0..3 {
        d.velocity_mut().unwrap().as_mut_slice()[n * 3] = 2.0;
    }
    let mat = bar_material();
    let ek = d.kinetic_energy().unwrap();
    let want = 0.5 * mat.rho * mat.a * 2.0 * 4.0;
    assert!((ek - want).abs() <= 1e-12 * want, "{ek} vs {want}");
}

/// STRUCT-012: wrong traction arrays are refused before `external_force`
/// changes; DKT tractions are not implemented (no shape functions, UD-053).
#[test]
fn traction_arrays_are_checked() {
    let mut m = bar(AnalysisMethod::Static);
    let ty = ElementType::BernoulliBeam2;
    let nq = m.fe_engine().nb_integration_points(ty, G).unwrap();
    let before = m.external_force().unwrap().clone();
    let short = Array::from_vec(vec![1.0; 3 * (2 * nq - 1)], 3).unwrap();
    assert!(
        m.compute_forces_by_local_traction_array(&short, ty)
            .is_err()
    );
    let wrong_nc = Array::from_vec(vec![1.0; 2 * 2 * nq], 2).unwrap();
    assert!(
        m.compute_forces_by_global_traction_array(&wrong_nc, ty)
            .is_err()
    );
    assert!(
        m.compute_forces_by_local_traction_array(&short, ElementType::BernoulliBeam3)
            .is_err()
    );
    assert_eq!(m.external_force().unwrap().as_slice(), before.as_slice());
    let nodes = vec![vec![0., 0., 0.], vec![1., 0., 0.], vec![0., 1., 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1, 2]];
    let mesh = mesh_of(
        3,
        ElementType::DiscreteKirchhoffTriangle18,
        &nodes,
        &conn,
        None,
    );
    let mut s = StructuralMechanicsModel::new(shared(mesh), None, "s").unwrap();
    s.add_material(
        StructuralMaterial {
            e: 1.,
            nu: 0.3,
            t: 0.1,
            ..StructuralMaterial::default()
        },
        "",
    )
    .unwrap();
    s.init_full_with(AnalysisMethod::Static).unwrap();
    let dkt = ElementType::DiscreteKirchhoffTriangle18;
    let nq = s.fe_engine().nb_integration_points(dkt, G).unwrap();
    let t = Array::from_vec(vec![1.0; 6 * nq], 6).unwrap();
    assert!(matches!(
        s.compute_forces_by_local_traction_array(&t, dkt),
        Err(Error::NotImplemented(_))
    ));
}

/// The residual parts of the solver callback (`external`, `internal`,
/// otherwise an error).
#[test]
fn residual_parts() {
    let mut m = bar(AnalysisMethod::Static);
    assert!(m.physics.can_split_residual());
    let (p, dm) = (&mut m.physics, &mut m.solver.dof_manager);
    p.assemble_residual_part(dm, "external").unwrap();
    p.assemble_residual_part(dm, "internal").unwrap();
    assert!(p.assemble_residual_part(dm, "inertial").is_err());
}

/// The model is serial; 1-D, mismatched dimensions and mixed-dimension
/// structural types (UD-511) are refused at construction.
#[test]
fn refused_configurations() {
    let mesh = akantu_mesh::Mesh::new(1, "line").unwrap();
    assert!(matches!(
        StructuralMechanicsModel::new(shared(mesh), None, "m"),
        Err(Error::NotImplemented(_))
    ));
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        1,
        1.0,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    assert!(StructuralMechanicsModel::new(shared(mesh), Some(1), "m").is_err());
    // a 2-D beam in a 3-D mesh
    let nodes = vec![vec![0., 0., 0.], vec![1., 0., 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1]];
    let mesh = mesh_of(3, ElementType::BernoulliBeam2, &nodes, &conn, None);
    assert!(matches!(
        StructuralMechanicsModel::new(shared(mesh), None, "m"),
        Err(Error::Unsupported(_))
    ));
}

/// Swap the model's consumer token for a placeholder of a scratch mesh, so
/// that the model can be lent as the handler of its own mesh's mutation.
fn lend_token(
    m: &mut StructuralMechanicsModel,
    scratch: &akantu_mesh::SharedMesh,
    name: &str,
) -> akantu_mesh::ConsumerToken {
    let placeholder = scratch
        .register_consumer(name, EventPriority::MODEL, MeshPart::Main)
        .unwrap();
    std::mem::replace(&mut m.token, placeholder)
}

/// Mesh topology changes reach the model as events, which it refuses
/// (UD-342: upstream's structural FE engine throws on element events).
/// Through `SharedMesh::mutate` the refusal happens in the capability check
/// (`check_mesh_event`), **before** the commit: the
/// mutation is rolled back, the refusal is recoverable
/// (`Error::NotImplemented`, not fatal) and the mesh domain and the model
/// stay usable. Before the fix the model refused at dispatch, after the
/// commit, which poisoned the domain.
#[test]
fn topology_changes_are_refused() {
    let mut m = bar(AnalysisMethod::Static);
    let mesh = m.physics.base.mesh.clone();
    let scratch = shared(akantu_mesh::Mesh::new(2, "scratch").unwrap());
    let (nodes_before, revision) = (
        mesh.read().unwrap().nodes().as_slice().to_vec(),
        mesh.revision().unwrap(),
    );
    let token = lend_token(&mut m, &scratch, "p1");
    let r = mesh.mutate(
        &mut [(&token, &mut m as &mut dyn MeshEventHandler)],
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[5., 5.])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "fu2_01".into(),
                    extension: None,
                },
            );
            Ok(())
        },
        None,
    );
    m.token = token;
    let Err(e) = &r else {
        panic!("expected the structural model's refusal, got {r:?}");
    };
    assert!(matches!(e, Error::NotImplemented(_)), "{e:?}");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert!(format!("{e}").contains("UD-342"), "{e}");
    // rolled back: mesh untouched
    assert_eq!(mesh.read().unwrap().nodes().as_slice(), &nodes_before[..]);
    assert_eq!(mesh.revision().unwrap(), revision);
    // the model stays usable (its blocks still match the mesh revision) ...
    m.solve_step(None).unwrap();
    // axial bar: u(L) = p L / (E A) = 10 * 2 / (2e3 * 0.5)
    let u = m.displacement().unwrap().as_slice()[6];
    assert!((u - 0.02).abs() <= 1e-12, "{u}");
    // ... and so does the mesh domain: a later mutation commits. (Any commit
    // advances the mesh revision, after which the structural FE engine
    // refuses its precomputed blocks until `refresh`.)
    let token = lend_token(&mut m, &scratch, "p2");
    let again = mesh.mutate(
        &mut [(&token, &mut m as &mut dyn MeshEventHandler)],
        |_| Ok(()),
        None,
    );
    m.token = token;
    assert!(again.is_ok(), "{again:?}");
    assert!(mesh.read().is_ok());
}

/// The capability check and the dispatch share one rule.
/// For every event kind, `check_mesh_event` returns exactly what the
/// dispatch returns at the level where the model handles events
/// (`FE_ENGINE`): topology events → `NotImplemented` (UD-342),
/// `MeshIsDistributed` → `Unsupported` (UD-131); the other levels accept.
#[test]
fn fu2_01_capability_check_matches_the_dispatch() {
    let mut m = bar(AnalysisMethod::Static);
    let mesh = m.physics.base.mesh.clone();
    let e0 = Element::new(ElementType::BernoulliBeam2, 0, G);
    let numbering = || {
        let mut n = akantu_core::ElementTypeMapArray::new("numbering");
        n.alloc(2, 1, ElementType::BernoulliBeam2, G, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[0, 1]);
        n
    };
    let events = [
        MeshEvent::NodesAdded {
            nodes: vec![3],
            old_nodes: None,
            origin: "fu2_01".into(),
            extension: None,
        },
        MeshEvent::NodesRemoved {
            nodes: vec![2],
            new_numbering: vec![0, 1, -1],
            origin: "fu2_01".into(),
        },
        MeshEvent::ElementsAdded {
            elements: vec![e0],
            constitutive_law: None,
            origin: "fu2_01".into(),
            extension: None,
        },
        MeshEvent::ElementsRemoved {
            elements: vec![e0],
            new_numbering: numbering(),
            origin: "fu2_01".into(),
        },
        MeshEvent::ElementsChanged {
            old: vec![e0],
            new: vec![e0],
            new_numbering: numbering(),
            origin: "fu2_01".into(),
        },
        MeshEvent::MeshIsDistributed {
            origin: "fu2_01".into(),
        },
    ];
    let guard = mesh.read().unwrap();
    for (k, event) in events.iter().enumerate() {
        let check = m.check_mesh_event(&guard, event);
        if k < 5 {
            assert!(
                matches!(&check, Err(Error::NotImplemented(t)) if t.contains("UD-342")),
                "{event:?}: {check:?}"
            );
        } else {
            assert!(
                matches!(&check, Err(Error::Unsupported(t)) if t.contains("UD-131")),
                "{event:?}: {check:?}"
            );
        }
        for level in m.event_priorities() {
            let dispatch = m.on_mesh_event_at(&guard, event, level);
            if level == EventPriority::FE_ENGINE {
                assert_eq!(
                    format!("{dispatch:?}"),
                    format!("{check:?}"),
                    "{event:?} at {level:?}"
                );
            } else {
                assert!(dispatch.is_ok(), "{event:?} at {level:?}: {dispatch:?}");
            }
        }
    }
}

#[test]
fn model_trait_and_stable_time_step() {
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        2,
        2.0,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = StructuralMechanicsModel::new(shared(mesh), None, "trait").unwrap();
    m.add_material(bar_material(), "").unwrap();
    {
        let dynm: &mut dyn Model = &mut m;
        dynm.init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        assert!(matches!(
            dynm.stable_time_step(),
            Err(Error::NotImplemented(_))
        ));
        assert_eq!(
            dynm.model_type(),
            akantu_core::enums::ModelType::StructuralMechanicsModel
        );
    }
    assert_eq!(m.nb_degree_of_freedom(), 3);
    assert_eq!(m.id(), "trait");
}

/// `set_extra_normals` checks its sizes.
#[test]
fn extra_normals_sizes() {
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        2,
        1.0,
        &[0., 0., 0.],
        &[1., 0., 0.],
        Some(&[0., 0., 1.]),
    );
    let mut m = StructuralMechanicsModel::with_compat(
        shared(mesh),
        None,
        "n",
        StructuralModelUpstreamCompat::default(),
    )
    .unwrap();
    assert!(
        m.set_extra_normals(ElementType::BernoulliBeam3, &[0., 0., 1.])
            .is_err()
    );
    m.set_extra_normals(ElementType::BernoulliBeam3, &[0., 1., 0., 0., 1., 0.])
        .unwrap();
}

// ---------------------------------------------------------------------------
// dumps (STRUCT-013)
// ---------------------------------------------------------------------------

/// `DataArray` name → (opening tag, ASCII contents) of a `.vtu` file.
fn data_arrays(path: &std::path::Path) -> std::collections::BTreeMap<String, (String, Vec<Real>)> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut out = std::collections::BTreeMap::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<DataArray ") {
        let tag_end = rest[start..].find('>').unwrap() + start;
        let tag = &rest[start..=tag_end];
        let name = tag
            .split("Name=\"")
            .nth(1)
            .map(|s| s.split('"').next().unwrap().to_owned())
            .unwrap_or_default();
        let body_end = rest[tag_end..].find("</DataArray>").unwrap() + tag_end;
        let values: Vec<Real> = rest[tag_end + 1..body_end]
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        out.insert(name, (tag.to_owned(), values));
        rest = &rest[body_end..];
    }
    out
}

/// Strided nodal fields (translations / rotations / forces / moments,
/// padded to 3 components), the full blocked flags and the per-point
/// stresses, written by the model's ParaView dumper in text mode.
#[test]
fn paraview_dump_of_the_structural_fields() {
    let mut m = bar(AnalysisMethod::Static);
    m.external_force_mut().unwrap().as_mut_slice()[8] = 3.0; // moment
    m.solve_step(None).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("structural_dump");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    m.set_directory(&dir).unwrap();
    m.dumpers.dumpable.set_text_mode().unwrap();
    m.add_dump_field_vector("displacement").unwrap();
    m.add_dump_field("rotation").unwrap();
    m.add_dump_field_vector("force").unwrap();
    m.add_dump_field("momentum").unwrap();
    m.add_dump_field("internal_force").unwrap();
    m.add_dump_field("blocked_dofs").unwrap();
    m.add_dump_field("stress").unwrap();
    m.dump(None, None).unwrap();
    let vtu = dir.join("bar-VTU/bar_0000.vtu");
    let a = data_arrays(&vtu);
    // ASCII mode prints 16 significant digits (IOHelper format)
    let close = |got: &[Real], want: &[Real], what: &str| {
        assert_eq!(got.len(), want.len(), "{what}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() <= 1e-15 * w.abs(), "{what}: {g} vs {w}");
        }
    };
    let u = m.displacement().unwrap().as_slice().to_vec();
    let (tag, disp) = &a["displacement"];
    assert!(tag.contains("NumberOfComponents=\"3\""), "{tag}");
    let want: Vec<Real> = (0..3).flat_map(|n| [u[n * 3], u[n * 3 + 1], 0.0]).collect();
    close(disp, &want, "displacement");
    let (_, rot) = &a["rotation"];
    close(rot, &[u[2], u[5], u[8]], "rotation");
    let (_, mom) = &a["momentum"];
    close(mom, &[0., 0., 3.], "momentum");
    let (tag, blocked) = &a["blocked_dofs"];
    assert!(tag.contains("NumberOfComponents=\"3\""), "{tag}");
    assert_eq!(blocked.len(), 9);
    let (tag, stress) = &a["stress"];
    let nq = m
        .fe_engine()
        .nb_integration_points(ElementType::BernoulliBeam2, G)
        .unwrap();
    assert!(
        tag.contains(&format!("NumberOfComponents=\"{}\"", 2 * nq)),
        "{tag}"
    );
    let s = m.stress(ElementType::BernoulliBeam2, G).unwrap();
    close(stress, s.as_slice(), "stress");
    // velocity is not allocated in a static analysis
    m.add_dump_field("velocity").unwrap();
    assert!(m.dump(None, None).is_err());
}

/// The `rotary_inertia` dump field (not upstream; D-059, STRUCT-013): the
/// nodal rotary blocks of the lumped mass, 9 components per node in 3-D (a
/// general frame: full blocks), 1 in 2-D, written as the model holds them
/// (ASCII mode, 16 significant digits); next to it the translational
/// `mass`. Before the lumped mass exists (a static analysis) the field is
/// an error at dump time, as `mass` (UD-515). (The `…Vector` variants pad
/// the 2-D single component to 3, as every field of fewer components.)
#[test]
fn paraview_dump_of_the_rotary_inertia() {
    use akantu_structural::LumpedRotaryInertia;
    let nodes = vec![
        vec![0.3, 0.1, -0.2],
        vec![1.26, -0.62, 1.4],
        vec![1.9, 0.75, 2.3],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    // unit normals orthogonal to each member: (axis × helper) normalised
    let normals: Vec<Vec<Real>> = conn
        .iter()
        .zip([[0.2_f64, 0.9, -0.4], [-0.7, 0.1, 0.5]])
        .map(|(c, h)| {
            let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let x = [
                d[1] * h[2] - d[2] * h[1],
                d[2] * h[0] - d[0] * h[2],
                d[0] * h[1] - d[1] * h[0],
            ];
            let n = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            vec![x[0] / n, x[1] / n, x[2] / n]
        })
        .collect();
    let mat = StructuralMaterial {
        e: 3.,
        rho: 2.5,
        a: 0.04,
        iz: 2e-4,
        iy: 6e-4,
        gj: 1.5,
        ..StructuralMaterial::default()
    };
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("structural_dump_rotary");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let close = |got: &[Real], want: &[Real], what: &str| {
        assert_eq!(got.len(), want.len(), "{what}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() <= 1e-15 * w.abs(), "{what}: {g} vs {w}");
        }
    };
    for rule in [
        LumpedRotaryInertia::Block3x3,
        LumpedRotaryInertia::IsotropicMax,
    ] {
        let mesh = mesh_of(
            3,
            ElementType::BernoulliBeam3,
            &nodes,
            &conn,
            Some(&normals),
        );
        let compat = StructuralModelUpstreamCompat {
            lumped_rotary_inertia: rule,
            ..StructuralModelUpstreamCompat::default()
        };
        let mut m =
            StructuralMechanicsModel::with_compat(shared(mesh), None, "rotary", compat).unwrap();
        m.add_material(mat, "").unwrap();
        m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        m.assemble_lumped_mass_matrix().unwrap();
        m.set_directory(&dir).unwrap();
        m.dumpers.dumpable.set_text_mode().unwrap();
        m.add_dump_field("rotary_inertia").unwrap();
        m.add_dump_field_vector("mass").unwrap();
        m.dump(None, None).unwrap();
        let a = data_arrays(&dir.join("rotary-VTU/rotary_0000.vtu"));
        let (tag, j) = &a["rotary_inertia"];
        assert!(tag.contains("NumberOfComponents=\"9\""), "{tag}");
        let want = m.rotary_inertia().unwrap().as_slice().to_vec();
        close(j, &want, "rotary_inertia");
        let lm = m.lumped_mass().unwrap().as_slice().to_vec();
        let (_, mass) = &a["mass"];
        let want: Vec<Real> = (0..3).flat_map(|n| lm[n * 6..n * 6 + 3].to_vec()).collect();
        close(mass, &want, "mass");
        // full blocks with Block3x3, diagonal ones with IsotropicMax
        let off_diagonal = (0..3).any(|n| j[n * 9 + 1] != 0.0);
        assert_eq!(
            off_diagonal,
            rule == LumpedRotaryInertia::Block3x3,
            "{rule:?}"
        );
        m.remove_dump_field("rotary_inertia").unwrap();
        m.remove_dump_field("mass").unwrap();
    }
    // 2-D: one component; a static analysis has no rotary inertia
    let mut s = bar(AnalysisMethod::Static);
    s.set_directory(&dir).unwrap();
    s.dumpers.dumpable.set_text_mode().unwrap();
    s.add_dump_field("rotary_inertia").unwrap();
    assert!(s.dump(None, None).is_err());
    let mut d = bar(AnalysisMethod::ExplicitLumpedMass);
    d.assemble_lumped_mass_matrix().unwrap();
    d.set_directory(&dir).unwrap();
    d.dumpers.dumpable.set_text_mode().unwrap();
    d.add_dump_field("rotary_inertia").unwrap();
    d.dump(None, None).unwrap();
    let a = data_arrays(&dir.join("bar-VTU/bar_0000.vtu"));
    let (tag, j) = &a["rotary_inertia"];
    assert!(tag.contains("NumberOfComponents=\"1\""), "{tag}");
    close(
        j,
        d.rotary_inertia().unwrap().as_slice(),
        "2-D rotary_inertia",
    );
}
