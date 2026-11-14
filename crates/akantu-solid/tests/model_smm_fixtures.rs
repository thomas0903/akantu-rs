// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential
// tests against fixtures/model/smm/* (oracle/probes/smm/smm_probe.cc, strict
// reference build, Eigen sparse solver): every recorded value bitwise
// (D-008) — patch tests of every element type (static / explicit / implicit,
// elastic plane strain + plane stress, anisotropic), lumped masses, stable
// time steps, eigenstrain, material reassignment, consistent-mass explicit
// and implicit dynamic runs with energies, Dirichlet/Neumann functors.

// Index loops mirror the upstream tensor expressions (C_ijkl ε_kl).
#![allow(clippy::needless_range_loop)]

mod model_common;
mod smm_patch;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{
    FixedValue, FlagOnly, FreeBoundary, FromHigherDim, FromSameDim, Increment, IncrementValue,
};
use akantu_solid::SolidMechanicsModel;
use akantu_validation::Fixture;
use model_common::*;
// explicit: the patch helper (returns whether the solver is Newton–Raphson)
// shadows model_common's `set_newton`, as the local function did
use smm_patch::set_newton;
use smm_patch::*;

#[test]
fn patch_static_elastic_bitwise() {
    let (c, n) = replay_patch_family("patch_static_elastic_");
    assert_eq!(c, 16);
    eprintln!("patch static: {c} cases, {n} values bitwise");
}

#[test]
fn patch_explicit_elastic_bitwise() {
    let (c, n) = replay_patch_family("patch_explicit_elastic_");
    assert_eq!(c, 16);
    eprintln!("patch explicit elastic: {c} cases, {n} values bitwise");
}

#[test]
fn patch_implicit_elastic_bitwise() {
    let (c, n) = replay_patch_family("patch_implicit_elastic_");
    assert_eq!(c, 16);
    eprintln!("patch implicit elastic: {c} cases, {n} values bitwise");
}

#[test]
fn patch_explicit_anisotropic_bitwise() {
    let (c, n) = replay_patch_family("patch_explicit_anisotropic_");
    assert_eq!(c, 12);
    eprintln!("patch explicit anisotropic: {c} cases, {n} values bitwise");
}

/// `TestPatchTestSMMLinear.AnisotropicStatic`
/// (`patch_test_linear_anisotropic_implicit.cc`): static,
/// 2 iterations, residual criterion 1e-10, every element type; upstream
/// checks (DOFs, gradient, stresses against the hand-rotated `C`, 1e-9).
#[test]
fn patch_static_anisotropic_bitwise() {
    let (c, n) = replay_patch_family("patch_static_anisotropic_");
    assert_eq!(c, 12);
    eprintln!("patch static anisotropic: {c} cases, {n} values bitwise");
}

/// Implicit-dynamic anisotropic patch runs: probe-defined scenarios (no
/// upstream counterpart; upstream has `AnisotropicExplicit` and
/// `AnisotropicStatic`).
#[test]
fn patch_implicit_anisotropic_bitwise() {
    let (c, n) = replay_patch_family("patch_implicit_anisotropic_");
    assert_eq!(c, 12);
    eprintln!("patch implicit anisotropic: {c} cases, {n} values bitwise");
}

/// The upstream "finite deformation" patch tests
/// (`ExplicitFiniteDeformation`, `ImplicitFiniteDeformation`) read
/// `material_check_stress_plane_strain_finite_deformation.dat`, which sets
/// `finite_deformation = false`: they are small-strain runs (upstream test
/// defect, reported), replayed bitwise like the others. SMM-023 has no
/// coverage from them.
#[test]
fn patch_finite_deformation_files_are_small_strain_bitwise() {
    let (c1, n1) = replay_patch_family("patch_explicit_fd_");
    let (c2, n2) = replay_patch_family("patch_implicit_fd_");
    assert_eq!(c1 + c2, 8);
    eprintln!(
        "patch 'fd' files: {} cases, {} values bitwise",
        c1 + c2,
        n1 + n2
    );
}

/* ---------------------------------------------------------------------- */

#[test]
fn lumped_masses_and_stable_time_steps_bitwise() {
    let cases = smm_cases("lumped_mass_");
    assert_eq!(cases.len(), 12);
    let mut n = 0;
    for c in &cases {
        let f = smm(c);
        let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim_of(&f));
        let mut model = model_on(&f, mesh, "solid_mechanics_model");
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap();
        model.assemble_mass_lumped().unwrap();
        n += assert_bits(&f, "mass", model.mass().unwrap().as_slice());
        n += assert_bits(&f, "stable_time_step", &[model.stable_time_step().unwrap()]);
    }
    eprintln!("lumped masses: {} cases, {n} values bitwise", cases.len());
}

/// `eigenstrain_tetrahedron_4` (the upstream test, Eigen solver) and
/// `eigenstrain_stiffness_tetrahedron_4` (then `assembleStiffnessMatrix(true)`):
/// `compat` sets
/// `SmmUpstreamCompat::stiffness_grad_u_ignores_eigen_grad_u` (UD-208).
fn replay_eigenstrain(name: &str, compat: bool) -> usize {
    let f = smm(name);
    let mesh = mesh_of(&f);
    let mut model = model_on(&f, mesh, "solid_mechanics_model");
    model
        .physics
        .upstream_compat
        .stiffness_grad_u_ignores_eigen_grad_u = compat;
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    model_common::set_newton(&mut model, 2, 2e-4, "residual");
    let blocked = bools(&f, "blocked_dofs");
    {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let x = mesh.nodes().as_slice().to_vec();
        let nodes: Vec<usize> = mesh
            .groups()
            .element_groups()
            .flat_map(|g| g.nodes().iter().map(|&n| n as usize).collect::<Vec<_>>())
            .collect();
        drop(mesh);
        // upstream loop: displacement(n, i) = α(i, 0) + Σ_j α(i, j+1) X(n, j)
        for &nd in &nodes {
            let mut u_g = model.displacement_mut().unwrap();
            let u = u_g.as_mut_slice();
            set_linear_dof(&mut u[nd * 3..nd * 3 + 3], &x[nd * 3..nd * 3 + 3]);
            drop(u_g);
            model.blocked_dofs_mut().unwrap().as_mut_slice()[nd * 3..nd * 3 + 3].fill(true);
        }
    }
    assert_eq!(model.blocked_dofs().unwrap().as_slice(), &blocked[..]);
    let eig = MatrixX::from_col_major(3, 3, vec![0.1; 9]).unwrap();
    {
        let law = model.material_mut(0).unwrap();
        for t in law
            .base_mut()
            .eigen_grad_u
            .get_mut(ElementType::Tetrahedron4, GhostType::NotGhost)
            .unwrap()
            .as_mut_slice()
            .as_chunks_mut::<9>()
            .0
        {
            t.copy_from_slice(eig.as_slice());
        }
    }
    model.solve_step(None).unwrap();
    let stiffness_after = param(&f, "stiffness_reassembled_after_solve")
        .as_bool()
        .unwrap_or(false);
    if stiffness_after {
        model.assemble_stiffness_matrix(true).unwrap();
    }
    let mut its = Vec::new();
    let mut errs = Vec::new();
    newton_history(&model, &mut its, &mut errs);
    assert_eq!(its, i64s(&f, "newton/nb_iterations"));
    let mut n = assert_bits(&f, "newton/error", &errs);
    n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
    n += assert_bits(
        &f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    n += check_matrix(&f, &model, "K");
    if stiffness_after && !compat {
        // corrected default: the oracle's ∇u minus ε⁰ (one subtraction,
        // bitwise), stresses untouched
        let t = tname(ElementType::Tetrahedron4);
        let want: Vec<Real> = f64s(&f, &format!("grad_u/{t}"))
            .iter()
            .map(|g| g - 0.1)
            .collect();
        let law = model.material(0).unwrap();
        n += assert_bits_vec(
            f.case_id(),
            "grad_u − ε⁰",
            law.base()
                .grad_u
                .get(ElementType::Tetrahedron4, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            &want,
        );
        n += assert_bits(
            &f,
            &format!("stress/{t}"),
            law.base()
                .stress
                .get(ElementType::Tetrahedron4, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
        );
    } else {
        n += check_internals(&f, &model, 0, "");
    }
    n
}

#[test]
fn eigenstrain_bitwise() {
    // the upstream test ends on a residual assembly: unaffected by UD-208,
    // bitwise in the default mode
    let n = replay_eigenstrain("eigenstrain_tetrahedron_4", false);
    eprintln!("eigenstrain: {n} values bitwise");
}

/// UD-208: the stiffness assembly's `grad_u` — upstream
/// `∇u` reproduced bitwise with the compat switch, `∇u − ε⁰` by default.
#[test]
fn eigenstrain_stiffness_grad_u_upstream_compat_and_default() {
    let n = replay_eigenstrain("eigenstrain_stiffness_tetrahedron_4", true);
    let m = replay_eigenstrain("eigenstrain_stiffness_tetrahedron_4", false);
    eprintln!("eigenstrain + stiffness: compat {n}, default {m} values bitwise");
}

/// The reassignment selector of the upstream test (barycentre against a
/// moving plane).
#[derive(Debug)]
struct StraightInterface {
    barycenters: std::collections::HashMap<(ElementType, Idx), Vec<Real>>,
    horizontal: std::sync::atomic::AtomicUsize,
}

impl akantu_model::constitutive_law::ConstitutiveLawSelector for StraightInterface {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        element: Element,
    ) -> akantu_core::Result<Option<usize>> {
        let b = &self.barycenters[&(element.ty(), element.index())];
        let h = self.horizontal.load(std::sync::atomic::Ordering::Relaxed);
        let name = if b[h] < 0.0 { "mat_1" } else { "mat_2" };
        Ok((ctx.law_by_name)(name))
    }
}

#[test]
fn reassign_material_bitwise() {
    let f = smm("reassign_cube_two_materials");
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 3);
    let mut barycenters = std::collections::HashMap::new();
    for ty in types_of(&mesh, 3) {
        for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
            let el = Element::new(ty, e as Idx, GhostType::NotGhost);
            barycenters.insert((ty, e as Idx), mesh.barycenter(el).unwrap());
        }
    }
    let sel = std::sync::Arc::new(StraightInterface {
        barycenters,
        horizontal: std::sync::atomic::AtomicUsize::new(0),
    });
    #[derive(Debug)]
    struct Shared(std::sync::Arc<StraightInterface>);
    impl akantu_model::constitutive_law::ConstitutiveLawSelector for Shared {
        fn select(
            &self,
            ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
            element: Element,
        ) -> akantu_core::Result<Option<usize>> {
            self.0.select(ctx, element)
        }
    }
    let mut model = model_on(&f, mesh, "solid_mechanics_model");
    model.set_material_selector(Box::new(Shared(sel.clone())));
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    let check = |model: &SolidMechanicsModel, prefix: &str| {
        let mesh = model.physics.base.mesh.read().unwrap();
        for ty in types_of(&mesh, 3) {
            let t = tname(ty);
            let idx: Vec<i64> = model
                .material_by_element()
                .get(ty, GhostType::NotGhost)
                .unwrap()
                .as_slice()
                .iter()
                .map(|&x| i64::from(x))
                .collect();
            assert_eq!(
                idx,
                i64s(&f, &format!("{prefix}material_index/{t}")),
                "{prefix} material index"
            );
            let loc: Vec<i64> = model
                .material_local_numbering()
                .get(ty, GhostType::NotGhost)
                .unwrap()
                .as_slice()
                .iter()
                .map(|&x| i64::from(x))
                .collect();
            assert_eq!(
                loc,
                i64s(&f, &format!("{prefix}local_numbering/{t}")),
                "{prefix} local numbering"
            );
            for m in 0..2 {
                let fl: Vec<i64> = model
                    .material(m)
                    .unwrap()
                    .base()
                    .element_filter
                    .get(ty, GhostType::NotGhost)
                    .map(|a| a.as_slice().iter().map(|&x| i64::from(x)).collect())
                    .unwrap_or_default();
                assert_eq!(
                    fl,
                    i64s(&f, &format!("{prefix}filter/mat_{}/{t}", m + 1)),
                    "{prefix} filter {m}"
                );
            }
        }
    };
    check(&model, "before/");
    sel.horizontal
        .store(1, std::sync::atomic::Ordering::Relaxed);
    model.reassign_material().unwrap();
    check(&model, "after/");
    // The oracle records no solve after the reassignment (its compacted
    // internals are corrupted, see oracle/probes/smm/smm_probe.cc); the
    // port's corrected compaction is checked against a fresh assignment in
    // model_ported_tests.rs (reassign_then_solve_equals_fresh_assignment).
    let n: usize = f
        .quantity_names("*")
        .iter()
        .filter(|q| q.starts_with("before/") || q.starts_with("after/"))
        .map(|q| i64s(&f, q).len())
        .sum();
    eprintln!("reassign: {n} values bitwise");
}

/* ---------------------------------------------------------------------- */

const BETA: [[Real; 3]; 3] = [
    [1e-2, -3e-3, 2e-3],
    [4e-3, 5e-3, -1e-3],
    [-2e-3, 1e-3, 6e-3],
];

fn replay_dynamic(name: &str, method: AnalysisMethod) -> usize {
    let f = smm(name);
    let dim = dim_of(&f);
    let ty = element_type(&pstr(&f, "element_type"));
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
    let mut model = model_on(&f, mesh, "solid_mechanics_model");
    model.init_full(&eigen_options(method)).unwrap();
    let x = model
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let nn = x.len() / dim;
    let xmin = x.chunks_exact(dim).map(|p| p[0]).fold(x[0], Real::min);
    for n in 0..nn {
        if (x[n * dim] - xmin).abs() < 1e-10 {
            model.blocked_dofs_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].fill(true);
        }
        let mut v = model.velocity_mut().unwrap();
        let v = v.as_mut_slice();
        for i in 0..dim {
            let mut s = 0.;
            for j in 0..dim {
                s += BETA[i][j] * x[n * dim + j];
            }
            v[n * dim + i] = s;
        }
    }
    assert_eq!(
        model.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs")[..]
    );
    let mut n = assert_bits(&f, "initial_velocity", model.velocity().unwrap().as_slice());
    let newton = set_newton(&mut model, 20, 1e-10, "residual");
    let stable = model.stable_time_step().unwrap();
    let dt = if method == AnalysisMethod::ImplicitDynamic {
        1e-4
    } else {
        0.5 * stable
    };
    n += assert_bits(&f, "time_step", &[stable, dt]);
    model.set_time_step(dt, None).unwrap();
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    let (mut ek, mut ep, mut ew, mut ewn, mut u, mut v, mut a) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let (mut its, mut errs) = (vec![], vec![]);
    let mut snap = |m: &mut SolidMechanicsModel| {
        ek.push(m.energy("kinetic").unwrap());
        ep.push(m.energy("potential").unwrap());
        ew.push(m.energy("external work").unwrap());
        ewn.push(m.energy("external work new").unwrap());
        u.extend_from_slice(m.displacement().unwrap().as_slice());
        v.extend_from_slice(m.velocity().unwrap().as_slice());
        a.extend_from_slice(m.acceleration().unwrap().as_slice());
    };
    snap(&mut model);
    for _ in 0..steps {
        model.solve_step(None).unwrap();
        if newton {
            newton_history(&model, &mut its, &mut errs);
        }
        snap(&mut model);
    }
    n += assert_bits(&f, "displacement", &u);
    n += assert_bits(&f, "velocity", &v);
    n += assert_bits(&f, "acceleration", &a);
    n += assert_bits(&f, "energy/kinetic", &ek);
    n += assert_bits(&f, "energy/potential", &ep);
    n += assert_bits(&f, "energy/external_work", &ew);
    n += assert_bits(&f, "energy/external_work_new", &ewn);
    if newton {
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    let ek0 = model
        .kinetic_energy_of_element(Element::new(ty, 0, GhostType::NotGhost))
        .unwrap();
    n += assert_bits(&f, "energy/kinetic_element_0", &[ek0]);
    n += assert_bits(
        &f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    n += check_internals(&f, &model, 0, "");
    n += check_matrix(&f, &model, "K");
    n += check_matrix(&f, &model, "M");
    if has(&f, "mass") {
        n += assert_bits(&f, "mass", model.mass().unwrap().as_slice());
    }
    n
}

#[test]
fn consistent_mass_explicit_bitwise() {
    let cases = smm_cases("consistent_explicit_");
    assert_eq!(cases.len(), 4);
    let mut n = 0;
    for c in &cases {
        n += replay_dynamic(c, AnalysisMethod::ExplicitConsistentMass);
    }
    eprintln!(
        "consistent explicit: {} cases, {n} values bitwise",
        cases.len()
    );
}

#[test]
fn implicit_dynamic_elastic_bitwise() {
    let cases = smm_cases("implicit_dynamic_");
    assert_eq!(cases.len(), 4);
    let mut n = 0;
    for c in &cases {
        n += replay_dynamic(c, AnalysisMethod::ImplicitDynamic);
    }
    eprintln!(
        "implicit dynamic: {} cases, {n} values bitwise",
        cases.len()
    );
}

/* ---------------------------------------------------------------------- */

#[test]
fn bc_functors_bitwise() {
    let cases = smm_cases("bc_functors_");
    assert_eq!(cases.len(), 3);
    let mut n = 0;
    for c in &cases {
        let f = smm(c);
        let dim = dim_of(&f);
        let mesh = mesh_of(&f);
        let mut model = model_on(&f, mesh, &pstr(&f, "model_id"));
        model
            .init_full(&eigen_options(AnalysisMethod::Static))
            .unwrap();
        let g = "boundary_0";
        let mut rec = |m: &SolidMechanicsModel, stage: &str| {
            n += assert_bits(
                &f,
                &format!("{stage}/displacement"),
                m.displacement().unwrap().as_slice(),
            );
            assert_eq!(
                m.blocked_dofs().unwrap().as_slice(),
                &bools(&f, &format!("{stage}/blocked_dofs"))[..],
                "{c}: {stage} blocked"
            );
            n += assert_bits(
                &f,
                &format!("{stage}/external_force"),
                m.external_force().unwrap().as_slice(),
            );
        };
        model
            .apply_dirichlet_bc(
                &FixedValue {
                    value: 0.01,
                    axis: SpatialDirection::X,
                },
                g,
            )
            .unwrap();
        rec(&model, "fixed_value_x");
        let iv = IncrementValue {
            value: 0.005,
            axis: SpatialDirection::Y,
        };
        model.apply_dirichlet_bc(&iv, g).unwrap();
        model.apply_dirichlet_bc(&iv, g).unwrap();
        rec(&model, "increment_value_y_twice");
        let inc = Increment {
            value: (0..dim).map(|i| 1e-3 * (i + 1) as Real).collect(),
        };
        model.apply_dirichlet_bc(&inc, g).unwrap();
        rec(&model, "increment_vector");
        model.blocked_dofs_mut().unwrap().as_mut_slice().fill(false);
        model
            .apply_dirichlet_bc(
                &FlagOnly {
                    axis: if dim > 1 {
                        SpatialDirection::Y
                    } else {
                        SpatialDirection::X
                    },
                },
                g,
            )
            .unwrap();
        rec(&model, "flag_only");
        let t: Vec<Real> = (0..dim).map(|i| 1e5 * (i + 1) as Real - 2.5e4).collect();
        model.apply_neumann_bc(&FromSameDim::new(t), g).unwrap();
        rec(&model, "from_same_dim");
        let mut sigma = Array::try_zeros(dim * dim, 1).unwrap();
        for j in 0..dim {
            for i in 0..dim {
                sigma.as_mut_slice()[i + j * dim] = 1e6 * (1 + i) as Real + 3e5 * j as Real;
            }
        }
        let functor = FromHigherDim::new(
            MatrixX::from_col_major(dim, dim, sigma.as_slice().to_vec()).unwrap(),
        );
        model.apply_neumann_bc(&functor, g).unwrap();
        rec(&model, "from_higher_dim");
        model.apply_neumann_bc(&FreeBoundary, g).unwrap();
        rec(&model, "free_boundary");
    }
    eprintln!("bc functors: {} cases, {n} values bitwise", cases.len());
}

/* ---------------------------------------------------------------------- */
/* adversarial cases and the Eigen-pinned loaded twins                      */
/* ---------------------------------------------------------------------- */

fn review_model(f: &Fixture, method: AnalysisMethod) -> SolidMechanicsModel {
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 3);
    let mut m = model_on(f, mesh, "solid_mechanics_model");
    m.set_material_selector(Box::new(
        akantu_model::constitutive_law::ElementDataSelector::new("physical_names"),
    ));
    m.init_full(&eigen_options(method)).unwrap();
    m
}

/// The material map, law-local numbering and filters recorded by the
/// oracle **for every element type present in the map**: upstream holds
/// only the volume type (`_hexahedron_8`), not the boundary `_quadrangle_4`
/// of the main mesh.
fn check_index_all(f: &Fixture, m: &SolidMechanicsModel, prefix: &str) -> usize {
    let want_types: Vec<String> = param(f, &format!("{prefix}index_map_types"))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        want_types,
        vec!["_hexahedron_8".to_owned()],
        "{}",
        f.case_id()
    );
    let have_types: Vec<String> = m
        .material_by_element()
        .as_map()
        .iter(GhostType::NotGhost)
        .map(|(ty, _)| tname(ty))
        .collect();
    assert_eq!(
        have_types,
        want_types,
        "{}: material map types",
        f.case_id()
    );
    let mut n = 0;
    for (ty, a) in m.material_by_element().as_map().iter(GhostType::NotGhost) {
        let t = tname(ty);
        let got: Vec<i64> = a
            .borrow()
            .as_slice()
            .iter()
            .map(|&x| i64::from(x))
            .collect();
        assert_eq!(got, i64s(f, &format!("{prefix}material_index/{t}")));
        let loc: Vec<i64> = m
            .material_local_numbering()
            .get(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .iter()
            .map(|&x| i64::from(x))
            .collect();
        assert_eq!(loc, i64s(f, &format!("{prefix}local_numbering/{t}")));
        n += got.len() + loc.len();
    }
    for i in 0..m.nb_materials() {
        for (ty, a) in m
            .material(i)
            .unwrap()
            .base()
            .element_filter
            .as_map()
            .iter(GhostType::NotGhost)
        {
            let key = format!("{prefix}filter/{i}/{}", tname(ty));
            assert!(
                has(f, &key),
                "{}: port-only filter block {key}",
                f.case_id()
            );
            let got: Vec<i64> = a.as_slice().iter().map(|&x| i64::from(x)).collect();
            assert_eq!(got, i64s(f, &key), "{key}");
            n += got.len();
        }
    }
    n
}

#[test]
fn review_mixed_static_bitwise() {
    let f = smm("review_mixed_static");
    let mut m = review_model(&f, AnalysisMethod::Static);
    model_common::set_newton(&mut m, 5, 1e-10, "solution");
    for ax in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ] {
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: ax,
            },
            "left",
        )
        .unwrap();
    }
    let mut sigma = vec![0.0; 9];
    for j in 0..3 {
        for i in 0..3 {
            sigma[i + 3 * j] =
                1e6 * (1 + i) as Real + 3e5 * j as Real - 2e5 * if i == j { 1.0 } else { 0.0 };
        }
    }
    m.apply_neumann_bc(
        &FromHigherDim::new(MatrixX::from_col_major(3, 3, sigma).unwrap()),
        "right",
    )
    .unwrap();
    m.apply_neumann_bc(&FromSameDim::new(vec![1e5, -2e5, -3e5]), "top")
        .unwrap();
    let mut n = assert_bits(
        &f,
        "external_force_before",
        m.external_force().unwrap().as_slice(),
    );
    assert_eq!(
        m.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs")[..]
    );
    n += check_index_all(&f, &m, "before/");
    m.solve_step(None).unwrap();
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    newton_history(&m, &mut its, &mut errs);
    assert_eq!(its, i64s(&f, "newton/nb_iterations"));
    n += assert_bits(&f, "newton/error", &errs);
    n += assert_bits(&f, "displacement", m.displacement().unwrap().as_slice());
    n += assert_bits(&f, "internal_force", m.internal_force().unwrap().as_slice());
    n += check_internals(&f, &m, 0, "mat0/");
    n += check_internals(&f, &m, 1, "mat1/");
    n += check_matrix(&f, &m, "K");
    let ep = m.energy("potential").unwrap();
    n += assert_bits(&f, "energy/potential", &[ep]);
    n += assert_bits(
        &f,
        "energy/external_work",
        &[m.energy("external work").unwrap()],
    );
    // UD-032: upstream's per-group potential energy integrates garbage (not
    // recorded); the port's group energies add up to the total
    let steel = m.energy_of_group("potential", "steel").unwrap();
    let alu = m.energy_of_group("potential", "alu").unwrap();
    assert!(
        ((steel + alu) - ep).abs() <= 1e-12 * ep.abs(),
        "{steel} + {alu} vs {ep}"
    );
    // boundary groups have no material (upstream: "No element of type")
    assert!(m.energy_of_group("potential", "top").is_err());
    eprintln!("review_mixed_static: {n} values bitwise");
}

#[test]
fn review_consistent_blocked_change_bitwise() {
    let f = smm("review_consistent_blocked_change");
    let mut m = review_model(&f, AnalysisMethod::ExplicitConsistentMass);
    model_common::set_newton(&mut m, 20, 1e-10, "residual");
    let x = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let nn = x.len() / 3;
    let xmin = x
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| p[0])
        .fold(x[0], Real::min);
    for n in 0..nn {
        if (x[n * 3] - xmin).abs() < 1e-10 {
            m.blocked_dofs_mut().unwrap().as_mut_slice()[n * 3..n * 3 + 3].fill(true);
        }
        let mut v = m.velocity_mut().unwrap();
        let v = v.as_mut_slice();
        for i in 0..3 {
            let mut s = 0.;
            for j in 0..3 {
                s += BETA[i][j] * x[n * 3 + j];
            }
            v[n * 3 + i] = s;
        }
    }
    let stable = m.stable_time_step().unwrap();
    let mut count = assert_bits(&f, "time_step", &[stable, 0.5 * stable]);
    m.set_time_step(0.5 * stable, None).unwrap();
    let (mut ek, mut ep, mut ew, mut ewn) = (vec![], vec![], vec![], vec![]);
    let (mut u, mut v, mut a, mut its, mut errs) = (vec![], vec![], vec![], vec![], vec![]);
    let mut snap = |m: &mut SolidMechanicsModel| {
        ek.push(m.energy("kinetic").unwrap());
        ep.push(m.energy("potential").unwrap());
        ew.push(m.energy("external work").unwrap());
        ewn.push(m.energy("external work new").unwrap());
        u.extend_from_slice(m.displacement().unwrap().as_slice());
        v.extend_from_slice(m.velocity().unwrap().as_slice());
        a.extend_from_slice(m.acceleration().unwrap().as_slice());
    };
    snap(&mut m);
    for s in 0..16 {
        if s == 8 {
            for n in 0..nn {
                if x[n * 3].abs() < 1e-10 {
                    m.blocked_dofs_mut().unwrap().as_mut_slice()[n * 3 + 2] = false;
                }
                if (x[n * 3] - 3.).abs() < 1e-10 {
                    m.blocked_dofs_mut().unwrap().as_mut_slice()[n * 3 + 1] = true;
                }
            }
            m.external_force_mut().unwrap().as_mut_slice()[5 * 3] = 1e7;
            m.external_force_mut().unwrap().as_mut_slice()[30 * 3 + 2] = -2e7;
        }
        m.solve_step(None).unwrap();
        newton_history(&m, &mut its, &mut errs);
        snap(&mut m);
    }
    count += assert_bits(&f, "displacement", &u);
    count += assert_bits(&f, "velocity", &v);
    count += assert_bits(&f, "acceleration", &a);
    count += assert_bits(&f, "energy/kinetic", &ek);
    count += assert_bits(&f, "energy/potential", &ep);
    count += assert_bits(&f, "energy/external_work", &ew);
    count += assert_bits(&f, "energy/external_work_new", &ewn);
    assert_eq!(its, i64s(&f, "newton/nb_iterations"));
    count += assert_bits(&f, "newton/error", &errs);
    assert_eq!(
        m.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs_final")[..]
    );
    count += assert_bits(&f, "internal_force", m.internal_force().unwrap().as_slice());
    count += check_matrix(&f, &m, "M");
    count += check_matrix(&f, &m, "K");
    count += check_index_all(&f, &m, "");
    eprintln!("review_consistent_blocked_change: {count} values bitwise");
}

/// Eigen-pinned twins of `fixtures/model/static/loaded_*` (MUMPS there):
/// the whole loaded static solve bitwise.
#[test]
fn loaded_static_eigen_bitwise() {
    let cases = smm_cases("loaded_static_");
    assert_eq!(cases.len(), 2, "{cases:?}");
    let mut total = 0;
    for c in &cases {
        let f = smm(c);
        let dim = dim_of(&f);
        let ty = element_type(&pstr(&f, "element_type"));
        let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
        let mut m = model_on(&f, mesh, "solid_mechanics_model");
        m.init_full(&eigen_options(AnalysisMethod::Static)).unwrap();
        let x = m
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        let amax = x.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        let tol = 1e-10 * amax.max(1.0);
        let xmin = x.chunks_exact(dim).map(|p| p[0]).fold(x[0], Real::min);
        let xmax = x.chunks_exact(dim).map(|p| p[0]).fold(x[0], Real::max);
        let load = f64s(&f, "nodal_load");
        for (n, p) in x.chunks_exact(dim).enumerate() {
            if (p[0] - xmin).abs() < tol {
                m.blocked_dofs_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].fill(true);
            }
            if (p[0] - xmax).abs() < tol {
                m.external_force_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim]
                    .copy_from_slice(&load);
            }
        }
        assert_eq!(
            m.blocked_dofs().unwrap().as_slice(),
            &bools(&f, "blocked_dofs")[..]
        );
        let mut n = assert_bits(&f, "external_force", m.external_force().unwrap().as_slice());
        model_common::set_newton(&mut m, 10, 1e-12, "residual");
        m.solve_step(None).unwrap();
        let (mut its, mut errs) = (Vec::new(), Vec::new());
        newton_history(&m, &mut its, &mut errs);
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{c}");
        n += assert_bits(&f, "newton/error", &errs);
        n += assert_bits(&f, "displacement", m.displacement().unwrap().as_slice());
        n += assert_bits(&f, "internal_force", m.internal_force().unwrap().as_slice());
        n += check_internals(&f, &m, 0, "");
        n += check_matrix(&f, &m, "K");
        let _ = ty;
        total += n;
    }
    eprintln!(
        "loaded static (Eigen): {} cases, {total} values bitwise",
        cases.len()
    );
}

/* ---------------------------------------------------------------------- */
/* smm probe v2, x86_64 oracle                                             */
/* ---------------------------------------------------------------------- */

/// The probe's `block_xmin` + `linear_velocity`.
fn block_xmin_linear_velocity(model: &mut SolidMechanicsModel, dim: usize) {
    let x = model
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let xmin = x.chunks_exact(dim).map(|p| p[0]).fold(x[0], Real::min);
    for n in 0..x.len() / dim {
        if (x[n * dim] - xmin).abs() < 1e-10 {
            model.blocked_dofs_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].fill(true);
        }
        let mut v_g = model.velocity_mut().unwrap();
        let v = v_g.as_mut_slice();
        for i in 0..dim {
            let mut s = 0.;
            for j in 0..dim {
                s += BETA[i][j] * x[n * dim + j];
            }
            v[n * dim + i] = s;
        }
    }
}

/// `model/smm/explicit_lumped_kinetic_*`: an
/// explicit lumped model has the sparse `K` and `M` of its scheme from
/// `init_full` on (upstream `TimeStepSolver::setIntegrationScheme` creates
/// every needed matrix whose type the model defines), no lumped `M` until
/// the first step, so `getKineticEnergy` takes the consistent-mass branch
/// before the first step and the lumped branch after it. The matrix
/// flags, both energies, the consistent `M` assembled by the first call,
/// the state and the lumped mass are replayed bit for bit.
#[test]
fn explicit_lumped_kinetic_before_first_step_bitwise() {
    let cases = smm_cases("explicit_lumped_kinetic_");
    assert_eq!(cases.len(), 2);
    let mut n = 0;
    for c in &cases {
        let f = smm(c);
        let dim = dim_of(&f);
        let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
        let mut model = model_on(&f, mesh, "solid_mechanics_model");
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap();
        block_xmin_linear_velocity(&mut model, dim);
        let flags = |m: &SolidMechanicsModel, stage: &str| {
            let dm = m.dof_manager();
            for id in ["K", "M", "C", "J"] {
                assert_eq!(
                    dm.has_matrix(id),
                    param(&f, &format!("{stage}/has_matrix_{id}"))
                        .as_bool()
                        .unwrap(),
                    "{c}: {stage}: matrix {id}"
                );
            }
            assert_eq!(
                dm.has_lumped_matrix("M"),
                param(&f, &format!("{stage}/has_lumped_matrix_M"))
                    .as_bool()
                    .unwrap(),
                "{c}: {stage}: lumped M"
            );
        };
        flags(&model, "after_init_full");
        let mut ek = vec![model.energy("kinetic").unwrap()];
        flags(&model, "after_kinetic_0");
        n += check_matrix_as(&f, &model, "M_after_kinetic_0", "M");
        let stable = model.stable_time_step().unwrap();
        let dt = 0.5 * stable;
        n += assert_bits(&f, "time_step", &[stable, dt]);
        model.set_time_step(dt, None).unwrap();
        model.solve_step(None).unwrap();
        flags(&model, "after_step_1");
        ek.push(model.energy("kinetic").unwrap());
        n += assert_bits(&f, "energy/kinetic", &ek);
        n += assert_bits(&f, "velocity", model.velocity().unwrap().as_slice());
        n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
        n += assert_bits(&f, "mass", model.mass().unwrap().as_slice());
    }
    println!("explicit_lumped_kinetic: {n} values bit-identical");
}

/// BC-001, BC-004, BC-005; `model/smm/periodic_static_*`:
/// `TestPeriodicStatic` (tensile) on the frozen `line`/`square`/`cube.msh`
/// made periodic — the periodic pairs, the state after every Dirichlet /
/// Neumann functor (1-D: the nodal load made periodic-consistent, and the
/// refusal of a Neumann functor, upstream `AKANTU_TO_IMPLEMENT`), the
/// static solve (Eigen) with its Newton history, internals and `K`, bit for
/// bit.
#[test]
fn periodic_bc_functors_bitwise() {
    use akantu_core::enums::ReduceOperation;
    use akantu_mesh::periodic::{make_consistent_for_periodicity_real, make_periodic};
    use akantu_model::boundary_condition::FromTraction;
    let cases = smm_cases("periodic_static_");
    assert_eq!(cases.len(), 4);
    let dirs = [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ];
    let mut n = 0;
    for c in &cases {
        let f = smm(c);
        let dim = dim_of(&f);
        let dp = param(&f, "periodic_directions").as_u64().unwrap() as usize;
        let mut mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
        for &d in &dirs[..dp] {
            let _ = make_periodic(&mut mesh, d, None).unwrap();
        }
        assert_eq!(
            mesh.is_periodic(),
            param(&f, "is_periodic").as_bool().unwrap()
        );
        let mut slaves = Vec::new();
        let mut masters = Vec::new();
        for node in 0..mesh.nb_nodes() as Idx {
            if mesh.is_periodic_slave(node).unwrap() {
                slaves.push(i64::from(node));
                masters.push(i64::from(mesh.periodic_master(node).unwrap()));
            }
        }
        assert_eq!(slaves, i64s(&f, "periodic/slaves"), "{c}: periodic slaves");
        assert_eq!(
            masters,
            i64s(&f, "periodic/masters"),
            "{c}: periodic masters"
        );
        let mut model = model_on(&f, mesh, "solid_mechanics_model");
        model
            .init_full(&eigen_options(AnalysisMethod::Static))
            .unwrap();
        let mut rec = |m: &SolidMechanicsModel, stage: &str| {
            n += assert_bits(
                &f,
                &format!("{stage}/displacement"),
                m.displacement().unwrap().as_slice(),
            );
            assert_eq!(
                m.blocked_dofs().unwrap().as_slice(),
                &bools(&f, &format!("{stage}/blocked_dofs"))[..],
                "{c}: {stage} blocked"
            );
            n += assert_bits(
                &f,
                &format!("{stage}/external_force"),
                m.external_force().unwrap().as_slice(),
            );
        };
        for &d in &dirs[..dim] {
            model
                .apply_dirichlet_bc(
                    &FixedValue {
                        value: 0.0,
                        axis: d,
                    },
                    "corner",
                )
                .unwrap();
        }
        rec(&model, "corner_fixed");
        let mut traction = vec![0.0; dim];
        traction[dim - 1] = 1e9;
        match dim {
            1 => {
                let mesh_h = model.physics.base.mesh.clone();
                let nodes = mesh_h
                    .read()
                    .unwrap()
                    .groups()
                    .element_group("x-plus")
                    .unwrap()
                    .nodes()
                    .to_vec();
                for node in nodes {
                    model.external_force_mut().unwrap().as_mut_slice()[node as usize] = traction[0];
                }
                rec(&model, "nodal_load");
                {
                    let mesh = mesh_h.read().unwrap();
                    // the helper takes a plain array: through a copy
                    let mut fe_g = model.external_force_mut().unwrap();
                    let mut fe = akantu_core::Array::clone(&fe_g);
                    make_consistent_for_periodicity_real(&mesh, &mut fe, ReduceOperation::Sum)
                        .unwrap();
                    fe_g.copy_from(&fe).unwrap();
                }
                rec(&model, "nodal_load_consistent");
                // upstream: AKANTU_TO_IMPLEMENT (Neumann functors need dim > 1)
                assert!(
                    !param(&f, "neumann_1d_error").as_str().unwrap().is_empty(),
                    "{c}: upstream accepted a 1-D Neumann functor"
                );
                let r = model.apply_neumann_bc(&FromTraction::new(traction.clone()), "x-plus");
                assert!(r.is_err(), "{c}: 1-D Neumann functor accepted");
                rec(&model, "after_neumann_1d");
            }
            2 => {
                model
                    .apply_dirichlet_bc(
                        &FixedValue {
                            value: 0.0,
                            axis: SpatialDirection::Y,
                        },
                        "y-minus",
                    )
                    .unwrap();
                rec(&model, "y_minus_fixed");
                model
                    .apply_neumann_bc(&FromTraction::new(traction.clone()), "y-plus")
                    .unwrap();
                rec(&model, "traction");
            }
            _ => {
                model
                    .apply_dirichlet_bc(
                        &FixedValue {
                            value: 0.0,
                            axis: SpatialDirection::Y,
                        },
                        "line",
                    )
                    .unwrap();
                rec(&model, "line_fixed");
                model
                    .apply_dirichlet_bc(
                        &FixedValue {
                            value: 0.0,
                            axis: SpatialDirection::Z,
                        },
                        "z-minus",
                    )
                    .unwrap();
                rec(&model, "z_minus_fixed");
                model
                    .apply_neumann_bc(&FromTraction::new(traction.clone()), "z-plus")
                    .unwrap();
                rec(&model, "traction");
            }
        }
        model_common::set_newton(&mut model, 5, 2e-4, "solution");
        let solved = model.solve_step(None);
        assert_eq!(
            solved.is_err(),
            !param(&f, "solve_error").as_str().unwrap().is_empty(),
            "{c}: solve outcome {solved:?}"
        );
        let nls = model.solver.non_linear_solver(None).unwrap();
        assert_eq!(
            nls.nb_iterations() as i64,
            param(&f, "newton/nb_iterations").as_i64().unwrap(),
            "{c}: iterations"
        );
        assert_eq!(
            nls.converged(),
            param(&f, "newton/converged").as_bool().unwrap(),
            "{c}: converged"
        );
        let error = nls.error();
        rec(&model, "solved");
        n += assert_bits(&f, "newton/error", &[error]);
        n += assert_bits(
            &f,
            "solved/internal_force",
            model.internal_force().unwrap().as_slice(),
        );
        n += check_internals(&f, &model, 0, "");
        n += check_matrix(&f, &model, "K");
    }
    println!("periodic_static: {n} values bit-identical");
}
