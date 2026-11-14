// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_material_mazars.cc, material_mazars.dat}
// (registered UNSTABLE).
//! Port of upstream `test_material_mazars` (TEST-138): one unit
//! `_hexahedron_8` built with `MeshAccessor` (upstream node order and
//! connectivity), Mazars concrete (`material_mazars.dat`),
//! `initFull()` (explicit lumped mass), every x DOF blocked,
//! `Δt = 0.5·getStableTimeStep()`, `nb_steps = ⌊7e5/150⌋ = 4666`; the
//! nodes on `x = 1` are moved by `adisp` per step (with `v_x = adisp/Δt`)
//! along upstream's piecewise program (compression to −0.003 at half speed,
//! unload, compression to −0.004, tension 0.0015 + 0.004 back, unload 0.0005,
//! tension 0.0025, unload), `solveStep()`, the energies `potential`,
//! `kinetic`, `dissipated` and `external work` are read every step as
//! upstream, and σ_xx of quadrature point 0 is tracked. Upstream criteria
//! (`EXIT_FAILURE` otherwise), kept verbatim:
//!
//! * at step `4·nb_steps/8`, σ_xx must not be below its value at step
//!   `2·nb_steps/8` ("after this second compression step softening should
//!   have started");
//! * `|σ_max| ≤ |σ_min|`, `|σ_max − 6.24e6| ≤ 1e5`, `|σ_min + 2.943e7| ≤ 1e6`.
//!
//! **The σ_max criterion is stale upstream (UD-733).** The test does not
//! compile at the reference commit (`Array<UInt> & connectivity =
//! mesh_accessor.getConnectivity(type)` binds `Array<Idx> &`; the test is
//! `UNSTABLE`, never built by default) and its reference values predate the
//! current code: the oracle replay (`oracle/probes/upstream_tests_2b`, case
//! `fixtures/model/upstream_tests_2b/material_mazars_hexahedron_8`, x86_64,
//! connectivity type fixed, three identical runs) gives σ_max = 1.5576e6 and
//! σ_min = −2.94283e7: upstream's own criterion fails on upstream. The port
//! reproduces the oracle run **bitwise** (σ_xx and the four energies at all
//! 4666 steps, the final nodal and quadrature-point state), so its verdict
//! is upstream's: the softening, `|σ_max| ≤ |σ_min|` and σ_min criteria are
//! asserted to pass, the σ_max criterion is asserted to fail with the
//! oracle's value (documented upstream defect, not a port failure).
//!
//! The CSV and ParaView dumps are not written. In 3-D without temperature
//! and with one stress evaluation per explicit step, the Mazars defaults of
//! the port (UD-138 committed trial, UD-140 mechanical strain, UD-038
//! thermal refresh) coincide with upstream; this is asserted bitwise by
//! replaying the run with `LawOptions::upstream()`.

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solid::material::LawOptions;
use model_common::{assert_bits, assert_bits_vec, case, init, model_with, param};

/// `material_mazars.dat` (verbatim).
const MATERIAL: &str = "material mazars [\n\
\t name = concrete\n\
\t rho = 3000        # density\n\
\t E   = 32e9        # young's modulus\n\
\t nu  = 0.2         # poisson's ratio\n\
\t K0  = 9.375e-5    # Damage threshold\n\
\t At  = 1.15       # Parameter damage traction 1\n\
\t Bt  = 10000   # Parameter damage traction 2\n\
\t Ac  = 0.8     # Parameter damage compression 1\n\
\t Bc  = 1391.3    # Parameter damage compression 2\n\
\t beta = 1.00   # Parameter for shear\n\
]\n";

const TYPE: ElementType = ElementType::Hexahedron8;

/// The run's observables.
struct Run {
    /// σ_xx of quadrature point 0 after every step.
    stress_xx: Vec<Real>,
    /// `(epot, ekin, edis, wext)` after every step.
    energies: Vec<[Real; 4]>,
    /// Upstream softening check (`stress(0,0) < stress_xx_compression_1`
    /// at step `4·nb_steps/8` fails).
    softening_failed: bool,
    sigma_max: Real,
    sigma_min: Real,
    /// Final displacement, velocity, mass and the stress / damage of every
    /// quadrature point.
    final_state: Vec<(&'static str, Vec<Real>)>,
}

fn hexahedron() -> Mesh {
    let width = 1.;
    let mut mesh = Mesh::new(3, "mesh").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for p in [
            [0., 0., 0.],
            [width, 0., 0.],
            [width, width, 0.],
            [0., width, 0.],
            [0., 0., width],
            [width, 0., width],
            [width, width, width],
            [0., width, width],
        ] {
            a.push_node(&p).unwrap();
        }
        a.connectivity_mut(TYPE, GhostType::NotGhost)
            .push_tuple(&[0 as Idx, 1, 2, 3, 4, 5, 6, 7]);
    }
    mesh
}

fn run(options: Option<LawOptions>) -> Run {
    let width = 1.;
    let mesh = hexahedron();
    let nodes = mesh.nodes().as_slice().to_vec();
    let nb_dof = 8;
    let mut model = model_with(mesh, MATERIAL);
    model.physics.law_options = options;
    init(&mut model, AnalysisMethod::ExplicitLumpedMass);
    for i in 0..nb_dof {
        model.blocked_dofs_mut().unwrap().as_mut_slice()[i * 3] = true;
    }
    let time_step = model.stable_time_step().unwrap() * 0.5;
    model.set_time_step(time_step, None).unwrap();

    let nb_steps: usize = (7e5 / 150.) as usize;
    assert_eq!(nb_steps, 4666);
    let nb = nb_steps as Real;
    let mut wext = 0.;
    let (mut sigma_max, mut sigma_min) = (0.0_f64, 0.0_f64);
    let mut max_disp = 0.;
    let mut stress_xx_compression_1 = 0.;
    let mut adisp = 0.;
    let mut out = Run {
        stress_xx: Vec::with_capacity(nb_steps),
        energies: Vec::with_capacity(nb_steps),
        softening_failed: false,
        sigma_max: 0.,
        sigma_min: 0.,
        final_state: Vec::new(),
    };
    let stress00 = |m: &akantu_solid::SolidMechanicsModel| -> Real {
        m.material(0)
            .unwrap()
            .base()
            .stress
            .get(TYPE, GhostType::NotGhost)
            .unwrap()
            .as_slice()[0]
    };
    for s in 0..nb_steps {
        if s == 0 {
            max_disp = 0.003;
            adisp = -(max_disp * 8. / nb) / 2.;
        }
        if s == 2 * nb_steps / 8 {
            stress_xx_compression_1 = stress00(&model);
            max_disp += 0.;
            adisp = max_disp * 8. / nb;
        }
        if s == 3 * nb_steps / 8 {
            max_disp = 0.004;
            adisp = -max_disp * 8. / nb;
        }
        if s == 4 * nb_steps / 8 {
            if stress00(&model) < stress_xx_compression_1 {
                // upstream: "after this second compression step softening
                // should have started" → EXIT_FAILURE
                out.softening_failed = true;
                return out;
            }
            max_disp += 0.0015;
            adisp = max_disp * 8. / nb;
        }
        if s == 5 * nb_steps / 8 {
            max_disp = 0. + 0.0005;
            adisp = -max_disp * 8. / nb;
        }
        if s == 6 * nb_steps / 8 {
            max_disp = 0.0035 - 0.001;
            adisp = max_disp * 8. / nb;
        }
        if s == 7 * nb_steps / 8 {
            adisp = -max_disp * 8. / nb;
        }
        for i in 0..nb_dof {
            if (nodes[i * 3] - width).abs() < Real::EPSILON {
                model.displacement_mut().unwrap().as_mut_slice()[i * 3] += adisp;
                model.velocity_mut().unwrap().as_mut_slice()[i * 3] = adisp / time_step;
            }
        }
        model.solve_step(None).unwrap();
        let epot = model.energy("potential").unwrap();
        let ekin = model.energy("kinetic").unwrap();
        let edis = model.energy("dissipated").unwrap();
        wext += model.energy("external work").unwrap();
        let sxx = stress00(&model);
        sigma_max = sigma_max.max(sxx);
        sigma_min = sigma_min.min(sxx);
        out.stress_xx.push(sxx);
        out.energies.push([epot, ekin, edis, wext]);
    }
    out.sigma_max = sigma_max;
    out.sigma_min = sigma_min;
    let law = model.material(0).unwrap();
    let mut damage = None;
    law.for_each_internal(&mut |n, f| {
        if n == "damage"
            && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
        {
            damage = Some(
                f.get(TYPE, GhostType::NotGhost)
                    .unwrap()
                    .as_slice()
                    .to_vec(),
            );
        }
    });
    let stress = law
        .base()
        .stress
        .get(TYPE, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    out.final_state = vec![
        (
            "final/displacement",
            model.displacement().unwrap().as_slice().to_vec(),
        ),
        (
            "final/velocity",
            model.velocity().unwrap().as_slice().to_vec(),
        ),
        ("final/stress", stress),
        ("final/damage", damage.expect("damage internal")),
        ("final/mass", model.mass().unwrap().as_slice().to_vec()),
    ];
    out
}

#[test]
fn test_material_mazars() {
    let r = run(None);
    let f = case("upstream_tests_2b/material_mazars_hexahedron_8");
    // the oracle run, bitwise
    let mut n = assert_bits(&f, "step/stress_xx", &r.stress_xx);
    for (k, name) in [
        "step/energy/potential",
        "step/energy/kinetic",
        "step/energy/dissipated",
        "step/energy/external_work_sum",
    ]
    .iter()
    .enumerate()
    {
        let got: Vec<Real> = r.energies.iter().map(|e| e[k]).collect();
        n += assert_bits(&f, name, &got);
    }
    for (name, got) in &r.final_state {
        n += assert_bits(&f, name, got);
    }
    let real = |k: &str| param(&f, k).as_f64().unwrap();
    n += assert_bits_vec(
        f.case_id(),
        "sigma_max / sigma_min",
        &[r.sigma_max, r.sigma_min],
        &[real("sigma_max"), real("sigma_min")],
    );
    assert_eq!(param(&f, "softening_failed").as_bool(), Some(false));
    assert_eq!(param(&f, "upstream_criteria_pass").as_bool(), Some(false));
    eprintln!("Mazars run: {n} values bitwise equal to the oracle");
    eprintln!(
        "sigma_max = {:e}, sigma_min = {:e}",
        r.sigma_max, r.sigma_min
    );

    // upstream criteria
    assert!(
        !r.softening_failed,
        "after this second compression step softening should have started"
    );
    assert!(r.stress_xx.iter().all(|s| s.is_finite()));
    assert!(
        r.sigma_max.abs() <= r.sigma_min.abs(),
        "|sigma_max| > |sigma_min|"
    );
    assert!(
        (r.sigma_min + 2.943e7).abs() <= 1e6,
        "sigma_min = {:e} (upstream −2.943e7 ± 1e6)",
        r.sigma_min
    );
    // UD-733: upstream's own σ_max reference is stale; the oracle fails it
    // with the same value (asserted bitwise above)
    assert!(
        (r.sigma_max - 6.24e6).abs() > 1e5,
        "UD-733: sigma_max = {:e} now meets upstream's 6.24e6 ± 1e5 (update the docs)",
        r.sigma_max
    );
}

/// The port's Mazars defaults coincide with upstream on this run (no
/// temperature, 3-D, one evaluation per explicit step): stress history and
/// energies bitwise equal to the `LawOptions::upstream()` replay.
#[test]
fn test_material_mazars_upstream_mode_is_bitwise_identical() {
    let a = run(None);
    let b = run(Some(LawOptions::upstream()));
    assert_eq!(a.stress_xx.len(), b.stress_xx.len());
    for (s, (x, y)) in a.stress_xx.iter().zip(&b.stress_xx).enumerate() {
        assert_eq!(x.to_bits(), y.to_bits(), "step {s}: σ_xx {x} vs {y}");
    }
    for (s, (x, y)) in a.energies.iter().zip(&b.energies).enumerate() {
        for k in 0..4 {
            assert_eq!(x[k].to_bits(), y[k].to_bits(), "step {s}: energy {k}");
        }
    }
}
