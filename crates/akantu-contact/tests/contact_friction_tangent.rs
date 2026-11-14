// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the
// consistent friction tangent (UD-369 correction) of
// src/model/contact_mechanics/resolutions/resolution_penalty.cc:
//
// * central finite differences of the port's own frictional force with the
//   history (t^p, τ^p, ξ_p, e_p) and the nodal area frozen, in 2-D and 3-D,
//   stick and slip, on flat, tilted, curved (kinked), warped bilinear and
//   triangle masters, both pressure laws, and across a master-facet change;
// * an independent reference implementation of the specification
//   (`consistent_friction_tangent`) reproduced against the port's tangent;
// * upstream's moduli (UD-369 compatibility switch) are not the derivative.

#![allow(clippy::needless_range_loop)] // entry indices mirror the formulas

mod common;

use akantu_contact::{ContactElement, ContactMechanicsModel, ContactUpstreamCompat};
use akantu_core::{Array, Element, Real};
use akantu_mesh::SharedMesh;
use common::*;

/// Frozen history of the slaves (`previous_*` rows of the last search).
#[derive(Clone, Debug)]
pub struct History {
    proj: Array<Real>,
    tang: Array<Real>,
    trac: Array<Real>,
    elem: Array<Element>,
}

pub fn history(m: &ContactMechanicsModel) -> History {
    History {
        proj: m.fields.previous_projections.clone(),
        tang: m.fields.previous_tangents.clone(),
        trac: m.fields.previous_tangential_tractions.clone(),
        elem: m.fields.previous_master_elements.clone(),
    }
}

pub fn restore(m: &mut ContactMechanicsModel, h: &History) {
    m.fields.previous_projections = h.proj.clone();
    m.fields.previous_tangents = h.tang.clone();
    m.fields.previous_tangential_tractions = h.trac.clone();
    m.fields.previous_master_elements = h.elem.clone();
}

/// Local tangential force and state code of the contact element of `slave`
/// after a search at `positions` with the history and the area frozen
/// (`None` if the slave lost its contact or changed master facet).
fn friction_force_at(
    m: &mut ContactMechanicsModel,
    positions: &Array<Real>,
    slave: i32,
    master: Element,
    area: Real,
    hist: &History,
) -> Option<(Vec<Real>, i32)> {
    m.positions_mut().copy_from(positions).unwrap();
    m.search().unwrap();
    restore(m, hist);
    let Some(ce) = m
        .contact_elements()
        .iter()
        .find(|c| c.slave == slave)
        .copied()
    else {
        eprintln!("slave {slave}: no contact element after the perturbation");
        return None;
    };
    if ce.master != master {
        eprintln!("slave {slave}: master {} instead of {master}", ce.master);
        return None;
    }
    m.fields.nodal_area.borrow_mut().tuple_mut(slave as usize)[0] = area;
    let (_, ft) = m.local_forces(0, &ce).unwrap();
    let st = akantu_contact::contact_state_code(m.contact_state().tuple(slave as usize)[0]);
    Some((ft, st))
}

/// Result of [`friction_fd`].
#[derive(Debug)]
pub struct FrictionFd {
    pub kt: Vec<Real>,
    pub kt_fd: Vec<Real>,
    pub nd: usize,
    pub state: i32,
    pub element: ContactElement,
    pub previous: Element,
}

/// Tangential moduli of the contact element of `slave` and the central
/// finite differences `−(f_T(u + h e_j) − f_T(u − h e_j))/2h` over its DOFs,
/// history frozen at `hist`, nodal area frozen.
pub fn friction_fd(
    m: &mut ContactMechanicsModel,
    shared: &SharedMesh,
    slave: i32,
    hist: &History,
    h: Real,
) -> FrictionFd {
    let p0 = m.positions().clone();
    m.search().unwrap();
    restore(m, hist);
    let ce: ContactElement = *m
        .contact_elements()
        .iter()
        .find(|c| c.slave == slave)
        .expect("slave in contact");
    let area = m.nodal_area().tuple(slave as usize)[0];
    let dim = m.spatial_dimension();
    let nd = ce.nb_nodes() * dim;
    // the force sets the state and the traction, then the moduli
    let _ = m.local_forces(0, &ce).unwrap();
    let state = akantu_contact::contact_state_code(m.contact_state().tuple(slave as usize)[0]);
    let (_, kt) = m.local_moduli(0, &ce).unwrap();
    let previous = hist.elem.tuple(slave as usize)[0];
    let mesh = shared.read().unwrap();
    let mut conn = vec![ce.slave];
    conn.extend_from_slice(&mesh.element_connectivity(ce.master).unwrap());
    drop(mesh);
    let mut kt_fd = vec![0.0; nd * nd];
    for j in 0..nd {
        let (node, comp) = (conn[j / dim] as usize, j % dim);
        let mut pp = p0.clone();
        pp.tuple_mut(node)[comp] += h;
        let mut pm = p0.clone();
        pm.tuple_mut(node)[comp] -= h;
        let (fp, sp) =
            friction_force_at(m, &pp, slave, ce.master, area, hist).expect("same pairing (+h)");
        let (fm, sm) =
            friction_force_at(m, &pm, slave, ce.master, area, hist).expect("same pairing (−h)");
        assert_eq!((sp, sm), (state, state), "contact state changed under ±h");
        for i in 0..nd {
            kt_fd[i + j * nd] = -(fp[i] - fm[i]) / (2.0 * h);
        }
    }
    // leave the model at the reference state (search + history + force)
    m.positions_mut().copy_from(&p0).unwrap();
    m.search().unwrap();
    restore(m, hist);
    let _ = m.local_forces(0, &ce).unwrap();
    FrictionFd {
        kt,
        kt_fd,
        nd,
        state,
        element: ce,
        previous,
    }
}

pub fn frob(a: &[Real]) -> Real {
    a.iter().map(|v| v * v).sum::<Real>().sqrt()
}

pub fn rel(a: &[Real], b: &[Real]) -> Real {
    let d: Vec<Real> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    frob(&d) / frob(b)
}

fn asym(k: &[Real], nd: usize) -> Real {
    let mut d: Real = 0.0;
    for i in 0..nd {
        for j in 0..nd {
            d = d.max((k[i + j * nd] - k[j + i * nd]).abs());
        }
    }
    d / frob(k)
}

/// Pressure laws and material text.
fn dat(mu: Real, eps_t: Real, law: &str) -> String {
    contact_dat("master", "slave", mu, 1e3, eps_t, true).replace("penalty_linear", law)
}

/// Master shape applied to the lower body's nodes (positions from the mesh
/// nodes), slave body translation.
#[derive(Clone, Copy, Debug)]
pub enum Master {
    /// Flat (unchanged).
    Flat,
    /// One master node raised (2-D: x = 0.5; 3-D: corner (0.5, −0.5, 0)).
    Tilted,
    /// 2-D: y += (y + 1)(0.25 − 0.6 x²) (kinked piecewise-linear arc);
    /// 3-D: z += (z + 1)(−0.15 (x² + y²)) (warped bilinear facets or curved
    /// triangles).
    Curved,
}

/// A 2-D (`tets = None`) or 3-D (`Some(tets)`) two-block model with the
/// master shaped by `shape` and the upper body translated by `shift`.
pub fn setup(
    dim3: Option<bool>,
    shape: Master,
    shift: &[Real],
    mu: Real,
    eps_t: Real,
    law: &str,
    compat: ContactUpstreamCompat,
) -> (SharedMesh, ContactMechanicsModel) {
    let mesh = match dim3 {
        None => two_blocks_2d(8, 1.0, 2, 0.4),
        Some(tets) => two_blocks_3d(4, 1.0, 2, 0.4, tets),
    };
    let dim = mesh.spatial_dimension();
    let (shared, mut m) = contact_model(mesh, &dat(mu, eps_t, law));
    m.set_upstream_compat(compat);
    let g = shared.read().unwrap();
    let lower = nodes_of_physical(&g, "lower_body");
    let upper = nodes_of_physical(&g, "upper_body");
    let x = g.nodes().clone();
    drop(g);
    let p = m.positions_mut();
    for &n in &lower {
        let t = x.tuple(n as usize);
        let q = p.tuple_mut(n as usize);
        match (shape, dim) {
            (Master::Flat, _) => {}
            (Master::Tilted, 2) => {
                if (t[0] - 0.5).abs() < 1e-12 && t[1].abs() < 1e-12 {
                    q[1] += 0.05;
                }
            }
            (Master::Tilted, _) => {
                if (t[0] - 0.5).abs() < 1e-12 && (t[1] + 0.5).abs() < 1e-12 && t[2].abs() < 1e-12 {
                    q[2] += 0.08;
                }
            }
            (Master::Curved, 2) => q[1] = t[1] + (t[1] + 1.0) * (0.25 - 0.6 * t[0] * t[0]),
            (Master::Curved, _) => {
                q[2] = t[2] + (t[2] + 1.0) * (-0.15 * (t[0] * t[0] + t[1] * t[1]));
            }
        }
    }
    for &n in &upper {
        let q = p.tuple_mut(n as usize);
        for d in 0..dim {
            q[d] += shift[d];
        }
    }
    (shared, m)
}

/// Slave node of the upper body closest to the reference point `p0`
/// (mesh coordinates).
pub fn slave_near(shared: &SharedMesh, p0: &[Real]) -> i32 {
    let m = shared.read().unwrap();
    let x = m.nodes();
    let slaves = nodes_of_physical(&m, "slave");
    *slaves
        .iter()
        .min_by(|&&a, &&b| {
            let da: Real = x
                .tuple(a as usize)
                .iter()
                .zip(p0)
                .map(|(u, v)| (u - v) * (u - v))
                .sum();
            let db: Real = x
                .tuple(b as usize)
                .iter()
                .zip(p0)
                .map(|(u, v)| (u - v) * (u - v))
                .sum();
            da.total_cmp(&db)
        })
        .unwrap()
}

/// History step (two searches with force assemblies), then the tangential
/// move `mv` of the upper body and a search; returns the frozen history.
pub fn history_step(m: &mut ContactMechanicsModel, shared: &SharedMesh, mv: &[Real]) -> History {
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    translate_group(m, shared, "upper_body", mv);
    m.search().unwrap();
    history(m)
}

const H: Real = 1e-7;
/// Bound on ‖K_T − K_fd‖/‖K_fd‖: the central-difference truncation error is
/// O(h²) and the rounding error O(ε |f| / (h |K|)) ≈ 1e-9 at h = 1e-7
/// (docs/NUMERICAL_VALIDATION.md).
const FD_BOUND: Real = 1e-6;

struct Case {
    label: &'static str,
    dim3: Option<bool>,
    shape: Master,
    law: &'static str,
    mu: Real,
    eps_t: Real,
    mv: [Real; 3],
    expect_state: i32,
}

/// Upper-body shift giving a penetration of ≈ 0.06–0.11 at the observed
/// slave (`slave_point`).
fn shift_of(dim3: Option<bool>, shape: Master) -> Vec<Real> {
    match (dim3, shape) {
        (None, Master::Curved) => vec![0.031, 0.071],
        (None, _) => vec![0.031, -0.06],
        (Some(_), Master::Curved) => vec![0.013, 0.007, -0.11],
        (Some(_), _) => vec![0.013, 0.007, -0.06],
    }
}

/// The observed slave node (mesh coordinates): above the master facet
/// [0.25, 0.5] (2-D) or [0, 0.5] × [−0.5, 0] (3-D) after the shift.
fn slave_point(dim3: Option<bool>) -> Vec<Real> {
    match dim3 {
        None => vec![0.4, 0.0],
        Some(_) => vec![0.4, -0.4, 0.0],
    }
}

fn run_case(c: &Case) -> (FrictionFd, Real, Real) {
    let dim = if c.dim3.is_some() { 3 } else { 2 };
    let (shared, mut m) = setup(
        c.dim3,
        c.shape,
        &shift_of(c.dim3, c.shape),
        c.mu,
        c.eps_t,
        c.law,
        ContactUpstreamCompat::default(),
    );
    let slave = slave_near(&shared, &slave_point(c.dim3));
    let hist = history_step(&mut m, &shared, &c.mv[..dim]);
    let r = friction_fd(&mut m, &shared, slave, &hist, H);
    let e = rel(&r.kt, &r.kt_fd);
    // upstream's moduli on the same state
    let mut compat = m.upstream_compat();
    compat.friction_moduli = true;
    m.set_upstream_compat(compat);
    let ce = r.element;
    let (_, ku) = m.local_moduli(0, &ce).unwrap();
    let eu = rel(&ku, &r.kt_fd);
    (r, e, eu)
}

/// UD-369: the default friction tangent is `−∂f_T/∂u` (finite
/// differences of the port's own force) in stick and slip, 2-D and 3-D, on
/// flat, tilted, curved, warped and triangle masters, for both pressure
/// laws; upstream's moduli are not.
#[test]
fn friction_tangent_matches_finite_differences() {
    let stick2 = [0.002, 0.0, 0.0];
    let slip2 = [0.02, 0.0, 0.0];
    let stick3 = [0.002, 0.001, 0.0];
    let slip3 = [0.02, 0.01, 0.0];
    let mut cases = Vec::new();
    for (shape, sname) in [
        (Master::Flat, "flat"),
        (Master::Tilted, "tilted"),
        (Master::Curved, "curved"),
    ] {
        for (dim3, dname) in [
            (None, "2-D seg2"),
            (Some(false), "3-D quad4"),
            (Some(true), "3-D tri3"),
        ] {
            for (law, lname) in [
                ("penalty_linear", "linear"),
                ("penalty_quadratic", "quadratic"),
            ] {
                let (stick, slip) = if dim3.is_none() {
                    (stick2, slip2)
                } else {
                    (stick3, slip3)
                };
                cases.push((
                    format!("{dname} {sname} {lname} stick"),
                    Case {
                        label: "stick",
                        dim3,
                        shape,
                        law,
                        mu: 0.5,
                        eps_t: 1e3,
                        mv: stick,
                        expect_state: 1,
                    },
                ));
                cases.push((
                    format!("{dname} {sname} {lname} slip"),
                    Case {
                        label: "slip",
                        dim3,
                        shape,
                        law,
                        mu: 0.05,
                        eps_t: 1e4,
                        mv: slip,
                        expect_state: 2,
                    },
                ));
            }
        }
    }
    let mut worst: Real = 0.0;
    for (name, c) in &cases {
        let (r, e, eu) = run_case(c);
        println!(
            "{name:<34}: state {} | ‖K_T‖ {:.3e} | ‖K_T − K_fd‖/‖K_fd‖ = {e:.2e} | upstream {eu:.2e} | asym(K_fd) {:.1e}",
            r.state,
            frob(&r.kt),
            asym(&r.kt_fd, r.nd)
        );
        assert_eq!(r.state, c.expect_state, "{name}: expected {}", c.label);
        assert!(e < FD_BOUND, "{name}: ‖K_T − K_fd‖/‖K_fd‖ = {e:e}");
        assert!(
            eu > 0.5,
            "{name}: upstream moduli unexpectedly consistent ({eu:e})"
        );
        worst = worst.max(e);
    }
    println!(
        "worst relative error over {} cases: {worst:.2e} (h = {H:e})",
        cases.len()
    );
}

/// The error of the finite differences against the tangent behaves like a
/// central difference (O(h²) truncation, O(1/h) rounding): reported for the
/// step-size table of docs/NUMERICAL_VALIDATION.md.
#[test]
fn friction_tangent_step_size_study() {
    for (dim3, mv) in [(None, [0.02, 0.0, 0.0]), (Some(false), [0.02, 0.01, 0.0])] {
        let dim = if dim3.is_some() { 3 } else { 2 };
        let mut errs = Vec::new();
        for h in [1e-5, 1e-6, 1e-7, 1e-8] {
            let (shared, mut m) = setup(
                dim3,
                Master::Curved,
                &shift_of(dim3, Master::Curved),
                0.05,
                1e4,
                "penalty_linear",
                ContactUpstreamCompat::default(),
            );
            let slave = slave_near(&shared, &slave_point(dim3));
            let hist = history_step(&mut m, &shared, &mv[..dim]);
            let r = friction_fd(&mut m, &shared, slave, &hist, h);
            assert_eq!(r.state, 2);
            errs.push((h, rel(&r.kt, &r.kt_fd)));
        }
        println!("{dim}-D curved slip: {errs:?}");
        // the best step is well inside the bound and h = 1e-5 is
        // truncation-dominated (larger than at 1e-6)
        let best = errs.iter().map(|e| e.1).fold(Real::INFINITY, Real::min);
        assert!(best < 1e-8, "best FD error {best:e}");
    }
}

/// A slave that changed master facet since the last search: the tangent is
/// still `−∂f_T/∂u` over the element DOFs (the shared node of the previous
/// facet enters `δΔx`; the other previous-facet nodes are outside the
/// element). Also with the UD-365 compatibility switch (current point on
/// the previous facet).
#[test]
fn friction_tangent_across_a_facet_change() {
    for (dim3, mv, shift) in [
        (None, vec![0.13, 0.0], vec![0.031, 0.0]),
        (Some(false), vec![0.2, 0.0, 0.0], vec![0.013, 0.007, -0.13]),
        (Some(true), vec![0.2, 0.0, 0.0], vec![0.013, 0.007, -0.13]),
    ] {
        let p0 = slave_point(dim3);
        for (mu, eps_t, label) in [(0.5, 30.0, 1), (0.05, 1e4, 2)] {
            for ud365 in [false, true] {
                let compat = ContactUpstreamCompat {
                    trial_slip_on_previous_element: ud365,
                    ..ContactUpstreamCompat::default()
                };
                let (shared, mut m) = setup(
                    dim3,
                    Master::Curved,
                    &shift,
                    mu,
                    eps_t,
                    "penalty_linear",
                    compat,
                );
                let slave = slave_near(&shared, &p0);
                let hist = history_step(&mut m, &shared, &mv);
                let r = friction_fd(&mut m, &shared, slave, &hist, H);
                let e = rel(&r.kt, &r.kt_fd);
                println!(
                    "{}-D facet change {} → {} (UD-365 compat {ud365}): state {} | rel err {e:.2e}",
                    if dim3.is_some() { 3 } else { 2 },
                    r.previous,
                    r.element.master,
                    r.state
                );
                assert_ne!(r.previous, r.element.master, "the slave must change facet");
                assert!(!r.previous.is_null());
                assert_eq!(r.state, label);
                assert!(e < FD_BOUND, "facet change: rel err {e:e}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Newton convergence of an implicit frictional sliding problem
// ---------------------------------------------------------------------------

/// Dense solve `a x = b` (Gaussian elimination with partial pivoting).
fn dense_solve(mut a: Vec<Real>, mut b: Vec<Real>, n: usize) -> Vec<Real> {
    for c in 0..n {
        let p = (c..n)
            .max_by(|&i, &j| a[i + c * n].abs().total_cmp(&a[j + c * n].abs()))
            .unwrap();
        if p != c {
            for k in 0..n {
                a.swap(c + k * n, p + k * n);
            }
            b.swap(c, p);
        }
        let d = a[c + c * n];
        for i in c + 1..n {
            let f = a[i + c * n] / d;
            if f == 0.0 {
                continue;
            }
            for k in c..n {
                a[i + k * n] -= f * a[c + k * n];
            }
            b[i] -= f * b[c];
        }
    }
    for c in (0..n).rev() {
        let mut v = b[c];
        for k in c + 1..n {
            v -= a[c + k * n] * b[k];
        }
        b[c] = v / a[c + c * n];
    }
    b
}

/// Dense contact stiffness (node-major displacement DOFs) assembled by the
/// model through a serial DOF manager.
fn dense_contact_k(m: &mut ContactMechanicsModel, shared: &SharedMesh) -> Vec<Real> {
    let dim = m.spatial_dimension();
    let mut dofs = akantu_solver::DofManager::new("dm", akantu_comm::Communicator::serial());
    dofs.register_nodal_dofs(&shared.read().unwrap(), "displacement", dim)
        .unwrap();
    m.assemble_stiffness_matrix(&mut dofs).unwrap();
    let n = m.positions().len() * dim;
    let leq = dofs
        .local_equation_numbers("displacement")
        .unwrap()
        .to_vec();
    let geq: Vec<i64> = leq
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

/// Residual norms of a Newton solve of `f_c(x) + k (x_t − x) = 0` (every
/// node tied by springs of stiffness `k` to its target position `x_t`,
/// Jacobian `k I + K_c`), with the friction history frozen at `hist` and
/// (optionally) the nodal areas frozen at `areas`. Also returns the contact
/// states of the slaves at every iteration.
fn newton(
    m: &mut ContactMechanicsModel,
    shared: &SharedMesh,
    target: &[Real],
    hist: Option<&History>,
    areas: Option<&Array<Real>>,
    k: Real,
    iterations: usize,
) -> (Vec<Real>, Vec<Vec<i32>>) {
    let n = target.len();
    let mut x = target.to_vec();
    let (mut res, mut states) = (Vec::new(), Vec::new());
    for it in 0..=iterations {
        m.positions_mut().as_mut_slice().copy_from_slice(&x);
        m.search().unwrap();
        if let Some(h) = hist {
            restore(m, h);
        }
        if let Some(a) = areas {
            m.fields.nodal_area.borrow_mut().copy_from(a).unwrap();
        }
        m.assemble_internal_forces().unwrap();
        let f = m.internal_force().as_slice().to_vec();
        let r: Vec<Real> = (0..n).map(|i| f[i] + k * (target[i] - x[i])).collect();
        res.push(frob(&r));
        states.push(
            m.contact_elements()
                .iter()
                .map(|c| {
                    akantu_contact::contact_state_code(m.contact_state().tuple(c.slave as usize)[0])
                })
                .collect(),
        );
        if it == iterations || !res[it].is_finite() {
            break;
        }
        let mut j = dense_contact_k(m, shared);
        for i in 0..n {
            j[i + i * n] += k;
        }
        let dx = dense_solve(j, r, n);
        for i in 0..n {
            x[i] += dx[i];
        }
    }
    (res, states)
}

/// Implicit frictional sliding: a deformable curved master and a
/// slave body tied by springs to a prescribed slide, history frozen at the
/// start of the step. With the consistent tangent (default) Newton converges
/// quadratically (nodal areas frozen: the exact Jacobian; areas recomputed
/// at every search: `∂A_s/∂u` is missing, and the convergence is
/// fast but not exactly quadratic); with upstream's friction moduli it does
/// not converge quadratically.
#[test]
fn newton_converges_quadratically_with_the_consistent_friction_tangent() {
    for dim3 in [None, Some(false)] {
        let dim = if dim3.is_some() { 3 } else { 2 };
        let slide: Vec<Real> = if dim == 2 {
            vec![0.02, 0.0]
        } else {
            vec![0.02, 0.01, 0.0]
        };
        for (upstream, frozen_areas) in [(false, true), (false, false), (true, true)] {
            let compat = ContactUpstreamCompat {
                friction_moduli: upstream,
                ..ContactUpstreamCompat::default()
            };
            let (shared, mut m) = setup(
                dim3,
                Master::Curved,
                &shift_of(dim3, Master::Curved),
                0.05,
                1e4,
                "penalty_linear",
                compat,
            );
            m.deformable_master_weighting = akantu_contact::DeformableMasterWeighting::OnePassFull;
            let k = 100.0;
            // step 0: first contact, the (empty) history frozen: no friction
            // (an unfrozen history would switch friction on from the second
            // iteration on, with the previous iterate as history)
            let x0 = m.positions().as_slice().to_vec();
            let empty = history(&m);
            let (r0, _) = newton(&mut m, &shared, &x0, Some(&empty), None, k, 12);
            assert!(
                *r0.last().unwrap() < 1e-10 * r0[0],
                "step 0 did not converge: {r0:?}"
            );
            // history of the step start
            m.search().unwrap();
            m.assemble_internal_forces().unwrap();
            m.search().unwrap();
            let hist = history(&m);
            let areas = m.nodal_area().clone();
            // step 1: slide the slave body's targets
            let mut target = x0.clone();
            let upper = nodes_of_physical(&shared.read().unwrap(), "upper_body");
            for &nn in &upper {
                for d in 0..dim {
                    target[dim * nn as usize + d] += slide[d];
                }
            }
            let (res, states) = newton(
                &mut m,
                &shared,
                &target,
                Some(&hist),
                frozen_areas.then_some(&areas),
                k,
                8,
            );
            let rel_res: Vec<Real> = res.iter().map(|r| r / res[0]).collect();
            println!(
                "{dim}-D, {} tangent, areas {}: ‖R_k‖/‖R_0‖ = {:?}, states {:?}",
                if upstream { "upstream" } else { "consistent" },
                if frozen_areas { "frozen" } else { "recomputed" },
                rel_res
                    .iter()
                    .map(|v| format!("{v:.1e}"))
                    .collect::<Vec<_>>(),
                states.last().unwrap()
            );
            if !upstream {
                assert!(
                    states.iter().all(|s| s == &states[0]),
                    "states changed: {states:?}"
                );
                assert!(
                    states[0].iter().all(|&s| s == 2),
                    "every slave in slip: {:?}",
                    states[0]
                );
            }
            // order estimates p_k = log(r_{k+1}/r_k) / log(r_k/r_{k−1}) above the
            // rounding floor (the residual stagnates at ≈ 4e-16 ‖R_0‖)
            let floor = 1e-14;
            let orders: Vec<Real> = (1..rel_res.len() - 1)
                .filter(|&i| rel_res[i + 1] > floor)
                .map(|i| (rel_res[i + 1] / rel_res[i]).ln() / (rel_res[i] / rel_res[i - 1]).ln())
                .collect();
            println!("    order estimates {orders:?}");
            let converged = rel_res.iter().position(|&r| r < 1e-11);
            match (upstream, frozen_areas) {
                (false, true) => {
                    let c = converged.expect("consistent tangent: converged");
                    assert!(c <= 8, "consistent tangent: {c} iterations");
                    let p = orders
                        .iter()
                        .rev()
                        .take(2)
                        .copied()
                        .fold(Real::NEG_INFINITY, Real::max);
                    assert!(p > 1.8, "consistent tangent: order {p}");
                }
                (false, false) => {
                    assert!(converged.is_some(), "recomputed areas: converged");
                }
                (true, _) => {
                    let p = orders.iter().copied().fold(Real::NEG_INFINITY, Real::max);
                    assert!(
                        converged.is_none() || p < 1.2,
                        "upstream tangent unexpectedly quadratic: {rel_res:?}"
                    );
                }
            }
        }
    }
}

/// UD-374 evidence: upstream's implicit coupler searches in every
/// `corrector()`, so the friction history (`previous_*`) is the previous
/// Newton iterate instead of the converged state of the previous step.
/// With that per-iteration history the same sliding step (consistent
/// tangent, areas frozen) does not converge (the residual stagnates), while
/// the step-level history converges quadratically; the step-level freeze is
/// the coupler's decision.
#[test]
fn per_iteration_friction_history_does_not_solve_the_increment() {
    let dim3 = None;
    let (shared, mut m) = setup(
        dim3,
        Master::Curved,
        &shift_of(dim3, Master::Curved),
        0.05,
        1e4,
        "penalty_linear",
        ContactUpstreamCompat::default(),
    );
    m.deformable_master_weighting = akantu_contact::DeformableMasterWeighting::OnePassFull;
    let k = 100.0;
    let x0 = m.positions().as_slice().to_vec();
    let empty = history(&m);
    let (r0, _) = newton(&mut m, &shared, &x0, Some(&empty), None, k, 12);
    assert!(*r0.last().unwrap() < 1e-10 * r0[0]);
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    m.search().unwrap();
    let hist = history(&m);
    let areas = m.nodal_area().clone();
    let mut target = x0.clone();
    for &nn in &nodes_of_physical(&shared.read().unwrap(), "upper_body") {
        target[2 * nn as usize] += 0.02;
    }
    let (frozen, _) = newton(&mut m, &shared, &target, Some(&hist), Some(&areas), k, 8);
    // per-iteration history: restore the step-start history only for the
    // first iterate, then let every search save the previous iterate
    restore(&mut m, &hist);
    let mut x = target.clone();
    let n = x.len();
    let mut per_iteration = Vec::new();
    for it in 0..=12 {
        m.positions_mut().as_mut_slice().copy_from_slice(&x);
        m.search().unwrap();
        if it == 0 {
            restore(&mut m, &hist);
        }
        m.fields.nodal_area.borrow_mut().copy_from(&areas).unwrap();
        m.assemble_internal_forces().unwrap();
        let f = m.internal_force().as_slice().to_vec();
        let r: Vec<Real> = (0..n).map(|i| f[i] + k * (target[i] - x[i])).collect();
        per_iteration.push(frob(&r));
        let mut j = dense_contact_k(&mut m, &shared);
        for i in 0..n {
            j[i + i * n] += k;
        }
        let dx = dense_solve(j, r, n);
        for i in 0..n {
            x[i] += dx[i];
        }
    }
    let rel_f: Vec<String> = frozen
        .iter()
        .map(|r| format!("{:.1e}", r / frozen[0]))
        .collect();
    let rel_p: Vec<String> = per_iteration
        .iter()
        .map(|r| format!("{:.1e}", r / per_iteration[0]))
        .collect();
    println!("step-level history:     ‖R_k‖/‖R_0‖ = {rel_f:?}");
    println!("per-iteration history:  ‖R_k‖/‖R_0‖ = {rel_p:?}");
    assert!(frozen.last().unwrap() / frozen[0] < 1e-11);
    // measured: stagnates at ≈ 1.5e-2 (the friction residual is redefined
    // at every iteration)
    assert!(
        per_iteration.last().unwrap() / per_iteration[0] > 1e-6,
        "per-iteration history unexpectedly converged: {rel_p:?}"
    );
}

// ---------------------------------------------------------------------------
// An independent reference implementation of the specification
// (`consistent_friction_tangent`, copied unchanged apart from formatting;
// linear facets, linear law, e_p = e): a second, independent coding of the
// same formulas
// ---------------------------------------------------------------------------

type V3 = [Real; 3];

fn dotn(a: &[Real], b: &[Real]) -> Real {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn inv(m: &[[Real; 2]; 2], sd: usize) -> [[Real; 2]; 2] {
    if sd == 1 {
        [[1.0 / m[0][0], 0.0], [0.0, 0.0]]
    } else {
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        [
            [m[1][1] / det, -m[0][1] / det],
            [-m[1][0] / det, m[0][0] / det],
        ]
    }
}

/// N, ∂N/∂ξ_α, ∂²N/∂ξ_α∂ξ_β of segment_2 / quadrangle_4 (akantu node order).
/// `(N, ∂N/∂ξ_α, ∂²N/∂ξ_α∂ξ_β)`.
type Shapes = (Vec<Real>, Vec<Vec<Real>>, Vec<Vec<Vec<Real>>>);

fn shapes(nn: usize, xi: &[Real]) -> Shapes {
    if nn == 2 {
        let x = xi[0];
        (
            vec![0.5 * (1.0 - x), 0.5 * (1.0 + x)],
            vec![vec![-0.5, 0.5]],
            vec![vec![vec![0.0, 0.0]]],
        )
    } else {
        assert_eq!(nn, 4);
        let (x, y) = (xi[0], xi[1]);
        let c = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let n: Vec<Real> = c
            .iter()
            .map(|&(a, b)| 0.25 * (1.0 + a * x) * (1.0 + b * y))
            .collect();
        let dx: Vec<Real> = c.iter().map(|&(a, b)| 0.25 * a * (1.0 + b * y)).collect();
        let dy: Vec<Real> = c.iter().map(|&(a, b)| 0.25 * b * (1.0 + a * x)).collect();
        let dxy: Vec<Real> = c.iter().map(|&(a, b)| 0.25 * a * b).collect();
        let z = vec![0.0; 4];
        (
            n,
            vec![dx, dy],
            vec![vec![z.clone(), dxy.clone()], vec![dxy, z]],
        )
    }
}

struct Spec {
    k: Vec<Real>,
    /// returned traction recomputed by the spec (sanity check vs the port)
    t: [Real; 2],
}

#[allow(clippy::too_many_lines)]
fn consistent_friction_tangent(
    m: &ContactMechanicsModel,
    shared: &SharedMesh,
    ce: &ContactElement,
    hist: &History,
    eps_n: Real,
    eps_t: Real,
    mu: Real,
) -> Spec {
    let dim = m.spatial_dimension();
    let sd = dim - 1;
    let s = ce.slave as usize;
    let mesh = shared.read().unwrap();
    let conn: Vec<i32> = mesh.element_connectivity(ce.master).unwrap().to_vec();
    drop(mesh);
    let nn = conn.len();
    let nd = dim * (nn + 1);
    let p = m.positions();
    let xk: Vec<V3> = conn
        .iter()
        .map(|&k| {
            let mut v = [0.0; 3];
            v[..dim].copy_from_slice(p.tuple(k as usize));
            v
        })
        .collect();
    let xi = m.fields.projections.tuple(s).to_vec();
    let xi_p = hist.proj.tuple(s).to_vec();
    assert_eq!(hist.elem.tuple(s)[0], ce.master, "spec assumes e_p = e");
    let g = m.gaps().tuple(s)[0];
    let area = m.nodal_area().tuple(s)[0];
    let nrm = m.normals().tuple(s).to_vec();
    let tau_st = m.fields.tangents.tuple(s).to_vec();
    let tp = hist.trac.tuple(s).to_vec();
    let taup = hist.tang.tuple(s).to_vec();
    let (nf, dn, d2n) = shapes(nn, &xi);
    let (nfp, _, _) = shapes(nn, &xi_p);
    // natural tangents, second derivatives
    let mut a = [[0.0; 3]; 2];
    let mut xab = [[[0.0; 3]; 2]; 2];
    for al in 0..sd {
        for k in 0..nn {
            for i in 0..dim {
                a[al][i] += dn[al][k] * xk[k][i];
                for be in 0..sd {
                    xab[al][be][i] += d2n[al][be][k] * xk[k][i];
                }
            }
        }
    }
    let mut mm = [[0.0; 2]; 2];
    let mut kap = [[0.0; 2]; 2];
    for al in 0..sd {
        for be in 0..sd {
            mm[al][be] = dotn(&a[al][..dim], &a[be][..dim]);
            kap[al][be] = dotn(&xab[al][be][..dim], &nrm);
        }
    }
    let mut hh = [[0.0; 2]; 2];
    for al in 0..sd {
        for be in 0..sd {
            hh[al][be] = mm[al][be] + g * kap[al][be];
        }
    }
    let hinv = inv(&hh, sd);
    // unit tangents with the stored orientation
    let mut alen = [0.0; 2];
    let mut tau = [[0.0; 3]; 2];
    let mut sgn = [1.0; 2];
    for al in 0..sd {
        alen[al] = dotn(&a[al][..dim], &a[al][..dim]).sqrt();
        let st = &tau_st[al * dim..(al + 1) * dim];
        sgn[al] = if dotn(st, &a[al][..dim]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        for i in 0..dim {
            tau[al][i] = sgn[al] * a[al][i] / alen[al];
        }
    }
    let mut mh = [[0.0; 2]; 2];
    for al in 0..sd {
        for be in 0..sd {
            mh[al][be] = dotn(&tau[al][..dim], &tau[be][..dim]);
        }
    }
    let mhi = inv(&mh, sd);
    let mut mhp = [[0.0; 2]; 2];
    for al in 0..sd {
        for be in 0..sd {
            mhp[al][be] = dotn(
                &taup[al * dim..(al + 1) * dim],
                &taup[be * dim..(be + 1) * dim],
            );
        }
    }
    let mhpi = inv(&mhp, sd);
    // trial traction
    let mut dx = [0.0; 3];
    for k in 0..nn {
        for i in 0..dim {
            dx[i] += (nf[k] - nfp[k]) * xk[k][i];
        }
    }
    let mut ttr = [0.0; 2];
    for al in 0..sd {
        for ga in 0..sd {
            for be in 0..sd {
                ttr[al] +=
                    tp[ga] * mhpi[ga][be] * dotn(&taup[be * dim..(be + 1) * dim], &tau[al][..dim]);
            }
        }
        ttr[al] -= eps_t * dotn(&tau[al][..dim], &dx[..dim]);
    }
    let q2: Real = (0..sd)
        .flat_map(|i| (0..sd).map(move |j| (i, j)))
        .map(|(i, j)| ttr[i] * mhi[i][j] * ttr[j])
        .sum();
    let q = q2.sqrt();
    let pn = eps_n * g.max(0.0);
    let stick = q <= mu * pn;
    let mut t = [0.0; 2];
    for al in 0..sd {
        t[al] = if stick {
            ttr[al]
        } else {
            mu * pn * ttr[al] / q
        };
    }
    // A^T v for a dim-vector v
    let at = |v: &[Real], out: &mut Vec<Real>| {
        out.iter_mut().for_each(|o| *o = 0.0);
        out[..dim].copy_from_slice(&v[..dim]);
        for k in 0..nn {
            for i in 0..dim {
                out[dim * (k + 1) + i] = -nf[k] * v[i];
            }
        }
    };
    let mut nvec = vec![0.0; nd];
    at(&nrm, &mut nvec);
    let mut tvec = vec![vec![0.0; nd]; 2];
    let mut nbar = vec![vec![0.0; nd]; 2];
    for al in 0..sd {
        at(&a[al][..dim], &mut tvec[al]);
        for k in 0..nn {
            for i in 0..dim {
                nbar[al][dim * (k + 1) + i] = dn[al][k] * nrm[i];
            }
        }
    }
    let mut kmat = vec![0.0; nd * nd];
    for j in 0..nd {
        let mut du = vec![0.0; nd];
        du[j] = 1.0;
        let dxs = &du[..dim];
        let dxk = |k: usize| &du[dim * (k + 1)..dim * (k + 2)];
        let _ = dxs;
        // δξ
        let mut dxi = [0.0; 2];
        for al in 0..sd {
            for be in 0..sd {
                dxi[al] += hinv[al][be] * (dotn(&tvec[be], &du) - g * dotn(&nbar[be], &du));
            }
        }
        // δa, δτ
        let mut dta = [[0.0; 3]; 2];
        for al in 0..sd {
            let mut da = [0.0; 3];
            for k in 0..nn {
                for i in 0..dim {
                    da[i] += dn[al][k] * dxk(k)[i];
                }
            }
            for be in 0..sd {
                for i in 0..dim {
                    da[i] += xab[al][be][i] * dxi[be];
                }
            }
            let that: Vec<Real> = (0..dim).map(|i| a[al][i] / alen[al]).collect();
            let c = dotn(&that, &da[..dim]);
            for i in 0..dim {
                dta[al][i] = sgn[al] * (da[i] - that[i] * c) / alen[al];
            }
        }
        let mut dmh = [[0.0; 2]; 2];
        for al in 0..sd {
            for be in 0..sd {
                dmh[al][be] =
                    dotn(&dta[al][..dim], &tau[be][..dim]) + dotn(&tau[al][..dim], &dta[be][..dim]);
            }
        }
        let mut dmhi = [[0.0; 2]; 2];
        for al in 0..sd {
            for be in 0..sd {
                for ga in 0..sd {
                    for th in 0..sd {
                        dmhi[al][be] -= mhi[al][ga] * dmh[ga][th] * mhi[th][be];
                    }
                }
            }
        }
        // δΔx, δt^tr
        let mut ddx = [0.0; 3];
        for k in 0..nn {
            for i in 0..dim {
                ddx[i] += (nf[k] - nfp[k]) * dxk(k)[i];
            }
        }
        for be in 0..sd {
            for i in 0..dim {
                ddx[i] += a[be][i] * dxi[be];
            }
        }
        let mut dttr = [0.0; 2];
        for al in 0..sd {
            for ga in 0..sd {
                for be in 0..sd {
                    dttr[al] += tp[ga]
                        * mhpi[ga][be]
                        * dotn(&taup[be * dim..(be + 1) * dim], &dta[al][..dim]);
                }
            }
            dttr[al] -=
                eps_t * (dotn(&dta[al][..dim], &dx[..dim]) + dotn(&tau[al][..dim], &ddx[..dim]));
        }
        let dg = -dotn(&nvec, &du);
        let dpn = eps_n * dg;
        let mut dt = [0.0; 2];
        if stick {
            dt = dttr;
        } else {
            let mut dq = 0.0;
            for al in 0..sd {
                for be in 0..sd {
                    dq += ttr[al] * mhi[al][be] * dttr[be] + 0.5 * ttr[al] * dmhi[al][be] * ttr[be];
                }
            }
            dq /= q;
            for be in 0..sd {
                dt[be] = mu * (dpn * ttr[be] / q + pn * dttr[be] / q - pn * ttr[be] * dq / (q * q));
            }
        }
        // δf
        let mut df = vec![0.0; nd];
        let mut atau = vec![0.0; nd];
        for al in 0..sd {
            at(&tau[al][..dim], &mut atau);
            // δ(Aᵀτ_α)
            let mut datau = vec![0.0; nd];
            datau[..dim].copy_from_slice(&dta[al][..dim]);
            for k in 0..nn {
                let dnk: Real = (0..sd).map(|ga| dn[ga][k] * dxi[ga]).sum();
                for i in 0..dim {
                    datau[dim * (k + 1) + i] = -nf[k] * dta[al][i] - dnk * tau[al][i];
                }
            }
            for be in 0..sd {
                for r in 0..nd {
                    df[r] += area
                        * (dt[be] * mhi[al][be] * atau[r]
                            + t[be] * dmhi[al][be] * atau[r]
                            + t[be] * mhi[al][be] * datau[r]);
                }
            }
        }
        for r in 0..nd {
            kmat[r + j * nd] = -df[r];
        }
    }
    Spec { k: kmat, t }
}

/// The port's friction tangent agrees with the independent
/// reference implementation of the specification to rounding (two codings
/// of the same formulas), on its configurations (curved 2-D and
/// warped 3-D masters, stick and slip).
#[test]
fn friction_tangent_matches_the_reviewers_reference_implementation() {
    for (dim3, mv) in [
        (None, vec![0.002, 0.0]),
        (None, vec![0.02, 0.0]),
        (Some(false), vec![0.002, 0.001, 0.0]),
        (Some(false), vec![0.02, 0.01, 0.0]),
    ] {
        for (mu, eps_t) in [(0.5, 1e3), (0.05, 1e4)] {
            let (shared, mut m) = setup(
                dim3,
                Master::Curved,
                &shift_of(dim3, Master::Curved),
                mu,
                eps_t,
                "penalty_linear",
                ContactUpstreamCompat::default(),
            );
            let slave = slave_near(&shared, &slave_point(dim3));
            let hist = history_step(&mut m, &shared, &mv);
            restore(&mut m, &hist);
            let ce = *m
                .contact_elements()
                .iter()
                .find(|c| c.slave == slave)
                .unwrap();
            if hist.elem.tuple(slave as usize)[0] != ce.master {
                continue;
            }
            let _ = m.local_forces(0, &ce).unwrap();
            let state =
                akantu_contact::contact_state_code(m.contact_state().tuple(slave as usize)[0]);
            let (_, kt) = m.local_moduli(0, &ce).unwrap();
            let spec = consistent_friction_tangent(&m, &shared, &ce, &hist, 1e3, eps_t, mu);
            let tport = m.tangential_tractions().tuple(slave as usize).to_vec();
            let e = rel(&kt, &spec.k);
            println!(
                "{}-D, mu {mu}, move {mv:?}: state {state}, traction port {tport:?} spec {:?}, ‖K_port − K_spec‖/‖K_spec‖ = {e:.2e}",
                if dim3.is_some() { 3 } else { 2 },
                &spec.t[..tport.len()]
            );
            for (a, b) in tport.iter().zip(&spec.t) {
                assert!(
                    (a - b).abs() <= 1e-12 * b.abs().max(1.0),
                    "traction {a} vs {b}"
                );
            }
            assert!(e < 1e-12, "port vs reference implementation: {e:e}");
        }
    }
}
