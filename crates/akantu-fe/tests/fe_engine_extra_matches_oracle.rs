// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential tests of the
// FE-engine outputs that fe_engine_matches_oracle does not cover, against
// fixtures/fe/engine_extra/* (written by oracle/probes/fe/fe_extra_probe*
// from upstream FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_regular>,
// src/fe_engine/{fe_engine_template_tmpl_field.hh, integrator_gauss_inline_impl.hh,
// shape_functions.cc, shape_functions_inline_impl.hh}).

//! `fe::engine_extra_<type>_matches_oracle`,
//! on the upstream test meshes with three ghost
//! copies of not-ghost elements appended:
//!
//! * FE-031: the elemental consistent field matrices (`assembleFieldMatrix`,
//!   `nb_dof = dim`, rule of order `2·degree`, `Nᵀ diag(ρ) N`) exactly as
//!   upstream hands them to `DOFManager::assembleElementalMatricesToMatrix`
//!   ([`FeEngine::field_matrix_elemental`]);
//! * FE-030: the elemental lumped arrays (row sum / diagonal scaling) as
//!   upstream hands them to `DOFManager::assembleElementalArrayLocalArray`
//!   ([`FeEngine::lumped_field_elemental`]), plus the oracle's assembled
//!   nodal array against a test-side element-order scatter (the production
//!   scatter is DOF-007);
//! * FE-014: arbitrary-order integration `IntegratorGauss::integrate<type,
//!   n>` for n = 1..5, per element and scalar (`Math::reduce`)
//!   ([`FeEngine::integrate_with_order`], [`FeEngine::integrate_scalar_with_order`]);
//! * FE-025: elemental-field interpolation from integration points with
//!   reordered filters on not-ghost and ghost elements, for the types where
//!   upstream is defined (UD-096): inverse basis matrices, interpolation
//!   point matrices and interpolated values
//!   ([`FeEngine::init_elemental_field_interpolation`],
//!   [`FeEngine::interpolate_elemental_field`]). The fixture must hold the
//!   12 `elemental/*` quantities for the 8 supported types of this set and
//!   none for the 4 others, for which the port returns
//!   `Error::NotImplemented`.
//!
//! Every comparison is exact (`fixtures/fe/engine_extra/tolerances.json`,
//! zero tolerance): the arithmetic order of upstream is replicated.

mod common;

use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::MeshAccessor;
use akantu_validation::DiffTest;

use common::bit_stats;

const NG: GhostType = GhostType::NotGhost;
const G: GhostType = GhostType::Ghost;

struct Ctx {
    t: DiffTest,
    summary: Vec<String>,
}

impl Ctx {
    fn input(&self, q: &str) -> Vec<f64> {
        self.t.reference_f64(q).unwrap().data
    }

    fn input_idx(&self, q: &str) -> Vec<Idx> {
        self.t
            .reference_i64(q)
            .unwrap()
            .data
            .into_iter()
            .map(|v| Idx::try_from(v).unwrap())
            .collect()
    }

    /// Compare `n` tuples of per-tuple column-major `rows × cols` matrices
    /// with a `[tuple, row, col]` quantity.
    fn check_matrices(&mut self, q: &str, a: &[f64], rows: usize, cols: usize, n: usize) {
        let mut rm = Vec::with_capacity(n * rows * cols);
        for t in a.chunks(rows * cols).take(n) {
            for r in 0..rows {
                for c in 0..cols {
                    rm.push(t[r + c * rows]);
                }
            }
        }
        self.check(q, &rm);
    }

    fn check(&mut self, q: &str, actual: &[f64]) {
        let e = self.t.reference_f64(q).unwrap();
        if e.data.len() == actual.len() {
            let (n, m) = bit_stats(&e.data, actual);
            self.summary.push(format!(
                "{q}: {} values, {n} not bit-identical, max {m} ulp",
                actual.len()
            ));
        }
        self.t.check_f64_shaped(q, &e.shape, actual);
    }

    fn finish(self, name: &str) {
        for l in &self.summary {
            eprintln!("{name}: {l}");
        }
        self.t.assert_pass();
    }
}

/// `rho(d, q) = 1.5 + 0.25 d + 0.01 e + 0.125 q` (the probe's functor, same
/// evaluation order), column-major `nb_dof × nq`.
#[allow(clippy::cast_precision_loss)] // small indices
fn rho(m: &mut [Real], el: akantu_core::Element, nb_dof: usize) {
    let nq = m.len() / nb_dof;
    for d in 0..nb_dof {
        for q in 0..nq {
            m[d + q * nb_dof] =
                1.5 + 0.25 * d as f64 + 0.01 * f64::from(el.index()) + 0.125 * q as f64;
        }
    }
}

fn extra(ty: ElementType) {
    let name = ty.name();
    let mut cx = Ctx {
        t: DiffTest::new(
            &format!(
                "fe::engine_extra_{}_matches_oracle",
                name.trim_start_matches('_')
            ),
            &format!("fe/engine_extra/{name}"),
        )
        .unwrap(),
        summary: Vec::new(),
    };
    let mut mesh = common::fe_test_mesh(ty);
    let dim = mesh.spatial_dimension();
    let nn = ty.nb_nodes_per_interpolation_element();
    {
        let gconn = cx.input_idx("ghost_connectivity");
        let mut acc = MeshAccessor::new(&mut mesh);
        let mut conn = acc.connectivity_mut(ty, G);
        for row in gconn.chunks(nn) {
            conn.push_tuple(row);
        }
    }
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "my_fem",
    )
    .unwrap();
    let ne = mesh.nb_element(ty, NG);

    // FE-031: elemental consistent field matrices
    let nb_dof = dim;
    let lmat = nb_dof * nn;
    let mut rho_fn = |m: &mut [Real], el| {
        rho(m, el, nb_dof);
        Ok(())
    };
    let mass = fe
        .field_matrix_elemental(&mesh, &mut rho_fn, nb_dof, ty, NG)
        .unwrap();
    let nrec = cx.t.reference_f64("field_matrix/elemental").unwrap().shape[0];
    assert_eq!(mass.len(), ne);
    cx.check_matrices("field_matrix/elemental", mass.as_slice(), lmat, lmat, nrec);

    // FE-030: elemental lumped arrays, and the assembled array by an
    // element-order scatter from zero (what DOFManagerDefault does)
    let lumped = fe
        .lumped_field_elemental(&mesh, &mut rho_fn, nb_dof, ty, NG)
        .unwrap();
    cx.check("lumped/elemental", lumped.as_slice());
    let mut assembled = vec![0.0; mesh.nb_nodes() * nb_dof];
    let conn = mesh.connectivity(ty, NG).unwrap();
    for e in 0..ne {
        let le = lumped.tuple(e);
        for (n, &node) in conn.tuple(e).iter().enumerate() {
            for d in 0..nb_dof {
                assembled[usize::try_from(node).unwrap() * nb_dof + d] += le[n * nb_dof + d];
            }
        }
    }
    cx.check("lumped/assembled", &assembled);

    // FE-014: arbitrary-order integration
    for order in 1..=5usize {
        let pre = format!("order_{order}/");
        let f = Array::from_vec(cx.input(&format!("{pre}field")), 2).unwrap();
        let mut out = Array::new(2);
        fe.integrate_with_order(&mesh, f.view(), &mut out, 2, ty, NG, order)
            .unwrap();
        cx.check(&format!("{pre}integrated"), out.as_slice());
        let f0: Vec<f64> = f.as_slice().chunks(2).map(|c| c[0]).collect();
        let f0 = Array::from_vec(f0, 1).unwrap();
        let s = fe
            .integrate_scalar_with_order(&mesh, f0.view(), ty, NG, order)
            .unwrap();
        cx.check(&format!("{pre}integral_scalar"), &[s]);
    }

    // FE-025: elemental-field interpolation (filtered, ghost). Upstream is
    // undefined for pentahedra and hexahedra (UD-096): the probe records
    // nothing for them and the port refuses them.
    let mut elemental = cx.t.fixture().quantity_names("elemental/*");
    elemental.sort_unstable();
    let interpolation_supported = !matches!(
        ty,
        ElementType::Pentahedron6
            | ElementType::Pentahedron15
            | ElementType::Hexahedron8
            | ElementType::Hexahedron20
    );
    if !interpolation_supported {
        assert!(
            elemental.is_empty(),
            "{name}: elemental quantities recorded for an unsupported type: {elemental:?}"
        );
        let points = ElementTypeMapArray::<Real>::new("points");
        let refused = fe.init_elemental_field_interpolation(&mesh, &points, None);
        assert!(
            matches!(refused, Err(Error::NotImplemented(_))),
            "{name}: UD-096 types must be refused: {:?}",
            refused.map(|_| ())
        );
    } else {
        let mut expected: Vec<String> = ["not_ghost", "ghost"]
            .iter()
            .flat_map(|g| {
                [
                    "points",
                    "filter",
                    "field",
                    "inv_matrices",
                    "itp_matrices",
                    "result",
                ]
                .iter()
                .map(move |q| format!("elemental/{g}/{q}"))
            })
            .collect();
        expected.sort_unstable();
        assert_eq!(elemental, expected, "{name}: elemental quantities");
        let mut points = ElementTypeMapArray::<Real>::new("points");
        let mut filter = ElementTypeMapArray::<Idx>::new("filter");
        let mut field = ElementTypeMapArray::<Real>::new("field");
        let mut result = ElementTypeMapArray::<Real>::new("result");
        for (g, gn) in [(NG, "not_ghost"), (G, "ghost")] {
            let p = cx.input(&format!("elemental/{gn}/points"));
            points
                .as_map_mut()
                .insert(ty, g, Array::from_vec(p, dim).unwrap())
                .unwrap();
            let f = cx.input_idx(&format!("elemental/{gn}/filter"));
            filter
                .as_map_mut()
                .insert(ty, g, Array::from_vec(f, 1).unwrap())
                .unwrap();
            let v = cx.input(&format!("elemental/{gn}/field"));
            field
                .as_map_mut()
                .insert(ty, g, Array::from_vec(v, 2).unwrap())
                .unwrap();
            let nel = mesh.nb_element(ty, g);
            let nip = points.get(ty, g).unwrap().len() / nel;
            result
                .as_map_mut()
                .insert(ty, g, Array::try_zeros(nel * nip, 2).unwrap())
                .unwrap();
        }
        let interp = fe
            .init_elemental_field_interpolation(&mesh, &points, Some(&filter))
            .unwrap();
        for g in [NG, G] {
            fe.interpolate_elemental_field(&mesh, &interp, &field, &mut result, g, Some(&filter))
                .unwrap();
        }
        let nq = fe.nb_integration_points(ty, NG).unwrap();
        for (g, gn) in [(NG, "not_ghost"), (G, "ghost")] {
            let flt = filter.get(ty, g).unwrap().as_slice().to_vec();
            let nf = flt.len();
            let nip = interp.nb_interpolation_points(ty, g).unwrap();
            cx.check_matrices(
                &format!("elemental/{gn}/inv_matrices"),
                interp
                    .quad_points_inv_matrices()
                    .get(ty, g)
                    .unwrap()
                    .as_slice(),
                nq,
                nq,
                nf,
            );
            cx.check_matrices(
                &format!("elemental/{gn}/itp_matrices"),
                interp
                    .interpolation_points_matrices()
                    .get(ty, g)
                    .unwrap()
                    .as_slice(),
                nip,
                nq,
                nf,
            );
            let r = result.get(ty, g).unwrap();
            let mut res = Vec::with_capacity(nf * nip * 2);
            for &e in &flt {
                for i in 0..nip {
                    res.extend_from_slice(r.tuple(usize::try_from(e).unwrap() * nip + i));
                }
            }
            cx.check(&format!("elemental/{gn}/result"), &res);
        }
    }
    cx.finish(name);
}

macro_rules! extra_tests {
    ($($test:ident => $ty:ident),* $(,)?) => {$(
        #[test]
        fn $test() {
            extra(ElementType::$ty);
        }
    )*};
}

extra_tests! {
    engine_extra_segment_2_matches_oracle => Segment2,
    engine_extra_segment_3_matches_oracle => Segment3,
    engine_extra_triangle_3_matches_oracle => Triangle3,
    engine_extra_triangle_6_matches_oracle => Triangle6,
    engine_extra_quadrangle_4_matches_oracle => Quadrangle4,
    engine_extra_quadrangle_8_matches_oracle => Quadrangle8,
    engine_extra_tetrahedron_4_matches_oracle => Tetrahedron4,
    engine_extra_tetrahedron_10_matches_oracle => Tetrahedron10,
    engine_extra_pentahedron_6_matches_oracle => Pentahedron6,
    engine_extra_pentahedron_15_matches_oracle => Pentahedron15,
    engine_extra_hexahedron_8_matches_oracle => Hexahedron8,
    engine_extra_hexahedron_20_matches_oracle => Hexahedron20,
}
