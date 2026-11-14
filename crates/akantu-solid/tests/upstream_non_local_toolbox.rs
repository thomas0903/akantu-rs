// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// test_non_local_toolbox/{test_non_local_averaging.cc,
// test_remove_damage_weight_function.cc (+ .verified),
// test_pair_computation.cc, test_build_neighborhood_parallel.cc}.

//! TEST-077 `test_non_local_averaging`, TEST-078
//! `test_remove_damage_weight_function`, TEST-080 `test_pair_computation`
//! (serial level) through the solid mechanics model with the upstream test
//! laws (`non_local_common`). TEST-079 `test_build_neighborhood_parallel`
//! and the 2-rank level of TEST-080 need the neighbourhood grid
//! synchronisers: the model refuses a distributed mesh, checked
//! here.

mod non_local_common;

use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt};
use akantu_core::{ElementType, GhostType, Real};
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use non_local_common::{TestNonLocalMaterial, data, register, test_law, test_law_mut};

const QUAD: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

/// Upstream `diff -biw` equality of two texts.
fn diff_biw(out: &str, reference: &str) -> Result<(), String> {
    let norm = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase()
            })
            .collect()
    };
    let (a, b) = (norm(out), norm(reference));
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        if x != y {
            return Err(format!("line {}: {x:?} vs reference {y:?}", k + 1));
        }
    }
    if a.len() != b.len() {
        return Err(format!("{} lines vs {} reference lines", a.len(), b.len()));
    }
    Ok(())
}

/// Model on `mesh` with `dat`, the test law registered as `test_material`,
/// `MeshDataMaterialSelector<std::string>("physical_names")`, `initFull()`.
fn model(
    mesh: &str,
    dat: &str,
    ctor: fn(
        &akantu_model::constitutive_law::LawSpec,
        Option<&Mesh>,
    ) -> akantu_core::Result<TestNonLocalMaterial<2>>,
) -> SolidMechanicsModel {
    let mut m = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut m, &data(mesh), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(data(dat)).unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(m), None, "solid_mechanics_model", parser)
            .unwrap();
    register(&mut model.physics.factory, "test_material", ctor);
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model
}

/// Set the `eigen_grad_u` of the quadrangle points of every law: `-1·a(k)`
/// where `k` counts the law's quadrature points.
fn set_eigen_grad_u(model: &mut SolidMechanicsModel, a: impl Fn(usize) -> Matrix<2, 2>) {
    for m in 0..model.nb_materials() {
        let law = model.material_mut(m).unwrap();
        let mut e = law.base_mut().eigen_grad_u.get_mut(QUAD, NG).unwrap();
        for (k, g) in e
            .as_mut_slice()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let v = a(k).scale(-1.);
            g[0] = v[0][0];
            g[1] = v[0][1];
            g[2] = v[1][0];
            g[3] = v[1][1];
        }
    }
}

fn frobenius(m: &[Real]) -> Real {
    m.iter().map(|x| x * x).sum::<Real>().sqrt()
}

/// TEST-077: the non-local average of a constant field is the field
/// (upstream criterion `Σ ‖ε_nl − ε‖_F ≤ 1e-12` over every point).
#[test]
fn test_non_local_averaging() {
    let mut model = model(
        "plate.msh",
        "material_avg.dat",
        TestNonLocalMaterial::test_material,
    );
    assert!(model.is_non_local());
    assert_eq!(model.nb_materials(), 2);
    let applied = Matrix::<2, 2>::identity().scale(2.);
    set_eigen_grad_u(&mut model, |_| applied);
    model.assemble_internal_forces().unwrap();
    let mut test_result = 0.;
    for m in 0..model.nb_materials() {
        let nl = test_law(&model, m).internal("grad_u non local").unwrap();
        for g in nl.get(QUAD, NG).unwrap().as_slice().as_chunks::<4>().0 {
            let d = [g[0] - 2., g[1], g[2], g[3] - 2.];
            test_result += frobenius(&d);
        }
    }
    assert!(test_result <= 10.0e-13, "the total norm is: {test_result}");
    // both laws share the neighbourhood `test_region` (one base_wf section)
    let manager = model.non_local_manager().unwrap();
    assert_eq!(manager.neighborhoods().len(), 1);
    assert_eq!(
        manager.neighborhood("test_region").unwrap().points().len(),
        64
    );
}

/// TEST-077's set-up (upstream's test law,
/// `base_wf` with radius 0.5 on the 4 × 4 plate of elements of size 0.5, two
/// laws sharing the neighbourhood) with a **non-uniform** field, so that
/// every point averages over a real neighbourhood of points of both laws
/// (TEST-077 itself averages a constant, which any normalised average
/// reproduces). Reference: the definition of the average, evaluated here
/// independently of the manager — every point's physical position and
/// integration weight `det J · w` computed from the mesh nodes with the
/// bilinear map of `_quadrangle_4` (only the natural coordinates and
/// weights of the quadrature rule are taken from the FE engine), the base
/// weight `w(r) = (1 − r²/R²)²` for `r ≤ R`, and `ε_nl(q) = Σ_j w(r_qj) J_j
/// ε(q_j) / Σ_j w(r_qj) J_j` over every point of the plate, in plain loops
/// (the manager sums over its grid pairs). Bound: rule SW5-3 (PROPOSED),
/// `1e-13` relative to the largest |component| (a sum of at most 64
/// positive weighted terms in another order: ≤ 64 · 4 ulp ≈ 6e-14).
#[test]
fn non_local_averaging_over_a_real_neighbourhood() {
    let mut model = model(
        "plate.msh",
        "material_avg.dat",
        TestNonLocalMaterial::test_material,
    );
    // a non-uniform field: point k of law m gets eigen_grad_u −a(m, k)
    let field = |m: usize, k: usize| {
        let x = (k as Real) * 0.37 + (m as Real) * 1.7;
        [1.0 + x.sin(), 0.25 * x, 0.1 * x * x, 2.0 - x.cos()]
    };
    for m in 0..model.nb_materials() {
        let law = model.material_mut(m).unwrap();
        let mut e = law.base_mut().eigen_grad_u.get_mut(QUAD, NG).unwrap();
        for (k, g) in e
            .as_mut_slice()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let a = field(m, k);
            for c in 0..4 {
                g[c] = -a[c];
            }
        }
    }
    model.assemble_internal_forces().unwrap();
    let radius = 0.5;
    // every point: law, slot in the law, position, J, local value
    let (natural, weights) = {
        let rule = model
            .physics
            .base
            .fems
            .get("")
            .unwrap()
            .quadrature_rule(QUAD, NG)
            .unwrap();
        (rule.points.clone(), rule.weights.clone())
    };
    let nq = weights.len();
    assert_eq!(nq, 4);
    let mesh = model.physics.base.mesh.read().unwrap();
    let x = mesh.nodes().as_slice().to_vec();
    let conn = mesh.connectivity(QUAD, NG).unwrap().as_slice().to_vec();
    drop(mesh);
    struct P {
        law: usize,
        slot: usize,
        pos: [Real; 2],
        jxw: Real,
        local: [Real; 4],
    }
    let mut points: Vec<P> = Vec::new();
    for m in 0..model.nb_materials() {
        let filter = model
            .material(m)
            .unwrap()
            .base()
            .element_filter
            .get(QUAD, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        let local = model
            .material(m)
            .unwrap()
            .base()
            .grad_u
            .get(QUAD, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        for (e, &el) in filter.iter().enumerate() {
            let nodes: Vec<[Real; 2]> = conn[4 * el as usize..4 * el as usize + 4]
                .iter()
                .map(|&n| [x[2 * n as usize], x[2 * n as usize + 1]])
                .collect();
            for (q, &wq) in weights.iter().enumerate() {
                let (xi, eta) = (natural.at(0, q), natural.at(1, q));
                // _quadrangle_4 shapes ¼(1 ± ξ)(1 ± η), nodes counter-clockwise
                let n = [
                    0.25 * (1.0 - xi) * (1.0 - eta),
                    0.25 * (1.0 + xi) * (1.0 - eta),
                    0.25 * (1.0 + xi) * (1.0 + eta),
                    0.25 * (1.0 - xi) * (1.0 + eta),
                ];
                let dn_dxi = [
                    -0.25 * (1.0 - eta),
                    0.25 * (1.0 - eta),
                    0.25 * (1.0 + eta),
                    -0.25 * (1.0 + eta),
                ];
                let dn_deta = [
                    -0.25 * (1.0 - xi),
                    -0.25 * (1.0 + xi),
                    0.25 * (1.0 + xi),
                    0.25 * (1.0 - xi),
                ];
                let mut pos = [0.0; 2];
                let mut jac = [[0.0; 2]; 2];
                for i in 0..4 {
                    for d in 0..2 {
                        pos[d] += n[i] * nodes[i][d];
                        jac[0][d] += dn_dxi[i] * nodes[i][d];
                        jac[1][d] += dn_deta[i] * nodes[i][d];
                    }
                }
                let det = jac[0][0] * jac[1][1] - jac[0][1] * jac[1][0];
                assert!(det > 0.0, "element {el}: det J {det}");
                let s = e * nq + q;
                points.push(P {
                    law: m,
                    slot: s,
                    pos,
                    jxw: det * wq,
                    local: [
                        local[4 * s],
                        local[4 * s + 1],
                        local[4 * s + 2],
                        local[4 * s + 3],
                    ],
                });
            }
        }
    }
    assert_eq!(points.len(), 64);
    let w = |r: Real| {
        if r <= radius {
            let a = 1.0 - r * r / (radius * radius);
            a * a
        } else {
            0.0
        }
    };
    let mut scale: Real = 0.0;
    for p in &points {
        for v in p.local {
            scale = scale.max(v.abs());
        }
    }
    let mut worst: Real = 0.0;
    let mut min_neighbours = usize::MAX;
    let mut moved: Real = 0.0;
    for p in &points {
        let (mut num, mut den, mut nb) = ([0.0; 4], 0.0, 0);
        for q in &points {
            let r = ((p.pos[0] - q.pos[0]).powi(2) + (p.pos[1] - q.pos[1]).powi(2)).sqrt();
            let wq = w(r) * q.jxw;
            if wq > 0.0 {
                nb += 1;
            }
            den += wq;
            for (n, v) in num.iter_mut().zip(q.local) {
                *n += wq * v;
            }
        }
        min_neighbours = min_neighbours.min(nb);
        let nl = test_law(&model, p.law)
            .internal("grad_u non local")
            .unwrap()
            .get(QUAD, NG)
            .unwrap();
        let got = &nl.as_slice()[4 * p.slot..4 * p.slot + 4];
        for ((&g, n), l) in got.iter().zip(num).zip(p.local) {
            worst = worst.max((g - n / den).abs());
            moved = moved.max((g - l).abs());
        }
    }
    // a real neighbourhood: several points of the plate (of both laws at
    // the interface between them) contribute to every average, which then
    // differs from the local value
    assert!(
        min_neighbours >= 4,
        "a point averages over {min_neighbours} points"
    );
    assert!(moved > 0.1 * scale, "the averages equal the local values");
    assert!(
        worst <= 1e-13 * scale,
        "non-local average vs its definition: {worst:e} (scale {scale})"
    );
    eprintln!(
        "64 points, at least {min_neighbours} neighbours each; max |ε_nl − definition| = \
         {worst:e} (scale {scale}); max |ε_nl − ε| = {moved:e}"
    );
}

/// TEST-078: `remove_wf` — weights printed before and after damaging four
/// points of every law (compared with the upstream `.verified`), then the
/// averages away from and inside the damaged region (upstream criterion,
/// including its reuse of the last `difference` for the damaged points).
#[test]
fn test_remove_damage_weight_function() {
    let mut model = model(
        "plate.msh",
        "material_remove_damage.dat",
        TestNonLocalMaterial::test_material_damage,
    );
    let applied = Matrix::<2, 2>::identity().scale(2.);
    let modified = Matrix::<2, 2>::identity();
    let damaged = |k: usize| (12..=15).contains(&k);
    set_eigen_grad_u(&mut model, |k| if damaged(k) { modified } else { applied });
    model.assemble_internal_forces().unwrap();
    let mut out = String::new();
    for n in ["mat_1", "mat_2"] {
        out += &model
            .non_local_manager()
            .unwrap()
            .neighborhood(n)
            .unwrap()
            .save_weights_text()
            .unwrap();
    }
    for m in 0..model.nb_materials() {
        let law = test_law_mut(&mut model, m);
        let mut d = law
            .internal_mut("damage")
            .unwrap()
            .get_mut(QUAD, NG)
            .unwrap();
        for (k, v) in d.as_mut_slice().iter_mut().enumerate() {
            if damaged(k) {
                *v = 0.9;
            }
        }
    }
    model.assemble_internal_forces().unwrap();
    for n in ["mat_1", "mat_2"] {
        out += &model
            .non_local_manager()
            .unwrap()
            .neighborhood(n)
            .unwrap()
            .save_weights_text()
            .unwrap();
    }
    let reference =
        std::fs::read_to_string(data("test_remove_damage_weight_function.verified")).unwrap();
    diff_biw(&out, &reference).unwrap();

    let mut test_result = 0.;
    for m in 0..model.nb_materials() {
        let nl = test_law(&model, m).internal("grad_u non local").unwrap();
        let mut in_damaged = [0.; 4];
        let mut difference = [Real::NAN; 4];
        for (k, g) in nl
            .get(QUAD, NG)
            .unwrap()
            .as_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
        {
            if damaged(k) {
                for c in 0..4 {
                    in_damaged[c] += g[c];
                }
            } else {
                difference = [g[0] - 2., g[1], g[2], g[3] - 2.];
            }
            test_result += frobenius(&difference);
        }
        for v in &mut in_damaged {
            *v *= 1. / 4.;
        }
        in_damaged[0] -= 1.41142;
        in_damaged[3] -= 1.41142;
        test_result += frobenius(&in_damaged);
    }
    assert!(test_result <= 10.0e-5, "the total norm is: {test_result}");
}

/// TEST-080 (serial level): the grid pair lists of `mat_1` and `mat_2`
/// equal a brute-force `n²` enumeration (same radius criterion, same half
/// lists, pairs only within a law).
#[test]
fn test_pair_computation() {
    let model = model(
        "pair_test.msh",
        "material_remove_damage.dat",
        TestNonLocalMaterial::test_material_damage,
    );
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let fe = model.physics.base.fems.get("").unwrap();
    let mut x = akantu_core::Array::new(2);
    fe.integration_points_coordinates(&mesh, &mut x, QUAD, NG, None)
        .unwrap();
    let nq = fe.nb_integration_points(QUAD, NG).unwrap();
    let law_of = model
        .material_by_element()
        .get(QUAD, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let radius = 0.5;
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let n = x.len();
    // (element, point) pairs of the brute force, upstream half criterion
    let mut brute = Vec::new();
    for g1 in 0..n {
        for g2 in 0..n {
            let (e1, e2) = (g1 / nq, g2 / nq);
            if law_of[e1] != law_of[e2] {
                continue;
            }
            let d = akantu_model::non_local::distance(x.tuple(g1), x.tuple(g2));
            if d <= radius + tol && g1 <= g2 {
                brute.push((g1, g2));
            }
        }
    }
    let manager = model.non_local_manager().unwrap();
    let mut grid = Vec::new();
    for name in ["mat_1", "mat_2"] {
        let nh = manager.neighborhood(name).unwrap();
        assert!(nh.pair_list(GhostType::Ghost).is_empty());
        for (q1, q2) in nh.pairs(NG) {
            grid.push((q1.global_num as usize, q2.global_num as usize));
        }
    }
    assert_eq!(
        grid.len(),
        brute.len(),
        "The number of pairs is not correct"
    );
    let mut sorted = grid.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), grid.len(), "duplicated pairs");
    for p in &grid {
        assert!(brute.contains(p), "The pairs are not correct: {p:?}");
    }
}
