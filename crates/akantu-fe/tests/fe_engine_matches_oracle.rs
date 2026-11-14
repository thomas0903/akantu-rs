// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential tests of the
// FE engine on meshes against fixtures/fe/engine/* and
// fixtures/fe/engine_cohesive/* (written by oracle/probes/fe/fe_engine_probe*
// from upstream FEEngineTemplate<IntegratorGauss, ShapeLagrange, kind>).

//! `fe::engine_<type>_matches_oracle` (FE-013…033): precomputed shapes,
//! derivatives and `det J · w`, interpolation, gradients (full and
//! filtered), integrals (field, scalar `Math::reduce`), integration-point
//! coordinates, `BᵀD`, `BᵀDB` (orders 4 and 2), `Nᵀb`, `NᵀbN`, inradius,
//! inverse map + `contains`, assembled lumped mass and facet normals on the
//! upstream test meshes; cohesive shapes, natural derivatives, mid-surface
//! Jacobians, mean/opening interpolation, integrals and normals on generated
//! meshes. Tolerances: `fixtures/fe/engine*/tolerances.json`; a bit-exactness
//! summary is printed with `--nocapture`.

mod common;

use akantu_core::{Array, Element, ElementKind, ElementType, GhostType, Idx};
use akantu_fe::element_class::ElementClass;
use akantu_fe::shape::{CohesiveReduction, ShapeFamily};
use akantu_fe::{FeEngine, with_element_class};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::DiffTest;

use common::bit_stats;

const G: GhostType = GhostType::NotGhost;

struct Ctx {
    t: DiffTest,
    summary: Vec<String>,
}

impl Ctx {
    fn new(test: &str, case: &str) -> Self {
        Self {
            t: DiffTest::new(test, case).unwrap(),
            summary: Vec::new(),
        }
    }

    fn input(&self, q: &str) -> Vec<f64> {
        self.t.reference_f64(q).unwrap().data
    }

    fn input_idx(&self, q: &str) -> Vec<Idx> {
        self.t
            .reference_i64(q)
            .unwrap()
            .data
            .into_iter()
            .map(|v| v as Idx)
            .collect()
    }

    /// Compare a `[tuple, row, col]` quantity with per-tuple column-major
    /// matrices of `rows × cols`.
    fn check_matrices(&mut self, q: &str, a: &[f64], rows: usize, cols: usize) {
        let mut rm = Vec::with_capacity(a.len());
        for t in a.chunks(rows * cols) {
            for r in 0..rows {
                for c in 0..cols {
                    rm.push(t[r + c * rows]);
                }
            }
        }
        self.check(q, &rm);
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

fn regular(ty: ElementType) {
    let name = ty.name();
    let mut cx = Ctx::new(
        &format!("fe::engine_{}_matches_oracle", name.trim_start_matches('_')),
        &format!("fe/engine/{name}"),
    );
    let mesh = common::fe_test_mesh(ty);
    let dim = mesh.spatial_dimension();
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "my_fem",
    )
    .unwrap();
    let nn = ty.nb_nodes_per_interpolation_element();
    let nd = ty.natural_space_dimension();

    cx.check(
        "integration_points",
        &fe.integration_points(ty, G).unwrap().as_slice()
            [..nd.max(1) * fe.nb_integration_points(ty, G).unwrap()],
    );
    cx.check("shapes", fe.shapes(ty, G).unwrap().as_slice());
    cx.check_matrices(
        "shapes_derivatives",
        fe.shapes_derivatives(ty, G).unwrap().as_slice(),
        dim,
        nn,
    );
    cx.check("jacobians", fe.jacobians(ty, G).unwrap().as_slice());

    let u = Array::from_vec(cx.input("field/nodal"), 2).unwrap();
    let mut uq = Array::new(2);
    fe.interpolate_on_integration_points(&mesh, u.view(), &mut uq, 2, ty, G, None)
        .unwrap();
    cx.check("field/interpolated", uq.as_slice());
    let mut grad = Array::new(2 * dim);
    fe.gradient_on_integration_points(&mesh, u.view(), &mut grad, 2, ty, G, None)
        .unwrap();
    cx.check_matrices("field/gradient", grad.as_slice(), 2, dim);
    let filter = cx.input_idx("filter");
    let mut gradf = Array::new(2 * dim);
    fe.gradient_on_integration_points(&mesh, u.view(), &mut gradf, 2, ty, G, Some(&filter))
        .unwrap();
    cx.check_matrices("field/gradient_filtered", gradf.as_slice(), 2, dim);
    let mut uqf = Array::new(2);
    fe.interpolate_on_integration_points(&mesh, u.view(), &mut uqf, 2, ty, G, Some(&filter))
        .unwrap();
    cx.check("field/interpolated_filtered", uqf.as_slice());
    // integration of the oracle's quadrature fields (isolates the GEMV sums)
    let uq_ref = Array::from_vec(cx.input("field/interpolated"), 2).unwrap();
    let uqf_ref = Array::from_vec(cx.input("field/interpolated_filtered"), 2).unwrap();
    let mut intf = Array::new(2);
    fe.integrate(uqf_ref.view(), &mut intf, 2, ty, G, Some(&filter))
        .unwrap();
    cx.check("field/integrated_filtered", intf.as_slice());
    let mut intu = Array::new(2);
    fe.integrate(uq_ref.view(), &mut intu, 2, ty, G, None)
        .unwrap();
    cx.check("field/integrated", intu.as_slice());
    let u0 = Array::from_vec(uq_ref.tuples().map(|t| t[0]).collect(), 1).unwrap();
    let s = fe.integrate_scalar(u0.view(), ty, G, None).unwrap();
    cx.check("field/integral_scalar", &[s]);

    let mut xq = Array::new(dim);
    fe.integration_points_coordinates(&mesh, &mut xq, ty, G, None)
        .unwrap();
    cx.check("quad_coords", xq.as_slice());
    let xq = Array::from_vec(cx.input("quad_coords"), dim).unwrap();

    // operators on the probe's element subsets
    let opf = cx.input_idx("op_filter");
    let d = Array::from_vec(cx.input("btd/input"), dim * dim).unwrap();
    let mut btd = Array::new(dim * nn);
    fe.compute_btd(d.view(), &mut btd, ty, G, Some(&opf))
        .unwrap();
    cx.check_matrices("btd", btd.as_slice(), dim, nn);
    let b4f = cx.input_idx("btdb_filter");
    let ts = akantu_core::voigt::voigt_table(dim).unwrap().size;
    let d4 = Array::from_vec(cx.input("btdb/input"), ts * ts).unwrap();
    let mut btdb = Array::new(1);
    fe.compute_btdb(d4.view(), &mut btdb, 4, ty, G, Some(&b4f))
        .unwrap();
    cx.check_matrices("btdb", btdb.as_slice(), dim * nn, dim * nn);
    let d2 = Array::from_vec(cx.input("btdb2/input"), dim * dim).unwrap();
    let mut btdb2 = Array::new(1);
    fe.compute_btdb(d2.view(), &mut btdb2, 2, ty, G, Some(&opf))
        .unwrap();
    cx.check_matrices("btdb2", btdb2.as_slice(), nn, nn);
    let b = Array::from_vec(cx.input("ntb/input"), 2).unwrap();
    let mut ntb = Array::new(1);
    fe.compute_ntb(b.view(), &mut ntb, ty, G, Some(&opf))
        .unwrap();
    cx.check_matrices("ntb", ntb.as_slice(), 2, nn);
    let sb = Array::from_vec(cx.input("ntbn/input"), 1).unwrap();
    let mut ntbn = Array::new(1);
    fe.compute_ntbn(sb.view(), &mut ntbn, ty, G, Some(&opf))
        .unwrap();
    cx.check_matrices("ntbn", ntbn.as_slice(), nn, nn);

    // inradius, inverse map, contains
    let ne = mesh.nb_element(ty, G);
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let mut r = Vec::new();
    let mut xi = Vec::new();
    let mut inside = Vec::new();
    for e in 0..ne {
        let el = Element::new(ty, e as Idx, G);
        r.push(fe.element_inradius(&mesh, el).unwrap());
        for q in 0..nq {
            let x = xq.tuple(e * nq + q);
            let mut nat = vec![0.0; dim];
            fe.inverse_map(&mesh, x, el, &mut nat).unwrap();
            xi.extend_from_slice(&nat);
            inside.push(i64::from(fe.contains(&mesh, x, el).unwrap()));
        }
    }
    cx.check("inradius", &r);
    cx.check("inverse_map/xi", &xi);
    cx.t.check_i64("inverse_map/contains", &inside);

    // lumped mass, assembled like DOFManager::assembleElementalArrayLocalArray
    let mut rho = |m: &mut [f64], el: Element| -> akantu_core::Result<()> {
        let nb_dof = 2;
        for (k, v) in m.iter_mut().enumerate() {
            let dd = (k % nb_dof) as f64;
            *v = 1.5 + 0.25 * dd + 0.01 * f64::from(el.index());
        }
        Ok(())
    };
    let elemental = fe
        .lumped_field_elemental(&mesh, &mut rho, 2, ty, G)
        .unwrap();
    let mut lumped = vec![0.0; mesh.nb_nodes() * 2];
    let conn = mesh.connectivity(ty, G).unwrap();
    for (e, c) in conn.tuples().enumerate() {
        let v = elemental.tuple(e);
        for (n, &node) in c.iter().enumerate() {
            for dd in 0..2 {
                lumped[node as usize * 2 + dd] += 1.0 * v[n * 2 + dd];
            }
        }
    }
    cx.check("lumped_mass", &lumped);

    // facet normals and facet jacobians
    if dim > 1 {
        let mut fb = FeEngine::with_precomputation(
            &mesh,
            Some(dim - 1),
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "boundary",
        )
        .unwrap();
        fb.compute_normals_on_integration_points(&mesh, G).unwrap();
        let facets: Vec<ElementType> = fb.element_types(&mesh, G).collect();
        for ft in facets {
            let n = fb
                .normals_on_integration_points(ft, G)
                .unwrap()
                .as_slice()
                .to_vec();
            cx.check(&format!("normals/{}", ft.name()), &n);
            let j = fb.jacobians(ft, G).unwrap().as_slice().to_vec();
            cx.check(&format!("facet_jacobians/{}", ft.name()), &j);
        }
    }
    cx.finish(name);
}

fn cohesive_mesh(cx: &Ctx, ty: ElementType, dim: usize) -> Mesh {
    let nodes = cx.t.reference_f64("mesh/nodes").unwrap();
    let conn = cx.t.reference_i64("mesh/connectivity").unwrap();
    let mut mesh = Mesh::new(dim, "cohesive").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for x in nodes.data.chunks(dim) {
            acc.push_node(x).unwrap();
        }
        let mut c = acc.connectivity_mut(ty, G);
        for row in conn.data.chunks(conn.shape[1]) {
            let r: Vec<Idx> = row.iter().map(|&v| v as Idx).collect();
            c.push_tuple(&r);
        }
    }
    mesh
}

fn cohesive(ty: ElementType) {
    let name = ty.name();
    let mut cx = Ctx::new(
        &format!(
            "fe::engine_cohesive_{}_matches_oracle",
            name.trim_start_matches('_')
        ),
        &format!("fe/engine_cohesive/{name}"),
    );
    let dim = ty.spatial_dimension();
    let mesh = cohesive_mesh(&cx, ty, dim);
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Cohesive,
        ShapeFamily::Cohesive,
        "cohesive_fem",
    )
    .unwrap();
    let (nd, nn) = with_element_class!(ty, E => (E::NATURAL_DIM, E::NB_SHAPES), else (0, 0));
    cx.check("shapes", fe.shapes(ty, G).unwrap().as_slice());
    if nd > 0 {
        cx.check_matrices(
            "shapes_derivatives",
            fe.shapes_derivatives(ty, G).unwrap().as_slice(),
            nd,
            nn,
        );
    }
    cx.check("jacobians", fe.jacobians(ty, G).unwrap().as_slice());
    let u = Array::from_vec(cx.input("field/nodal"), 2).unwrap();
    let mut mean = Array::new(2);
    fe.interpolate_cohesive(
        &mesh,
        u.view(),
        &mut mean,
        ty,
        G,
        CohesiveReduction::Mean,
        None,
    )
    .unwrap();
    cx.check("field/mean", mean.as_slice());
    let mut opening = Array::new(2);
    fe.interpolate_cohesive(
        &mesh,
        u.view(),
        &mut opening,
        ty,
        G,
        CohesiveReduction::Opening,
        None,
    )
    .unwrap();
    cx.check("field/opening", opening.as_slice());
    let mut intf = Array::new(2);
    fe.integrate(opening.view(), &mut intf, 2, ty, G, None)
        .unwrap();
    cx.check("field/integrated_opening", intf.as_slice());
    let o0 = Array::from_vec(opening.tuples().map(|t| t[0]).collect(), 1).unwrap();
    let s = fe.integrate_scalar(o0.view(), ty, G, None).unwrap();
    cx.check("field/integral_scalar", &[s]);
    if dim > 1 {
        let all: Vec<Idx> = (0..mesh.nb_element(ty, G) as Idx).collect();
        let mut normals = Array::new(dim);
        fe.compute_cohesive_normals(
            &mesh,
            mesh.nodes().view(),
            &mut normals,
            ty,
            G,
            CohesiveReduction::Mean,
            Some(&all),
        )
        .unwrap();
        cx.check("normals", normals.as_slice());
    }
    cx.finish(name);
}

macro_rules! tests {
    ($f:ident; $($name:ident => $ty:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            $f(ElementType::$ty);
        }
    )*};
}

tests!(regular;
    engine_segment_2_matches_oracle => Segment2,
    engine_segment_3_matches_oracle => Segment3,
    engine_triangle_3_matches_oracle => Triangle3,
    engine_triangle_6_matches_oracle => Triangle6,
    engine_quadrangle_4_matches_oracle => Quadrangle4,
    engine_quadrangle_8_matches_oracle => Quadrangle8,
    engine_tetrahedron_4_matches_oracle => Tetrahedron4,
    engine_tetrahedron_10_matches_oracle => Tetrahedron10,
    engine_pentahedron_6_matches_oracle => Pentahedron6,
    engine_pentahedron_15_matches_oracle => Pentahedron15,
    engine_hexahedron_8_matches_oracle => Hexahedron8,
    engine_hexahedron_20_matches_oracle => Hexahedron20,
);

tests!(cohesive;
    engine_cohesive_1d_2_matches_oracle => Cohesive1d2,
    engine_cohesive_2d_4_matches_oracle => Cohesive2d4,
    engine_cohesive_2d_6_matches_oracle => Cohesive2d6,
    engine_cohesive_3d_6_matches_oracle => Cohesive3d6,
    engine_cohesive_3d_12_matches_oracle => Cohesive3d12,
    engine_cohesive_3d_8_matches_oracle => Cohesive3d8,
    engine_cohesive_3d_16_matches_oracle => Cohesive3d16,
);
