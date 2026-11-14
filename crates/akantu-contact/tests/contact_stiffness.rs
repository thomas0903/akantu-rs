// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Intent of
// test/test_model/test_contact_mechanics_model/test_coupler/test_coupled_stiffness.cc
// (contact tangent stiffness against finite differences) and the UD-008
// check (full residual derivative test of the normal moduli).

#![allow(clippy::needless_range_loop)] // step/entry indices mirror the upstream test loops

mod common;

use akantu_contact::{ContactElement, ContactMechanicsModel, ContactUpstreamCompat};
use akantu_core::{Array, Real};
use akantu_mesh::SharedMesh;
use common::*;

/// Local normal force of the contact element of `slave` after a search on
/// `positions`, with the nodal area frozen at `area`.
fn local_force(
    model: &mut ContactMechanicsModel,
    shared: &SharedMesh,
    positions: &Array<Real>,
    slave: i32,
    master: akantu_core::Element,
    area: Real,
) -> Option<Vec<Real>> {
    model.positions_mut().copy_from(positions).unwrap();
    model.search().unwrap();
    let ce = *model.contact_elements().iter().find(|c| c.slave == slave)?;
    if ce.master != master {
        return None;
    }
    model
        .fields
        .nodal_area
        .borrow_mut()
        .tuple_mut(slave as usize)[0] = area;
    let _ = shared;
    let (f, _) = model.local_forces(0, &ce).unwrap();
    Some(f)
}

/// `(K, K_fd)` of the normal moduli of the contact element of `slave`.
fn normal_moduli_vs_fd(
    model: &mut ContactMechanicsModel,
    shared: &SharedMesh,
    slave: i32,
    h: Real,
) -> (Vec<Real>, Vec<Real>, usize) {
    let p0 = model.positions().clone();
    model.search().unwrap();
    let ce: ContactElement = *model
        .contact_elements()
        .iter()
        .find(|c| c.slave == slave)
        .expect("slave in contact");
    let area = model.nodal_area().tuple(slave as usize)[0];
    let dim = model.spatial_dimension();
    let nd = ce.nb_nodes() * dim;
    let (k, _) = model.local_moduli(0, &ce).unwrap();
    let m = shared.read().unwrap();
    let mut conn = vec![ce.slave];
    conn.extend_from_slice(&m.element_connectivity(ce.master).unwrap());
    drop(m);
    let mut kfd = vec![0.0; nd * nd];
    for j in 0..nd {
        let (node, comp) = (conn[j / dim] as usize, j % dim);
        let mut pp = p0.clone();
        pp.tuple_mut(node)[comp] += h;
        let mut pm = p0.clone();
        pm.tuple_mut(node)[comp] -= h;
        let fp =
            local_force(model, shared, &pp, slave, ce.master, area).expect("same pairing (+h)");
        let fm =
            local_force(model, shared, &pm, slave, ce.master, area).expect("same pairing (−h)");
        for i in 0..nd {
            kfd[i + j * nd] = -(fp[i] - fm[i]) / (2.0 * h);
        }
    }
    model.positions_mut().copy_from(&p0).unwrap();
    (k, kfd, nd)
}

fn frob(a: &[Real]) -> Real {
    a.iter().map(|v| v * v).sum::<Real>().sqrt()
}

/// Two blocks; the master facet under the slave is tilted (its right node
/// raised) and the slave projects away from the facet midpoint.
fn tilted_setup(
    law: &str,
    compat: ContactUpstreamCompat,
) -> (SharedMesh, ContactMechanicsModel, i32) {
    let mesh = two_blocks_2d(4, 1.0, 1, 0.3);
    let dat = contact_dat("master", "slave", 0.0, 1e3, 0.0, true).replace("penalty_linear", law);
    let (shared, mut model) = contact_model(mesh, &dat);
    model.set_upstream_compat(compat);
    translate_group(&mut model, &shared, "upper_body", &[0.0, -0.02]);
    // raise the lower-top node at x = 0.5
    let m = shared.read().unwrap();
    let x = m.nodes().clone();
    let slave = (0..m.nb_nodes())
        .find(|&n| {
            let p = x.tuple(n);
            (p[0] - 0.3).abs() < 1e-12
                && p[1].abs() < 1e-12
                && nodes_of_physical(&m, "slave").contains(&(n as i32))
        })
        .unwrap() as i32;
    let raised = (0..m.nb_nodes())
        .find(|&n| {
            let p = x.tuple(n);
            (p[0] - 0.5).abs() < 1e-12
                && p[1].abs() < 1e-12
                && nodes_of_physical(&m, "master").contains(&(n as i32))
        })
        .unwrap();
    drop(m);
    model.positions_mut().tuple_mut(raised)[1] += 0.05;
    (shared, model, slave)
}

fn rel_err(k: &[Real], kfd: &[Real]) -> Real {
    let diff: Vec<Real> = k.iter().zip(kfd).map(|(a, b)| a - b).collect();
    frob(&diff) / frob(kfd)
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

/// UD-368: the consistent normal moduli are the derivative of the penalty
/// normal force (frozen nodal area) for both pressure laws in 2-D on a
/// tilted master segment, and symmetric; upstream's expression is not
/// (≈ 20 % here, the master–slave coupling terms have the wrong sign).
#[test]
fn normal_moduli_are_the_force_derivative_2d() {
    for law in ["penalty_linear", "penalty_quadratic"] {
        let (shared, mut model, slave) = tilted_setup(law, ContactUpstreamCompat::default());
        let (k, kfd, nd) = normal_moduli_vs_fd(&mut model, &shared, slave, 1e-6);
        let e = rel_err(&k, &kfd);
        assert!(e < 1e-7, "{law}: ‖K − K_fd‖/‖K_fd‖ = {e:.3e}");
        assert!(asym(&k, nd) < 1e-14, "{law}: asymmetry {}", asym(&k, nd));
        let (shared, mut model, slave) = tilted_setup(law, ContactUpstreamCompat::all());
        let (k, kfd, _) = normal_moduli_vs_fd(&mut model, &shared, slave, 1e-6);
        let e = rel_err(&k, &kfd);
        assert!(
            e > 1e-2,
            "{law}: upstream moduli unexpectedly consistent ({e:.3e})"
        );
    }
}

/// Same in 3-D on a warped bilinear quadrangle (curvature term) and on a
/// tilted triangle.
#[test]
fn normal_moduli_are_the_force_derivative_3d() {
    for tets in [false, true] {
        for law in ["penalty_linear", "penalty_quadratic"] {
            let mesh = two_blocks_3d(2, 1.0, 1, 0.3, tets);
            let dat =
                contact_dat("master", "slave", 0.0, 1e3, 0.0, true).replace("penalty_linear", law);
            let (shared, mut model) = contact_model(mesh, &dat);
            translate_group(&mut model, &shared, "upper_body", &[0.07, 0.11, -0.03]);
            // warp/tilt the master facets: raise the lower-top corner (1, 1, 0)
            // and lower the centre node (0, 0, 0)
            let (slave, raised, centre) = {
                let m = shared.read().unwrap();
                let x = m.nodes();
                let find = |p0: [Real; 3], group: &str| {
                    let g = nodes_of_physical(&m, group);
                    *g.iter()
                        .find(|&&n| {
                            let p = x.tuple(n as usize);
                            (0..3).all(|d| (p[d] - p0[d]).abs() < 1e-12)
                        })
                        .unwrap()
                };
                (
                    find([0.3, 0.3, 0.0], "slave"),
                    find([1.0, 1.0, 0.0], "master"),
                    find([0.0, 0.0, 0.0], "master"),
                )
            };
            model.positions_mut().tuple_mut(raised as usize)[2] += 0.1;
            model.positions_mut().tuple_mut(centre as usize)[2] -= 0.02;
            let (k, kfd, nd) = normal_moduli_vs_fd(&mut model, &shared, slave, 1e-6);
            let e = rel_err(&k, &kfd);
            assert!(
                e < 1e-6,
                "tets = {tets}, {law}: ‖K − K_fd‖/‖K_fd‖ = {e:.3e}"
            );
            assert!(asym(&k, nd) < 1e-13, "asymmetry {}", asym(&k, nd));
        }
    }
}
