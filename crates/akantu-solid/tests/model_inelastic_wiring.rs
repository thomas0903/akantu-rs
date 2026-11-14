// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS).
// The inelastic and viscoelastic laws
// driven through SolidMechanicsModel (src/model/solid_mechanics/
// solid_mechanics_model.cc: assembleStiffnessMatrix, getEnergy;
// materials/material_viscoelastic/material_viscoelastic_maxwell.cc:
// hasStiffnessMatrixChanged).

//! Model-level wiring checks that do not need new oracle fixtures: the
//! stiffness reassembly decision reads the model state of the phase (the
//! time step, for `viscoelastic_maxwell`), and the laws' energies are
//! reachable through the model.

mod model_common;

use akantu_core::Real;
use akantu_core::enums::AnalysisMethod;
use model_common::*;

const MAXWELL: &str = "material viscoelastic_maxwell [\n  name = m\n  rho = 1\n  Einf = 10\n  nu = 0.25\n  Eta = [10]\n  Ev = [5]\n]\n";

/// The upstream patch-test mesh of a `fixtures/model/static/<case>` input.
fn patch_mesh(case_name: &str, dim: usize) -> akantu_mesh::Mesh {
    read_mesh(&case(case_name).input_path("mesh").unwrap(), dim)
}

fn patch_model(dat: &str) -> akantu_solid::SolidMechanicsModel {
    model_with(patch_mesh("static/patch_triangle_3", 2), dat)
}

/// `E_eff(Δt) = Einf + (1 − e^{−Δt/λ})·Ev·λ/Δt` (`λ = Eta/Ev = 2`).
fn e_eff(dt: Real) -> Real {
    let lambda: Real = 2.;
    10. + (1. - (-dt / lambda).exp()) * 5. * lambda / dt
}

/// Upstream `MaterialViscoelasticMaxwell::hasStiffnessMatrixChanged` is
/// `previous_dt != model.getTimeStep()`, so `assembleStiffnessMatrix` after
/// `setTimeStep` reassembles `K` even before any stress evaluation (e.g.
/// the `_linear` solver assembles the matrix before the residual). The
/// model asks the laws with its phase services
/// (`Material::has_matrix_changed_in`); with the state-free question the
/// Maxwell law would compare with the last time step **it** evaluated and
/// keep the stale `K`.
#[test]
fn maxwell_stiffness_follows_the_model_time_step() {
    let mut model = patch_model(MAXWELL);
    init(&mut model, AnalysisMethod::Static);
    model.set_time_step(1.0, None).unwrap();
    model.assemble_stiffness_matrix(false).unwrap();
    let k1: Vec<Real> = model.dof_manager().matrix("K").unwrap().coo().2.to_vec();
    // unchanged time step: no reassembly needed, K identical
    model.assemble_stiffness_matrix(false).unwrap();
    let k1b: Vec<Real> = model.dof_manager().matrix("K").unwrap().coo().2.to_vec();
    assert_eq!(k1, k1b);
    model.set_time_step(0.25, None).unwrap();
    model.assemble_stiffness_matrix(false).unwrap();
    let k2: Vec<Real> = model.dof_manager().matrix("K").unwrap().coo().2.to_vec();
    let ratio = e_eff(0.25) / e_eff(1.0);
    let mut checked = 0;
    for (a, b) in k1.iter().zip(&k2) {
        if a.abs() > 1e-12 {
            assert!(
                (b / a - ratio).abs() <= 1e-12,
                "K scales with E_eff(Δt): {b} / {a} vs {ratio}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
}

/// The laws' own energies are reachable through the model
/// (`SolidMechanicsModel::getEnergy(id)` sums `material.getEnergy(id)`):
/// a quasi-static uniaxial ramp in 3-D (corrected defaults) gives a positive
/// `plastic` energy for the J2 law and a positive `dissipated` energy for
/// Mazars (both zero unless the law's own bookkeeping runs in the model's
/// `afterSolveStep`).
#[test]
fn inelastic_energies_through_the_model() {
    // modified Newton (elastic / secant tangents, upstream contract): solver
    // settings chosen so that it converges; not a fixture tolerance
    for (dat, id, increment) in [
        (
            "material plastic_linear_isotropic_hardening [\n  name = p\n  rho = 1\n  E = 100\n  nu = 0.3\n  sigma_y = 0.05\n  h = 50\n]\n",
            "plastic",
            2e-3,
        ),
        (
            "material mazars [\n  name = c\n  rho = 1\n  E = 25e9\n  nu = 0.2\n  K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n]\n",
            "dissipated",
            1e-4,
        ),
    ] {
        let mut model = model_with(patch_mesh("static/patch_tetrahedron_4", 3), dat);
        init(&mut model, AnalysisMethod::Static);
        set_newton(&mut model, 500, 1e-8, "residual");
        let x: Vec<Real> = {
            let mesh = model.physics.base.mesh.read().unwrap();
            mesh.nodes().as_slice().to_vec()
        };
        let nn = x.len() / 3;
        let xmin = (0..nn).map(|n| x[n * 3]).fold(Real::INFINITY, Real::min);
        let xmax = (0..nn)
            .map(|n| x[n * 3])
            .fold(Real::NEG_INFINITY, Real::max);
        for step in 1..=4 {
            {
                let mut b = model.blocked_dofs_mut().unwrap();
                for n in 0..nn {
                    let on_face =
                        (x[n * 3] - xmin).abs() < 1e-10 || (x[n * 3] - xmax).abs() < 1e-10;
                    for c in 0..3 {
                        b.as_mut_slice()[n * 3 + c] = on_face;
                    }
                }
            }
            {
                let strain = increment * Real::from(step);
                let mut u = model.displacement_mut().unwrap();
                for n in 0..nn {
                    if (x[n * 3] - xmax).abs() < 1e-10 {
                        u.as_mut_slice()[n * 3] = strain * (xmax - xmin);
                    } else if (x[n * 3] - xmin).abs() < 1e-10 {
                        u.as_mut_slice()[n * 3] = 0.;
                    }
                }
            }
            model.solve_step(None).unwrap();
        }
        let e_model = model.energy(id).unwrap();
        assert!(e_model > 0. && e_model.is_finite(), "{id}: {e_model}");
    }
}
