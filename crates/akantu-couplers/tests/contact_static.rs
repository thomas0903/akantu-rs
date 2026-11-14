// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The static coupled
// solid–contact path of CouplerSolidContact (CPL-002,
// coupler_solid_contact_tmpl.hh: contact search in every Newton corrector,
// contact stiffness added to K) on the setup of
// examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc.

mod common;

use akantu_contact::{ContactUpstreamCompat, PhysicalSurfaceSelector};
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_couplers::CouplerSolidContact;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use common::*;

pub const HERTZ_DAT: &str = include_str!("data/contact/hertz_material.dat");

pub fn hertz_coupler(compat: ContactUpstreamCompat) -> CouplerSolidContact {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/hertz.msh"),
        2,
    );
    let parser = Parser::parse_str(HERTZ_DAT, "material.dat").unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let mut c = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    c.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    c.contact.set_upstream_compat(compat);
    let fix = |axis| FixedValue { value: 0.0, axis };
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::Y), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "loading")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "symmetry")
        .unwrap();
    c
}

pub fn hertz_step(c: &mut CouplerSolidContact) -> akantu_core::Result<()> {
    let inc = IncrementValue {
        value: -1e-4,
        axis: SpatialDirection::Y,
    };
    c.apply_dirichlet_bc(&inc, "loading")?;
    c.solve_step(None)
}

#[test]
fn hertz_static_corrected_vs_upstream() {
    let mut up = hertz_coupler(ContactUpstreamCompat::all());
    let mut co = hertz_coupler(ContactUpstreamCompat::default());
    let mut worst: f64 = 0.0;
    let (mut it_up, mut it_co) = (0i64, 0i64);
    // 30 steps: the contact zone grows to 6 contact elements (fast tier
    // budget; the 100-step bitwise replay is in coupler_fixtures.rs)
    for s in 0..30 {
        hertz_step(&mut up).unwrap_or_else(|e| panic!("upstream step {s}: {e}"));
        hertz_step(&mut co).unwrap_or_else(|e| panic!("corrected step {s}: {e}"));
        let n = |c: &CouplerSolidContact| match c
            .solid
            .solver
            .non_linear_solver(None)
            .unwrap()
            .get_param("nb_iterations")
            .unwrap()
        {
            akantu_parser::param::ParamValue::Int(i) => i,
            _ => unreachable!(),
        };
        it_up += n(&up);
        it_co += n(&co);
        assert_eq!(
            up.contact.contact_elements().len(),
            co.contact.contact_elements().len(),
            "step {s}: active sets"
        );
        let uu_g = up.displacement().unwrap();
        let uu = uu_g.as_slice();
        let uc_g = co.displacement().unwrap();
        let uc = uc_g.as_slice();
        let scale = uu.iter().fold(0.0f64, |m, x| m.max(x.abs()));
        let diff = uu
            .iter()
            .zip(uc)
            .fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
        worst = worst.max(diff / scale);
    }
    eprintln!(
        "max relative displacement difference {worst:e}; Newton iterations upstream {it_up}, \
         corrected {it_co}"
    );
    // PROPOSED rule (docs/NUMERICAL_VALIDATION.md): two Newton
    // solutions of the same discrete equations (UD-368 changes only the
    // tangent; with a rigid master and one contact element per slave node
    // the UD-002 assembly modes give the same forces) differ by at most the
    // solver criterion (default: solution increment norm < 1e-10 absolute,
    // displacements ~1e-2: 1e-8 relative); asserted 100x tighter.
    assert!(
        worst <= 1e-10,
        "corrected and upstream converged states differ: {worst:e}"
    );
}

/// Dense copy of the DOF manager's matrix `id` (COO entries summed,
/// symmetric storage mirrored).
fn dense(c: &CouplerSolidContact, id: &str) -> (usize, Vec<f64>) {
    let k = c.solid.solver.dof_manager.matrix(id).unwrap();
    let n = usize::try_from(k.size()).unwrap();
    let mut d = vec![0.0; n * n];
    let sym = k.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
    for ((i, j), v) in k
        .irn_one_based()
        .into_iter()
        .zip(k.jcn_one_based())
        .zip(k.values())
    {
        let (i, j) = (
            usize::try_from(i - 1).unwrap(),
            usize::try_from(j - 1).unwrap(),
        );
        d[i * n + j] += v;
        if sym && i != j {
            d[j * n + i] += v;
        }
    }
    (n, d)
}

/// TEST-216 (`test_coupled_stiffness`, same intent: the coupled solid +
/// contact tangent; upstream's driver uses APIs that no longer exist:
/// `_implicit_contact`, `search(displacement)`, `getContactMap`). After
/// ten static Hertz steps (contact active), the coupler's `K` equals the
/// solid's `K` plus the contact stiffness assembled alone into the same DOF
/// numbering, to the rounding of the reassociated additions: each entry is
/// a sum of a few element contributions `k_e`, whose reassociation error is
/// bounded by a small multiple of `ε Σ|k_e|`; the asserted
/// `|ΔK| ≤ 8ε (|K_s| + |K_c|)` (PROPOSED) uses the summed entries instead,
/// which is never larger (`|K_s| + |K_c| ≤ Σ|k_e|`; cancelling contact
/// contributions make it smaller), i.e. a stricter test that holds here.
/// The
/// contact moduli are checked against finite differences of the contact
/// forces in akantu-contact (`contact_stiffness.rs`), the solid stiffness
/// in akantu-solid.
#[test]
fn coupled_stiffness_is_solid_plus_contact() {
    let mut c = hertz_coupler(ContactUpstreamCompat::default());
    for _ in 0..10 {
        hertz_step(&mut c).unwrap();
    }
    assert!(!c.contact.contact_elements().is_empty());
    c.assemble_stiffness_matrix().unwrap();
    let (n, kc) = dense(&c, "K");
    c.solid.assemble_stiffness_matrix(true).unwrap();
    let (_, ks) = dense(&c, "K");
    // the contact stiffness alone, into a zeroed K of the same DOF manager
    {
        let akantu_solid::SolidMechanicsModel { solver, .. } = &mut c.solid;
        let contact = &mut c.contact;
        solver
            .dof_manager
            .collective("test:contact_stiffness", |dm| {
                dm.zero_matrix("K")?;
                contact.assemble_stiffness_matrix(dm)?;
                dm.finalize_matrix("K")
            })
            .unwrap();
    }
    let (_, kk) = dense(&c, "K");
    let mut nonzero_contact = 0;
    for i in 0..n * n {
        if kk[i] != 0.0 {
            nonzero_contact += 1;
        }
        let bound = 8. * f64::EPSILON * (ks[i].abs() + kk[i].abs());
        assert!(
            (kc[i] - (ks[i] + kk[i])).abs() <= bound,
            "entry {i}: {} vs {} + {}",
            kc[i],
            ks[i],
            kk[i]
        );
    }
    assert!(nonzero_contact > 0);
}

/// UD-622 (CPL-004, latent upstream): `CohesiveContactSolverCallback`
/// assembles the solid stiffness with `need_to_reassemble = false`; with a
/// solid whose laws all report an unchanged stiffness (elastic only) `K`
/// would not be zeroed and every assembly would add the contact moduli once
/// more. Upstream's callback only takes a `SolidMechanicsModelCohesive`,
/// whose cohesive laws always report a changed stiffness (so it always
/// reassembles: no reachable effect, `cohesive_static.rs`
/// `ud622_has_no_reachable_effect_with_cohesive_laws`); the mechanism is
/// shown here with the plain SMM physics as the solid part — a
/// configuration upstream's callback cannot take.
#[test]
fn ud622_latent_accumulation_with_a_plain_solid_part() {
    use akantu_couplers::contact::{CallbackFlavour, SolidContactCallback};
    use akantu_solver::SolverCallback;
    for upstream_stiffness in [false, true] {
        let mut c = hertz_coupler(ContactUpstreamCompat::default());
        for _ in 0..10 {
            hertz_step(&mut c).unwrap();
        }
        let mut snapshots = Vec::new();
        for _ in 0..2 {
            {
                let akantu_solid::SolidMechanicsModel {
                    physics, solver, ..
                } = &mut c.solid;
                let mut cb = SolidContactCallback::new(
                    physics,
                    &mut c.contact,
                    AnalysisMethod::Static,
                    CallbackFlavour::StandAlone { upstream_stiffness },
                    akantu_couplers::SolidContactCompat::default(),
                );
                solver
                    .dof_manager
                    .collective("test:K", |dm| {
                        cb.assemble_matrix(dm, "K")?;
                        dm.finalize_matrix("K")
                    })
                    .unwrap();
            }
            snapshots.push(dense(&c, "K").1);
        }
        let same = snapshots[0] == snapshots[1];
        if upstream_stiffness {
            assert!(!same, "upstream mode: the contact moduli should accumulate");
        } else {
            assert!(same, "corrected mode: K must be reassembled");
        }
    }
}

/// The coupler's split residual (upstream `assembleResidual(part)`,
/// consumed by the `linear` non-linear solver): `external` = external
/// force + contact force, `internal` = the stored solid internal force (not
/// recomputed), any other part refused; the stand-alone flavour does not
/// split. After ten static Hertz steps the contact force is non-zero, so a
/// part without it is detected (mutant M12 dies).
#[test]
fn split_residual_parts() {
    use akantu_couplers::SolidContactCallback;
    use akantu_couplers::contact::CallbackFlavour;
    use akantu_solid::model::DOF_ID;
    use akantu_solver::SolverCallback;
    let mut c = hertz_coupler(ContactUpstreamCompat::default());
    for _ in 0..10 {
        hertz_step(&mut c).unwrap();
    }
    assert!(
        c.contact
            .internal_force()
            .as_slice()
            .iter()
            .any(|&v| v != 0.0)
    );
    let f_ext = c.solid.external_force().unwrap().clone();
    let f_int = c.solid.internal_force().unwrap().clone();
    let f_c = c.contact.internal_force().clone();
    let akantu_solid::SolidMechanicsModel {
        physics, solver, ..
    } = &mut c.solid;
    let dm = &mut solver.dof_manager;
    let mut cb = SolidContactCallback::coupler(physics, &mut c.contact, AnalysisMethod::Static);
    assert!(cb.can_split_residual());
    let mut part = |cb: &mut SolidContactCallback<'_>, name: &str| {
        dm.zero_residual();
        cb.assemble_residual_part(dm, name)
            .map(|()| dm.residual().values().to_vec())
    };
    let external = part(&mut cb, "external").unwrap();
    let internal = part(&mut cb, "internal").unwrap();
    assert!(part(&mut cb, "inertial").is_err(), "unknown part accepted");
    dm.zero_residual();
    dm.assemble_to_residual(DOF_ID, f_ext.view(), 1.0).unwrap();
    dm.assemble_to_residual(DOF_ID, f_c.view(), 1.0).unwrap();
    assert_eq!(
        external,
        dm.residual().values(),
        "external = f_ext + f_contact"
    );
    dm.zero_residual();
    dm.assemble_to_residual(DOF_ID, f_int.view(), 1.0).unwrap();
    assert_eq!(internal, dm.residual().values(), "internal = f_int");
    dm.zero_residual();
    dm.assemble_to_residual(DOF_ID, f_ext.view(), 1.0).unwrap();
    assert_ne!(
        external,
        dm.residual().values(),
        "the contact force is part of external"
    );
    cb.flavour = CallbackFlavour::StandAlone {
        upstream_stiffness: false,
    };
    assert!(!cb.can_split_residual());
}

/// The contact couplers run on the default DOF manager
/// only; a `dof_manager_type` other than `default` in the coupler's section
/// is refused (it used to be ignored), `default` is accepted.
#[test]
fn contact_coupler_refuses_another_dof_manager_type() {
    for (t, ok) in [("default", true), ("petsc", false)] {
        let mesh = read_mesh(
            &frozen_mesh("examples/c++/contact_mechanics_model/hertz/hertz.msh"),
            2,
        );
        let text =
            format!("{HERTZ_DAT}\nmodel coupler_solid_contact [\n  dof_manager_type = {t}\n]\n");
        let parser = Parser::parse_str(&text, "material.dat").unwrap();
        let r = CouplerSolidContact::with_parser(
            SharedMesh::new(mesh),
            None,
            "coupler_solid_contact",
            parser,
        );
        match (r, ok) {
            (Ok(_), true) => {}
            (Err(akantu_core::Error::Unsupported(m)), false) => {
                assert!(m.contains("default DOF manager only"), "{m}");
            }
            (r, _) => panic!("dof_manager_type = {t}: {:?}", r.err()),
        }
    }
}
