// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_mesh_utils/test_mesh_periodic/
// {test_periodic_static.cc, test_periodic_static_fixture.hh,
// test_periodic_mass.cc, test_mesh_periodic_fixture.hh, material.dat}
// (gtest suite `test_mesh_periodic`, PARALLEL_LEVEL 1 2).
//! Serial ports (one rank) of the solid-mechanics cases of the periodic
//! mesh suite (TEST-043, TEST-044); the mesh-only cases of the suite are in
//! `akantu-mesh/tests/upstream_mesh_utils.rs`.
//!
//! * `TestPeriodicStatic.Traction`, typed cases `(dim, periodic dims,
//!   tensile)` ∈ {(1,1), (2,1), (3,0), (3,1), (3,2)}: frozen `line.msh`,
//!   `square.msh`, `cube.msh` (D-009) made periodic in the first
//!   `periodic dims` directions, steel (`material.dat`), static analysis,
//!   Newton-Raphson (max 5 iterations, threshold 2e-4 on the solution);
//!   corner fixed, a traction of 1e9 on the top face (1-D: nodal force on
//!   `x-plus`, made periodic-consistent with the default `_sum`), checked
//!   against upstream's `ValidationHelper` stress and strain references
//!   (stress: ‖σ − σ_ref‖∞ / ‖σ_ref‖ ≤ 2e-9 on every quadrature point;
//!   displacement: ‖u − (x − x_min) ε_ref‖∞ / ‖ε_ref‖ ≤ 1e-6 on every local or
//!   master node; 1-D compares magnitudes, as upstream).
//! * `TestPeriodicMass.UniformMass`: `cube.msh` periodic in x, y and z,
//!   explicit lumped mass: every component of the lumped mass of every local
//!   or master node equals the first one within a relative 2e-11.
//!
//! The 2-rank variants (upstream `PARALLEL_LEVEL 1 2`) and 4-rank runs:
//! `*_parallel` (local world, rank 0 reads, RCB, then the distributed
//! `makePeriodic`).
//!
//! The ParaView dumps of the upstream tests are not reproduced.

mod model_common;

use akantu_validation::reduce::NanMinMax;
use std::path::PathBuf;

use akantu_core::enums::{AnalysisMethod, ReduceOperation, SpatialDirection};
use akantu_core::{Array, ElementType, GhostType, Idx, Real, TypeFilter};
use akantu_mesh::periodic::{make_consistent_for_periodicity_real, make_periodic};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromTraction};
use akantu_parser::{ParamValue, Parser};
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::{read_mesh, set_newton};

/// `test/test_mesh_utils/test_mesh_periodic/material.dat` (verbatim).
const MATERIAL: &str = "material elastic [\n\
\t name = steel\n\
\t rho = 7800   # density\n\
\t E   = 2.1e11 # young's modulus\n\
\t nu  = 0.3    # poisson's ratio\n\
]\n";

const DISPLACEMENT_THRESHOLD: Real = 1e-6;
const STRESS_THRESHOLD: Real = 2e-9;

const DIRECTIONS: [SpatialDirection; 3] = [
    SpatialDirection::X,
    SpatialDirection::Y,
    SpatialDirection::Z,
];

/// `TestMeshPeriodic::SetUp`: read `line/square/cube.msh` (frozen) and call
/// `makePeriodic` for the first `dim_periodic` directions, on `comm`
/// (`None`: serial, as upstream's one-rank run): rank 0 reads, the mesh is
/// distributed (RCB; upstream's default partitioner is Scotch, the test's
/// criteria do not depend on the partition), then made periodic (the
/// distributed `makePeriodic`), as upstream's
/// `TestMeshPeriodic::SetUp`.
fn periodic_mesh_on(comm: Option<&Communicator>, dim: usize, dim_periodic: usize) -> Mesh {
    let name = ["line", "square", "cube"][dim - 1];
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_periodic")
        .join(format!("{name}.msh"));
    let mut mesh = match comm {
        None => read_mesh(&f, dim),
        Some(c) => {
            let mut mesh = Mesh::with_communicator(dim, "mesh", c.clone()).unwrap();
            if c.rank() == 0 {
                akantu_io::read_mesh(&mut mesh, &f, MeshIoType::Gmsh).unwrap();
            }
            let mut p = RcbPartitioner;
            let _events =
                distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
            mesh
        }
    };
    for &d in &DIRECTIONS[..dim_periodic] {
        let _ = make_periodic(&mut mesh, d, None).unwrap();
    }
    assert_eq!(mesh.is_periodic(), dim_periodic > 0);
    mesh
}

fn model(mesh: Mesh, method: AnalysisMethod) -> SolidMechanicsModel {
    let parser = Parser::parse_str(MATERIAL, "material.dat").unwrap();
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

fn real_param(m: &SolidMechanicsModel, name: &str) -> Real {
    match m.material(0).unwrap().get_param(name).unwrap() {
        ParamValue::Real(v) => v,
        other => panic!("{name}: {other:?}"),
    }
}

fn fixed(value: Real, axis: SpatialDirection) -> FixedValue {
    FixedValue { value, axis }
}

/// Column-major `dim × dim` stress reference and strain reference of
/// upstream `ValidationHelper<dim, dim_periodic>` (traction `t` along the
/// last axis).
fn references(
    dim: usize,
    dim_periodic: usize,
    t: Real,
    e: Real,
    nu: Real,
) -> (Vec<Real>, Vec<Real>) {
    let mut stress = vec![0.0; dim * dim];
    let mut strain = vec![0.0; dim];
    let at = |i: usize, j: usize| i + j * dim;
    match (dim, dim_periodic) {
        (1, _) => {
            // generic template: σ_xx = t / 2, ε_x = t / 2 / E
            stress[0] = t / 2.;
            strain[0] = t / 2. / e;
        }
        (2, 1) => {
            let s_yy = t;
            let s_xx = nu / (1.0 - nu) * s_yy;
            stress[at(0, 0)] = s_xx;
            stress[at(1, 1)] = s_yy;
            strain[1] = (1. + nu) * (-nu * s_xx + (1. - nu) * s_yy) / e;
        }
        (3, 0) => {
            stress[at(2, 2)] = t;
            let u_z = t / e;
            strain[0] = -nu * u_z;
            strain[1] = -nu * u_z;
            strain[2] = u_z;
        }
        (3, 1) => {
            let s_zz = t;
            let s_xx = nu * s_zz;
            stress[at(0, 0)] = s_xx;
            stress[at(2, 2)] = s_zz;
            strain[1] = -nu * (s_xx + s_zz) / e;
            strain[2] = (s_zz - nu * s_xx) / e;
        }
        (3, 2) => {
            let s_zz = t;
            let s_xx = nu / (1.0 - nu) * s_zz;
            stress[at(0, 0)] = s_xx;
            stress[at(1, 1)] = s_xx;
            stress[at(2, 2)] = s_zz;
            strain[2] = (s_zz - nu * s_xx - nu * s_xx) / e;
        }
        _ => unreachable!("not an upstream typed case"),
    }
    (stress, strain)
}

fn norm2(v: &[Real]) -> Real {
    v.iter().map(|x| x * x).sum::<Real>().sqrt()
}

/// `TestPeriodicStatic<(dim, dim_periodic, tensile)>.Traction`.
fn periodic_static_traction(dim: usize, dim_periodic: usize) {
    periodic_static_traction_on(None, dim, dim_periodic);
}

/// [`periodic_static_traction`] on `comm` (`None`: serial).
fn periodic_static_traction_on(comm: Option<&Communicator>, dim: usize, dim_periodic: usize) {
    let mesh = periodic_mesh_on(comm, dim, dim_periodic);
    let element_type: ElementType = mesh
        .element_types(TypeFilter::new().dim(dim))
        .next()
        .unwrap();
    let mut m = model(mesh, AnalysisMethod::Static);
    let e = real_param(&m, "E");
    let nu = real_param(&m, "nu");

    // applyBC
    for &d in &DIRECTIONS[..dim] {
        m.apply_dirichlet_bc(&fixed(0.0, d), "corner").unwrap();
    }
    let mut traction = vec![0.0; dim];
    traction[dim - 1] = 1e9;
    match dim {
        1 => {
            let mesh_h = m.physics.base.mesh.clone();
            let mesh = mesh_h.read().unwrap();
            let nodes = mesh
                .groups()
                .element_group("x-plus")
                .unwrap()
                .nodes()
                .as_slice()
                .to_vec();
            let mut f = m.external_force_mut().unwrap();
            for n in nodes {
                f.as_mut_slice()[usize::try_from(n).unwrap()] = traction[0];
            }
            // the mesh helper takes a plain array: through a copy (values
            // and order unchanged)
            let mut plain = Array::clone(&f);
            make_consistent_for_periodicity_real(&mesh, &mut plain, ReduceOperation::Sum).unwrap();
            f.copy_from(&plain).unwrap();
        }
        2 => {
            m.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::Y), "y-minus")
                .unwrap();
            m.apply_neumann_bc(&FromTraction::new(traction.clone()), "y-plus")
                .unwrap();
        }
        _ => {
            m.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::Y), "line")
                .unwrap();
            m.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::Z), "z-minus")
                .unwrap();
            m.apply_neumann_bc(&FromTraction::new(traction.clone()), "z-plus")
                .unwrap();
        }
    }
    // initSolver
    set_newton(&mut m, 5, 2e-4, "solution");
    // solve
    m.solve_step(None).unwrap();

    let case = format!("({dim}, {dim_periodic}, tensile)");
    let (s_ref, strain) = references(dim, dim_periodic, traction[dim - 1], e, nu);
    let s_ref_norm = norm2(&s_ref);

    // stresses
    let law = m.material(0).unwrap();
    let stress = law
        .base()
        .stress
        .get(element_type, GhostType::NotGhost)
        .unwrap();
    let mut worst_stress = 0.0_f64;
    for (q, s) in stress.as_slice().chunks_exact(dim * dim).enumerate() {
        let inf = s
            .iter()
            .zip(&s_ref)
            .map(|(a, b)| if dim == 1 { a.abs() - b } else { a - b }.abs())
            .fold(0.0_f64, NanMinMax::nan_max);
        let error = inf / s_ref_norm;
        worst_stress = worst_stress.nan_max(error);
        assert!(
            error.abs() <= STRESS_THRESHOLD,
            "{case}: quadrature point {q}: {s:?} != {s_ref:?} (error {error:e})"
        );
    }

    // displacements
    let mesh_h = m.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let x_g = mesh.nodes();
    let x = x_g.as_slice();
    // upstream `mesh->getLowerBounds()`: the global box (distributed: reduced
    // over the ranks), not this rank's nodes
    let lower = mesh.lower_bounds().to_vec();
    let mut local_lower = vec![Real::MAX; dim];
    for p in x.chunks_exact(dim) {
        for d in 0..dim {
            local_lower[d] = local_lower[d].min(p[d]);
        }
    }
    if comm.is_none() {
        assert_eq!(lower, local_lower, "serial: global box = local box");
    }
    let strain_norm = norm2(&strain);
    let u_g = m.displacement().unwrap();
    let u = u_g.as_slice();
    let mut worst_disp = 0.0_f64;
    for (n, (p, un)) in x.chunks_exact(dim).zip(u.chunks_exact(dim)).enumerate() {
        if !mesh
            .is_local_or_master_node(Idx::try_from(n).unwrap())
            .unwrap()
        {
            continue;
        }
        let inf = (0..dim)
            .map(|d| {
                if dim == 1 {
                    ((un[d] / strain[d]).abs() - p[d].abs()).abs()
                } else {
                    (un[d] - (p[d] - lower[d]) * strain[d]).abs()
                }
            })
            .fold(0.0_f64, NanMinMax::nan_max);
        let error = inf / strain_norm;
        worst_disp = worst_disp.nan_max(error);
        assert!(
            error.abs() <= DISPLACEMENT_THRESHOLD,
            "{case}: node {n}: u = {un:?}, x = {p:?} (error {error:e})"
        );
    }
    eprintln!(
        "periodic static {case}: max stress error {worst_stress:.3e} (tol {STRESS_THRESHOLD:e}), \
         max displacement error {worst_disp:.3e} (tol {DISPLACEMENT_THRESHOLD:e})"
    );
}

/// `TestPeriodicStatic.Traction`, the five upstream typed cases (one rank).
#[test]
fn test_periodic_static_traction() {
    for (dim, dim_periodic) in [(1, 1), (2, 1), (3, 0), (3, 1), (3, 2)] {
        periodic_static_traction(dim, dim_periodic);
    }
}

/// `TestPeriodicMass.UniformMass` (one rank).
#[test]
fn test_periodic_mass_uniform_mass() {
    let _ = periodic_mass_on(None);
}

/// Returns the number of checked components (local or master nodes).
fn periodic_mass_on(comm: Option<&Communicator>) -> usize {
    let dim = 3;
    let mesh = periodic_mesh_on(comm, dim, 3);
    let mut m = model(mesh, AnalysisMethod::ExplicitLumpedMass);
    m.assemble_mass_lumped().unwrap();
    let mesh_h = m.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let masses_g = m.mass().unwrap();
    let masses = masses_g.as_slice();
    let reference_mass = masses[0];
    assert!(reference_mass > 0.0);
    let mut worst = 0.0_f64;
    let mut checked = 0;
    for (node, masses_i) in masses.chunks_exact(dim).enumerate() {
        if !mesh
            .is_local_or_master_node(Idx::try_from(node).unwrap())
            .unwrap()
        {
            continue;
        }
        for &mass in masses_i {
            let rel = (mass - reference_mass) / reference_mass;
            worst = worst.nan_max(rel.abs());
            assert!(
                rel.abs() <= 2e-11,
                "node {node}: mass {mass} vs {reference_mass} (rel {rel:e})"
            );
            checked += 1;
        }
    }
    // 27 nodes, periodic in x, y, z: the 8 = 2³ independent nodes remain
    // local or master, the others are periodic slaves.
    if comm.is_none() {
        assert_eq!(
            checked,
            8 * dim,
            "local or master nodes of the periodic cube"
        );
    }
    eprintln!(
        "periodic mass: {checked} components, max relative deviation {worst:.3e} (tol 2e-11)"
    );
    checked
}

// ---------------------------------------------------------------------------
// 2 ranks (upstream PARALLEL_LEVEL 1 2)
// ---------------------------------------------------------------------------

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::MeshIoType;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

fn on_ranks(n: usize, f: impl Fn(&Communicator) + Send + Sync + Copy + 'static) {
    let out = run_local(n, TIMEOUT, move |c| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&c))).map_err(|e| {
            e.downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default()
        })
    });
    for (r, o) in out.into_iter().enumerate() {
        if let Err(e) = o {
            panic!("{n} ranks, rank {r}: {e}");
        }
    }
}

/// `TestPeriodicStatic.Traction` at 2 ranks (upstream `PARALLEL_LEVEL 1 2`)
/// and 4 ranks, upstream's criteria on every rank. Upstream's own test
/// passes at 2 ranks on the oracle (`mpiexec -n 2
/// test_mesh_periodic_gtest`, the six cases). Before the distributed
/// periodic numbering existed the model
/// refused the distributed periodic mesh; with only that wired,
/// (1, 1), (2, 1) and (3, 2) failed: the DOF numbering did not give the
/// periodic masters' copies their global numbers through the periodic
/// synchronizer and the solution did not reach them (upstream
/// `updateDOFsData` `_giu_global_conn`, `getArrayPerDOFs`
/// `makeConsistentForPeriodicity(_id)`).
#[test]
fn test_periodic_static_traction_parallel() {
    for n in [2, 4] {
        for (dim, dim_periodic) in [(1, 1), (2, 1), (3, 0), (3, 1), (3, 2)] {
            on_ranks(n, move |c| {
                periodic_static_traction_on(Some(c), dim, dim_periodic);
            });
        }
    }
}

/// `TestPeriodicMass.UniformMass` at 2 and 4 ranks: upstream's criterion
/// on every rank, and over the ranks the 8 independent nodes of the
/// periodic cube counted once (local or master).
#[test]
fn test_periodic_mass_uniform_mass_parallel() {
    for n in [2, 4] {
        on_ranks(n, |c| {
            let checked = periodic_mass_on(Some(c));
            let mut total = [u64::try_from(checked).unwrap()];
            c.all_reduce(&mut total, akantu_comm::ReduceOp::Sum)
                .unwrap();
            assert_eq!(total[0], 8 * 3, "independent nodes over the ranks");
        });
    }
}
