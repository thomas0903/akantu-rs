// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent
// checks of the diffusion model (no oracle): analytical steady states,
// convergence of the transient under refinement, energy balance of the
// explicit scheme, capacity sums, the stable-time-step formula, and the
// behaviours behind the upstream-defect switches UD-280 / UD-281 / UD-284.

mod common;

use akantu_core::enums::{AnalysisMethod, MatrixType, SparseSolverType};
use akantu_core::{Array, Element, ElementType, GhostType, Real};
use akantu_diffusion::{DiffusionModel, DiffusionUpstreamCompat, HeatTransferModel};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use common::*;

fn heat_dat(dim: usize, k: &str, rho: Real, c: Real, var: Real, t_ref: Real) -> String {
    let _ = dim;
    format!(
        "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = m\n    \
         density = {rho}\n    capacity = {c}\n    conductivity = {k}\n    \
         conductivity_variation = {var}\n    temperature_reference = {t_ref}\n  ]\n]\n"
    )
}

/// 1-D bar `[0, L]` of `n` segments with node coordinates `x(i)`.
fn bar(n: usize, x: impl Fn(usize) -> Real) -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for i in 0..=n {
            a.push_node(&[x(i)]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        for i in 0..n {
            c.push_tuple(&[i as akantu_core::Idx, (i + 1) as akantu_core::Idx]);
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

fn model(mesh: Mesh, dat: &str, method: AnalysisMethod) -> HeatTransferModel {
    let parser = Parser::parse_str(dat, "heat.dat").unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    let mut o = ModelOptions::new(method);
    if method != AnalysisMethod::ExplicitLumpedMass {
        o.solver_options.sparse_solver_type = Some(SparseSolverType::Eigen);
    }
    m.init_full(&o).unwrap();
    m
}

fn coords(m: &HeatTransferModel) -> Vec<Real> {
    m.physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

/* --------------------------------------------------------------------- */

/// Steady 1-D conduction with constant `k` on a graded mesh: the FE
/// solution is the exact linear profile (nodally exact), the end internal
/// flows are the exact conduction flux `±k ΔT / L` and sum to zero.
#[test]
fn steady_state_1d_linear_profile_is_exact() {
    let n = 13;
    let l = 2.5;
    let mesh = bar(n, |i| l * (i as Real / n as Real).powf(1.7));
    let mut m = model(
        mesh,
        &heat_dat(1, "[[3.5]]", 1.0, 1.0, 0.0, 0.0),
        AnalysisMethod::Static,
    );
    let (t0, t1) = (40.0, -12.0);
    m.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
    m.blocked_dofs_mut().unwrap().as_mut_slice()[n] = true;
    m.temperature_mut().unwrap().as_mut_slice()[0] = t0;
    m.temperature_mut().unwrap().as_mut_slice()[n] = t1;
    m.solve_step(None).unwrap();
    let x = coords(&m);
    for (i, t) in m.temperature().unwrap().as_slice().iter().enumerate() {
        let want = t0 + (t1 - t0) * x[i] / l;
        assert!((t - want).abs() <= 1e-12 * 40.0, "node {i}: {t} vs {want}");
    }
    let q_g = m.internal_heat_rate().unwrap();
    let q = q_g.as_slice();
    let flux = 3.5 * (t1 - t0) / l;
    assert!(
        (q[0] - flux).abs() <= 1e-12 * flux.abs(),
        "{} vs {flux}",
        q[0]
    );
    assert!(
        (q[n] + flux).abs() <= 1e-12 * flux.abs(),
        "{} vs {}",
        q[n],
        -flux
    );
    let s: Real = q.iter().sum();
    assert!(s.abs() <= 1e-12 * flux.abs(), "Σq = {s:e}");
}

/// Steady 1-D conduction with `k(T) = k0 + β (T − T_ref)`: exact profile by
/// the Kirchhoff transform `φ(T) = k0 s + β s²/2` (`s = T − T_ref`) linear in
/// `x`. With linear segments and their one-point rule the diffusivity is
/// `k(T_mid)` and `k(T_mid) (T_{i+1} − T_i) = φ(T_{i+1}) − φ(T_i)` exactly, so
/// the converged FE solution is **nodally exact** on any mesh (uniform and
/// graded checked; the conductivity matrix ignores `dk/dT`, Newton
/// converges linearly).
#[test]
fn steady_state_1d_temperature_dependent_is_nodally_exact() {
    let (k0, beta, t_ref, l) = (2.0, 0.05, 10.0, 1.0);
    let (t0, t1) = (100.0, 10.0);
    let phi = |t: Real| k0 * (t - t_ref) + 0.5 * beta * (t - t_ref) * (t - t_ref);
    let exact = |x: Real| {
        let p = phi(t0) + (phi(t1) - phi(t0)) * x / l;
        t_ref + (-k0 + (k0 * k0 + 2.0 * beta * p).sqrt()) / beta
    };
    for (n, graded) in [(8usize, false), (64, false), (17, true)] {
        let mesh = bar(n, |i| {
            let r = i as Real / n as Real;
            l * if graded { r * r } else { r }
        });
        let dat = heat_dat(1, &format!("[[{k0}]]"), 1.0, 1.0, beta, t_ref);
        let mut m = model(mesh, &dat, AnalysisMethod::Static);
        set_newton(&mut m, 200, 1e-13, "solution");
        m.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
        m.blocked_dofs_mut().unwrap().as_mut_slice()[n] = true;
        m.temperature_mut().unwrap().as_mut_slice()[0] = t0;
        m.temperature_mut().unwrap().as_mut_slice()[n] = t1;
        m.solve_step(None).unwrap();
        let x = coords(&m);
        for (i, t) in m.temperature().unwrap().as_slice().iter().enumerate() {
            let want = exact(x[i]);
            assert!(
                (t - want).abs() <= 1e-12 * t0,
                "n = {n}, graded = {graded}, node {i}: {t} vs {want}"
            );
        }
    }
}

/// Transient 1-D conduction `T_t = κ T_xx`, `T(0) = T(1) = 0`,
/// `T(x, 0) = sin(πx)`: exact `e^{−κπ²t} sin(πx)`. With `Δt ∝ h²` the
/// nodal error at a fixed time decreases quadratically, for the implicit
/// (backward Euler, consistent capacity) and the explicit (forward Euler,
/// lumped capacity) schemes.
#[test]
fn transient_1d_converges_quadratically() {
    let (k, rho, c) = (1.0, 1.0, 1.0);
    let kappa = k / (rho * c);
    let t_end = 0.05;
    for method in [
        AnalysisMethod::ImplicitDynamic,
        AnalysisMethod::ExplicitLumpedMass,
    ] {
        let mut errors = Vec::new();
        for n in [10usize, 20, 40] {
            let h = 1.0 / n as Real;
            let mesh = bar(n, |i| i as Real * h);
            let mut m = model(
                mesh,
                &heat_dat(1, &format!("[[{k}]]"), rho, c, 0.0, 0.0),
                method,
            );
            set_newton(&mut m, 10, 1e-12, "solution");
            // Δt = 0.2 h² / κ (explicit stability: h² / (2κ))
            let steps = (t_end / (0.2 * h * h / kappa)).round() as usize;
            let dt = t_end / steps as Real;
            m.set_time_step(dt, None).unwrap();
            let x = coords(&m);
            for (i, t) in m
                .temperature_mut()
                .unwrap()
                .as_mut_slice()
                .iter_mut()
                .enumerate()
            {
                *t = (std::f64::consts::PI * x[i]).sin();
            }
            m.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
            m.blocked_dofs_mut().unwrap().as_mut_slice()[n] = true;
            m.temperature_mut().unwrap().as_mut_slice()[0] = 0.0;
            m.temperature_mut().unwrap().as_mut_slice()[n] = 0.0;
            for _ in 0..steps {
                m.solve_step(None).unwrap();
            }
            let decay = (-kappa * std::f64::consts::PI.powi(2) * t_end).exp();
            let e = m
                .temperature()
                .unwrap()
                .as_slice()
                .iter()
                .enumerate()
                .map(|(i, t)| (t - decay * (std::f64::consts::PI * x[i]).sin()).abs())
                .fold(0.0, Real::max);
            errors.push(e);
        }
        for w in errors.windows(2) {
            let rate = (w[0] / w[1]).log2();
            assert!(rate > 1.8, "{method:?}: rate {rate} (errors {errors:?})");
        }
    }
}

/* --------------------------------------------------------------------- */

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

fn patch_model(ty: &str, method: AnalysisMethod) -> HeatTransferModel {
    let f = case(&format!("patch_static_{ty}"));
    let mesh = read_mesh(&mesh_path(&f), dim_of(&f));
    let dim = dim_of(&f);
    let k = match dim {
        1 => "[[2.0]]",
        2 => "[[2.0, 0.5], [0.5, 3.0]]",
        _ => "[[2.0, 0.5, 0.1], [0.5, 3.0, 0.2], [0.1, 0.2, 1.5]]",
    };
    model(mesh, &heat_dat(dim, k, 2.5, 3.0, 0.0, 0.0), method)
}

/// Volume of the mesh (`Σ ∫ 1` per element type of the model dimension).
fn volume(m: &HeatTransferModel) -> Real {
    let mesh = m.physics.base.mesh.read().unwrap();
    let fe = m.physics.base.fems.get("").unwrap();
    let mut v = 0.0;
    for ty in types_of(&mesh, m.spatial_dimension()) {
        let nq = fe.nb_integration_points(ty, GhostType::NotGhost).unwrap();
        let ones = vec![1.0; nq * mesh.nb_element(ty, GhostType::NotGhost)];
        v += fe
            .integrate_scalar(
                akantu_core::array::ArrayView::new(&ones, 1).unwrap(),
                ty,
                GhostType::NotGhost,
                None,
            )
            .unwrap();
    }
    v
}

/// `Σ_i M_i` (lumped) and `Σ_ij M_ij` (consistent) equal `∫ ρc` for every
/// element type.
#[test]
fn capacity_sums_equal_integral_of_rho_c() {
    for ty in TYPES {
        let mut m = patch_model(ty, AnalysisMethod::ExplicitLumpedMass);
        m.assemble_capacity_matrix_lumped().unwrap();
        let want = 2.5 * 3.0 * volume(&m);
        let lumped: Real = m.lumped_capacity().unwrap().as_slice().iter().sum();
        assert!(
            (lumped - want).abs() <= 1e-13 * want,
            "{ty}: Σ lumped {lumped} vs {want}"
        );
        m.assemble_capacity_matrix().unwrap();
        let mm = m.dof_manager().matrix("M").unwrap();
        let (r, c, v) = mm.coo();
        let sym = mm.matrix_type() == MatrixType::Symmetric;
        let total: Real = r
            .iter()
            .zip(c)
            .zip(v)
            .map(|((ri, ci), vi)| if sym && ri != ci { 2.0 * vi } else { *vi })
            .sum();
        assert!(
            (total - want).abs() <= 1e-13 * want,
            "{ty}: Σ consistent {total} vs {want}"
        );
    }
}

/// Explicit scheme, insulated body (nothing blocked), nodal heat source
/// `Q`: `Σ M_i Ṫ_i = Q` at every step (`Σ_i ∫ ∇N_i · D∇T = 0`, partition of
/// unity) and `Σ M_i T_i` grows by exactly `(N − 1) Δt Q` (heat balance to
/// round-off).
#[test]
fn explicit_energy_balance() {
    for ty in TYPES {
        let mut m = patch_model(ty, AnalysisMethod::ExplicitLumpedMass);
        // 0.2 dt_stable: stable for every type (UD-283)
        let dt = 0.2 * m.stable_time_step().unwrap();
        m.set_time_step(dt, None).unwrap();
        let x = coords(&m);
        let d = m.spatial_dimension();
        for (i, t) in m
            .temperature_mut()
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            let p = &x[i * d..(i + 1) * d];
            *t = 20.0 + 5.0 * p[0] * p[0] - 3.0 * p.iter().sum::<Real>();
        }
        let q = 0.7;
        m.external_heat_rate_mut().unwrap().as_mut_slice()[0] = q;
        m.assemble_capacity_matrix_lumped().unwrap();
        let cap = m.lumped_capacity().unwrap();
        let heat = |m: &HeatTransferModel| -> Real {
            m.temperature()
                .unwrap()
                .as_slice()
                .iter()
                .zip(cap.as_slice())
                .map(|(t, c)| t * c)
                .sum()
        };
        let h0 = heat(&m);
        let steps = 25;
        for _ in 0..steps {
            m.solve_step(None).unwrap();
            // Σ M_i Ṫ_i = Σ r_i = Q (Σ q_int = 0, partition of unity); a
            // sum of n mixed-sign terms is exact to n·ε·Σ|terms|
            let terms: Vec<Real> = m
                .temperature_rate()
                .unwrap()
                .as_slice()
                .iter()
                .zip(cap.as_slice())
                .map(|(r, c)| r * c)
                .collect();
            let p: Real = terms.iter().sum();
            let scale: Real = terms.iter().map(|t| t.abs()).sum::<Real>() + q;
            let bound = 4.0 * terms.len() as Real * Real::EPSILON * scale;
            assert!(
                (p - q).abs() <= bound,
                "{ty}: Σ M Ṫ = {p} instead of {q} (bound {bound:e})"
            );
        }
        let h1 = heat(&m);
        // forward Euler applies the rate of the previous step (the
        // predictor `T += Δt Ṫⁿ`; Ṫ⁰ = 0): N steps add (N − 1) Δt Q
        let added = (steps - 1) as Real * dt * q;
        assert!(
            ((h1 - h0) - added).abs() <= 1e-12 * h0.abs(),
            "{ty}: Σ M T grew by {} instead of {added}",
            h1 - h0
        );
    }
}

/// `getStableTimeStep()` = `2 h² / 4 · ρc / k_max` with `h` the smallest
/// inradius and `k_max` the largest eigenvalue of the conductivity.
#[test]
fn stable_time_step_formula() {
    for ty in ["triangle_3", "quadrangle_8", "tetrahedron_10"] {
        let mut m = patch_model(ty, AnalysisMethod::ExplicitLumpedMass);
        let mesh = m.physics.base.mesh.read().unwrap();
        let d = m.spatial_dimension();
        let mut h = Real::MAX;
        for t in types_of(&mesh, d) {
            let conn = mesh.connectivity(t, GhostType::NotGhost).unwrap();
            for e in 0..conn.len() {
                let xs: Vec<Real> = conn
                    .tuple(e)
                    .iter()
                    .flat_map(|&n| {
                        let n = n as usize;
                        mesh.nodes().as_slice()[n * d..(n + 1) * d].to_vec()
                    })
                    .collect();
                h = h.min(akantu_fe::engine::element_inradius_of(t, &xs));
            }
        }
        drop(mesh);
        let kmax = match d {
            2 => {
                // [[2, .5], [.5, 3]]: (5 + sqrt(1 + 1)) / 2
                (5.0 + 2.0_f64.sqrt()) / 2.0
            }
            _ => {
                let k = akantu_core::tensor::MatrixX::from_col_major(
                    3,
                    3,
                    vec![2.0, 0.5, 0.1, 0.5, 3.0, 0.2, 0.1, 0.2, 1.5],
                )
                .unwrap();
                akantu_linalg::dense::eigh(&k, akantu_linalg::dense::Storage::Dynamic)
                    .unwrap()
                    .into_iter()
                    .fold(0.0, Real::max)
            }
        };
        let want = 2.0 * h * h / 4.0 * 2.5 * 3.0 / kmax;
        let got = m.stable_time_step().unwrap();
        assert!((got - want).abs() <= 1e-13 * want, "{ty}: {got} vs {want}");
    }
}

/// UD-282 (reproduced): `conductivity_variation · (T − T_ref)` is added to
/// every entry of the conductivity, off-diagonal ones included.
#[test]
fn conductivity_variation_applies_to_every_entry() {
    let f = case("patch_static_triangle_3");
    let mesh = read_mesh(&mesh_path(&f), 2);
    let dat = heat_dat(2, "[[2.0, 0.5], [0.5, 3.0]]", 1.0, 1.0, 0.25, 4.0);
    let mut m = model(mesh, &dat, AnalysisMethod::ExplicitLumpedMass);
    m.temperature_mut().unwrap().as_mut_slice().fill(10.0);
    m.assemble_residual().unwrap();
    let s = 0.25 * (10.0 - 4.0);
    let diff = m
        .law_internal(
            0,
            "diffusivity",
            ElementType::Triangle3,
            GhostType::NotGhost,
        )
        .unwrap()
        .as_slice()
        .to_vec();
    for c in diff.as_chunks::<4>().0 {
        assert_eq!(c, &[2.0 + s, 0.5 + s, 0.5 + s, 3.0 + s]);
    }
}

/// A generic `DiffusionModel` refuses `heat_diffusion` (upstream: "works
/// only with model of type HeatTransferModel").
#[test]
fn generic_model_refuses_heat_diffusion() {
    let f = case("patch_static_triangle_3");
    let mesh = read_mesh(&mesh_path(&f), 2);
    let parser = Parser::parse_str(
        &heat_dat(2, "[[1, 0], [0, 1]]", 1.0, 1.0, 0.0, 0.0),
        "h.dat",
    )
    .unwrap();
    let mut m = DiffusionModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "diffusion",
        "diffusion",
        akantu_core::enums::ModelType::HeatTransferModel,
        parser,
    )
    .unwrap();
    let e = m
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap_err()
        .to_string();
    assert!(e.contains("works only with model of type"), "{e}");
}

/// UD-285: a conductivity smaller than `dim × dim` is refused (upstream's
/// fixed-size block reads out of bounds).
#[test]
fn ud285_small_conductivity_is_refused() {
    let f = case("patch_static_tetrahedron_4");
    let mesh = read_mesh(&mesh_path(&f), 3);
    let parser = Parser::parse_str(
        &heat_dat(3, "[[1, 0], [0, 1]]", 1.0, 1.0, 0.0, 0.0),
        "h.dat",
    )
    .unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    let e = m
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap_err()
        .to_string();
    assert!(e.contains("UD-285"), "{e}");
}

/// UD-281: upstream gates the lumped capacity by the consistent flag — an
/// explicit solve after a consistent assembly divides by a lumped capacity
/// that was never assembled (compat switch: non-finite rates); by default
/// the lumped capacity has its own flag.
#[test]
fn ud281_lumped_capacity_gating() {
    for compat in [false, true] {
        let mut m = patch_model("quadrangle_4", AnalysisMethod::ExplicitLumpedMass);
        m.physics.upstream_compat = DiffusionUpstreamCompat {
            lumped_capacity_gated_by_consistent_flag: compat,
            ..DiffusionUpstreamCompat::default()
        };
        let dt = m.stable_time_step().unwrap();
        m.set_time_step(0.5 * dt, None).unwrap();
        let x = coords(&m);
        for (i, t) in m
            .temperature_mut()
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            *t = 1.0 + x[2 * i];
        }
        m.assemble_capacity_matrix().unwrap();
        let r = m.solve_step(None);
        let finite = r.is_ok()
            && m.temperature_rate()
                .unwrap()
                .as_slice()
                .iter()
                .all(|v| v.is_finite());
        if compat {
            // upstream: the lumped capacity is never assembled after the
            // consistent one; the lumped solve divides by a zero capacity
            assert!(!finite, "upstream mode must not produce a finite rate");
        } else {
            assert!(finite, "default mode: {r:?}");
            assert!(
                m.lumped_capacity()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .all(|c| *c > 0.0)
            );
        }
    }
}

/// UD-280: with two laws the law-local and mesh numberings differ; the
/// default per-element energy uses the mesh element, the compat switch
/// upstream's law-local index.
#[test]
fn ud280_element_energy_uses_the_mesh_element() {
    let f = case("two_laws_mixed");
    let build = |compat: bool| {
        let mesh = read_mesh(&mesh_path(&f), 2);
        let mut m = model_on(&f, mesh, "heat_transfer");
        m.physics.upstream_compat = DiffusionUpstreamCompat {
            element_energy_uses_law_local_index: compat,
            ..DiffusionUpstreamCompat::default()
        };
        m.set_law_selector(Box::new(
            akantu_model::constitutive_law::ElementDataSelector::new("physical_names"),
        ));
        m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap();
        let x = coords(&m);
        for (i, t) in m
            .temperature_mut()
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            *t = 1.0 + x[2 * i];
        }
        m
    };
    let mut fixed = build(false);
    let mut upstream = build(true);
    // the quadrangles are elements 0, 1 of their type and law-local 0, 1;
    // the triangles are mesh elements 0, 1 of `_triangle_3`: law "tris" is
    // law 1 with local indices 0, 1 — equal. The two laws each own one
    // type, so pick a triangle and compare with a hand integral instead.
    let e_tri = Element::new(ElementType::Triangle3, 1, GhostType::NotGhost);
    let a = fixed.energy_of_element("thermal", e_tri).unwrap();
    let b = upstream.energy_of_element("thermal", e_tri).unwrap();
    // law-local and mesh indices coincide here: both modes agree and equal
    // c ρ ∫ T over the triangle (T linear: area · mean nodal T)
    assert_eq!(a, b);
    let mesh = fixed.physics.base.mesh.read().unwrap();
    let conn = mesh
        .connectivity(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap();
    let nodes = conn.tuple(1).to_vec();
    let p: Vec<[Real; 2]> = nodes
        .iter()
        .map(|&n| {
            let n = n as usize;
            [
                mesh.nodes().as_slice()[2 * n],
                mesh.nodes().as_slice()[2 * n + 1],
            ]
        })
        .collect();
    let area = 0.5
        * ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]))
            .abs();
    let mean_t: Real = nodes
        .iter()
        .map(|&n| 1.0 + mesh.nodes().as_slice()[2 * n as usize])
        .sum::<Real>()
        / 3.0;
    let want = 5.0 * 1.0 * area * mean_t;
    assert!((a - want).abs() <= 1e-13 * want, "{a} vs {want}");
}

/// The gradient cache (upstream release counters, UD-284 family): after a
/// residual assembly at the current release, a temperature changed by the
/// user is not seen until the next predictor / corrector.
#[test]
fn gradient_cache_follows_the_release() {
    let mut m = patch_model("triangle_3", AnalysisMethod::ExplicitLumpedMass);
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.5 * dt, None).unwrap();
    m.solve_step(None).unwrap();
    assert_eq!(m.temperature_release(), 1);
    m.assemble_residual().unwrap();
    let before = m.internal_heat_rate().unwrap().as_slice().to_vec();
    let x = coords(&m);
    for (i, t) in m
        .temperature_mut()
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *t += 3.0 * x[2 * i] - x[2 * i + 1];
    }
    m.assemble_residual().unwrap();
    assert_eq!(
        m.internal_heat_rate().unwrap().as_slice(),
        before.as_slice(),
        "cached gradient (release unchanged)"
    );
    m.solve_step(None).unwrap();
    assert_ne!(
        m.internal_heat_rate().unwrap().as_slice(),
        before.as_slice()
    );
}

/// Dumps: the heat-transfer aliases, law internals, `capacity_lumped`
/// refused, `increment` reported as not allocated at dump time.
#[test]
fn dumps_with_aliases() {
    let dir = std::env::temp_dir().join(format!("akantu-diffusion-dump-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut m = patch_model("triangle_6", AnalysisMethod::ExplicitLumpedMass);
    m.set_directory(&dir).unwrap();
    m.set_base_name("heat").unwrap();
    for f in [
        "temperature",
        "temperature_rate",
        "internal_heat_rate",
        "external_heat_rate",
        "blocked_dofs",
        "conductivity",
        "∇u",
        "partitions",
    ] {
        m.add_dump_field(f).unwrap();
    }
    assert!(m.add_dump_field("capacity_lumped").is_err());
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.5 * dt, None).unwrap();
    m.solve_step(None).unwrap();
    m.dump(None, None).unwrap();
    let mut found = false;
    for e in walk(&dir) {
        if e.extension().is_some_and(|x| x == "vtu") {
            let s = std::fs::read_to_string(&e).unwrap();
            for name in [
                "temperature",
                "temperature_rate",
                "internal_heat_rate",
                "conductivity",
                "∇u",
            ] {
                assert!(
                    s.contains(&format!("Name=\"{name}\"")),
                    "{name} missing in {}",
                    e.display()
                );
            }
            found = true;
        }
    }
    assert!(found, "no VTU written");
    m.add_dump_field("increment").unwrap();
    let e = m.dump(None, None).unwrap_err().to_string();
    assert!(e.contains("increment"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Parameters as upstream parses them: the top-left `dim × dim` block of a
/// larger conductivity is kept; `set` runs `updateInternalParameters`.
#[test]
fn parameters_parse_and_update() {
    let mut m = patch_model("quadrangle_4", AnalysisMethod::ExplicitLumpedMass);
    let law = m.law_mut(0).unwrap();
    assert_eq!(law.name(), "m");
    assert_eq!(law.law_type(), "heat_diffusion");
    let k = akantu_core::tensor::MatrixX::from_col_major(3, 3, (1..=9).map(Real::from).collect())
        .unwrap();
    law.set_param("conductivity", akantu_parser::ParamValue::MatrixReal(k))
        .unwrap();
    match law.get_param("conductivity").unwrap() {
        akantu_parser::ParamValue::MatrixReal(c) => {
            assert_eq!((c.rows(), c.cols()), (2, 2));
            assert_eq!(c.as_slice(), &[1.0, 2.0, 4.0, 5.0]);
        }
        other => panic!("{other:?}"),
    }
    // diffusivity re-set on every quadrature point
    let d = m
        .law_internal(
            0,
            "diffusivity",
            ElementType::Quadrangle4,
            GhostType::NotGhost,
        )
        .unwrap()
        .as_slice()
        .to_vec();
    for c in d.as_chunks::<4>().0 {
        assert_eq!(c, &[1.0, 2.0, 4.0, 5.0]);
    }
    assert!(m.law(0).unwrap().get_param("bogus").is_err());
    let _ = Array::<Real>::new(1);
}

/// UD-287: upstream's generalized-trapezoidal predictor keeps the rate, so
/// backward Euler freezes after its first step (`(M/Δt + K)(Tⁿ⁺¹ − Tⁿ) = 0`
/// under constant loads); the corrected default keeps decaying.
#[test]
fn ud287_backward_euler_freezes_only_in_upstream_mode() {
    for keep_rate in [true, false] {
        let n = 10;
        let mesh = bar(n, |i| i as Real * 0.1);
        let mut m = model(
            mesh,
            &heat_dat(1, "[[1.0]]", 1.0, 1.0, 0.0, 0.0),
            AnalysisMethod::ImplicitDynamic,
        );
        m.dof_manager_mut()
            .set_upstream_compat(akantu_solver::UpstreamCompat {
                generalized_trapezoidal_keeps_rate_in_predictor: keep_rate,
                ..akantu_solver::UpstreamCompat::default()
            });
        m.set_time_step(0.002, None).unwrap();
        let x = coords(&m);
        for (i, t) in m
            .temperature_mut()
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            *t = (std::f64::consts::PI * x[i]).sin();
        }
        for i in [0, n] {
            m.blocked_dofs_mut().unwrap().as_mut_slice()[i] = true;
            m.temperature_mut().unwrap().as_mut_slice()[i] = 0.0;
        }
        let mut mid = Vec::new();
        for _ in 0..4 {
            m.solve_step(None).unwrap();
            mid.push(m.temperature().unwrap().as_slice()[5]);
        }
        if keep_rate {
            assert!(
                mid.windows(2).all(|w| w[0] == w[1]),
                "upstream freezes: {mid:?}"
            );
        } else {
            assert!(
                mid.windows(2).all(|w| w[1] < w[0]),
                "corrected decays: {mid:?}"
            );
            // textbook BE per step on the discrete sine mode decays by a
            // constant factor
            let r1 = mid[1] / mid[0];
            let r2 = mid[3] / mid[2];
            assert!((r1 - r2).abs() <= 1e-12, "{r1} vs {r2}");
        }
    }
}

/// UD-283 (reproduced, category C): upstream's stable time step
/// `2 h² / 4 · ρc / k_max` (inradius `h`) is a heuristic, not a bound of the
/// lumped forward-Euler scheme: on an insulated patch the serendipity
/// elements (`_quadrangle_8`, `_hexahedron_20`, diagonally scaled lumped
/// capacity) diverge at half of it, every element type is stable at a
/// fifth of it.
#[test]
fn ud283_stable_time_step_is_a_heuristic() {
    let growth = |ty: &str, factor: Real| -> Real {
        let mut m = patch_model(ty, AnalysisMethod::ExplicitLumpedMass);
        let dt = m.stable_time_step().unwrap();
        m.set_time_step(factor * dt, None).unwrap();
        let x = coords(&m);
        let d = m.spatial_dimension();
        for (i, t) in m
            .temperature_mut()
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            let p = &x[i * d..(i + 1) * d];
            *t = 20.0 + 5.0 * p[0] * p[0] - 3.0 * p.iter().sum::<Real>();
        }
        let mut r = Vec::new();
        for _ in 0..60 {
            m.solve_step(None).unwrap();
            r.push(
                m.temperature_rate()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .fold(0.0f64, |a, b| a.max(b.abs())),
            );
        }
        r[59] / r[1]
    };
    for ty in ["quadrangle_8", "hexahedron_20"] {
        assert!(growth(ty, 0.5) > 1e6, "{ty} diverges at 0.5 dt_stable");
    }
    for ty in TYPES {
        assert!(growth(ty, 0.2) < 1.0, "{ty} is stable at 0.2 dt_stable");
    }
}

/// UD-287: temporal order of the generalized trapezoidal family
/// in isolation. On a uniform 1-D mesh with fixed ends the sampled mode
/// `v_i = sin(πx_i)` is an eigenvector of `K` and of the consistent `M`, so
/// the semi-discrete solution is exactly `exp(−λt) v` with
/// `λ = κ (6/h²)(1 − cos πh)/(2 + cos πh)`; comparing with it removes the
/// spatial error. Corrected predictor: backward Euler is of order 1 from any
/// initial rate; the trapezoidal rule 1 (Crank–Nicolson) is of order 2 from a
/// consistent initial rate (`M u̇⁰ = −K u⁰`: set analytically, `−λ v`, or
/// with [`DiffusionModel::set_consistent_initial_rate`]) and of order 1 from
/// the default `u̇⁰ = 0` (its first step is half a backward-Euler step).
/// Upstream predictor (rate kept): neither converges
/// (backward Euler freezes after its first step).
#[test]
fn ud287_generalized_trapezoidal_temporal_orders() {
    use akantu_core::enums::IntegrationSchemeType as T;
    let n = 10;
    let h = 1.0 / n as Real;
    let theta = std::f64::consts::PI * h;
    let lambda = 6.0 / (h * h) * (1.0 - theta.cos()) / (2.0 + theta.cos());
    let t_end = 0.1;
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Init {
        /// `u̇⁰ = 0` (what `init_full` leaves, every upstream run)
        Rest,
        /// `u̇⁰ = −λ v` (exact consistent rate)
        Analytic,
        /// `set_consistent_initial_rate()`
        Api,
    }
    let run = |scheme: T, keep_rate: bool, init: Init, steps: usize| -> Real {
        let mut m = model(
            bar(n, |i| i as Real * h),
            &heat_dat(1, "[[1.0]]", 1.0, 1.0, 0.0, 0.0),
            AnalysisMethod::ImplicitDynamic,
        );
        // a solver of its own carrying the scheme under test (a solver's
        // scheme cannot be replaced, as upstream)
        let options = akantu_solver::ModelSolverOptions {
            time_step_solver_type: Some(akantu_core::enums::TimeStepSolverType::Dynamic),
            non_linear_solver_type: Some(akantu_core::enums::NonLinearSolverType::NewtonRaphson),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..akantu_solver::ModelSolverOptions::default()
        };
        m.get_new_solver("scheme_under_test", &options).unwrap();
        m.set_integration_scheme(
            "scheme_under_test",
            "temperature",
            scheme,
            akantu_solver::SolutionType::TEMPERATURE,
        )
        .unwrap();
        m.solver.set_default_solver("scheme_under_test").unwrap();
        set_newton(&mut m, 10, 1e-13, "residual");
        m.dof_manager_mut()
            .set_upstream_compat(akantu_solver::UpstreamCompat {
                generalized_trapezoidal_keeps_rate_in_predictor: keep_rate,
                ..akantu_solver::UpstreamCompat::default()
            });
        m.set_time_step(t_end / steps as Real, None).unwrap();
        let x = coords(&m);
        let v: Vec<Real> = x
            .iter()
            .map(|&xi| (std::f64::consts::PI * xi).sin())
            .collect();
        m.temperature_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&v);
        if init == Init::Analytic {
            // consistent initial rate M u̇ = −K u
            for (r, vi) in m
                .diffusion_rate_mut()
                .unwrap()
                .as_mut_slice()
                .iter_mut()
                .zip(&v)
            {
                *r = -lambda * vi;
            }
        }
        for i in [0, n] {
            m.blocked_dofs_mut().unwrap().as_mut_slice()[i] = true;
            m.temperature_mut().unwrap().as_mut_slice()[i] = 0.0;
            m.diffusion_rate_mut().unwrap().as_mut_slice()[i] = 0.0;
        }
        if init == Init::Api {
            m.set_consistent_initial_rate().unwrap();
            let rate_g = m.diffusion_rate().unwrap();
            for (i, (r, vi)) in rate_g.as_slice().iter().zip(&v).enumerate() {
                assert!(
                    (r + lambda * vi).abs() <= 1e-12 * lambda,
                    "consistent rate at node {i}: {r} vs {}",
                    -lambda * vi
                );
            }
        }
        for _ in 0..steps {
            m.solve_step(None).unwrap();
        }
        let decay = (-lambda * t_end).exp();
        m.temperature()
            .unwrap()
            .as_slice()
            .iter()
            .zip(&v)
            .map(|(t, vi)| (t - decay * vi).abs())
            .fold(0.0, Real::max)
    };
    let steps = [10usize, 20, 40, 80];
    for (scheme, init, order) in [
        (T::BackwardEuler, Init::Rest, 1.0),
        (T::BackwardEuler, Init::Analytic, 1.0),
        (T::TrapezoidalRule1, Init::Rest, 1.0),
        (T::TrapezoidalRule1, Init::Analytic, 2.0),
        (T::TrapezoidalRule1, Init::Api, 2.0),
    ] {
        let errs: Vec<Real> = steps.iter().map(|&s| run(scheme, false, init, s)).collect();
        let rates: Vec<Real> = errs.windows(2).map(|w| (w[0] / w[1]).log2()).collect();
        println!("{scheme:?} from {init:?}: corrected errors {errs:?} rates {rates:.3?}");
        for r in &rates {
            assert!(
                (r - order).abs() < 0.1,
                "{scheme:?} from {init:?}: observed order {r}, expected {order} (errors {errs:?})"
            );
        }
        if init != Init::Analytic {
            continue;
        }
        let upstream: Vec<Real> = steps.iter().map(|&s| run(scheme, true, init, s)).collect();
        println!("{scheme:?} from {init:?}: upstream errors {upstream:?}");
        // upstream: no convergence, the error at the finest step stays
        // orders of magnitude above the corrected one
        let last = steps.len() - 1;
        assert!(
            upstream[last] > 100.0 * errs[last] && upstream[last] > 0.5 * upstream[0],
            "{scheme:?}: upstream errors {upstream:?} vs corrected {errs:?}"
        );
    }
}

/// `setTimeStep` reaches only the model's own dumper (upstream
/// `diffusion_model.cc:270-274`: `mesh.getDumper(id).setTimeStep(dt)`): a
/// second dumper keeps no time step, so it writes no time description
/// (`.pvd`), while the model's dumper does.
#[test]
fn set_time_step_reaches_only_the_model_dumper() {
    let dir = std::env::temp_dir().join(format!("akantu-diffusion-dump-ts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut m = patch_model("triangle_3", AnalysisMethod::ExplicitLumpedMass);
    m.set_directory(&dir).unwrap();
    m.set_base_name("heat").unwrap();
    m.add_dump_field("temperature").unwrap();
    {
        let d = &mut m.dumpers.dumpable;
        d.register_paraview("other", Some("other"), false);
        d.add_dump_mesh_to_dumper(
            "other",
            akantu_io::dumper::MeshSelection::new(
                Some(2),
                GhostType::NotGhost,
                akantu_core::ElementKind::Regular,
            ),
        )
        .unwrap();
        d.dumper_mut("other").unwrap().set_directory(&dir).unwrap();
    }
    m.add_dump_field_to_dumper("other", "temperature").unwrap();
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.5 * dt, None).unwrap();
    m.solve_step(None).unwrap();
    m.dump(None, None).unwrap();
    m.dump_named("other", None, None).unwrap();
    let pvds: Vec<String> = walk(&dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "pvd"))
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        pvds.iter().any(|n| n.starts_with("heat")),
        "the model dumper has a time step: {pvds:?}"
    );
    assert!(
        !pvds.iter().any(|n| n.starts_with("other")),
        "the other dumper got no time step: {pvds:?}"
    );
}
