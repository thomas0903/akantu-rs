// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// {test_solid_mechanics_model_material_eigenstrain.cc, cube_3d_tet_4.msh,
// material_elastic_plane_strain.dat}.
//! Port of upstream `test_solid_mechanics_model_material_eigenstrain`
//! (TEST-106) with its own pass criterion: the committed upstream mesh
//! `cube_3d_tet_4.msh` (kept with the oracle fixture
//! `fixtures/model/smm/eigenstrain_tetrahedron_4/inputs`, SHA-256 equal to
//! upstream's), steel (`E = 2.1e11`, `ν = 0.3`), `initFull(_static)`,
//! Newton–Raphson `threshold = 2e-4`, `max_iterations = 2`, `_residual`;
//! `buildFacets` + `createBoundaryGroupFromGeometry`, every node of every
//! element group blocked at `u_i = α_i0 + Σ_j α_i(j+1) X_j` (upstream's
//! `alpha`), `eigen_grad_u = 0.1` (all nine entries) at every quadrature
//! point, one `solveStep()`. Every quadrature point must carry
//! `σ = λ tr(ε − ε⁰) I + 2μ (ε − ε⁰)` with `ε = sym(α_{·,1..3})` and
//! `ε⁰ = 0.1`: `‖σ − σ_ref‖∞ / ‖σ‖∞ ≤ 1e-13` (upstream `EXIT_FAILURE`
//! otherwise; `test_solid_mechanics_model_material_eigenstrain.cc`).
//!
//! The upstream run (oracle, bitwise) is replayed by
//! `model_smm_fixtures.rs::eigenstrain_bitwise`; this file checks the
//! upstream criterion on the port's own run, with the port's own
//! `buildFacets` / boundary groups (built before the model: mesh-only
//! operations; upstream builds them after `initFull`).

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::clusters::create_boundary_group_from_geometry;
use akantu_mesh::utils::build_facets;
use akantu_validation::fixtures_root;
use model_common::{init, model_with, read_mesh, set_newton};

/// `material_elastic_plane_strain.dat` (verbatim).
const MATERIAL: &str = "permissive_parser = true\n\
\n\
material elastic [\n\
\t name = steel\n\
\t rho = 7800   # density\n\
\t E   = 2.1e11 # young's modulus\n\
\t nu  = 0.3    # poisson's ratio\n\
         Plane_Stress = 0 # plane strain\n\
]\n";

/// Upstream `alpha` (3 × 4, row-major).
const ALPHA: [[Real; 4]; 3] = [
    [0.01, 0.02, 0.03, 0.04],
    [0.05, 0.06, 0.07, 0.08],
    [0.09, 0.10, 0.11, 0.12],
];

/// Upstream `prescribed_stress<_tetrahedron_4>(0.1)` (row-major 3 × 3).
fn prescribed_stress(eigen: Real) -> [[Real; 3]; 3] {
    let (nu, e) = (0.3, 2.1e11);
    let mut strain = [[0.; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            strain[i][j] = (ALPHA[i][j + 1] + ALPHA[j][i + 1]) / 2. - eigen;
        }
    }
    let trace = strain[0][0] + strain[1][1] + strain[2][2];
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let mut stress = [[0.; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            stress[i][j] = if i == j { trace * lambda } else { 0. } + 2. * mu * strain[i][j];
        }
    }
    stress
}

#[test]
fn test_solid_mechanics_model_material_eigenstrain() {
    const DIM: usize = 3;
    let path = fixtures_root().join("model/smm/eigenstrain_tetrahedron_4/inputs/cube_3d_tet_4.msh");
    let mut mesh = read_mesh(&path, DIM);
    let _ = build_facets(&mut mesh).unwrap();
    let nb_clusters = create_boundary_group_from_geometry(&mut mesh).unwrap();
    assert!(nb_clusters >= 1);
    let x = mesh.nodes().as_slice().to_vec();
    let group_nodes: Vec<usize> = mesh
        .groups()
        .element_groups()
        .flat_map(|g| g.nodes().iter().map(|&n| n as usize).collect::<Vec<_>>())
        .collect();
    assert!(!group_nodes.is_empty());
    let mut model = model_with(mesh, MATERIAL);
    init(&mut model, AnalysisMethod::Static);
    set_newton(&mut model, 2, 2e-4, "residual");
    for &n in &group_nodes {
        for (i, alpha_i) in ALPHA.iter().enumerate() {
            let mut u = alpha_i[0];
            for (j, xj) in x[n * DIM..(n + 1) * DIM].iter().enumerate() {
                u += alpha_i[j + 1] * xj;
            }
            model.displacement_mut().unwrap().as_mut_slice()[n * DIM + i] = u;
            model.blocked_dofs_mut().unwrap().as_mut_slice()[n * DIM + i] = true;
        }
    }
    let ty = ElementType::Tetrahedron4;
    {
        let law = model.material_mut(0).unwrap();
        let mut eig = law
            .base_mut()
            .eigen_grad_u
            .get_mut(ty, GhostType::NotGhost)
            .unwrap();
        eig.as_mut_slice().fill(0.1);
    }
    model.solve_step(None).unwrap();
    let nls = model.solver.non_linear_solver(None).unwrap();
    eprintln!("Converged in {} ({:e})", nls.nb_iterations(), nls.error());

    let presc = prescribed_stress(0.1);
    let stress = model
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    assert!(!stress.is_empty());
    let stress_tolerance = 1e-13;
    let mut worst = 0.0_f64;
    for (q, s) in stress.as_chunks::<{ DIM * DIM }>().0.iter().enumerate() {
        // column-major (r, c) at r + 3c
        let mut diff_inf = 0.0_f64;
        let mut s_inf = 0.0_f64;
        for r in 0..DIM {
            for c in 0..DIM {
                let v = s[r + DIM * c];
                diff_inf = diff_inf.max((v - presc[r][c]).abs());
                s_inf = s_inf.max(v.abs());
            }
        }
        let err = diff_inf / s_inf;
        worst = worst.max(err);
        assert!(
            err <= stress_tolerance,
            "q {q}: stress error: {err} > {stress_tolerance}\nstress: {s:?}\nprescribed stress: {presc:?}"
        );
    }
    eprintln!(
        "eigenstrain: {} points, max relative stress error {worst:e}",
        stress.len() / 9
    );
}
