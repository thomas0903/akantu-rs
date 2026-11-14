// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for the diffusion model, derived from
// src/model/diffusion_model/diffusion_model.cc, diffusion_law.cc,
// heat_diffusion.cc and src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh: the D-068 capability check, UD-811
// (non-regular element kinds), the value checks (mesh events
// incl. UD-290, the gradient cache at release −1, `apply_dirichlet_bc`, the
// explicit consistent-mass options) and `df07_*`.

use akantu_core::enums::{
    AnalysisMethod, EventPriority, NonLinearSolverType, SpatialDirection, TimeStepSolverType,
};
use akantu_core::{Element, ElementType, Error, GhostType, Idx, Real};
use akantu_diffusion::{DiffusionUpstreamCompat, HeatTransferModel};
use akantu_mesh::{
    Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, MeshTransaction, SharedMesh,
};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::Parser;

fn heat_dat(k: &str, rho: Real, c: Real) -> String {
    format!(
        "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = m\n    \
         density = {rho}\n    capacity = {c}\n    conductivity = {k}\n  ]\n]\n"
    )
}

/// 1-D bar `[0, n·h]` of `n` segments.
fn bar(n: usize, h: Real) -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for i in 0..=n {
            a.push_node(&[i as Real * h]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        for i in 0..n {
            c.push_tuple(&[i as Idx, (i + 1) as Idx]);
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// Unit square cut along the diagonal (1,0)-(0,1), the diagonal nodes
/// doubled (nodes 4, 5), optionally one `_cohesive_2d_4` between the two
/// triangles.
fn square(with_cohesive: bool) -> Mesh {
    let mut m = Mesh::new(2, "square").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for p in [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 1.0],
        ] {
            a.push_node(&p).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[4, 3, 5]);
        if with_cohesive {
            drop(c);
            let mut c = a.connectivity_mut(ElementType::Cohesive2d4, GhostType::NotGhost);
            c.push_tuple(&[1, 2, 4, 5]);
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

fn build(mesh: Mesh, dat: &str) -> HeatTransferModel {
    let parser = Parser::parse_str(dat, "heat.dat").unwrap();
    HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser).unwrap()
}

fn options(method: AnalysisMethod) -> ModelOptions {
    let mut o = ModelOptions::new(method);
    if method != AnalysisMethod::ExplicitLumpedMass {
        o.solver_options.sparse_solver_type = Some(akantu_core::enums::SparseSolverType::Eigen);
    }
    o
}

fn model(mesh: Mesh, dat: &str, method: AnalysisMethod) -> HeatTransferModel {
    let mut m = build(mesh, dat);
    m.init_full(&options(method)).unwrap();
    m
}

fn bar_model(n: usize, method: AnalysisMethod) -> HeatTransferModel {
    model(bar(n, 0.25), &heat_dat("[[2.0]]", 2.5, 3.0), method)
}

/// `SharedMesh::mutate` with the model as the mesh's only consumer: the
/// model's own token is moved out for the call (a placeholder token of a
/// scratch domain keeps the field initialised), as in the SMM regressions.
fn mutate(
    m: &mut HeatTransferModel,
    change: impl FnOnce(&mut MeshTransaction<'_>) -> akantu_core::Result<()>,
) -> akantu_core::Result<()> {
    let shared = m.physics.base.mesh.clone();
    let scratch = SharedMesh::new(Mesh::new(m.spatial_dimension(), "scratch").unwrap());
    let placeholder = scratch
        .register_consumer("placeholder", EventPriority::MODEL, MeshPart::Main)
        .unwrap();
    let token = std::mem::replace(&mut m.0.token, placeholder);
    let r = shared
        .mutate(
            &mut [(&token, m as &mut dyn MeshEventHandler)],
            change,
            None,
        )
        .map(|_| ());
    m.0.token = token;
    r
}

fn values(a: &akantu_core::Array<Real>) -> Vec<Real> {
    a.as_slice().to_vec()
}

/* --------------------------------------------------------------------- */
/* D-068: the capability check states the dispatch's refusals            */
/* --------------------------------------------------------------------- */

/// D-068: node removal (refused by the model level of the dispatch,
/// UD-209) is refused by the capability check before the commit: the
/// mutation is rolled back, the model arrays are untouched, and the
/// domain and the model keep working.
#[test]
fn d068_node_removal_is_refused_before_commit() {
    // a bar with one unused node: `purify` removes it (`NodesRemoved`)
    let mut mesh = bar(4, 0.25);
    MeshAccessor::new(&mut mesh).push_node(&[9.0]).unwrap();
    let mut m = model(
        mesh,
        &heat_dat("[[2.0]]", 2.5, 3.0),
        AnalysisMethod::ExplicitLumpedMass,
    );
    let shared = m.physics.base.mesh.clone();
    let revision = shared.revision().unwrap();
    let t_before = values(&m.temperature().unwrap());
    let dt = m.stable_time_step().unwrap();
    let r = mutate(&mut m, |tx| tx.purify(MeshPart::Main));
    let Err(e) = &r else {
        panic!("expected the refusal, got {r:?}");
    };
    assert!(matches!(e, Error::NotImplemented(_)), "{e:?}");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert!(format!("{e}").contains("node removal"), "{e}");
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(
        shared.read().unwrap().nb_nodes(),
        6,
        "the unused node is kept"
    );
    assert_eq!(values(&m.temperature().unwrap()), t_before);
    // the domain and the model keep working
    mutate(&mut m, |_| Ok(())).unwrap();
    assert_eq!(m.stable_time_step().unwrap().to_bits(), dt.to_bits());
}

/// D-068 forwarding obligation: the model dispatches to its DOF manager
/// (level 20) and forwards the manager's check; the DOF manager refuses
/// `MeshIsDistributed` after its DOFs were numbered on the undistributed
/// mesh, and the model (both handlers) reports the same refusal.
#[test]
fn d068_check_forwards_to_the_dof_manager() {
    let m = bar_model(4, AnalysisMethod::Static);
    let mut distributed = bar(4, 0.25);
    MeshAccessor::new(&mut distributed).set_distributed();
    let ev = MeshEvent::MeshIsDistributed {
        origin: "v102".into(),
    };
    let direct = m
        .dof_manager()
        .check_mesh_event(&distributed, &ev)
        .map_err(|e| e.to_string());
    assert!(direct.is_err(), "{direct:?}");
    let via_heat =
        MeshEventHandler::check_mesh_event(&m, &distributed, &ev).map_err(|e| e.to_string());
    assert_eq!(via_heat, direct);
    let via_diffusion =
        MeshEventHandler::check_mesh_event(&m.0, &distributed, &ev).map_err(|e| e.to_string());
    assert_eq!(via_diffusion, direct);
}

/* --------------------------------------------------------------------- */
/* UD-811: non-regular element kinds                                     */
/* --------------------------------------------------------------------- */

/// UD-811: upstream's `initFull` throws at law assignment for a cohesive
/// element of the model dimension (`No element of type
/// (not_ghost:_cohesive_2d_4) in this ElementTypeMapArray<int> class
/// ("heat_transfer:0:heat_diffusion:element_filter")`, oracle
/// case `ud283_cohesive_init`); the port refuses with the same error class
/// (a missing element type block).
#[test]
fn ud811_cohesive_elements_are_refused_at_init_full() {
    let mut m = build(
        square(true),
        &heat_dat("[[2.0, 0.0], [0.0, 2.0]]", 2.5, 3.0),
    );
    let r = m.init_full(&options(AnalysisMethod::ExplicitLumpedMass));
    let Err(e) = &r else {
        panic!("expected the refusal, got {r:?}");
    };
    assert!(matches!(e, Error::NotFound { .. }), "{e:?}");
    let msg = e.to_string();
    assert!(
        msg.contains(&ElementType::Cohesive2d4.to_string()) && msg.contains("UD-811"),
        "{msg}"
    );
}

/// UD-811: an `ElementsAdded` event with a cohesive element of the model
/// dimension is refused by the capability check before the commit
/// (upstream: `onElementsAdded` → `addElement` throws during the dispatch).
#[test]
fn ud811_added_cohesive_elements_are_refused_before_commit() {
    let mut m = model(
        square(false),
        &heat_dat("[[2.0, 0.0], [0.0, 2.0]]", 2.5, 3.0),
        AnalysisMethod::ExplicitLumpedMass,
    );
    let shared = m.physics.base.mesh.clone();
    let revision = shared.revision().unwrap();
    let r = mutate(&mut m, |tx| {
        let e = tx.push_element(
            MeshPart::Main,
            ElementType::Cohesive2d4,
            GhostType::NotGhost,
            &[1, 2, 4, 5],
        )?;
        tx.record(
            MeshPart::Main,
            MeshEvent::ElementsAdded {
                elements: vec![e],
                constitutive_law: None,
                origin: "v102".into(),
                extension: None,
            },
        );
        Ok(())
    });
    let Err(e) = &r else {
        panic!("expected the refusal, got {r:?}");
    };
    assert!(matches!(e, Error::NotFound { .. }), "{e:?}");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(
        shared
            .read()
            .unwrap()
            .nb_element(ElementType::Cohesive2d4, GhostType::NotGhost),
        0
    );
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.5 * dt, None).unwrap();
    m.solve_step(None).unwrap();
}

/// UD-283 / UD-811: a cohesive element appended without an event (the
/// port's analogue of upstream's raw `MeshAccessor` after `initFull`, the
/// only path that reaches it upstream: oracle `ud283_cohesive_dt`, Δt
/// 0.142802 → 0) leaves the default stable time step unchanged (regular
/// kinds) and gives 0 in compat mode.
#[test]
fn ud283_raw_cohesive_element_zeroes_dt_only_in_compat_mode() {
    let mut m = model(
        square(false),
        &heat_dat("[[2.0, 0.0], [0.0, 2.0]]", 2.5, 3.0),
        AnalysisMethod::ExplicitLumpedMass,
    );
    let dt0 = m.stable_time_step().unwrap();
    // regular elements only: h = inradius of the right isosceles triangle
    // of legs 1 (the port's 2·r convention, as upstream's _triangle_3)
    let h = 2.0 - 2.0f64.sqrt();
    let want = 2.0 * h * h / 4.0 * 2.5 * 3.0 / 2.0;
    assert!((dt0 - want).abs() <= 1e-14 * want, "{dt0} vs {want}");
    mutate(&mut m, |tx| {
        tx.push_element(
            MeshPart::Main,
            ElementType::Cohesive2d4,
            GhostType::NotGhost,
            &[1, 2, 4, 5],
        )
        .map(|_| ())
    })
    .unwrap();
    assert_eq!(m.stable_time_step().unwrap().to_bits(), dt0.to_bits());
    m.physics.upstream_compat = DiffusionUpstreamCompat {
        stable_time_step_includes_all_kinds: true,
        ..DiffusionUpstreamCompat::default()
    };
    assert_eq!(m.stable_time_step().unwrap(), 0.0, "upstream: Δt = 0");
}

/* --------------------------------------------------------------------- */
/* Mesh events (UD-290, law bookkeeping)                                 */
/* --------------------------------------------------------------------- */

/// UD-290 + `laws_on_elements_added`: a segment and its new node appended
/// at the end of an explicit bar. The flows grow with the nodes (upstream
/// keeps their old size), the new element is assigned to the law (law
/// index 0, next law-local number, internals resized), and the capacities
/// are re-flagged: the lumped capacity of the next solve covers the new
/// element (`Σ M_i = ρc · L'`).
#[test]
fn ud290_added_nodes_and_elements_resize_flows_reflag_capacities_and_assign_laws() {
    let n = 4;
    let h = 0.25;
    let mut m = bar_model(n, AnalysisMethod::ExplicitLumpedMass);
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.2 * dt, None).unwrap();
    m.solve_step(None).unwrap();
    let rc = 2.5 * 3.0;
    let lumped0: Real = m.lumped_capacity().unwrap().as_slice().iter().sum();
    assert!((lumped0 - rc * n as Real * h).abs() <= 1e-13 * lumped0);
    let mut added = None;
    mutate(&mut m, |tx| {
        let node = tx.push_node(MeshPart::Main, &[(n + 1) as Real * h])?;
        let e = tx.push_element(
            MeshPart::Main,
            ElementType::Segment2,
            GhostType::NotGhost,
            &[n as Idx, node],
        )?;
        added = Some((node, e));
        tx.record(
            MeshPart::Main,
            MeshEvent::ElementsAdded {
                elements: vec![e],
                constitutive_law: None,
                origin: "v102".into(),
                extension: None,
            },
        );
        tx.record(
            MeshPart::Main,
            MeshEvent::NodesAdded {
                nodes: vec![node],
                old_nodes: None,
                origin: "v102".into(),
                extension: None,
            },
        );
        Ok(())
    })
    .unwrap();
    let (node, e) = added.unwrap();
    let nb_nodes = n + 2;
    assert_eq!(m.external_flow().unwrap().len(), nb_nodes);
    assert_eq!(m.internal_flow().unwrap().len(), nb_nodes);
    assert_eq!(m.temperature().unwrap().len(), nb_nodes);
    assert_eq!(m.law_by_element().element_to_vec(e).unwrap(), vec![0]);
    assert_eq!(m.law_local_numbering().element(e).unwrap(), &[n as Idx]);
    let nq = m
        .law_internal(0, "∇u", ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .len();
    assert_eq!(
        nq,
        n + 1,
        "∇u resized to the new element count (1 point each)"
    );
    m.temperature_mut().unwrap().as_mut_slice()[node as usize] = 1.0;
    m.solve_step(None).unwrap();
    let lumped: Vec<Real> = values(&m.lumped_capacity().unwrap());
    assert_eq!(lumped.len(), nb_nodes);
    let sum: Real = lumped.iter().sum();
    let want = rc * (n + 1) as Real * h;
    assert!(
        (sum - want).abs() <= 1e-13 * want,
        "lumped capacity reassembled after the event: Σ = {sum} vs {want}"
    );
    assert!(
        m.temperature_rate()
            .unwrap()
            .as_slice()
            .iter()
            .all(|v| v.is_finite())
    );
}

/// `laws_on_elements_removed`: erasing the first segment of a bar (whose
/// first cell is covered twice: element `n` duplicates element 0, so no
/// node is orphaned) moves the last element into its slot; the law's
/// filter and the handler maps follow consistently (every element's
/// law-local number points back to it), the internals are resized, and
/// the capacities and a solve see the remaining elements only.
#[test]
fn removed_elements_renumber_the_law_bookkeeping() {
    let n = 5;
    let h = 0.25;
    let mut mesh = bar(n, h);
    MeshAccessor::new(&mut mesh)
        .connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
        .push_tuple(&[0, 1]);
    let mut m = model(mesh, &heat_dat("[[2.0]]", 2.5, 3.0), AnalysisMethod::Static);
    mutate(&mut m, |tx| {
        tx.erase_elements(
            MeshPart::Main,
            &[Element::new(ElementType::Segment2, 0, GhostType::NotGhost)],
        )
    })
    .unwrap();
    let ty = ElementType::Segment2;
    let ng = GhostType::NotGhost;
    let filter: Vec<Idx> = m.law(0).unwrap().base().filter(ty, ng).to_vec();
    assert_eq!(filter.len(), n);
    assert_eq!(
        m.physics
            .base
            .mesh
            .read()
            .unwrap()
            .element_connectivity(Element::new(ty, 0, ng))
            .unwrap(),
        &[0, 1],
        "the duplicate moved into slot 0"
    );
    for (local, &el) in filter.iter().enumerate() {
        let e = Element::new(ty, el, ng);
        assert_eq!(m.law_by_element().element_to_vec(e).unwrap(), vec![0]);
        assert_eq!(m.law_local_numbering().element(e).unwrap(), &[local as Idx]);
    }
    assert_eq!(
        m.law_internal(0, "∇u", ty, ng).unwrap().len(),
        n,
        "∇u resized"
    );
    m.assemble_capacity_matrix_lumped().unwrap();
    let sum: Real = m.lumped_capacity().unwrap().as_slice().iter().sum();
    let want = 2.5 * 3.0 * n as Real * h;
    assert!((sum - want).abs() <= 1e-13 * want, "{sum} vs {want}");
    // a steady linear profile between the blocked ends is nodally exact
    {
        let mut b = m.blocked_dofs_mut().unwrap();
        b.as_mut_slice()[0] = true;
        b.as_mut_slice()[n] = true;
    }
    {
        let mut t = m.temperature_mut().unwrap();
        t.as_mut_slice()[0] = 10.0;
        t.as_mut_slice()[n] = 20.0;
    }
    m.solve_step(None).unwrap();
    let t = values(&m.temperature().unwrap());
    for (i, ti) in t.iter().enumerate() {
        let want = 10.0 + 10.0 * i as Real / n as Real;
        assert!((ti - want).abs() <= 1e-12 * 20.0, "T[{i}] = {ti} vs {want}");
    }
}

/// The diffusion twin of the solid model's
/// `review_v134_element_composition.rs::v134_erasure_keeps_the_law_maps_of_
/// the_other_blocks`): an erasure renumbers the law maps of the erased
/// block only (upstream's `ConstitutiveLaw::onElementsRemoved`,
/// `constitutive_law_tmpl.hh:344-400`, resizes the types of
/// `new_numbering`). On a mixed triangle / quadrangle mesh, erasing a
/// triangle leaves the quadrangles' law index and law-local numbering
/// unchanged (before a later fix they were reset to `-1`), and every
/// map agrees with the law's filters afterwards.
#[test]
fn lr10_03_erasure_keeps_the_law_maps_of_the_other_blocks() {
    let tri = ElementType::Triangle3;
    let quad = ElementType::Quadrangle4;
    let ng = GhostType::NotGhost;
    let mut mesh = Mesh::new(2, "mixed").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for p in [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [2.0, 0.0],
            [2.0, 1.0],
        ] {
            a.push_node(&p).unwrap();
        }
        let mut c = a.connectivity_mut(tri, ng);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[0, 2, 3]);
        drop(c);
        let mut q = a.connectivity_mut(quad, ng);
        q.push_tuple(&[1, 4, 5, 2]);
        q.push_tuple(&[4, 5, 2, 1]);
    }
    mesh.compute_bounding_box().unwrap();
    let mut m = model(
        mesh,
        &heat_dat("[[2.0, 0.0], [0.0, 2.0]]", 2.5, 3.0),
        AnalysisMethod::Static,
    );
    // the maps of every element of `ty`, and their agreement with the
    // law's filter (each element's law-local number points back to it)
    let maps = |m: &HeatTransferModel, ty: ElementType| -> Vec<(Vec<Idx>, Vec<Idx>)> {
        let n = m.physics.base.mesh.read().unwrap().nb_element(ty, ng);
        (0..n)
            .map(|e| {
                let el = Element::new(ty, e as Idx, ng);
                let law = m.law_by_element().element_to_vec(el).unwrap();
                let local = m.law_local_numbering().element(el).unwrap().to_vec();
                let l = usize::try_from(law[0]).unwrap_or_else(|_| {
                    panic!("{ty:?} element {e}: no law ({law:?})");
                });
                let k = usize::try_from(local[0]).unwrap();
                assert_eq!(
                    m.law(l).unwrap().base().filter(ty, ng)[k],
                    e as Idx,
                    "{ty:?} element {e}"
                );
                (law, local)
            })
            .collect()
    };
    let quads_before = maps(&m, quad);
    assert_eq!(quads_before.len(), 2);
    mutate(&mut m, |tx| {
        tx.erase_elements(MeshPart::Main, &[Element::new(tri, 0, ng)])
    })
    .unwrap();
    assert_eq!(
        maps(&m, quad),
        quads_before,
        "the quadrangles keep their maps"
    );
    let tris = maps(&m, tri);
    assert_eq!(
        tris,
        vec![(vec![0], vec![0])],
        "the kept triangle renumbered"
    );
    assert_eq!(m.law(0).unwrap().base().filter(tri, ng).to_vec(), vec![0]);
}

/* --------------------------------------------------------------------- */
/* Thermal energy by node, gradient cache, BCs, solver options           */
/* --------------------------------------------------------------------- */

/// UD-289: `thermal_energy_by_node()` = `Σ_n q_int[n] · Δt` over the local
/// and master nodes (serial: every node). The distributed counterpart
/// (slave nodes excluded, mutant M14) is checked by the parallel scenarios
/// (`*_distributed_vs_serial`, `energy_by_node`).
#[test]
fn ud289_thermal_energy_by_node_is_the_flow_sum_times_dt() {
    let mut m = bar_model(6, AnalysisMethod::ExplicitLumpedMass);
    let dt = 0.2 * m.stable_time_step().unwrap();
    m.set_time_step(dt, None).unwrap();
    {
        let mut t = m.temperature_mut().unwrap();
        for (i, v) in t.as_mut_slice().iter_mut().enumerate() {
            *v = (i * i) as Real;
        }
    }
    m.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
    m.solve_step(None).unwrap();
    let q = values(&m.internal_flow().unwrap());
    let mut want = 0.0;
    for v in &q {
        want += v * dt;
    }
    let got = m.thermal_energy_by_node().unwrap();
    assert_eq!(got.to_bits(), want.to_bits(), "{got} vs {want}");
    assert!(q.iter().any(|v| *v != 0.0));
}

/// UD-284 (release −1, mutant M16): a gradient block computed while the
/// model's release is still −1 (no predictor yet) is recomputed by the next
/// assembly (upstream `diffusivity_release != -1 and … == release`), so a
/// temperature changed between two residual assemblies before the first
/// solve is seen: the internal flow equals the one of a fresh model.
#[test]
fn ud284_blocks_cached_at_release_minus_one_are_recomputed() {
    let set = |m: &mut HeatTransferModel, f: &dyn Fn(usize) -> Real| {
        let mut t = m.temperature_mut().unwrap();
        for (i, v) in t.as_mut_slice().iter_mut().enumerate() {
            *v = f(i);
        }
    };
    let t1 = |i: usize| 1.0 + i as Real;
    let t2 = |i: usize| 3.0 * (i * i) as Real;
    let mut m = bar_model(5, AnalysisMethod::Static);
    assert_eq!(m.diffusion_release(), -1);
    set(&mut m, &t1);
    m.assemble_residual().unwrap();
    let q1 = values(&m.internal_flow().unwrap());
    assert_eq!(m.diffusion_release(), -1, "no predictor / corrector yet");
    set(&mut m, &t2);
    m.assemble_residual().unwrap();
    let q2 = values(&m.internal_flow().unwrap());
    let mut fresh = bar_model(5, AnalysisMethod::Static);
    set(&mut fresh, &t2);
    fresh.assemble_residual().unwrap();
    let want = values(&fresh.internal_flow().unwrap());
    assert_ne!(q1, q2);
    assert_eq!(
        q2.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        want.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "recomputed from the new temperature"
    );
}

/// Port extension `apply_dirichlet_bc`: the nodes of an element group are
/// blocked and set to the functor's value; other nodes are untouched; an
/// unknown group is an error.
#[test]
fn apply_dirichlet_bc_blocks_and_sets_the_group_nodes() {
    let mut mesh = bar(4, 0.25);
    {
        let mut g = mesh.groups_mut().create_element_group("left", 1).unwrap();
        g.add(
            Element::new(ElementType::Segment2, 0, GhostType::NotGhost),
            None,
            false,
        )
        .unwrap();
        g.add_node(0, false);
        g.add_node(1, false);
    }
    let mut m = model(mesh, &heat_dat("[[2.0]]", 2.5, 3.0), AnalysisMethod::Static);
    m.apply_dirichlet_bc(
        &FixedValue {
            value: 7.5,
            axis: SpatialDirection::X,
        },
        "left",
    )
    .unwrap();
    let b = m.blocked_dofs().unwrap().as_slice().to_vec();
    let t = values(&m.temperature().unwrap());
    assert_eq!(b, vec![true, true, false, false, false]);
    assert_eq!(&t[..2], &[7.5, 7.5]);
    assert!(t[2..].iter().all(|&v| v == 0.0));
    assert!(
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 1.0,
                axis: SpatialDirection::X
            },
            "nowhere"
        )
        .is_err()
    );
}

/// `getDefaultSolverOptions(_dynamic)` with `_explicit_consistent_mass`:
/// Newton–Raphson, forward Euler, temperature-rate solution (upstream's
/// `initFull(_explicit_consistent_mass)` itself fails: its default solver
/// id is `unknown`, `_not_defined`). One step from `Ṫ⁰ = 0` leaves `T`
/// unchanged and sets `Ṫ¹ = M⁻¹(f − K T⁰)` — the consistent rate of
/// [`HeatTransferModel::set_consistent_initial_rate`] (same Newton system).
#[test]
fn explicit_consistent_mass_options_solve_for_the_consistent_rate() {
    let n = 8;
    let sine = |m: &mut HeatTransferModel| {
        let mut t = m.temperature_mut().unwrap();
        for (i, v) in t.as_mut_slice().iter_mut().enumerate() {
            *v = (std::f64::consts::PI * i as Real / n as Real).sin();
        }
    };
    let mut ecm = build(bar(n, 1.0 / n as Real), &heat_dat("[[1.0]]", 1.0, 1.0));
    let r = ecm.init_full(&options(AnalysisMethod::ExplicitConsistentMass));
    assert!(
        r.is_err(),
        "upstream: unknown solver for this method, {r:?}"
    );
    let mut ecm = model(
        bar(n, 1.0 / n as Real),
        &heat_dat("[[1.0]]", 1.0, 1.0),
        AnalysisMethod::ImplicitDynamic,
    );
    ecm.physics.base.method = AnalysisMethod::ExplicitConsistentMass;
    let mut o = akantu_solver::ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Dynamic),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        ..akantu_solver::ModelSolverOptions::default()
    };
    o.sparse_solver_type = Some(akantu_core::enums::SparseSolverType::Eigen);
    let defaults = akantu_solver::SolverCallback::default_solver_options(
        &ecm.physics,
        TimeStepSolverType::Dynamic,
    );
    assert_eq!(
        defaults.integration_scheme_type.get("temperature"),
        Some(&akantu_core::enums::IntegrationSchemeType::ForwardEuler)
    );
    assert_eq!(
        defaults.solution_type.get("temperature"),
        Some(&akantu_solver::SolutionType::TEMPERATURE_RATE)
    );
    ecm.get_new_solver("ecm", &o).unwrap();
    ecm.set_integration_scheme(
        "ecm",
        "temperature",
        akantu_core::enums::IntegrationSchemeType::ForwardEuler,
        akantu_solver::SolutionType::TEMPERATURE_RATE,
    )
    .unwrap();
    sine(&mut ecm);
    {
        let mut b = ecm.blocked_dofs_mut().unwrap();
        b.as_mut_slice()[0] = true;
        b.as_mut_slice()[n] = true;
    }
    let t0 = values(&ecm.temperature().unwrap());
    ecm.set_time_step(1e-3, Some("ecm")).unwrap();
    ecm.solve_step(Some("ecm")).unwrap();
    assert_eq!(
        values(&ecm.temperature().unwrap()),
        t0,
        "forward Euler from Ṫ⁰ = 0"
    );
    let rate = values(&ecm.temperature_rate().unwrap());
    let mut api = model(
        bar(n, 1.0 / n as Real),
        &heat_dat("[[1.0]]", 1.0, 1.0),
        AnalysisMethod::ImplicitDynamic,
    );
    sine(&mut api);
    {
        let mut b = api.blocked_dofs_mut().unwrap();
        b.as_mut_slice()[0] = true;
        b.as_mut_slice()[n] = true;
    }
    api.set_consistent_initial_rate().unwrap();
    assert_eq!(values(&api.temperature().unwrap()), t0, "T untouched");
    let rate_api = values(&api.temperature_rate().unwrap());
    let scale = rate.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    for (a, b) in rate.iter().zip(&rate_api) {
        assert!((a - b).abs() <= 1e-12 * scale, "{a} vs {b}");
    }
    // the sampled sine is an eigenvector of K and the consistent M:
    // Ṫ = −λ T with λ = (6/h²)(1 − cos πh)/(2 + cos πh)
    let hh = 1.0 / n as Real;
    let th = std::f64::consts::PI * hh;
    let lambda = 6.0 / (hh * hh) * (1.0 - th.cos()) / (2.0 + th.cos());
    for i in 1..n {
        assert!(
            (rate_api[i] + lambda * t0[i]).abs() <= 1e-12 * lambda,
            "Ṫ[{i}] = {} vs {}",
            rate_api[i],
            -lambda * t0[i]
        );
    }
}

/// An out-of-range connectivity entry never reaches the stable time
/// step's coordinate gathering (which now returns an error instead of
/// slicing): the model's construction refuses the mesh already.
#[test]
fn df07_out_of_range_connectivity_is_refused_without_panic() {
    let mut mesh = bar(2, 0.5);
    MeshAccessor::new(&mut mesh)
        .connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
        .push_tuple(&[1, 7]);
    let parser = Parser::parse_str(&heat_dat("[[1.0]]", 1.0, 1.0), "heat.dat").unwrap();
    let r = HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser);
    assert!(
        matches!(r, Err(Error::OutOfRange { .. })),
        "{:?}",
        r.map(|_| ())
    );
}
