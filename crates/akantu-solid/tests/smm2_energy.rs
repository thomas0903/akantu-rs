// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). SMM-010 / SMM-011 /
// SMM-012: the energy accessors of SolidMechanicsModel against the oracle
// cases fixtures/model/smm2/energy_* (oracle/probes/smm2), every value
// bitwise:
// - `getEnergy(id)` for kinetic, potential, external work (explicit:
//   power on the velocity × Δt; static: on the displacement increment, with
//   the reactions −f_int on the blocked DOFs), external work new and an
//   unknown id (0 per material);
// - `getEnergy(id, group)` (kinetic per volume group, unknown id) and
//   `getEnergy(id, element)` (kinetic of every element, unknown id);
// - two static load steps, so the increment differs from the displacement;
// - a mesh made periodic in x: periodic slaves are excluded from the
//   kinetic energy and the work.
// `getEnergy("potential", element/group)` is not compared: upstream
// integrates an uninitialised vector (UD-032).

mod model_common;

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_core::tensor::MatrixX;
use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_mesh::periodic::make_periodic;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromHigherDim, FromSameDim, IncrementValue};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;
use akantu_validation::Fixture;
use model_common::*;

const G: GhostType = GhostType::NotGhost;

fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

fn model_on(f: &Fixture, mesh: Mesh, method: AnalysisMethod) -> SolidMechanicsModel {
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    m.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    m.init_full(&eigen_options(method)).unwrap();
    m
}

fn fixed(m: &mut SolidMechanicsModel, value: Real, axis: SpatialDirection, group: &str) {
    m.apply_dirichlet_bc(&FixedValue { value, axis }, group)
        .unwrap();
}

fn nodes(m: &SolidMechanicsModel) -> Vec<Real> {
    let mesh_h = m.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    mesh.nodes().as_slice().to_vec()
}

/// smm_probe's `linear_velocity` (sequential sums from 0).
fn linear_velocity(m: &mut SolidMechanicsModel) {
    const BETA: [[Real; 3]; 3] = [
        [1e-2, -3e-3, 2e-3],
        [4e-3, 5e-3, -1e-3],
        [-2e-3, 1e-3, 6e-3],
    ];
    let dim = m.spatial_dimension();
    let x = nodes(m);
    let mut v_g = m.velocity_mut().unwrap();
    let v = v_g.as_mut_slice();
    for (n, vn) in v.chunks_exact_mut(dim).enumerate() {
        for (i, vi) in vn.iter_mut().enumerate() {
            let mut s = 0.0;
            for j in 0..dim {
                s += BETA[i][j] * x[n * dim + j];
            }
            *vi = s;
        }
    }
}

/// The probe's `velocity_y`.
fn velocity_y(m: &mut SolidMechanicsModel) {
    let dim = m.spatial_dimension();
    let x = nodes(m);
    let mut v_g = m.velocity_mut().unwrap();
    let v = v_g.as_mut_slice();
    for (n, vn) in v.chunks_exact_mut(dim).enumerate() {
        let y = x[n * dim + 1];
        vn[0] = 0.01 * y + 0.002;
        vn[1] = -0.005 * y;
        if dim == 3 {
            vn[2] = 0.003 * y - 0.001;
        }
    }
}

fn probe_sigma() -> MatrixX {
    let mut s = vec![0.0; 9];
    for j in 0..3 {
        for i in 0..3 {
            s[i + 3 * j] =
                1e6 * (1 + i) as Real + 3e5 * j as Real - 2e5 * if i == j { 1.0 } else { 0.0 };
        }
    }
    MatrixX::from_col_major(3, 3, s).unwrap()
}

#[test]
fn energy_explicit_3d_matches_the_oracle_bitwise() {
    let f = case("smm2/energy_explicit_3d");
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 3);
    let mut m = model_on(&f, mesh, AnalysisMethod::ExplicitLumpedMass);
    for ax in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ] {
        fixed(&mut m, 0.0, ax, "left");
    }
    m.apply_neumann_bc(&FromHigherDim::new(probe_sigma()), "right")
        .unwrap();
    m.apply_neumann_bc(&FromSameDim::new(vec![1e5, -2e5, -3e5]), "top")
        .unwrap();
    linear_velocity(&mut m);
    let stable = m.stable_time_step().unwrap();
    let dt = 0.5 * stable;
    m.set_time_step(dt, None).unwrap();
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    let (mut ek, mut ep, mut ew, mut ewn) = (vec![], vec![], vec![], vec![]);
    let (mut eks, mut eka, mut eb, mut ebg, mut u, mut v) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut snap = |m: &mut SolidMechanicsModel| {
        ek.push(m.energy("kinetic").unwrap());
        ep.push(m.energy("potential").unwrap());
        ew.push(m.energy("external work").unwrap());
        ewn.push(m.energy("external work new").unwrap());
        eks.push(m.energy_of_group("kinetic", "steel").unwrap());
        eka.push(m.energy_of_group("kinetic", "alu").unwrap());
        eb.push(m.energy("bogus").unwrap());
        ebg.push(m.energy_of_group("bogus", "alu").unwrap());
        u.extend_from_slice(m.displacement().unwrap().as_slice());
        v.extend_from_slice(m.velocity().unwrap().as_slice());
    };
    snap(&mut m);
    for _ in 0..steps {
        m.solve_step(None).unwrap();
        snap(&mut m);
    }
    n += assert_bits(&f, "energy/kinetic", &ek);
    n += assert_bits(&f, "energy/potential", &ep);
    n += assert_bits(&f, "energy/external_work", &ew);
    n += assert_bits(&f, "energy/external_work_new", &ewn);
    n += assert_bits(&f, "energy/kinetic_steel", &eks);
    n += assert_bits(&f, "energy/kinetic_alu", &eka);
    n += assert_bits(&f, "energy/bogus", &eb);
    n += assert_bits(&f, "energy/bogus_alu", &ebg);
    n += assert_bits(&f, "displacement", &u);
    n += assert_bits(&f, "velocity", &v);
    let ty = ElementType::Hexahedron8;
    let ne = {
        let mesh_h = m.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.nb_element(ty, G)
    };
    let (mut ek_el, mut eb_el) = (vec![], vec![]);
    for e in 0..ne {
        let el = Element::new(ty, e as akantu_core::Idx, G);
        ek_el.push(m.energy_of_element("kinetic", el).unwrap());
        eb_el.push(m.energy_of_element("bogus", el).unwrap());
    }
    n += assert_bits(&f, "energy/kinetic_element", &ek_el);
    n += assert_bits(&f, "energy/bogus_element", &eb_el);
    let k3 = m.kinetic_energy_of_element(Element::new(ty, 3, G)).unwrap();
    let b7 = m
        .energy_of_element("bogus", Element::new(ty, 7, G))
        .unwrap();
    n += assert_bits(&f, "energy/kinetic_type_index", &[k3, b7]);
    // independent: the two volume groups partition the 12 elements, so the
    // group kinetic energies add up to the sum of the element energies
    // (consistent `½ ∫ ρ v²`, not the lumped total). The terms are positive
    // and summed in another order: |Δ| ≤ 2·12·u·Σ < 32 ε Σ.
    let mut per_el = 0.0;
    for x in &ek_el {
        per_el += x;
    }
    let groups = eks[steps] + eka[steps];
    assert!(
        (groups - per_el).abs() <= 32.0 * f64::EPSILON * per_el,
        "{groups} vs {per_el}"
    );
    eprintln!("energy_explicit_3d: {n} values bitwise");
}

#[test]
fn energy_static_3d_matches_the_oracle_bitwise() {
    let f = case("smm2/energy_static_3d");
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 3);
    let mut m = model_on(&f, mesh, AnalysisMethod::Static);
    set_newton(&mut m, 5, 1e-10, "solution");
    for ax in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ] {
        fixed(&mut m, 0.0, ax, "left");
    }
    fixed(&mut m, 1e-4, SpatialDirection::X, "right");
    let top = FromSameDim::new(vec![1e5, -2e5, -3e5]);
    m.apply_neumann_bc(&top, "top").unwrap();
    let (mut ew, mut ewn, mut ep, mut u, mut du, mut fi, mut fe) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut its = vec![];
    for step in 0..2 {
        if step == 1 {
            m.apply_dirichlet_bc(
                &IncrementValue {
                    value: 1e-4,
                    axis: SpatialDirection::X,
                },
                "right",
            )
            .unwrap();
            m.apply_neumann_bc(&top, "top").unwrap();
        }
        m.solve_step(None).unwrap();
        its.push(m.solver.non_linear_solver(None).unwrap().nb_iterations() as i64);
        ew.push(m.energy("external work").unwrap());
        ewn.push(m.energy("external work new").unwrap());
        ep.push(m.energy("potential").unwrap());
        u.extend_from_slice(m.displacement().unwrap().as_slice());
        du.extend_from_slice(m.increment().unwrap().as_slice());
        fi.extend_from_slice(m.internal_force().unwrap().as_slice());
        fe.extend_from_slice(m.external_force().unwrap().as_slice());
    }
    assert_eq!(its, i64s(&f, "newton/nb_iterations"));
    let mut n = assert_bits(&f, "energy/external_work", &ew);
    n += assert_bits(&f, "energy/external_work_new", &ewn);
    n += assert_bits(&f, "energy/potential", &ep);
    n += assert_bits(&f, "displacement", &u);
    n += assert_bits(&f, "increment", &du);
    n += assert_bits(&f, "internal_force", &fi);
    n += assert_bits(&f, "external_force", &fe);
    // the second step's increment is not its displacement (the case
    // distinguishes `Δu` from `u`)
    let nn = u.len() / 2;
    assert_ne!(&u[nn..], &du[nn..]);
    eprintln!("energy_static_3d: {n} values bitwise");
}

#[test]
fn energy_periodic_2d_matches_the_oracle_bitwise() {
    let f = case("smm2/energy_periodic_2d");
    let mut mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let _ = make_periodic(&mut mesh, SpatialDirection::X, None).unwrap();
    let slaves: Vec<i64> = (0..mesh.nb_nodes())
        .map(|i| i64::from(mesh.is_periodic_slave(i as akantu_core::Idx).unwrap()))
        .collect();
    assert_eq!(slaves, i64s(&f, "periodic_slave"));
    assert!(slaves.contains(&1));
    let mut m = model_on(&f, mesh, AnalysisMethod::ExplicitLumpedMass);
    fixed(&mut m, 0.0, SpatialDirection::Y, "bottom");
    m.apply_neumann_bc(&FromSameDim::new(vec![1e6, -2e6]), "top")
        .unwrap();
    velocity_y(&mut m);
    let stable = m.stable_time_step().unwrap();
    let dt = 0.5 * stable;
    m.set_time_step(dt, None).unwrap();
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    m.assemble_mass_lumped().unwrap();
    n += assert_bits(&f, "mass", m.mass().unwrap().as_slice());
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    let (mut ek, mut ep, mut ew, mut ewn, mut eks, mut u, mut v) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut snap = |m: &mut SolidMechanicsModel| {
        ek.push(m.energy("kinetic").unwrap());
        ep.push(m.energy("potential").unwrap());
        ew.push(m.energy("external work").unwrap());
        ewn.push(m.energy("external work new").unwrap());
        eks.push(m.energy_of_group("kinetic", "steel").unwrap());
        u.extend_from_slice(m.displacement().unwrap().as_slice());
        v.extend_from_slice(m.velocity().unwrap().as_slice());
    };
    snap(&mut m);
    for _ in 0..steps {
        m.solve_step(None).unwrap();
        snap(&mut m);
    }
    n += assert_bits(&f, "displacement", &u);
    n += assert_bits(&f, "velocity", &v);
    n += assert_bits(&f, "energy/kinetic", &ek);
    n += assert_bits(&f, "energy/potential", &ep);
    n += assert_bits(&f, "energy/external_work", &ew);
    n += assert_bits(&f, "energy/external_work_new", &ewn);
    n += assert_bits(&f, "energy/kinetic_steel", &eks);
    eprintln!("energy_periodic_2d: {n} values bitwise");
}
