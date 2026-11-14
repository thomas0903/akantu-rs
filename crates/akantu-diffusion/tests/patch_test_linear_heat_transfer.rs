// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/patch_tests/
// patch_test_linear_heat_transfer_{static,implicit,explicit}.cc,
// patch_test_linear_heat_transfer_fixture.hh, patch_test_linear_fixture.hh,
// test_patch_linear_heat_transfer_{static,implicit,explicit}.py,
// patch_test_linear_heat_transfer_fixture.py.

//! The upstream heat-transfer patch tests (TEST-055…057 and the Python
//! variants TEST-061…063; never built upstream, UD-014 / UD-289), typed
//! over every regular element type, with the upstream assertions and
//! tolerances (`dofs_tolerance = 1e-10` absolute on `T`,
//! `gradient_tolerance = 1e-8` and `result_tolerance = 1e-9` relative
//! infinity norms on `∇T` and `D∇T = C·∇T` at every quadrature point). The
//! model uses the default solver options (as upstream's `initModel` with
//! the default sparse solver). Meshes and input file: the upstream files
//! stored with `fixtures/model/diffusion/patch_static_<type>`.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{GhostType, Real};
use akantu_diffusion::HeatTransferModel;
use akantu_model::ModelOptions;
use common::*;

const ALPHA: [Real; 4] = [0.01, 0.02, 0.03, 0.04];
const DOFS_TOLERANCE: Real = 1e-10;
const GRADIENT_TOLERANCE: Real = 1e-8;
const RESULT_TOLERANCE: Real = 1e-9;

const TYPES: [&str; 12] = [
    "segment_2",
    "segment_3",
    "triangle_3",
    "triangle_6",
    "quadrangle_4",
    "quadrangle_8",
    "tetrahedron_4",
    "tetrahedron_10",
    "hexahedron_8",
    "hexahedron_20",
    "pentahedron_6",
    "pentahedron_15",
];

fn set_linear_dof(x: &[Real]) -> Real {
    let mut t = ALPHA[0];
    for (j, xj) in x.iter().enumerate() {
        t += ALPHA[j + 1] * xj;
    }
    t
}

fn inf_norm(v: &[Real]) -> Real {
    v.iter().fold(0.0, |m, x| m.max(x.abs()))
}

/// `TestPatchTestHTMLinear::initModel`: parse, `initFull(method)`,
/// `applyBC` (every element-group node blocked at the affine field), the
/// parent's Newton–Raphson settings (3 iterations, 1e-6, solution), and for
/// dynamic analyses `setTimeStep(0.8 dt)` then `setTimeStep(0.5 dt)`.
fn init_model(ty: &str, method: AnalysisMethod) -> HeatTransferModel {
    let f = case(&format!("patch_static_{ty}"));
    let mesh = patch_mesh(&f);
    let boundary = group_nodes(&mesh);
    let mut model = model_on(&f, mesh, &format!("_{ty}"));
    model.init_full(&ModelOptions::new(method)).unwrap();
    let x = model.physics.base.mesh.read().unwrap().nodes().clone();
    let d = x.nb_component();
    for &n in &boundary {
        model.blocked_dofs_mut().unwrap().as_mut_slice()[n] = true;
        model.temperature_mut().unwrap().as_mut_slice()[n] =
            set_linear_dof(&x.as_slice()[n * d..(n + 1) * d]);
    }
    set_newton(&mut model, 3, 1e-6, "solution");
    if method != AnalysisMethod::Static {
        let dt = model.stable_time_step().unwrap();
        model.set_time_step(0.8 * dt, None).unwrap();
        let dt = model.stable_time_step().unwrap();
        model.set_time_step(0.5 * dt, None).unwrap();
    }
    model
}

/// `TestPatchTestHTMLinear::checkAll`: `checkDOFs(T)`, `checkGradient(∇T)`,
/// `checkResults(C·∇Tᵀ, D∇T)`.
fn check_all(ty: &str, model: &HeatTransferModel) {
    let x = model.physics.base.mesh.read().unwrap().nodes().clone();
    let d = x.nb_component();
    let t_g = model.temperature().unwrap();
    let t = t_g.as_slice();
    for (n, tn) in t.iter().enumerate() {
        let want = set_linear_dof(&x.as_slice()[n * d..(n + 1) * d]);
        assert!(
            (tn - want).abs() <= DOFS_TOLERANCE,
            "{ty}: checkDOFs node {n}: {tn} vs {want}"
        );
    }
    let law = model.law(0).unwrap();
    let c = match law.get_param("conductivity").unwrap() {
        akantu_parser::ParamValue::MatrixReal(m) => m,
        other => panic!("{other:?}"),
    };
    let pgrad: Vec<Real> = (0..d).map(|j| ALPHA[j + 1]).collect();
    // C * grad_T.transpose(): dim × 1
    let presult: Vec<Real> = (0..d)
        .map(|i| (0..d).map(|k| c.at(i, k) * pgrad[k]).sum())
        .collect();
    let ety = element_type(&format!("_{ty}"));
    let grad_g = law
        .base()
        .internal("∇u")
        .unwrap()
        .get(ety, GhostType::NotGhost)
        .unwrap();
    let grad = grad_g.as_slice();
    for (q, g) in grad.chunks_exact(d).enumerate() {
        let diff: Vec<Real> = g.iter().zip(&pgrad).map(|(a, b)| a - b).collect();
        let err = inf_norm(&diff) / inf_norm(g);
        assert!(
            err <= GRADIENT_TOLERANCE,
            "{ty}: checkGradient qp {q}: {err:e}"
        );
    }
    let flux_g = law
        .base()
        .internal("D∇u")
        .unwrap()
        .get(ety, GhostType::NotGhost)
        .unwrap();
    let flux = flux_g.as_slice();
    for (q, r) in flux.chunks_exact(d).enumerate() {
        let diff: Vec<Real> = r.iter().zip(&presult).map(|(a, b)| a - b).collect();
        let err = inf_norm(&diff) / inf_norm(&presult);
        assert!(
            err <= RESULT_TOLERANCE,
            "{ty}: checkResults qp {q}: {err:e}"
        );
    }
}

/// `TYPED_TEST(TestPatchTestHTMLinear, Static)`.
#[test]
fn test_patch_test_htm_linear_static() {
    for ty in TYPES {
        let mut model = init_model(ty, AnalysisMethod::Static);
        set_newton(&mut model, 2, 2e-4, "residual");
        model.solve_step(None).unwrap();
        check_all(ty, &model);
    }
}

/// Shared body of `TYPED_TEST(TestPatchTestHTMLinear, Implicit / Explicit)`:
/// every node at the affine field (the static solution), 100 steps.
fn dynamic(method: AnalysisMethod) {
    for ty in TYPES {
        let mut model = init_model(ty, method);
        let x = model.physics.base.mesh.read().unwrap().nodes().clone();
        let d = x.nb_component();
        for n in 0..x.len() {
            model.temperature_mut().unwrap().as_mut_slice()[n] =
                set_linear_dof(&x.as_slice()[n * d..(n + 1) * d]);
        }
        for _ in 0..100 {
            model.solve_step(None).unwrap();
        }
        check_all(ty, &model);
    }
}

/// `TYPED_TEST(TestPatchTestHTMLinear, Implicit)`.
#[test]
fn test_patch_test_htm_linear_implicit() {
    dynamic(AnalysisMethod::ImplicitDynamic);
}

/// `TYPED_TEST(TestPatchTestHTMLinear, Explicit)`.
#[test]
fn test_patch_test_htm_linear_explicit() {
    dynamic(AnalysisMethod::ExplicitLumpedMass);
}
