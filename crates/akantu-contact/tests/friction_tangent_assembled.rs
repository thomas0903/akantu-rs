// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the
// consistent friction tangent of
// src/model/contact_mechanics/resolutions/resolution_penalty.cc (UD-369
// correction) at the ASSEMBLED level:
// central finite differences of the assembled contact force
// against the assembled stiffness (DOF manager), over every DOF of the
// mesh, with the friction history and the nodal areas frozen; the friction
// part is isolated with a frictionless twin model at the same positions.
//
// The harness was written independently of the other tests; its
// assertions are extended
// to the facet-change cases now that the previous-facet coupling is
// assembled and the non-zero traction history test kept as the
// regression of the transport derivative.
// Env V81_ONLY=<substring> selects cases.

#![allow(clippy::needless_range_loop, clippy::cast_precision_loss)]

use akantu_contact::{
    ContactElement, ContactMechanicsModel, ContactUpstreamCompat, DeformableMasterWeighting,
    PhysicalSurfaceSelector, contact_state_code,
};
use akantu_core::enums::AnalysisMethod;
use akantu_core::{Array, Element, ElementType, GhostType, Real};
use akantu_mesh::utils::fill_element_to_subelements_data;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_parser::Parser;
use akantu_solver::DofManager;

// ------------------------------------------------------------------ meshes

fn group(m: &mut Mesh, name: &str, dim: usize, ty: ElementType, ids: &[usize]) {
    m.groups_mut().create_element_group(name, dim).unwrap();
    for &i in ids {
        let e = Element::try_new(ty, i, GhostType::NotGhost).unwrap();
        m.element_group_add(name, e, true, true).unwrap();
    }
    m.groups_mut().element_group_mut(name).unwrap().optimize();
}

/// 2-D: lower body `nl` quads on [-1,1]x[-1,0] (group `lower`, top edges
/// `master`), upper body `nu` quads on [-wu,wu]x[0,1] (`upper`, bottom
/// edges `slave`); distinct interface nodes.
fn mesh_2d(nl: usize, nu: usize, wu: Real) -> Mesh {
    let mut m = Mesh::new(2, "v81_2d").unwrap();
    let (q, s, g) = (
        ElementType::Quadrangle4,
        ElementType::Segment2,
        GhostType::NotGhost,
    );
    let (mut quads, mut segs) = (Vec::new(), Vec::new());
    let (mut ql, mut qu, mut sm, mut ss) = (vec![], vec![], vec![], vec![]);
    {
        let mut a = MeshAccessor::new(&mut m);
        let mut row = |n: usize, w: Real, y: Real| -> Vec<i32> {
            (0..=n)
                .map(|j| {
                    a.push_node(&[-w + 2.0 * w * j as Real / n as Real, y])
                        .unwrap()
                })
                .collect()
        };
        let (lb, lt) = (row(nl, 1.0, -1.0), row(nl, 1.0, 0.0));
        let (ub, ut) = (row(nu, wu, 0.0), row(nu, wu, 1.0));
        for j in 0..nl {
            ql.push(quads.len());
            quads.push([lb[j], lb[j + 1], lt[j + 1], lt[j]]);
            sm.push(segs.len());
            segs.push([lt[j], lt[j + 1]]);
        }
        for j in 0..nu {
            qu.push(quads.len());
            quads.push([ub[j], ub[j + 1], ut[j + 1], ut[j]]);
            ss.push(segs.len());
            segs.push([ub[j], ub[j + 1]]);
        }
        for c in &quads {
            a.connectivity_mut(q, g).push_tuple(c);
        }
        for c in &segs {
            a.connectivity_mut(s, g).push_tuple(c);
        }
    }
    group(&mut m, "lower", 2, q, &ql);
    group(&mut m, "upper", 2, q, &qu);
    group(&mut m, "master", 1, s, &sm);
    group(&mut m, "slave", 1, s, &ss);
    fill_element_to_subelements_data(&mut m).unwrap();
    m
}

/// 3-D: lower body `nl x nl` cells on [-1,1]^2 x [-1,0], upper `nu x nu`
/// cells on [-wu,wu]^2 x [0,1]; hexahedra (quadrangle facets) or five
/// tetrahedra per cell (triangle facets, alternating diagonal pattern).
fn mesh_3d(nl: usize, nu: usize, wu: Real, tets: bool) -> Mesh {
    let mut m = Mesh::new(3, "v81_3d").unwrap();
    let g = GhostType::NotGhost;
    let (vt, ft) = if tets {
        (ElementType::Tetrahedron4, ElementType::Triangle3)
    } else {
        (ElementType::Hexahedron8, ElementType::Quadrangle4)
    };
    let (mut vols, mut facs): (Vec<Vec<i32>>, Vec<Vec<i32>>) = (vec![], vec![]);
    let (mut vl, mut vu, mut fm, mut fs) = (vec![], vec![], vec![], vec![]);
    let mut xyz: Vec<[Real; 3]> = Vec::new();
    {
        let mut a = MeshAccessor::new(&mut m);
        for (n, w, z0, lower) in [(nl, 1.0, -1.0, true), (nu, wu, 0.0, false)] {
            let h = 2.0 * w / n as Real;
            let mut id = vec![vec![[0i32; 2]; n + 1]; n + 1];
            for i in 0..=n {
                for j in 0..=n {
                    for k in 0..2 {
                        let x = [-w + h * i as Real, -w + h * j as Real, z0 + k as Real];
                        id[i][j][k] = a.push_node(&x).unwrap();
                        xyz.push(x);
                    }
                }
            }
            for i in 0..n {
                for j in 0..n {
                    // hex order: bottom 0..3 counter-clockwise, top 4..7
                    let c = [
                        id[i][j][0],
                        id[i + 1][j][0],
                        id[i + 1][j + 1][0],
                        id[i][j + 1][0],
                        id[i][j][1],
                        id[i + 1][j][1],
                        id[i + 1][j + 1][1],
                        id[i][j + 1][1],
                    ];
                    let face = if lower {
                        [c[4], c[5], c[6], c[7]]
                    } else {
                        [c[0], c[1], c[2], c[3]]
                    };
                    let (vlist, flist) = if lower {
                        (&mut vl, &mut fm)
                    } else {
                        (&mut vu, &mut fs)
                    };
                    if tets {
                        // five-tetrahedron split, mirrored on odd cells so
                        // that neighbouring faces conform
                        let odd = (i + j) % 2 == 1;
                        let t5: [[usize; 4]; 5] = if odd {
                            [
                                [1, 0, 2, 5],
                                [3, 2, 0, 7],
                                [4, 5, 7, 0],
                                [6, 7, 5, 2],
                                [0, 2, 5, 7],
                            ]
                        } else {
                            [
                                [0, 1, 3, 4],
                                [2, 3, 1, 6],
                                [5, 4, 6, 1],
                                [7, 6, 4, 3],
                                [1, 3, 4, 6],
                            ]
                        };
                        for t in t5 {
                            let mut tt: Vec<i32> = t.iter().map(|&k| c[k]).collect();
                            // positive orientation
                            let p: Vec<[Real; 3]> = tt.iter().map(|&n| xyz[n as usize]).collect();
                            if orient(&p) < 0.0 {
                                tt.swap(0, 1);
                            }
                            vlist.push(vols.len());
                            vols.push(tt);
                        }
                        // the face diagonal matching the split
                        let (d0, d1) = if lower {
                            if odd {
                                ([c[4], c[5], c[7]], [c[5], c[6], c[7]])
                            } else {
                                ([c[4], c[5], c[6]], [c[4], c[6], c[7]])
                            }
                        } else if odd {
                            ([c[0], c[1], c[2]], [c[0], c[2], c[3]])
                        } else {
                            ([c[0], c[1], c[3]], [c[1], c[2], c[3]])
                        };
                        for f in [d0, d1] {
                            flist.push(facs.len());
                            facs.push(f.to_vec());
                        }
                    } else {
                        vlist.push(vols.len());
                        vols.push(c.to_vec());
                        flist.push(facs.len());
                        facs.push(face.to_vec());
                    }
                }
            }
        }
        for c in &vols {
            a.connectivity_mut(vt, g).push_tuple(c);
        }
        for c in &facs {
            a.connectivity_mut(ft, g).push_tuple(c);
        }
    }
    group(&mut m, "lower", 3, vt, &vl);
    group(&mut m, "upper", 3, vt, &vu);
    group(&mut m, "master", 2, ft, &fm);
    group(&mut m, "slave", 2, ft, &fs);
    fill_element_to_subelements_data(&mut m).unwrap();
    m
}

fn orient(p: &[[Real; 3]]) -> Real {
    let d = |k: usize, i: usize| p[k][i] - p[0][i];
    let (a, b, c) = (
        [d(1, 0), d(1, 1), d(1, 2)],
        [d(2, 0), d(2, 1), d(2, 2)],
        [d(3, 0), d(3, 1), d(3, 2)],
    );
    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0])
}

// ------------------------------------------------------------------ model

fn dat(mu: Real, law: &str) -> String {
    format!(
        "contact_detector [\n  type = explicit\n  master = master\n  slave = slave\n  \
         projection_tolerance = 1e-12\n  max_iterations = 100\n  extension_tolerance = 1e-5\n]\n\n\
         contact_resolution {law} [\n  name = c\n  mu = {mu}\n  epsilon_n = 1e3\n  \
         epsilon_t = 2e3\n  is_master_deformable = true\n]\n"
    )
}

fn model(mesh: Mesh, mu: Real, law: &str) -> (SharedMesh, ContactMechanicsModel) {
    let parser = Parser::parse_str(&dat(mu, law), "v81.dat").unwrap();
    let sel = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let shared = SharedMesh::new(mesh);
    let mut m = ContactMechanicsModel::with_parser(shared.clone(), None, "cm", parser).unwrap();
    m.init_full(AnalysisMethod::ExplicitLumpedMass).unwrap();
    m.set_surface_selector(Box::new(sel)).unwrap();
    // V81_WEIGHT=half: the two-pass weight (½ on force and stiffness)
    m.deformable_master_weighting = if std::env::var("V81_WEIGHT").as_deref() == Ok("half") {
        DeformableMasterWeighting::TwoPassHalf
    } else {
        DeformableMasterWeighting::OnePassFull
    };
    (shared, m)
}

fn group_nodes(shared: &SharedMesh, name: &str) -> Vec<usize> {
    let g = shared.read().unwrap();
    let mut v: Vec<usize> = g
        .groups()
        .element_group(name)
        .unwrap()
        .nodes()
        .iter()
        .map(|&n| n as usize)
        .collect();
    v.sort_unstable();
    v
}

/// The scenario applied to a model: curve the master surface (current
/// positions), press the upper body in, step 0 (search + forces), slide,
/// step 1 (search + forces: the base state; history = step 0).
#[derive(Clone, Copy, Debug)]
struct Scenario {
    dim: usize,
    tets: bool,
    curve: Real,
    pen: Real,
    offset: [Real; 2],
    slide: [Real; 2],
    /// number of slides after the first contact (2: the history of the
    /// base state carries a non-zero traction t^p)
    slides: usize,
}

fn apply(sc: &Scenario, shared: &SharedMesh, m: &mut ContactMechanicsModel) {
    let lower = group_nodes(shared, "lower");
    let upper = group_nodes(shared, "upper");
    let d = sc.dim;
    {
        let p = m.positions_mut();
        for &n in &lower {
            let t = p.tuple_mut(n);
            let (x, y) = (t[0], if d == 3 { t[1] } else { 0.0 });
            let up = t[d - 1] + 1.0; // 0 at the bottom, 1 at the top
            let bump = sc.curve * (x * x + 0.5 * y * y + 0.7 * x * y + 0.4 * x * x * x);
            t[d - 1] += up * bump;
        }
        for &n in &upper {
            let t = p.tuple_mut(n);
            t[0] += sc.offset[0];
            if d == 3 {
                t[1] += sc.offset[1];
            }
            t[d - 1] -= sc.pen;
        }
    }
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    for _ in 0..sc.slides {
        {
            let p = m.positions_mut();
            for &n in &upper {
                let t = p.tuple_mut(n);
                t[0] += sc.slide[0];
                if d == 3 {
                    t[1] += sc.slide[1];
                }
            }
        }
        m.search().unwrap();
        m.assemble_internal_forces().unwrap();
    }
}

struct Frozen {
    proj: Array<Real>,
    tang: Array<Real>,
    trac: Array<Real>,
    elem: Array<Element>,
    area: Array<Real>,
    pairs: Vec<ContactElement>,
}

fn freeze(m: &ContactMechanicsModel) -> Frozen {
    Frozen {
        proj: m.fields.previous_projections.clone(),
        tang: m.fields.previous_tangents.clone(),
        trac: m.fields.previous_tangential_tractions.clone(),
        elem: m.fields.previous_master_elements.clone(),
        area: (*m.fields.nodal_area.borrow()).clone(),
        pairs: m.contact_elements().to_vec(),
    }
}

/// Assembled (internal, tangential) forces at `p` with `fz` frozen; `None`
/// if the pairing changed.
fn forces_at(
    m: &mut ContactMechanicsModel,
    p: &Array<Real>,
    fz: &Frozen,
) -> Option<(Vec<Real>, Vec<Real>)> {
    m.positions_mut().copy_from(p).unwrap();
    m.search().unwrap();
    if m.contact_elements() != &fz.pairs[..] {
        return None;
    }
    m.fields.previous_projections = fz.proj.clone();
    m.fields.previous_tangents = fz.tang.clone();
    m.fields.previous_tangential_tractions = fz.trac.clone();
    m.fields.previous_master_elements = fz.elem.clone();
    m.fields
        .nodal_area
        .borrow_mut()
        .copy_from(&fz.area)
        .unwrap();
    m.assemble_internal_forces().unwrap();
    Some((
        m.internal_force().as_slice().to_vec(),
        m.fields.tangential_force.borrow().as_slice().to_vec(),
    ))
}

fn dense_k(m: &mut ContactMechanicsModel, shared: &SharedMesh, dim: usize) -> Vec<Real> {
    let mut dofs = DofManager::new("dm", akantu_comm::Communicator::serial());
    dofs.register_nodal_dofs(&shared.read().unwrap(), "displacement", dim)
        .unwrap();
    m.assemble_stiffness_matrix(&mut dofs).unwrap();
    let n = m.positions().len() * dim;
    let geq: Vec<i64> = dofs
        .local_equation_numbers("displacement")
        .unwrap()
        .iter()
        .map(|&l| dofs.local_to_global_equation_number(l as usize).unwrap())
        .collect();
    let k = dofs.matrix("K").unwrap();
    let mut d = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            d[i + j * n] = k.get(geq[i], geq[j]);
        }
    }
    d
}

fn frob(v: &[Real]) -> Real {
    v.iter().map(|x| x * x).sum::<Real>().sqrt()
}

#[derive(Debug)]
struct Result1 {
    name: String,
    states: Vec<i32>,
    facet_changes: usize,
    err_total: Real,
    err_friction: Real,
    err_friction_elem: Real,
    missing_max: Real,
    nkt: Real,
    err_slave_cols: Real,
}

/// Run one scenario: `compat` on the frictional model (the twin is
/// frictionless with the same switches).
fn run(
    name: &str,
    sc: Scenario,
    mu: Real,
    law: &str,
    compat: ContactUpstreamCompat,
    h: Real,
) -> Option<Result1> {
    let mk = |mu| {
        let mesh = if sc.dim == 2 {
            mesh_2d(4, 3, 0.6)
        } else {
            mesh_3d(3, 2, 0.6, sc.tets)
        };
        let (s, mut m) = model(mesh, mu, law);
        m.set_upstream_compat(compat);
        apply(&sc, &s, &mut m);
        (s, m)
    };
    let (shared, mut m) = mk(mu);
    let (shared0, mut m0) = mk(0.0);
    let d = sc.dim;
    if m.contact_elements().is_empty() {
        println!("{name}: no contact");
        return None;
    }
    let states: Vec<i32> = m
        .contact_elements()
        .iter()
        .map(|c| contact_state_code(m.contact_state().tuple(c.slave as usize)[0]))
        .collect();
    let facet_changes = m
        .contact_elements()
        .iter()
        .filter(|c| {
            let pe = m.fields.previous_master_elements.tuple(c.slave as usize)[0];
            !pe.is_null() && pe != c.master
        })
        .count();
    let fz = freeze(&m);
    let fz0 = freeze(&m0);
    let tp_max = fz
        .pairs
        .iter()
        .flat_map(|c| fz.trac.tuple(c.slave as usize).to_vec())
        .fold(0.0_f64, |a, v| a.max(v.abs()));
    let k = dense_k(&mut m, &shared, d);
    let k0 = dense_k(&mut m0, &shared0, d);
    let p0 = m.positions().clone();
    let n = p0.len() * d;
    let (mut kfd, mut kfd_t) = (vec![0.0; n * n], vec![0.0; n * n]);
    for j in 0..n {
        let mut pp = p0.clone();
        pp.as_mut_slice()[j] += h;
        let mut pm = p0.clone();
        pm.as_mut_slice()[j] -= h;
        let (fp, tp) = forces_at(&mut m, &pp, &fz).or_else(|| {
            println!("{name}: pairing changed at +h on dof {j}");
            None
        })?;
        let (fm, tm) = forces_at(&mut m, &pm, &fz)?;
        // the frictionless twin must keep its pairing too
        forces_at(&mut m0, &pp, &fz0)?;
        for i in 0..n {
            kfd[i + j * n] = -(fp[i] - fm[i]) / (2.0 * h);
            kfd_t[i + j * n] = -(tp[i] - tm[i]) / (2.0 * h);
        }
    }
    let kt: Vec<Real> = k.iter().zip(&k0).map(|(a, b)| a - b).collect();
    let diff =
        |a: &[Real], b: &[Real]| -> Vec<Real> { a.iter().zip(b).map(|(x, y)| x - y).collect() };
    let err_total = frob(&diff(&k, &kfd)) / frob(&kfd);
    let err_friction = frob(&diff(&kt, &kfd_t)) / frob(&kfd_t);
    // restricted to the DOFs of the current contact elements (rows and
    // columns), and the largest FD entry where the assembled K_T has none
    let mut in_el = vec![false; n * n];
    for ce in &fz.pairs {
        let g = shared.read().unwrap();
        let mut nodes = vec![ce.slave as usize];
        nodes.extend(
            g.element_connectivity(ce.master)
                .unwrap()
                .iter()
                .map(|&x| x as usize),
        );
        for &a in &nodes {
            for &b in &nodes {
                for ia in 0..d {
                    for ib in 0..d {
                        in_el[a * d + ia + (b * d + ib) * n] = true;
                    }
                }
            }
        }
    }
    // columns of the slave DOFs only (a slave belongs to one element, so
    // these columns hold no dropped coupling)
    let slave_cols: Vec<usize> = fz
        .pairs
        .iter()
        .flat_map(|c| (0..d).map(move |i| c.slave as usize * d + i))
        .collect();
    let (mut sn, mut sd_) = (0.0, 0.0);
    for &j in &slave_cols {
        for i in 0..n {
            sn += (kt[i + j * n] - kfd_t[i + j * n]).powi(2);
            sd_ += kfd_t[i + j * n].powi(2);
        }
    }
    let err_slave_cols = (sn / sd_).sqrt();
    let (mut num, mut den, mut missing_max) = (0.0, 0.0, 0.0_f64);
    for idx in 0..n * n {
        if in_el[idx] {
            num += (kt[idx] - kfd_t[idx]).powi(2);
            den += kfd_t[idx].powi(2);
        } else {
            missing_max = missing_max.max(kfd_t[idx].abs());
        }
    }
    let r = Result1 {
        name: name.to_string(),
        states,
        facet_changes,
        err_total,
        err_friction,
        err_friction_elem: (num / den).sqrt(),
        missing_max,
        nkt: frob(&kfd_t),
        err_slave_cols,
    };
    println!(
        "{:<40} |t^p| {:.1e} states {:?} fc {} | total {:.2e} | friction {:.2e} (slave columns {:.2e}; element blocks {:.2e}; max |K_fd| outside {:.2e}; |K_T,fd| {:.2e})",
        r.name,
        tp_max,
        r.states,
        r.facet_changes,
        r.err_total,
        r.err_friction,
        r.err_slave_cols,
        r.err_friction_elem,
        r.missing_max,
        r.nkt
    );
    Some(r)
}

fn selected(name: &str) -> bool {
    std::env::var("V81_ONLY").map_or(true, |s| name.contains(&s))
}

fn cases() -> Vec<(String, Scenario, Real)> {
    let mut v = Vec::new();
    for (dim, tets, tag) in [(2, false, "2d"), (3, false, "3d-quad"), (3, true, "3d-tri")] {
        for (curve, ctag) in [(0.0, "flat"), (0.06, "curved")] {
            for (mu, slide, stag) in [
                (0.3, [0.05, 0.03], "slip"),
                (0.3, [1e-4, 5e-5], "stick"),
                (0.3, [0.35, 0.21], "slip-facet-change"),
                (60.0, [0.35, 0.21], "stick-facet-change"),
            ] {
                let sc = Scenario {
                    dim,
                    tets,
                    curve,
                    pen: 0.03,
                    offset: [0.013, 0.007],
                    slide,
                    slides: 1,
                };
                v.push((format!("{tag}-{ctag}-{stag}"), sc, mu));
            }
        }
    }
    v
}

/// Default (corrected) mode: the assembled tangent is the derivative of the
/// assembled force. Bound 1e-6 (FD truncation ~h²·|f'''| and cancellation
/// ~ε|f|/h with h = 1e-6 on O(1e3) stiffnesses; the spec reaches ~1e-9).
#[test]
fn consistent_friction_tangent_matches_the_assembled_force_on_every_dof() {
    let mut worst: Real = 0.0;
    let mut worst_elem: Real = 0.0;
    let mut worst_slave_cols: Real = 0.0;
    for (name, sc, mu) in cases() {
        if !selected(&name) {
            continue;
        }
        for law in ["penalty_linear", "penalty_quadratic"] {
            let full = format!("{name}-{}", &law[8..]);
            if let Some(r) = run(&full, sc, mu, law, ContactUpstreamCompat::default(), 1e-6) {
                worst_elem = worst_elem.max(r.err_friction_elem);
                worst_slave_cols = worst_slave_cols.max(r.err_slave_cols);
                // every case, facet changes included (the
                // previous-facet coupling is assembled)
                worst = worst.max(r.err_friction).max(r.err_total);
            }
        }
    }
    println!(
        "worst (all cases): {worst:.2e}; worst slave-column friction error: {worst_slave_cols:.2e}; worst element-block friction error: {worst_elem:.2e}"
    );
    assert!(worst < 1e-6);
    assert!(worst_slave_cols < 1e-6);
}

/// Base state with a non-zero traction history `t^p` (two slides after
/// the first contact): the transport derivative `t^p_γ m̂_p^{γβ}(τ^p_β·δτ_α)`
/// of the trial traction is exercised (it vanishes when `t^p = 0`, the
/// case of every single-slide configuration).
#[test]
fn consistent_friction_tangent_with_a_nonzero_traction_history() {
    let mut worst: Real = 0.0;
    for (dim, tets, tag) in [(2, false, "2d"), (3, false, "3d-quad"), (3, true, "3d-tri")] {
        for (mu, slide, stag) in [(0.3, [0.02, 0.012], "slip"), (0.3, [1e-4, 5e-5], "stick")] {
            let name = format!("{tag}-curved-{stag}-tp");
            if !selected(&name) {
                continue;
            }
            let sc = Scenario {
                dim,
                tets,
                curve: 0.06,
                pen: 0.03,
                offset: [0.013, 0.007],
                slide,
                slides: 2,
            };
            if let Some(r) = run(
                &name,
                sc,
                mu,
                "penalty_linear",
                ContactUpstreamCompat::default(),
                1e-6,
            ) {
                worst = worst.max(r.err_slave_cols).max(r.err_friction);
            }
        }
    }
    println!("worst (t^p != 0): {worst:.2e}");
    assert!(worst < 1e-6);
}
