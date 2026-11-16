// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the
// elemental matrices of src/fe_engine/fe_engine_template_tmpl.hh
// (computeBtDB + integrate) and fe_engine_template_tmpl_field.hh
// (assembleFieldMatrix) on threads (no upstream counterpart: D-123).

//! The fused `∫ BᵀDB` per element ([`FeEngine::compute_btdb_integrated`])
//! is bit for bit `compute_btdb` followed by `integrate`, and the
//! consistent-mass elemental matrices are the same at every thread count:
//! every element type with physical derivatives, a Voigt tangent (order 4)
//! and a `dim × dim` matrix (order 2), with and without a filter, at 1, 2,
//! 3, 4 and 8 threads with the finest and the automatic split.

mod common;

use akantu_core::exec::{Exec, ExecPolicy, Grain};
use akantu_core::{Array, Element, ElementKind, ElementType, GhostType, Idx, Real, Result};
use akantu_fe::{FeEngine, shape::ShapeFamily};

/// Executors of 1, 2, 3, 4 and 8 threads, finest and automatic split.
fn execs() -> Vec<Exec> {
    let mut v = Vec::new();
    for n in [1, 2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let p = ExecPolicy::serial().with_threads(n).unwrap();
            v.push(Exec::new(p.with_grain(grain)).unwrap());
        }
    }
    v
}

/// Finite values over a wide exponent range, with signed zeros and
/// subnormals (no NaN: two NaNs meeting in an addition may return either
/// payload in different code paths, D-046).
struct Values(u64);

impl Values {
    fn next(&mut self) -> Real {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let r = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        match r % 16 {
            0 => 0.0,
            1 => -0.0,
            2 => 5e-324,
            _ => {
                let m = (r >> 11) as Real / (1u64 << 53) as Real;
                let e = ((r >> 3) % 21) as i32 - 10;
                if r & 4 == 0 {
                    m * Real::powi(2.0, e)
                } else {
                    -m * Real::powi(2.0, e)
                }
            }
        }
    }
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn engine(mesh: &akantu_mesh::Mesh, exec: &Exec) -> FeEngine {
    let mut fe = FeEngine::with_precomputation(
        mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    fe.set_exec(exec.clone());
    fe
}

/// Types whose shape derivatives are physical (element dimension = mesh
/// dimension), i.e. those `compute_btdb` accepts.
fn types() -> Vec<ElementType> {
    common::test_element_types().collect()
}

#[test]
fn fused_btdb_integration_is_btdb_then_integrate_at_every_thread_count() {
    let execs = execs();
    for ty in types() {
        let mesh = common::fe_test_mesh(ty);
        let g = GhostType::NotGhost;
        let ne = mesh.nb_element(ty, g);
        let dim = ty.spatial_dimension();
        let serial = engine(&mesh, &Exec::serial());
        let nq = serial.nb_integration_points(ty, g).unwrap();
        let every_other: Vec<Idx> = (0..ne).rev().step_by(2).map(|e| e as Idx).collect();
        for filter in [None, Some(every_other.as_slice())] {
            let nb = filter.map_or(ne, <[Idx]>::len);
            for order in [4usize, 2] {
                let ts = if order == 4 {
                    akantu_core::voigt::voigt_table(dim).unwrap().size
                } else {
                    dim
                };
                let mut vals = Values(0x9e37_79b9 ^ (ne as u64) << 8 ^ order as u64);
                let d: Vec<Real> = (0..nb * nq * ts * ts).map(|_| vals.next()).collect();
                let d = Array::from_vec(d, ts * ts).unwrap();
                let n = if order == 4 {
                    dim * ty.nb_nodes_per_element()
                } else {
                    ty.nb_nodes_per_element()
                };
                let mut tmp = Array::new(n * n);
                let mut reference = Array::new(n * n);
                let r1 = serial
                    .compute_btdb(d.view(), &mut tmp, order, ty, g, filter)
                    .and_then(|()| {
                        serial.integrate(tmp.view(), &mut reference, n * n, ty, g, filter)
                    });
                for exec in &execs {
                    let fe = engine(&mesh, exec);
                    let mut out = Array::new(n * n);
                    let r2 = fe.compute_btdb_integrated(d.view(), &mut out, order, ty, g, filter);
                    let what = format!(
                        "{} order {order} filter {} threads {} {:?}",
                        ty.name(),
                        filter.is_some(),
                        exec.threads(),
                        exec.policy().grain()
                    );
                    match (&r1, &r2) {
                        (Ok(()), Ok(())) => {
                            assert_eq!(out.len(), reference.len(), "{what}");
                            assert_eq!(out.nb_component(), reference.nb_component(), "{what}");
                            assert_eq!(bits(out.as_slice()), bits(reference.as_slice()), "{what}");
                        }
                        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "{what}"),
                        _ => panic!("{what}: {r1:?} vs {r2:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn fused_btdb_integration_reports_the_errors_of_the_two_calls() {
    let ty = ElementType::Triangle3;
    let mesh = common::fe_test_mesh(ty);
    let g = GhostType::NotGhost;
    let fe = engine(&mesh, &Exec::serial());
    let nq = fe.nb_integration_points(ty, g).unwrap();
    let ne = mesh.nb_element(ty, g);
    let good = Array::from_vec(vec![1.0; ne * nq * 9], 9).unwrap();
    let short = Array::from_vec(vec![1.0; (ne - 1) * nq * 9], 9).unwrap();
    let bad_filter: [Idx; 1] = [ne as Idx];
    type Case<'a> = (&'a Array<Real>, usize, Option<&'a [Idx]>);
    let cases: [Case<'_>; 4] = [
        (&good, 3, None),
        (&short, 4, None),
        (&good, 4, Some(&bad_filter)),
        (&good, 2, None),
    ];
    for (d, order, filter) in cases {
        let mut tmp = Array::new(1);
        let mut reference = Array::new(1);
        let r1 = fe
            .compute_btdb(d.view(), &mut tmp, order, ty, g, filter)
            .and_then(|()| {
                let n2 = tmp.nb_component();
                fe.integrate(tmp.view(), &mut reference, n2, ty, g, filter)
            });
        let mut out = Array::new(1);
        let r2 = fe.compute_btdb_integrated(d.view(), &mut out, order, ty, g, filter);
        assert_eq!(
            r1.as_ref().map_err(ToString::to_string),
            r2.as_ref().map_err(ToString::to_string),
            "order {order} filter {filter:?}"
        );
    }
}

#[test]
fn consistent_mass_is_the_same_at_every_thread_count() {
    let execs = execs();
    for ty in types() {
        let mesh = common::fe_test_mesh(ty);
        let g = GhostType::NotGhost;
        let dim = ty.spatial_dimension();
        // a density that repeats over runs of elements (the reuse of point
        // matrices) and changes sign of zero now and then
        let rho = |m: &mut [Real], e: Element| -> Result<()> {
            for (i, v) in m.iter_mut().enumerate() {
                *v = match (e.index() as usize / 3 + i) % 5 {
                    0 => -0.0,
                    1 => 0.0,
                    k => 1.0 + k as Real / 7.0,
                };
            }
            Ok(())
        };
        let serial = engine(&mesh, &Exec::serial());
        let mut f = rho;
        let reference = serial
            .field_matrix_elemental(&mesh, &mut f, dim, ty, g)
            .unwrap();
        for exec in &execs {
            let fe = engine(&mesh, exec);
            let mut f = rho;
            let out = fe
                .field_matrix_elemental(&mesh, &mut f, dim, ty, g)
                .unwrap();
            assert_eq!(
                bits(out.as_slice()),
                bits(reference.as_slice()),
                "{} threads {} {:?}",
                ty.name(),
                exec.threads(),
                exec.policy().grain()
            );
        }
    }
}
