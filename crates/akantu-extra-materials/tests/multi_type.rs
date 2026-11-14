// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): independent checks of extra_packages/
// extra-materials/src/material_damage/material_damage_iterative.cc
// (computeStress / computeAllStresses / updateDamage) and
// src/material_FE2/solid_mechanics_model_RVE.cc (volume, averageTensorField,
// homogenizeStiffness) on a mesh with several element types.

//! Multi-type checks of two corrected defaults: no
//! upstream test uses a mesh with several element types, so UD-573 (the
//! maximum equivalent stress over the element types) and UD-585 (the RVE
//! volume summed over the types) need their own tests. Mesh:
//! `tests/data/mixed/mixed_t3_q4.msh`, the unit square with one
//! `_quadrangle_4` on `[0, 0.25] × [0, 1]` and two `_triangle_3` on the rest.
//! Expected values are closed forms (areas, the elastic stress of a
//! piecewise-affine field), not calls into the laws.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::tensor::Matrix;
use akantu_core::{ElementType, GhostType, Real};
use akantu_extra_materials::{
    ExtraLawOptions, MaterialDamageIterative, SolidMechanicsModelRve, as_iterative_damage,
};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use common::{data, model_with_options, read_mesh, with_iterative};

const T3: ElementType = ElementType::Triangle3;
const Q4: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

fn mixed_mesh() -> akantu_mesh::Mesh {
    read_mesh(&data("mixed/mixed_t3_q4.msh"), 2)
}

/// `u_x` piecewise linear in `x` (slope `e1` on the quadrangle `x ≤ 0.25`,
/// `e2` on the triangles), `u_y = 0`: uniform uniaxial strains `e1` (Q4,
/// reproduced exactly by the bilinear element) and `e2` (T3).
fn set_two_strains(model: &mut SolidMechanicsModel, e1: Real, e2: Real) {
    let x: Vec<Real> = {
        let mesh = model.physics.base.mesh.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let mut u = model.displacement_mut().unwrap();
    for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
        let ux = if p[0] <= 0.25 {
            e1 * p[0]
        } else {
            e1 * 0.25 + e2 * (p[0] - 0.25)
        };
        u.as_mut_slice()[2 * n] = ux;
        u.as_mut_slice()[2 * n + 1] = 0.;
    }
}

const DAMAGE_ITERATIVE: &str = "material damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 10\n prescribed_dam = 0.1\n max_damage = 0.99\n]\n";

/// UD-573: with `ν = 0`, `σ_eq = E·ε/Sc`: 0.5 on the quadrangle
/// (`ε = 5`), 2 on the triangles (`ε = 20`). Default: the maximum over both
/// types (2), so `updateDamage` damages the two triangle points; upstream
/// (per-type reset): the maximum of the type evaluated last, the
/// quadrangle (element-type order `_triangle_3 < _quadrangle_4`), 0.5 < 1,
/// so nothing is damaged.
#[test]
fn damage_iterative_max_over_types_default_vs_upstream_last_type() {
    for (options, want_max, want_damaged) in [
        (ExtraLawOptions::default(), 2., 2),
        (
            ExtraLawOptions {
                equivalent_stress_max_over_types: false,
                ..ExtraLawOptions::default()
            },
            0.5,
            0,
        ),
    ] {
        let mut model = model_with_options(mixed_mesh(), DAMAGE_ITERATIVE, options);
        model
            .init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        set_two_strains(&mut model, 5., 20.);
        model.assemble_internal_forces().unwrap();
        let law = model.material(0).unwrap();
        // both types hold points of the law
        for ty in [T3, Q4] {
            assert!(!law.base().element_filter.get(ty, NG).unwrap().is_empty());
        }
        let max = as_iterative_damage(law)
            .unwrap()
            .norm_max_equivalent_stress();
        assert!(
            (max - want_max).abs() <= 1e-12 * want_max,
            "max over types {}: {max} vs {want_max}",
            options.equivalent_stress_max_over_types
        );
        let nb = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
        assert_eq!(nb, want_damaged);
        let m = model
            .material(0)
            .unwrap()
            .as_any()
            .unwrap()
            .downcast_ref::<MaterialDamageIterative<2>>()
            .unwrap();
        let d_t3 = if want_damaged > 0 { 0.1 } else { 0. };
        for &d in m.damage.damage.get(T3, NG).unwrap().as_slice() {
            assert_eq!(d, d_t3);
        }
        for &d in m.damage.damage.get(Q4, NG).unwrap().as_slice() {
            assert_eq!(d, 0.);
        }
    }
}

const ELASTIC: &str = "material elastic [\n name = m\n rho = 1\n E = 3\n nu = 0.25\n]\n";

fn rve(options: ExtraLawOptions) -> SolidMechanicsModelRve {
    let parser = Parser::parse_str(ELASTIC, "material.dat").unwrap();
    let mut rve =
        SolidMechanicsModelRve::new_with_options(mixed_mesh(), false, 0, parser, "rve", options)
            .unwrap();
    rve.init_full().unwrap();
    rve
}

/// UD-585: the RVE volume is the sum of the element areas over the types
/// (1 = 0.25 + 0.75); upstream assigns per type and keeps the last type's
/// (the quadrangle, 0.25). Independent consequence: the average of a
/// constant eigen-gradient `E*` over the RVE is `E*` by default and
/// `E*/0.25 = 4·E*` upstream.
#[test]
fn rve_volume_sum_over_types_default_vs_upstream_last_type() {
    let e_star: Matrix<2, 2> = [[2e-3, 5e-4], [5e-4, -1e-3]];
    for (options, volume, factor) in [
        (ExtraLawOptions::default(), 1., 1.),
        (
            ExtraLawOptions {
                rve_volume_sum_over_types: false,
                ..ExtraLawOptions::default()
            },
            0.25,
            4.,
        ),
    ] {
        let mut rve = rve(options);
        println!(
            "volume {:e} (error {:.1e})",
            rve.volume(),
            (rve.volume() - volume).abs()
        );
        assert!(
            (rve.volume() - volume).abs() <= 1e-15,
            "volume {} vs {volume}",
            rve.volume()
        );
        {
            let law = rve.model.material_mut(0).unwrap();
            for ty in [T3, Q4] {
                let mut a = law.base_mut().eigen_grad_u.get_mut(ty, NG).unwrap();
                assert!(!a.is_empty());
                for t in a.as_mut_slice().as_chunks_mut::<4>().0 {
                    t.copy_from_slice(e_star.as_flattened());
                }
            }
        }
        let avg = rve.homogenize_eigen_grad_u().unwrap();
        let worst = avg
            .as_flattened()
            .iter()
            .zip(e_star.as_flattened())
            .map(|(a, b)| (a - factor * b).abs() / (factor * b.abs()))
            .fold(0_f64, f64::max);
        println!("average eigen-gradient: worst relative error {worst:.1e}");
        for (a, b) in avg.as_flattened().iter().zip(e_star.as_flattened()) {
            assert!(
                (a - factor * b).abs() <= 1e-15 * factor * b.abs(),
                "{avg:?} vs {factor}·{e_star:?}"
            );
        }
    }
}

/// Homogenisation of a homogeneous elastic RVE on the mixed mesh: since
/// `σ = C:ε` at every point, the averages satisfy `Σ̄ = C·Ē` whatever the
/// (corner-driven) displacement field, so `homogenizeStiffness` returns the
/// plane-strain Voigt stiffness (engineering shear) up to rounding of the
/// integrals and of the 3 × 3 inverse (relative `1e-12`).
#[test]
fn rve_homogeneous_elastic_stiffness_on_mixed_mesh() {
    let mut rve = rve(ExtraLawOptions::default());
    let c = rve.homogenize_stiffness().unwrap();
    let (e, nu) = (3., 0.25);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    // column-major [col][row]
    let want: Matrix<3, 3> = [
        [lambda + 2. * mu, lambda, 0.],
        [lambda, lambda + 2. * mu, 0.],
        [0., 0., mu],
    ];
    let worst = (0..9)
        .map(|k| (c[k / 3][k % 3] - want[k / 3][k % 3]).abs() / (lambda + 2. * mu))
        .fold(0_f64, f64::max);
    println!("homogenised stiffness: worst |ΔC|/(λ + 2μ) = {worst:.1e}");
    for j in 0..3 {
        for i in 0..3 {
            assert!(
                (c[j][i] - want[j][i]).abs() <= 1e-12 * (lambda + 2. * mu),
                "C({i}, {j}) = {} vs {}",
                c[j][i],
                want[j][i]
            );
        }
    }
}
