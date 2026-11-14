// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests of
// the contact mechanics model (src/model/contact_mechanics: resolution_penalty.cc
// computeTrialTangentialTraction, resolution.cc assembly, geometry_utils
// outsideDirection / isValidProjection):
//
// * the UD-365 correction (slip increment of a slave that changed
//   master facet) and the ½ two-pass weight on force **and** stiffness
//   (deformable master; assembled K = finite differences of the assembled
//   force);
// * frictionless contact keeps K symmetric;
// * UD-371: contact preserved under rigid rotations of 0…360°,
//   frame indifference of gaps, forces and stiffness;
// * UD-372 — projections on triangle facets lie in the triangle.

#![allow(clippy::needless_range_loop)] // entry indices mirror the formulas

mod common;

use akantu_contact::{
    ContactMechanicsModel, ContactUpstreamCompat, DeformableMasterWeighting, contact_state_code,
};
use akantu_core::enums::{ContactState, MatrixType};
use akantu_core::{Array, Element, ElementType, GhostType, Real};
use akantu_mesh::SharedMesh;
use akantu_solver::DofManager;
use common::*;

fn frob(a: &[Real]) -> Real {
    a.iter().map(|v| v * v).sum::<Real>().sqrt()
}

fn rel(a: &[Real], b: &[Real]) -> Real {
    let d: Vec<Real> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    frob(&d) / frob(b)
}

/// Node of `group` at the mesh coordinates `p0`.
fn node_at(shared: &SharedMesh, group: &str, p0: &[Real]) -> i32 {
    let m = shared.read().unwrap();
    let x = m.nodes();
    *nodes_of_physical(&m, group)
        .iter()
        .find(|&&n| {
            x.tuple(n as usize)
                .iter()
                .zip(p0)
                .all(|(a, b)| (a - b).abs() < 1e-12)
        })
        .unwrap()
}

// ---------------------------------------------------------------------------
// UD-365 (1)
// ---------------------------------------------------------------------------

/// Tangential traction (2-D: one component), state and master facet of
/// `slave` after a search and a force assembly.
fn traction_of(m: &mut ContactMechanicsModel, slave: i32) -> (Real, ContactState, Element) {
    m.search().unwrap();
    m.assemble_internal_forces().unwrap();
    let e = m
        .contact_elements()
        .iter()
        .find(|c| c.slave == slave)
        .expect("slave in contact")
        .master;
    (
        m.tangential_tractions().tuple(slave as usize)[0],
        m.contact_state().tuple(slave as usize)[0],
        e,
    )
}

/// The slip increment is measured on the current configuration with each
/// point on its own facet (UD-365 correction): a slave crossing to the next
/// master facet by a small relative slip δ while master and slave are
/// translated together sees the increment δ — the same traction change as a
/// slave sliding by δ inside a facet. Upstream's interpolation of the
/// current natural coordinate on the previous facet (compat switch) sees an
/// increment of almost a whole facet length instead (spurious slip with a
/// reversed traction), so forcing the default back to it fails this test.
#[test]
fn ud365_slip_increment_across_a_facet_change() {
    let mut out = Vec::new();
    for compat_ud365 in [false, true] {
        // lower: 8 facets of length 0.25 on [-1, 1]; upper: one slave
        // segment, slaves A (right end) and B (left end)
        let mesh = two_blocks_2d(8, 1.0, 1, 0.3);
        let (shared, mut m) =
            contact_model(mesh, &contact_dat("master", "slave", 0.5, 1e3, 1e3, false));
        m.set_upstream_compat(ContactUpstreamCompat {
            trial_slip_on_previous_element: compat_ud365,
            ..ContactUpstreamCompat::default()
        });
        let a = node_at(&shared, "slave", &[0.3, 0.0]);
        let b = node_at(&shared, "slave", &[-0.3, 0.0]);
        // A at x = 0.248 (facet [0, 0.25], ξ = 0.984), penetration 0.05
        translate_group(&mut m, &shared, "upper_body", &[-0.052, -0.05]);
        traction_of(&mut m, a);
        traction_of(&mut m, a);
        // small stick slide: non-zero traction t1 (same at A and B)
        translate_group(&mut m, &shared, "upper_body", &[-0.001, 0.0]);
        let (t1a, s1a, e1) = traction_of(&mut m, a);
        let t1b = m.tangential_tractions().tuple(b as usize)[0];
        assert_eq!(s1a, ContactState::Stick);
        assert!(t1a.abs() > 0.5, "t1 = {t1a}");
        // master and slave translated together by d, the slave body by a
        // further relative slip δ = 0.004: A crosses to facet [0.25, 0.5]
        let d = [0.1, 0.03];
        translate_group(&mut m, &shared, "lower_body", &d);
        translate_group(&mut m, &shared, "upper_body", &[d[0] + 0.004, d[1]]);
        let (t2a, s2a, e2) = traction_of(&mut m, a);
        let t2b = m.tangential_tractions().tuple(b as usize)[0];
        let s2b = m.contact_state().tuple(b as usize)[0];
        assert_ne!(e1, e2, "A must change master facet");
        out.push((compat_ud365, t1a, t2a, s2a, t1b, t2b, s2b));
    }
    for (compat, t1a, t2a, s2a, t1b, t2b, s2b) in &out {
        println!(
            "UD-365 compat {compat}: A {t1a:.6} → {t2a:.6} ({s2a:?}) | B (no facet change) {t1b:.6} → {t2b:.6} ({s2b:?})"
        );
    }
    let (_, t1a, t2a, s2a, t1b, t2b, s2b) = out[0];
    // default: the same change as B, which slid δ inside its facet: −ε_t δ
    assert_eq!(s2a, ContactState::Stick);
    assert_eq!(s2b, ContactState::Stick);
    assert!((t1a - t1b).abs() <= 1e-12 * t1b.abs(), "{t1a} vs {t1b}");
    assert!(
        ((t2a - t1a) - (t2b - t1b)).abs() <= 1e-9 * (t2b - t1b).abs(),
        "increment at A {} vs B {}",
        t2a - t1a,
        t2b - t1b
    );
    assert!(
        ((t2b - t1b).abs() - 1e3 * 0.004).abs() < 1e-9,
        "Δt_B = {}",
        t2b - t1b
    );
    // compat: almost a whole facet of spurious slip at A, opposite to the
    // true slip (the traction moves the wrong way, up to the slip limit)
    let (_, ct1a, ct2a, cs2a, _, ct2b, _) = out[1];
    assert_eq!(cs2a, ContactState::Slip);
    assert!(
        (ct2a - ct1a).signum() != (t2a - t1a).signum(),
        "upstream: increment opposite to the true slip expected"
    );
    assert!(
        (ct2b - t2b).abs() <= 1e-9 * t2b.abs(),
        "B is unaffected by the switch"
    );
}

// ---------------------------------------------------------------------------
// UD-365 (2): ½ two-pass weight on force and stiffness
// ---------------------------------------------------------------------------

/// Two-pass flat punch (master = slave = every interface segment),
/// deformable master.
fn two_pass(weighting: DeformableMasterWeighting) -> (SharedMesh, ContactMechanicsModel) {
    let mut mesh = two_blocks_2d(4, 1.0, 2, 0.4);
    let segs: Vec<Element> = (0..6)
        .map(|i| Element::try_new(ElementType::Segment2, i, GhostType::NotGhost).unwrap())
        .collect();
    add_group(&mut mesh, "contact", 1, &segs);
    let (shared, mut m) = contact_model(
        mesh,
        &contact_dat("contact", "contact", 0.0, 1e3, 0.0, true),
    );
    m.deformable_master_weighting = weighting;
    translate_group(&mut m, &shared, "upper_body", &[0.013, -0.01]);
    (shared, m)
}

/// Dense K (displacement DOFs, node-major) assembled by the model.
fn dense_k(m: &mut ContactMechanicsModel, shared: &SharedMesh) -> (Vec<Real>, usize, MatrixType) {
    let mut dofs = DofManager::new("dm", akantu_comm::Communicator::serial());
    dofs.register_nodal_dofs(&shared.read().unwrap(), "displacement", 2)
        .unwrap();
    m.assemble_stiffness_matrix(&mut dofs).unwrap();
    let n = m.positions().len() * 2;
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
    (d, n, k.matrix_type())
}

/// The ½ weight of `TwoPassHalf` applies to the force and the stiffness:
/// Σf = 0, f(½) = ½ f(1), K(½) = ½ K(1), and K(½) is the finite-difference
/// derivative of the assembled force (nodal areas frozen) — a stiffness
/// weighted differently from the force (upstream: ½ on the force only)
/// fails the last check, a weight of 1 for both the second.
#[test]
fn two_pass_half_weight_on_force_and_stiffness() {
    let (shared, mut half) = two_pass(DeformableMasterWeighting::TwoPassHalf);
    let (_, mut full) = two_pass(DeformableMasterWeighting::OnePassFull);
    half.search().unwrap();
    full.search().unwrap();
    assert!(half.contact_elements().len() >= 4);
    half.assemble_internal_forces().unwrap();
    full.assemble_internal_forces().unwrap();
    let fh = half.internal_force().as_slice().to_vec();
    let ff = full.internal_force().as_slice().to_vec();
    for c in 0..2 {
        let s: Real = fh.iter().skip(c).step_by(2).sum();
        let a: Real = fh.iter().skip(c).step_by(2).map(|v| v.abs()).sum();
        assert!(s.abs() <= 1e-14 * a, "Σf_{c} = {s:e} (Σ|f| = {a:e})");
    }
    for (h, f) in fh.iter().zip(&ff) {
        assert_eq!(h.to_bits(), (0.5 * f).to_bits());
    }
    let (kh, n, ty) = dense_k(&mut half, &shared);
    let (kf, _, _) = dense_k(&mut full, &shared);
    assert_eq!(ty, MatrixType::Symmetric);
    for (h, f) in kh.iter().zip(&kf) {
        assert_eq!(h.to_bits(), (0.5 * f).to_bits());
    }
    // finite differences of the assembled force, areas and pairs frozen
    let p0 = half.positions().clone();
    let areas = half.nodal_area().clone();
    let pairs: Vec<_> = half.contact_elements().to_vec();
    let h = 1e-7;
    let mut kfd = vec![0.0; n * n];
    let f_at = |m: &mut ContactMechanicsModel, p: &Array<Real>| -> Vec<Real> {
        m.positions_mut().copy_from(p).unwrap();
        m.search().unwrap();
        assert_eq!(m.contact_elements(), &pairs[..], "pairing changed under ±h");
        m.fields.nodal_area.borrow_mut().copy_from(&areas).unwrap();
        m.assemble_internal_forces().unwrap();
        m.internal_force().as_slice().to_vec()
    };
    for j in 0..n {
        let mut pp = p0.clone();
        pp.as_mut_slice()[j] += h;
        let mut pm = p0.clone();
        pm.as_mut_slice()[j] -= h;
        let fp = f_at(&mut half, &pp);
        let fm = f_at(&mut half, &pm);
        for i in 0..n {
            kfd[i + j * n] = -(fp[i] - fm[i]) / (2.0 * h);
        }
    }
    let e = rel(&kh, &kfd);
    println!(
        "two-pass ½: ‖K − K_fd‖/‖K_fd‖ = {e:.2e} ({} contact elements)",
        pairs.len()
    );
    assert!(e < 1e-6, "assembled K vs FD of the assembled force: {e:e}");
}

// ---------------------------------------------------------------------------
// Frictionless K stays symmetric
// ---------------------------------------------------------------------------

/// μ = 0 in the default mode: K keeps symmetric storage (upstream: K stays
/// symmetric); with friction in stick/slip the unsymmetric friction block
/// promotes it.
#[test]
fn frictionless_k_stays_symmetric() {
    for (mu, expected) in [(0.0, MatrixType::Symmetric), (0.3, MatrixType::Unsymmetric)] {
        let mesh = two_blocks_2d(4, 1.0, 2, 0.4);
        let (shared, mut m) =
            contact_model(mesh, &contact_dat("master", "slave", mu, 1e3, 1e3, true));
        translate_group(&mut m, &shared, "upper_body", &[0.013, -0.02]);
        m.search().unwrap();
        m.assemble_internal_forces().unwrap();
        translate_group(&mut m, &shared, "upper_body", &[0.003, 0.0]);
        m.search().unwrap();
        m.assemble_internal_forces().unwrap();
        assert!(!m.contact_elements().is_empty());
        let states: Vec<i32> = m
            .contact_elements()
            .iter()
            .map(|c| contact_state_code(m.contact_state().tuple(c.slave as usize)[0]))
            .collect();
        let (_, _, ty) = dense_k(&mut m, &shared);
        println!("μ = {mu}: states {states:?}, K storage {ty:?}");
        assert_eq!(ty, expected, "μ = {mu}");
    }
}

// ---------------------------------------------------------------------------
// UD-371: rigid rotations
// ---------------------------------------------------------------------------

fn rotate(p: &mut Array<Real>, x0: &[Real], deg: Real) {
    let a = deg.to_radians();
    let (c, s) = (a.cos(), a.sin());
    for (q, x) in p
        .as_mut_slice()
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(x0.as_chunks::<2>().0)
    {
        q[0] = c * x[0] - s * x[1];
        q[1] = s * x[0] + c * x[1];
    }
}

/// `_detection_quadrangle_4.msh`, slave body pushed by
/// 1e-3): 11 contact elements unrotated. The corrected orientation keeps
/// all 11 under every rigid rotation of 0…360°; upstream's (compat) loses
/// all of them for rotations in (90°, 270°) (at exactly 90° and 270° the
/// sign is rounding noise).
#[test]
fn ud371_contact_preserved_under_rigid_rotations() {
    let file = contact_mesh_path("test_detector/_detection_quadrangle_4.msh");
    let mesh = read_mesh(&file, 2);
    let slave_body = nodes_of_physical(&mesh, "slave_body");
    let mut x0 = mesh.nodes().as_slice().to_vec();
    drop(mesh);
    for &n in &slave_body {
        x0[2 * n as usize + 1] += 1e-3;
    }
    let dat = contact_dat("master", "slave", 0.0, 1e3, 0.0, false);
    for deg in [
        0.0, 30.0, 60.0, 89.0, 90.0, 91.0, 120.0, 180.0, 225.0, 269.0, 270.0, 271.0, 300.0, 359.0,
    ] {
        let mut counts = [0usize; 2];
        for (k, reference) in [false, true].into_iter().enumerate() {
            let (_s, mut m) = contact_model(read_mesh(&file, 2), &dat);
            m.set_upstream_compat(ContactUpstreamCompat {
                normal_orientation: reference,
                ..ContactUpstreamCompat::default()
            });
            rotate(m.positions_mut(), &x0, deg);
            m.search().unwrap();
            counts[k] = m.contact_elements().len();
        }
        println!(
            "rotation {deg:>5}°: corrected {} contacts, upstream orientation {}",
            counts[0], counts[1]
        );
        assert_eq!(counts[0], 11, "{deg}°: corrected orientation");
        if (91.0..=269.0).contains(&deg) {
            assert_eq!(counts[1], 0, "{deg}°: upstream orientation");
        } else if deg != 90.0 && deg != 270.0 {
            assert_eq!(counts[1], 11, "{deg}°: upstream orientation");
        }
    }
}

/// Rotation matrix (row-major `dim × dim`) of `deg` degrees about `axis`
/// (ignored in 2-D).
fn rotation(dim: usize, deg: Real, axis: [Real; 3]) -> [[Real; 3]; 3] {
    let a = deg.to_radians();
    let (c, s) = (a.cos(), a.sin());
    if dim == 2 {
        return [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
    }
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let (x, y, z) = (axis[0] / n, axis[1] / n, axis[2] / n);
    let t = 1.0 - c;
    [
        [c + x * x * t, x * y * t - z * s, x * z * t + y * s],
        [y * x * t + z * s, c + y * y * t, y * z * t - x * s],
        [z * x * t - y * s, z * y * t + x * s, c + z * z * t],
    ]
}

/// Contact state of a frictional sliding sequence on a curved deformable
/// master, every stage rotated rigidly by `r`; vectors and local moduli
/// rotated back (`Rᵀ v`, `Rᵀ K R` block-wise).
struct FrameState {
    pairs: Vec<akantu_contact::ContactElement>,
    gaps: Vec<Real>,
    areas: Vec<Real>,
    tractions: Vec<Real>,
    force: Vec<Real>,
    kn: Vec<Vec<Real>>,
    kt: Vec<Vec<Real>>,
}

fn frame_run(dim: usize, r: &[[Real; 3]; 3]) -> FrameState {
    let mesh = if dim == 2 {
        two_blocks_2d(8, 1.0, 2, 0.4)
    } else {
        two_blocks_3d(4, 1.0, 2, 0.4, false)
    };
    let x = mesh.nodes().as_slice().to_vec();
    let lower = nodes_of_physical(&mesh, "lower_body");
    let upper = nodes_of_physical(&mesh, "upper_body");
    let (_shared, mut m) =
        contact_model(mesh, &contact_dat("master", "slave", 0.3, 1e3, 1e3, true));
    // curved master: 2-D kinked arc, 3-D warped bilinear facets
    let mut base = x.clone();
    for &n in &lower {
        let p = &mut base[dim * n as usize..dim * (n as usize + 1)];
        if dim == 2 {
            p[1] += (p[1] + 1.0) * (0.25 - 0.6 * p[0] * p[0]);
        } else {
            p[2] += (p[2] + 1.0) * (-0.15 * (p[0] * p[0] + p[1] * p[1]));
        }
    }
    let rot = |v: &[Real]| -> Vec<Real> {
        v.chunks_exact(dim)
            .flat_map(|q| (0..dim).map(move |i| (0..dim).map(|k| r[i][k] * q[k]).sum::<Real>()))
            .collect()
    };
    let back = |v: &[Real]| -> Vec<Real> {
        v.chunks_exact(dim)
            .flat_map(|q| (0..dim).map(move |i| (0..dim).map(|k| r[k][i] * q[k]).sum::<Real>()))
            .collect()
    };
    let stages: [[Real; 3]; 4] = if dim == 2 {
        [
            [0.031, 0.12, 0.0],
            [0.031, 0.12, 0.0],
            [0.036, 0.12, 0.0],
            [0.051, 0.12, 0.0],
        ]
    } else {
        [
            [0.013, 0.007, -0.11],
            [0.013, 0.007, -0.11],
            [0.016, 0.009, -0.11],
            [0.033, 0.019, -0.11],
        ]
    };
    for st in stages {
        let mut p = base.clone();
        for &n in &upper {
            for d in 0..dim {
                p[dim * n as usize + d] += st[d];
            }
        }
        m.positions_mut().as_mut_slice().copy_from_slice(&rot(&p));
        m.search().unwrap();
        m.assemble_internal_forces().unwrap();
    }
    let pairs = m.contact_elements().to_vec();
    let back_k = |k: &[Real], nd: usize| -> Vec<Real> {
        let mut out = vec![0.0; nd * nd];
        for bi in 0..nd / dim {
            for bj in 0..nd / dim {
                for i in 0..dim {
                    for j in 0..dim {
                        let mut v = 0.0;
                        for p in 0..dim {
                            for q in 0..dim {
                                v += r[p][i] * k[dim * bi + p + (dim * bj + q) * nd] * r[q][j];
                            }
                        }
                        out[dim * bi + i + (dim * bj + j) * nd] = v;
                    }
                }
            }
        }
        out
    };
    let (mut kn, mut kt) = (Vec::new(), Vec::new());
    for e in &pairs {
        let (a, b) = m.local_moduli(0, e).unwrap();
        let nd = e.nb_nodes() * dim;
        kn.push(back_k(&a, nd));
        kt.push(back_k(&b, nd));
    }
    FrameState {
        gaps: m.gaps().as_slice().to_vec(),
        areas: m.nodal_area().as_slice().to_vec(),
        tractions: m.tangential_tractions().as_slice().to_vec(),
        force: back(m.internal_force().as_slice()),
        pairs,
        kn,
        kt,
    }
}

/// Frame indifference (corrected default): a frictional sliding sequence
/// rotated rigidly — including rotations that turn the master normal by
/// 90° or more — gives the same contact elements and gaps, and rotated
/// nodal forces, tractions and local moduli (normal and friction), within
/// rounding of the rotated input (≤ 1.2e-14 measured; bound 1e-12).
#[test]
fn ud371_frame_indifference_of_forces_and_stiffness() {
    for dim in [2, 3] {
        let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let s0 = frame_run(dim, &id);
        assert!(
            s0.pairs.len() >= 3,
            "{dim}-D: {} contact elements",
            s0.pairs.len()
        );
        assert!(
            s0.tractions.iter().any(|t| *t != 0.0),
            "friction must be active"
        );
        for (deg, axis) in [
            (37.0, [1.0, 0.0, 0.0]),
            (90.0, [1.0, 0.0, 0.0]),
            (135.0, [1.0, 2.0, 0.5]),
            (180.0, [0.0, 1.0, 0.0]),
            (250.0, [0.3, -1.0, 2.0]),
            (300.0, [0.0, 0.0, 1.0]),
        ] {
            let s = frame_run(dim, &rotation(dim, deg, axis));
            assert_eq!(s.pairs, s0.pairs, "{dim}-D {deg}°: contact elements");
            let e = [
                rel(&s.gaps, &s0.gaps),
                rel(&s.areas, &s0.areas),
                rel(&s.tractions, &s0.tractions),
                rel(&s.force, &s0.force),
                s.kn.iter()
                    .zip(&s0.kn)
                    .map(|(a, b)| rel(a, b))
                    .fold(0.0, Real::max),
                s.kt.iter()
                    .zip(&s0.kt)
                    .map(|(a, b)| rel(a, b))
                    .fold(0.0, Real::max),
            ];
            println!(
                "{dim}-D rotation {deg:>5}° about {axis:?}: rel. differences gaps {:.1e}, areas {:.1e}, tractions {:.1e}, force {:.1e}, K_N {:.1e}, K_T {:.1e}",
                e[0], e[1], e[2], e[3], e[4], e[5]
            );
            for v in e {
                assert!(v < 1e-12, "{dim}-D {deg}°: frame indifference {e:?}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// UD-372: projections on triangle facets
// ---------------------------------------------------------------------------

/// Natural coordinates of the accepted projections on a flat triangulated
/// master: the corrected rule keeps every one inside its triangle
/// (barycentric coordinates ≥ −tol); upstream's box accepts a coplanar
/// neighbour on which the projection lies outside (tie, last facet wins).
#[test]
fn ud372_triangle_projections_lie_in_their_facet() {
    let mut outside = [0usize; 2];
    for (k, upstream) in [false, true].into_iter().enumerate() {
        let mesh = two_blocks_3d(4, 1.0, 2, 0.4, true);
        let (shared, mut m) =
            contact_model(mesh, &contact_dat("master", "slave", 0.0, 1e3, 0.0, false));
        m.set_upstream_compat(ContactUpstreamCompat {
            projection_validity: upstream,
            ..ContactUpstreamCompat::default()
        });
        translate_group(&mut m, &shared, "upper_body", &[0.013, 0.007, -0.06]);
        m.search().unwrap();
        assert_eq!(m.contact_elements().len(), 9);
        for e in m.contact_elements() {
            assert_eq!(e.master.ty(), ElementType::Triangle3);
            let xi = m.fields.projections.tuple(e.slave as usize);
            let (a, b) = (xi[0], xi[1]);
            if a < -1e-5 || b < -1e-5 || 1.0 - a - b < -1e-5 {
                outside[k] += 1;
            }
        }
        // the gap and the normal do not depend on the coplanar facet
        for e in m.contact_elements() {
            assert!((m.gaps().tuple(e.slave as usize)[0] - 0.06).abs() < 1e-12);
        }
    }
    println!(
        "projections outside their triangle: corrected {}, upstream box {}",
        outside[0], outside[1]
    );
    assert_eq!(outside[0], 0);
    assert!(
        outside[1] > 0,
        "the configuration must exhibit UD-372 upstream"
    );
}
