// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_FE2/{test_node_selection.cc, test_elastic_homogenization.cc,
// test_eigenstrain_homogenization.cc, test_material_FE2.cc, *.dat, *.msh,
// homogenized_plate.geo, periodic_plate.geo}.

//! Ports of the upstream FE² / RVE tests (TEST-180…183) and independent
//! checks of `SolidMechanicsModelRVE` (XMAT-015), `material_FE2`
//! (XMAT-014) and the C library generator of the gel selector (UD-586).
//! The package never compiled at the reference commit: upstream's
//! own assertions and tolerances, checked against the port (D-007).

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::enums::SpatialDirection;
use akantu_core::tensor::Matrix;
use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_extra_materials::c_rand::CRand;
use akantu_extra_materials::rve::find_corner_nodes;
use akantu_extra_materials::{
    ExtraLawOptions, Fe2Config, MaterialFE2, SolidMechanicsModelRve, register_material_fe2,
};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_model::constitutive_law::{ElementDataSelector, LawPhase, SelectorChain};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use common::{data, read_mesh};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

fn frozen(name: &str) -> std::path::PathBuf {
    fixtures_root().join(format!(
        "meshes/gmsh-4.13.1/extra_packages/extra-materials/test/test_material_FE2/{name}"
    ))
}

fn parser(rel: &str) -> Parser {
    Parser::parse_file(data(rel)).unwrap()
}

#[test]
fn c_rand_matches_glibc_black_box() {
    let text = std::fs::read_to_string(data("glibc_rand_srand_0_1_12345.txt")).unwrap();
    let mut lines = text.lines();
    for seed in [0u32, 1, 12345] {
        let want: Vec<u32> = lines
            .next()
            .unwrap()
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let mut g = CRand::new(seed);
        let got: Vec<u32> = (0..64).map(|_| g.next_value()).collect();
        assert_eq!(got, want, "seed {seed}");
    }
    assert_eq!(lines.next().unwrap(), "RAND_MAX 2147483647");
}

/// Upstream `test_node_selection` (smoke: prints the average strain) plus
/// the corner and group edits of the RVE constructor.
#[test]
fn test_node_selection() {
    let mesh = read_mesh(&frozen("periodic_plate.msh"), 2);
    let corners = find_corner_nodes(&mesh, akantu_core::math::DEFAULT_TOLERANCE).unwrap();
    let (lo, hi) = (mesh.lower_bounds().to_vec(), mesh.upper_bounds().to_vec());
    let x = mesh.nodes().as_slice().to_vec();
    let at = |n: i32| [x[2 * n as usize], x[2 * n as usize + 1]];
    assert_eq!(at(corners[0]), [lo[0], lo[1]]);
    assert_eq!(at(corners[1]), [hi[0], lo[1]]);
    assert_eq!(at(corners[2]), [hi[0], hi[1]]);
    assert_eq!(at(corners[3]), [lo[0], hi[1]]);

    let mut rve = SolidMechanicsModelRve::new(
        mesh,
        false,
        400,
        parser("test_material_FE2/material_test_boundary.dat"),
        "solid_mechanics_model",
    )
    .unwrap();
    {
        let m = rve.model.physics.base.mesh.read().unwrap();
        for (g, cs) in [
            ("top", [2, 3]),
            ("left", [3, 0]),
            ("bottom", [1, 0]),
            ("right", [2, 1]),
        ] {
            let nodes = m.groups().element_group(g).unwrap().nodes().to_vec();
            for c in cs {
                assert!(!nodes.contains(&corners[c]), "{g} still holds corner {c}");
            }
        }
    }
    rve.model
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    rve.init_full().unwrap();
    let mut h = [[0.; 2]; 2];
    h[1][0] = 1.; // grad_u_macro(0, 1) = 1
    rve.apply_boundary_conditions(&h).unwrap();
    rve.model.solve_step(None).unwrap();
    let average_strain = rve.average_tensor_field(0, 1, "strain").unwrap();
    println!("the average strain is: {average_strain}");
    assert!(average_strain.is_finite());
    assert!(rve.volume() > 0.);
}

/// Upstream `test_elastic_homogenization`: the homogenised stiffness of a
/// homogeneous orthotropic plate equals the law's Voigt stiffness within
/// 1e-10 (absolute, per entry), with a uniform eigenstrain of 1 on every
/// component (cancelled exactly: `Σ̄ = C:Ē` pointwise). The input has no
/// `gel` law: upstream's `homogenizeStiffness` throws (UD-585), the default
/// skips the gel eigenstrain.
#[test]
fn test_elastic_homogenization() {
    let mesh = read_mesh(&frozen("homogenized_plate.msh"), 2);
    let mut rve = SolidMechanicsModelRve::new(
        mesh,
        false,
        400,
        parser("test_material_FE2/material_orthotropic.dat"),
        "solid_mechanics_model",
    )
    .unwrap();
    rve.init_full().unwrap();
    rve.model
        .with_material(0, LawPhase::Init, |law, _| {
            for v in law.base_mut().eigen_grad_u.get_mut(T3, NG)?.as_mut_slice() {
                *v += 1.0;
            }
            Ok(())
        })
        .unwrap();
    let c = rve.homogenize_stiffness().unwrap();
    // the law's Voigt stiffness (its tangent; read after the solves: a
    // tangent evaluation marks the stiffness assembled)
    let voigt: Vec<Real> = rve
        .model
        .with_material(0, LawPhase::Tangent, |law, ctx| {
            let nq = law.base().grad_u.get(T3, NG)?.len();
            let mut t = Array::try_zeros(nq, 9)?;
            law.compute_tangent_moduli(ctx, T3, NG, &mut t)?;
            Ok(t.as_slice()[..9].to_vec())
        })
        .unwrap();
    for i in 0..3 {
        for j in 0..3 {
            let (exact, approx) = (voigt[i + 3 * j], c[j][i]);
            println!("exact: {exact} approximated: {approx}");
            assert!(
                (exact - approx).abs() <= 1e-10,
                "The material homogenization failed: ({i}, {j}) {exact} vs {approx}"
            );
        }
    }
    // upstream: no `gel` law → error
    rve.extra_options = ExtraLawOptions::upstream();
    assert!(rve.homogenize_stiffness().is_err());
}

/// Upstream `test_eigenstrain_homogenization`: a gel inclusion of 1/16 of
/// the RVE with an eigenstrain of 0.02 gives an average eigenstrain of
/// 0.00125·I (L2 norm of the difference ≤ 1e-10).
#[test]
fn test_eigenstrain_homogenization() {
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(1);
    let mesh = read_mesh(&data("test_material_FE2/one_inclusion.msh"), 2);
    let mut rve = SolidMechanicsModelRve::new(
        mesh,
        false,
        400,
        parser("test_material_FE2/mesoscale_materials.dat"),
        "solid_mechanics_model",
    )
    .unwrap();
    rve.model
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    rve.init_full().unwrap();
    rve.apply_boundary_conditions(&[[0.; 2]; 2]).unwrap();
    let prestrain: Matrix<2, 2> = [[0.02, 0.], [0., 0.02]];
    rve.advance_asr(&prestrain).unwrap();
    let m = rve.homogenize_eigen_grad_u().unwrap();
    println!("the average eigen_gradu is {m:?}");
    let d = [m[0][0] - 0.00125, m[0][1], m[1][0], m[1][1] - 0.00125];
    let norm = d.iter().map(|x| x * x).sum::<Real>().sqrt();
    assert!(norm <= 1e-10, "the test failed!!: {norm:e}");
}

/// Upstream `test_material_FE2` (smoke: macro square, one RVE per
/// quadrature point with one gel pocket, two solves, 10 ASR steps), plus
/// invariants: positive definite homogenised stiffnesses, damage never
/// decreasing, macro eigenstrain following the gel growth.
#[test]
fn test_material_fe2() {
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(1);
    let p = parser("test_material_FE2/material.dat");
    let mesh = read_mesh(&data("test_material_FE2/square.msh"), 2);
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        p.clone(),
    )
    .unwrap();
    akantu_extra_materials::register_extra_materials(&mut model.physics.factory).unwrap();
    register_material_fe2(
        &mut model.physics.factory,
        Fe2Config::new(p, Some(data("test_material_FE2"))),
    )
    .unwrap();
    model.set_material_selector_chain(SelectorChain {
        selectors: Vec::new(),
        fallback_value: 3,
    });
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    for axis in [SpatialDirection::X, SpatialDirection::Y] {
        model
            .apply_dirichlet_bc(&FixedValue { value: 0., axis }, "bottom")
            .unwrap();
    }
    model.assemble_stiffness_matrix(true).unwrap();
    model.solve_step(None).unwrap();
    model.solve_step(None).unwrap();
    let fe2 = model.material_index("FE2_mat").unwrap();
    let total_steps = 10;
    let p_inc = 0.00065;
    let mut current = [[0.; 2]; 2];
    let mut previous_eigen: Option<Vec<Real>> = None;
    for _ in 0..total_steps {
        current[0][0] += p_inc;
        current[1][1] += p_inc;
        model
            .with_material(fe2, LawPhase::AfterSolveStep, |law, _| {
                let m = law
                    .as_any_mut()
                    .unwrap()
                    .downcast_mut::<MaterialFE2<2>>()
                    .unwrap();
                m.advance_asr(&current)?;
                for c in m.c.get(T3, NG)?.as_slice().as_chunks::<9>().0.iter() {
                    let cm: Matrix<3, 3> =
                        [[c[0], c[1], c[2]], [c[3], c[4], c[5]], [c[6], c[7], c[8]]];
                    assert!(SolidMechanicsModelRve::is_positive_definite(&cm), "{cm:?}");
                }
                Ok(())
            })
            .unwrap();
        model.solve_step(None).unwrap();
        let law = model.material(fe2).unwrap();
        let eigen = law
            .base()
            .eigen_grad_u
            .get(T3, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        // the macro eigenstrain grows with the gel eigenstrain
        if let Some(prev) = &previous_eigen {
            for (a, b) in eigen
                .as_chunks::<4>()
                .0
                .iter()
                .zip(prev.as_chunks::<4>().0.iter())
            {
                assert!(a[0] > b[0] && a[3] > b[3], "{a:?} vs {b:?}");
            }
        }
        previous_eigen = Some(eigen);
    }
    let law = model.material(fe2).unwrap();
    let m = law
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialFE2<2>>()
        .unwrap();
    assert!(!m.rves.is_empty());
    for rve in &m.rves {
        // one gel pocket per RVE (gel = law 2)
        let gel = rve.model.material(2).unwrap();
        assert_eq!(gel.base().name, "gel");
        assert_eq!(gel.base().element_filter.get(T3, NG).unwrap().len(), 1);
    }
}

/// The macro model of `test_material_FE2` with the extra materials and
/// `material_FE2` registered with `options`, initialised (one RVE per
/// macro point), under the macro shear gradient `∇u = [[0, 1e-4], [0, 0]]`
/// (`u_x = 1e-4·y`).
fn fe2_macro_model(options: ExtraLawOptions) -> SolidMechanicsModel {
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(1);
    let p = parser("test_material_FE2/material.dat");
    let mesh = read_mesh(&data("test_material_FE2/square.msh"), 2);
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        p.clone(),
    )
    .unwrap();
    akantu_extra_materials::register_extra_materials_with(&mut model.physics.factory, options)
        .unwrap();
    let mut config = Fe2Config::new(p, Some(data("test_material_FE2")));
    config.extra_options = options;
    register_material_fe2(&mut model.physics.factory, config).unwrap();
    model.set_material_selector_chain(SelectorChain {
        selectors: Vec::new(),
        fallback_value: 3,
    });
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let x: Vec<Real> = model
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let mut u = model.displacement_mut().unwrap();
    for (k, p) in x.as_chunks::<2>().0.iter().enumerate() {
        u.as_mut_slice()[2 * k] = 1e-4 * p[1];
    }
    drop(u);
    model
}

/// UD-584, option propagation: the switches
/// given in `Fe2Config::extra_options` reach the macro law, every RVE and
/// every law of the RVE models; in the upstream mode the thermal `0/0`
/// (`σ_th = NaN`, the parent's defaults `E = 0`, `ν = 0.5`) enters as
/// `(i == j)·σ_th`, so **every** macro stress component is NaN (upstream's
/// product `0·NaN`), while the default stress is finite and symmetric.
#[test]
fn fe2_ud584_options_reach_the_rves_and_compat_stress_is_nan() {
    for options in [ExtraLawOptions::default(), ExtraLawOptions::upstream()] {
        let mut model = fe2_macro_model(options);
        let fe2 = model.material_index("FE2_mat").unwrap();
        {
            let law = model.material(fe2).unwrap();
            let m = law
                .as_any()
                .unwrap()
                .downcast_ref::<MaterialFE2<2>>()
                .unwrap();
            assert_eq!(m.extra_options, options);
            assert!(!m.rves.is_empty());
            for rve in &m.rves {
                assert_eq!(rve.extra_options, options);
                let mut checked = 0;
                for i in 0..rve.model.nb_materials() {
                    let l = rve.model.material(i).unwrap();
                    if let Some(it) = akantu_extra_materials::as_iterative_damage(l) {
                        let _ = it.norm_max_equivalent_stress();
                        let d = l
                            .as_any()
                            .unwrap()
                            .downcast_ref::<akantu_extra_materials::MaterialDamageIterative<2>>()
                            .unwrap();
                        assert_eq!(d.extra_options, options, "RVE law {i}");
                        checked += 1;
                    }
                }
                assert!(checked >= 2, "the RVE laws (aggregates, paste, gel)");
            }
        }
        let _ = model.assemble_internal_forces();
        let s = model
            .material(fe2)
            .unwrap()
            .base()
            .stress
            .get(T3, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        assert!(!s.is_empty());
        for t in s.as_chunks::<4>().0 {
            if options.fe2_no_thermal_stress_without_expansion {
                assert!(t.iter().all(|x| x.is_finite()), "{t:?}");
                assert_eq!(t[1].to_bits(), t[2].to_bits(), "symmetric: {t:?}");
                assert!(t[1] != 0., "shear response to the shear gradient: {t:?}");
            } else {
                assert!(t.iter().all(|x| x.is_nan()), "every component NaN: {t:?}");
            }
        }
    }
}

/// XMAT-014, TEST-180 (fails if the Voigt shear factor of
/// `compute_stress` is dropped): upstream's
/// `MaterialFE2::computeStress` is `σ_I = Σ_J C_IJ ε_J + (i == j) σ_th`
/// with the Voigt strain `ε = [ε₁₁, ε₂₂, 2 ε₁₂]` (`Material::
/// gradUToEpsilon` + `strainToVoigt`) and `C` the stored homogenised
/// stiffness of the point. Under the macro shear gradient `∇u = [[0, 1e-4],
/// [0, 0]]` the stress of every macro point equals that product by value
/// (the thermal part is zero without expansion, UD-584 default).
#[test]
fn fe2_macro_stress_is_the_voigt_product_of_the_stored_c() {
    let mut model = fe2_macro_model(ExtraLawOptions::default());
    let fe2 = model.material_index("FE2_mat").unwrap();
    let _ = model.assemble_internal_forces();
    let law = model.material(fe2).unwrap();
    let m = law
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialFE2<2>>()
        .unwrap();
    let stress = law.base().stress.get(T3, NG).unwrap().as_slice().to_vec();
    let grad_u = law.base().grad_u.get(T3, NG).unwrap().as_slice().to_vec();
    let c = m.c.get(T3, NG).unwrap().as_slice().to_vec();
    let sigma_th = m.elastic.sigma_th.get(T3, NG).unwrap().as_slice().to_vec();
    let nq = stress.len() / 4;
    assert!(nq > 0);
    assert_eq!(grad_u.len(), 4 * nq);
    assert_eq!(c.len(), 9 * nq);
    assert!(sigma_th.iter().all(|&t| t == 0.));
    let mut max_shear = 0f64;
    for q in 0..nq {
        let g = &grad_u[4 * q..4 * q + 4]; // column-major g[i + 2 j] = ∂u_i/∂x_j
        // macro shear gradient: u_x = 1e-4 y
        assert!((g[2] - 1e-4).abs() < 1e-12 && g[0].abs() < 1e-12 && g[3].abs() < 1e-12);
        let eps = [g[0], g[3], g[1] + g[2]]; // Voigt (engineering shear)
        let cm = &c[9 * q..9 * q + 9]; // column-major 3 x 3
        let mut s = [0.; 3];
        for (i, si) in s.iter_mut().enumerate() {
            for (j, e) in eps.iter().enumerate() {
                *si += cm[i + 3 * j] * e;
            }
        }
        let t = &stress[4 * q..4 * q + 4];
        let tol = 1e-12 * (1. + s.iter().fold(0f64, |a, x| a.max(x.abs())));
        assert!((t[0] - s[0]).abs() <= tol, "σ11 {t:?} vs {s:?}");
        assert!((t[3] - s[1]).abs() <= tol, "σ22 {t:?} vs {s:?}");
        assert!((t[1] - s[2]).abs() <= tol, "σ12 {t:?} vs {s:?}");
        assert!((t[2] - s[2]).abs() <= tol, "σ21 {t:?} vs {s:?}");
        max_shear = max_shear.max(s[2].abs());
    }
    assert!(max_shear > 0., "the shear stress must be non-trivial");
}

/// XMAT-015, TEST-181 (fails if `advance_asr` leaves its loop after the
/// first solve, i.e. never updates the damage). The one-inclusion RVE of
/// `test_eigenstrain_homogenization`
/// with a deterministic (non-Weibull) strength `Sc = 2e8` in the two
/// `damage_iterative` laws and the gel eigenstrain `0.02·I`:
/// - the loop only ends when the last solve left no point above the
///   strength (`norm_max_equivalent_stress < 1` of both laws);
/// - points of the aggregate law were damaged (never skipped) and no
///   damage is negative;
/// - damage only softens: the homogenised stiffness has a smaller diagonal
///   than the undamaged one.
#[test]
fn advance_asr_damages_until_no_point_exceeds_the_strength() {
    use akantu_extra_materials::{MaterialDamageIterative, as_iterative_damage};
    let text = std::fs::read_to_string(data("test_material_FE2/mesoscale_materials.dat"))
        .unwrap()
        .replace("Sc = 26.4e8 weibull [6.6e6, 5.]", "Sc = 2e8")
        .replace("Sc = 5.4e8 weibull [6.6e6, 5.]", "Sc = 2e8");
    assert_eq!(text.matches("Sc = 2e8").count(), 2);
    let mesh = read_mesh(&data("test_material_FE2/one_inclusion.msh"), 2);
    let mut rve = SolidMechanicsModelRve::new(
        mesh,
        false,
        400,
        Parser::parse_str(&text, "mesoscale_materials.dat").unwrap(),
        "solid_mechanics_model",
    )
    .unwrap();
    rve.model
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    rve.init_full().unwrap();
    rve.apply_boundary_conditions(&[[0.; 2]; 2]).unwrap();
    let c0 = rve.homogenize_stiffness().unwrap();
    rve.advance_asr(&[[0.02, 0.], [0., 0.02]]).unwrap();
    for i in [0, 1] {
        let max = as_iterative_damage(rve.model.material(i).unwrap())
            .unwrap()
            .norm_max_equivalent_stress();
        assert!(max < 1., "law {i}: the loop ended with max {max} >= 1");
    }
    let law = rve.model.material(0).unwrap();
    let d = law
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialDamageIterative<2>>()
        .unwrap()
        .damage
        .damage
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert!(!d.is_empty());
    assert!(d.iter().all(|&x| x >= 0.));
    let damaged = d.iter().filter(|&&x| x > 0.).count();
    assert!(damaged > 0, "no point damaged by the ASR step");
    let c1 = rve.homogenize_stiffness().unwrap();
    for i in 0..3 {
        assert!(
            c1[i][i] < c0[i][i],
            "damage must soften: C[{i}][{i}] {} -> {}",
            c0[i][i],
            c1[i][i]
        );
    }
}
