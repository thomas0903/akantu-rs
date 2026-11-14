// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests:
// before the fix the contact branch of the corrected UD-241 default
// read the previous trial's damage, an evaluation outside a solve step
// forgot the last converged maximal opening, and the UD-224 refusal predicted the cohesive law
// from the facet's law instead of the actual selection. Upstream behaviour
// from material_cohesive_linear{.cc,_inline_impl.hh},
// material_cohesive_bilinear.cc, material_cohesive_linear_uncoupled.cc,
// material_cohesive_exponential.cc, material.cc (savePreviousState in
// beforeSolveStep) and solid_mechanics_model_cohesive.cc
// (insertElements → onElementsAdded → material selection). The point-level
// tangent checks are in `laws::linear` / `laws::others` unit tests.

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{Element, ElementKind, ElementType, GhostType, Real};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawPhase, SelectorChain, SelectorContext,
};
use akantu_parser::Parser;
use common::{Fixture, MATERIAL_0, MATERIAL_1};

const NG: GhostType = GhostType::NotGhost;
const MESH: &str = "_cohesive_2d_4_triangle_3.msh";
const COH: ElementType = ElementType::Cohesive2d4;

/// `MATERIAL_0` with the cohesive law `law` (named `insertion`) and the
/// extra parameter lines `extra`.
fn dat0(law: &str, extra: &str) -> String {
    MATERIAL_0.replace(
        "material cohesive_linear [\n    name = insertion\n",
        &format!("material {law} [\n    name = insertion\n{extra}"),
    )
}

/// `MATERIAL_1` (`cohesive_bilinear`) with the extra parameter lines.
fn dat1(extra: &str) -> String {
    MATERIAL_1.replace(
        "material cohesive_bilinear [\n    name = insertion\n",
        &format!("material cohesive_bilinear [\n    name = insertion\n{extra}"),
    )
}

/// The intrinsic `cohesive_exponential` (σc = 1e6, δc = 2e-5).
fn dat_exponential() -> String {
    MATERIAL_0.replace(
        "material cohesive_linear [\n    name = insertion\n    beta = 1\n    G_c = 10\n",
        "material cohesive_exponential [\n    name = insertion\n    beta = 1\n    delta_c = 2e-5\n",
    )
}

/// Every displacement reset, the second side (local nodes
/// 2, 3) of every cohesive element moved by `−delta` in y — a normal opening
/// `delta` of this frozen mesh's horizontal interface.
fn impose(model: &mut SolidMechanicsModelCohesive, delta: Real) {
    model.smm.displacement_mut().unwrap().fill(0.0);
    let conn = model
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(COH, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let mut u_g = model.smm.displacement_mut().unwrap();
    let u = u_g.as_mut_slice();
    for e in conn.chunks(4) {
        for &n in &e[2..] {
            u[n as usize * 2 + 1] = -delta;
        }
    }
}

/// A cohesive internal of the law `insertion`.
fn field(model: &SolidMechanicsModelCohesive, name: &str) -> Vec<Real> {
    model
        .cohesive_material("insertion")
        .unwrap()
        .array(name, COH, NG)
        .unwrap()
        .as_slice()
        .to_vec()
}

/// The quantities a trial evaluation produces (law `insertion`).
fn trial_state(model: &SolidMechanicsModelCohesive) -> Vec<(&'static str, Vec<Real>)> {
    let mat = model.cohesive_material("insertion").unwrap();
    [
        "opening",
        "tractions",
        "contact_tractions",
        "contact_opening",
        "delta max",
        "damage",
        "delta_n_max",
        "delta_t_max",
        "damage_n",
        "damage_t",
    ]
    .into_iter()
    .filter_map(|n| {
        mat.array(n, COH, NG)
            .ok()
            .map(|a| (n, a.as_slice().to_vec()))
    })
    .collect()
}

fn assert_bitwise(what: &str, a: &[(&'static str, Vec<Real>)], b: &[(&'static str, Vec<Real>)]) {
    assert_eq!(a.len(), b.len(), "{what}");
    for ((name, x), (_, y)) in a.iter().zip(b) {
        assert_eq!(x.len(), y.len(), "{what}: {name}");
        let differ: Vec<(usize, Real, Real)> = x
            .iter()
            .zip(y)
            .enumerate()
            .filter(|(_, (p, q))| !akantu_validation::exact_eq(**p, **q)) // D-046
            .map(|(i, (p, q))| (i, *p, *q))
            .take(4)
            .collect();
        assert!(differ.is_empty(), "{what}: {name} differs: {differ:?}");
    }
}

/// An extrinsic model of `dat` whose five interface facets are opened by
/// the insertion check (uniform mode-I stress 1.1 σc): the elements carry
/// the strength the check prepared.
fn extrinsic_inserted(
    dat: &str,
    configure: impl FnOnce(&mut SolidMechanicsModelCohesive),
) -> Fixture {
    let mut f = Fixture::create_with(MESH, 2, COH, dat, true, configure);
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * 1.1).collect();
    f.set_initial_condition(&init);
    f.model.assemble_internal_forces().unwrap();
    let n = f.model.check_cohesive_stress().unwrap();
    assert_eq!(n, f.group_size, "the check opens the interface");
    f.model.smm.displacement_mut().unwrap().fill(0.0);
    f
}

// ----------------------------------------------------------------

/// `cohesive_bilinear`, penalty 1e12:
/// a trial past complete failure (Δn = +24e-6 > δc) followed by a
/// compression trial (Δn = −4e-6) with the same saved history. The contact
/// branch is decided from the committed history: the compression takes the
/// penalty branch (contact traction Δn·penalty, no cohesive traction) and
/// every trial gives bitwise the result of the direct compression of a
/// fresh model, whatever the trials before it. Before the fix the first
/// compression after the overshoot read the trial's damage 1, suppressed the
/// contact and treated the compression as cohesive opening (−804,020.1
/// cohesive, 0 contact). With `contact_after_breaking = true` the contact
/// never depends on the damage (control).
#[test]
fn co3_01_overshoot_past_failure_then_compression_takes_the_contact_branch() {
    let _g = common::lock();
    for cab in [false, true] {
        let extra = if cab {
            "    penalty = 1e12\n    contact_after_breaking = true\n"
        } else {
            "    penalty = 1e12\n"
        };
        let dat = dat1(extra);
        let mut direct = Fixture::create(MESH, 2, COH, &dat, false);
        impose(&mut direct.model, -4e-6);
        direct.model.assemble_internal_forces().unwrap();
        let expected = trial_state(&direct.model);
        let t = field(&direct.model, "tractions");
        let c = field(&direct.model, "contact_tractions");
        assert_eq!(t.len(), 20, "ten interface points");
        for q in 0..10 {
            assert_eq!(t[2 * q + 1], 0.0, "cab {cab}: point {q}: cohesive traction");
            assert!(
                (c[2 * q + 1] + 4e6).abs() <= 1e-9 * 4e6,
                "cab {cab}: point {q}: contact traction {}",
                c[2 * q + 1]
            );
        }

        let mut f = Fixture::create(MESH, 2, COH, &dat, false);
        impose(&mut f.model, 24e-6);
        f.model.assemble_internal_forces().unwrap();
        assert!(
            field(&f.model, "damage").iter().all(|d| *d == 1.0),
            "cab {cab}: the overshooting trial breaks every point"
        );
        for k in 0..2 {
            impose(&mut f.model, -4e-6);
            f.model.assemble_internal_forces().unwrap();
            assert_bitwise(
                &format!("cab {cab}: compression {k} after the overshoot"),
                &trial_state(&f.model),
                &expected,
            );
        }
    }
}

/// Every trial evaluation is a function of the committed
/// history and the present opening only — evaluating the same openings in
/// two different orders gives bitwise the same results, for
/// `cohesive_bilinear` (intrinsic), `cohesive_linear` and
/// `cohesive_linear_uncoupled` (extrinsic, strength from the insertion
/// check), with both `contact_after_breaking` settings. The sequence crosses
/// complete failure and compression.
#[test]
fn co3_01_trials_do_not_depend_on_earlier_trials() {
    let _g = common::lock();
    let openings = [24e-6, -4e-6, 6e-6, -4e-6, 2e-6, -1e-6, 0.0, 30e-6];
    for cab in [false, true] {
        let extra = if cab {
            "    penalty = 1e12\n    contact_after_breaking = true\n"
        } else {
            "    penalty = 1e12\n"
        };
        let cases: [(&str, String, bool); 3] = [
            ("cohesive_bilinear", dat1(extra), false),
            ("cohesive_linear", dat0("cohesive_linear", extra), true),
            (
                "cohesive_linear_uncoupled",
                dat0("cohesive_linear_uncoupled", extra),
                true,
            ),
        ];
        for (law, dat, extrinsic) in cases {
            let mut f = if extrinsic {
                extrinsic_inserted(&dat, |_| {})
            } else {
                Fixture::create(MESH, 2, COH, &dat, false)
            };
            let mut first = Vec::new();
            for &d in &openings {
                impose(&mut f.model, d);
                f.model.assemble_internal_forces().unwrap();
                first.push(trial_state(&f.model));
            }
            for (k, &d) in openings.iter().enumerate().rev() {
                impose(&mut f.model, d);
                f.model.assemble_internal_forces().unwrap();
                assert_bitwise(
                    &format!("{law} (cab {cab}): opening {d:e}"),
                    &trial_state(&f.model),
                    &first[k],
                );
            }
        }
    }
}

// ----------------------------------------------------------------

/// The maximal opening of the law (`δ_max`, uncoupled `δn,max`).
fn max_name(law: &str) -> &'static str {
    if law == "cohesive_linear_uncoupled" {
        "delta_n_max"
    } else {
        "delta max"
    }
}

/// One explicit lumped step with every DOF blocked at the imposed opening.
fn blocked_step(model: &mut SolidMechanicsModelCohesive, delta: Real) {
    impose(model, delta);
    model.smm.blocked_dofs_mut().unwrap().fill(true);
    model.solve_step(None).unwrap();
}

/// Load to Δn = 5e-6 in a
/// converged (explicit) step, unload to 2e-6 with the public
/// `assemble_internal_forces` outside a step, overshoot to 10e-6 outside a
/// step, then reload to 4e-6 and to 6e-6 in converged steps. The converged
/// maximum survives the standalone unloading (it healed to 2e-6 before the
/// fix) and the standalone overshoot is a trial that the next converged
/// step does not commit; the reloading steps follow the committed maximum
/// (5e-6, then 6e-6). For `cohesive_bilinear` (intrinsic), `cohesive_linear`
/// and `cohesive_linear_uncoupled` (extrinsic) and `cohesive_exponential`
/// (intrinsic; upstream's converged-history law has the same lifecycle
/// defect). `delta_max_per_iterate` keeps the converged maximum at the
/// standalone unloading too (upstream's monotone per-evaluation update,
/// control case).
#[test]
fn co3_02_standalone_evaluations_keep_the_converged_maximum() {
    let _g = common::lock();
    let cases: [(&str, String, bool); 4] = [
        ("cohesive_bilinear", MATERIAL_1.to_owned(), false),
        ("cohesive_linear", MATERIAL_0.to_owned(), true),
        (
            "cohesive_linear_uncoupled",
            dat0("cohesive_linear_uncoupled", ""),
            true,
        ),
        ("cohesive_exponential", dat_exponential(), false),
    ];
    for (law, dat, extrinsic) in cases {
        for per_iterate in [false, true] {
            if per_iterate && law == "cohesive_exponential" {
                continue;
            }
            let configure = |m: &mut SolidMechanicsModelCohesive| {
                m.ext.compat.delta_max_per_iterate = per_iterate;
            };
            let mut f = if extrinsic {
                extrinsic_inserted(&dat, configure)
            } else {
                Fixture::create_with(MESH, 2, COH, &dat, false, configure)
            };
            let name = max_name(law);
            let what = format!("{law} (per iterate {per_iterate})");
            blocked_step(&mut f.model, 5e-6);
            let converged = field(&f.model, name);
            assert!(
                converged.iter().all(|v| (v - 5e-6).abs() <= 1e-12),
                "{what}: converged maximum {converged:?}"
            );
            let damage_converged = trial_state(&f.model);
            // standalone unloading
            impose(&mut f.model, 2e-6);
            f.model.assemble_internal_forces().unwrap();
            assert_eq!(
                field(&f.model, name),
                converged,
                "{what}: a standalone unloading healed the converged maximum"
            );
            if per_iterate {
                continue;
            }
            // the same unloading evaluated again: identical
            let unloaded = trial_state(&f.model);
            f.model.assemble_internal_forces().unwrap();
            assert_bitwise(&what, &trial_state(&f.model), &unloaded);
            // a standalone overshoot is a trial, not committed by the next step
            impose(&mut f.model, 10e-6);
            f.model.assemble_internal_forces().unwrap();
            assert!(
                field(&f.model, name).iter().all(|v| *v >= 9e-6),
                "{what}: the standalone trial opens to 10e-6"
            );
            blocked_step(&mut f.model, 4e-6);
            assert_eq!(
                field(&f.model, name),
                converged,
                "{what}: reloading step below the committed maximum"
            );
            blocked_step(&mut f.model, 5e-6);
            assert_bitwise(
                &format!("{what}: back at the converged opening"),
                &trial_state(&f.model),
                &damage_converged,
            );
            blocked_step(&mut f.model, 6e-6);
            let reloaded = field(&f.model, name);
            assert!(
                reloaded.iter().all(|v| (v - 6e-6).abs() <= 1e-12),
                "{what}: reloading step past the committed maximum: {reloaded:?}"
            );
        }
    }
}

/// Call a material hook of the law `insertion` as a solver would.
fn hook(model: &mut SolidMechanicsModelCohesive, phase: LawPhase, converged: Option<bool>) {
    let i = model.smm.material_index("insertion").unwrap();
    model
        .smm
        .with_material(i, phase, |law, ctx| match converged {
            None => law.before_solve_step(ctx),
            Some(c) => law.after_solve_step(ctx, c),
        })
        .unwrap();
}

/// Failed step and retry through the material hooks of
/// the solver lifecycle (`before_solve_step`, trial evaluations,
/// `after_solve_step(converged)`): a converged step at 5e-6; a failed step
/// whose trial overshoots to 9e-6 restores the committed maximum; the retry
/// converges at 6e-6; a later standalone unloading keeps 6e-6 (before the
/// fix it fell back to the step-entry baseline 5e-6).
#[test]
fn co3_02_failed_step_retry_and_standalone_unloading() {
    let _g = common::lock();
    let cases: [(&str, String, bool); 4] = [
        ("cohesive_bilinear", MATERIAL_1.to_owned(), false),
        ("cohesive_linear", MATERIAL_0.to_owned(), true),
        (
            "cohesive_linear_uncoupled",
            dat0("cohesive_linear_uncoupled", ""),
            true,
        ),
        ("cohesive_exponential", dat_exponential(), false),
    ];
    for (law, dat, extrinsic) in cases {
        let mut f = if extrinsic {
            extrinsic_inserted(&dat, |_| {})
        } else {
            Fixture::create(MESH, 2, COH, &dat, false)
        };
        let name = max_name(law);
        let m = &mut f.model;
        let step = |m: &mut SolidMechanicsModelCohesive, delta: Real, converged: bool| {
            hook(m, LawPhase::BeforeSolveStep, None);
            impose(m, delta);
            m.assemble_internal_forces().unwrap();
            hook(m, LawPhase::AfterSolveStep, Some(converged));
        };
        step(m, 5e-6, true);
        let committed = field(m, name);
        assert!(committed.iter().all(|v| (v - 5e-6).abs() <= 1e-12), "{law}");
        step(m, 9e-6, false);
        assert_eq!(field(m, name), committed, "{law}: failed step restores");
        step(m, 6e-6, true);
        let retried = field(m, name);
        assert!(
            retried.iter().all(|v| (v - 6e-6).abs() <= 1e-12),
            "{law}: retry {retried:?}"
        );
        impose(m, 2e-6);
        m.assemble_internal_forces().unwrap();
        assert_eq!(
            field(m, name),
            retried,
            "{law}: standalone unloading after the retry"
        );
    }
}

// ----------------------------------------------------------------

/// Selector: bulk elements → `bulk`, facets → `facets`,
/// cohesive elements → `cohesive(k)` (`k`: the element's index).
#[derive(Debug)]
struct ByKind {
    bulk: usize,
    facets: usize,
    cohesive: fn(usize) -> usize,
}

impl ConstitutiveLawSelector for ByKind {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> akantu_core::Result<Option<usize>> {
        Ok(Some(if el.kind() == ElementKind::Cohesive {
            (self.cohesive)(el.idx())
        } else if el.ty().spatial_dimension() < ctx.mesh.spatial_dimension() {
            self.facets
        } else {
            self.bulk
        }))
    }
}

/// `MATERIAL_1` (bulk 0, bilinear `insertion` 1) plus `cohesive_linear`
/// `zero` (index 2: no strength outside the insertion check, UD-224).
fn three_laws() -> String {
    let zero = "  material cohesive_linear [\n    name = zero\n    sigma_c = 1e6\n    G_c = 10\n    beta = 1\n  ]\n]\n";
    let base = MATERIAL_1.trim_end();
    format!("{}\n{zero}", &base[..base.len() - 1])
}

fn model_with(
    dat: &str,
    selector: ByKind,
) -> (akantu_mesh::SharedMesh, SolidMechanicsModelCohesive) {
    let mesh = common::read_mesh(&common::cohesive_mesh(MESH), 2);
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    model.set_material_selector_chain(SelectorChain {
        selectors: vec![Box::new(selector)],
        fallback_value: 0,
    });
    (mesh, model)
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

/// Facets → the bilinear law, cohesive
/// elements → `zero` (`cohesive_linear`). The actual planned assignment is
/// validated inside the insertion transaction: the intrinsic `init_full` is
/// refused (UD-224) naming `zero` and its five elements, the mesh is
/// unchanged and no insertion flag is left. Before the fix the refusal
/// looked at the facets' law (bilinear) and five zero-strength elements were
/// inserted (all ten `σc,eff` exactly 0).
#[test]
fn co3_03_custom_selector_cannot_bypass_the_zero_strength_refusal() {
    let _g = common::lock();
    let (mesh, mut model) = model_with(
        &three_laws(),
        ByKind {
            bulk: 0,
            facets: 1,
            cohesive: |_| 2,
        },
    );
    let revision = mesh.revision().unwrap();
    let e = model
        .init_full(&explicit(), false)
        .expect_err("refused")
        .to_string();
    assert!(
        e.contains("UD-224") && e.contains("'zero' (cohesive_linear, 5 element(s))"),
        "{e}"
    );
    assert_eq!(mesh.revision().unwrap(), revision, "mesh changed");
    assert_eq!(mesh.read().unwrap().nb_element(COH, NG), 0);
    assert_eq!(nb_flagged(&model), 0, "insertion flags left set");
}

/// Inverse mapping: facets → `zero`, cohesive elements →
/// the bilinear law. The elements carry strength (σc,eff from σc): accepted.
/// Before the fix the facet law (`zero`) was refused.
#[test]
fn co3_03_inverse_mapping_is_accepted() {
    let _g = common::lock();
    let (mesh, mut model) = model_with(
        &three_laws(),
        ByKind {
            bulk: 0,
            facets: 2,
            cohesive: |_| 1,
        },
    );
    model.init_full(&explicit(), false).unwrap();
    assert_eq!(mesh.read().unwrap().nb_element(COH, NG), 5);
    let sc = field(&model, "sigma_c_eff");
    assert_eq!(sc.len(), 10);
    assert!(sc.iter().all(|v| *v > 0.0), "{sc:?}");
}

/// Mixed laws: two of the five cohesive elements →
/// `zero`, the others → bilinear. Refused for the two, nothing inserted.
#[test]
fn co3_03_mixed_selection_is_refused_before_any_change() {
    let _g = common::lock();
    let (mesh, mut model) = model_with(
        &three_laws(),
        ByKind {
            bulk: 0,
            facets: 1,
            cohesive: |k| if k % 2 == 1 { 2 } else { 1 },
        },
    );
    let revision = mesh.revision().unwrap();
    let e = model
        .init_full(&explicit(), false)
        .expect_err("refused")
        .to_string();
    assert!(
        e.contains("'zero' (cohesive_linear, 2 element(s))") && !e.contains("'insertion'"),
        "{e}"
    );
    assert_eq!(mesh.revision().unwrap(), revision);
    assert_eq!(mesh.read().unwrap().nb_element(COH, NG), 0);
}

/// `MATERIAL_0` (bulk 0, `cohesive_linear` `insertion` 1, checked
/// extrinsically) plus a bilinear law `second` (index 2).
fn extrinsic_two_laws() -> String {
    let second = "  material cohesive_bilinear [\n    name = second\n    beta = 1\n    G_c = 10\n    sigma_c = 1e6\n    delta_0 = 1e-7\n  ]\n]\n";
    let base = MATERIAL_0.trim_end();
    format!("{}\n{second}", &base[..base.len() - 1])
}

/// The extrinsic model of [`extrinsic_two_laws`] with the selector, under
/// the mode-I stress 1.1 σc (stresses computed).
fn extrinsic_with(selector: ByKind) -> Fixture {
    let mesh = common::read_mesh(&common::cohesive_mesh(MESH), 2);
    let parser = Parser::parse_str(&extrinsic_two_laws(), "material.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    model.set_material_selector_chain(SelectorChain {
        selectors: vec![Box::new(selector)],
        fallback_value: 0,
    });
    model.init_full(&explicit(), true).unwrap();
    let mut f = Fixture {
        mesh,
        model,
        dim: 2,
        cohesive_type: COH,
        surface: 0.0,
        group_size: 5,
        nb_steps: 0,
        extrinsic: true,
        stable_time_step: 0.0,
        partition: Vec::new(),
    };
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * 1.1).collect();
    f.set_initial_condition(&init);
    f.model.assemble_internal_forces().unwrap();
    f
}

/// Manual insertion on an extrinsic model: a
/// user-flagged facet whose cohesive element the selector sends to the
/// strengthless `insertion` (`cohesive_linear`) is refused; sent to the
/// bilinear `second` it is accepted (the facet's own law is
/// `cohesive_linear`: the inverse mapping, falsely refused before the fix).
#[test]
fn co3_03_manual_insertion_validates_the_planned_law() {
    let _g = common::lock();
    for (target, refused) in [(1_usize, true), (2, false)] {
        let mut f = extrinsic_with(ByKind {
            bulk: 0,
            facets: 1,
            cohesive: if target == 1 { |_| 1 } else { |_| 2 },
        });
        let revision = f.mesh.revision().unwrap();
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
        let r = f.model.insert_elements();
        if refused {
            let e = r.expect_err("refused").to_string();
            assert!(
                e.contains("UD-224") && e.contains("'insertion' (cohesive_linear, 1 element(s))"),
                "{e}"
            );
            assert_eq!(f.mesh.revision().unwrap(), revision);
            assert_eq!(f.nb_cohesive(), 0);
        } else {
            assert_eq!(r.unwrap(), 1);
            assert_eq!(f.nb_cohesive(), 1);
            let mat = f.model.cohesive_material("second").unwrap();
            let sc_g = mat.array("sigma_c_eff", COH, NG).unwrap();
            let sc = sc_g.as_slice();
            assert_eq!(sc.len(), 2);
            assert!(sc.iter().all(|v| *v > 0.0), "{sc:?}");
        }
    }
}

/// Checked extrinsic path: the insertion check of the
/// facets' law (`insertion`, `cohesive_linear`) prepares the strength of the
/// new elements; a selector sending those elements to another law (`second`)
/// would leave them without the prepared parameters (and misalign the
/// checking law's arrays): refused before any change (UD-246), the check's
/// appended parameters and the flags restored. The default-like selection
/// (elements → the facets' law) inserts the five elements (control).
#[test]
fn co3_03_checked_insertion_requires_the_checking_law() {
    let _g = common::lock();
    for (target, refused) in [(2_usize, true), (1, false)] {
        let mut f = extrinsic_with(ByKind {
            bulk: 0,
            facets: 1,
            cohesive: if target == 1 { |_| 1 } else { |_| 2 },
        });
        let revision = f.mesh.revision().unwrap();
        let r = f.model.check_cohesive_stress();
        if refused {
            let e = r.expect_err("refused").to_string();
            assert!(e.contains("UD-246") && e.contains("'second'"), "{e}");
            assert_eq!(f.mesh.revision().unwrap(), revision);
            assert_eq!(f.nb_cohesive(), 0);
            assert_eq!(nb_flagged(&f.model), 0, "flags restored");
            let mat = f.model.cohesive_material("insertion").unwrap();
            assert_eq!(
                mat.array("sigma_c_eff", COH, NG).map_or(0, |a| a.len()),
                0,
                "the check's prepared strengths are rolled back"
            );
        } else {
            assert_eq!(r.unwrap(), 5);
            let sc = field(&f.model, "sigma_c_eff");
            assert_eq!(sc.len(), 10);
            assert!(sc.iter().all(|v| *v > 1e6), "{sc:?}");
        }
    }
}
