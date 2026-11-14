// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_energies/{test_solid_mechanics_model_linear_elastic_potential_energy.cc,
// test_solid_mechanics_model_kinetic_energy.cc} and the fixture
// test_solid_mechanics_model_fixture.hh (rank 0 reads, `mesh->distribute()`);
// CMake: `PARALLEL_LEVEL 1 2`.

//! TEST-128 / TEST-129 at 2 ranks: the energies of the solid
//! mechanics model on a distributed patch mesh (local world, RCB partition)
//! equal the analytic values of the serial tests (`E_pot = ½ ε²`,
//! `E_kin = ½`, tolerance 1e-8: upstream's), the energy being the
//! reduction over the ranks.

mod model_common;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SparseSolverType};
use akantu_core::{ElementType, Real};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;

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

const UNIT: &str = "material elastic [\n  name = uniterial\n  rho = 1\n  E = 1\n  nu = 0.0\n]\n";

fn ty(name: &str) -> ElementType {
    format!("_{name}").parse().unwrap()
}

/// `TestSMMFixture::SetUp` on `comm`: rank 0 reads the patch mesh, then
/// `mesh->distribute()` (RCB partition).
fn patch_mesh(comm: &Communicator, name: &str) -> Mesh {
    let f = fixtures_root()
        .join("model/smm")
        .join(format!("lumped_mass_{name}"))
        .join(format!("inputs/_{name}.msh"));
    let dim = ty(name).spatial_dimension();
    let mut mesh = Mesh::with_communicator(dim, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, &f, MeshIoType::Gmsh).unwrap();
    }
    let mut p = RcbPartitioner;
    let _events = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    // the mesh is really shared: every rank owns elements and they add up to
    // the serial mesh
    let mut owned =
        [
            u32::try_from(mesh.nb_element_filtered(akantu_core::TypeFilter::new().dim(dim)))
                .unwrap(),
        ];
    assert!(
        owned[0] > 0,
        "rank {} owns no element of {name}",
        comm.rank()
    );
    comm.all_reduce(&mut owned, akantu_comm::communicator::ReduceOp::Sum)
        .unwrap();
    let serial = {
        let mut m = Mesh::new(dim, "serial").unwrap();
        akantu_io::read_mesh(&mut m, &f, MeshIoType::Gmsh).unwrap();
        m.nb_element_filtered(akantu_core::TypeFilter::new().dim(dim))
    };
    assert_eq!(owned[0] as usize, serial, "{name}");
    mesh
}

fn model(mesh: Mesh, method: AnalysisMethod) -> SolidMechanicsModel {
    let parser = Parser::parse_str(UNIT, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    let mut options = ModelOptions::new(method);
    if method == AnalysisMethod::Static {
        // the Eigen backend refuses parallel systems (as upstream); the
        // faer extension solves the centralised system on rank 0 (D-040)
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    m.init_full(&options).unwrap();
    m
}

/// Global bounds of the mesh (upstream `getLowerBounds()`).
fn bounds(comm: &Communicator, mesh: &Mesh) -> (Vec<Real>, Vec<Real>) {
    let dim = mesh.spatial_dimension();
    let mut lo = vec![Real::MAX; dim];
    let mut hi = vec![Real::MIN; dim];
    for p in mesh.nodes().as_slice().chunks_exact(dim) {
        for d in 0..dim {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    comm.all_reduce(&mut lo, akantu_comm::communicator::ReduceOp::Min)
        .unwrap();
    comm.all_reduce(&mut hi, akantu_comm::communicator::ReduceOp::Max)
        .unwrap();
    (lo, hi)
}

fn potential_energy(comm: Communicator, name: &str) -> Result<(), String> {
    let mesh = patch_mesh(&comm, name);
    let dim = mesh.spatial_dimension();
    let x = mesh.nodes().as_slice().to_vec();
    let (lo, hi) = bounds(&comm, &mesh);
    let length = hi[0] - lo[0];
    let mut m = model(mesh, AnalysisMethod::Static);
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
        m.solve_step(None).map_err(|e| e.to_string())?;
        let ep = m.energy("potential").map_err(|e| e.to_string())?;
        if (ep - 0.5 * eps * eps).abs() >= 1e-8 {
            return Err(format!("{name} ε = {eps}: E_pot = {ep}"));
        }
    }
    Ok(())
}

fn kinetic_energy(comm: Communicator, name: &str, method: AnalysisMethod) -> Result<(), String> {
    let mesh = patch_mesh(&comm, name);
    let dim = mesh.spatial_dimension();
    let mut m = model(mesh, method);
    m.assemble_mass_lumped().map_err(|e| e.to_string())?;
    for v in m
        .velocity_mut()
        .unwrap()
        .as_mut_slice()
        .chunks_exact_mut(dim)
    {
        v[0] = 1.;
    }
    let dt = m.stable_time_step().map_err(|e| e.to_string())? * 0.8;
    m.set_time_step(dt, None).map_err(|e| e.to_string())?;
    for _ in 0..5 {
        m.solve_step(None).map_err(|e| e.to_string())?;
        let ek = m.energy("kinetic").map_err(|e| e.to_string())?;
        if (ek - 0.5).abs() >= 1e-8 {
            return Err(format!("{method} {name}: E_kin = {ek}"));
        }
    }
    Ok(())
}

fn on_two_ranks(f: impl Fn(Communicator) -> Result<(), String> + Sync) {
    for (r, o) in run_local(2, Duration::from_secs(300), f)
        .into_iter()
        .enumerate()
    {
        if let Err(e) = o {
            panic!("rank {r}: {e}");
        }
    }
}

macro_rules! per_type {
    ($($ty:ident),*) => {
        mod potential {
            use super::*;
            $( #[test] fn $ty() { on_two_ranks(|c| potential_energy(c, stringify!($ty))); } )*
        }
        mod kinetic_explicit {
            use super::*;
            $( #[test] fn $ty() {
                on_two_ranks(|c| kinetic_energy(c, stringify!($ty), AnalysisMethod::ExplicitLumpedMass));
            } )*
        }
        mod kinetic_implicit {
            use super::*;
            $( #[test] fn $ty() {
                on_two_ranks(|c| kinetic_energy(c, stringify!($ty), AnalysisMethod::ImplicitDynamic));
            } )*
        }
    };
}

per_type!(
    segment_2,
    segment_3,
    triangle_3,
    triangle_6,
    quadrangle_4,
    quadrangle_8,
    tetrahedron_4,
    tetrahedron_10,
    hexahedron_8,
    hexahedron_20,
    pentahedron_6,
    pentahedron_15
);

#[test]
fn twelve_types() {
    assert_eq!(TYPES.len(), 12);
}

// ------------------------------------------------------------- work tests

use akantu_core::enums::SpatialDirection;
use akantu_core::{Idx, TypeFilter};
use akantu_mesh::utils::build_facets;
use akantu_model::boundary_condition::{FixedValue, FromTraction};

/// `TestSMMFixture::SetUp` of the work tests on `comm`: distributed, then
/// `buildFacets` when the mesh has no element of dimension − 1.
fn work_mesh(comm: &Communicator, f: &std::path::Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::with_communicator(dim, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, f, MeshIoType::Gmsh).unwrap();
    }
    let mut p = RcbPartitioner;
    let _events = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    if dim > 1 && mesh.nb_element_filtered(TypeFilter::new().dim(dim - 1)) == 0 {
        let _ = build_facets(&mut mesh).unwrap();
    }
    mesh
}

fn work_groups(mesh: &mut Mesh, lo: &[Real], hi: &[Real]) -> Vec<Idx> {
    let dim = mesh.spatial_dimension();
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

fn apply_traction(m: &mut SolidMechanicsModel, x: &[Real], x_max: Real, t_x: Real) {
    let dim = m.spatial_dimension();
    if dim == 1 {
        let mut f_g = m.external_force_mut().unwrap();
        for (n, p) in x.iter().enumerate() {
            if *p > x_max - 1e-6 {
                f_g.as_mut_slice()[n] = t_x;
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

fn work_model(mesh: Mesh, method: AnalysisMethod) -> SolidMechanicsModel {
    let parser = Parser::parse_str(UNIT, "work.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    let mut options = ModelOptions::new(method);
    if method == AnalysisMethod::Static {
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    m.init_full(&options).unwrap();
    m
}

fn work_quasistatic(comm: Communicator, name: &str) -> Result<(), String> {
    let f = fixtures_root()
        .join("model/smm")
        .join(format!("lumped_mass_{name}"))
        .join(format!("inputs/_{name}.msh"));
    let dim = ty(name).spatial_dimension();
    let mut mesh = work_mesh(&comm, &f, dim);
    let (lo, hi) = bounds(&comm, &mesh);
    let fixed = work_groups(&mut mesh, &lo, &hi);
    let x = mesh.nodes().as_slice().to_vec();
    let mut m = work_model(mesh, AnalysisMethod::Static);
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
        m.apply_dirichlet_bc(&fixed_x(u), "el_fixed")
            .map_err(|e| e.to_string())?;
        let mut work = 0.0;
        let mut epot = 0.0;
        for i in 0..=N {
            m.external_force_mut().unwrap().as_mut_slice().fill(0.0);
            apply_traction(&mut m, &x_only, hi[0], (i as Real) / N as Real);
            m.solve_step(None).map_err(|e| e.to_string())?;
            epot = m.energy("potential").map_err(|e| e.to_string())?;
            work += m.energy("external work").map_err(|e| e.to_string())?;
            if i == 0 && work.abs() > 1e-12 {
                return Err(format!("{name} u = {u}: work at zero force = {work:e}"));
            }
        }
        if (work - epot).abs() > 1e-2 {
            return Err(format!("{name} u = {u}: work {work} vs E_pot {epot}"));
        }
    }
    Ok(())
}

/// `TestSMMFixtureWorkDynamic.WorkExplicit` on `bar_<type>.msh`: after 25
/// steps the work summed over 200 more equals the change of the total energy
/// within 5e-2 (upstream's criterion).
fn work_explicit(comm: Communicator, name: &str) -> Result<(), String> {
    let f = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join(format!("bar_{name}.msh"));
    let dim = ty(name).spatial_dimension();
    let mut mesh = work_mesh(&comm, &f, dim);
    let (lo, hi) = bounds(&comm, &mesh);
    let _ = work_groups(&mut mesh, &lo, &hi);
    let x_only: Vec<Real> = mesh
        .nodes()
        .as_slice()
        .chunks_exact(dim)
        .map(|p| p[0])
        .collect();
    let mut m = work_model(mesh, AnalysisMethod::ExplicitLumpedMass);
    apply_traction(&mut m, &x_only, hi[0], 0.5);
    let dt = m.stable_time_step().map_err(|e| e.to_string())? * 0.1;
    m.set_time_step(dt, None).map_err(|e| e.to_string())?;
    for u in [0.00, 0.01, -0.01] {
        m.apply_dirichlet_bc(&fixed_x(u), "el_fixed")
            .map_err(|e| e.to_string())?;
        for _ in 0..25 {
            m.solve_step(None).map_err(|e| e.to_string())?;
        }
        let etot0 = m.energy("potential").map_err(|e| e.to_string())?
            + m.energy("kinetic").map_err(|e| e.to_string())?;
        let mut w = 0.0;
        for _ in 0..200 {
            m.solve_step(None).map_err(|e| e.to_string())?;
            w += m.energy("external work").map_err(|e| e.to_string())?;
        }
        let epot = m.energy("potential").map_err(|e| e.to_string())?;
        let ekin = m.energy("kinetic").map_err(|e| e.to_string())?;
        let de = ekin + epot - etot0;
        if (w - de).abs() > 5e-2 {
            return Err(format!("{name} u = {u}: work {w} vs ΔE {de}"));
        }
    }
    Ok(())
}

macro_rules! work_per_type {
    ($($t:ident),*) => {
        mod work_quasistatic_2_ranks {
            use super::*;
            $( #[test] fn $t() { on_two_ranks(|c| work_quasistatic(c, stringify!($t))); } )*
        }
        mod work_explicit_2_ranks {
            use super::*;
            $( #[test] fn $t() { on_two_ranks(|c| work_explicit(c, stringify!($t))); } )*
        }
    };
}
work_per_type!(
    segment_2,
    segment_3,
    triangle_3,
    triangle_6,
    quadrangle_4,
    quadrangle_8,
    tetrahedron_4,
    tetrahedron_10,
    hexahedron_8,
    hexahedron_20,
    pentahedron_6,
    pentahedron_15
);
