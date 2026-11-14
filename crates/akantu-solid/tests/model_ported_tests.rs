// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (same intent,
// upstream tolerances) of test/test_model/test_solid_mechanics_model/
// {test_solid_mechanics_model_dynamics.cc, test_material_selector.cc,
// test_energies/test_solid_mechanics_model_kinetic_energy.cc,
// test_energies/test_solid_mechanics_model_linear_elastic_potential_energy.cc},
// test/test_model/patch_tests/test_lumped_mass.cc, plus independent
// mathematical checks of the solid mechanics model (rigid-body motion,
// energy balance, analytical stable time step, mass totals, reassignment).
// Test data: upstream meshes copied under tests/data, fixture inputs and the
// frozen gmsh meshes of fixtures/meshes.

mod model_common;

use akantu_validation::reduce::NanMinMax;
use std::path::PathBuf;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::*;

const TYPES: [&str; 12] = [
    "segment_2",
    "segment_3",
    "triangle_3",
    "triangle_6",
    "quadrangle_4",
    "quadrangle_8",
    "tetrahedron_4",
    "tetrahedron_10",
    "hexahedron_8",
    "hexahedron_20",
    "pentahedron_6",
    "pentahedron_15",
];

fn ty(name: &str) -> ElementType {
    format!("_{name}").parse().unwrap()
}

/// Patch-test mesh `_<type>.msh` (unit square / cube), from the inputs of
/// the oracle fixtures.
fn patch_mesh(name: &str) -> Mesh {
    let f = fixtures_root()
        .join("model/smm")
        .join(format!("lumped_mass_{name}"))
        .join(format!("inputs/_{name}.msh"));
    read_mesh(&f, ty(name).spatial_dimension())
}

/// Frozen `bar_<type>.msh` (groups `BC`, `bulk` from physical names).
fn bar_mesh(name: &str) -> Mesh {
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join(format!("bar_{name}.msh"));
    read_mesh(&f, ty(name).spatial_dimension())
}

const UNIT: &str = "material elastic [\n  name = uniterial\n  rho = 1\n  E = 1\n  nu = 0.0\n]\n";

fn model(mesh: Mesh, dat: &str, method: AnalysisMethod) -> SolidMechanicsModel {
    let id = "solid_mechanics_model";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m = SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, id, parser).unwrap();
    m.init_full(&ModelOptions::new(method)).unwrap();
    m
}

fn bounds(x: &[Real], dim: usize) -> (Vec<Real>, Vec<Real>) {
    let mut lo = vec![Real::MAX; dim];
    let mut hi = vec![Real::MIN; dim];
    for p in x.chunks_exact(dim) {
        for d in 0..dim {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    (lo, hi)
}

/* ---------------------------------------------------------------------- */
/* test_solid_mechanics_model_dynamics.cc                                  */
/* ---------------------------------------------------------------------- */

/// Upstream `Verification<dim>` (A = 0.1, E = 1, ν = 0, ρ = 1, k = (½, 0, 0)).
fn verification(dim: usize, x: &[Real], t: Real) -> (Vec<Real>, Vec<Real>) {
    let a = 1e-1;
    let (e, nu, rho) = (1.0_f64, 0.0_f64, 1.0_f64);
    let lambda = e * nu / (1. + nu) / (1. - 2. * nu);
    let mu = e / 2. / (1. + nu);
    let cp = ((lambda + 2. * mu) / rho).sqrt();
    let cs = (mu / rho).sqrt();
    let c = (e / rho).sqrt();
    let k = [0.5, 0., 0.];
    let knorm = 0.5_f64;
    match dim {
        1 => {
            let omega = c * k[0];
            (
                vec![a * (k[0] * x[0] - omega * t).cos()],
                vec![omega * a * (k[0] * x[0] - omega * t).sin()],
            )
        }
        2 => {
            let kpush = [k[0], k[1]];
            let kshear = [k[1], k[0]];
            let (op, os) = (knorm * cp, knorm * cs);
            let ph = x[0] * kpush[0] + x[1] * kpush[1];
            let (pp, ps) = (ph - op * t, ph - os * t);
            (
                (0..2)
                    .map(|i| a * (kpush[i] * pp.cos() + kshear[i] * ps.cos()))
                    .collect(),
                (0..2)
                    .map(|i| a * (kpush[i] * op * pp.cos() + kshear[i] * os * ps.cos()))
                    .collect(),
            )
        }
        _ => {
            // kshear1 = k × ψ1 = (0, −½, 0), kshear2 = k × ψ2 = (0, 0, ½)
            let ks1 = [0., -0.5, 0.];
            let ks2 = [0., 0., 0.5];
            let (op, os) = (knorm * cp, knorm * cs);
            let ph = x[0] * k[0] + x[1] * k[1] + x[2] * k[2];
            let (pp, ps) = (ph - op * t, ph - os * t);
            (
                (0..3)
                    .map(|i| a * (k[i] * pp.cos() + ks1[i] * ps.cos() + ks2[i] * ps.cos()))
                    .collect(),
                (0..3)
                    .map(|i| {
                        a * (k[i] * op * pp.cos() + ks1[i] * os * ps.cos() + ks2[i] * os * ps.cos())
                    })
                    .collect(),
            )
        }
    }
}

/// Upstream `SolutionFunctor` on the group `BC`: x blocked, displacement
/// and velocity set to the analytical solution.
fn apply_solution(m: &mut SolidMechanicsModel, t: Real) {
    let dim = m.spatial_dimension();
    let (nodes, x) = {
        let mesh = m.physics.base.mesh.read().unwrap();
        (
            mesh.groups()
                .element_group("BC")
                .unwrap()
                .nodes()
                .as_slice()
                .to_vec(),
            mesh.nodes().as_slice().to_vec(),
        )
    };
    for n in nodes {
        let n = n as usize;
        let (u, v) = verification(dim, &x[n * dim..(n + 1) * dim], t);
        m.blocked_dofs_mut().unwrap().as_mut_slice()[n * dim] = true;
        m.displacement_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].copy_from_slice(&u);
        m.velocity_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].copy_from_slice(&v);
    }
}

fn dynamics_max_error(name: &str, method: AnalysisMethod) -> Real {
    let mesh = bar_mesh(name);
    let dim = mesh.spatial_dimension();
    let nn = mesh.nb_nodes();
    let x = mesh.nodes().as_slice().to_vec();
    let mut m = model(mesh, UNIT, method);
    let dt = m.stable_time_step().unwrap() / 10.;
    m.set_time_step(dt, None).unwrap();
    for n in 0..nn {
        let (u, v) = verification(dim, &x[n * dim..(n + 1) * dim], 0.);
        m.displacement_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].copy_from_slice(&u);
        m.velocity_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].copy_from_slice(&v);
    }
    apply_solution(&mut m, 0.);
    let max_steps = (5. / dt) as usize;
    let mut t = 0.;
    let mut max_error = -1.0_f64;
    for _ in 0..max_steps {
        apply_solution(&mut m, t);
        let u_g = m.displacement().unwrap();
        let u = u_g.as_slice();
        let mut err = 0.;
        for n in 0..nn {
            let (s, _) = verification(dim, &x[n * dim..(n + 1) * dim], t);
            for d in 0..dim {
                let diff = s[d] - u[n * dim + d];
                err += diff * diff;
            }
        }
        max_error = max_error.nan_max(err.sqrt() / nn as Real);
        drop(u_g);
        m.solve_step(None).unwrap();
        t += dt;
    }
    max_error
}

/// `TestSMMFixtureBarExplicit.Dynamics` (every element type, tol 2e-3).
#[test]
fn test_solid_mechanics_model_dynamics_explicit() {
    for name in TYPES {
        let e = dynamics_max_error(name, AnalysisMethod::ExplicitLumpedMass);
        assert!(e.abs() < 2e-3, "{name}: max error {e}");
    }
}

/// `TestSMMFixtureBarImplicit.Dynamics` (tol 2e-3), 1-D and 2-D types (fast
/// tier); the 3-D types run in
/// [`test_solid_mechanics_model_dynamics_implicit_3d`].
#[test]
fn test_solid_mechanics_model_dynamics_implicit() {
    for name in &TYPES[..6] {
        let e = dynamics_max_error(name, AnalysisMethod::ImplicitDynamic);
        assert!(e.abs() < 2e-3, "{name}: max error {e}");
    }
}

/// `TestSMMFixtureBarImplicit.Dynamics`, the six 3-D types.
/// Medium tier: compiled in optimised builds only —
/// `cargo test --release -p akantu-solid --test model_ported_tests`
/// (docs/TESTING.md) — because the unoptimised sparse LU of a debug test
/// build needs minutes per 3-D bar.
#[cfg(not(debug_assertions))]
#[test]
fn test_solid_mechanics_model_dynamics_implicit_3d() {
    assert_eq!(TYPES.len(), 12);
    for name in &TYPES[6..] {
        let e = dynamics_max_error(name, AnalysisMethod::ImplicitDynamic);
        assert!(e.abs() < 2e-3, "{name}: max error {e}");
    }
}

/* ---------------------------------------------------------------------- */
/* test_energies                                                           */
/* ---------------------------------------------------------------------- */

/// `KineticEnergyExplicit` / `KineticEnergyImplicit`: unit domain, ρ = 1,
/// v_x = 1 → E_kin = ½ during 5 steps (tol 1e-8).
#[test]
fn test_solid_mechanics_model_kinetic_energy() {
    for method in [
        AnalysisMethod::ExplicitLumpedMass,
        AnalysisMethod::ImplicitDynamic,
    ] {
        for name in TYPES {
            let mesh = patch_mesh(name);
            let dim = mesh.spatial_dimension();
            let mut m = model(mesh, UNIT, method);
            m.assemble_mass_lumped().unwrap();
            for v in m
                .velocity_mut()
                .unwrap()
                .as_mut_slice()
                .chunks_exact_mut(dim)
            {
                v[0] = 1.;
            }
            let dt = m.stable_time_step().unwrap() * 0.8;
            m.set_time_step(dt, None).unwrap();
            for _ in 0..5 {
                m.solve_step(None).unwrap();
                let ek = m.energy("kinetic").unwrap();
                assert!((ek - 0.5).abs() < 1e-8, "{method} {name}: E_kin = {ek}");
            }
        }
    }
}

/// `LinearElasticPotentialEnergy`: uniaxial strains ε ∈ {0, .1, .2, .3}
/// imposed along x, E = 1 on the unit domain → E_pot = ½ ε² (tol 1e-8).
#[test]
fn test_solid_mechanics_model_linear_elastic_potential_energy() {
    for name in TYPES {
        let mesh = patch_mesh(name);
        let dim = mesh.spatial_dimension();
        let x = mesh.nodes().as_slice().to_vec();
        let (lo, hi) = bounds(&x, dim);
        let length = hi[0] - lo[0];
        let mut m = model(mesh, UNIT, AnalysisMethod::Static);
        for eps in [0.0, 0.1, 0.2, 0.3] {
            for (n, p) in x.chunks_exact(dim).enumerate() {
                let reduced = (p[0] - lo[0]) / length;
                m.displacement_mut().unwrap().as_mut_slice()[n * dim] = reduced * eps;
                let mut b = m.blocked_dofs_mut().unwrap();
                let b = b.as_mut_slice();
                b[n * dim] = true;
                if p[0] < lo[0] + 1e-6 && dim > 1 && p[1] < lo[1] + 1e-6 {
                    b[n * dim + 1] = true;
                    if dim > 2 && p[2] < lo[2] + 1e-6 {
                        b[n * dim + 2] = true;
                    }
                }
            }
            m.solve_step(None).unwrap();
            let ep = m.energy("potential").unwrap();
            assert!(
                (ep - 0.5 * eps * eps).abs() < 1e-8,
                "{name} ε = {eps}: E_pot = {ep}"
            );
        }
    }
}

/* ---------------------------------------------------------------------- */
/* test_material_selector.cc, test_lumped_mass.cc                          */
/* ---------------------------------------------------------------------- */

/// `test_material_selector`: `MeshDataMaterialSelector<std::string>
/// ("physical_names")` puts segment 0 in `chocolate`, 1 in `chewing-gum`,
/// 2 in `candy`.
#[test]
fn test_material_selector() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/test_solid_mechanics_model");
    let mesh = read_mesh(&dir.join("material_selector.msh"), 1);
    let parser = Parser::parse_file(dir.join("material_selector.dat")).unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    m.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    m.init_full(&ModelOptions::default()).unwrap();
    for (name, el) in [("chocolate", 0), ("chewing-gum", 1), ("candy", 2)] {
        let law = m.material_by_name(name).unwrap();
        let f = law
            .base()
            .element_filter
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap();
        assert_eq!(f.as_slice()[0], el, "{name}");
        assert_eq!(
            m.material_of(Element::new(ElementType::Segment2, el, GhostType::NotGhost))
                .unwrap()
                .base()
                .name,
            name
        );
    }
}

/// `TestLumpedMassesFixture.TestLumpedMass`: per component, the nodal
/// lumped masses sum to `∫ ρ` (relative tol 2e-15), every element type.
#[test]
fn test_lumped_mass() {
    let dat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.0\n]\n";
    for name in TYPES {
        let mesh = patch_mesh(name);
        let dim = mesh.spatial_dimension();
        let t = ty(name);
        let mut m = model(mesh, dat, AnalysisMethod::ExplicitLumpedMass);
        m.assemble_mass_lumped().unwrap();
        let rho = m.material(0).unwrap().rho();
        let fe = m.physics.base.fems.get("").unwrap();
        let ne = m
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nb_element(t, GhostType::NotGhost);
        let nq = fe.nb_integration_points(t, GhostType::NotGhost).unwrap();
        let rho_q = vec![rho; ne * nq];
        let mass = fe
            .integrate_scalar(
                akantu_core::array::ArrayView::new(&rho_q, 1).unwrap(),
                t,
                GhostType::NotGhost,
                None,
            )
            .unwrap();
        let masses = m.mass().unwrap();
        let masses = masses.as_slice();
        for s in 0..dim {
            let sum: Real = masses.chunks_exact(dim).map(|v| v[s]).sum();
            assert!(
                ((mass - sum) / mass).abs() < 2e-15,
                "{name}: {mass} vs {sum}"
            );
        }
    }
}

/* ---------------------------------------------------------------------- */
/* independent checks                                                       */
/* ---------------------------------------------------------------------- */

/// Rigid-body translation: a uniform velocity creates no strain, no
/// stress, no internal force; the kinetic energy stays exactly constant
/// (explicit central difference with zero acceleration).
#[test]
fn rigid_body_translation_is_stress_free() {
    for name in [
        "triangle_3",
        "quadrangle_8",
        "tetrahedron_10",
        "hexahedron_8",
    ] {
        let mesh = patch_mesh(name);
        let dim = mesh.spatial_dimension();
        let mut m = model(mesh, UNIT, AnalysisMethod::ExplicitLumpedMass);
        for v in m
            .velocity_mut()
            .unwrap()
            .as_mut_slice()
            .chunks_exact_mut(dim)
        {
            v[0] = 0.25;
            v[dim - 1] = -0.5;
        }
        let dt = 0.5 * m.stable_time_step().unwrap();
        m.set_time_step(dt, None).unwrap();
        m.solve_step(None).unwrap();
        let ek0 = m.energy("kinetic").unwrap();
        for _ in 0..20 {
            m.solve_step(None).unwrap();
        }
        let fmax = m
            .internal_force()
            .unwrap()
            .as_slice()
            .iter()
            .fold(0.0_f64, |a, b| a.nan_max(b.abs()));
        assert!(fmax < 1e-12, "{name}: internal force {fmax}");
        let s = m
            .material(0)
            .unwrap()
            .base()
            .stress
            .get(ty(name), GhostType::NotGhost)
            .unwrap();
        assert!(
            s.as_slice().iter().all(|x| x.abs() < 1e-12),
            "{name}: stress"
        );
        drop(s);
        let ek = m.energy("kinetic").unwrap();
        assert!((ek - ek0).abs() <= 1e-14 * ek0, "{name}: {ek} vs {ek0}");
    }
}

/// Energy balance of the free explicit dynamics (no load, no damping): the
/// total energy E_kin + E_pot oscillates around its initial value within
/// the O(Δt²) error of central differences (relative 1e-2 at Δt = ½ Δt_crit
/// over 200 steps; measured ≤ 4e-3).
#[test]
fn explicit_energy_is_conserved() {
    for name in [
        "triangle_3",
        "quadrangle_4",
        "tetrahedron_4",
        "hexahedron_8",
    ] {
        let mesh = patch_mesh(name);
        let dim = mesh.spatial_dimension();
        let x = mesh.nodes().as_slice().to_vec();
        let mut m = model(mesh, UNIT, AnalysisMethod::ExplicitLumpedMass);
        for (v, p) in m
            .velocity_mut()
            .unwrap()
            .as_mut_slice()
            .chunks_exact_mut(dim)
            .zip(x.chunks_exact(dim))
        {
            v[0] = 1e-2 * p[dim - 1];
            v[dim - 1] = -1e-2 * p[0];
        }
        let dt = 0.5 * m.stable_time_step().unwrap();
        m.set_time_step(dt, None).unwrap();
        m.solve_step(None).unwrap();
        let e0 = m.energy("kinetic").unwrap() + m.energy("potential").unwrap();
        let mut worst = 0.0_f64;
        for _ in 0..200 {
            m.solve_step(None).unwrap();
            let e = m.energy("kinetic").unwrap() + m.energy("potential").unwrap();
            worst = worst.nan_max(((e - e0) / e0).abs());
        }
        assert!(worst < 1e-2, "{name}: relative energy drift {worst}");
    }
}

/// Implicit trapezoidal rule (Newmark β = ¼, γ = ½) conserves the energy of
/// a linear system: E_kin + E_pot constant to the Newton tolerance.
#[test]
fn implicit_trapezoidal_energy_is_conserved() {
    for name in ["triangle_3", "quadrangle_8"] {
        let mesh = patch_mesh(name);
        let dim = mesh.spatial_dimension();
        let x = mesh.nodes().as_slice().to_vec();
        let mut m = model(mesh, UNIT, AnalysisMethod::ImplicitDynamic);
        set_newton(&mut m, 20, 1e-12, "residual");
        for (v, p) in m
            .velocity_mut()
            .unwrap()
            .as_mut_slice()
            .chunks_exact_mut(dim)
            .zip(x.chunks_exact(dim))
        {
            v[0] = 1e-2 * p[1];
            v[1] = -1e-2 * p[0];
        }
        let dt = 2. * m.stable_time_step().unwrap();
        m.set_time_step(dt, None).unwrap();
        m.solve_step(None).unwrap();
        let e0 = m.energy("kinetic").unwrap() + m.energy("potential").unwrap();
        for _ in 0..30 {
            m.solve_step(None).unwrap();
            let e = m.energy("kinetic").unwrap() + m.energy("potential").unwrap();
            assert!(((e - e0) / e0).abs() < 1e-8, "{name}: {e} vs {e0}");
        }
    }
}

/// Stable time step of a single right isosceles triangle with legs `L`:
/// `h = 2 r_in = (2 − √2) L`, `c = √((λ + 2μ)/ρ)` (plane strain).
#[test]
fn stable_time_step_matches_analytical_value() {
    let l = 0.5;
    let mut mesh = Mesh::new(2, "tri").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in [[0., 0.], [l, 0.], [0., l]] {
            acc.push_node(&p).unwrap();
        }
        acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost)
            .push_tuple(&[0 as Idx, 1, 2]);
    }
    let dat = "material elastic [\n  name = m\n  rho = 2\n  E = 3\n  nu = 0.25\n]\n";
    let mut m = model(mesh, dat, AnalysisMethod::ExplicitLumpedMass);
    let (e, nu, rho) = (3.0_f64, 0.25_f64, 2.0_f64);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let c = ((lambda + 2. * mu) / rho).sqrt();
    let h = (2. - 2.0_f64.sqrt()) * l;
    let dt = m.stable_time_step().unwrap();
    assert!(
        (dt - h / c).abs() <= 4. * Real::EPSILON * dt,
        "{dt} vs {}",
        h / c
    );
}

/// Consistent mass: for each component the entries of `M` sum to `ρ V`
/// (partition of unity of the shape functions), and `M` is symmetric
/// positive on the rigid translation.
#[test]
fn consistent_mass_totals() {
    let dat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n]\n";
    for name in TYPES {
        let mesh = patch_mesh(name);
        let dim = mesh.spatial_dimension();
        let mut m = model(mesh, dat, AnalysisMethod::ImplicitDynamic);
        m.assemble_mass().unwrap();
        let mat = m.dof_manager().matrix("M").unwrap();
        let (r, c, v) = mat.coo();
        let sym = mat.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
        let mut per = vec![0.0; dim];
        for ((&i, &j), &x) in r.iter().zip(c).zip(v) {
            let (i, j) = (i as usize, j as usize);
            if i % dim == j % dim {
                per[i % dim] += if sym && i != j { 2. * x } else { x };
            }
        }
        for s in per {
            assert!(((s - 7800.) / 7800.).abs() < 1e-13, "{name}: Σ M = {s}");
        }
    }
}

/// Reassignment (corrected `removeElements` compaction): after moving
/// elements between laws, a static solve gives the same state as a model
/// whose laws were assigned that way from the start.
#[test]
fn reassign_then_solve_equals_fresh_assignment() {
    use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
    use std::sync::atomic::{AtomicBool, Ordering};
    #[derive(Debug)]
    struct Half(std::sync::Arc<AtomicBool>);
    impl ConstitutiveLawSelector for Half {
        fn select(
            &self,
            ctx: &SelectorContext<'_>,
            e: Element,
        ) -> akantu_core::Result<Option<usize>> {
            let b = ctx.mesh.barycenter(e)?;
            let along_y = self.0.load(Ordering::Relaxed);
            let x = if along_y { b[1] } else { b[0] };
            Ok((ctx.law_by_name)(if x < 0.5 { "soft" } else { "stiff" }))
        }
    }
    let dat = "material elastic [\n  name = soft\n  rho = 1\n  E = 1\n  nu = 0.3\n]\nmaterial elastic [\n  name = stiff\n  rho = 2\n  E = 5\n  nu = 0.2\n]\n";
    let solve = |flip_before_init: bool, reassign: bool| {
        let flag = std::sync::Arc::new(AtomicBool::new(flip_before_init));
        let mut m = SolidMechanicsModel::with_parser(
            SharedMesh::new(patch_mesh("quadrangle_8")),
            None,
            "smm",
            Parser::parse_str(dat, "m.dat").unwrap(),
        )
        .unwrap();
        m.set_material_selector(Box::new(Half(flag.clone())));
        m.init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        if reassign {
            flag.store(true, Ordering::Relaxed);
            m.reassign_material().unwrap();
        }
        let x = m
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
            if p[1] < 1e-10 {
                m.blocked_dofs_mut().unwrap().as_mut_slice()[2 * n..2 * n + 2].fill(true);
            } else if (p[1] - 1.).abs() < 1e-10 {
                m.blocked_dofs_mut().unwrap().as_mut_slice()[2 * n + 1] = true;
                m.displacement_mut().unwrap().as_mut_slice()[2 * n + 1] = 1e-2;
            }
        }
        m.solve_step(None).unwrap();
        (
            m.displacement().unwrap().as_slice().to_vec(),
            m.material_by_element()
                .get(ElementType::Quadrangle8, GhostType::NotGhost)
                .unwrap()
                .as_slice()
                .to_vec(),
        )
    };
    let fresh = solve(true, false);
    let moved = solve(false, true);
    assert_eq!(fresh.1, moved.1, "material per element");
    let (n, w) = diff_stats(&moved.0, &fresh.0);
    assert!(w < 1e-12, "{n} displacements differ (max rel {w:e})");
}

/// Corrected `removeElements` compaction (upstream builds the renumbering
/// with a counter of kept elements and leaves entries unset — the oracle's
/// internals are garbage after a real move, see the report): every
/// element staying in a law keeps its internal values; elements arriving in
/// a law get the internal's default value (upstream `resizeInternals`; the
/// `eigen_grad_u` *parameter* is not re-copied, as upstream).
#[test]
fn reassign_compaction_keeps_internal_values() {
    use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
    use std::sync::atomic::{AtomicBool, Ordering};
    #[derive(Debug)]
    struct Half(std::sync::Arc<AtomicBool>);
    impl ConstitutiveLawSelector for Half {
        fn select(
            &self,
            ctx: &SelectorContext<'_>,
            e: Element,
        ) -> akantu_core::Result<Option<usize>> {
            let b = ctx.mesh.barycenter(e)?;
            let x = if self.0.load(Ordering::Relaxed) {
                b[1]
            } else {
                b[0]
            };
            Ok((ctx.law_by_name)(if x < 0.5 { "a" } else { "b" }))
        }
    }
    let dat = "material elastic [\n  name = a\n  rho = 1\n  E = 1\n  nu = 0.3\n]\nmaterial elastic [\n  name = b\n  rho = 1\n  E = 2\n  nu = 0.3\n  eigen_grad_u = [[0.5, 0], [0, 0.5]]\n]\n";
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(patch_mesh("triangle_6")),
        None,
        "smm",
        Parser::parse_str(dat, "m.dat").unwrap(),
    )
    .unwrap();
    m.set_material_selector(Box::new(Half(flag.clone())));
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let t = ElementType::Triangle6;
    let nq = m
        .physics
        .base
        .fems
        .get("")
        .unwrap()
        .nb_integration_points(t, GhostType::NotGhost)
        .unwrap();
    // tag law a's eigen_grad_u with the global element index
    let before_a: Vec<Idx> = m
        .material(0)
        .unwrap()
        .base()
        .element_filter
        .get(t, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    {
        let law = m.material_mut(0).unwrap();
        let filter = law
            .base()
            .element_filter
            .get(t, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .to_vec();
        let mut eig = law
            .base_mut()
            .eigen_grad_u
            .get_mut(t, GhostType::NotGhost)
            .unwrap();
        let eig = eig.as_mut_slice();
        for (local, &g) in filter.iter().enumerate() {
            for q in 0..nq {
                eig[(local * nq + q) * 4..(local * nq + q + 1) * 4].fill(Real::from(g));
            }
        }
    }
    flag.store(true, Ordering::Relaxed);
    m.reassign_material().unwrap();
    let mut moved_in_b = 0;
    for (law_i, expect_tag) in [(0usize, true), (1usize, false)] {
        let law = m.material(law_i).unwrap();
        let filter = law
            .base()
            .element_filter
            .get(t, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .to_vec();
        let eig = law.base().eigen_grad_u.get(t, GhostType::NotGhost).unwrap();
        let eig = eig.as_slice();
        assert_eq!(eig.len(), filter.len() * nq * 4);
        for (local, &g) in filter.iter().enumerate() {
            let v = eig[local * nq * 4];
            if expect_tag {
                let want = if before_a.contains(&g) {
                    Real::from(g)
                } else {
                    0.0
                };
                assert_eq!(
                    v, want,
                    "law a, element {g}: internal moved to another element"
                );
            } else if v == 0.0 {
                moved_in_b += 1; // arrived: default value
            } else {
                assert_eq!(v, 0.5, "law b, element {g}");
            }
        }
        let loc = m
            .material_local_numbering()
            .get(t, GhostType::NotGhost)
            .unwrap()
            .as_slice();
        for (local, &g) in filter.iter().enumerate() {
            assert_eq!(
                loc[g as usize] as usize, local,
                "local numbering of element {g}"
            );
        }
    }
    assert!(
        moved_in_b > 0,
        "the reassignment moved no element into law b"
    );
}
