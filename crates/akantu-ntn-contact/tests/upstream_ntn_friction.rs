// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (same intent,
// upstream tolerances) of extra_packages/traction-at-split-node-contact/test/
// test_ntn_model/{test_ntn_friction.cc, test_ntn_friction_fixture.hh}:
// TestNTNFrictionFixture.TestCompression and
// MaterialFileTests/ParamTestNTNFrictionFixture.TestRateAndState over the 12
// material files upstream instantiates (the three revised_n_shaped files are
// commented out upstream as "not passing in current state": documented
// exceptions of TEST-177, pinned by the characterisation test
// `revised_n_shaped_cases_disabled_upstream_characterisation`).

mod common;

use akantu_core::Real;
use akantu_core::enums::SpatialDirection;
use akantu_model::boundary_condition::{FlagOnly, FromSameDim};
use akantu_ntn_contact::{NtnContactSolverCallback, TasnUpstreamCompat};
use akantu_parser::param::ParameterRegistry;
use akantu_parser::{Parser, SearchScope};
use akantu_solid::SolidMechanicsModel;
use common::*;

struct Fixture {
    model: SolidMechanicsModel,
    solver: NtnContactSolverCallback,
    parser: Parser,
    time_step_factor: Real,
}

/// Upstream `TestNTNFrictionFixture::SetUp(mesh_file, filename, periodic)`.
fn set_up(
    mesh_path: &std::path::Path,
    material: &str,
    periodic: bool,
    compat: TasnUpstreamCompat,
) -> Fixture {
    let mut mesh = read_mesh(mesh_path, 2);
    let parser = Parser::parse_file(data(material)).unwrap();
    if periodic {
        // the mesh events matter only to consumers registered on the mesh
        // (none yet)
        let _slider_events = akantu_mesh::periodic::make_periodic(
            &mut mesh,
            SpatialDirection::X,
            Some(("slider_left", "slider_right")),
        )
        .unwrap();
        let _base_events = akantu_mesh::periodic::make_periodic(
            &mut mesh,
            SpatialDirection::X,
            Some(("base_left", "base_right")),
        )
        .unwrap();
    }
    let mut model = model(mesh, parser.clone());
    let time_step_factor: Real = parser
        .user_section()
        .unwrap()
        .get_parameter_value("time_step_factor", SearchScope::Current)
        .unwrap();
    let normal_dir = 1;
    let solver = NtnContactSolverCallback::new(
        &mut model,
        "slider_bottom",
        "base_top",
        normal_dir,
        time_step_factor,
        compat,
    )
    .unwrap();
    Fixture {
        model,
        solver,
        parser,
        time_step_factor,
    }
}

impl Fixture {
    fn apply_fixed_boundary_conditions(&mut self) {
        let m = &mut self.model;
        m.apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::Y,
            },
            "base_bottom",
        )
        .unwrap();
        m.apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "base_bottom",
        )
        .unwrap();
        m.apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "base_left",
        )
        .unwrap();
        m.apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "base_right",
        )
        .unwrap();
        m.apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "slider_left",
        )
        .unwrap();
        m.apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "slider_right",
        )
        .unwrap();
    }

    fn solve_step(&mut self, solver_id: Option<&str>) {
        self.try_solve_step(solver_id).unwrap();
    }

    fn try_solve_step(&mut self, solver_id: Option<&str>) -> akantu_core::Result<()> {
        self.solver.solve_step(&mut self.model, solver_id)
    }

    fn apply_velocity(&mut self, shear_vel: Real) {
        apply_velocity_bc(&mut self.model, "slider_top", 0.5 * shear_vel, 0);
        apply_velocity_bc(&mut self.model, "base_bottom", -0.5 * shear_vel, 0);
    }

    /// Upstream fixture `setToSteadyState(shear_vel)`.
    fn set_to_steady_state(&mut self, shear_vel: Real) {
        let (slider, base) = {
            let mesh_h = self.model.physics.base.mesh.clone();
            let mesh = mesh_h.read().unwrap();
            let g = mesh.groups();
            (
                g.element_group("slider").unwrap().nodes().to_vec(),
                g.element_group("base").unwrap().nodes().to_vec(),
            )
        };
        let dt = self.model.time_step(None).unwrap();
        let dim = 2;
        {
            let mut v_g = self.model.velocity_mut().unwrap();
            let v = v_g.as_mut_slice();
            for &n in &slider {
                v[n as usize * dim] = 0.5 * shear_vel;
            }
            for &n in &base {
                v[n as usize * dim] = -0.5 * shear_vel;
            }
        }
        {
            let mut inc_g = self.model.increment_mut().unwrap();
            let inc = inc_g.as_mut_slice();
            for &n in &slider {
                inc[n as usize * dim] = 0.5 * shear_vel * dt;
            }
            for &n in &base {
                inc[n as usize * dim] = -0.5 * shear_vel * dt;
            }
        }
        let friction = self.solver.friction_mut();
        let phi = friction
            .get_param("friction_state")
            .unwrap()
            .as_real()
            .unwrap();
        let nb = friction.is_sticking().len();
        for n in 0..nb {
            friction.is_sticking_mut().set_at(n, false);
            friction.update_friction_state(n, phi).unwrap();
        }
    }
}

// upstream reference values (test_ntn_friction.cc), copied verbatim
#[allow(clippy::excessive_precision)]
const REF_PRESSURE: [Real; 28] = [
    0.0,
    0.0,
    0.0,
    -0.000000000061,
    -0.000000072362,
    -0.000011543530,
    -0.000611366799,
    -0.015767551005,
    -0.243299242707,
    -2.545124342651,
    -19.590589983818,
    -117.424265038805,
    -570.863394812224,
    -2319.934641685013,
    -8063.335008171387,
    -24394.402146997298,
    -65124.820990435350,
    -155070.572077111341,
    -332093.403606061882,
    -643767.301224190160,
    -1135088.793070584768,
    -1826677.261436731322,
    -2689111.493139004800,
    -3625981.040552595165,
    -4480965.840503907762,
    -5077927.732134595513,
    -5286961.279143503867,
    -5090152.021338184364,
];

/// Upstream `TestCompression` body; returns the checked pressures.
fn compression(compat: TasnUpstreamCompat) -> Vec<Real> {
    let mut f = set_up(
        &data("ntn_test_compression.msh"),
        "material_ntn_friction_slip_weakening.dat",
        false,
        compat,
    );
    let ref_contact_node = 1;
    let pressure_direction = 1;
    let trac = [0.0, -1e6];
    f.apply_fixed_boundary_conditions();
    let simulation_time = 5e-4;
    let time_step = f.model.time_step(None).unwrap();
    let nb_steps = (simulation_time / time_step + 1.0) as usize;
    let period = 5e-5;
    let mut checked = Vec::new();
    for s in 0..nb_steps {
        let t = s as Real * time_step;
        let trac_t = if t < period / 2.0 {
            let sn = (t / period * 2.0 * std::f64::consts::PI).sin();
            vec![trac[0] * sn, trac[1] * sn]
        } else {
            vec![0.0, 0.0]
        };
        f.model
            .apply_neumann_bc(&FromSameDim::new(trac_t), "slider_top")
            .unwrap();
        f.solve_step(None);
        if s % 5 == 0 {
            let p = f.solver.contact_pressure().tuple(ref_contact_node)[pressure_direction];
            // upstream indexes ref_pressure(check_counter) unchecked
            assert!(
                checked.len() < REF_PRESSURE.len(),
                "more checks than reference values"
            );
            let r = REF_PRESSURE[checked.len()];
            assert!(
                (p - r).abs() <= 5e-9,
                "step {s}: pressure {p:.12} vs reference {r:.12}"
            );
            checked.push(p);
        }
    }
    checked
}

#[test]
fn test_ntn_friction_fixture_compression() {
    let checked = compression(TasnUpstreamCompat::default());
    assert_eq!(checked.len(), REF_PRESSURE.len());
}

/// Same case through the upstream factory fallback (UD-039: the
/// `no_regularisation` option allocates Rubin–Ampuero with `t* = 0`):
/// bit-identical pressures.
#[test]
fn test_ntn_friction_fixture_compression_upstream_compat() {
    let a = compression(TasnUpstreamCompat::default());
    let b = compression(TasnUpstreamCompat::upstream());
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.to_bits(), y.to_bits());
    }
}

/// Energy terms of one rate-and-state step (the upstream test writes them to
/// `firction-energies-<file>.csv`).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // fields are reported on failure
struct Energies {
    ekin: Real,
    epot: Real,
    work: Real,
    econ: [Real; 2],
    tot: Real,
}

/// Upstream `TestRateAndState` body for one material file.
fn rate_and_state(material: &str, compat: TasnUpstreamCompat) -> Vec<Energies> {
    rate_and_state_checked(material, compat, true).unwrap().0
}

/// [`rate_and_state`] with the upstream velocity assertion optional; also
/// returns the base-node velocity errors `|v_x + V/2|` found at the checked
/// steps.
fn rate_and_state_checked(
    material: &str,
    compat: TasnUpstreamCompat,
    assert_velocity: bool,
) -> akantu_core::Result<(Vec<Energies>, Vec<Real>)> {
    let mut f = set_up(&ras_mesh_path(), material, true, compat);
    let data_s = f.parser.user_section().unwrap();
    let get = |n: &str| -> Real { data_s.get_parameter_value(n, SearchScope::Current).unwrap() };
    let getv =
        |n: &str| -> Vec<Real> { data_s.get_parameter_value(n, SearchScope::Current).unwrap() };
    let shear_vel = get("shear_velocity");
    let trac_top = getv("top_traction");
    let trac_bottom = getv("bot_traction");
    let fss = get("fss");
    let nb_steps = get("nb_steps") as usize;
    let (e, nu) = {
        let mat = f.model.material_by_name("slider").unwrap();
        (
            mat.get_param("E").unwrap().as_real().unwrap(),
            mat.get_param("nu").unwrap().as_real().unwrap(),
        )
    };
    let shear_modulus = e / (2.0 * (1.0 + nu));
    let normal_strain_applied = trac_top[1] / e - nu * nu * trac_top[1] / e;
    let positions: Vec<Real> = {
        let mesh_h = f.model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    {
        let mut disp_g = f.model.displacement_mut().unwrap();
        let disp = disp_g.as_mut_slice();
        for n in 0..positions.len() / 2 {
            disp[2 * n] = fss * -trac_top[1] / (shear_modulus) * positions[2 * n + 1];
            disp[2 * n + 1] = normal_strain_applied * positions[2 * n + 1];
        }
    }
    f.model
        .apply_neumann_bc(&FromSameDim::new(trac_top.clone()), "slider_top")
        .unwrap();
    f.model
        .apply_neumann_bc(&FromSameDim::new(trac_bottom), "base_bottom")
        .unwrap();
    f.set_to_steady_state(shear_vel);
    let stable_time_step = f.model.stable_time_step().unwrap();
    let time_step = stable_time_step * f.time_step_factor;
    f.model.set_time_step(time_step, None).unwrap();
    let base_nodes: Vec<akantu_core::Idx> = {
        let mesh_h = f.model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.groups()
            .element_group("base")
            .unwrap()
            .nodes()
            .to_vec()
    };
    f.apply_velocity(shear_vel);
    let mut einit = 0.0;
    let mut energies = Vec::with_capacity(nb_steps);
    let mut velocity_errors = Vec::new();
    for s in 0..nb_steps {
        f.try_solve_step(Some("explicit_lumped"))?;
        let ekin = f.model.energy("kinetic").unwrap();
        let epot = f.model.energy("potential").unwrap();
        let work = f.model.energy("external work new").unwrap();
        let econ = f.solver.external_work();
        if s == 0 {
            einit = ekin + epot - (work + econ[0] + econ[1]);
        }
        let tot = ekin + epot - (work + econ[0] + econ[1]) - einit;
        energies.push(Energies {
            ekin,
            epot,
            work,
            econ,
            tot,
        });
        if s % 100 == 0 {
            let v_g = f.model.velocity().unwrap();
            let v = v_g.as_slice();
            for &n in &base_nodes {
                let vx = v[n as usize * 2];
                velocity_errors.push((vx - (-shear_vel / 2.0)).abs());
                if assert_velocity {
                    assert!(
                        (vx - (-shear_vel / 2.0)).abs() <= 1e-10,
                        "{material} step {s}: base node {n} velocity {vx} vs {}",
                        -shear_vel / 2.0
                    );
                }
            }
        }
        f.apply_velocity(shear_vel);
    }
    Ok((energies, velocity_errors))
}

/// Independent check (not in the upstream test, which only writes the
/// energies to a CSV file): upstream's energy expression `E_kin + E_pot −
/// (W_ext + W_contact + W_friction)` stays at its initial value. Upstream's
/// friction "work" is the dissipation `−Σ f_fric·Δu ≥ 0`; in these steady
/// sliding runs `W_ext + W_friction = 0` and `E_kin + E_pot` is constant up
/// to rounding, so the check is a power balance of steady sliding only: it
/// does not detect a sign error of the friction work, and transient runs do
/// not satisfy upstream's expression.
///
/// Bound (PROPOSED rule NTN-EB, docs/NUMERICAL_VALIDATION.md,
/// tightened from 1e-9): 1e-10 of the largest energy term;
/// accumulation rounding of the 1000-step running sums over ≈ 5·10² DOFs is
/// ≲ n_steps · n_dof · ε ≈ 1000 · 5·10² · 1.1e-16 ≈ 6e-11 of the largest
/// term (observed ≤ 2.2e-11).
fn check_energy_balance(material: &str, e: &[Energies]) {
    let scale = e
        .iter()
        .map(|x| {
            x.ekin
                .abs()
                .max(x.epot.abs())
                .max(x.work.abs())
                .max(x.econ[0].abs())
                .max(x.econ[1].abs())
        })
        .fold(0.0, Real::max);
    assert!(scale > 0.0);
    for (s, x) in e.iter().enumerate() {
        assert!(
            x.tot.abs() <= 1e-10 * scale,
            "{material} step {s}: energy balance error {} (scale {scale})",
            x.tot
        );
    }
}

macro_rules! ras_test {
    ($name:ident, $file:literal) => {
        #[test]
        fn $name() {
            let e = rate_and_state($file, TasnUpstreamCompat::default());
            check_energy_balance($file, &e);
        }
    };
}

ras_test!(
    test_rate_and_state_dieterich_ageing_regularized,
    "material_ntn_friction_rate_and_state_dieterich_ageing_regularized.dat"
);
ras_test!(
    test_rate_and_state_dieterich_ageing,
    "material_ntn_friction_rate_and_state_dieterich_ageing.dat"
);
ras_test!(
    test_rate_and_state_dieterich_slip,
    "material_ntn_friction_rate_and_state_dieterich_slip.dat"
);
ras_test!(
    test_rate_and_state_revised_dieterich_ageing_regularized,
    "material_ntn_friction_rate_and_state_revised_dieterich_ageing_regularized.dat"
);
ras_test!(
    test_rate_and_state_revised_dieterich_ageing,
    "material_ntn_friction_rate_and_state_revised_dieterich_ageing.dat"
);
ras_test!(
    test_rate_and_state_revised_dieterich_slip,
    "material_ntn_friction_rate_and_state_revised_dieterich_slip.dat"
);
ras_test!(
    test_rate_and_state_regularized_dieterich_ageing_regularized,
    "material_ntn_friction_rate_and_state_regularized_dieterich_ageing_regularized.dat"
);
ras_test!(
    test_rate_and_state_regularized_dieterich_ageing,
    "material_ntn_friction_rate_and_state_regularized_dieterich_ageing.dat"
);
ras_test!(
    test_rate_and_state_regularized_dieterich_slip,
    "material_ntn_friction_rate_and_state_regularized_dieterich_slip.dat"
);
ras_test!(
    test_rate_and_state_n_shaped_ageing_regularized,
    "material_ntn_friction_rate_and_state_n_shaped_ageing_regularized.dat"
);
ras_test!(
    test_rate_and_state_n_shaped_ageing,
    "material_ntn_friction_rate_and_state_n_shaped_ageing.dat"
);
ras_test!(
    test_rate_and_state_n_shaped_slip,
    "material_ntn_friction_rate_and_state_n_shaped_slip.dat"
);

/// Characterisation test (not a port: upstream does not run these cases) of
/// the three `revised_n_shaped` material files that upstream comments out of
/// `INSTANTIATE_TEST_SUITE_P` ("TODO: Fix the following tests (not passing in
/// current state)"); they are documented exceptions of TEST-177 (UD-405:
/// the data's `fss` is not a steady state of the revised N-shaped law, which
/// vanishes as V → 0, so the test's intent "base velocity −V/2" is not met
/// upstream either). The port reproduces the upstream failure of the test's
/// base-velocity check (a base node moves at −V/4 instead of −V/2 after the
/// first step). With the slip state law a NaN state (slip law at zero slip
/// velocity, `−θ·0/d_c·ln 0`) then reaches a node in contact: upstream
/// (compat switch `rate_and_state_non_finite_propagates`) the NaN strength
/// silently makes the node stick (`alpha < 1` is false); by default the port
/// stops with the UD-039 domain error (an upstream law/test-data issue,
/// not a port defect).
#[test]
fn revised_n_shaped_cases_disabled_upstream_characterisation() {
    let upstream_nan = TasnUpstreamCompat {
        rate_and_state_non_finite_propagates: true,
        ..TasnUpstreamCompat::default()
    };
    for f in [
        "material_ntn_friction_rate_and_state_revised_n_shaped_ageing_regularized.dat",
        "material_ntn_friction_rate_and_state_revised_n_shaped_ageing.dat",
        "material_ntn_friction_rate_and_state_revised_n_shaped_slip.dat",
    ] {
        let (_, verr) = rate_and_state_checked(f, upstream_nan, false).unwrap();
        assert!(
            verr.iter().any(|e| e.is_nan() || *e > 1e-10),
            "{f}: the upstream-disabled case now passes the velocity check"
        );
        match rate_and_state_checked(f, TasnUpstreamCompat::default(), false) {
            // no non-finite coefficient reached a node in contact: the
            // corrected mode is the upstream run
            Ok((_, verr2)) => {
                assert_eq!(verr.len(), verr2.len());
                for (x, y) in verr.iter().zip(&verr2) {
                    assert_eq!(x.to_bits(), y.to_bits(), "{f}");
                }
                eprintln!("{f}: completes (velocity check fails as upstream)");
            }
            Err(akantu_core::Error::Numerical(m)) => {
                // upstream: the NaN strength makes `alpha < 1` false, so the
                // node silently sticks
                assert!(m.contains("UD-039"), "{m}");
                eprintln!("{f}: NaN coefficient at a node in contact (UD-039 domain error here)");
            }
            Err(e) => panic!("{f}: unexpected error {e}"),
        }
    }
}
