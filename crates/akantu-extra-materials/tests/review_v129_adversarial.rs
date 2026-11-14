// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): adversarial checks of
// extra_packages/extra-materials/src/material_damage/
// material_damage_iterative_non_local{.cc,_inline_impl.hh} and
// material_orthotropic_damage_iterative_non_local_inline_impl.hh.

//! Adversarial cases, written independently of the port:
//!
//! * A1: the whole iterative-damage loop (solve, `updateDamage`, repeat)
//!   through `damage_iterative_non_local` with a radius smaller than the
//!   smallest distance between quadrature points equals the local
//!   `damage_iterative` run: same damaged counts per step, the same damage
//!   and stress fields bit for bit (the neighbourhood is the point itself,
//!   so the average is the local value to one rounding).
//! * A2: UD-573 on the non-local variants (the branch's UD-573 sentence
//!   "the per-type reset of `computeNonLocalStress` follows the same
//!   switch", untested on the branch): on the T3/Q4 mesh of
//!   `multi_type.rs`, the maximum normalised equivalent stress of the
//!   non-local pass is the maximum over both types (2) by default and the
//!   last type's (0.5) with `equivalent_stress_max_over_types = false`,
//!   for both iterative non-local laws.
//! * A3: as A1 for `damage_orthotropic_iterative_non_local` against the local
//!   `orthotropic_damage_iterative` (kills mutant R4: the reset dropped).
//! * A4: `vreepeerlings_non_local` with a rate-dependent threshold
//!   (`Arate = 10`): the averaged rate is used (kills mutant R2).

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real, UInt};
use akantu_extra_materials::non_local::MaterialDamageIterativeNonLocal;
use akantu_extra_materials::{
    ExtraLawOptions, MaterialDamageIterative, as_iterative_damage,
    register_extra_non_local_materials_with,
};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_solid::SolidMechanicsModel;
use common::{data, model_with_options, read_mesh, with_iterative, with_law};

const T3: ElementType = ElementType::Triangle3;
const Q4: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

fn model(mesh: akantu_mesh::Mesh, dat: &str, options: ExtraLawOptions) -> SolidMechanicsModel {
    let mut m = model_with_options(mesh, dat, options);
    register_extra_non_local_materials_with(&mut m.physics.factory, options).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    m
}

fn field(m: &SolidMechanicsModel, name: &str) -> Vec<Real> {
    let mut out = None;
    m.material(0).unwrap().for_each_internal(&mut |n, f| {
        if n == name
            && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
        {
            out = Some(f.get(T3, NG).unwrap().as_slice().to_vec());
        }
    });
    out.unwrap()
}

fn stress(m: &SolidMechanicsModel) -> Vec<Real> {
    m.material(0)
        .unwrap()
        .base()
        .stress
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec()
}

const PARAMS: &str =
    " rho = 1\n E = 1\n nu = 0.\n Sc = 10\n prescribed_dam = 0.1\n max_damage = 0.99\n";

/// A1 (module docs).
#[test]
fn v129_a1_damage_loop_with_a_point_sized_radius_equals_the_local_law() {
    let local_dat = format!("material damage_iterative [\n name = m\n{PARAMS}]\n");
    let nl_dat = format!(
        "material damage_iterative_non_local [\n name = m\n{PARAMS}]\n\
         non_local m base_wf [\n radius = 0.05\n]\n"
    );
    let run = |dat: &str, non_local: bool| -> (Vec<UInt>, Vec<Real>, Vec<Real>) {
        let mesh = read_mesh(&data("test_material_damage/plate.msh"), 2);
        let mut m = model(mesh, dat, ExtraLawOptions::default());
        for (value, axis, group) in [
            (0., akantu_core::enums::SpatialDirection::X, "left"),
            (0., akantu_core::enums::SpatialDirection::Y, "bottom"),
            (2., akantu_core::enums::SpatialDirection::Y, "top"),
        ] {
            m.apply_dirichlet_bc(&FixedValue { value, axis }, group)
                .unwrap();
        }
        // distinct strengths: σ_eq = 1/Sc at the uniform state
        with_law(&mut m, 0, |law, _| {
            let any = law.as_any_mut().unwrap();
            let sc = if let Some(l) = any.downcast_mut::<MaterialDamageIterative<2>>() {
                &mut l.sc
            } else {
                &mut any
                    .downcast_mut::<MaterialDamageIterativeNonLocal<2>>()
                    .unwrap()
                    .local
                    .sc
            };
            let mut sc = sc.get_mut(T3, NG)?;
            for (q, s) in sc.as_mut_slice().iter_mut().enumerate() {
                *s = 0.8 + 0.0137 * ((q * 7) % 40) as Real;
            }
            Ok(())
        });
        if non_local {
            let n = m.non_local_manager().unwrap().neighborhood("m").unwrap();
            assert_eq!(n.radius(), 0.05);
        }
        let mut counts = Vec::new();
        for _ in 0..12 {
            m.solve_step(None).unwrap();
            let max = as_iterative_damage(m.material(0).unwrap())
                .unwrap()
                .norm_max_equivalent_stress();
            let n = if max >= 1. {
                with_iterative(&mut m, 0, |l, ctx| l.update_damage(ctx))
            } else {
                0
            };
            counts.push(n);
        }
        m.solve_step(None).unwrap();
        (counts, field(&m, "damage"), stress(&m))
    };
    let (cl, dl, sl) = run(&local_dat, false);
    let (cn, dn, sn) = run(&nl_dat, true);
    eprintln!("A1 counts local {cl:?} non-local {cn:?}");
    assert!(cl.iter().sum::<UInt>() > 5, "the loop damages: {cl:?}");
    assert_eq!(cl, cn, "damaged counts per step");
    assert_eq!(dl, dn, "damage field");
    assert_eq!(sl, sn, "stress field");
}

/// A2 (module docs): `u_x` piecewise linear (strain 5 on the quadrangle,
/// 20 on the triangles), `ν = 0`, `Sc = 10`: point values 0.5 and 2.
#[test]
fn v129_a2_non_local_maximum_over_types_ud573() {
    for kw in [
        "damage_iterative_non_local",
        "damage_orthotropic_iterative_non_local",
    ] {
        for (over_types, want) in [(true, 2.), (false, 0.5)] {
            let options = ExtraLawOptions {
                equivalent_stress_max_over_types: over_types,
                ..ExtraLawOptions::default()
            };
            let dat = format!(
                "material {kw} [\n name = m\n{PARAMS}]\n\
                 non_local m base_wf [\n radius = 0.05\n]\n"
            );
            let mesh = read_mesh(&data("mixed/mixed_t3_q4.msh"), 2);
            let mut m = model(mesh, &dat, options);
            {
                let x: Vec<Real> = m
                    .physics
                    .base
                    .mesh
                    .read()
                    .unwrap()
                    .nodes()
                    .as_slice()
                    .to_vec();
                let mut u = m.displacement_mut().unwrap();
                for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
                    let ux = if p[0] <= 0.25 {
                        5. * p[0]
                    } else {
                        5. * 0.25 + 20. * (p[0] - 0.25)
                    };
                    u.as_mut_slice()[2 * n] = ux;
                    u.as_mut_slice()[2 * n + 1] = 0.;
                }
            }
            m.assemble_internal_forces().unwrap();
            let law = m.material(0).unwrap();
            for ty in [T3, Q4] {
                assert!(!law.base().element_filter.get(ty, NG).unwrap().is_empty());
            }
            let max = as_iterative_damage(law)
                .unwrap()
                .norm_max_equivalent_stress();
            eprintln!("A2 {kw} over_types={over_types}: max {max:?}");
            assert!(
                (max - want).abs() <= 1e-12 * want,
                "{kw} over_types={over_types}: {max} vs {want}"
            );
        }
    }
}

/// A3: as A1 for `damage_orthotropic_iterative_non_local` against the local
/// `orthotropic_damage_iterative` (one point per update): the reset of the
/// maximum before the non-local pass (the local pass's prologue, UD-1005's
/// "no reset of its own") is exercised by the loop (with the reset
/// dropped this test fails, the other tests of the branch pass).
#[test]
fn v129_a3_orthotropic_damage_loop_with_a_point_sized_radius_equals_the_local_law() {
    use akantu_extra_materials::non_local::MaterialOrthotropicDamageIterativeNonLocal;
    use akantu_extra_materials::orthotropic_damage::MaterialOrthotropicDamageIterative;
    let params =
        " rho = 1\n E = 1\n nu = 0.2\n Sc = 10\n prescribed_dam = 0.1\n max_damage = 0.99\n";
    let local_dat = format!("material orthotropic_damage_iterative [\n name = m\n{params}]\n");
    let nl_dat = format!(
        "material damage_orthotropic_iterative_non_local [\n name = m\n{params}]\n\
         non_local m base_wf [\n radius = 0.05\n]\n"
    );
    let run = |dat: &str| -> (Vec<UInt>, Vec<Real>, Vec<Real>) {
        let mesh = read_mesh(&data("test_material_damage/plate.msh"), 2);
        let mut m = model(mesh, dat, ExtraLawOptions::default());
        for (value, axis, group) in [
            (0., akantu_core::enums::SpatialDirection::X, "left"),
            (0., akantu_core::enums::SpatialDirection::Y, "bottom"),
            (2., akantu_core::enums::SpatialDirection::Y, "top"),
        ] {
            m.apply_dirichlet_bc(&FixedValue { value, axis }, group)
                .unwrap();
        }
        with_law(&mut m, 0, |law, _| {
            let any = law.as_any_mut().unwrap();
            let sc = if let Some(l) = any.downcast_mut::<MaterialOrthotropicDamageIterative<2>>() {
                &mut l.sc
            } else {
                &mut any
                    .downcast_mut::<MaterialOrthotropicDamageIterativeNonLocal<2>>()
                    .unwrap()
                    .local
                    .sc
            };
            let mut sc = sc.get_mut(T3, NG)?;
            for (q, s) in sc.as_mut_slice().iter_mut().enumerate() {
                *s = 0.8 + 0.0137 * ((q * 7) % 40) as Real;
            }
            Ok(())
        });
        let mut counts = Vec::new();
        for _ in 0..12 {
            m.solve_step(None).unwrap();
            let max = as_iterative_damage(m.material(0).unwrap())
                .unwrap()
                .norm_max_equivalent_stress();
            let n = if max >= 1. {
                with_iterative(&mut m, 0, |l, ctx| l.update_damage(ctx))
            } else {
                0
            };
            counts.push(n);
        }
        m.solve_step(None).unwrap();
        (counts, field(&m, "damage"), stress(&m))
    };
    let (cl, dl, sl) = run(&local_dat);
    let (cn, dn, sn) = run(&nl_dat);
    eprintln!("A3 counts local {cl:?} non-local {cn:?}");
    assert!(cl.iter().sum::<UInt>() > 3, "the loop damages: {cl:?}");
    assert_eq!(cl, cn, "damaged counts per step");
    assert_eq!(dl, dn, "damage field");
    assert_eq!(sl, sn, "stress field");
}

/// A4: `vreepeerlings_non_local` with a rate-dependent
/// threshold (`Arate = 10`; the branch's test keeps the code's default
/// `Arate = 0`, UD-782, under which `kapao = Crate` and the rate never
/// enters the damage): the damage of every point is the point update of the
/// **averaged** strain **and rate** (bit for bit), and the update with the
/// averaged strain but the **local** rate differs somewhere, so a
/// non-local pass that uses the local rate fails here.
#[test]
fn v129_a4_vreepeerlings_non_local_uses_the_averaged_rate() {
    use akantu_extra_materials::non_local::MaterialVreePeerlingsNonLocal;
    let radius = 0.6;
    let wavy = |x: Real, y: Real| {
        [
            1e-3 * (0.7 * x * x + 0.4 * x * y - 0.2 * y),
            1e-3 * (0.9 * y * y - 0.5 * x * x + 0.3 * x),
        ]
    };
    let vel = |x: Real, y: Real| [2e-1 * (x * y + 0.3 * y), 2e-1 * (0.5 * x * x - y)];
    let build = |params: &str| {
        let dat = format!(
            "material vreepeerlings_non_local [\n name = m\n rho = 1\n{params}]\n\
             non_local m base_wf [\n radius = {radius}\n]\n"
        );
        let mesh = read_mesh(&data("test_material_damage/plate.msh"), 2);
        let options = ExtraLawOptions::default();
        let mut m = model_with_options(mesh, &dat, options);
        register_extra_non_local_materials_with(&mut m.physics.factory, options).unwrap();
        m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap();
        let x: Vec<Real> = m
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        {
            let mut d = m.displacement_mut().unwrap();
            for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
                d.as_mut_slice()[2 * n..2 * n + 2].copy_from_slice(&wavy(p[0], p[1]));
            }
        }
        {
            let mut v = m.velocity_mut().unwrap();
            for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
                v.as_mut_slice()[2 * n..2 * n + 2].copy_from_slice(&vel(p[0], p[1]));
            }
        }
        m.assemble_internal_forces().unwrap();
        m
    };
    let probe = build(" E = 1e9\n nu = 0.2\n Kapaoi = 0.1\n Kapac = 2\n Crate = 0.1\n");
    let es = field(&probe, "equi-strain_non_local");
    let (lo, hi) = es
        .iter()
        .fold((Real::INFINITY, Real::NEG_INFINITY), |(a, b), &x| {
            (a.min(x), b.max(x))
        });
    let kapaoi = lo + 0.3 * (hi - lo);
    let params = format!(
        " E = 1e9\n nu = 0.2\n Kapaoi = {kapaoi:e}\n Kapac = {:e}\n Crate = {:e}\n Arate = 10\n Brate = 1\n",
        10. * hi,
        0.5 * kapaoi
    );
    let m = build(&params);
    let (es_nl, esr_nl, esr) = (
        field(&m, "equi-strain_non_local"),
        field(&m, "equi-strain-rate_non_local"),
        field(&m, "equi-strain-rate"),
    );
    let (dam, kapa) = (field(&m, "damage"), field(&m, "Kapa"));
    let law = m
        .material(0)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialVreePeerlingsNonLocal<2>>()
        .unwrap();
    assert_eq!(law.local.arate, 10.);
    let mut differs = 0;
    for q in 0..dam.len() {
        let (d, ..) = law
            .local
            .damage_on_quad(0., es_nl[q], esr_nl[q], kapa[q], (0., 0., 0.));
        assert_eq!(dam[q].to_bits(), d.to_bits(), "q{q}");
        let (dl, ..) = law
            .local
            .damage_on_quad(0., es_nl[q], esr[q], kapa[q], (0., 0., 0.));
        differs += usize::from(dl != d);
    }
    eprintln!(
        "A4: {} damaged of {}, {differs} points where the local rate gives another damage",
        dam.iter().filter(|&&d| d > 0.).count(),
        dam.len()
    );
    assert!(dam.iter().any(|&d| d > 0.) && dam.contains(&0.));
    assert!(differs > 0, "the rate does not matter in this case");
}
