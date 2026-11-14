// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_energies/{test_solid_mechanics_model_work_quasistatic.cc,
// test_solid_mechanics_model_work_dynamics.cc,
// test_solid_mechanics_model_work_material.dat} and the fixture
// test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_fixture.hh
// (gtest suite `test_solid_mechanics_model_energies`).
//! Ports of the two work tests of the energies suite (TEST-130, TEST-131),
//! same intent, same loads, same upstream tolerances, for the 12 element
//! types of `TestSMMFixture` (pentahedra included, as upstream's
//! `gtest_element_types`). The other two cases of the suite
//! (`LinearElasticPotentialEnergy`, `KineticEnergy*`) are ported in
//! `model_ported_tests.rs`.
//!
//! * `WorkQuasistatic`: patch mesh `_<type>.msh` (unit domain), static
//!   analysis; x blocked on the `x = x_min` side (prescribed `u ∈ {0, 0.1,
//!   −0.1}`), y/z blocked on its lowest edge/corner; a surface traction
//!   `t_x = i / 100` on `x = x_max`, ramped over `i = 0…100`; the sum of the
//!   per-step (infinitesimal) external work `F·Δu` must equal the final
//!   potential energy within 1e-2 (upstream: "rather large error" of the
//!   finite integration steps), and be 0 within 1e-12 at `i = 0`. Added
//!   independent check: the problem is linear, so the
//!   right Riemann sum of the ramp is exactly `W = (N + 1)/N · E_pot`;
//!   checked within 1e-12 (round-off; rule in `docs/NUMERICAL_VALIDATION.md`,
//!   reviewed), which also sees work errors
//!   far below upstream's 1e-2.
//! * `WorkExplicit` (`TestSMMFixtureWorkDynamic`): bar mesh `bar_<type>.msh`,
//!   explicit lumped mass, `Δt = 0.1 Δt_crit`, traction `t_x = 0.5` on
//!   `x = x_max`, `u_x ∈ {0, 0.01, −0.01}` prescribed on `x = x_min`; after 25
//!   steps, the work summed over 200 more steps must equal the change of
//!   total energy `E_kin + E_pot` within 5e-2 (upstream: "not very exact for
//!   such a coarse mesh"). `WorkImplicit` is commented out upstream.
//!
//! As upstream, 1-D tractions are applied as nodal forces on the `x_max`
//! nodes (upstream's "hack around non-implemented `FromTraction` for 1D").
//! The node and element groups are created before the model (upstream
//! creates them after `initFull`; mesh groups do not depend on the model).

mod model_common;

use std::path::Path;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{ElementType, Idx, Real, TypeFilter};
use akantu_mesh::utils::build_facets;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromTraction};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::read_mesh;

/// One `#[test]` per element type of `TestElementTypes`
/// (`test_gtest_utils.hh`: regular types, order 1/2) in a module named after
/// the upstream test, so that the 12 types run on the test threads;
/// `cargo test -p akantu-solid --test upstream_energies
/// test_solid_mechanics_model_work_quasistatic` runs every type.
macro_rules! for_each_type {
    ($module:ident => $runner:ident) => {
        mod $module {
            for_each_type!(@tests $runner;
                segment_2, segment_3, triangle_3, triangle_6, quadrangle_4, quadrangle_8,
                tetrahedron_4, tetrahedron_10, hexahedron_8, hexahedron_20, pentahedron_6,
                pentahedron_15);
        }
    };
    (@tests $runner:ident; $($ty:ident),*) => {
        $(
            #[test]
            fn $ty() {
                super::$runner(stringify!($ty));
            }
        )*
    };
}

/// `test_solid_mechanics_model_work_material.dat` (verbatim).
const WORK_MATERIAL: &str = "material elastic [\n\
\t name = uniterial\n\
\t rho = 1    # density\n\
\t E   = 1    # young's modulus\n\
\t nu  = 0.0  # poisson's ratio\n\
]\n";

fn ty(name: &str) -> ElementType {
    format!("_{name}").parse().unwrap()
}

/// `TestSMMFixture::SetUp`: read the mesh, `buildFacets` when the mesh has
/// no element of dimension − 1 (2-D/3-D only).
fn fixture_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = read_mesh(path, dim);
    if dim > 1 && mesh.nb_element_filtered(TypeFilter::new().dim(dim - 1)) == 0 {
        let _ = build_facets(&mut mesh).unwrap();
    }
    mesh
}

/// Patch mesh `_<type>.msh` (the copy kept with the oracle fixtures).
fn patch_mesh(name: &str) -> Mesh {
    let f = fixtures_root()
        .join("model/smm")
        .join(format!("lumped_mass_{name}"))
        .join(format!("inputs/_{name}.msh"));
    fixture_mesh(&f, ty(name).spatial_dimension())
}

/// Frozen `bar_<type>.msh` (D-009).
fn bar_mesh(name: &str) -> Mesh {
    let f = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join(format!("bar_{name}.msh"));
    fixture_mesh(&f, ty(name).spatial_dimension())
}

/// `mesh.getLowerBounds()` / `getUpperBounds()`.
fn bounds(mesh: &Mesh) -> (Vec<Real>, Vec<Real>) {
    let dim = mesh.spatial_dimension();
    let mut lo = vec![Real::MAX; dim];
    let mut hi = vec![Real::MIN; dim];
    for p in mesh.nodes().as_slice().chunks_exact(dim) {
        for d in 0..dim {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    (lo, hi)
}

/// Node groups `apply_force` (`x > x_max − 1e-6`) and `fixed`
/// (`x < x_min + 1e-6`), and the element groups of dimension − 1 built on
/// them (`createElementGroupFromNodeGroup`). Returns the `fixed` nodes
/// (WorkQuasistatic blocks some of their y/z DOFs).
fn work_groups(mesh: &mut Mesh) -> Vec<Idx> {
    let dim = mesh.spatial_dimension();
    let (lo, hi) = bounds(mesh);
    let (mut force, mut fixed) = (Vec::new(), Vec::new());
    for (i, p) in mesh.nodes().as_slice().chunks_exact(dim).enumerate() {
        let i = Idx::try_from(i).unwrap();
        if p[0] > hi[0] - 1e-6 {
            force.push(i);
        } else if p[0] < lo[0] + 1e-6 {
            fixed.push(i);
        }
    }
    for (g, nodes) in [("apply_force", &force), ("fixed", &fixed)] {
        let ng = mesh.groups_mut().create_node_group(g).unwrap();
        for &n in nodes {
            ng.add(n, false);
        }
    }
    mesh.create_element_group_from_node_group("el_apply_force", "apply_force", dim - 1)
        .unwrap();
    mesh.create_element_group_from_node_group("el_fixed", "fixed", dim - 1)
        .unwrap();
    fixed
}

fn model(mesh: Mesh, method: AnalysisMethod) -> SolidMechanicsModel {
    let parser = Parser::parse_str(
        WORK_MATERIAL,
        "test_solid_mechanics_model_work_material.dat",
    )
    .unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    m.init_full(&ModelOptions::new(method)).unwrap();
    m
}

/// Surface traction `t = (t_x, 0, …)` on `el_apply_force`; in 1-D the
/// upstream hack: `f_ext(x_max nodes) = t_x`.
fn apply_traction(m: &mut SolidMechanicsModel, x: &[Real], x_max: Real, t_x: Real) {
    let dim = m.spatial_dimension();
    if dim == 1 {
        let mut f_g = m.external_force_mut().unwrap();
        let f = f_g.as_mut_slice();
        for (n, p) in x.iter().enumerate() {
            if *p > x_max - 1e-6 {
                f[n] = t_x;
            }
        }
    } else {
        let mut t = vec![0.0; dim];
        t[0] = t_x;
        m.apply_neumann_bc(&FromTraction::new(t), "el_apply_force")
            .unwrap();
    }
}

fn fixed_x(u: Real) -> FixedValue {
    FixedValue {
        value: u,
        axis: SpatialDirection::X,
    }
}

/// Upstream `TestSMMFixture.WorkQuasistatic` for one element type.
fn work_quasistatic(name: &str) {
    let mut mesh = patch_mesh(name);
    let dim = mesh.spatial_dimension();
    let (lo, hi) = bounds(&mesh);
    let fixed = work_groups(&mut mesh);
    let x = mesh.nodes().as_slice().to_vec();
    let mut m = model(mesh, AnalysisMethod::Static);
    {
        let mut flags_g = m.blocked_dofs_mut().unwrap();
        let flags = flags_g.as_mut_slice();
        for &n in &fixed {
            let n = usize::try_from(n).unwrap();
            let p = &x[n * dim..(n + 1) * dim];
            if dim > 1 && p[1] < lo[1] + 1e-6 {
                flags[n * dim + 1] = true;
                if dim > 2 && p[2] < lo[2] + 1e-6 {
                    flags[n * dim + 2] = true;
                }
            }
        }
    }
    let x_only: Vec<Real> = x.chunks_exact(dim).map(|p| p[0]).collect();
    const N: usize = 100;
    for u in [0.0, 0.1, -0.1] {
        m.apply_dirichlet_bc(&fixed_x(u), "el_fixed").unwrap();
        let mut work = 0.0;
        let mut epot = 0.0;
        for i in 0..=N {
            m.external_force_mut().unwrap().as_mut_slice().fill(0.0);
            apply_traction(&mut m, &x_only, hi[0], (1.0 * i as Real) / N as Real);
            m.solve_step(None).unwrap();
            epot = m.energy("potential").unwrap();
            // In static, this is the infinitesimal work.
            let fds = m.energy("external work").unwrap();
            work += fds;
            if i == 0 {
                assert!(
                    work.abs() <= 1e-12,
                    "{name} u = {u}: work at zero force = {work:e}"
                );
            }
        }
        // Independent check (rule in NUMERICAL_VALIDATION,
        // reviewed): the problem is linear and the
        // traction ramp is `i/N`, so the right Riemann sum of the work is
        // exactly `(N + 1)/N · E_pot` (up to round-off).
        let riemann = (N as Real + 1.0) / N as Real * epot;
        eprintln!(
            "work_quasistatic {name} u = {u}: work {work:.6e}, E_pot {epot:.6e}, |diff| {:.3e} \
             (tol 1e-2), |W − (N+1)/N E_pot| {:.3e} (tol 1e-12)",
            (work - epot).abs(),
            (work - riemann).abs()
        );
        assert!(
            (work - epot).abs() <= 1e-2,
            "{name} u = {u}: work {work} vs E_pot {epot}"
        );
        assert!(
            epot > 0.0 && (work - riemann).abs() <= 1e-12,
            "{name} u = {u}: work {work} vs (N + 1)/N · E_pot = {riemann}"
        );
    }
}

// `TestSMMFixture.WorkQuasistatic` (12 types, tolerances 1e-12 / 1e-2).
for_each_type!(test_solid_mechanics_model_work_quasistatic => work_quasistatic);

/// Upstream `TestSMMFixtureWorkDynamic.WorkExplicit` for one element type.
fn work_explicit(name: &str) {
    let mut mesh = bar_mesh(name);
    let dim = mesh.spatial_dimension();
    let (_, hi) = bounds(&mesh);
    let _ = work_groups(&mut mesh);
    let x_only: Vec<Real> = mesh
        .nodes()
        .as_slice()
        .chunks_exact(dim)
        .map(|p| p[0])
        .collect();
    let mut m = model(mesh, AnalysisMethod::ExplicitLumpedMass);
    apply_traction(&mut m, &x_only, hi[0], 0.5);
    let dt = m.stable_time_step().unwrap() * 0.1;
    m.set_time_step(dt, None).unwrap();

    for u in [0.00, 0.01, -0.01] {
        m.apply_dirichlet_bc(&fixed_x(u), "el_fixed").unwrap();
        // "Equilibrate" to get a reference state of total energy and work.
        for _ in 0..25 {
            m.solve_step(None).unwrap();
        }
        let etot0 = m.energy("potential").unwrap() + m.energy("kinetic").unwrap();
        let mut w = 0.0;
        for _ in 0..200 {
            m.solve_step(None).unwrap();
            w += m.energy("external work").unwrap();
        }
        let epot = m.energy("potential").unwrap();
        let ekin = m.energy("kinetic").unwrap();
        let de = ekin + epot - etot0;
        eprintln!(
            "work_explicit {name} u = {u}: W {w:.6e}, ΔE {de:.6e}, |diff| {:.3e} (tol 5e-2)",
            (w - de).abs()
        );
        assert!(
            (w - de).abs() <= 5e-2,
            "{name} u = {u}: work {w} vs ΔE {de} (E_kin {ekin}, E_pot {epot})"
        );
    }
}

// `TestSMMFixtureWorkDynamic.WorkExplicit` (12 types, tolerance 5e-2).
for_each_type!(test_solid_mechanics_model_work_explicit => work_explicit);
