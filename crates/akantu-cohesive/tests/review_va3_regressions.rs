// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// with hand-derivable expectations: the cohesive `initFull`
// time step (upstream solid_mechanics_model_cohesive.cc
// `initFullImpl`), the nodal arrays copied to doubled nodes
// (`onNodesAdded`), `insertElements(only_double_facets = true)`
// (cohesive_element_inserter.cc / cohesive_element_inserter_helper.cc
// `insertFacetsOnly`), the `cohesive_zones` filter of `limitCheckFacets`
// and `addPhysicalVolume`, and `MaterialCohesiveRulesSelector`
// (material_selector_cohesive.cc).

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::selector::{MaterialCohesiveRules, MaterialCohesiveRulesSelector};
use akantu_core::array_shared::ArrayRef;
use akantu_core::enums::{AnalysisMethod, TimeStepSolverType};
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, PHYSICAL_NAMES};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ElementDataSelector, LawIndexSelector, SelectorChain};
use akantu_parser::Parser;
use akantu_solid::model::DOF_ID;

const Q4: ElementType = ElementType::Quadrangle4;
const S2: ElementType = ElementType::Segment2;
const C24: ElementType = ElementType::Cohesive2d4;
const NG: GhostType = GhostType::NotGhost;

/// Unit quadrangles stacked along y; quadrangle `k` spans `k ≤ y ≤ k + 1`
/// and carries the physical name `names[k]`.
fn stack(names: &[&str]) -> SharedMesh {
    let mut m = Mesh::new(2, "stack").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for k in 0..=names.len() {
            let y = k as Real;
            acc.push_node(&[0., y]).unwrap();
            acc.push_node(&[1., y]).unwrap();
        }
        let mut c = acc.connectivity_mut(Q4, NG);
        for k in 0..names.len() {
            let b = 2 * k as Idx;
            c.try_push_tuple(&[b, b + 1, b + 3, b + 2]).unwrap();
        }
    }
    let pn = m
        .data_mut()
        .strings_mut_or_insert(PHYSICAL_NAMES)
        .unwrap()
        .alloc(names.len(), 1, Q4, NG, String::new())
        .unwrap();
    for (v, n) in pn.as_mut_slice().iter_mut().zip(names) {
        *v = (*n).to_owned();
    }
    SharedMesh::new(m)
}

const BULK: &str =
    "  material elastic [\n    name = body\n    rho = 1\n    E = 1e6\n    nu = 0.\n  ]\n";

fn cohesive(name: &str, sigma_c: Real) -> String {
    format!(
        "  material cohesive_linear [\n    name = {name}\n    beta = 0\n    G_c = 0.5\n    sigma_c = {sigma_c}\n  ]\n"
    )
}

fn model_on(mesh: SharedMesh, dat: &str) -> SolidMechanicsModelCohesive {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    SolidMechanicsModelCohesive::with_parser(mesh, None, "solid_mechanics_model_cohesive", parser)
        .unwrap()
}

fn init(m: &mut SolidMechanicsModelCohesive, extrinsic: bool) {
    m.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        extrinsic,
    )
    .unwrap();
}

/// Index of the segment facet at height `y` (horizontal facets of the stack).
fn facet_at(m: &SolidMechanicsModelCohesive, y: Real) -> usize {
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    let fm = mesh.mesh_facets().unwrap();
    let conn = fm.connectivity(S2, NG).unwrap();
    let x = fm.nodes();
    (0..conn.len())
        .find(|&f| {
            let t = conn.tuple(f);
            t.iter().all(|&n| x.tuple(n as usize)[1] == y)
        })
        .unwrap_or_else(|| panic!("no horizontal facet at y = {y}"))
}

fn flags(a: &akantu_core::ElementTypeMapArray<bool>) -> Vec<bool> {
    a.get(S2, NG).unwrap().as_slice().to_vec()
}

/// Uniform tension `σ_yy = s` (u_y = s/E·y), a small `u_x = 1e-7·x` so that
/// the nodes of a facet differ, and the bulk stresses computed.
fn tension(m: &mut SolidMechanicsModelCohesive, s: Real) {
    let x = m
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    {
        let mut u_g = m.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for n in 0..x.len() / 2 {
            u[2 * n] = 1e-7 * x[2 * n];
            u[2 * n + 1] = s / 1e6 * x[2 * n + 1];
        }
    }
    m.assemble_internal_forces().unwrap();
}

/// Give every nodal array of the displacement DOFs except the displacement
/// itself a distinct non-zero value per entry (velocity, acceleration,
/// previous displacement, increment, solution) and an irregular blocked
/// pattern.
fn mark_nodal_state(m: &mut SolidMechanicsModelCohesive) {
    let fam = m.smm.dof_manager_mut().family_mut(DOF_ID).unwrap();
    let mut k = 0.0;
    let mut next = || {
        k += 1.0;
        k * 0.125 + 1.0
    };
    for d in fam.derivatives.values_mut() {
        let mut d_g = d.write().unwrap();
        for v in d_g.as_mut_slice() {
            *v = next();
        }
    }
    {
        let mut prev_g = fam
            .previous
            .as_mut()
            .expect("previous displacement registered (upstream initSolver)")
            .write()
            .unwrap();
        for v in prev_g.as_mut_slice() {
            *v = next();
        }
    }
    {
        let mut inc_g = fam
            .increment
            .as_mut()
            .expect("displacement increment registered (upstream initSolver)")
            .write()
            .unwrap();
        for v in inc_g.as_mut_slice() {
            *v = next();
        }
    }
    assert_eq!(fam.solution.len(), fam.dofs.len(), "solution sized per DOF");
    for v in fam.solution.as_mut_slice() {
        *v = next();
    }
    let mut blocked_g = fam
        .blocked
        .as_mut()
        .expect("blocked DOFs registered")
        .write()
        .unwrap();
    for (i, b) in blocked_g.as_mut_slice().iter_mut().enumerate() {
        *b = i % 3 == 1;
    }
}

/// `(old, new)` node pairs of a doubling: the new nodes are appended after
/// the `n_before` original nodes at the coordinates of their old node.
fn doubled_pairs(m: &SolidMechanicsModelCohesive, n_before: usize) -> Vec<(usize, usize)> {
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    let x = mesh.nodes();
    (n_before..x.len())
        .map(|n| {
            let o = (0..n_before)
                .find(|&o| x.tuple(o) == x.tuple(n))
                .unwrap_or_else(|| panic!("new node {n} has no coincident old node"));
            (o, n)
        })
        .collect()
}

/// Every nodal array of the displacement DOFs (upstream `onNodesAdded`:
/// displacement, blocked flags, velocity, acceleration, previous
/// displacement, increment, DOF-manager solution) holds at each new node
/// bitwise the row of its old node.
fn assert_copied(m: &SolidMechanicsModelCohesive, pairs: &[(usize, usize)], what: &str) {
    assert!(!pairs.is_empty(), "{what}: no doubled node");
    let fam = m.smm.dof_manager().family(DOF_ID).unwrap();
    let row = |a: &[Real], n: usize| {
        a[2 * n..2 * n + 2]
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    };
    let mut arrays: Vec<(&str, ArrayRef<'_, Real>)> =
        vec![("displacement", fam.dofs.read().unwrap())];
    for (order, d) in &fam.derivatives {
        arrays.push((
            if *order == 1 {
                "velocity"
            } else {
                "acceleration"
            },
            d.read().unwrap(),
        ));
    }
    arrays.push((
        "previous displacement",
        fam.previous.as_ref().unwrap().read().unwrap(),
    ));
    arrays.push(("increment", fam.increment.as_ref().unwrap().read().unwrap()));
    arrays.push(("solution", ArrayRef::from(&fam.solution)));
    // with the blocked flags below: the seven arrays of upstream onNodesAdded
    assert_eq!(
        arrays.len(),
        6,
        "{what}: displacement, velocity, acceleration, previous, increment, solution"
    );
    for (name, a_g) in arrays {
        let a = a_g.as_slice();
        assert_eq!(
            a.len(),
            2 * m.smm.physics.base.mesh.read().unwrap().nb_nodes(),
            "{what}: {name} resized"
        );
        for &(o, n) in pairs {
            assert_eq!(
                row(a, n),
                row(a, o),
                "{what}: {name} of new node {n} (old {o})"
            );
        }
    }
    let b_g = fam.blocked.as_ref().unwrap().read().unwrap();
    let b = b_g.as_slice();
    for &(o, n) in pairs {
        assert_eq!(
            &b[2 * n..2 * n + 2],
            &b[2 * o..2 * o + 2],
            "{what}: blocked of {n}"
        );
    }
}

// ---------------------------------------------------------------------------
// the time step the laws see after `init_full`
// ---------------------------------------------------------------------------

#[test]
fn au3_09_init_full_gives_the_laws_the_solver_time_step() {
    let dat = format!("{BULK}{}", cohesive("coh", 10.));
    // plain init_full: the TimeStepSolver default, 0, upstream and port
    let mut m = model_on(stack(&["a", "a"]), &dat);
    init(&mut m, true);
    assert_eq!(m.smm.time_step(None).unwrap(), 0.0);
    assert_eq!(m.smm.law_time_step().to_bits(), 0.0_f64.to_bits());
    // a solver created and given its time step before init_full (upstream:
    // initFull keeps an existing default solver; laws read getTimeStep(),
    // i.e. that solver's time step)
    let mut m = model_on(stack(&["a", "a"]), &dat);
    m.smm
        .get_new_solver_of_type("explicit_lumped", TimeStepSolverType::DynamicLumped, None)
        .unwrap();
    m.smm.solver.set_time_step(2.5e-4, None).unwrap();
    init(&mut m, true);
    assert_eq!(
        m.smm.time_step(None).unwrap().to_bits(),
        2.5e-4_f64.to_bits()
    );
    assert_eq!(
        m.smm.law_time_step().to_bits(),
        2.5e-4_f64.to_bits(),
        "the laws see the default solver's time step after init_full"
    );
}

// ---------------------------------------------------------------------------
// nodal arrays of doubled nodes
// ---------------------------------------------------------------------------

#[test]
fn au3_06_extrinsic_insertion_copies_every_nodal_array_to_the_doubled_nodes() {
    let _g = common::lock();
    let dat = format!("{BULK}{}", cohesive("coh", 10.));
    let mut m = model_on(stack(&["a", "a"]), &dat);
    init(&mut m, true);
    tension(&mut m, 20.);
    mark_nodal_state(&mut m);
    let n_before = m.smm.physics.base.mesh.read().unwrap().nb_nodes();
    assert_eq!(
        m.check_cohesive_stress().unwrap(),
        1,
        "the interface facet opens"
    );
    let pairs = doubled_pairs(&m, n_before);
    assert_eq!(pairs.len(), 2, "both interface nodes doubled");
    assert_copied(&m, &pairs, "extrinsic insertion");
}

// ---------------------------------------------------------------------------
// insertElements(only_double_facets = true)
// ---------------------------------------------------------------------------

#[test]
fn au3_02_insert_facets_only_doubles_facets_and_nodes_without_cohesive_elements() {
    let _g = common::lock();
    let dat = format!("{BULK}{}", cohesive("coh", 10.));
    let mut m = model_on(stack(&["a", "a"]), &dat);
    init(&mut m, true);
    tension(&mut m, 2.);
    mark_nodal_state(&mut m);
    let interface = facet_at(&m, 1.);
    let (n_nodes, n_facets) = {
        let mesh = m.smm.physics.base.mesh.read().unwrap();
        (
            mesh.nb_nodes(),
            mesh.mesh_facets().unwrap().nb_element(S2, NG),
        )
    };
    assert_eq!((n_nodes, n_facets), (6, 7));
    let check_before = flags(m.ext.inserter.check_facets());
    assert!(check_before[interface], "the interface facet is checkable");
    // nothing flagged: no transaction, nothing changes
    let rev = m.smm.physics.base.mesh.revision().unwrap();
    let r = m.insert_facets_only().unwrap();
    assert!(r.doubled_facets.is_empty() && r.doubled_nodes.is_empty());
    assert_eq!(m.smm.physics.base.mesh.revision().unwrap(), rev);
    m.ext
        .inserter
        .insertion_facets_mut()
        .get_mut(S2, NG)
        .unwrap()
        .as_mut_slice()[interface] = true;
    let r = m.insert_facets_only().unwrap();
    // upstream insertElements(true) returns the number of new cohesive
    // elements (updateGlobalData of the cohesive model): 0
    assert!(r.new_elements.is_empty());
    assert_eq!(r.doubled_facets.len(), 1);
    assert_eq!(
        r.doubled_facets[0].0,
        Element::try_new(S2, interface, NG).unwrap()
    );
    assert_eq!(
        r.doubled_facets[0].1,
        Element::try_new(S2, n_facets, NG).unwrap()
    );
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    mesh.check_consistency().unwrap();
    akantu_mesh::utils::check_facet_mesh_consistency(&mesh).unwrap();
    assert_eq!(mesh.nb_element(C24, NG), 0, "no cohesive element");
    assert_eq!(mesh.nb_nodes(), 8, "the two interface nodes doubled");
    let fm = mesh.mesh_facets().unwrap();
    assert_eq!(fm.nb_element(S2, NG), 8, "the interface facet doubled");
    // the old facet keeps the bottom quadrangle, the new one the top one;
    // both are now boundary facets
    let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
    let q = |k: usize| Element::try_new(Q4, k, NG).unwrap();
    assert_eq!(
        e2s.element(Element::try_new(S2, interface, NG).unwrap())
            .unwrap()[0],
        vec![q(0), Element::NULL]
    );
    assert_eq!(
        e2s.element(Element::try_new(S2, n_facets, NG).unwrap())
            .unwrap()[0],
        vec![q(1), Element::NULL]
    );
    // the top quadrangle uses the new nodes, the bottom one the old ones
    let conn = mesh.connectivity(Q4, NG).unwrap();
    assert_eq!(conn.tuple(0), &[0, 1, 3, 2]);
    let top: Vec<Idx> = conn.tuple(1).to_vec();
    assert!(
        !top.contains(&2) && !top.contains(&3),
        "top quadrangle {top:?}"
    );
    assert!(
        top.contains(&6) && top.contains(&7),
        "top quadrangle {top:?}"
    );
    let new_conn = fm.connectivity(S2, NG).unwrap().tuple(n_facets).to_vec();
    let mut sorted = new_conn.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![6, 7], "new facet nodes {new_conn:?}");
    drop(conn);
    drop(mesh);
    // The facet FE engine received the new facet
    // (upstream doubleFacets sends the facets' event, mutant N3): its
    // precomputed arrays cover the 8 facets, the new facet's shapes and
    // integration weights equal those of the facet it doubles (same nodes'
    // coordinates)
    let nq = m.ext.facet_fe.nb_integration_points(S2, NG).unwrap();
    let shapes = m.ext.facet_fe.shapes(S2, NG).unwrap().as_slice().to_vec();
    let jac = m
        .ext
        .facet_fe
        .jacobians(S2, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(jac.len(), 8 * nq, "facet FE jacobians cover the new facet");
    assert_eq!(
        shapes.len(),
        8 * nq * 2,
        "facet FE shapes cover the new facet"
    );
    for k in 0..nq {
        assert_eq!(
            jac[n_facets * nq + k].to_bits(),
            jac[interface * nq + k].to_bits()
        );
        for i in 0..2 {
            assert_eq!(
                shapes[(n_facets * nq + k) * 2 + i].to_bits(),
                shapes[(interface * nq + k) * 2 + i].to_bits()
            );
        }
    }
    let pairs = doubled_pairs(&m, n_nodes);
    assert_eq!(pairs.len(), 2);
    let r_pairs: Vec<(usize, usize)> = r
        .doubled_nodes
        .iter()
        .map(|&(o, n)| (o as usize, n as usize))
        .collect();
    assert_eq!(r_pairs, pairs, "the reported doubling");
    assert_copied(&m, &pairs, "insert_facets_only");
    // upstream updateInsertionFacets (extrinsic): the doubled facet is
    // neither flagged nor checked any more; both maps cover the new facet
    let ins = flags(m.ext.inserter.insertion_facets());
    let chk = flags(m.ext.inserter.check_facets());
    assert_eq!(ins.len(), 8);
    assert_eq!(chk.len(), 8);
    assert!(ins.iter().all(|f| !f));
    assert!(!chk[interface] && !chk[n_facets]);
    for f in 0..n_facets {
        if f != interface {
            assert_eq!(chk[f], check_before[f], "check flag of facet {f} unchanged");
        }
    }
    // no law received an element
    let coh = m.cohesive_material("coh").unwrap();
    assert!(
        akantu_solid::Material::base(coh)
            .element_filter
            .get(C24, NG)
            .map_or(true, |a| a.is_empty())
    );
    // the crack is free: an explicit step runs on the separated bodies
    m.set_time_step(1e-4, None).unwrap();
    m.solve_step(None).unwrap();
}

// ---------------------------------------------------------------------------
// cohesive_zones / addPhysicalVolume
// ---------------------------------------------------------------------------

/// The checkable interior facets of a three-quadrangle stack `a | a | b`
/// (interfaces at y = 1: a–a, y = 2: a–b) for a zone set.
fn checkable(zones_dat: Option<&str>, add_volumes: &[&str]) -> (bool, bool) {
    let inserter = zones_dat.map_or_else(String::new, |z| {
        format!("  cohesive_inserter [\n    cohesive_zones = [{z}]\n  ]\n")
    });
    let dat = format!(
        "model solid_mechanics_model_cohesive [\n{inserter}{BULK}{}]\n",
        cohesive("coh", 10.)
    );
    let mut m = model_on(stack(&["a", "a", "b"]), &dat);
    for v in add_volumes {
        m.ext.inserter.add_physical_volume(*v);
    }
    init(&mut m, true);
    let c = flags(m.ext.inserter.check_facets());
    // boundary facets are never checkable
    let (f1, f2) = (facet_at(&m, 1.), facet_at(&m, 2.));
    for (f, v) in c.iter().enumerate() {
        if f != f1 && f != f2 {
            assert!(!v, "boundary facet {f} checkable");
        }
    }
    (c[f1], c[f2])
}

#[test]
fn au3_02_cohesive_zones_keep_facets_whose_two_neighbours_are_in_the_zones() {
    let _g = common::lock();
    // no zones: every interior facet
    assert_eq!(checkable(None, &[]), (true, true));
    // parsed cohesive_zones (upstream registerParam "cohesive_zones")
    assert_eq!(checkable(Some("a"), &[]), (true, false));
    assert_eq!(checkable(Some("b"), &[]), (false, false));
    assert_eq!(checkable(Some("a, b"), &[]), (true, true));
    assert_eq!(checkable(Some("b, a"), &[]), (true, true));
    assert_eq!(checkable(Some("c"), &[]), (false, false));
    // addPhysicalVolume (no inserter section; initAutomaticInsertion's
    // limitCheckFacets applies the zones)
    assert_eq!(checkable(None, &["a"]), (true, false));
    assert_eq!(checkable(None, &["a", "b"]), (true, true));
    // a parsed section replaces the zone set added before init_full
    // (Parsable::parseSection sets the parameter)
    assert_eq!(checkable(Some("a"), &["b"]), (true, false));
}

/// The port's default behaviour after an insertion, **not upstream's**
/// (UD-808, B): with `cohesive_zones`, upstream's `limitCheckFacets` reads
/// `physical_names` of the cohesive neighbour of the doubled facets and
/// throws once a cohesive element exists (pinned-oracle case
/// `v92_ig_tg_zones_update_after_insertion`); the port
/// treats that neighbour as outside every zone — the facet is already
/// excluded by the cohesive-neighbour rule — and recomputes the flags. The
/// compat switch `zones_unnamed_neighbour_error` reproduces upstream's error
/// (second part).
#[test]
fn au3_02_cohesive_zones_limit_dynamic_insertion() {
    let _g = common::lock();
    let dat = format!(
        "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    cohesive_zones = [a]\n  ]\n{BULK}{}]\n",
        cohesive("coh", 10.)
    );
    let mut m = model_on(stack(&["a", "a", "b"]), &dat);
    init(&mut m, true);
    tension(&mut m, 20.);
    assert_eq!(
        m.check_cohesive_stress().unwrap(),
        1,
        "only the a–a facet opens"
    );
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    let conn = mesh.connectivity(C24, NG).unwrap();
    let x = mesh.nodes();
    for &n in conn.tuple(0) {
        assert_eq!(x.tuple(n as usize)[1], 1.0, "cohesive node {n} on y = 1");
    }
    drop(conn);
    drop(x);
    drop(mesh);
    // update_automatic_insertion after a zone change (upstream
    // updateAutomaticInsertion): the a–b facet becomes checkable (UD-808
    // default: upstream throws here, see below)
    m.ext.inserter.add_physical_volume("b");
    m.update_automatic_insertion().unwrap();
    let c = flags(m.ext.inserter.check_facets());
    assert!(c[facet_at(&m, 2.)]);
    // the doubled facets (cohesive neighbour) are not checkable
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    let fm = mesh.mesh_facets().unwrap();
    let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
    for (f, &v) in c.iter().enumerate() {
        let row = &e2s.element(Element::try_new(S2, f, NG).unwrap()).unwrap()[0];
        if row.iter().any(|e| e.ty() == C24) {
            assert!(!v, "facet {f} next to the cohesive element is checkable");
        }
    }
    drop(mesh);
    assert_eq!(
        m.check_cohesive_stress().unwrap(),
        1,
        "then the a–b facet opens"
    );

    // UD-808 compat: upstream's exception once a cohesive element exists
    // (before the first insertion the update succeeds)
    let mut m = model_on(stack(&["a", "a", "b"]), &dat);
    m.ext.compat.zones_unnamed_neighbour_error = true;
    init(&mut m, true);
    m.update_automatic_insertion().unwrap();
    tension(&mut m, 20.);
    assert_eq!(m.check_cohesive_stress().unwrap(), 1);
    m.ext.inserter.add_physical_volume("b");
    let Err(akantu_core::Error::Mesh(msg)) = m.update_automatic_insertion() else {
        panic!("UD-808 compat: update_automatic_insertion must fail after an insertion");
    };
    assert_eq!(
        msg,
        "No element of type (not_ghost:_cohesive_2d_4) in this \
         ElementTypeMapArray<std::__cxx11::basic_string<char, std::char_traits<char>, \
         std::allocator<char> >> class (\"stack:mesh_data:physical_names\")"
    );
}

// ---------------------------------------------------------------------------
// MaterialCohesiveRulesSelector
// ---------------------------------------------------------------------------

fn rules(pairs: &[(&str, &str, &str)]) -> MaterialCohesiveRules {
    pairs
        .iter()
        .map(|(a, b, m)| (((*a).to_owned(), (*b).to_owned()), (*m).to_owned()))
        .collect()
}

/// `facet_material` of every facet of the stack `names` after `init_full`
/// with the rules selector, and the law index of the facet at height y.
fn rules_model(
    names: &[&str],
    r: MaterialCohesiveRules,
    extrinsic: bool,
) -> Result<SolidMechanicsModelCohesive, akantu_core::Error> {
    // bulk laws named after the physical names (upstream example
    // cohesive_extrinsic_ig_tg), then two cohesive laws
    let dat = format!(
        "  material elastic [\n    name = a\n    rho = 1\n    E = 1e6\n    nu = 0.\n  ]\n\
         material elastic [\n    name = b\n    rho = 1\n    E = 1e6\n    nu = 0.\n  ]\n{}{}",
        cohesive("ig", 10.),
        cohesive("tg", 50.)
    );
    let mut m = model_on(stack(names), &dat);
    let sel = MaterialCohesiveRulesSelector {
        rules: r,
        mesh_data_id: PHYSICAL_NAMES.to_owned(),
        default: m.default_cohesive_selector(),
    };
    m.set_material_selector_chain(SelectorChain {
        selectors: vec![
            Box::new(sel),
            Box::new(ElementDataSelector::new(PHYSICAL_NAMES)),
            Box::new(m.default_cohesive_selector()),
            Box::new(LawIndexSelector),
        ],
        fallback_value: 0,
    });
    m.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        extrinsic,
    )?;
    Ok(m)
}

fn facet_materials(m: &SolidMechanicsModelCohesive) -> Vec<Idx> {
    m.ext
        .facet_material
        .get(S2, NG)
        .unwrap()
        .as_slice()
        .to_vec()
}

/// The two neighbours' names of every facet (`(a, a)` for a boundary
/// facet of an `a` element).
fn neighbour_names(m: &SolidMechanicsModelCohesive, names: &[&str]) -> Vec<(String, String)> {
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    let fm = mesh.mesh_facets().unwrap();
    let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
    (0..fm.nb_element(S2, NG))
        .map(|f| {
            let row = &e2s.element(Element::try_new(S2, f, NG).unwrap()).unwrap()[0];
            let n1 = names[row[0].idx()].to_owned();
            let n2 = if row.len() > 1 && !row[1].is_null() {
                names[row[1].idx()].to_owned()
            } else {
                n1.clone()
            };
            (n1, n2)
        })
        .collect()
}

#[test]
fn au3_02_rules_selector_assigns_facets_by_the_names_of_their_neighbours() {
    let _g = common::lock();
    let names = ["a", "a", "b"];
    // law indices: a 0, b 1, ig 2, tg 3
    let (ig, tg) = (2, 3);
    // direct pairs (a, b) and (a, a)
    let m = rules_model(&names, rules(&[("a", "b", "tg"), ("a", "a", "ig")]), true).unwrap();
    let fmat = facet_materials(&m);
    let nn = neighbour_names(&m, &names);
    for (f, (n1, n2)) in nn.iter().enumerate() {
        let expected = match (n1.as_str(), n2.as_str()) {
            ("a", "b") | ("b", "a") => tg,
            ("a", "a") => ig,
            // (b, b): no rule -> default cohesive selection: the facet
            // keeps the initial facet_material, the first cohesive law
            ("b", "b") => ig,
            other => panic!("{other:?}"),
        };
        assert_eq!(fmat[f], expected, "facet {f} ({n1}, {n2})");
    }
    assert_eq!(fmat[facet_at(&m, 2.)], tg, "a–b interface");
    assert_eq!(fmat[facet_at(&m, 1.)], ig, "a–a interface");
    // each law's facet filter holds exactly its facets (extrinsic)
    for (law, idx) in [("ig", ig), ("tg", tg)] {
        let c = m.cohesive_material(law).unwrap();
        let n = fmat.iter().filter(|&&x| x == idx).count();
        let filt: Vec<Idx> = c.core.facet_filter.get(S2, NG).unwrap().as_slice().to_vec();
        let mine: Vec<Idx> = (0..fmat.len())
            .filter(|&f| fmat[f] == idx)
            .map(|f| f as Idx)
            .collect();
        assert_eq!(filt.len(), n, "{law} facets");
        assert_eq!(filt, mine, "{law} facet filter");
    }
    // the reversed key: only (b, a) given; the a–b facet's neighbours are
    // (a, b) in element order, found by the second lookup
    let m = rules_model(&names, rules(&[("b", "a", "tg"), ("b", "b", "tg")]), true).unwrap();
    let fmat = facet_materials(&m);
    assert_eq!(nn[facet_at(&m, 2.)], ("a".to_owned(), "b".to_owned()));
    assert_eq!(fmat[facet_at(&m, 2.)], tg, "reversed lookup (b, a)");
    assert_eq!(fmat[facet_at(&m, 1.)], ig, "(a, a): no rule, default");
    // boundary facets of the b element: (b, b) -> tg
    for (f, (n1, n2)) in nn.iter().enumerate() {
        if n1 == "b" && n2 == "b" {
            assert_eq!(fmat[f], tg, "boundary facet {f} of b");
        }
    }
    // a rule naming no material: error (upstream getMaterialIndex throws)
    let Err(e) = rules_model(&names, rules(&[("a", "a", "missing")]), true) else {
        panic!("unknown material in a rule must be an error");
    };
    assert!(e.to_string().contains("missing"), "{e}");
    // bulk elements: by physical name (ElementDataSelector after the rules)
    let m = rules_model(&names, rules(&[("a", "b", "tg")]), true).unwrap();
    let li = m
        .smm
        .physics
        .materials
        .law_index_map()
        .get(Q4, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(li, vec![0, 0, 1]);
}

/// Upstream looks up `(id1, id2)` — the names of the
/// facet's first and second neighbour in `element_to_subelement` order —
/// **before** `(id2, id1)` (`material_selector_cohesive.cc:155-158`). With
/// both keys mapped to different laws the facet takes the direct key's law;
/// with only the reversed key, that one; with neither, the default cohesive
/// selection (the first cohesive law, `ig`), which differs from both rule
/// laws here, so each assertion distinguishes the lookup from the fallback.
#[test]
fn cg03_rules_selector_looks_up_the_direct_key_before_the_reversed_one() {
    let _g = common::lock();
    // law indices: a 0, b 1, ig 2, tg 3; a third cohesive law is not
    // needed: the boundary/same-name facets have no rule and take ig
    let (ig, tg) = (2, 3);
    // Each model is built, checked and dropped in its own helper call
    // (`cg03_facet_material`), so that one model is alive at a time: four
    // models shadowed in one frame needed 2.0–2.5 MiB of stack in an
    // unoptimised x86_64 build with `akantu-core/export-all`, above the
    // 2 MiB of a test thread (merge round). Same assertions.
    // stack a | b: the interface facet's neighbours are (a, b) in element
    // order; stack b | a: (b, a)
    for (names, order) in [(["a", "b"], ("a", "b")), (["b", "a"], ("b", "a"))] {
        let (nn, f, got) = cg03_facet_material(&names, &[("a", "b", "tg"), ("b", "a", "ig")]);
        assert_eq!(nn[f], (order.0.to_owned(), order.1.to_owned()), "{names:?}");
        let expected = if order == ("a", "b") { tg } else { ig };
        assert_eq!(
            got, expected,
            "{names:?}: both keys mapped, the direct key {order:?} wins"
        );
        // the same with the two laws exchanged
        let (_, f2, got) = cg03_facet_material(&names, &[("a", "b", "ig"), ("b", "a", "tg")]);
        assert_eq!(f2, f);
        let expected = if order == ("a", "b") { ig } else { tg };
        assert_eq!(got, expected, "{names:?}: exchanged laws");
        // only the key that is reversed for this facet: found by the
        // second lookup (tg), not the default (ig)
        let reversed = (order.1, order.0, "tg");
        let (_, f3, got) = cg03_facet_material(&names, &[reversed]);
        assert_eq!(f3, f);
        assert_eq!(got, tg, "{names:?}: reversed key {reversed:?} only");
        // no key for the pair: the default cohesive selection (ig)
        let (_, f4, got) = cg03_facet_material(&names, &[("a", "a", "tg")]);
        assert_eq!(f4, f);
        assert_eq!(got, ig, "{names:?}: no rule, default");
    }
}

/// One model of `cg03…`: the neighbour names of every facet, the interface
/// facet (`y = 1`) and the law index of that facet.
#[inline(never)]
fn cg03_facet_material(
    names: &[&str],
    pairs: &[(&str, &str, &str)],
) -> (Vec<(String, String)>, usize, Idx) {
    let m = rules_model(names, rules(pairs), true).unwrap();
    let nn = neighbour_names(&m, names);
    let f = facet_at(&m, 1.);
    let got = facet_materials(&m)[f];
    (nn, f, got)
}

#[test]
fn au3_02_rules_selector_dynamic_insertion_takes_the_facet_law() {
    let _g = common::lock();
    let names = ["a", "a", "b"];
    let mut m = rules_model(&names, rules(&[("a", "b", "tg"), ("a", "a", "ig")]), true).unwrap();
    // σ_yy = 20: above ig's σc = 10, below tg's 50
    tension(&mut m, 20.);
    assert_eq!(m.check_cohesive_stress().unwrap(), 1);
    assert_eq!(
        akantu_solid::Material::base(m.cohesive_material("ig").unwrap())
            .element_filter
            .get(C24, NG)
            .unwrap()
            .len(),
        1,
        "the a–a facet opens with ig"
    );
    assert!(
        akantu_solid::Material::base(m.cohesive_material("tg").unwrap())
            .element_filter
            .get(C24, NG)
            .map_or(true, |a| a.is_empty())
    );
    // σ_yy = 60: the a–b facet opens with tg
    tension(&mut m, 60.);
    assert_eq!(m.check_cohesive_stress().unwrap(), 1);
    assert_eq!(
        akantu_solid::Material::base(m.cohesive_material("tg").unwrap())
            .element_filter
            .get(C24, NG)
            .unwrap()
            .len(),
        1
    );
    let li = m
        .smm
        .physics
        .materials
        .law_index_map()
        .get(C24, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(li, vec![2, 3], "cohesive elements take their facet's law");
}

/// UD-250 (A): after a facets-only insertion upstream's
/// `checkCohesiveStress` writes the doubled facets' stresses past
/// `facet_stress` (resized only on `ElementsAdded`); the port resizes it, so
/// the check works (the doubled facets are boundary facets, never
/// checkable) instead of panicking.
#[test]
fn ud250_stress_check_after_a_facets_only_insertion() {
    let _g = common::lock();
    let dat = format!("{BULK}{}", cohesive("coh", 10.));
    let mut m = model_on(stack(&["a", "a", "a"]), &dat);
    init(&mut m, true);
    tension(&mut m, 2.);
    let f1 = facet_at(&m, 1.);
    m.ext
        .inserter
        .insertion_facets_mut()
        .get_mut(S2, NG)
        .unwrap()
        .as_mut_slice()[f1] = true;
    m.insert_facets_only().unwrap();
    let nf = m
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .mesh_facets()
        .unwrap()
        .nb_element(S2, NG);
    let nq = m.ext.facet_fe.nb_integration_points(S2, NG).unwrap();
    assert_eq!(
        m.ext.facet_stress.get(S2, NG).unwrap().len(),
        nf * nq,
        "one facet_stress entry per facet quadrature point"
    );
    // tension above σc: the remaining interior facet (y = 2) opens; the
    // doubled facets (y = 1) are not checkable
    tension(&mut m, 20.);
    assert_eq!(m.check_cohesive_stress().unwrap(), 1);
    let mesh = m.smm.physics.base.mesh.read().unwrap();
    let conn = mesh.connectivity(C24, NG).unwrap();
    for &n in conn.tuple(0) {
        assert_eq!(mesh.nodes().tuple(n as usize)[1], 2.0);
    }
}
