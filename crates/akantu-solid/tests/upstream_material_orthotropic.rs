// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_material_orthotropic.cc, orthotropic.dat, square.geo}.
//! Port of the model run of upstream `test_material_orthotropic` (TEST-137;
//! the material part is `material_plumbing.rs::test_material_orthotropic_materials`):
//! the frozen `square.msh` (D-009), `createBoundaryGroupFromGeometry`, the
//! two materials of `orthotropic.dat` (every element in `test_mat_1`, the
//! default selector's first material), explicit lumped mass with
//! `Δt = Δt_crit / 10`, the Neumann condition `σ·n` with `σ = 10³ I` on
//! `boundary_0`, 1000 steps, `E_pot` and `E_kin` after every step (upstream
//! writes them to `energy.csv`; ParaView dumps not reproduced).
//!
//! Upstream has no assertion (smoke test). Independent checks added here:
//!
//! * `test_material_orthotropic` (the upstream run): every energy is finite,
//!   `E_kin ≥ 0`, `E_pot ≥ 0`, the run is not trivial (`E_pot > 0`), and the
//!   first law holds. The body starts at rest and unstrained under a
//!   constant `f_ext`, so the exact external work is `f_ext · u_n`.
//!   Akantu's `"external work"` adds `Δt f_ext · v_{k}` per step (the
//!   end-of-step velocity: a rectangle rule), and for the central-difference
//!   scheme `Σ_{k≤n} Δt v_k = u_n + (Δt/2) v_n + (Δt²/4) a_n` exactly
//!   (Akantu starts with `a_0 = 0`). The summed work `W_n` therefore carries
//!   the first-order term `(Δt/2) f_ext · v_n` (≈ `1/(2N)` of the energy in
//!   this run, which is all of the uncorrected residual `W_n − E_n`). The test
//!   checks, relative to `max_n E_n` (`E = E_kin + E_pot`):
//!   - the corrected balance `max_n abs(W_n − (Δt/2) f_ext · v_n − E_n)`,
//!   - the exact-work balance `max_n abs(f_ext · u_n − E_n)`,
//!
//!   both `≤ 1e-4`. What remains after the correction is second order:
//!   `E_n − f_ext · u_n = (Δt²/8) a_nᵀ M a_n` (the modified energy that the
//!   scheme conserves exactly for a linear system) and `(Δt²/4) f_ext · a_n`,
//!   both bounded by a few times the kinetic energy after the first
//!   (half-kick) step, `(Δt²/8) f_extᵀ M⁻¹ f_ext` (measured 7.0e-6 and
//!   6.7e-6). Rule in `docs/NUMERICAL_VALIDATION.md` (reviewed).
//!   This is a **consistency** check: the internal force, `E_pot`, `E_kin`
//!   and the work all use the same stiffness and mass, so a wrong
//!   orthotropic stiffness still balances (the mutants
//!   "C'₁₁ built with E2" and "G12 doubled" pass it).
//! * `test_material_orthotropic_stiffness` checks the stiffness itself
//!   through the model: on the same mesh, with every element in
//!   `test_mat_1` (axes `x`, `y`) and then in `test_mat_2` (axes rotated by
//!   90°), the displacement `u = ε x` of a general uniform strain is
//!   prescribed and the internal forces are assembled. The stress at every
//!   quadrature point must equal the closed-form `σ = Rᵀ (C (R ε Rᵀ)) R`,
//!   with `C` the inverse of the 2-D compliance of the engineering constants
//!   (`1/E1, 1/E2, −ν12/E1`, shear `1/G12`), and the assembled internal force
//!   must balance the consistent nodal traction of that stress
//!   (`f_ext + f_int = 0` at every node), both within `1e-12` relative
//!   (round-off; linear elements reproduce a uniform strain exactly). The
//!   stiffness values are also verified bitwise against the oracle by the
//!   M0 material fixtures (`material_fixtures::
//!   elastic_orthotropic_material_points_match_oracle_bitwise`,
//!   `fixtures/material/elastic_orthotropic/*`).

mod model_common;

use akantu_core::tensor::MatrixX;
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::SharedMesh;
use akantu_mesh::clusters::create_boundary_group_from_geometry;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FromHigherDim;
use akantu_model::constitutive_law::DefaultSelector;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use akantu_validation::reduce::NanMinMax;
use model_common::read_mesh;

/// `orthotropic.dat` (verbatim).
const ORTHOTROPIC: &str = "material elastic_orthotropic [\n\
         name = test_mat_1\n\
         rho  = 3.8328782866680342684   # density in g/mol/A^2\n\
         E1   = 10 * 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         E2   = 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         nu12 = 0.33333333333333333333  # poisson's ratio\n\
         G12  = 3.6702824005625512938   # shear modulus in kcal/mol/A^3 \n\
]\n\
\n\
material elastic_orthotropic [\n\
         name = test_mat_2\n\
\t n1 = [0, 1, 0]\n\
\t n2 = [-1, 0, 0]\n\
         rho  = 3.8328782866680342684   # density in g/mol/A^2\n\
         E1   = 10 * 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         E2   = 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         nu12 = 0.33333333333333333333  # poisson's ratio\n\
         G12  = 3.6702824005625512938   # shear modulus in kcal/mol/A^3 \n\
]\n";

/// Relative tolerance of the corrected and exact-work energy balances
/// (reviewed; see the module doc).
const ENERGY_BALANCE_TOL: f64 = 1e-4;

/// Relative tolerance of the closed-form stiffness checks (reviewed;
/// round-off of a uniform strain reproduced by linear
/// elements).
const STIFFNESS_TOL: f64 = 1e-12;

/// The plate of upstream's test: `square.msh` with its boundary group and
/// the two materials of `orthotropic.dat`; `material` forces every element
/// into that material (upstream: the default selector, i.e. material 0).
fn plate_model(material: Option<usize>) -> SolidMechanicsModel {
    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_materials/square.msh",
    );
    let mut mesh = read_mesh(&path, 2);
    let nb_clusters = create_boundary_group_from_geometry(&mut mesh).unwrap();
    assert_eq!(nb_clusters, 1, "the square has one boundary cluster");

    let parser = Parser::parse_str(ORTHOTROPIC, "orthotropic.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    if let Some(m) = material {
        model.set_material_selector(Box::new(DefaultSelector(m)));
    }
    // initFull() with the default analysis method (explicit lumped mass)
    model.init_full(&ModelOptions::default()).unwrap();
    assert_eq!(model.nb_materials(), 2);
    model
}

fn dot(a: &[Real], b: &[Real]) -> Real {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
fn test_material_orthotropic() {
    let max_steps = 1000;
    let mut model = plate_model(None);

    let time_step = model.stable_time_step().unwrap();
    let dt = time_step / 10.;
    model.set_time_step(dt, None).unwrap();
    model.assemble_mass_lumped().unwrap();

    let mut stress = MatrixX::identity(2);
    for v in stress.as_mut_slice() {
        *v *= 1e3;
    }
    model
        .apply_neumann_bc(&FromHigherDim::new(stress), "boundary_0")
        .unwrap();
    assert!(
        model.blocked_dofs().unwrap().as_slice().iter().all(|b| !b),
        "the plate has no Dirichlet condition (the work is f_ext · v on every DOF)"
    );

    let mut work = 0.0;
    let mut max_total = 0.0_f64;
    // per step: (E, W, f·u, f·v, (Δt²/8) aᵀMa)
    let mut rows = Vec::with_capacity(max_steps);
    for s in 0..max_steps {
        model.solve_step(None).unwrap();
        let epot = model.energy("potential").unwrap();
        let ekin = model.energy("kinetic").unwrap();
        work += model.energy("external work").unwrap();
        assert!(
            epot.is_finite() && ekin.is_finite() && work.is_finite(),
            "step {s}: epot {epot}, ekin {ekin}, work {work}"
        );
        assert!(
            epot >= 0.0 && ekin >= 0.0,
            "step {s}: epot {epot}, ekin {ekin}"
        );
        let f_g = model.external_force().unwrap();
        let f = f_g.as_slice();
        let fu = dot(f, model.displacement().unwrap().as_slice());
        let fv = dot(f, model.velocity().unwrap().as_slice());
        // diagnostic only: the modified-energy term of the error analysis
        let a_g = model.acceleration().unwrap();
        let a = a_g.as_slice();
        let m_g = model.mass().unwrap();
        let m = m_g.as_slice();
        let ama: Real = a.iter().zip(m).map(|(ai, mi)| ai * mi * ai).sum();
        let total = epot + ekin;
        max_total = max_total.nan_max(total);
        rows.push((total, work, fu, fv, 0.125 * dt * dt * ama));
        if s == max_steps / 2 || s == max_steps - 1 {
            eprintln!(
                "step {s}: epot {epot:.6e}, ekin {ekin:.6e}, tot {total:.6e}, work {work:.6e}, \
                 f·u {fu:.6e}"
            );
        }
    }
    assert!(max_total > 0.0, "trivial run");
    assert!(
        rows.iter().any(|r| r.0 > 0.0 && r.2 > 0.0),
        "no deformation"
    );

    // (worst relative value, step) of each residual
    let mut corrected = (0.0_f64, 0);
    let mut exact = (0.0_f64, 0);
    let mut uncorrected = (0.0_f64, 0);
    let mut modified = (0.0_f64, 0);
    for (s, &(total, w, fu, fv, half_ama)) in rows.iter().enumerate() {
        let update = |worst: &mut (f64, usize), r: f64| {
            let rel = r.abs() / max_total;
            if rel > worst.0 {
                *worst = (rel, s);
            }
        };
        update(&mut corrected, w - 0.5 * dt * fv - total);
        update(&mut exact, fu - total);
        update(&mut uncorrected, w - total);
        update(&mut modified, total - fu - half_ama);
    }
    eprintln!(
        "orthotropic plate, relative to max(E_kin + E_pot) = {max_total:.6e}:\n  \
         corrected balance  max |W − (Δt/2) f·v − E| = {:.3e} at step {} (tol {ENERGY_BALANCE_TOL:e})\n  \
         exact-work balance max |f·u − E|            = {:.3e} at step {} (tol {ENERGY_BALANCE_TOL:e})\n  \
         diagnostic: uncorrected max |W − E|         = {:.3e} at step {}\n  \
         diagnostic: modified energy max |E − f·u − (Δt²/8) aᵀMa| = {:.3e} at step {}",
        corrected.0,
        corrected.1,
        exact.0,
        exact.1,
        uncorrected.0,
        uncorrected.1,
        modified.0,
        modified.1
    );
    assert!(
        corrected.0 <= ENERGY_BALANCE_TOL,
        "corrected energy balance violated: {:e} at step {}",
        corrected.0,
        corrected.1
    );
    assert!(
        exact.0 <= ENERGY_BALANCE_TOL,
        "exact-work energy balance violated: {:e} at step {}",
        exact.0,
        exact.1
    );
}

/// Closed-form 2-D orthotropic stress for the strain `eps` (symmetric) and
/// the material axes `n1`, `n2` (unit vectors): `σ' = C ε'` in the material
/// frame, `ε' = R ε Rᵀ`, `σ = Rᵀ σ' R` (rows of `R`: `n1`, `n2`), with `C`
/// the inverse of the 2-D compliance of the engineering constants.
fn orthotropic_stress(
    (e1, e2, nu12, g12): (Real, Real, Real, Real),
    n: [[Real; 2]; 2],
    eps: [[Real; 2]; 2],
) -> [[Real; 2]; 2] {
    // compliance (Voigt, engineering shear) and its inverse
    let (s11, s22, s12) = (1. / e1, 1. / e2, -nu12 / e1);
    let det = s11 * s22 - s12 * s12;
    let (c11, c22, c12, c66) = (s22 / det, s11 / det, -s12 / det, g12);
    let rot = |a: [[Real; 2]; 2], fwd: bool| -> [[Real; 2]; 2] {
        let mut out = [[0.0; 2]; 2];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, o) in row.iter_mut().enumerate() {
                for k in 0..2 {
                    for l in 0..2 {
                        *o += if fwd {
                            n[i][k] * a[k][l] * n[j][l]
                        } else {
                            n[k][i] * a[k][l] * n[l][j]
                        };
                    }
                }
            }
        }
        out
    };
    let ep = rot(eps, true);
    let s11p = c11 * ep[0][0] + c12 * ep[1][1];
    let s22p = c12 * ep[0][0] + c22 * ep[1][1];
    let s12p = c66 * 2. * ep[0][1];
    rot([[s11p, s12p], [s12p, s22p]], false)
}

#[test]
fn test_material_orthotropic_stiffness() {
    const G: GhostType = GhostType::NotGhost;
    let ty = ElementType::Triangle3;
    // the constants of `orthotropic.dat`, parsed from their verbatim text
    let value = |text: &str| -> Real { text.parse().unwrap() };
    let e2 = value("9.7874197348334701168");
    let constants = (
        10. * e2,
        e2,
        value("0.33333333333333333333"),
        value("3.6702824005625512938"),
    );
    // a general uniform strain (normal and shear components)
    let eps = [[1.0e-3, 0.4e-3], [0.4e-3, -0.7e-3]];
    for (index, name, axes) in [
        (0, "test_mat_1", [[1., 0.], [0., 1.]]),
        (1, "test_mat_2", [[0., 1.], [-1., 0.]]),
    ] {
        let mut model = plate_model(Some(index));
        let sigma = orthotropic_stress(constants, axes, eps);
        let sigma_norm = sigma
            .iter()
            .flatten()
            .fold(0.0_f64, |m, v| m.nan_max(v.abs()));
        let nb_element = model.physics.base.mesh.read().unwrap().nb_element(ty, G);
        assert!(nb_element > 0);
        {
            let law = model.material(index).unwrap();
            assert_eq!(law.name(), name);
            assert_eq!(
                law.base().element_filter.get(ty, G).unwrap().len(),
                nb_element
            );
        }

        // u = ε x
        let x = model
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        for (u, p) in model
            .displacement_mut()
            .unwrap()
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(x.as_chunks::<2>().0)
        {
            for (ui, row) in u.iter_mut().zip(&eps) {
                *ui = row[0] * p[0] + row[1] * p[1];
            }
        }
        model.assemble_internal_forces().unwrap();

        // stress at every quadrature point vs the closed form
        let law = model.material(index).unwrap();
        let stress = law.base().stress.get(ty, G).unwrap();
        assert!(stress.len() >= nb_element);
        assert_eq!(stress.nb_component(), 4);
        let mut worst_stress = 0.0_f64;
        for q in 0..stress.len() {
            let s = stress.tuple(q);
            // symmetric tensors: the storage order of [xy, yx] is irrelevant
            let expected = [sigma[0][0], sigma[0][1], sigma[1][0], sigma[1][1]];
            for (v, e) in s.iter().zip(expected) {
                worst_stress = worst_stress.nan_max((v - e).abs() / sigma_norm);
            }
        }

        // assembled internal force vs the consistent traction of σ
        let mut bc = MatrixX::zeros(2, 2);
        for (i, row) in sigma.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                *bc.at_mut(i, j) = v;
            }
        }
        drop(stress);
        model
            .apply_neumann_bc(&FromHigherDim::new(bc), "boundary_0")
            .unwrap();
        let f_ext_g = model.external_force().unwrap();
        let f_ext = f_ext_g.as_slice();
        let f_int_g = model.internal_force().unwrap();
        let f_int = f_int_g.as_slice();
        let f_norm = f_ext.iter().fold(0.0_f64, |m, v| m.nan_max(v.abs()));
        assert!(f_norm > 0.0);
        let worst_force = f_ext
            .iter()
            .zip(f_int)
            .fold(0.0_f64, |m, (e, i)| m.nan_max((e + i).abs() / f_norm));

        eprintln!(
            "{name}: σ = {sigma:?}; max |σ_q − σ| / max|σ| = {worst_stress:.3e}, \
             max |f_ext + f_int| / max|f_ext| = {worst_force:.3e} (tol {STIFFNESS_TOL:e})"
        );
        assert!(
            worst_stress <= STIFFNESS_TOL,
            "{name}: stress differs from the closed-form orthotropic stress: {worst_stress:e}"
        );
        assert!(
            worst_force <= STIFFNESS_TOL,
            "{name}: internal force does not balance the closed-form traction: {worst_force:e}"
        );
    }
}
