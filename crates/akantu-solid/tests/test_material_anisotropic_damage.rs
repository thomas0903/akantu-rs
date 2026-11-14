// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks
// of `anisotropic_damage` (src/model/solid_mechanics/materials/
// material_damage/material_anisotropic_damage*; no upstream test exists):
// the Desmorat-type proposal (rejected, kept as
// the opt-in `AnisotropicDamageModel::DesmoratExperimental`; the default is
// `Unsupported`), the upstream defects (UD-031/UD-058), and the `mv01_*`
// saturation counterexample.
// Tests requested (UD-031/UD-058): D = 0 recovers elasticity in
// every stress state, covariance, point-order independence, bounded damage
// eigenvalues, irreversibility, an energy inequality.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::{Element, GhostType, Real};
use akantu_model::constitutive_law::{InternalFieldMut, LawContext, LawPhase, LawSpec};
use akantu_parser::ParamValue;
use akantu_parser::ParameterRegistry;
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::anisotropic_damage::{
    AnisotropicDamageModel, MaterialAnisotropicDamage,
};
use akantu_validation::reduce::NanMinMax;
use common::{NG, SimplexModel, factory, materials_from_dat_with, simplex};
use timed::{Point, TimedModel, col_major, one_element};

type Handler = akantu_model::ConstitutiveLawsHandler<dyn Material>;

const DAT: &str =
    "material anisotropic_damage [\n  name = concrete\n  rho = 2400\n  E = 30e9\n  nu = 0.2\n]\n";

/// Options of the experimental (unapproved) Desmorat-type model.
fn experimental() -> LawOptions {
    LawOptions {
        anisotropic_damage: AnisotropicDamageModel::DesmoratExperimental,
        ..LawOptions::default()
    }
}

fn law(model: &SimplexModel, text: &str, upstream: bool) -> Handler {
    let options = if upstream {
        LawOptions::upstream()
    } else {
        experimental()
    };
    materials_from_dat_with(model, text.as_bytes(), Some(options)).unwrap()
}

fn elastic_stress(dim: usize, g: &[Real]) -> Vec<Real> {
    let (e, nu) = (30e9_f64, if dim == 1 { 0.0 } else { 0.2_f64 });
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let tr: Real = (0..dim).map(|i| g[i * (dim + 1)]).sum();
    let mut s = vec![0.0; dim * dim];
    for r in 0..dim {
        for c in 0..dim {
            s[r + c * dim] =
                mu * (g[r + c * dim] + g[c + r * dim]) + if r == c { lambda * tr } else { 0. };
        }
    }
    s
}

fn close(a: &[Real], b: &[Real], rel: Real) -> bool {
    let max = b
        .iter()
        .fold(0.0_f64, |m, x| m.nan_max(x.abs()))
        .max(1e-300);
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= rel * max)
}

fn sym_eigenvalues(dim: usize, d: &[Real]) -> Vec<Real> {
    let m = akantu_core::tensor::MatrixX::from_col_major(dim, dim, d.to_vec()).unwrap();
    akantu_linalg::dense::eigh(&m, akantu_linalg::dense::Storage::Dynamic).unwrap()
}

/// `D = 0` (strains below the threshold `K0`, or purely compressive
/// strains of any size, which have no positive part): the stress is the
/// elastic stress in tension, compression and shear, 1-D/2-D (plane
/// strain)/3-D. Upstream gives σ ≡ ±0 (UD-031) and, after populating
/// `elastic_stress`, the wrong sign in compression (UD-058).
#[test]
fn undamaged_recovers_elasticity() {
    for dim in 1..=3 {
        let model = one_element(dim);
        let cases: Vec<Vec<Real>> = match dim {
            1 => vec![vec![3e-5], vec![-1e-3]],
            2 => vec![
                col_major(&[&[3e-5, 0.], &[0., -1e-5]]),
                col_major(&[&[0., 2e-5], &[2e-5, 0.]]),
                col_major(&[&[-1e-3, 1e-4], &[1e-4, -2e-3]]),
            ],
            _ => vec![
                col_major(&[&[2e-5, 1e-5, 0.], &[1e-5, -1e-5, 0.], &[0., 0., 1e-5]]),
                col_major(&[&[0., 0., 3e-5], &[0., 0., 0.], &[3e-5, 0., 0.]]),
                col_major(&[&[-1e-3, 2e-4, 0.], &[2e-4, -2e-3, 1e-4], &[0., 1e-4, -5e-4]]),
            ],
        };
        for g in cases {
            let mut h = law(&model, DAT, false);
            let l = h.law_mut(0).unwrap();
            let timed = TimedModel {
                inner: &model,
                dt: 0.,
            };
            let mut p = Point {
                law: l,
                ty: simplex(dim),
            };
            p.step(&timed, &g, 1);
            assert!(
                p.internal("damage").iter().all(|x| *x == 0.),
                "{dim}-D: damage for {g:?}"
            );
            let want = elastic_stress(dim, &g);
            assert!(
                close(&p.stress(), &want, 1e-14),
                "{dim}-D: {:?} vs {want:?}",
                p.stress()
            );
            // effective stress internal = elastic stress
            assert!(close(&p.internal("elastic_stress"), &want, 1e-14));
        }
    }
}

/// Uniaxial tension (1-D): damage starts when the equivalent strain
/// reaches `K0` (`κ(0) = K0`, upstream `K(0) = −K0`), and `κ`, `κ⁻¹` are
/// mutual inverses.
#[test]
fn damage_onset_and_threshold_inverse() {
    let model = one_element(1);
    let k0 = 5e-5;
    for (eps, damaged) in [(k0 * (1. - 1e-9), false), (k0 * (1. + 1e-6), true)] {
        let mut h = law(&model, DAT, false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(1),
        };
        p.step(&timed, &[eps], 1);
        assert_eq!(p.internal("damage")[0] > 0., damaged, "ε = {eps}");
        assert_eq!(p.internal("trace_damage")[0] > 0., damaged);
        assert_eq!(p.internal("equivalent_strain")[0], eps);
    }
    let spec = LawSpec {
        spatial_dimension: 3,
        option: String::new(),
        id: "m:0:anisotropic_damage".into(),
    };
    let m = akantu_solid::materials::anisotropic_damage::MaterialAnisotropicDamage::<3>::new(
        &spec, None,
    )
    .unwrap();
    assert!((m.threshold(0.) - k0).abs() < 1e-20);
    for x in [6e-5, 1e-4, 3e-4, 1e-3] {
        assert!((m.threshold(m.threshold_inverse(x)) - x).abs() <= 1e-14 * x);
    }
}

/// Irreversibility (damage and its threshold variable never decrease on
/// unloading/reloading below the previous maximum), bounded principal
/// damages (`≤ Dc`) and a finite stress under very large tension, damage
/// trace consistent with the threshold (`tr ΔD = ΔT` below the cap).
#[test]
fn irreversibility_and_bounds() {
    let model = one_element(3);
    let mut h = law(&model, DAT, false);
    let l = h.law_mut(0).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut p = Point {
        law: l,
        ty: simplex(3),
    };
    let path = |x: Real| {
        col_major(&[
            &[x, 0.2 * x, 0.],
            &[0.2 * x, -0.2 * x, 0.],
            &[0., 0., 0.1 * x],
        ])
    };
    p.step(&timed, &path(1e-4), 1);
    let (d1, t1) = (p.internal("damage"), p.internal("trace_damage")[0]);
    let tr1 = d1[0] + d1[4] + d1[8];
    assert!(
        t1 > 0. && (tr1 - t1).abs() <= 1e-12 * t1,
        "tr D = {tr1}, T = {t1}"
    );
    for x in [5e-5, -1e-4, 8e-5] {
        p.step(&timed, &path(x), 1);
        assert_eq!(p.internal("damage"), d1, "unloading to {x} changed D");
        assert_eq!(p.internal("trace_damage")[0], t1);
    }
    p.step(&timed, &path(2e-4), 1);
    assert!(p.internal("trace_damage")[0] > t1);
    for x in [1e-3, 1e-2, 1e-1, 1.0] {
        p.step(&timed, &path(x), 1);
        let ev = sym_eigenvalues(3, &p.internal("damage"));
        assert!(
            ev.iter().all(|e| *e <= 0.99 + 1e-12 && *e >= -1e-12),
            "eigenvalues {ev:?}"
        );
        assert!(p.stress().iter().all(|s| s.is_finite()));
    }
}

/// Damage growth releases energy at fixed strain: after a damaging step
/// at `ε`, `½σ(D_new):ε ≤ ½σ(D_old):ε`; and the energy of the damaged state
/// is positive (stored energy of a degraded, still positive-definite
/// stiffness).
#[test]
fn damage_growth_releases_energy() {
    let model = one_element(3);
    let mut h = law(&model, DAT, false);
    let l = h.law_mut(0).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut p = Point {
        law: l,
        ty: simplex(3),
    };
    let energy = |p: &mut Point<'_>| {
        p.law
            .compute_potential_energy(&timed.ctx(LawPhase::Energy), simplex(3))
            .unwrap();
        p.law
            .base()
            .potential_energy
            .get(simplex(3), NG)
            .unwrap()
            .as_slice()[0]
    };
    let g1 = col_major(&[&[1e-4, 3e-5, 0.], &[3e-5, 2e-5, 1e-5], &[0., 1e-5, -3e-5]]);
    let g2 = col_major(&[&[3e-4, 1e-4, 0.], &[1e-4, 1e-4, 4e-5], &[0., 4e-5, -1e-4]]);
    p.step(&timed, &g1, 1);
    p.step(&timed, &g2, 1);
    let w_new = energy(&mut p);
    let d_new = p.internal("damage");
    // the same strain with the damage of step 1 frozen (threshold variable
    // set out of reach, so the evaluation does not grow the damage)
    let mut h2 = law(&model, DAT, false);
    let l2 = h2.law_mut(0).unwrap();
    let mut q = Point {
        law: l2,
        ty: simplex(3),
    };
    q.step(&timed, &g1, 1);
    q.write_internal("trace_damage", &[1e9]);
    q.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    q.set_grad_u(&g2);
    q.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(3), NG)
        .unwrap();
    let w_old = energy(&mut q);
    assert_ne!(d_new, q.internal("damage"));
    assert!(
        w_new <= w_old && w_new > 0.,
        "W(D_new) = {w_new}, W(D_old) = {w_old}"
    );
}

/// Covariance: the strain path rotated by `R` gives `R σ Rᵀ` and `R D Rᵀ`
/// (3-D). Bound 1e-10 relative (eigen-decompositions of rotated tensors).
#[test]
fn desmorat_model_is_frame_covariant() {
    let r = {
        let (a, b, c) = (0.7_f64, -0.3_f64, 1.1_f64);
        let rz = |t: Real| {
            [
                [t.cos(), -t.sin(), 0.],
                [t.sin(), t.cos(), 0.],
                [0., 0., 1.],
            ]
        };
        let rx = |t: Real| {
            [
                [1., 0., 0.],
                [0., t.cos(), -t.sin()],
                [0., t.sin(), t.cos()],
            ]
        };
        let mul = |a: [[Real; 3]; 3], b: [[Real; 3]; 3]| -> [[Real; 3]; 3] {
            std::array::from_fn(|i| {
                std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum())
            })
        };
        mul(mul(rz(a), rx(b)), rz(c))
    };
    let rotate = |m: &[Real]| -> Vec<Real> {
        let mut out = vec![0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                out[i + 3 * j] = (0..3)
                    .flat_map(|k| (0..3).map(move |l| (k, l)))
                    .map(|(k, l)| r[i][k] * m[k + 3 * l] * r[j][l])
                    .sum();
            }
        }
        out
    };
    let path: Vec<Vec<Real>> = [1e-4, 3e-4, 1e-4, -2e-4, 5e-4]
        .iter()
        .map(|x| {
            col_major(&[
                &[*x, 0.3 * x, -0.1 * x],
                &[0.3 * x, -0.2 * x, 0.2 * x],
                &[-0.1 * x, 0.2 * x, 0.4 * x],
            ])
        })
        .collect();
    let model = one_element(3);
    let run = |rot: bool| -> Vec<(Vec<Real>, Vec<Real>)> {
        let mut h = law(&model, DAT, false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        path.iter()
            .map(|g| {
                p.step(&timed, &if rot { rotate(g) } else { g.clone() }, 1);
                (p.stress(), p.internal("damage"))
            })
            .collect()
    };
    for (k, ((s0, d0), (s1, d1))) in run(false).iter().zip(run(true)).enumerate() {
        assert!(
            close(&s1, &rotate(s0), 1e-10),
            "step {k}: stress not covariant"
        );
        assert!(
            close(&d1, &rotate(d0), 1e-10),
            "step {k}: damage not covariant"
        );
    }
}

/// The trial state is computed from the committed state: repeated
/// evaluations are idempotent and a failed step rolls back (upstream
/// accumulates damage at every evaluation).
#[test]
fn committed_state_semantics() {
    let model = one_element(2);
    let g1 = col_major(&[&[1e-4, 2e-5], &[2e-5, -1e-5]]);
    let g2 = col_major(&[&[2e-4, 1e-5], &[1e-5, 5e-5]]);
    let run = |upstream: bool, n: usize| -> Vec<Real> {
        let mut h = law(&model, DAT, upstream);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(2),
        };
        p.step(&timed, &g1, n);
        p.step(&timed, &g2, n);
        p.internal("damage")
    };
    assert_eq!(run(false, 1), run(false, 3));
    assert_ne!(
        run(true, 1),
        run(true, 3),
        "upstream accumulates per evaluation"
    );
    let mut h = law(&model, DAT, false);
    let l = h.law_mut(0).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut p = Point {
        law: l,
        ty: simplex(2),
    };
    p.step(&timed, &g1, 1);
    let committed = (p.internal("damage"), p.internal("trace_damage"));
    p.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    p.set_grad_u(&g2);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(2), NG)
        .unwrap();
    assert_ne!(p.internal("damage"), committed.0);
    p.law
        .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), false)
        .unwrap();
    assert_eq!(
        (p.internal("damage"), p.internal("trace_damage")),
        committed
    );
}

/// Quadrature points are independent: two points in a block give the same
/// results in either order (upstream changes `E`, `ν` material-wide in
/// the broken tensile case, so a point's result depends on the points
/// processed before it — shown by the parameter change).
#[test]
fn point_order_independence() {
    let model = one_element(2);
    let ga = col_major(&[&[5e-3, 0.], &[0., 4e-3]]);
    let gb = col_major(&[&[1e-4, 3e-5], &[3e-5, -2e-5]]);
    let run = |order: [&Vec<Real>; 2]| -> Vec<Vec<Real>> {
        let mut h = law(&model, DAT, false);
        let l = h.law_mut(0).unwrap();
        let ty = simplex(2);
        l.for_each_internal_mut(&mut |_, f| match f {
            InternalFieldMut::Real(f) => f.resize(ty, NG, 2),
            _ => Ok(()),
        })
        .unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        l.before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        let mut gu = l.base_mut().grad_u.get_mut(ty, NG).unwrap();
        gu.as_mut_slice()[..4].copy_from_slice(order[0]);
        gu.as_mut_slice()[4..].copy_from_slice(order[1]);
        drop(gu);
        l.compute_stress(&timed.ctx(LawPhase::Stress), ty, NG)
            .unwrap();
        let s = l.base().stress.get(ty, NG).unwrap().as_slice().to_vec();
        vec![s[..4].to_vec(), s[4..].to_vec()]
    };
    let ab = run([&ga, &gb]);
    let ba = run([&gb, &ga]);
    assert_eq!(ab[0], ba[1]);
    assert_eq!(ab[1], ba[0]);
    // upstream: one tensile evaluation in the broken state changes E
    let mut h = law(&model, DAT, true);
    let l = h.law_mut(0).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut p = Point {
        law: l,
        ty: simplex(2),
    };
    p.step(&timed, &ga, 1);
    let e = match p.law.get_param("E").unwrap() {
        ParamValue::Real(x) => x,
        _ => unreachable!(),
    };
    assert_ne!(e, 30e9, "upstream mutates E material-wide");
    let mut h = law(&model, DAT, false);
    let l = h.law_mut(0).unwrap();
    let mut p = Point {
        law: l,
        ty: simplex(2),
    };
    p.step(&timed, &ga, 1);
    assert_eq!(p.law.get_param("E").unwrap(), ParamValue::Real(30e9));
}

/// Upstream mode: σ ≡ 0 (the stress reads the never-written
/// `elastic_stress`), `trace_damage` stays 0 and the damage keeps growing
/// at every step under constant strain (`K(0) = −K0 < 0`, new trace written
/// into the history slot) — UD-031 evidence.
#[test]
fn upstream_mode_defects() {
    let model = one_element(2);
    let mut h = law(&model, DAT, true);
    let l = h.law_mut(0).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut p = Point {
        law: l,
        ty: simplex(2),
    };
    let g = col_major(&[&[1e-5, 0.], &[0., 1e-6]]);
    p.step(&timed, &g, 1);
    let d1 = p.internal("damage")[0];
    p.step(&timed, &g, 1);
    assert!(p.stress().iter().all(|s| *s == 0.));
    assert_eq!(p.internal("trace_damage")[0], 0.);
    assert!(
        p.internal("damage")[0] > d1 && d1 > 0.,
        "damage grows under constant strain"
    );
}

/// `mazars-drucker-prager`: the option is not expressible in an input file
/// (section options are identifiers without `-`, upstream grammar), so it
/// is created through the factory; `k` is parsable. With `k > 0` the
/// equivalent strain gains `k·tr ε` (hydrostatic compression with a small
/// positive principal strain does not damage).
#[test]
fn drucker_prager_variant_through_the_factory() {
    let model = one_element(3);
    let bad = "material anisotropic_damage mazars-drucker-prager [\n  name = c\n  E = 30e9\n]\n";
    assert!(materials_from_dat_with(&model, bad.as_bytes(), Some(experimental())).is_err());
    let spec = LawSpec {
        spatial_dimension: 3,
        option: "mazars-drucker-prager".into(),
        id: "solid_mechanics_model:0:anisotropic_damage".into(),
    };
    let f = factory();
    let mut l = f.create("anisotropic_damage", &spec, &model).unwrap();
    assert!(
        f.create(
            "anisotropic_damage",
            &LawSpec {
                option: "drucker".into(),
                ..spec.clone()
            },
            &model
        )
        .is_err()
    );
    *l.options_mut().unwrap() = experimental();
    for (n, v) in [("E", 30e9), ("nu", 0.2), ("k", 0.5)] {
        l.set_param_from_input(n, ParamValue::Real(v)).unwrap();
    }
    assert_eq!(l.param_names().iter().filter(|n| **n == "k").count(), 1);
    l.update_internal_parameters().unwrap();
    l.add_element(Element::new(simplex(3), 0, GhostType::NotGhost))
        .unwrap();
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: &model,
    };
    l.init_material(&ctx).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut p = Point {
        law: l.as_mut(),
        ty: simplex(3),
    };
    // ε = diag(1e-4, -1e-4, -1e-4): ε̂ = 1e-4 + 0.5·(−1e-4) = 5e-5 ≤ K0
    p.step(
        &timed,
        &col_major(&[&[1e-4, 0., 0.], &[0., -1e-4, 0.], &[0., 0., -1e-4]]),
        1,
    );
    assert!((p.internal("equivalent_strain")[0] - 5e-5).abs() < 1e-20);
    assert!(p.internal("damage").iter().all(|x| *x == 0.));
}

/// The experimental model rejects plane stress (the upstream mode accepts
/// it).
#[test]
fn plane_stress_rejected_by_the_experimental_model() {
    let model = one_element(2);
    let ps = "material anisotropic_damage [\n  name = c\n  E = 30e9\n  nu = 0.2\n  Plane_Stress = true\n]\n";
    assert!(materials_from_dat_with(&model, ps.as_bytes(), Some(experimental())).is_err());
    assert!(materials_from_dat_with(&model, ps.as_bytes(), Some(LawOptions::upstream())).is_ok());
}

/// Parameters in upstream registration order (equivalent-strain functor,
/// threshold functor, then `Dc`) with upstream defaults.
#[test]
fn anisotropic_damage_parameters() {
    let model = one_element(2);
    let mut h = law(&model, DAT, false);
    let l = h.law_mut(0).unwrap();
    let names = l.param_names();
    let tail: Vec<&str> = names[names.len() - 4..].to_vec();
    assert_eq!(tail, ["a", "A", "K0", "Dc"]);
    // parsable only: not readable
    assert!(l.get_param("Dc").is_err());
    let _ = l.law_id();
}

// ---------------------------------------------------------------------------
// Default refusal and the experimental model
// ---------------------------------------------------------------------------

/// No admissible corrected specification is approved, so the default
/// `anisotropic_damage` returns `Error::Unsupported` from `computeStress`
/// and `computeTangentModuli` in every dimension (UD-030 precedent); the
/// upstream compatibility mode and the explicit experimental opt-in still
/// evaluate. (Before the fix the default evaluated the rejected proposal.)
#[test]
fn mv01_default_is_unsupported() {
    assert_eq!(
        LawOptions::default().anisotropic_damage,
        AnisotropicDamageModel::Unsupported
    );
    assert_eq!(
        LawOptions::upstream().anisotropic_damage,
        AnisotropicDamageModel::Upstream
    );
    for dim in 1..=3 {
        let model = one_element(dim);
        let ty = simplex(dim);
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let g = vec![1e-4; dim * dim];
        let mut h = materials_from_dat_with(&model, DAT.as_bytes(), None).unwrap();
        let l = h.law_mut(0).unwrap();
        let mut p = Point { law: l, ty };
        p.law
            .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        p.set_grad_u(&g);
        let err = p
            .law
            .compute_stress(&timed.ctx(LawPhase::Stress), ty, NG)
            .unwrap_err();
        assert!(
            matches!(err, akantu_core::Error::Unsupported(_)),
            "{dim}-D: {err:?}"
        );
        let voigt = dim * (dim + 1) / 2;
        let mut t = akantu_core::Array::zeros(1, voigt * voigt);
        let err = p
            .law
            .compute_tangent_moduli(&timed.ctx(LawPhase::Tangent), ty, NG, &mut t)
            .unwrap_err();
        assert!(matches!(err, akantu_core::Error::Unsupported(_)));
        for upstream in [true, false] {
            let mut h = law(&model, DAT, upstream);
            let mut p = Point {
                law: h.law_mut(0).unwrap(),
                ty,
            };
            p.step(&timed, &g, 1);
            assert!(p.stress().iter().all(|x| x.is_finite()));
        }
    }
}

/// The experimental model's kernel with `E = 30e9`, `ν = 0.2` and the
/// default threshold parameters (direct evaluation of the law).
fn experimental_kernel() -> MaterialAnisotropicDamage<3> {
    let spec = LawSpec {
        spatial_dimension: 3,
        option: String::new(),
        id: "m:0:anisotropic_damage".into(),
    };
    let mut m = MaterialAnisotropicDamage::<3>::new(&spec, None).unwrap();
    *m.options_mut().unwrap() = experimental();
    m.set_param_from_input("E", ParamValue::Real(30e9)).unwrap();
    m.set_param_from_input("nu", ParamValue::Real(0.2)).unwrap();
    m.update_internal_parameters().unwrap();
    m
}

fn rows3(r: [[Real; 3]; 3]) -> [[Real; 3]; 3] {
    // column-major storage of a row-wise literal (symmetric here anyway)
    std::array::from_fn(|j| std::array::from_fn(|i| r[i][j]))
}

fn half_contraction(s: &[[Real; 3]; 3], e: &[[Real; 3]; 3]) -> Real {
    0.5 * (0..3)
        .flat_map(|j| (0..3).map(move |i| s[j][i] * e[j][i]))
        .sum::<Real>()
}

/// `mv01_*` counterexample (reproduced on the public point kernel
/// of the experimental model): spectral clipping of the total trial tensor
/// is not monotone. Step 1 `ε₁ = diag(4e-4, 0, 0)` saturates x
/// (`D₁ = diag(0.99, 0, 0)`); step 2 with a rotated positive eigenvector
/// gives `min eig(D₂ − D₁) = −0.1336…` (damage in x decreases), and at the
/// mixed strain with the same positive eigenspace the stored energy at fixed
/// strain grows by 1.257e5 J/m³ (negative dissipation). This documents why
/// the model is not the default (`mv01_default_is_unsupported`); the bounds
/// are relative 1e-9 (eigen-decompositions and host `tan`/`atan`).
#[test]
fn mv01_saturation_counterexample_is_reproduced_in_the_experimental_model() {
    let m = experimental_kernel();
    let zero = [[0.0; 3]; 3];
    let eps1 = rows3([[4e-4, 0., 0.], [0., 0., 0.], [0., 0., 0.]]);
    let (_, d1, t1, _, _) = m.desmorat_on_quad(&eps1, &zero, 0.).unwrap();
    let rel = |a: Real, b: Real| (a - b).abs() <= 1e-9 * b.abs();
    assert!(rel(t1, 1.127_418_988_179_359_7), "T1 = {t1}");
    assert!(rel(d1[0][0], 0.99) && d1[1][1].abs() < 1e-12 && d1[2][2].abs() < 1e-12);

    let eps2 = rows3([[5e-4, 5e-4, 0.], [5e-4, 5e-4, 0.], [0., 0., 0.]]);
    let (_, d2, t2, _, _) = m.desmorat_on_quad(&eps2, &d1, t1).unwrap();
    assert!(rel(t2, 1.636_043_905_281_058_3), "T2 = {t2}");
    assert!(rel(d2[0][0], 0.945_945_840_600_684_1), "{d2:?}");
    assert!(rel(d2[0][1], 0.182_150_920_809_544_31), "{d2:?}");
    assert!(rel(d2[1][1], 0.236_859_810_647_525_43), "{d2:?}");
    let diff: Vec<Real> = (0..3)
        .flat_map(|j| (0..3).map(move |i| (i, j)))
        .map(|(i, j)| d2[j][i] - d1[j][i])
        .collect();
    let min_eig = sym_eigenvalues(3, &diff)
        .into_iter()
        .fold(Real::INFINITY, NanMinMax::nan_min);
    assert!(
        rel(min_eig, -0.133_612_657_753_937_2),
        "min eig(D2 - D1) = {min_eig}"
    );

    // same positive eigenspace, principal strains (1e-3, -1e-2, 0)
    let eps2b = rows3([[-4.5e-3, 5.5e-3, 0.], [5.5e-3, -4.5e-3, 0.], [0., 0., 0.]]);
    let (s_new, d2b, t2b, _, _) = m.desmorat_on_quad(&eps2b, &d1, t1).unwrap();
    assert!(
        rel(t2b, t2)
            && (0..3).all(|j| (0..3)
                .all(|i| rel(d2b[j][i], d2[j][i]) || (d2b[j][i] - d2[j][i]).abs() < 1e-15))
    );
    // frozen D1: threshold history out of reach (no evolution)
    let (s_old, d_frozen, _, _, _) = m.desmorat_on_quad(&eps2b, &d1, t2b + 1e-12).unwrap();
    assert_eq!(d_frozen, d1);
    let psi_old = half_contraction(&s_old, &eps2b);
    let psi_new = half_contraction(&s_new, &eps2b);
    assert!(
        rel(psi_old, 877_817.164_179_104_4),
        "psi(eps2, D1) = {psi_old}"
    );
    assert!(
        rel(psi_new, 1_003_543.332_524_060_3),
        "psi(eps2, D2) = {psi_new}"
    );
    // "released" energy is negative: the damage update is not dissipative
    assert!(psi_old - psi_new < -1.257e5);
}

/* ------------------------------------------------------------------------ */
/* UD-058 corrected model (D-066 (3))                                         */
/* ------------------------------------------------------------------------ */
// The acceptance tests of the UD-058 specification (module documentation
// of `anisotropic_damage.rs`), with the deterministic θ (property 5 checked
// numerically). There is no oracle
// for this model. Tolerances (docs/NUMERICAL_VALIDATION.md,
// PROPOSED): the properties hold exactly in real arithmetic; the bounds are
// rounding allowances of 3 × 3 symmetric eigen-solves and sums of
// dimensionless quantities of order 1 (damage, `tr P = 1`): 1e-13 absolute
// on damage eigenvalues and traces, 1e-12 relative on energies, 1e-6 on the
// central-difference tangent (h = 1e-7 of the strain scale, round-off
// ≈ ε/h ≈ 2e-9 of the stress, truncation O(h²)).

/// Options of the corrected UD-058 model.
fn corrected() -> LawOptions {
    LawOptions {
        anisotropic_damage: AnisotropicDamageModel::Corrected,
        ..LawOptions::default()
    }
}

/// The corrected model's kernel (`E = 30e9`, `ν = 0.2`, or `ν = 0` in 1-D;
/// default threshold parameters, `Dc = 0.99`); `option` is the allocator
/// option (`""` or `mazars-drucker-prager`) and `k` its parameter.
fn corrected_kernel<const D: usize>(option: &str, k: Real) -> MaterialAnisotropicDamage<D> {
    let spec = LawSpec {
        spatial_dimension: D,
        option: option.into(),
        id: "m:0:anisotropic_damage".into(),
    };
    let mut m = MaterialAnisotropicDamage::<D>::new(&spec, None).unwrap();
    *m.options_mut().unwrap() = corrected();
    m.set_param_from_input("E", ParamValue::Real(30e9)).unwrap();
    m.set_param_from_input("nu", ParamValue::Real(if D == 1 { 0. } else { 0.2 }))
        .unwrap();
    if !option.is_empty() {
        m.set_param_from_input("k", ParamValue::Real(k)).unwrap();
    }
    m.update_internal_parameters().unwrap();
    m
}

/// SplitMix64 in [-1, 1).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> Real {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)] // 53-bit draw
        let v = (z >> 11) as Real / (1u64 << 53) as Real;
        2. * v - 1.
    }
    #[allow(clippy::needless_range_loop)] // (i, j) pairs of a symmetric matrix
    fn sym3(&mut self, scale: Real) -> [[Real; 3]; 3] {
        let mut m = [[0.0; 3]; 3];
        for j in 0..3 {
            for i in j..3 {
                let x = scale * self.next();
                m[j][i] = x;
                m[i][j] = x;
            }
        }
        m
    }
}

fn eig3(m: &[[Real; 3]; 3]) -> Vec<Real> {
    sym_eigenvalues(3, m.as_flattened())
}

/// `ψ(ε, D)` of the specification, written independently of the law:
/// `G[tr(εHεH) − (A:ε)²/t] + K/2[b⟨tr ε⟩₊² + ⟨tr ε⟩₋²]`, `A = 1 − D`,
/// `H = √A`, `t = tr A`, `b = max(1 − tr D, 1 − Dc)` (3-D constants).
fn psi(eps: &[[Real; 3]; 3], d: &[[Real; 3]; 3], dc: Real, nu: Real) -> Real {
    let e = 30e9;
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let g = e / (2. * (1. + nu));
    let k = lambda + 2. * g / 3.;
    let a: [[Real; 3]; 3] =
        std::array::from_fn(|j| std::array::from_fn(|i| f64::from(u8::from(i == j)) - d[j][i]));
    let am = akantu_core::tensor::MatrixX::from_col_major(3, 3, a.as_flattened().to_vec()).unwrap();
    let (w, v) = akantu_linalg::dense::eigh_vectors(
        &am,
        akantu_linalg::dense::Storage::Dynamic,
        akantu_linalg::dense::SortOrder::Natural,
    )
    .unwrap();
    let h: [[Real; 3]; 3] = std::array::from_fn(|c| {
        std::array::from_fn(|r| {
            (0..3)
                .map(|q| v.at(r, q) * w[q].max(0.).sqrt() * v.at(c, q))
                .sum()
        })
    });
    let mul = |x: &[[Real; 3]; 3], y: &[[Real; 3]; 3]| -> [[Real; 3]; 3] {
        std::array::from_fn(|c| std::array::from_fn(|r| (0..3).map(|q| x[q][r] * y[c][q]).sum()))
    };
    let eh = mul(eps, &h);
    let tr_ehh: Real = (0..3).map(|i| mul(&eh, &eh)[i][i]).sum();
    let a_eps: Real = (0..3)
        .flat_map(|j| (0..3).map(move |i| (i, j)))
        .map(|(i, j)| a[j][i] * eps[j][i])
        .sum();
    let t = a[0][0] + a[1][1] + a[2][2];
    let tr_d = d[0][0] + d[1][1] + d[2][2];
    let b = (1. - tr_d).max(1. - dc);
    let v = eps[0][0] + eps[1][1] + eps[2][2];
    g * (tr_ehh - a_eps * a_eps / t) + 0.5 * k * (b * v.max(0.).powi(2) + v.min(0.).powi(2))
}

/// The stored energy `½σ:ε` of the law at frozen `D` (no evolution: the
/// threshold history is set to `κ⁻¹(ε̂)`).
fn frozen_energy(
    m: &MaterialAnisotropicDamage<3>,
    eps: &[[Real; 3]; 3],
    d: &[[Real; 3]; 3],
) -> Real {
    let (_, _, _, eps_hat, _) = m.corrected_on_quad(eps, &[[0.0; 3]; 3], 0.).unwrap();
    let t_hold = m.threshold_inverse(eps_hat);
    let (s, d_same, _, _, _) = m.corrected_on_quad(eps, d, t_hold).unwrap();
    assert_eq!(&d_same, d, "frozen evaluation evolved the damage");
    half_contraction(&s, eps)
}

/// UD-058 specification, acceptance test: `D = 0` (below the threshold, or a purely compressive
/// strain: `ε̂ = 0`) recovers the elastic stress in 1-D/2-D/3-D (tension,
/// compression, shear), through the material interface.
#[test]
fn ud058_undamaged_recovers_elasticity() {
    let options = corrected();
    for dim in 1..=3 {
        let model = one_element(dim);
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let s = 1e-5;
        let mut cases: Vec<Vec<Real>> = vec![
            (0..dim * dim)
                .map(|k| if k % (dim + 1) == 0 { s } else { 0. })
                .collect(),
            (0..dim * dim)
                .map(|k| if k % (dim + 1) == 0 { -100. * s } else { 0. })
                .collect(),
        ];
        if dim > 1 {
            let mut shear = vec![0.0; dim * dim];
            shear[1] = s;
            shear[dim] = s;
            cases.push(shear);
        }
        for g in cases {
            let mut h = materials_from_dat_with(&model, DAT.as_bytes(), Some(options)).unwrap();
            let mut p = Point {
                law: h.law_mut(0).unwrap(),
                ty: simplex(dim),
            };
            p.step(&timed, &g, 1);
            assert!(
                p.internal("damage").iter().all(|d| *d == 0.),
                "{dim}-D {g:?}"
            );
            assert!(
                close(&p.stress(), &elastic_stress(dim, &g), 1e-12),
                "{dim}-D {g:?}: {:?} vs {:?}",
                p.stress(),
                elastic_stress(dim, &g)
            );
        }
    }
}

/// A loading path on the corrected model: the first step
/// saturates x exactly at `Dc` (the increment is scaled, `T = tr D = Dc`
/// instead of `κ⁻¹(ε̂)`), the rotated second step opens the saturated
/// direction and therefore stops the evolution (`θ = 0`, property 7): no
/// healing (`D₂ − D₁ ⪰ 0`), and the energy at fixed strain does not rise.
#[test]
fn ud058_mv01_path_is_irreversible_and_dissipative() {
    let m = corrected_kernel::<3>("", 0.);
    let zero = [[0.0; 3]; 3];
    let eps1 = rows3([[4e-4, 0., 0.], [0., 0., 0.], [0., 0., 0.]]);
    let (_, d1, t1, _, _) = m.corrected_on_quad(&eps1, &zero, 0.).unwrap();
    assert!((d1[0][0] - 0.99).abs() <= 1e-15, "{d1:?}");
    assert!(d1[1][1] == 0. && d1[2][2] == 0. && d1[0][1] == 0.);
    assert!((t1 - 0.99).abs() <= 1e-15, "T1 = {t1}");
    for eps2 in [
        rows3([[5e-4, 5e-4, 0.], [5e-4, 5e-4, 0.], [0., 0., 0.]]),
        rows3([[-4.5e-3, 5.5e-3, 0.], [5.5e-3, -4.5e-3, 0.], [0., 0., 0.]]),
    ] {
        let (_, d2, t2, _, _) = m.corrected_on_quad(&eps2, &d1, t1).unwrap();
        let diff: [[Real; 3]; 3] =
            std::array::from_fn(|j| std::array::from_fn(|i| d2[j][i] - d1[j][i]));
        let min_eig = eig3(&diff)
            .into_iter()
            .fold(Real::INFINITY, NanMinMax::nan_min);
        assert!(min_eig >= -1e-13, "min eig(D2 - D1) = {min_eig}");
        assert_eq!(d2, d1, "an opened saturated direction stops the evolution");
        assert_eq!(t2, t1);
        let released = frozen_energy(&m, &eps2, &d1) - frozen_energy(&m, &eps2, &d2);
        assert!(released >= 0., "released {released}");
    }
    // loading orthogonal to the saturated direction keeps damaging (y) once
    // ε̂ exceeds the threshold κ(T₁) = κ(0.99) ≈ 3.3e-4 of the accumulated
    // damage (the threshold is a function of tr D)
    let eps3 = rows3([[0., 0., 0.], [0., 1e-3, 0.], [0., 0., 0.]]);
    let (_, d3, t3, _, _) = m.corrected_on_quad(&eps3, &d1, t1).unwrap();
    assert!(d3[1][1] > 0. && d3[0][0] == d1[0][0] && t3 > t1, "{d3:?}");
    let tr3 = d3[0][0] + d3[1][1] + d3[2][2];
    assert!((tr3 - t3).abs() <= 1e-13, "tr D = {tr3}, T = {t3}");
}

/// UD-058 specification, acceptance tests and (property 5) on random
/// non-proportional paths (3-D kernel; mazars and Drucker–Prager `k = 0.1`):
/// at every step `D_{n+1} − D_n ⪰ 0` (Loewner), `λ_max(D) ≤ Dc`,
/// `tr D = T`, and every damaging step dissipates (`ψ(ε, D_n) ≥ ψ(ε,
/// D_{n+1})`, the energy at the step's strain); `½σ:ε = ψ` at frozen `D`
/// (property 6, against the independently written `ψ`). Saturated steps and
/// stops by an opened saturated direction are reached.
#[test]
fn ud058_random_paths_are_bounded_irreversible_and_dissipative() {
    let mut damaging = 0;
    let mut saturated = 0;
    for (option, k) in [("", 0.), ("mazars-drucker-prager", 0.1)] {
        let m = corrected_kernel::<3>(option, k);
        let mut r = Rng(if k == 0. { 58 } else { 59 });
        for _path in 0..150 {
            let mut eps = [[0.0; 3]; 3];
            let mut d = [[0.0; 3]; 3];
            let mut t = 0.;
            for _step in 0..30 {
                let inc = r.sym3(1.5e-4);
                let unload = r.next() < -0.6;
                eps = std::array::from_fn(|j| {
                    std::array::from_fn(|i| {
                        if unload {
                            0.5 * eps[j][i]
                        } else {
                            eps[j][i] + inc[j][i]
                        }
                    })
                });
                let (s, d_new, t_new, _, _) = m.corrected_on_quad(&eps, &d, t).unwrap();
                let diff: [[Real; 3]; 3] =
                    std::array::from_fn(|j| std::array::from_fn(|i| d_new[j][i] - d[j][i]));
                let min_inc = eig3(&diff)
                    .into_iter()
                    .fold(Real::INFINITY, NanMinMax::nan_min);
                assert!(min_inc >= -1e-13, "healing: min eig(ΔD) = {min_inc}");
                let lmax = eig3(&d_new)
                    .into_iter()
                    .fold(Real::NEG_INFINITY, NanMinMax::nan_max);
                assert!(lmax <= 0.99 + 1e-13, "λ_max(D) = {lmax}");
                let tr = d_new[0][0] + d_new[1][1] + d_new[2][2];
                assert!((tr - t_new).abs() <= 1e-13, "tr D = {tr}, T = {t_new}");
                assert!(t_new >= t);
                let symmetric =
                    (0..3).all(|j| (0..3).all(|i| d_new[j][i].to_bits() == d_new[i][j].to_bits()));
                assert!(symmetric, "D symmetric: {d_new:?}");
                let w = half_contraction(&s, &eps);
                let w_ref = psi(&eps, &d_new, 0.99, 0.2);
                assert!(
                    (w - w_ref).abs() <= 1e-12 * w_ref.abs().max(1.),
                    "½σ:ε {w} vs ψ {w_ref}"
                );
                if d_new != d {
                    damaging += 1;
                    let released = psi(&eps, &d, 0.99, 0.2) - w_ref;
                    assert!(
                        released >= -1e-12 * w_ref.abs().max(1.),
                        "released {released}"
                    );
                    if lmax > 0.99 - 1e-12 {
                        saturated += 1;
                    }
                }
                d = d_new;
                t = t_new;
            }
        }
    }
    eprintln!("ud058 random paths: {damaging} damaging steps, {saturated} reaching Dc");
    assert!(damaging > 1000 && saturated > 50, "{damaging} {saturated}");
}

/// A direction
/// saturated by the previous step whose projection `P̃_ii` of the next
/// increment is below the saturation residue used to drop its couplings
/// `P̃_ij` with the open directions, so a step that saturates a second
/// direction left an off-diagonal between two directions at `Dc` and
/// `λ_max(D) − Dc ≈ 1e-7` (measured +9.9e-8 for `s/e = 1e-7`). The
/// case: step 1 `ε = 4e-4 e_x⊗e_x` from `D = 0` saturates x exactly;
/// step 2 `ε = [[0, s, 0], [s, e, 0], [0, 0, 0]]`. Now `λ_max(D) ≤ Dc` to
/// rounding (the bound of the random-path test), `tr D = T`, no healing.
#[test]
fn lr01_saturated_direction_couplings_keep_lambda_max_at_dc() {
    let m = corrected_kernel::<3>("", 0.);
    let dc = 0.99;
    let x = [[4e-4, 0., 0.], [0., 0., 0.], [0., 0., 0.]];
    let (_, d1, t1, _, _) = m.corrected_on_quad(&x, &[[0.0; 3]; 3], 0.).unwrap();
    assert!(
        (d1[0][0] - dc).abs() <= 1e-15,
        "step 1 saturates x: D = {d1:?}"
    );
    let mut worst = Real::NEG_INFINITY;
    for e in [1e-3, 1e-2, 1e-1] {
        for ratio in [1e-7, 1.1e-7, 5e-8] {
            let s = ratio * e;
            let eps = [[0., s, 0.], [s, e, 0.], [0., 0., 0.]];
            let (_, d2, t2, _, _) = m.corrected_on_quad(&eps, &d1, t1).unwrap();
            let lmax = eig3(&d2)
                .into_iter()
                .fold(Real::NEG_INFINITY, NanMinMax::nan_max);
            worst = NanMinMax::nan_max(worst, lmax - dc);
            assert!(
                lmax <= dc + 1e-13,
                "e = {e:e}, s/e = {ratio:e}: λ_max(D) − Dc = {:e}",
                lmax - dc
            );
            let tr = d2[0][0] + d2[1][1] + d2[2][2];
            assert!((tr - t2).abs() <= 1e-13, "tr D = {tr}, T = {t2}");
            let diff: [[Real; 3]; 3] =
                std::array::from_fn(|j| std::array::from_fn(|i| d2[j][i] - d1[j][i]));
            let min_inc = eig3(&diff)
                .into_iter()
                .fold(Real::INFINITY, NanMinMax::nan_min);
            assert!(min_inc >= -1e-13, "healing: min eig(ΔD) = {min_inc}");
            assert!(t2 > t1, "the open direction y still damages");
        }
    }
    eprintln!("saturated direction: max λ_max(D) − Dc = {worst:e}");
}

/// UD-058 specification, acceptance test: objectivity `D(QεQᵀ) = Q D(ε) Qᵀ` (and the stress)
/// for the same rotated history.
#[test]
fn ud058_is_objective() {
    let m = corrected_kernel::<3>("", 0.);
    let (c, s) = (0.6_f64, 0.8_f64);
    let q = rows3([
        [c, -s, 0.],
        [s * 0.6, c * 0.6, -0.8],
        [s * 0.8, c * 0.8, 0.6],
    ]);
    let rot = |x: &[[Real; 3]; 3]| -> [[Real; 3]; 3] {
        // Q x Qᵀ (column-major)
        std::array::from_fn(|cc| {
            std::array::from_fn(|rr| {
                (0..3)
                    .flat_map(|a| (0..3).map(move |b| (a, b)))
                    .map(|(a, b)| q[a][rr] * x[b][a] * q[b][cc])
                    .sum()
            })
        })
    };
    let mut r = Rng(7);
    let (mut eps, mut d, mut t) = ([[0.0; 3]; 3], [[0.0; 3]; 3], 0.);
    let (mut d_r, mut t_r) = ([[0.0; 3]; 3], 0.);
    for _ in 0..25 {
        let inc = r.sym3(1.2e-4);
        eps = std::array::from_fn(|j| std::array::from_fn(|i| eps[j][i] + inc[j][i]));
        let (s1, d1, t1, _, _) = m.corrected_on_quad(&eps, &d, t).unwrap();
        let (s2, d2, t2, _, _) = m.corrected_on_quad(&rot(&eps), &d_r, t_r).unwrap();
        let (rd, rs) = (rot(&d1), rot(&s1));
        let scale = s1
            .as_flattened()
            .iter()
            .fold(0.0_f64, |a, x| a.max(x.abs()));
        for j in 0..3 {
            for i in 0..3 {
                assert!((rd[j][i] - d2[j][i]).abs() <= 1e-12, "D: {rd:?} vs {d2:?}");
                assert!(
                    (rs[j][i] - s2[j][i]).abs() <= 1e-12 * scale,
                    "σ: {rs:?} vs {s2:?}"
                );
            }
        }
        assert!((t1 - t2).abs() <= 1e-12 * t1.abs().max(1.));
        (d, t, d_r, t_r) = (d1, t1, d2, t2);
    }
    assert!(t > 0.5, "the path damages: T = {t}");
}

/// UD-058 specification, acceptance test: 2-D (plane strain) and 1-D (`ν = 0`, lateral strains
/// zero) equal the 3-D law under the embedded kinematics, bit for bit
/// (same 3 × 3 arithmetic); `k = 0` with the Drucker–Prager option equals
/// `mazars`; purely compressive paths (`ε̂ = 0`) give no evolution and
/// finite stresses (UD-763 does not exist here).
#[test]
fn ud058_reductions_options_and_compression() {
    let m3 = corrected_kernel::<3>("", 0.);
    let m2 = corrected_kernel::<2>("", 0.);
    let m1 = corrected_kernel::<1>("", 0.);
    let m3_nu0 = {
        let mut m = corrected_kernel::<3>("", 0.);
        m.set_param_from_input("nu", ParamValue::Real(0.)).unwrap();
        m.update_internal_parameters().unwrap();
        m
    };
    let dp0 = corrected_kernel::<3>("mazars-drucker-prager", 0.);
    let mut r = Rng(11);
    let (mut g2, mut d2, mut t2) = ([[0.0; 2]; 2], [[0.0; 2]; 2], 0.);
    let (mut d3, mut t3) = ([[0.0; 3]; 3], 0.);
    let (mut g1, mut d1, mut t1) = ([[0.0; 1]; 1], [[0.0; 1]; 1], 0.);
    let (mut d13, mut t13) = ([[0.0; 3]; 3], 0.);
    for _ in 0..30 {
        let inc = r.sym3(1e-4);
        g2 = std::array::from_fn(|j| std::array::from_fn(|i| g2[j][i] + inc[j][i]));
        let e3: [[Real; 3]; 3] = std::array::from_fn(|j| {
            std::array::from_fn(|i| if i < 2 && j < 2 { g2[j][i] } else { 0. })
        });
        let (s2, nd2, nt2, _, _) = m2.corrected_on_quad(&g2, &d2, t2).unwrap();
        let (s3, nd3, nt3, _, _) = m3.corrected_on_quad(&e3, &d3, t3).unwrap();
        for j in 0..2 {
            for i in 0..2 {
                assert_eq!(s2[j][i].to_bits(), s3[j][i].to_bits());
                assert_eq!(nd2[j][i].to_bits(), nd3[j][i].to_bits());
            }
        }
        assert_eq!(nt2.to_bits(), nt3.to_bits());
        (d2, t2, d3, t3) = (nd2, nt2, nd3, nt3);
        g1[0][0] += inc[0][0].abs() * 0.5;
        let e13: [[Real; 3]; 3] = std::array::from_fn(|j| {
            std::array::from_fn(|i| if i == 0 && j == 0 { g1[0][0] } else { 0. })
        });
        let (s1, nd1, nt1, _, _) = m1.corrected_on_quad(&g1, &d1, t1).unwrap();
        let (s13, nd13, nt13, _, _) = m3_nu0.corrected_on_quad(&e13, &d13, t13).unwrap();
        assert_eq!(s1[0][0].to_bits(), s13[0][0].to_bits());
        assert_eq!(nd1[0][0].to_bits(), nd13[0][0].to_bits());
        assert_eq!(nt1.to_bits(), nt13.to_bits());
        (d1, t1, d13, t13) = (nd1, nt1, nd13, nt13);
        // k = 0 Drucker–Prager is mazars
        let a = m3.corrected_on_quad(&e3, &d3, t3).unwrap();
        let b = dp0.corrected_on_quad(&e3, &d3, t3).unwrap();
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }
    assert!(t2 > 0. && t1 > 0.);
    // compression: ε̂ = 0, no evolution, finite stress
    let c = rows3([[-1e-2, 1e-3, 0.], [1e-3, -2e-2, 0.], [0., 0., -5e-3]]);
    let (s, d, t, eps_hat, _) = m3.corrected_on_quad(&c, &[[0.0; 3]; 3], 0.).unwrap();
    assert_eq!((eps_hat, t), (0., 0.));
    assert_eq!(d, [[0.0; 3]; 3]);
    assert!(s.as_flattened().iter().all(|x| x.is_finite()));
}

/// UD-058 specification, acceptance test: repeated evaluations of a step give the same state
/// (computed from the committed state), a discarded step rolls back, two
/// points give the same results in either order; reloading to the strain
/// of a saturated step changes nothing (no ratcheting).
#[test]
fn ud058_committed_state_and_point_order() {
    let model = one_element(3);
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let g1 = col_major(&[&[2e-4, 5e-5, 0.], &[5e-5, -1e-5, 2e-5], &[0., 2e-5, 4e-5]]);
    let g2 = col_major(&[&[3e-4, 1e-4, 0.], &[1e-4, 2e-4, 0.], &[0., 0., 1e-5]]);
    let run = |n: usize| -> (Vec<Real>, Vec<Real>) {
        let mut h = materials_from_dat_with(&model, DAT.as_bytes(), Some(corrected())).unwrap();
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(3),
        };
        p.step(&timed, &g1, n);
        p.step(&timed, &g2, n);
        (p.internal("damage"), p.stress())
    };
    assert_eq!(run(1), run(3));
    let mut h = materials_from_dat_with(&model, DAT.as_bytes(), Some(corrected())).unwrap();
    let mut p = Point {
        law: h.law_mut(0).unwrap(),
        ty: simplex(3),
    };
    p.step(&timed, &g1, 1);
    let committed = (p.internal("damage"), p.internal("trace_damage"));
    p.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    p.set_grad_u(&g2);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(3), NG)
        .unwrap();
    assert_ne!(p.internal("damage"), committed.0);
    p.law
        .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), false)
        .unwrap();
    assert_eq!(
        (p.internal("damage"), p.internal("trace_damage")),
        committed
    );
    // A non-monotone sequence inside one step — evaluate
    // at a larger strain, then at the step's strain, before the step ends:
    // the trial state of every evaluation comes from the committed
    // `(D_n, T_n)`, so the result equals the single evaluation at the
    // step's strain (a trial state read from the current damage would keep
    // the larger strain's damage: the step's strain is below the raised
    // threshold). The step's strain is the smaller one, `g2` the larger
    // (the damage of `g2` is the cap of the model: a still larger strain
    // would give the same state and could not tell the two readings apart).
    let step_strain: Vec<Real> = g2.iter().map(|x| 0.8 * x).collect();
    p.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    p.set_grad_u(&step_strain);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(3), NG)
        .unwrap();
    let single = (p.internal("damage"), p.internal("trace_damage"));
    assert_ne!(single.0, committed.0, "the step's strain must damage");
    p.law
        .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), false)
        .unwrap();
    assert_eq!(
        (p.internal("damage"), p.internal("trace_damage")),
        committed
    );
    p.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    p.set_grad_u(&g2);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(3), NG)
        .unwrap();
    assert_ne!(
        p.internal("damage"),
        single.0,
        "the larger strain must damage more than the step's strain"
    );
    p.set_grad_u(&step_strain);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(3), NG)
        .unwrap();
    assert_eq!(
        (p.internal("damage"), p.internal("trace_damage")),
        single,
        "the damage of the step's strain must not depend on an earlier larger evaluation"
    );
    p.law
        .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), false)
        .unwrap();
    assert_eq!(
        (p.internal("damage"), p.internal("trace_damage")),
        committed
    );
    // saturate, then reload to the same strain: nothing changes
    let big = col_major(&[&[5e-3, 0., 0.], &[0., 0., 0.], &[0., 0., 0.]]);
    p.step(&timed, &big, 1);
    let sat = (p.internal("damage"), p.internal("trace_damage"));
    let lmax = sym_eigenvalues(3, &sat.0)
        .into_iter()
        .fold(Real::NEG_INFINITY, NanMinMax::nan_max);
    assert!((lmax - 0.99).abs() <= 1e-13, "λ_max(D) = {lmax}: {sat:?}");
    p.step(&timed, &big, 1);
    assert_eq!((p.internal("damage"), p.internal("trace_damage")), sat);
    // point order: the kernel is point-local (no material-wide state)
    let m = corrected_kernel::<3>("", 0.);
    let a = rows3([[3e-4, 1e-4, 0.], [1e-4, 1e-4, 0.], [0., 0., 0.]]);
    let b = rows3([[5e-3, 0., 0.], [0., -1e-3, 0.], [0., 0., 0.]]);
    let z = [[0.0; 3]; 3];
    let ra = m.corrected_on_quad(&a, &z, 0.).unwrap();
    let _ = m.corrected_on_quad(&b, &z, 0.).unwrap();
    assert_eq!(
        format!("{ra:?}"),
        format!("{:?}", m.corrected_on_quad(&a, &z, 0.).unwrap())
    );
}

/// UD-058 specification, domains and tangent: the tangent is the frozen-damage derivative `∂σ/∂ε|_D`
/// (central differences in Voigt notation with engineering shear strains),
/// in tension and compression, undamaged and damaged, 1-D/2-D/3-D; at
/// `D = 0` it is the elastic tangent.
#[test]
fn ud058_secant_tangent_is_the_frozen_damage_derivative() {
    fn check<const D: usize>(m: &MaterialAnisotropicDamage<D>, eps: Matrix<D>, d: Matrix<D>) {
        let pairs: Vec<(usize, usize)> = match D {
            1 => vec![(0, 0)],
            2 => vec![(0, 0), (1, 1), (0, 1)],
            _ => vec![(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)],
        };
        let n = pairs.len();
        let mut c = vec![0.0; n * n];
        m.corrected_tangent(&eps, &d, &mut c).unwrap();
        // σ at frozen D: no evolution with the threshold history at κ⁻¹(ε̂)
        let sigma = |e: &Matrix<D>| -> Matrix<D> {
            let (_, _, _, eh, _) = m.corrected_on_quad(e, &[[0.0; D]; D], 0.).unwrap();
            let (s, dd, _, _, _) = m
                .corrected_on_quad(e, &d, m.threshold_inverse(eh).max(10.))
                .unwrap();
            assert_eq!(dd, d);
            s
        };
        let scale = eps
            .as_flattened()
            .iter()
            .fold(0.0_f64, |a, x| a.max(x.abs()));
        let h = 1e-7 * scale;
        let s0 = sigma(&eps);
        let smax = s0
            .as_flattened()
            .iter()
            .fold(0.0_f64, |a, x| a.max(x.abs()));
        for (jj, &(k, l)) in pairs.iter().enumerate() {
            let (mut ep, mut em) = (eps, eps);
            // engineering shear: γ_kl = 2ε_kl, a unit of γ moves ε_kl and ε_lk by ½
            let dh = if k == l { h } else { 0.5 * h };
            ep[l][k] += dh;
            em[l][k] -= dh;
            if k != l {
                ep[k][l] += dh;
                em[k][l] -= dh;
            }
            let (sp, sm) = (sigma(&ep), sigma(&em));
            for (ii, &(i, j)) in pairs.iter().enumerate() {
                let fd = (sp[j][i] - sm[j][i]) / (2. * h);
                assert!(
                    (fd - c[ii + jj * n]).abs() <= 1e-6 * smax / scale,
                    "{D}-D C[{ii}][{jj}] = {} vs FD {fd}",
                    c[ii + jj * n]
                );
            }
        }
    }
    type Matrix<const D: usize> = [[Real; D]; D];
    let m3 = corrected_kernel::<3>("", 0.);
    let m2 = corrected_kernel::<2>("", 0.);
    let m1 = corrected_kernel::<1>("", 0.);
    let tension = rows3([[3e-4, 1e-4, -2e-5], [1e-4, 1e-4, 3e-5], [-2e-5, 3e-5, 2e-5]]);
    let compression = rows3([
        [-3e-4, 1e-4, -2e-5],
        [1e-4, -1e-4, 3e-5],
        [-2e-5, 3e-5, 2e-5],
    ]);
    let (_, dmg, _, _, _) = m3.corrected_on_quad(&tension, &[[0.0; 3]; 3], 0.).unwrap();
    assert!(dmg[0][0] > 0.1, "{dmg:?}");
    for d in [[[0.0; 3]; 3], dmg] {
        check(&m3, tension, d);
        check(&m3, compression, d);
        let b2 = |x: &[[Real; 3]; 3]| -> [[Real; 2]; 2] {
            std::array::from_fn(|j| std::array::from_fn(|i| x[j][i]))
        };
        check(&m2, b2(&tension), b2(&d));
        check(&m2, b2(&compression), b2(&d));
        check(&m1, [[tension[0][0]]], [[d[0][0]]]);
        check(&m1, [[compression[0][0]]], [[d[0][0]]]);
    }
    // D = 0: the elastic tangent of the parent (λ + 2μ, λ, μ)
    let mut c = vec![0.0; 36];
    m3.corrected_tangent(&tension, &[[0.0; 3]; 3], &mut c)
        .unwrap();
    let (e, nu) = (30e9_f64, 0.2_f64);
    let (lambda, mu) = (nu * e / ((1. + nu) * (1. - 2. * nu)), e / (2. * (1. + nu)));
    let want = |i: usize, j: usize| match (i < 3, j < 3) {
        (true, true) if i == j => lambda + 2. * mu,
        (true, true) => lambda,
        _ if i == j => mu,
        _ => 0.,
    };
    for j in 0..6 {
        for i in 0..6 {
            assert!(
                (c[i + 6 * j] - want(i, j)).abs() <= 1e-12 * (lambda + 2. * mu),
                "C[{i}][{j}]"
            );
        }
    }
}

/// UD-058 specification, domains and tangent: parameters outside the domains are refused by the
/// corrected model at `initMaterial`, and after a parameter update at the
/// next evaluation; plane stress is refused; the upstream mode accepts them.
#[test]
fn ud058_parameter_domains_are_refused() {
    let model = one_element(2);
    for extra in [
        "  Dc = 1\n",
        "  Dc = 0\n",
        "  a = 0\n",
        "  A = -1\n",
        "  K0 = 0\n",
        "  Plane_Stress = 1\n",
        "nu = 0.5",
    ] {
        let dat = if extra.starts_with("nu") {
            DAT.replace("nu = 0.2", extra)
        } else {
            DAT.replace("  nu = 0.2\n", &format!("  nu = 0.2\n{extra}"))
        };
        let err = materials_from_dat_with(&model, dat.as_bytes(), Some(corrected()))
            .err()
            .unwrap_or_else(|| panic!("{extra}: accepted"));
        assert!(err.to_string().contains("UD-058"), "{extra}: {err}");
        assert!(
            materials_from_dat_with(&model, dat.as_bytes(), Some(LawOptions::upstream())).is_ok(),
            "{extra}: upstream mode"
        );
    }
    let timed = TimedModel {
        inner: &model,
        dt: 0.,
    };
    let mut h = materials_from_dat_with(&model, DAT.as_bytes(), Some(corrected())).unwrap();
    let l = h.law_mut(0).unwrap();
    // Dc is parsable only (upstream): set through the input path
    l.set_param_from_input("Dc", ParamValue::Real(1.5)).unwrap();
    let err = l
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(2), NG)
        .unwrap_err();
    assert!(err.to_string().contains("UD-058"), "{err}");
}
