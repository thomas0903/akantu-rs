// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks
// of the `phasefield` material (src/model/solid_mechanics/materials/
// material_damage/material_phasefield*, src/model/phase_field/energy_split*,
// energy_splits/*) at material-point level: degradation bounds,
// split reconstruction, energy identities, finite-difference tangents (and
// the upstream tangent defects), thermal stored energy (UD-109), degraded
// mass. The upstream phase-field tests (test/test_model/
// test_phase_field_model/*) need the phase-field model and the coupler and
// are deferred to that work package.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::{Element, Real};
use akantu_model::constitutive_law::{InternalFieldMut, LawPhase};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::phasefield::{EnergySplit, EnergySplitKind};
use akantu_validation::reduce::NanMinMax;
use common::{NG, SimplexModel, materials_from_dat_with, simplex};
use timed::{Point, TimedModel, one_element};

type Handler = akantu_model::ConstitutiveLawsHandler<dyn Material>;

fn dat(split: &str, extra: &str) -> String {
    format!(
        "material phasefield {split} [\n  name = plate\n  rho = 2.\n  E = 210.0\n  nu = 0.3\n{extra}]\n"
    )
}

fn law(model: &SimplexModel, text: &str, upstream: bool) -> Handler {
    materials_from_dat_with(model, text.as_bytes(), upstream.then(LawOptions::upstream)).unwrap()
}

/// Column-major gradient from a seed (symmetric part dominates).
fn grad(dim: usize, seed: u32, scale: Real) -> Vec<Real> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(12345);
    (0..dim * dim)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (f64::from((x >> 8) & 0xffff) / 65535. - 0.5) * scale
        })
        .collect()
}

/// Stress after one evaluation at damage `d` (current damage written
/// before the stress, as the coupler does).
fn stress_at(p: &mut Point<'_>, timed: &TimedModel<'_>, g: &[Real], d: Real) -> Vec<Real> {
    p.write_internal("damage", &[d]);
    p.set_grad_u(g);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), p.ty, NG)
        .unwrap();
    p.stress()
}

/// `g(d) = (1 − d)² + η`: `g(0) = 1 + η`, `g(1) = η`, decreasing on [0, 1].
#[test]
fn degradation_function_bounds() {
    let model = one_element(2);
    for eta in [0.0, 1e-3] {
        let mut h = law(&model, &dat("no_split", &format!("  eta = {eta}\n")), false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(2),
        };
        let g = [1e-3, 0., 0., 0.];
        let s0 = stress_at(&mut p, &timed, &g, 0.)[0];
        let mut last = Real::INFINITY;
        for k in 0..=20 {
            let d = f64::from(k) / 20.;
            let ratio = stress_at(&mut p, &timed, &g, d)[0] / s0;
            let want = ((1. - d) * (1. - d) + eta) / (1. + eta);
            assert!(
                (ratio - want).abs() < 1e-14,
                "eta {eta} d {d}: {ratio} vs {want}"
            );
            assert!(ratio <= last);
            last = ratio;
        }
        assert!((last - eta / (1. + eta)).abs() < 1e-15);
    }
}

/// Undamaged (`d = 0`, `η = 0`) both splits give the elastic stress
/// `λ tr ε I + 2μ ε` in every dimension (plane strain and plane stress in
/// 2-D); bound 16 u·max|σ| (the split sums traces sequentially).
#[test]
fn undamaged_split_recovers_elastic_stress() {
    for dim in 1..=3 {
        for split in ["no_split", "volumetric_deviatoric"] {
            for plane_stress in [false, true] {
                if plane_stress && dim != 2 {
                    continue;
                }
                let extra = if plane_stress {
                    "  Plane_Stress = true\n"
                } else {
                    ""
                };
                let model = one_element(dim);
                let mut h = law(&model, &dat(split, extra), false);
                let l = h.law_mut(0).unwrap();
                let timed = TimedModel {
                    inner: &model,
                    dt: 0.,
                };
                let mut p = Point {
                    law: l,
                    ty: simplex(dim),
                };
                for seed in 0..10 {
                    let g = grad(dim, seed, 2e-3);
                    let s = stress_at(&mut p, &timed, &g, 0.);
                    let (e, nu) = (210.0_f64, if dim == 1 { 0.0 } else { 0.3_f64 });
                    let lambda = if plane_stress {
                        nu * e / ((1. + nu) * (1. - nu))
                    } else {
                        nu * e / ((1. + nu) * (1. - 2. * nu))
                    };
                    let mu = e / (2. * (1. + nu));
                    let tr: Real = (0..dim).map(|i| g[i * (dim + 1)]).sum();
                    let max = s.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
                    for r in 0..dim {
                        for c in 0..dim {
                            let eps = 0.5 * (g[r + c * dim] + g[c + r * dim]);
                            let want = 2. * mu * eps + if r == c { lambda * tr } else { 0. };
                            let got = s[r + c * dim];
                            assert!(
                                (got - want).abs() <= 16. * Real::EPSILON * max,
                                "{dim}-D {split} ps={plane_stress}: {got} vs {want}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// The volumetric–deviatoric split reconstructs the strain
/// (`ε = dev + (tr/3)I`), the stress (`σ⁺ + σ⁻ = σ_elastic`) and the energy
/// (`ψ⁺ + ½κ⟨tr⟩₋² = ½λ tr² + μ ε:ε`); at `d = 1`, `η = 0` the no-split law
/// carries nothing and the volumetric–deviatoric law transmits volumetric
/// compression only.
#[test]
fn volumetric_deviatoric_split_reconstruction() {
    let vol_dev = EnergySplit::new(EnergySplitKind::VolumetricDeviatoric, 210., 0.3, false);
    let no_split = EnergySplit::new(EnergySplitKind::NoSplit, 210., 0.3, false);
    let split = vol_dev;
    for seed in 0..50 {
        let g = grad(3, seed, 2e-3);
        let eps: [[Real; 3]; 3] =
            std::array::from_fn(|j| std::array::from_fn(|i| 0.5 * (g[i + 3 * j] + g[j + 3 * i])));
        let (plus, minus) = vol_dev.sigma(&eps, 0.);
        let (el, zero) = no_split.sigma(&eps, 0.);
        assert_eq!(zero, [[0.0; 3]; 3]);
        let tr = eps[0][0] + eps[1][1] + eps[2][2];
        for j in 0..3 {
            for i in 0..3 {
                assert!((plus[j][i] + minus[j][i] - el[j][i]).abs() < 1e-13 * 0.5);
            }
        }
        let k = split.kappa();
        let psi_plus = vol_dev.phi(&eps);
        let psi_minus = 0.5 * k * tr.min(0.) * tr.min(0.);
        let psi = no_split.phi(&eps);
        assert!((psi_plus + psi_minus - psi).abs() <= 1e-12 * psi.abs());
        // minus part is purely volumetric and compressive
        if tr >= 0. {
            assert_eq!(minus, [[0.0; 3]; 3]);
        } else {
            assert!(minus[0][0] < 0. && minus[1][0] == 0.);
        }
    }
    // fully broken material point
    let model = one_element(3);
    for (s, compressive_only) in [("no_split", false), ("volumetric_deviatoric", true)] {
        let mut h = law(&model, &dat(s, ""), false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        let tension = stress_at(
            &mut p,
            &timed,
            &[1e-3, 0., 0., 0., 1e-3, 0., 0., 0., 1e-3],
            1.,
        );
        assert!(tension.iter().all(|x| *x == 0.), "{s}: {tension:?}");
        let comp = stress_at(
            &mut p,
            &timed,
            &[-1e-3, 2e-4, 0., 2e-4, -1e-3, 0., 0., 0., -1e-3],
            1.,
        );
        if compressive_only {
            let k = 210. / (3. * (1. - 0.6));
            assert!((comp[0] - k * (-3e-3)).abs() < 1e-12 && comp[1] == 0.);
        } else {
            assert!(comp.iter().all(|x| *x == 0.));
        }
    }
}

/// `½σ:ε = g·ψ⁺ + ψ⁻` (potential energy of the law against the split's
/// driving energy `computePhiOnQuad`), both splits, damaged.
#[test]
fn potential_energy_is_degraded_split_energy() {
    for split_name in ["no_split", "volumetric_deviatoric"] {
        let kind = EnergySplitKind::from_option(split_name).unwrap();
        let model = one_element(3);
        let mut h = law(&model, &dat(split_name, "  eta = 1e-2\n"), false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        let split = EnergySplit::new(kind, 210., 0.3, false);
        for seed in 0..20 {
            let g = grad(3, seed, 2e-3);
            let d = 0.37;
            stress_at(&mut p, &timed, &g, d);
            p.law
                .compute_potential_energy(&timed.ctx(LawPhase::Energy), simplex(3))
                .unwrap();
            let w = p
                .law
                .base()
                .potential_energy
                .get(simplex(3), NG)
                .unwrap()
                .as_slice()[0];
            let eps: [[Real; 3]; 3] = std::array::from_fn(|j| {
                std::array::from_fn(|i| 0.5 * (g[i + 3 * j] + g[j + 3 * i]))
            });
            let tr = eps[0][0] + eps[1][1] + eps[2][2];
            let gd = (1. - d) * (1. - d) + 1e-2;
            let psi_minus = match kind {
                EnergySplitKind::NoSplit => 0.,
                EnergySplitKind::VolumetricDeviatoric => 0.5 * split.kappa() * tr.min(0.).powi(2),
            };
            let want = gd * split.phi(&eps) + psi_minus;
            assert!(
                (w - want).abs() <= 1e-12 * want.abs(),
                "{split_name}: {w} vs {want}"
            );
        }
    }
}

/// Central finite-difference tangent (engineering shear in Voigt order) of
/// the stress at a point with damage `d`.
fn fd_tangent(
    p: &mut Point<'_>,
    timed: &TimedModel<'_>,
    g: &[Real],
    d: Real,
    dim: usize,
) -> Vec<Real> {
    let v = dim * (dim + 1) / 2;
    let pairs: Vec<(usize, usize)> = match dim {
        1 => vec![(0, 0)],
        2 => vec![(0, 0), (1, 1), (0, 1)],
        _ => vec![(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)],
    };
    let h = 1e-7;
    let mut out = vec![0.0; v * v];
    for (col, &(a, b)) in pairs.iter().enumerate() {
        let perturbed = |sign: Real| {
            let mut gp = g.to_vec();
            if a == b {
                gp[a + a * dim] += sign * h;
            } else {
                // engineering shear γ = 2ε_ab: ε_ab += h/2 through both entries
                gp[a + b * dim] += sign * h / 2.;
                gp[b + a * dim] += sign * h / 2.;
            }
            gp
        };
        let sp = stress_at(p, timed, &perturbed(1.), d);
        let sm = stress_at(p, timed, &perturbed(-1.), d);
        for (row, &(i, j)) in pairs.iter().enumerate() {
            out[row + col * v] = (sp[i + j * dim] - sm[i + j * dim]) / (2. * h);
        }
    }
    out
}

/// The tangent (consistent mode, default) equals the finite-difference
/// derivative of the stress for both splits in 1-D, 2-D and 3-D, in tension
/// and compression with damage; it is symmetric. Upstream's
/// volumetric–deviatoric tangent differs in 1-D compression with damage
/// (candidate: `g_h·E` instead of `g_h κ + g·(4/3)μ`). Bound
/// 1e-6 relative (the stress is piecewise linear: the central difference is
/// exact up to `u·|σ|/h ≈ 1e-9` relative).
#[test]
fn tangent_matches_finite_differences() {
    for dim in 1..=3 {
        for split in ["no_split", "volumetric_deviatoric"] {
            for upstream in [false, true] {
                let model = one_element(dim);
                let mut h = law(&model, &dat(split, "  eta = 1e-3\n"), upstream);
                let l = h.law_mut(0).unwrap();
                let timed = TimedModel {
                    inner: &model,
                    dt: 0.,
                };
                let mut p = Point {
                    law: l,
                    ty: simplex(dim),
                };
                for (seed, sign) in [(1, 1.), (2, -1.), (3, 1.), (4, -1.)] {
                    let mut g = grad(dim, seed, 2e-3);
                    // make the trace clearly positive / negative
                    for i in 0..dim {
                        g[i * (dim + 1)] = sign * (1e-3 + g[i * (dim + 1)].abs());
                    }
                    let d = 0.4;
                    let fd = fd_tangent(&mut p, &timed, &g, d, dim);
                    stress_at(&mut p, &timed, &g, d);
                    let t = p.tangent(&timed);
                    let v = dim * (dim + 1) / 2;
                    let max = fd.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
                    let err = t
                        .iter()
                        .zip(&fd)
                        .fold(0.0_f64, |m, (a, b)| m.nan_max((a - b).abs()));
                    for i in 0..v {
                        for j in 0..v {
                            assert_eq!(t[i + j * v].to_bits(), t[j + i * v].to_bits());
                        }
                    }
                    let defect =
                        upstream && dim == 1 && split == "volumetric_deviatoric" && sign < 0.;
                    if defect {
                        assert!(
                            err > 0.1 * max,
                            "1-D vol/dev compression: upstream tangent unexpectedly consistent"
                        );
                    } else {
                        assert!(
                            err <= 1e-6 * max,
                            "{dim}-D {split} upstream={upstream} sign {sign}: {t:?} vs {fd:?}"
                        );
                    }
                }
            }
        }
    }
}

/// With a thermal strain the stress splits on `tr(ε − ε_th I)`; the
/// consistent tangent tests the same sign (FD-exact), upstream tests the
/// total-strain trace (candidate) and is wrong in the band
/// `0 < tr ε < 3ε_th`.
#[test]
fn thermal_tangent_sign_test() {
    for upstream in [false, true] {
        let model = one_element(3);
        let mut h = law(
            &model,
            &dat("volumetric_deviatoric", "  alpha = 1e-5\n  delta_T = 30\n"),
            upstream,
        );
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        // tr ε = 3e-4 > 0 while 3 ε_th = 9e-4 (or 1.17e-3 upstream)
        let g = [1e-4, 2e-5, 0., 2e-5, 1e-4, 0., 0., 0., 1e-4];
        let d = 0.5;
        let fd = fd_tangent(&mut p, &timed, &g, d, 3);
        stress_at(&mut p, &timed, &g, d);
        let t = p.tangent(&timed);
        let max = fd.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
        let err = t
            .iter()
            .zip(&fd)
            .fold(0.0_f64, |m, (a, b)| m.nan_max((a - b).abs()));
        if upstream {
            assert!(err > 0.1 * max);
        } else {
            assert!(err <= 1e-6 * max, "{t:?} vs {fd:?}");
        }
    }
}

/// UD-109 for the phase-field law: with a temperature change the default
/// energy `½σ:(ε − ε_th I)` is a potential of the stress (FD of `W` with
/// respect to every `∇u` component equals σ); upstream's `½σ:∇u` is not.
#[test]
fn thermal_energy_is_a_potential_of_the_stress() {
    for split in ["no_split", "volumetric_deviatoric"] {
        for upstream in [false, true] {
            let model = one_element(2);
            let mut h = law(
                &model,
                &dat(split, "  alpha = 1e-5\n  delta_T = 30\n"),
                upstream,
            );
            let l = h.law_mut(0).unwrap();
            let timed = TimedModel {
                inner: &model,
                dt: 0.,
            };
            let mut p = Point {
                law: l,
                ty: simplex(2),
            };
            let g = [8e-4, 1e-4, 3e-4, 5e-4];
            let d = 0.2;
            let energy = |p: &mut Point<'_>, g: &[Real]| {
                stress_at(p, &timed, g, d);
                p.law
                    .compute_potential_energy(&timed.ctx(LawPhase::Energy), simplex(2))
                    .unwrap();
                p.law
                    .base()
                    .potential_energy
                    .get(simplex(2), NG)
                    .unwrap()
                    .as_slice()[0]
            };
            let s = stress_at(&mut p, &timed, &g, d);
            let h = 1e-8;
            let mut worst = 0.0_f64;
            for l in 0..4 {
                let mut gp = g.to_vec();
                gp[l] += h;
                let mut gm = g.to_vec();
                gm[l] -= h;
                let dw = (energy(&mut p, &gp) - energy(&mut p, &gm)) / (2. * h);
                worst = worst.nan_max((dw - s[l]).abs());
            }
            let max = s.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
            if upstream {
                assert!(
                    worst > 1e-3 * max,
                    "{split}: upstream energy unexpectedly a potential"
                );
            } else {
                assert!(
                    worst <= 1e-6 * max,
                    "{split}: dW/d∇u − σ = {worst} (σ ~ {max})"
                );
            }
        }
    }
}

/// `degrade_mass`: `ρ_q = min(ρ, ρ·g(d_q))` per quadrature point (two points
/// with different damage: upstream scales every point by the first one and
/// writes out of bounds, candidate), mass matrix flagged changed.
#[test]
fn degraded_mass_per_quadrature_point() {
    let model = one_element(2);
    let mut h = law(
        &model,
        &dat("no_split", "  degrade_mass = true\n  eta = 1e-2\n"),
        false,
    );
    let l = h.law_mut(0).unwrap();
    assert!(l.has_mass_matrix_changed() && l.has_matrix_changed("M"));
    let ty = simplex(2);
    l.for_each_internal_mut(&mut |n, f| {
        if n == "damage"
            && let InternalFieldMut::Real(f) = f
        {
            f.resize(ty, NG, 2)?;
            f.get_mut(ty, NG)?
                .as_mut_slice()
                .copy_from_slice(&[0.5, 0.0]);
        }
        Ok(())
    })
    .unwrap();
    let mut rho = [0.0; 2];
    l.rho_on_quadrature_points(Element::new(ty, 0, NG), &mut rho);
    assert_eq!(rho, [2. * (0.25 + 1e-2), 2.]);
    let mut h2 = law(&model, &dat("no_split", ""), false);
    let l2 = h2.law_mut(0).unwrap();
    assert!(!l2.has_matrix_changed("M"));
    let mut r = [0.0];
    l2.rho_on_quadrature_points(Element::new(ty, 0, NG), &mut r);
    assert_eq!(r, [2.]);
}

/// Options: `no_split` (also the empty option) and `volumetric_deviatoric`;
/// anything else is rejected by the allocator.
#[test]
fn phasefield_options() {
    let model = one_element(2);
    assert!(materials_from_dat_with(&model, dat("", "").as_bytes(), None).is_ok());
    assert!(materials_from_dat_with(&model, dat("hybrid", "").as_bytes(), None).is_err());
    assert_eq!(
        EnergySplitKind::from_option("").unwrap(),
        EnergySplitKind::NoSplit
    );
    // eta is parsable but not readable (upstream registration flags)
    let mut h = law(&model, &dat("no_split", "  eta = 0.5\n"), false);
    let l = h.law_mut(0).unwrap();
    assert!(l.get_param("eta").is_err());
    assert!(l.get_param("degrade_mass").is_ok() && l.get_param("is_isotropic").is_ok());
}
