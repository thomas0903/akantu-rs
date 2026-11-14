// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential tests of the
// structural element kernels and FE engine against fixtures/fe/structural/*
// (written by oracle/probes/structural/structural_probe.cc from upstream
// FEEngineTemplate<IntegratorGauss, ShapeStructural, _ek_structural> and the
// element-level parts of StructuralMechanicsModel).

//! `fe::structural_<case>_matches_oracle` (ELEM-021/022/023/027, FE-016,
//! FE-035, FE-036, element parts of STRUCT-003…012): rotations, `det J · w`,
//! `N·T`, `B·T`, interpolation, generalised strains, tangents, `BᵀDB`,
//! element stiffness, stresses, `Bᵀσ`, internal forces, traction rotation,
//! `Nᵀb`, line loads, `NᵀbN`, mass shapes and consistent mass, on the
//! upstream test meshes and on generated inclined / 3-D oriented beams and
//! distorted DKT triangles. Tolerances: `fixtures/fe/structural/tolerances.json`;
//! a bit-exactness summary is printed with `--nocapture`.
//!
//! The replays run the engine in **legacy mode**
//! ([`StructuralUpstreamCompat::UPSTREAM`]: upstream's `+dw/ds` θy row of
//! the 3-D beam, UD-293 / D-003 B), which the oracle reproduces bit for bit.
//! Each case also checks that the corrected default engine differs from the
//! legacy one only by that documented row: every other precomputed array
//! and every θy-independent quantity is bit-identical, and the θy row of
//! `N·T` is the exact negation (zero entries: ±0 compare equal). The
//! default engine also validates the beam normals (UD-343):
//! every fixture normal passes the default admissibility
//! criterion; the margins are asserted and printed.

mod common;

use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_fe::structural::kernels::{
    DktBendingStiffness, tangent_bernoulli_beam_2, tangent_bernoulli_beam_3,
    tangent_discrete_kirchhoff_triangle_18,
};
use akantu_fe::structural::{
    BEAM3_NORMAL_ADMISSIBILITY, EXTRA_NORMAL, StructuralElement, StructuralFeEngine,
    StructuralUpstreamCompat,
};
use akantu_fe::with_structural_element;
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::DiffTest;
use akantu_validation::reduce::NanMinMax;

use common::bit_stats;

const G: GhostType = GhostType::NotGhost;

struct Ctx {
    t: DiffTest,
    summary: Vec<String>,
}

impl Ctx {
    fn input(&self, q: &str) -> Vec<f64> {
        self.t.reference_f64(q).unwrap().data
    }

    fn input_array(&self, q: &str) -> Array<Real> {
        let f = self.t.reference_f64(q).unwrap();
        let nbc = f.shape.get(1).copied().unwrap_or(1);
        Array::from_vec(f.data, nbc).unwrap()
    }

    fn check(&mut self, q: &str, actual: &[f64]) {
        let shape = match self.t.reference_f64(q) {
            Ok(e) => {
                if e.data.len() == actual.len() {
                    let (n, m) = bit_stats(&e.data, actual);
                    self.summary.push(format!(
                        "{q}: {} values, {n} not bit-identical, max {m} ulp",
                        actual.len()
                    ));
                }
                e.shape
            }
            Err(_) => vec![actual.len()],
        };
        self.t.check_f64_shaped(q, &shape, actual);
    }

    fn finish(self, name: &str) {
        for l in &self.summary {
            eprintln!("{name}: {l}");
        }
        self.t.assert_pass();
    }
}

fn param_usize(t: &DiffTest, key: &str) -> usize {
    t.fixture().manifest().parameters[key].as_u64().unwrap() as usize
}

/// Mesh of the case: nodes, connectivity and (beams 3-D) `extra_normal`.
fn case_mesh(cx: &Ctx, ty: ElementType) -> Mesh {
    let dim = param_usize(&cx.t, "spatial_dimension");
    let nodes = cx.input("nodes");
    let conn = cx.t.reference_i64("connectivity").unwrap();
    let mut mesh = Mesh::new(dim, "structural").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for x in nodes.chunks(dim) {
            acc.push_node(x).unwrap();
        }
        let mut c = acc.connectivity_mut(ty, G);
        for row in conn.data.chunks(conn.shape[1]) {
            let r: Vec<Idx> = row.iter().map(|&v| v as Idx).collect();
            c.push_tuple(&r);
        }
    }
    if cx.t.reference_f64("extra_normal").is_ok() {
        let n = cx.input_array("extra_normal");
        mesh.data_mut()
            .reals_mut_or_insert(EXTRA_NORMAL)
            .unwrap()
            .as_map_mut()
            .replace(ty, G, n.into());
    }
    mesh
}

/// Generalised tangents per quadrature point from the per-element material
/// table (`E, A, I, Iz, Iy, GJ, rho, t, nu`), upstream formulas (DKT:
/// legacy `t³` bending block, as the oracle).
fn tangents(ty: ElementType, mat: &[f64], nq: usize) -> Array<Real> {
    let ns = with_structural_element!(ty, E => E::NB_STRESS, else 0);
    let mut d = Vec::new();
    for m in mat.chunks(9) {
        let one: Vec<Real> = match ty {
            ElementType::BernoulliBeam2 => tangent_bernoulli_beam_2(m[0], m[1], m[2]).to_vec(),
            ElementType::BernoulliBeam3 => {
                tangent_bernoulli_beam_3(m[0], m[1], m[3], m[4], m[5]).to_vec()
            }
            _ => tangent_discrete_kirchhoff_triangle_18(
                m[0],
                m[8],
                m[7],
                DktBendingStiffness::Legacy,
            )
            .to_vec(),
        };
        for _ in 0..nq {
            d.extend_from_slice(&one);
        }
    }
    Array::from_vec(d, ns * ns).unwrap()
}

/// UD-343: the `extra_normal` rows of a 3-D beam case
/// are admissible by the default criterion (`|n·n − 1|` and `|n·t̂|` at
/// most [`BEAM3_NORMAL_ADMISSIBILITY`]); the margins are printed.
fn fixture_normals_are_admissible(mesh: &Mesh, ty: ElementType) {
    if ty != ElementType::BernoulliBeam3 {
        return;
    }
    let x_g = mesh.nodes();
    let x = x_g.as_slice();
    let conn = mesh.connectivity(ty, G).unwrap();
    let normals = mesh.data().reals(EXTRA_NORMAL).unwrap().get(ty, G).unwrap();
    let (mut unit, mut orth): (Real, Real) = (0., 0.);
    for (e, c) in conn.as_slice().chunks(2).enumerate() {
        let (a, b) = (c[0] as usize, c[1] as usize);
        let d: Vec<Real> = (0..3).map(|i| x[3 * b + i] - x[3 * a + i]).collect();
        let l = d.iter().map(|v| v * v).sum::<Real>().sqrt();
        let n = &normals.as_slice()[3 * e..3 * e + 3];
        let nn: Real = n.iter().map(|v| v * v).sum();
        let nt: Real = (0..3).map(|i| n[i] * d[i] / l).sum();
        unit = unit.nan_max((nn - 1.).abs());
        orth = orth.nan_max(nt.abs());
    }
    eprintln!(
        "UD-343 fixture normals ({} elements): max |n·n − 1| = {unit:e}, max |n·t| = {orth:e} \
         (bound {BEAM3_NORMAL_ADMISSIBILITY:e})",
        conn.len()
    );
    assert!(unit <= BEAM3_NORMAL_ADMISSIBILITY && orth <= BEAM3_NORMAL_ADMISSIBILITY);
}

/// The corrected default engine (UD-293) against the legacy one on the
/// same mesh: rotations, Jacobians and `B·T` bit-identical; `N·T` identical
/// except the θy row of `_bernoulli_beam_3`, which is the exact negation.
fn corrected_default_differs_only_in_theta_y(
    legacy: &StructuralFeEngine,
    mesh: &Mesh,
    ty: ElementType,
) {
    fixture_normals_are_admissible(mesh, ty);
    let fe = StructuralFeEngine::with_precomputation(mesh, None, "structural_fem").unwrap();
    assert_eq!(fe.upstream_compat(), StructuralUpstreamCompat::default());
    let bits = |a: &Array<Real>| a.as_slice().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert_eq!(
        bits(fe.rotations(mesh, ty, G).unwrap()),
        bits(legacy.rotations(mesh, ty, G).unwrap())
    );
    assert_eq!(
        bits(fe.jacobians(mesh, ty, G).unwrap()),
        bits(legacy.jacobians(mesh, ty, G).unwrap())
    );
    assert_eq!(
        bits(fe.shapes_derivatives(mesh, ty, G).unwrap()),
        bits(legacy.shapes_derivatives(mesh, ty, G).unwrap())
    );
    let (ndof, nc) =
        with_structural_element!(ty, E => (E::NB_DOF_PER_NODE, E::NB_ELEMENT_DOFS), else panic!());
    let (c, l) = (
        fe.shapes(mesh, ty, G).unwrap(),
        legacy.shapes(mesh, ty, G).unwrap(),
    );
    let mut flipped = 0;
    for (i, (a, b)) in c.as_slice().iter().zip(l.as_slice()).enumerate() {
        let row = (i % (ndof * nc)) % ndof;
        if ty == ElementType::BernoulliBeam3 && row == 4 {
            // exact negation; zero sums are +0 in both modes (round to nearest)
            if *b == 0. {
                assert_eq!(*a, 0., "θy entry {i}");
            } else {
                assert_eq!(a.to_bits(), (-b).to_bits(), "θy entry {i}");
                flipped += 1;
            }
        } else {
            assert_eq!(a.to_bits(), b.to_bits(), "N·T entry {i}");
        }
    }
    if ty == ElementType::BernoulliBeam3 {
        assert!(flipped > 0, "the θy row must be exercised");
    }
}

fn run(case: &str) {
    let name = format!("fe::structural_{case}_matches_oracle");
    let mut cx = Ctx {
        t: DiffTest::new(&name, &format!("fe/structural/{case}")).unwrap(),
        summary: Vec::new(),
    };
    let ty: ElementType = cx.t.fixture().manifest().parameters["element_type"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (ndof, ns, nc, has_shapes) = with_structural_element!(ty,
        E => (E::NB_DOF_PER_NODE, E::NB_STRESS, E::NB_ELEMENT_DOFS, E::HAS_SHAPES), else panic!());
    let mesh = case_mesh(&cx, ty);
    let fe = StructuralFeEngine::with_precomputation_compat(
        &mesh,
        None,
        "structural_fem",
        StructuralUpstreamCompat::UPSTREAM,
    )
    .unwrap();
    corrected_default_differs_only_in_theta_y(&fe, &mesh, ty);
    let rule = fe.quadrature_rule(ty, G).unwrap().clone();
    let nq = rule.nb_points();
    assert_eq!(nq, param_usize(&cx.t, "nb_quadrature_points"));

    cx.check("integration_points", rule.points.as_slice());
    cx.check("integration_weights", &rule.weights);
    cx.check("rotations", fe.rotations(&mesh, ty, G).unwrap().as_slice());
    cx.check("jacobians", fe.jacobians(&mesh, ty, G).unwrap().as_slice());
    cx.check(
        "shapes_derivatives",
        fe.shapes_derivatives(&mesh, ty, G).unwrap().as_slice(),
    );
    if has_shapes {
        cx.check("shapes", fe.shapes(&mesh, ty, G).unwrap().as_slice());
    }

    let u = cx.input_array("field/nodal");
    let mut strain = Array::new(1);
    fe.gradient_on_integration_points(&mesh, u.view(), &mut strain, ndof, ty, G, None)
        .unwrap();
    assert_eq!(strain.nb_component(), ns);
    cx.check("field/strain", strain.as_slice());
    if has_shapes {
        let mut uq = Array::new(1);
        fe.interpolate_on_integration_points(&mesh, u.view(), &mut uq, ndof, ty, G, None)
            .unwrap();
        cx.check("field/interpolated", uq.as_slice());
    }

    let d = tangents(ty, &cx.input("material"), nq);
    cx.check("tangent", d.as_slice());
    let mut btdb = Array::new(1);
    fe.compute_btdb(&mesh, d.view(), &mut btdb, ty, G, None)
        .unwrap();
    cx.check("btdb", btdb.as_slice());
    let mut k = Array::new(1);
    fe.element_stiffness(&mesh, d.view(), &mut k, ty, G)
        .unwrap();
    assert_eq!(k.nb_component(), nc * nc);
    cx.check("stiffness", k.as_slice());

    let mut sigma = Array::new(1);
    fe.compute_stresses(&mesh, d.view(), u.view(), &mut sigma, ty, G)
        .unwrap();
    cx.check("stress", sigma.as_slice());
    let mut btd = Array::new(1);
    fe.compute_btd(&mesh, sigma.view(), &mut btd, ty, G, None)
        .unwrap();
    cx.check("btd", btd.as_slice());
    let mut fint = Array::new(1);
    fe.integrate(&mesh, btd.view(), &mut fint, nc, ty, G, None)
        .unwrap();
    cx.check("internal_force_elemental", fint.as_slice());

    let tg = cx.input_array("traction/global");
    let mut tl = Array::new(1);
    fe.tractions_to_local(&mesh, tg.view(), &mut tl, ty, G)
        .unwrap();
    cx.check("traction/local", tl.as_slice());
    if has_shapes {
        let mut ntb = Array::new(1);
        fe.compute_ntb(&mesh, tl.view(), &mut ntb, ty, G, None)
            .unwrap();
        cx.check("ntb", ntb.as_slice());
        let mut load = Array::new(1);
        fe.line_load_local(&mesh, tl.view(), &mut load, ty, G)
            .unwrap();
        cx.check("line_load", load.as_slice());

        let b = cx.input_array("ntbn/b");
        let mut ntbn = Array::new(1);
        fe.compute_ntbn(&mesh, b.view(), &mut ntbn, ty, G, None)
            .unwrap();
        cx.check("ntbn", ntbn.as_slice());

        let mrule = akantu_fe::gauss_rule(ty, fe.integration_orders().mass_order(ty)).unwrap();
        cx.check("mass/integration_points", mrule.points.as_slice());
        let mut ms = Array::new(1);
        fe.mass_shapes(&mesh, &mut ms, ty, G).unwrap();
        cx.check("mass/shapes", ms.as_slice());
        let field = cx.input_array("mass/field");
        let mut m = Array::new(1);
        fe.consistent_mass(&mesh, field.view(), &mut m, ty, G)
            .unwrap();
        cx.check("mass/elemental", m.as_slice());
    }
    cx.finish(&name);
}

#[test]
fn structural_bernoulli_beam_2_fe_engine_matches_oracle() {
    run("bernoulli_beam_2_fe_engine");
}

#[test]
fn structural_bernoulli_beam_2_model_matches_oracle() {
    run("bernoulli_beam_2_model");
}

#[test]
fn structural_bernoulli_beam_2_inclined_matches_oracle() {
    run("bernoulli_beam_2_inclined");
}

#[test]
fn structural_bernoulli_beam_3_fe_engine_matches_oracle() {
    run("bernoulli_beam_3_fe_engine");
}

#[test]
fn structural_bernoulli_beam_3_model_matches_oracle() {
    run("bernoulli_beam_3_model");
}

#[test]
fn structural_bernoulli_beam_3_oriented_matches_oracle() {
    run("bernoulli_beam_3_oriented");
}

#[test]
fn structural_discrete_kirchhoff_triangle_18_model_matches_oracle() {
    run("discrete_kirchhoff_triangle_18_model");
}

#[test]
fn structural_discrete_kirchhoff_triangle_18_distorted_matches_oracle() {
    run("discrete_kirchhoff_triangle_18_distorted");
}
