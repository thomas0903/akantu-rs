// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests:
// UD-224 (cohesive
// insertion into a linear-family law without elastic branch outside the
// extrinsic insertion check, refused by default, compat switch) and UD-241
// (converged `δ_max` history; explicit dynamics unaffected).
// Upstream behaviour from solid_mechanics_model_cohesive.cc
// (initConstitutiveLaws, insertIntrinsicElements),
// material_cohesive_linear{.cc,_inline_impl.hh} (sigma_c_eff, the traction)
// and material_cohesive_linear_uncoupled.cc. The implicit evidence of UD-241
// (path independence, iterate overshoot) is in `cohesive_fixtures.rs`.

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_validation::reduce::NanMinMax;
use common::{Fixture, MATERIAL_0, MATERIAL_1};

const NG: GhostType = GhostType::NotGhost;
const MESH: &str = "_cohesive_2d_4_triangle_3.msh";
const COH: ElementType = ElementType::Cohesive2d4;

/// The linear-family laws without an elastic branch (UD-224).
const ZERO_STRENGTH_LAWS: [&str; 4] = [
    "cohesive_linear",
    "cohesive_linear_fatigue",
    "cohesive_linear_friction",
    "cohesive_linear_uncoupled",
];

/// `MATERIAL_0` with the cohesive law `law` (named `insertion`).
fn dat(law: &str) -> String {
    MATERIAL_0.replace("material cohesive_linear [", &format!("material {law} ["))
}

fn build(mesh: SharedMesh, dat: &str) -> SolidMechanicsModelCohesive {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    SolidMechanicsModelCohesive::with_parser(mesh, None, "solid_mechanics_model_cohesive", parser)
        .unwrap()
}

fn explicit() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)
}

fn nb_flagged(model: &SolidMechanicsModelCohesive) -> usize {
    model
        .ext
        .inserter
        .insertion_facets()
        .as_map()
        .iter_all()
        .map(|(_, _, a)| a.as_slice().iter().filter(|f| **f).count())
        .sum()
}

/// UD-224: an intrinsic `init_full` that would insert cohesive
/// elements into a linear-family law without an elastic branch is refused,
/// naming the law and the remedy; nothing is inserted and the mesh is
/// unchanged. Upstream inserts them with `σc,eff = 0` (no strength).
#[test]
fn co2_01_intrinsic_linear_family_laws_are_refused() {
    for law in ZERO_STRENGTH_LAWS {
        let mesh = common::read_mesh(&common::cohesive_mesh(MESH), 2);
        let mut model = build(mesh.clone(), &dat(law));
        let revision = mesh.revision().unwrap();
        let e = model
            .init_full(&explicit(), false)
            .expect_err(law)
            .to_string();
        assert!(
            e.contains("UD-224")
                && e.contains(&format!("'insertion' ({law}, 5 element(s))"))
                && e.contains("cohesive_bilinear")
                && e.contains("intrinsic_linear_zero_strength"),
            "{law}: {e}"
        );
        assert_eq!(mesh.revision().unwrap(), revision, "{law}: mesh changed");
        assert_eq!(mesh.read().unwrap().nb_element(COH, NG), 0, "{law}");
        assert_eq!(nb_flagged(&model), 0, "{law}: insertion flags left set");
    }
}

/// The laws with strength are accepted intrinsically —
/// `cohesive_bilinear` (σc,eff drawn from σc, elastic branch up to δ₀) and
/// `cohesive_exponential` (σc).
#[test]
fn co2_01_intrinsic_bilinear_and_exponential_are_accepted() {
    for (law, extra) in [
        ("cohesive_bilinear", "    delta_0 = 1e-7\n"),
        ("cohesive_exponential", "    delta_c = 2e-5\n"),
    ] {
        let mesh = common::read_mesh(&common::cohesive_mesh(MESH), 2);
        let d = dat(law).replace("    G_c = 10\n", &format!("    G_c = 10\n{extra}"));
        let d = if law == "cohesive_exponential" {
            d.replace("    G_c = 10\n", "")
        } else {
            d
        };
        let mut model = build(mesh.clone(), &d);
        model.init_full(&explicit(), false).unwrap();
        assert_eq!(mesh.read().unwrap().nb_element(COH, NG), 5, "{law}");
    }
}

/// The same refusal on an extrinsic model for insertions outside
/// `check_cohesive_stress` — `insert_intrinsic_elements` and a manual
/// `insert_elements` of user-flagged facets. Nothing changes; the flags set
/// by `insert_intrinsic_elements` are restored, so the extrinsic run
/// afterwards inserts exactly the elements of upstream's mode-I scenario.
#[test]
fn co2_01_extrinsic_model_refuses_insertion_outside_the_check() {
    let _g = common::lock();
    let mut f = Fixture::create(MESH, 2, COH, MATERIAL_0, true);
    let revision = f.mesh.revision().unwrap();
    let e = f
        .model
        .insert_intrinsic_elements()
        .expect_err("refused")
        .to_string();
    assert!(
        e.contains("UD-224") && e.contains("'insertion' (cohesive_linear"),
        "{e}"
    );
    assert_eq!(f.mesh.revision().unwrap(), revision);
    assert_eq!(f.nb_cohesive(), 0);
    assert_eq!(
        nb_flagged(&f.model),
        0,
        "flags of the refused pass left set"
    );

    // a user-flagged facet
    let facet = {
        let check = f
            .model
            .ext
            .inserter
            .check_facets()
            .get(ElementType::Segment2, NG)
            .unwrap();
        check.as_slice().iter().position(|c| *c).unwrap()
    };
    f.model
        .ext
        .inserter
        .insertion_facets_mut()
        .get_mut(ElementType::Segment2, NG)
        .unwrap()
        .as_mut_slice()[facet] = true;
    let e = f.model.insert_elements().expect_err("refused").to_string();
    assert!(e.contains("UD-224"), "{e}");
    assert_eq!(f.mesh.revision().unwrap(), revision);
    assert_eq!(f.nb_cohesive(), 0);
    // the user's flag is the user's to clear (insert_elements changes nothing)
    assert_eq!(nb_flagged(&f.model), 1);
    f.model.ext.inserter.insertion_facets_mut().fill(false);

    // upstream's ExtrinsicModeI scenario (test_cohesive.cc) afterwards
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * (1. - 1e-5)).collect();
    f.set_initial_condition(&init);
    let inc: Vec<Real> = strain.iter().map(|s| s * 2e-2).collect();
    f.steps(&inc, |_, _, _| {});
    assert_eq!(f.nb_cohesive(), f.group_size);
}

/// `CohesiveUpstreamCompat::intrinsic_linear_zero_strength`
/// reproduces upstream: the intrinsic `cohesive_linear` elements are
/// inserted with `σc,eff = 0` and carry no traction when the interface
/// opens, where `cohesive_bilinear` resists.
#[test]
fn co2_01_compat_reproduces_the_zero_strength_interface() {
    let _g = common::lock();
    let traction_norm = |f: &mut Fixture| -> (Real, Real) {
        // a mode-I opening of δc/4 (δc = 2 G_c/σc = 2e-5): the second side
        // of every cohesive element (local nodes 2, 3) moved by 5e-6 against
        // the element's normal (computed at the undeformed state first; the
        // normal points from the second side to the first)
        f.model.assemble_internal_forces().unwrap();
        let normals = f
            .model
            .cohesive_material("insertion")
            .unwrap()
            .array("normal", COH, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        let nq = normals.len() / 2 / f.nb_cohesive();
        let conn: Vec<i64> = f
            .mesh
            .read()
            .unwrap()
            .connectivity(COH, NG)
            .unwrap()
            .as_slice()
            .iter()
            .map(|&n| i64::from(n))
            .collect();
        {
            let mut u_g = f.model.smm.displacement_mut().unwrap();
            let u = u_g.as_mut_slice();
            for (k, e) in conn.chunks(4).enumerate() {
                let nrm = &normals[2 * k * nq..2 * k * nq + 2];
                for &n in &e[2..] {
                    u[2 * n as usize] = -5e-6 * nrm[0];
                    u[2 * n as usize + 1] = -5e-6 * nrm[1];
                }
            }
        }
        f.model.assemble_internal_forces().unwrap();
        let mat = f.model.cohesive_material("insertion").unwrap();
        let t_g = mat.array("tractions", COH, NG).unwrap();
        let t = t_g.as_slice();
        let o_g = mat.array("opening", COH, NG).unwrap();
        let o = o_g.as_slice();
        (
            t.iter().fold(0.0_f64, |m, v| m.nan_max(v.abs())),
            o.iter().fold(0.0_f64, |m, v| m.nan_max(v.abs())),
        )
    };
    let mut f = Fixture::create_with(MESH, 2, COH, MATERIAL_0, false, |m| {
        m.ext.compat.intrinsic_linear_zero_strength = true;
    });
    assert_eq!(f.nb_cohesive(), f.group_size);
    {
        let mat = f.model.cohesive_material("insertion").unwrap();
        let sc_g = mat.array("sigma_c_eff", COH, NG).unwrap();
        let sc = sc_g.as_slice();
        assert!(!sc.is_empty() && sc.iter().all(|v| *v == 0.0), "{sc:?}");
    }
    let (t, o) = traction_norm(&mut f);
    assert!(o > 0.0, "the interface must open");
    assert_eq!(t, 0.0, "zero-strength interface (upstream)");

    let mut b = Fixture::create(MESH, 2, COH, MATERIAL_1, false);
    let (tb, ob) = traction_norm(&mut b);
    assert!(ob > 0.0 && tb > 0.0, "bilinear resists: {tb:e}");
}

/// The state compared by [`co2_03_explicit_dynamics_is_unaffected`]: nodal
/// fields and every cohesive internal of the law `insertion`.
fn explicit_state(f: &Fixture) -> Vec<(String, Vec<Real>)> {
    let m = &f.model;
    let mut v = vec![
        (
            "displacement".to_owned(),
            m.smm.displacement().unwrap().as_slice().to_vec(),
        ),
        (
            "velocity".to_owned(),
            m.smm.velocity().unwrap().as_slice().to_vec(),
        ),
    ];
    let mat = m.cohesive_material("insertion").unwrap();
    for name in [
        "opening",
        "tractions",
        "contact_tractions",
        "contact_opening",
        "delta max",
        "damage",
        "reversible_energy",
        "total_energy",
        "delta_n_max",
        "delta_t_max",
        "damage_n",
        "damage_t",
    ] {
        if let Ok(a) = mat.array(name, COH, NG) {
            v.push((name.to_owned(), a.as_slice().to_vec()));
        }
    }
    v
}

/// UD-241: explicit dynamics evaluates the traction once per step,
/// after the history is saved (`beforeSolveStep`), so the converged-history
/// default and upstream's per-iterate update give **bitwise** the same run:
/// upstream's mode-I scenario followed by unloading into compression, for
/// the intrinsic `cohesive_bilinear` (δ₀ floor) and the extrinsic
/// `cohesive_linear` and `cohesive_linear_uncoupled` (`δn,max`, `δt,max`).
/// (The explicit oracle replays of `cohesive_fixtures.rs` run the default
/// and are bitwise against upstream too.)
#[test]
fn co2_03_explicit_dynamics_is_unaffected() {
    let _g = common::lock();
    let uncoupled = dat("cohesive_linear_uncoupled");
    for (dat, extrinsic) in [
        (MATERIAL_1, false),
        (MATERIAL_0, true),
        (uncoupled.as_str(), true),
    ] {
        let run = |per_iterate: bool| {
            let mut f = Fixture::create_with(MESH, 2, COH, dat, extrinsic, |m| {
                m.ext.compat.delta_max_per_iterate = per_iterate;
            });
            let strain = f.mode_i_strain();
            let init: Vec<Real> = strain.iter().map(|s| s * (1. - 1e-5)).collect();
            f.set_initial_condition(&init);
            let inc: Vec<Real> = strain.iter().map(|s| s * 2e-2).collect();
            let n = f.nb_steps;
            f.nb_steps = n / 2;
            f.steps(&inc, |_, _, _| {});
            let back: Vec<Real> = inc.iter().map(|s| -2. * s).collect();
            f.steps(&back, |_, _, _| {});
            assert!(f.nb_cohesive() > 0, "the scenario inserts");
            explicit_state(&f)
        };
        let corrected = run(false);
        let upstream = run(true);
        // the interface opens (uncoupled: its own maximal normal opening)
        let opened = corrected
            .iter()
            .filter(|(n, _)| n == "delta max" || n == "delta_n_max")
            .flat_map(|(_, v)| v.iter())
            .fold(0.0_f64, |m, v| m.nan_max(*v));
        assert!(opened > 0.0, "extrinsic {extrinsic}: the interface opens");
        for ((name, a), (_, b)) in corrected.iter().zip(&upstream) {
            assert_eq!(a.len(), b.len(), "{name}");
            let differ = a
                .iter()
                .zip(b)
                .filter(|(x, y)| !akantu_validation::exact_eq(**x, **y)) // D-046
                .count();
            assert_eq!(
                differ, 0,
                "explicit {name} (extrinsic {extrinsic}): {differ} values differ"
            );
        }
    }
}
