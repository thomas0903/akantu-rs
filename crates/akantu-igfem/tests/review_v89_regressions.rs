// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests (no upstream counterpart), written independently
// of the port, for the IGFEM port of Akantu's (EPFL/LSMS)
// extra_packages/igfem: MaterialIGFEM::setSubMaterial (UD-611),
// ShapeLagrange<_ek_igfem>::contains (UD-606),
// SolidMechanicsModelIGFEM patch tests and computeValuesOnEnrichedNodes,
// and the off-centre inclusion convergence (feature `cgal`).

//! Everything but the
//! convergence study runs without the GPL feature `cgal`: the interfaces
//! are cut by the straight-line generator of `common::cut`.

mod common;

use std::sync::Arc;

use akantu_core::{Element, ElementKind, ElementType, GhostType, Real};
use akantu_igfem::enrichment::InterfaceGeometry;
use akantu_igfem::{IgfemFeEngine, SolidMechanicsModelIgfem};
use akantu_mesh::SharedMesh;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use common::cut::{Line, cut, structured};
use common::model::{counts, set_newton};

/// The convergence study of the example `igfem_off_centre_inclusion`
/// (feature `cgal`), shared so that the test and the example cannot drift.
#[cfg(feature = "cgal")]
#[path = "../examples/igfem_off_centre_inclusion.rs"]
mod study;

/// Upstream `test_solid_mechanics_model_igfem` material file: law 0 =
/// aggregate (`E = 10`, `ν = 0.3`), law 1 = gel (`E = 1`, `ν = 0.25`),
/// law 2 = `igfem_elastic` (sub-material 1 = gel, 2 = aggregate).
const DAT: &str = include_str!("data/upstream/smm_igfem/material.dat");

/// The same with two `igfem_elastic` laws (2 and 3).
const DAT_TWO_IGFEM_LAWS: &str = "seed = 1
material elastic [
 name = aggregate
 E = 10.
 nu = 0.3
]
material elastic [
 name = gel
 E = 1.
 nu = 0.25
]
material igfem_elastic [
 name = igfem_a
 name_sub_mat_1 = gel
 name_sub_mat_2 = aggregate
]
material igfem_elastic [
 name = igfem_b
 name_sub_mat_1 = gel
 name_sub_mat_2 = aggregate
]
";

/// Exactness bound of the patch tests (rounding only: the exact field lies
/// in the discrete space; measured ≤ 2.3e-16), the crate's
/// absolute patch-test rule (`docs/NUMERICAL_VALIDATION.md`).
const EXACT: Real = 1e-12;
/// Upstream's absolute stop of the parent inverse map (`‖x − x(ξ)‖`,
/// started at `ξ = 0` = parent node 0): `computeValuesOnEnrichedNodes` is
/// accurate to `|∇u| · 1e-10` when an enriched node lies within `1e-10` of
/// parent node 0 (UD semantics kept).
const INVERSE_MAP_STOP: Real = 1e-10;

/// Half plane `φ ≤ 0` = sub-material 1 ("gel") of the IGFEM law.
#[derive(Debug)]
struct HalfPlane(Line);

impl InterfaceGeometry for HalfPlane {
    fn is_inside(&self, p: &[Real], _domain: &str) -> akantu_core::Result<bool> {
        Ok(self.0.phi(p) <= 0.0)
    }
}

/// IGFEM elements → law 2 (or, with `alternate`, laws 2 and 3 in turn, so
/// that the IGFEM laws' element filters are not the identity: UD-611);
/// regular elements by the side of their barycenter.
#[derive(Debug)]
struct SideSelector(Line, bool);

impl ConstitutiveLawSelector for SideSelector {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> akantu_core::Result<Option<usize>> {
        if el.ty().kind() == ElementKind::Igfem {
            let alternate = usize::try_from(el.index()).unwrap() % 2;
            return Ok(Some(if self.1 { 2 + alternate } else { 2 }));
        }
        let b = ctx.mesh.barycenter(el)?;
        Ok(Some(if self.0.phi(&b) <= 0.0 { 1 } else { 0 }))
    }
}

fn lame(model: &SolidMechanicsModelIgfem, i: usize) -> (Real, Real) {
    let get = |p: &str| match model.smm.material(i).unwrap().get_param(p).unwrap() {
        akantu_parser::ParamValue::Real(v) => v,
        v => panic!("{p}: {v:?}"),
    };
    (get("lambda"), get("mu"))
}

/// Exact bi-material field: `u = H x + t` on the gel side (`φ ≤ 0`),
/// `u = H x + t − α d(x)` on the aggregate side, `d` the signed distance;
/// `α` from traction continuity
/// `(μ₂ I + (λ₂ + μ₂) n n) α = (σ₂(H) − σ₁(H)) n`.
struct Exact {
    line: Line,
    h: [[Real; 2]; 2],
    t: [Real; 2],
    alpha: [Real; 2],
    norm: Real,
}

impl Exact {
    fn new(line: Line, l1: (Real, Real), l2: (Real, Real)) -> Self {
        let h = [[0.3, -0.7], [0.45, 0.2]];
        let t = [0.1, -0.05];
        let norm = (line.a * line.a + line.b * line.b).sqrt();
        let n = [line.a / norm, line.b / norm];
        let sigma = |l: (Real, Real)| {
            let tr = h[0][0] + h[1][1];
            let mut s = [[0.0; 2]; 2];
            for i in 0..2 {
                for j in 0..2 {
                    s[i][j] = if i == j { l.0 * tr } else { 0.0 } + l.1 * (h[i][j] + h[j][i]);
                }
            }
            s
        };
        let (s1, s2) = (sigma(l1), sigma(l2));
        let rhs = [
            (s2[0][0] - s1[0][0]) * n[0] + (s2[0][1] - s1[0][1]) * n[1],
            (s2[1][0] - s1[1][0]) * n[0] + (s2[1][1] - s1[1][1]) * n[1],
        ];
        let (lam2, mu2) = l2;
        let m = [
            [mu2 + (lam2 + mu2) * n[0] * n[0], (lam2 + mu2) * n[0] * n[1]],
            [(lam2 + mu2) * n[1] * n[0], mu2 + (lam2 + mu2) * n[1] * n[1]],
        ];
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let alpha = [
            (m[1][1] * rhs[0] - m[0][1] * rhs[1]) / det,
            (-m[1][0] * rhs[0] + m[0][0] * rhs[1]) / det,
        ];
        Self {
            line,
            h,
            t,
            alpha,
            norm,
        }
    }

    fn u(&self, x: &[Real]) -> [Real; 2] {
        let d = self.line.phi(x) / self.norm;
        let mut v = [
            self.h[0][0] * x[0] + self.h[0][1] * x[1] + self.t[0],
            self.h[1][0] * x[0] + self.h[1][1] * x[1] + self.t[1],
        ];
        if d > 0.0 {
            v[0] -= self.alpha[0] * d;
            v[1] -= self.alpha[1] * d;
        }
        v
    }

    /// Upper bound of `|∇u|` on both sides (`‖H‖_F + |α|`, `|∇d| = 1`).
    fn gradient_bound(&self) -> Real {
        let h = self.h;
        (h[0][0] * h[0][0] + h[0][1] * h[0][1] + h[1][0] * h[1][0] + h[1][1] * h[1][1]).sqrt()
            + (self.alpha[0] * self.alpha[0] + self.alpha[1] * self.alpha[1]).sqrt()
    }
}

/// Edge `(p, q)` (local indices) of every enriched node `k` of an IGFEM
/// element (tri4: node 3 on edge (1, 2); tri5: node 3 on (0, 1), node 4 on
/// (2, 0)).
fn enriched_edges(ty: ElementType) -> &'static [(usize, usize, usize)] {
    match ty {
        ElementType::IgfemTriangle4 => &[(3, 1, 2)],
        ElementType::IgfemTriangle5 => &[(3, 0, 1), (4, 2, 0)],
        _ => &[],
    }
}

#[derive(Debug, Default)]
struct PatchResult {
    counts: [usize; 3],
    std_err: Real,
    enr_err: Real,
    min_sub_jacobian: Real,
    solve: String,
    real_err: Real,
    real_status: String,
    gradient_bound: Real,
}

/// Dirichlet patch test on the cut `n × n` structured mesh of `[−1, 1]²`:
/// every boundary DOF (standard and enriched) gets the exact value, the
/// interior is solved; returns the max nodal errors.
fn patch_with(line: Line, n: usize, on_tol: Real, dat: &str, alternate: bool) -> PatchResult {
    let (nodes, tris) = structured(n, -1.0, 1.0, -1.0, 1.0);
    let cm = cut(&nodes, &tris, &line, on_tol);
    let nstd = cm.nb_standard_nodes;
    let mesh = SharedMesh::new(cm.mesh);
    let parser = akantu_parser::Parser::parse_str(dat, "material.dat").unwrap();
    let mut res = PatchResult::default();
    let mut model = SolidMechanicsModelIgfem::new(mesh, Some(2), "v89", parser).unwrap();
    model.set_material_selector(Box::new(SideSelector(line, alternate)));
    model
        .ctx
        .set_geometry(Some(Arc::new(HalfPlane(line))))
        .unwrap();
    model
        .init_full(&akantu_model::ModelOptions::new(
            akantu_core::enums::AnalysisMethod::Static,
        ))
        .unwrap();
    let ex = Exact::new(line, lame(&model, 1), lame(&model, 0));
    res.gradient_bound = ex.gradient_bound();
    let mesh_h = model.mesh().clone();
    let (pos, exact_nodal, on_boundary) = {
        let m = mesh_h.read().unwrap();
        res.counts = counts(&m);
        let pos = m.nodes().clone();
        let nn = pos.len();
        let mut exact_nodal = vec![[0.0; 2]; nn];
        let mut on_boundary = vec![false; nn];
        for i in 0..nn {
            let x = pos.tuple(i);
            on_boundary[i] = (0..2).any(|d| (x[d].abs() - 1.0).abs() < 1e-12);
            if i < nstd {
                exact_nodal[i] = ex.u(x);
            }
        }
        // exact enriched values u_e = u(x_e) − [(1 − t) u(p) + t u(q)]
        for ty in [ElementType::IgfemTriangle4, ElementType::IgfemTriangle5] {
            for e in 0..m.nb_element(ty, GhostType::NotGhost) {
                let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
                let c: Vec<usize> = m
                    .element_connectivity(el)
                    .unwrap()
                    .iter()
                    .map(|&v| usize::try_from(v).unwrap())
                    .collect();
                for &(k, p, q) in enriched_edges(ty) {
                    let (xp, xq, xe) = (pos.tuple(c[p]), pos.tuple(c[q]), pos.tuple(c[k]));
                    let len2 = (xq[0] - xp[0]).powi(2) + (xq[1] - xp[1]).powi(2);
                    let t = ((xe[0] - xp[0]) * (xq[0] - xp[0]) + (xe[1] - xp[1]) * (xq[1] - xp[1]))
                        / len2;
                    let (up, uq, ue) = (ex.u(xp), ex.u(xq), ex.u(xe));
                    exact_nodal[c[k]] = [
                        ue[0] - ((1.0 - t) * up[0] + t * uq[0]),
                        ue[1] - ((1.0 - t) * up[1] + t * uq[1]),
                    ];
                }
            }
        }
        res.min_sub_jacobian = Real::INFINITY;
        let eng = model.ctx.engine().unwrap();
        for ty in [ElementType::IgfemTriangle4, ElementType::IgfemTriangle5] {
            if m.nb_element(ty, GhostType::NotGhost) > 0 {
                for j in eng
                    .jacobians(&m, ty, GhostType::NotGhost)
                    .unwrap()
                    .as_slice()
                {
                    res.min_sub_jacobian = res.min_sub_jacobian.min(*j);
                }
            }
        }
        (pos, exact_nodal, on_boundary)
    };
    {
        let mut disp = model.smm.displacement().unwrap().clone();
        let mut boun = model.smm.blocked_dofs().unwrap().clone();
        for i in 0..pos.len() {
            if on_boundary[i] {
                boun.tuple_mut(i).copy_from_slice(&[true, true]);
                disp.tuple_mut(i).copy_from_slice(&exact_nodal[i]);
            }
        }
        model
            .smm
            .displacement_mut()
            .unwrap()
            .copy_from(&disp)
            .unwrap();
        model
            .smm
            .blocked_dofs_mut()
            .unwrap()
            .copy_from(&boun)
            .unwrap();
    }
    set_newton(&mut model, 4, 1e-10);
    match model.smm.solve_step(None) {
        Ok(()) => res.solve = "ok".into(),
        Err(e) => res.solve = format!("solve error: {e}"),
    }
    // physical values at the enriched nodes (upstream computeValuesOnEnrichedNodes)
    match model.compute_values_on_enriched_nodes() {
        Ok(()) => {
            res.real_status = "ok".into();
            let real = model.real_displacement().unwrap();
            for i in nstd..pos.len() {
                let ue = ex.u(pos.tuple(i));
                let d = real.tuple(i);
                let err = ((d[0] - ue[0]).powi(2) + (d[1] - ue[1]).powi(2)).sqrt();
                res.real_err = res
                    .real_err
                    .max(if err.is_nan() { Real::INFINITY } else { err });
            }
        }
        Err(e) => res.real_status = format!("error: {e}"),
    }
    let u = model.smm.displacement().unwrap();
    for (i, ex_i) in exact_nodal.iter().enumerate() {
        let d = u.tuple(i);
        let err = ((d[0] - ex_i[0]).powi(2) + (d[1] - ex_i[1]).powi(2)).sqrt();
        let err = if err.is_nan() { Real::INFINITY } else { err };
        if i < nstd {
            res.std_err = res.std_err.max(err);
        } else {
            res.enr_err = res.enr_err.max(err);
        }
    }
    res
}

/// UD-611 regression: two IGFEM
/// laws share the `_igfem_triangle_5` elements alternately, so their element
/// filters are not the identity. The sub-material assignment must read each
/// element's own connectivity; upstream's filter-position connectivity
/// assigns the wrong sub-materials and breaks the patch test.
#[test]
fn ud611_two_igfem_laws_non_identity_filter() {
    for line in [
        Line {
            a: 1.0,
            b: 0.3,
            c: 0.137,
        },
        Line {
            a: -0.6,
            b: 1.0,
            c: -0.0731,
        },
    ] {
        let r = patch_with(line, 8, 0.0, DAT_TWO_IGFEM_LAWS, true);
        assert!(
            r.counts[2] >= 2,
            "{line:?}: both IGFEM laws need elements: {r:?}"
        );
        assert_eq!(r.solve, "ok", "{line:?}");
        assert!(r.std_err <= EXACT && r.enr_err <= EXACT, "{line:?}: {r:?}");
    }
}

/// Patch tests with straight interfaces in general position, through mesh
/// nodes (tri4 + tri5) and shifted by `δ·h` from the nodes (tiny triangle
/// sub-elements next to a vertex), and almost parallel to a mesh row (thin
/// quadrangle sub-elements), `δ = 1e-2 … 1e-14`. Every DOF is exact; the
/// physical values at the enriched nodes are exact unless an enriched node
/// lies within `1e-10` of its parent node 0, where upstream's inverse-map
/// stop limits them to `|∇u| · 1e-10`.
#[test]
fn patch_tests_arbitrary_and_degenerate_cuts() {
    let n = 8; // h = 0.25
    let h = 0.25;
    // (name, line, on-node tolerance of the cutter, δ or None)
    let mut cases: Vec<(String, Line, Real, Option<Real>)> = vec![
        (
            "general a=1 b=0.3 c=0.137".into(),
            Line {
                a: 1.0,
                b: 0.3,
                c: 0.137,
            },
            0.0,
            None,
        ),
        (
            "general a=-0.6 b=1 c=-0.0731".into(),
            Line {
                a: -0.6,
                b: 1.0,
                c: -0.0731,
            },
            0.0,
            None,
        ),
        (
            "through nodes y=2x".into(),
            Line {
                a: 2.0,
                b: -1.0,
                c: 0.0,
            },
            1e-14,
            None,
        ),
    ];
    let s5 = (5.0 as Real).sqrt();
    for k in [2, 4, 6, 8, 10, 12, 13, 14] {
        let delta = (10.0 as Real).powi(-k);
        // shift normal to y = 2x by δ·h: c = δ·h·|(2, −1)|
        cases.push((
            format!("y=2x shifted by {delta:e} h"),
            Line {
                a: 2.0,
                b: -1.0,
                c: delta * h * s5,
            },
            0.0,
            Some(delta),
        ));
    }
    for k in [2, 4, 6, 8, 10, 12, 14] {
        let delta = (10.0 as Real).powi(-k);
        // almost along the row y = 0.25: y = 0.25 + δh + 1e-3·δh·x
        cases.push((
            format!("near the row y=0.25, {delta:e} h"),
            Line {
                a: -1e-3 * delta * h,
                b: 1.0,
                c: 0.25 + delta * h,
            },
            0.0,
            Some(delta),
        ));
    }
    let mut failures = Vec::new();
    for (name, line, tol, delta) in cases {
        let r = patch_with(line, n, tol, DAT, false);
        eprintln!(
            "{name}: counts {:?}, min det J·w {:.3e}, solve {}, max error standard {:.3e} / \
             enriched DOF {:.3e}, real_displacement {} {:.3e}",
            r.counts, r.min_sub_jacobian, r.solve, r.std_err, r.enr_err, r.real_status, r.real_err
        );
        let real_bound = match delta {
            // δ·h > 1e-10: the inverse map iterates (affine: exact)
            None => EXACT,
            Some(d) if d * h > INVERSE_MAP_STOP => EXACT,
            Some(_) => r.gradient_bound * INVERSE_MAP_STOP + EXACT,
        };
        let ok = r.solve == "ok"
            && r.counts[1] + r.counts[2] > 0
            && r.min_sub_jacobian > 0.0
            && r.std_err <= EXACT
            && r.enr_err <= EXACT
            && r.real_status == "ok"
            && r.real_err <= real_bound;
        if !ok {
            failures.push(format!(
                "{name}: {r:?} (real_displacement bound {real_bound:e})"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "patch tests failed:\n{}",
        failures.join("\n")
    );
}

/// UD-606: `contains` tests the parent natural coordinates with the
/// parent's shape (triangle, `Math::tolerance`). Upstream's
/// `ShapeLagrange<_ek_igfem>::contains` does not compile at the reference
/// commit (the IGFEM `GeometricalElement` specialisations have no
/// `contains`); the pre-2020 API it was written against would have used the
/// default `_gst_point` test `ξ₀ < DBL_EPSILON`. This pins the port's
/// behaviour where that reading differs: it rejects the parent's centroid
/// and vertex 1 and accepts points with `ξ₀ < 0` outside the element.
#[test]
fn ud606_contains_tests_the_parent_shape() {
    let (nodes, tris) = structured(4, -1.0, 1.0, -1.0, 1.0);
    let line = Line {
        a: 1.0,
        b: 0.3,
        c: 0.137,
    };
    let cm = cut(&nodes, &tris, &line, 0.0);
    let mesh = &cm.mesh;
    let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
    let ty = ElementType::IgfemTriangle5;
    let nb = mesh.nb_element(ty, GhostType::NotGhost);
    assert!(nb > 0);
    for e in 0..nb {
        let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
        let x = mesh
            .extract_nodal_values_from_element(&mesh.nodes(), el)
            .unwrap();
        let v = |k: usize| [x[2 * k], x[2 * k + 1]];
        let (p0, p1, p2) = (v(0), v(1), v(2));
        let at = |a: Real, b: Real| {
            // x = p0 + a (p1 − p0) + b (p2 − p0), i.e. ξ = (a, b)
            [
                p0[0] + a * (p1[0] - p0[0]) + b * (p2[0] - p0[0]),
                p0[1] + a * (p1[1] - p0[1]) + b * (p2[1] - p0[1]),
            ]
        };
        // (point, expected `contains`, pre-2020 `_gst_point` reading)
        let cases = [
            (at(1. / 3., 1. / 3.), true, false), // centroid
            (at(1.0, 0.0), true, false),         // vertex 1
            (at(0.0, 0.0), true, true),          // vertex 0
            (at(0.25, 0.5), true, false),        // interior
            (at(-0.5, 0.25), false, true),       // outside, ξ₀ < 0
            (at(0.75, 0.75), false, false),      // outside, beyond edge (1, 2)
        ];
        for (k, (p, expected, gst_point)) in cases.iter().enumerate() {
            assert_eq!(
                fe.contains(mesh, p, el).unwrap(),
                *expected,
                "element {e}, case {k}"
            );
            let xi = fe.inverse_map(mesh, p, el).unwrap();
            assert_eq!(
                xi[0] < Real::EPSILON,
                *gst_point,
                "element {e}, case {k}: ξ = {xi:?}"
            );
        }
    }
}

/// Off-centre inclusion convergence (example
/// `igfem_off_centre_inclusion`), n = 8 … 64: the L2 errors decrease
/// monotonically, the rates increase towards 2, the finest rate is ≥ 1.8
/// (the regression criterion) and the errors match the recorded
/// x86_64 release values (printed to 4 significant digits).
#[cfg(feature = "cgal")]
#[test]
fn off_centre_inclusion_convergence() {
    let levels = study::convergence(64).unwrap();
    // L2 errors for n = 8, 16, 32, 64.
    let v89 = [2.521e-2, 9.188e-3, 2.731e-3, 7.607e-4];
    assert_eq!(levels.len(), v89.len());
    for (l, r) in levels.iter().zip(v89) {
        eprintln!("n = {}: L2 = {:.6e}, counts {:?}", l.n, l.l2, l.counts);
        // 4 significant digits: rounding ≤ 0.5e-3 relative
        assert!(
            (l.l2 / r - 1.0).abs() <= 5e-4,
            "n = {}: {} vs the review's {r}",
            l.n,
            l.l2
        );
    }
    let rates: Vec<Real> = levels
        .windows(2)
        .map(|w| study::rate(&w[0], &w[1]))
        .collect();
    eprintln!("rates {rates:?}");
    for w in levels.windows(2) {
        assert!(w[1].l2 < w[0].l2);
    }
    for w in rates.windows(2) {
        assert!(w[1] > w[0], "rates {rates:?}");
    }
    assert!(*rates.last().unwrap() >= 1.8, "rates {rates:?}");
}
