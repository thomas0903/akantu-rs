// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): independent checks (D-007) of
// src/model/solid_mechanics/solid_mechanics_model_embedded_interface/* and
// materials/material_embedded/* (no oracle: upstream does not build this case).

//! Independent mathematical checks of the embedded model in 3-D on a
//! conforming tetrahedral background (`_tetrahedron_4`, faces as
//! primitives, UD-407 default): interface topology, affine patch test, the
//! closed-form reinforcement stiffness and the six rigid-body modes, a bar
//! in a matrix under uniaxial extension, and the nodal forces of
//! pre-stressed bars at rest (tolerances: `docs/NUMERICAL_VALIDATION.md`).

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_embedded::{
    EmbeddedInterfaceModel, EmbeddedInterfaceModelOptions, MaterialReinforcement,
};
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::LawPhase;
use akantu_solid::Material;

const DIM: usize = 3;

fn bars() -> Vec<(&'static str, Vec<[Real; 3]>)> {
    vec![
        (
            "bar_a",
            vec![[0.11, 0.17, 0.23], [0.5, 0.5, 0.5], [0.89, 0.83, 0.77]],
        ),
        ("bar_b", vec![[0.93, 0.12, 0.31], [0.07, 0.71, 0.88]]),
        // along a mesh line (edges shared by several tetrahedra)
        ("bar_c", vec![[0.0, 0.5, 0.5], [1.0, 0.5, 0.5]]),
        ("bar_d", vec![[0.37, 0.05, 0.62], [0.41, 0.97, 0.18]]),
    ]
}

fn laws() -> Vec<(&'static str, Real, Real, Real)> {
    vec![
        ("bar_a", 2.0e4, 0.01, 3.5),
        ("bar_b", 1.5e4, 0.02, -1.25),
        ("bar_c", 3.0e4, 0.005, 0.0),
        ("bar_d", 2.5e4, 0.015, 7.0),
    ]
}

fn primitive_of(bars: &[(&str, Vec<[Real; 3]>)]) -> Mesh {
    let mut nodes: Vec<Vec<Real>> = Vec::new();
    let mut segs = Vec::new();
    let mut names = Vec::new();
    for (name, pts) in bars {
        let first = nodes.len();
        nodes.extend(pts.iter().map(|p| p.to_vec()));
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

fn dat(e_c: Real, nu: Real, laws: &[(&str, Real, Real, Real)]) -> String {
    let mut s = format!("material elastic [\n name = concrete\n E = {e_c:e}\n nu = {nu}\n]\n");
    for (name, e, a, sp) in laws {
        s += &format!(
            "material reinforcement elastic [\n name = {name}\n E = {e:e}\n area = {a:e}\n pre_stress = {sp:e}\n]\n"
        );
    }
    s
}

fn init(mesh: Mesh, bars: &[(&str, Vec<[Real; 3]>)], dat: &str) -> EmbeddedInterfaceModel {
    let mut model = common::model(mesh, primitive_of(bars), dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    model
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

fn unit(p: &[Real; 3], q: &[Real; 3]) -> [Real; 3] {
    let l = common::dist(p, q);
    [(q[0] - p[0]) / l, (q[1] - p[1]) / l, (q[2] - p[2]) / l]
}

fn dot(a: &[Real], b: &[Real]) -> Real {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
fn interface_topology_invariants_3d() {
    let mesh = common::grid_tetrahedra(3, 1.0, "concrete");
    let bars = bars();
    let model = init(mesh, &bars, &dat(1e3, 0.25, &laws()));
    let segs = common::interface_segments(&model);
    assert_eq!(model.interface_mesh().nb_nodes(), 2 * segs.len());
    let background = model.smm.physics.base.mesh.read().unwrap();
    for (name, pts) in &bars {
        let (p, q) = (pts[0], pts[pts.len() - 1]);
        let l = common::dist(&p, &q);
        let t = unit(&p, &q);
        let mut pieces = Vec::new();
        for (x0, x1, n, el) in &segs {
            if n != name {
                continue;
            }
            assert_eq!(el.ty(), ElementType::Tetrahedron4);
            let xe = common::element_coords(&background, el.ty(), el.idx());
            for x in [x0, x1] {
                for lam in common::barycentric(&xe, x) {
                    assert!((-1e-12..=1. + 1e-12).contains(&lam), "{name}: outside {el}");
                }
                let d: Vec<Real> = (0..3).map(|i| x[i] - p[i]).collect();
                let s = dot(&d, &t);
                let off: Vec<Real> = (0..3).map(|i| d[i] - s * t[i]).collect();
                assert!(
                    dot(&off, &off).sqrt() <= 1e-14 * l.max(1.0),
                    "{name}: off the bar"
                );
            }
            let s0 = (0..3).map(|i| (x0[i] - p[i]) * t[i]).sum::<Real>();
            let s1 = (0..3).map(|i| (x1[i] - p[i]) * t[i]).sum::<Real>();
            pieces.push((s0.min(s1), s0.max(s1)));
        }
        assert!(!pieces.is_empty());
        pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(pieces[0].0.abs() <= 1e-12 * l, "{name}: start");
        assert!(
            (pieces[pieces.len() - 1].1 - l).abs() <= 1e-12 * l,
            "{name}: end"
        );
        for w in pieces.windows(2) {
            assert!(
                (w[0].1 - w[1].0).abs() <= 1e-12 * l,
                "{name}: gap/overlap {w:?}"
            );
        }
        for (a, b) in &pieces {
            assert!(b - a > 1e-12 * l, "{name}: degenerate piece");
        }
        let total: Real = pieces.iter().map(|(a, b)| b - a).sum();
        assert!((total - l).abs() <= 1e-12 * l);
    }
}

#[test]
fn affine_patch_test_3d() {
    let mesh = common::grid_tetrahedra(3, 1.0, "concrete");
    let laws = laws();
    let mut model = init(mesh, &bars(), &dat(1e3, 0.25, &laws));
    let h = [
        [1.3e-3, -0.7e-3, 0.2e-3],
        [0.4e-3, -2.1e-3, 0.9e-3],
        [-0.6e-3, 0.3e-3, 1.1e-3],
    ];
    let hmax = 2.1e-3;
    let nodes = common::nodes(&model);
    {
        let mut u = model.smm.displacement_mut().unwrap();
        for (n, x) in nodes.iter().enumerate() {
            for (i, hi) in h.iter().enumerate() {
                u.as_mut_slice()[n * DIM + i] = dot(hi, x) + 0.1 * (i as Real + 1.);
            }
        }
    }
    model.assemble_internal_forces().unwrap();
    for (name, e, a, sp) in &laws {
        let pts = &bars().into_iter().find(|b| b.0 == *name).unwrap().1;
        let (p, q) = (pts[0], pts[pts.len() - 1]);
        let t = unit(&p, &q);
        let l = common::dist(&p, &q);
        let mut eps = 0.;
        for i in 0..3 {
            for j in 0..3 {
                eps += t[i] * 0.5 * (h[i][j] + h[j][i]) * t[j];
            }
        }
        let law = law(&model, name);
        let (ty, g) = (ElementType::Segment2, GhostType::NotGhost);
        let gu = law.base().grad_u.get(ty, g).unwrap();
        let sigma = law.base().stress.get(ty, g).unwrap();
        let ge = law.gradu_embedded.get(ty, g).unwrap();
        let dc = law.directing_cosines.get(ty, g).unwrap();
        assert!(!gu.is_empty());
        for k in 0..gu.len() {
            assert!((gu.as_slice()[k] - eps).abs() <= 1e-12 * hmax, "{name}: ε");
            let s_ref = e * eps + sp;
            assert!(
                (sigma.as_slice()[k] - s_ref).abs() <= 1e-12 * (e * hmax + sp.abs()),
                "{name}: σ"
            );
            let du = ge.get_tuple(k).unwrap();
            for i in 0..3 {
                for j in 0..3 {
                    assert!(
                        (du[i + 3 * j] - h[i][j]).abs() <= 1e-12 * hmax,
                        "{name}: ∇u"
                    );
                }
            }
            let c = dc.get_tuple(k).unwrap();
            let c_ref = [
                t[0] * t[0],
                t[1] * t[1],
                t[2] * t[2],
                t[1] * t[2],
                t[0] * t[2],
                t[0] * t[1],
            ];
            for (x, y) in c.iter().zip(c_ref) {
                assert!((x - y).abs() <= 1e-14, "{name}: cosines");
            }
        }
        drop(gu);
        drop(sigma);
        drop(ge);
        drop(dc);
        let idx = model.smm.material_index(name).unwrap();
        let w = model
            .smm
            .with_material(idx, LawPhase::Energy, |m, ctx| {
                m.energy_of(ctx, "potential")
            })
            .unwrap();
        let w_ref = 0.5 * a * (e * eps + sp) * eps * l;
        assert!(
            (w - w_ref).abs() <= 1e-12 * (0.5 * a * (e * hmax + sp.abs()) * hmax * l),
            "{name}: energy {w} vs {w_ref}"
        );
    }
}

#[test]
fn reinforcement_stiffness_closed_form_and_rigid_modes_3d() {
    let mesh = common::grid_tetrahedra(2, 1.0, "concrete");
    let laws = laws();
    let mut model = init(mesh, &bars(), &dat(0.0, 0.25, &laws));
    model.assemble_stiffness_matrix(true).unwrap();
    let nodes = common::nodes(&model);
    let ndof = nodes.len() * DIM;
    let k = {
        let kk = model.smm.dof_manager().matrix("K").unwrap();
        (0..ndof)
            .map(|i| {
                (0..ndof)
                    .map(|j| kk.get(i64::try_from(i).unwrap(), i64::try_from(j).unwrap()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let mut k_ref = vec![vec![0.0; ndof]; ndof];
    let background = model.smm.physics.base.mesh.read().unwrap();
    for (x0, x1, name, el) in common::interface_segments(&model) {
        let (_, e, a, _) = laws.iter().find(|l| l.0 == name).unwrap();
        let ls = common::dist(&x0, &x1);
        let t: Vec<Real> = (0..3).map(|i| (x1[i] - x0[i]) / ls).collect();
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
            let dn = dot(&t, &grad[a_loc]);
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
    for r in 0..ndof {
        for c in 0..ndof {
            assert!(
                (k[r][c] - k_ref[r][c]).abs() <= 1e-12 * kmax,
                "K({r},{c}) = {} vs {}",
                k[r][c],
                k_ref[r][c]
            );
        }
    }
    let mut modes: Vec<Vec<Real>> = Vec::new();
    for i in 0..3 {
        modes.push(
            nodes
                .iter()
                .flat_map(|_| (0..3).map(move |k| if k == i { 1.0 } else { 0.0 }))
                .collect(),
        );
    }
    modes.push(nodes.iter().flat_map(|x| [-x[1], x[0], 0.0]).collect());
    modes.push(nodes.iter().flat_map(|x| [0.0, -x[2], x[1]]).collect());
    modes.push(nodes.iter().flat_map(|x| [x[2], 0.0, -x[0]]).collect());
    for (m, r) in modes.iter().enumerate() {
        for row in &k {
            let kr: Real = dot(row, r);
            assert!(kr.abs() <= 1e-12 * kmax * 6.0, "mode {m}: {kr}");
        }
    }
}

#[test]
fn bar_in_matrix_uniaxial_closed_form_3d() {
    let l = 1.0;
    let (e_c, nu) = (1e3, 0.2);
    let mesh = common::grid_tetrahedra(3, l, "concrete");
    let bars: Vec<(&str, Vec<[Real; 3]>)> = vec![
        ("bar_a", vec![[0.0, 0.37, 0.58], [l, 0.37, 0.58]]),
        (
            "bar_b",
            vec![[0.0, 0.81, 0.12], [0.4, 0.81, 0.12], [l, 0.81, 0.12]],
        ),
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
            if x[0] == l {
                b[n * DIM] = true;
                u[n * DIM] = delta;
            }
            if x[0] == 0.0 && x[1] == 0.0 && x[2] == 0.0 {
                b[n * DIM + 1] = true;
                b[n * DIM + 2] = true;
            }
            if x[0] == 0.0 && x[1] == l && x[2] == 0.0 {
                b[n * DIM + 2] = true;
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
    let eps = delta / l;
    let u = model.smm.displacement().unwrap().as_slice().to_vec();
    let mut max_err: Real = 0.0;
    for (n, x) in nodes.iter().enumerate() {
        max_err = max_err
            .max((u[n * DIM] - eps * x[0]).abs() / delta)
            .max((u[n * DIM + 1] + nu * eps * x[1]).abs() / delta)
            .max((u[n * DIM + 2] + nu * eps * x[2]).abs() / delta);
        assert!((u[n * DIM] - eps * x[0]).abs() <= 1e-10 * delta, "u_x");
        assert!(
            (u[n * DIM + 1] + nu * eps * x[1]).abs() <= 1e-10 * delta,
            "u_y"
        );
        assert!(
            (u[n * DIM + 2] + nu * eps * x[2]).abs() <= 1e-10 * delta,
            "u_z"
        );
    }
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    let reaction: Real = nodes
        .iter()
        .enumerate()
        .filter(|(_, x)| x[0] == l)
        .map(|(n, _)| f[n * DIM])
        .sum();
    let expected = (e_c * l * l + laws.iter().map(|w| w.1 * w.2).sum::<Real>()) * eps;
    assert!(
        (reaction.abs() - expected).abs() <= 1e-10 * expected,
        "reaction {reaction} vs {expected}"
    );
    eprintln!(
        "bar in matrix: max |u − u_exact|/δ = {max_err:.2e}, reaction rel. error = {:.2e}",
        (reaction.abs() - expected).abs() / expected
    );
}

#[test]
fn prestress_at_rest_nodal_forces_closed_form_3d() {
    let mesh = common::grid_tetrahedra(3, 1.0, "concrete");
    let laws = laws();
    let mut model = init(mesh, &bars(), &dat(1e3, 0.25, &laws));
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    let nodes = common::nodes(&model);
    let mut f_ref = vec![0.0; nodes.len() * DIM];
    let background = model.smm.physics.base.mesh.read().unwrap();
    let segs = common::interface_segments(&model);
    for (name, pts) in bars() {
        let (_, _, a, sp) = laws.iter().find(|w| w.0 == name).unwrap();
        let (p, q) = (pts[0], pts[pts.len() - 1]);
        let t = unit(&p, &q);
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
    let scale = laws.iter().fold(0.0_f64, |m, w| m.max((w.2 * w.3).abs()));
    for (k, (x, y)) in f.iter().zip(&f_ref).enumerate() {
        assert!((x - y).abs() <= 1e-12 * scale, "f_int[{k}] = {x} vs {y}");
    }
}
