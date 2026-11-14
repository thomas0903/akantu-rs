// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): independent checks (D-007) of
// src/model/solid_mechanics/solid_mechanics_model_embedded_interface/* and
// materials/material_embedded/* (no oracle: the upstream model does not
// compile at the reference commit).

//! Independent mathematical checks of the embedded model in 2-D (D-007;
//! tolerances justified in `docs/NUMERICAL_VALIDATION.md`):
//!
//! * interface-mesh topology invariants (tiling of every bar, pieces inside
//!   their background element, lengths, node count, names);
//! * patch test: an affine background displacement gives the exact axial
//!   strain `tᵀ sym(H) t`, stress `E ε + σ_p`, `gradu_embedded = H`,
//!   directing cosines, energy `½ A (Eε + σ_p) ε L` — with several
//!   reinforcement laws over mixed `_triangle_3` / `_triangle_6` background
//!   elements (UD-590 regression);
//! * the reinforcement stiffness equals the closed form
//!   `Σ A E L g gᵀ`, `g_{a,i} = t_i (t·∇N_a)` built from the geometry alone,
//!   and annihilates the rigid-body modes;
//! * a bar in a matrix: uniaxial extension of a block with full-length bars
//!   (reaction `(E_c H + Σ E_s A) δ / L`, exact linear field);
//! * pre-stress equilibrium: nodal forces of a pre-stressed bar at rest are
//!   `−A σ_p t (N(Q) − N(P))`; layered pre-stressed bars on a free block
//!   give the exact uniform state `ε = −α σ_p / (E_c + α E_s)`.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_embedded::{
    EmbeddedInterfaceModel, EmbeddedInterfaceModelOptions, INTERFACE_FE_ENGINE,
    MaterialReinforcement,
};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solid::Material;

const DIM: usize = 2;

/// Bars of the 2-D checks: name → polyline (collinear primitive segments).
fn bars() -> Vec<(&'static str, Vec<[Real; 2]>)> {
    vec![
        // two collinear primitive segments
        ("bar_a", vec![[0.13, 0.21], [1.0, 0.49], [1.87, 0.77]]),
        ("bar_b", vec![[0.31, 0.93], [1.42, 0.08]]),
        // along the mesh line y = 0.5 (edges shared by two triangles)
        ("bar_c", vec![[0.05, 0.5], [1.95, 0.5]]),
        ("bar_d", vec![[0.6, 0.1], [0.6, 0.9]]),
    ]
}

fn primitive_of(bars: &[(&str, Vec<[Real; 2]>)]) -> Mesh {
    let mut nodes: Vec<Vec<Real>> = Vec::new();
    let mut segs = Vec::new();
    let mut names = Vec::new();
    for (name, pts) in bars {
        let first = nodes.len();
        for p in pts {
            nodes.push(p.to_vec());
        }
        for k in 0..pts.len() - 1 {
            segs.push([
                i32::try_from(first + k).unwrap(),
                i32::try_from(first + k + 1).unwrap(),
            ]);
            names.push(*name);
        }
    }
    let refs: Vec<&[Real]> = nodes.iter().map(Vec::as_slice).collect();
    common::reinforcement_mesh(DIM, &refs, &segs, &names)
}

/// Input file: concrete + one reinforcement law per bar.
fn dat(e_c: Real, nu: Real, laws: &[(&str, Real, Real, Real)]) -> String {
    let mut s = format!(
        "material elastic [\n name = concrete\n E = {e_c:e}\n nu = {nu}\n Plane_Stress = true\n]\n"
    );
    for (name, e, a, sp) in laws {
        s += &format!(
            "material reinforcement elastic [\n name = {name}\n E = {e:e}\n area = {a:e}\n pre_stress = {sp:e}\n]\n"
        );
    }
    s
}

fn law<'a>(m: &'a EmbeddedInterfaceModel, name: &str) -> &'a MaterialReinforcement<DIM> {
    m.smm
        .material_by_name(name)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialReinforcement<DIM>>()
        .unwrap()
}

fn standard_laws() -> Vec<(&'static str, Real, Real, Real)> {
    vec![
        ("bar_a", 2.0e4, 0.01, 3.5),
        ("bar_b", 1.5e4, 0.02, -1.25),
        ("bar_c", 3.0e4, 0.005, 0.0),
        ("bar_d", 2.5e4, 0.015, 7.0),
    ]
}

fn init(mesh: Mesh, bars: &[(&str, Vec<[Real; 2]>)], dat: &str) -> EmbeddedInterfaceModel {
    let mut model = common::model(mesh, primitive_of(bars), dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    model
}

fn unit(p: &[Real; 2], q: &[Real; 2]) -> [Real; 2] {
    let l = common::dist(p, q);
    [(q[0] - p[0]) / l, (q[1] - p[1]) / l]
}

#[test]
fn interface_topology_invariants_2d() {
    let mesh = common::grid_triangles(5, 4, 2.0, 1.0, "concrete");
    let bars = bars();
    let model = init(mesh, &bars, &dat(1e3, 0.25, &standard_laws()));
    let segs = common::interface_segments(&model);
    let im = model.interface_mesh();
    // two new nodes per segment, no sharing
    assert_eq!(im.nb_nodes(), 2 * segs.len());
    let background = model.smm.physics.base.mesh.read().unwrap();
    for (name, pts) in &bars {
        let p = pts[0];
        let q = pts[pts.len() - 1];
        let l = common::dist(&p, &q);
        let t = unit(&p, &q);
        let mut pieces: Vec<(Real, Real)> = Vec::new();
        for (x0, x1, n, el) in &segs {
            if n != name {
                continue;
            }
            assert_eq!(el.ghost_type(), GhostType::NotGhost);
            assert_eq!(el.ty(), ElementType::Triangle3);
            // both end points inside (or on) the associated element
            let xe = common::element_coords(&background, el.ty(), el.idx());
            for x in [x0, x1] {
                for lam in common::barycentric(&xe, x) {
                    assert!(
                        (-1e-12..=1. + 1e-12).contains(&lam),
                        "{name}: point {x:?} outside {el} ({lam})"
                    );
                }
                // on the bar
                let s = (x[0] - p[0]) * t[0] + (x[1] - p[1]) * t[1];
                let off = [(x[0] - p[0]) - s * t[0], (x[1] - p[1]) - s * t[1]];
                assert!(
                    off[0].hypot(off[1]) <= 1e-14 * l.max(1.0),
                    "{name}: off the bar"
                );
            }
            let s0 = (x0[0] - p[0]) * t[0] + (x0[1] - p[1]) * t[1];
            let s1 = (x1[0] - p[0]) * t[0] + (x1[1] - p[1]) * t[1];
            pieces.push((s0.min(s1), s0.max(s1)));
        }
        assert!(!pieces.is_empty(), "{name}: no interface segment");
        pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
        // the pieces tile [0, L]: no gap, no overlap, no duplicate, no
        // degenerate piece
        assert!(
            pieces[0].0.abs() <= 1e-12 * l,
            "{name}: starts at {}",
            pieces[0].0
        );
        assert!(
            (pieces[pieces.len() - 1].1 - l).abs() <= 1e-12 * l,
            "{name}: ends at {}",
            pieces[pieces.len() - 1].1
        );
        for w in pieces.windows(2) {
            assert!(
                (w[0].1 - w[1].0).abs() <= 1e-12 * l,
                "{name}: gap/overlap {:?} {:?}",
                w[0],
                w[1]
            );
        }
        let total: Real = pieces.iter().map(|(a, b)| b - a).sum();
        assert!(
            (total - l).abs() <= 1e-12 * l,
            "{name}: length {total} vs {l}"
        );
        for (a, b) in &pieces {
            assert!(b - a > 1e-12 * l, "{name}: degenerate piece");
        }
    }
    // the bar along the mesh line is cut at every vertex it passes, once
    let n_c = segs.iter().filter(|s| s.2 == "bar_c").count();
    // x = 0.05 … 1.95 crosses the vertical lines x = 0.4, 0.8, 1.2, 1.6 and
    // the diagonals through them: 5 pieces along the shared edges
    assert_eq!(n_c, 5);
    // each law received exactly the segments of its name
    for (name, _, _, _) in standard_laws() {
        let l = law(&model, name);
        let filter = l
            .base()
            .element_filter
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap();
        let expected = segs.iter().filter(|s| s.2 == name).count();
        assert_eq!(filter.len(), expected, "{name}");
        // the law is one-dimensional (UD-589)
        assert_eq!(l.base().stress.nb_component(), 1);
        assert_eq!(l.base().grad_u.nb_component(), 1);
        assert_eq!(l.base().spatial_dimension, 1);
    }
}

/// Affine field `u = H x + c` on every background node.
fn impose_affine(model: &mut EmbeddedInterfaceModel, h: &[[Real; 2]; 2], c: &[Real; 2]) {
    let nodes = common::nodes(model);
    let mut u = model.smm.displacement_mut().unwrap();
    for (n, x) in nodes.iter().enumerate() {
        for i in 0..DIM {
            u.as_mut_slice()[n * DIM + i] = h[i][0] * x[0] + h[i][1] * x[1] + c[i];
        }
    }
}

fn check_affine_patch(model: &mut EmbeddedInterfaceModel, laws: &[(&str, Real, Real, Real)]) {
    let bars = bars();
    let h = [[1.3e-3, -0.7e-3], [0.4e-3, -2.1e-3]];
    impose_affine(model, &h, &[0.25, -0.5]);
    model.assemble_internal_forces().unwrap();
    let nq = model
        .fe_engine(INTERFACE_FE_ENGINE)
        .unwrap()
        .nb_integration_points(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    let hmax = 2.1e-3;
    for (name, e, a, sp) in laws {
        let pts = &bars.iter().find(|b| b.0 == *name).unwrap().1;
        let (p, q) = (pts[0], pts[pts.len() - 1]);
        let t = unit(&p, &q);
        let l = common::dist(&p, &q);
        let sym = |i: usize, j: usize| 0.5 * (h[i][j] + h[j][i]);
        let eps = t[0] * t[0] * sym(0, 0) + 2. * t[0] * t[1] * sym(0, 1) + t[1] * t[1] * sym(1, 1);
        let law = law(model, name);
        let g = GhostType::NotGhost;
        let ty = ElementType::Segment2;
        let gu = law.base().grad_u.get(ty, g).unwrap();
        let sigma = law.base().stress.get(ty, g).unwrap();
        let ge = law.gradu_embedded.get(ty, g).unwrap();
        let dc = law.directing_cosines.get(ty, g).unwrap();
        assert!(!gu.is_empty());
        assert_eq!(gu.len(), sigma.len());
        for k in 0..gu.len() {
            let x = gu.as_slice()[k];
            assert!((x - eps).abs() <= 1e-12 * hmax, "{name}: ε {x} vs {eps}");
            let s = sigma.as_slice()[k];
            let s_ref = e * eps + sp;
            assert!(
                (s - s_ref).abs() <= 1e-12 * (e * hmax + sp.abs()),
                "{name}: σ {s} vs {s_ref}"
            );
            let du = ge.get_tuple(k).unwrap();
            for i in 0..DIM {
                for j in 0..DIM {
                    assert!(
                        (du[i + j * DIM] - h[i][j]).abs() <= 1e-12 * hmax,
                        "{name}: ∇u"
                    );
                }
            }
            let c = dc.get_tuple(k).unwrap();
            let c_ref = [t[0] * t[0], t[1] * t[1], t[0] * t[1]];
            for (x, y) in c.iter().zip(c_ref) {
                assert!((x - y).abs() <= 1e-14, "{name}: cosines {c:?} vs {c_ref:?}");
            }
        }
        assert_eq!(gu.len() % nq, 0);
        drop(gu);
        drop(sigma);
        drop(ge);
        drop(dc);
        // energy of the law: A ∫ ½ σ ε ds = ½ A (E ε + σ_p) ε L
        let idx = model.smm.material_index(name).unwrap();
        let w = model
            .smm
            .with_material(
                idx,
                akantu_model::constitutive_law::LawPhase::Energy,
                |m, ctx| m.energy_of(ctx, "potential"),
            )
            .unwrap();
        let w_ref = 0.5 * a * (e * eps + sp) * eps * l;
        assert!(
            (w - w_ref).abs() <= 1e-12 * (0.5 * a * (e * hmax + sp.abs()) * hmax * l),
            "{name}: energy {w} vs {w_ref}"
        );
    }
}

#[test]
fn affine_patch_test_2d() {
    let mesh = common::grid_triangles(5, 4, 2.0, 1.0, "concrete");
    let laws = standard_laws();
    let mut model = init(mesh, &bars(), &dat(1e3, 0.25, &laws));
    check_affine_patch(&mut model, &laws);
}

/// Background mixing `_triangle_3` (left square) and `_triangle_6` (right
/// square, own nodes) with the bars of two laws crossing both: the
/// per-position background data of UD-590 (upstream indexes them by
/// interface element id and pair index, wrong as soon as a law does not own
/// every segment or two background types exist).
fn mixed_mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    let (n3, n6);
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let n = 3usize;
        let id3 = |i: usize, j: usize| i32::try_from(j * (n + 1) + i).unwrap();
        for j in 0..=n {
            for i in 0..=n {
                acc.push_node(&[i as Real / n as Real, j as Real / n as Real])
                    .unwrap();
            }
        }
        let mut t3 = Vec::new();
        for j in 0..n {
            for i in 0..n {
                t3.push([id3(i, j), id3(i + 1, j), id3(i + 1, j + 1)]);
                t3.push([id3(i, j), id3(i + 1, j + 1), id3(i, j + 1)]);
            }
        }
        // right square: quadratic triangles on a 2 × 2 grid with mid-side
        // nodes (straight edges: affine geometry)
        let m = 2usize;
        let base = (n + 1) * (n + 1);
        let g = 2 * m + 1; // nodes per row of the refined grid
        let id6 = |i: usize, j: usize| i32::try_from(base + j * g + i).unwrap();
        for j in 0..g {
            for i in 0..g {
                acc.push_node(&[
                    1. + i as Real / (2 * m) as Real,
                    j as Real / (2 * m) as Real,
                ])
                .unwrap();
            }
        }
        let mut t6 = Vec::new();
        for j in 0..m {
            for i in 0..m {
                let (i2, j2) = (2 * i, 2 * j);
                // corners a=(i2,j2) b=(i2+2,j2) c=(i2+2,j2+2) d=(i2,j2+2)
                t6.push([
                    id6(i2, j2),
                    id6(i2 + 2, j2),
                    id6(i2 + 2, j2 + 2),
                    id6(i2 + 1, j2),
                    id6(i2 + 2, j2 + 1),
                    id6(i2 + 1, j2 + 1),
                ]);
                t6.push([
                    id6(i2, j2),
                    id6(i2 + 2, j2 + 2),
                    id6(i2, j2 + 2),
                    id6(i2 + 1, j2 + 1),
                    id6(i2 + 1, j2 + 2),
                    id6(i2, j2 + 1),
                ]);
            }
        }
        let mut c3 = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for t in &t3 {
            c3.try_push_tuple(t).unwrap();
        }
        drop(c3);
        n3 = t3.len();
        let mut c6 = acc.connectivity_mut(ElementType::Triangle6, GhostType::NotGhost);
        for t in &t6 {
            c6.try_push_tuple(t).unwrap();
        }
        n6 = t6.len();
    }
    common::name_elements(&mut mesh, ElementType::Triangle3, n3, "concrete");
    common::name_elements(&mut mesh, ElementType::Triangle6, n6, "concrete");
    mesh
}

#[test]
fn affine_patch_test_mixed_background_types_two_laws() {
    // bar_a and bar_b cross from the T3 square into the T6 square
    let bars: Vec<(&str, Vec<[Real; 2]>)> = bars()
        .into_iter()
        .filter(|(n, _)| *n == "bar_a" || *n == "bar_b")
        .collect();
    let laws: Vec<_> = standard_laws()
        .into_iter()
        .filter(|(n, ..)| *n == "bar_a" || *n == "bar_b")
        .collect();
    let mut model = init(mixed_mesh(), &bars, &dat(1e3, 0.25, &laws));
    for (name, ..) in &laws {
        let bg = law(&model, name)
            .background_elements(ElementType::Segment2)
            .unwrap();
        assert!(
            bg.iter().any(|e| e.ty() == ElementType::Triangle3),
            "{name}"
        );
        assert!(
            bg.iter().any(|e| e.ty() == ElementType::Triangle6),
            "{name}"
        );
    }
    // law bar_b does not own the first interface elements: its positions
    // differ from the element ids
    let fb = law(&model, "bar_b")
        .base()
        .element_filter
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    assert!(
        fb.iter()
            .enumerate()
            .any(|(p, &e)| usize::try_from(e).unwrap() != p)
    );
    check_affine_patch(&mut model, &laws);
}

/// Dense `K` of the model (both triangles of the symmetric storage).
fn dense_k(model: &EmbeddedInterfaceModel, n: usize) -> Vec<Vec<Real>> {
    let k = model.smm.dof_manager().matrix("K").unwrap();
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| k.get(i64::try_from(i).unwrap(), i64::try_from(j).unwrap()))
                .collect()
        })
        .collect()
}

#[test]
fn reinforcement_stiffness_closed_form_and_rigid_modes_2d() {
    let mesh = common::grid_triangles(5, 4, 2.0, 1.0, "concrete");
    let laws = standard_laws();
    // concrete E = 0: K is the reinforcement stiffness alone
    let mut model = init(mesh, &bars(), &dat(0.0, 0.25, &laws));
    model.assemble_stiffness_matrix(true).unwrap();
    let nodes = common::nodes(&model);
    let ndof = nodes.len() * DIM;
    let k = dense_k(&model, ndof);
    // closed form from the geometry alone
    let mut k_ref = vec![vec![0.0; ndof]; ndof];
    let background = model.smm.physics.base.mesh.read().unwrap();
    for (x0, x1, name, el) in common::interface_segments(&model) {
        let (_, e, a, _) = laws.iter().find(|l| l.0 == name).unwrap();
        let ls = common::dist(&x0, &x1);
        let t = [(x1[0] - x0[0]) / ls, (x1[1] - x0[1]) / ls];
        let xe = common::element_coords(&background, el.ty(), el.idx());
        let grad = common::simplex_gradients(&xe);
        let conn = background
            .connectivity(el.ty(), GhostType::NotGhost)
            .unwrap()
            .get_tuple(el.idx())
            .unwrap()
            .to_vec();
        let mut gvec = Vec::new();
        for (a_loc, &node) in conn.iter().enumerate() {
            let dn = t[0] * grad[a_loc][0] + t[1] * grad[a_loc][1];
            for (i, ti) in t.iter().enumerate() {
                gvec.push((usize::try_from(node).unwrap() * DIM + i, ti * dn));
            }
        }
        for &(r, gr) in &gvec {
            for &(c, gc) in &gvec {
                k_ref[r][c] += a * e * ls * gr * gc;
            }
        }
    }
    let kmax = k_ref.iter().flatten().fold(0.0_f64, |m, x| m.max(x.abs()));
    assert!(kmax > 0.);
    for r in 0..ndof {
        for c in 0..ndof {
            assert!(
                (k[r][c] - k_ref[r][c]).abs() <= 1e-12 * kmax,
                "K({r},{c}) = {} vs closed form {}",
                k[r][c],
                k_ref[r][c]
            );
        }
    }
    // rigid-body modes: two translations and the infinitesimal rotation
    let modes: Vec<Vec<Real>> = vec![
        nodes.iter().flat_map(|_| [1.0, 0.0]).collect(),
        nodes.iter().flat_map(|_| [0.0, 1.0]).collect(),
        nodes.iter().flat_map(|x| [-x[1], x[0]]).collect(),
    ];
    for (m, r) in modes.iter().enumerate() {
        for row in &k {
            let kr: Real = row.iter().zip(r).map(|(a, b)| a * b).sum();
            assert!(kr.abs() <= 1e-12 * kmax * 4.0, "mode {m}: (K r) = {kr}");
        }
    }
}

#[test]
fn bar_in_matrix_uniaxial_closed_form_2d() {
    let (lx, ly) = (2.0, 1.0);
    let (e_c, nu) = (1e3, 0.3);
    let mesh = common::grid_triangles(6, 3, lx, ly, "concrete");
    // full-length horizontal bars at heights off the mesh lines
    let bars: Vec<(&str, Vec<[Real; 2]>)> = vec![
        ("bar_a", vec![[0.0, 0.37], [lx, 0.37]]),
        ("bar_b", vec![[0.0, 0.81], [1.1, 0.81], [lx, 0.81]]),
    ];
    let laws = vec![("bar_a", 2.0e4, 0.01, 0.0), ("bar_b", 3.0e4, 0.02, 0.0)];
    let mut model = init(mesh, &bars, &dat(e_c, nu, &laws));
    let nodes = common::nodes(&model);
    let delta = 1e-3;
    {
        let mut b = vec![false; nodes.len() * DIM];
        let mut u = vec![0.0; nodes.len() * DIM];
        for (n, x) in nodes.iter().enumerate() {
            if x[0] == 0.0 {
                b[n * DIM] = true;
            }
            if x[0] == lx {
                b[n * DIM] = true;
                u[n * DIM] = delta;
            }
            if x[0] == 0.0 && x[1] == 0.0 {
                b[n * DIM + 1] = true;
            }
        }
        model
            .smm
            .blocked_dofs_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&b);
        model
            .smm
            .displacement_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&u);
    }
    model.solve_step(None).unwrap();
    let eps = delta / lx;
    let u = model.smm.displacement().unwrap().as_slice().to_vec();
    let mut max_err: Real = 0.0;
    for (n, x) in nodes.iter().enumerate() {
        max_err = max_err
            .max((u[n * DIM] - eps * x[0]).abs() / delta)
            .max((u[n * DIM + 1] + nu * eps * x[1]).abs() / delta);
        assert!(
            (u[n * DIM] - eps * x[0]).abs() <= 1e-10 * delta,
            "u_x at {x:?}"
        );
        assert!(
            (u[n * DIM + 1] + nu * eps * x[1]).abs() <= 1e-10 * delta,
            "u_y at {x:?}"
        );
    }
    for (name, e, ..) in &laws {
        for s in law(&model, name)
            .base()
            .stress
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .as_slice()
        {
            assert!((s - e * eps).abs() <= 1e-10 * e * eps, "{name}: σ {s}");
        }
    }
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    let reaction: Real = nodes
        .iter()
        .enumerate()
        .filter(|(_, x)| x[0] == lx)
        .map(|(n, _)| f[n * DIM])
        .sum();
    let expected = (e_c * ly + laws.iter().map(|l| l.1 * l.2).sum::<Real>()) * eps;
    assert!(
        (reaction.abs() - expected).abs() <= 1e-10 * expected,
        "reaction {reaction} vs {expected}"
    );
    eprintln!(
        "bar in matrix: max |u − u_exact|/δ = {max_err:.2e}, reaction rel. error = {:.2e}",
        (reaction.abs() - expected).abs() / expected
    );
    // potential energy = ½ R δ
    let w = model.energy("potential").unwrap();
    assert!(
        (w - 0.5 * expected * delta).abs() <= 1e-10 * w,
        "energy {w}"
    );
}

#[test]
fn prestress_at_rest_nodal_forces_closed_form_2d() {
    let mesh = common::grid_triangles(5, 4, 2.0, 1.0, "concrete");
    let laws = standard_laws();
    let mut model = init(mesh, &bars(), &dat(1e3, 0.25, &laws));
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    let nodes = common::nodes(&model);
    let mut f_ref = vec![0.0; nodes.len() * DIM];
    let background = model.smm.physics.base.mesh.read().unwrap();
    let segs = common::interface_segments(&model);
    for (name, pts) in bars() {
        let (_, _, a, sp) = laws.iter().find(|l| l.0 == name).unwrap();
        let (p, q) = (pts[0], pts[pts.len() - 1]);
        let t = unit(&p, &q);
        // N(Q) − N(P): in the background elements of the first and last
        // pieces
        for (end, sign) in [(q, 1.0), (p, -1.0)] {
            let (_, _, _, el) = segs
                .iter()
                .filter(|s| s.2 == name)
                .find(|s| common::dist(&s.0, &end) < 1e-12 || common::dist(&s.1, &end) < 1e-12)
                .unwrap();
            let xe = common::element_coords(&background, el.ty(), el.idx());
            let lam = common::barycentric(&xe, &end);
            let conn = background
                .connectivity(el.ty(), GhostType::NotGhost)
                .unwrap()
                .get_tuple(el.idx())
                .unwrap()
                .to_vec();
            for (k, &node) in conn.iter().enumerate() {
                for i in 0..DIM {
                    f_ref[usize::try_from(node).unwrap() * DIM + i] -=
                        a * sp * t[i] * sign * lam[k];
                }
            }
        }
    }
    let scale = laws.iter().fold(0.0_f64, |m, l| m.max((l.2 * l.3).abs()));
    for (k, (x, y)) in f.iter().zip(&f_ref).enumerate() {
        assert!((x - y).abs() <= 1e-12 * scale, "f_int[{k}] = {x} vs {y}");
    }
    // self-equilibrated: zero resultant force and moment
    let (fx, fy): (Real, Real) = (f.iter().step_by(2).sum(), f.iter().skip(1).step_by(2).sum());
    let mz: Real = nodes
        .iter()
        .enumerate()
        .map(|(n, x)| x[0] * f[n * DIM + 1] - x[1] * f[n * DIM])
        .sum();
    assert!(fx.abs() <= 1e-12 * scale && fy.abs() <= 1e-12 * scale && mz.abs() <= 1e-12 * scale);
}

#[test]
fn prestress_layers_free_block_uniform_state_2d() {
    let (nx, ny, lx, ly) = (4usize, 4usize, 2.0, 1.0);
    let (e_c, nu, e_s, alpha, sp) = (1e3, 0.3, 2e4, 0.02, 50.0);
    let mesh = common::grid_triangles(nx, ny, lx, ly, "concrete");
    let h = ly / ny as Real;
    let names: Vec<String> = (0..=ny).map(|j| format!("layer_{j}")).collect();
    let bars: Vec<(&str, Vec<[Real; 2]>)> = (0..=ny)
        .map(|j| {
            let y = h * j as Real;
            (names[j].as_str(), vec![[0.0, y], [lx, y]])
        })
        .collect();
    let laws: Vec<(&str, Real, Real, Real)> = (0..=ny)
        .map(|j| {
            let trib = if j == 0 || j == ny { 0.5 * h } else { h };
            (names[j].as_str(), e_s, alpha * trib, sp)
        })
        .collect();
    let mut model = init(mesh, &bars, &dat(e_c, nu, &laws));
    let nodes = common::nodes(&model);
    // x blocked on x = 0, y blocked at the origin (free body otherwise)
    {
        let mut b = model.smm.blocked_dofs_mut().unwrap();
        b.as_mut_slice().fill(false);
        for (n, x) in nodes.iter().enumerate() {
            if x[0] == 0.0 {
                b.as_mut_slice()[n * DIM] = true;
            }
            if x[0] == 0.0 && x[1] == 0.0 {
                b.as_mut_slice()[n * DIM + 1] = true;
            }
        }
    }
    model.solve_step(None).unwrap();
    let eps = -alpha * sp / (e_c + alpha * e_s);
    let u = model.smm.displacement().unwrap().as_slice().to_vec();
    let mut max_err: Real = 0.0;
    for (n, x) in nodes.iter().enumerate() {
        max_err = max_err
            .max((u[n * DIM] - eps * x[0]).abs() / (eps.abs() * lx))
            .max((u[n * DIM + 1] + nu * eps * x[1]).abs() / (eps.abs() * lx));
        assert!(
            (u[n * DIM] - eps * x[0]).abs() <= 1e-10 * eps.abs() * lx,
            "u_x at {x:?}"
        );
        assert!(
            (u[n * DIM + 1] + nu * eps * x[1]).abs() <= 1e-10 * eps.abs() * lx,
            "u_y at {x:?}"
        );
    }
    for (name, ..) in &laws {
        for s in law(&model, name)
            .base()
            .stress
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .as_slice()
        {
            let s_ref = e_s * eps + sp;
            assert!(
                (s - s_ref).abs() <= 1e-10 * sp,
                "{name}: σ_s {s} vs {s_ref}"
            );
        }
    }
    let concrete = model.smm.material_by_name("concrete").unwrap();
    for s in concrete
        .base()
        .stress
        .get(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .chunks(4)
    {
        // column-major 2 × 2: (xx, yx, xy, yy)
        assert!((s[0] - e_c * eps).abs() <= 1e-10 * alpha * sp, "σ_xx {s:?}");
        assert!(s[1].abs() <= 1e-10 * alpha * sp && s[2].abs() <= 1e-10 * alpha * sp);
        assert!(s[3].abs() <= 1e-10 * alpha * sp, "σ_yy {s:?}");
    }
    eprintln!("layered pre-stress: max |u − u_exact|/(|ε| L) = {max_err:.2e}");
    // the free body needs no reaction
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    for (n, x) in nodes.iter().enumerate() {
        if x[0] == 0.0 {
            assert!(
                f[n * DIM].abs() <= 1e-9 * alpha * sp * h,
                "reaction at {x:?}: {}",
                f[n * DIM]
            );
        }
    }
}
