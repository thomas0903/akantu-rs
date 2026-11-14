// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_material_non_local/{test_material_damage_non_local.cc,
// test_material_non_local.cc, custom_non_local_test_material.{hh,cc},
// material.dat, material_damage_non_local.dat, mesh_section_gap.msh}.

//! TEST-147 `test_material_damage_non_local` and TEST-148
//! `test_material_non_local`.

mod non_local_common;

use std::path::PathBuf;
use std::sync::Mutex;

use akantu_core::enums::{AnalysisMethod, MeshIoType, SpatialDirection};
use akantu_core::random::{IdxSlot, RandomGenerator};
use akantu_core::tensor::MatrixX;
use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromHigherDim};
use akantu_parser::Parser;
use akantu_solid::{Material, SolidMechanicsModel};
use non_local_common::{TestNonLocalMaterial, register, test_law, test_law_mut};

/// The global random engine is process-wide: the tests drawing `Yd` run
/// one at a time.
static RNG: Mutex<()> = Mutex::new(());

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/upstream/material_non_local")
        .join(name)
}

fn frozen_mesh() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
         test_materials/test_material_non_local/mesh.msh",
    )
}

/// TEST-147 driver: `material_damage_non_local.dat` (`marigo_non_local`,
/// `remove_wf`, radius 0.01, update rate 10) on `mesh_section_gap.msh`,
/// explicit, `Δt = Δt_crit/2.5`, `u_x = 0` on `Fixed`, traction `σ =
/// 5e8·I` on `Traction`, 1100 steps; returns, for every element whose
/// barycentre lies within 0.025 of `(L/2 − 0.025, H/2 − 0.025)` in both
/// directions, the damage of its first point. `upstream_marigo`: UD-027
/// compat (the keyword allocates the local Marigo law, as the reference).
/// The upstream test draws `Yd` with a time-based seed (no `AKA_SEED`);
/// the port fixes the seed to make the run reproducible.
fn damage_non_local_run(upstream_marigo: bool) -> (Vec<Real>, bool, bool) {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut mesh, &data("mesh_section_gap.msh"), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(data("material_damage_non_local.dat")).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    if upstream_marigo {
        akantu_solid::materials::non_local::register_upstream_marigo_non_local(
            &mut model.physics.factory,
        );
    }
    RandomGenerator::<IdxSlot>::seed(5489);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let is_non_local = model.is_non_local();
    // radius 0.01 < mesh size 0.05: every neighbourhood is the point
    // itself (the non-local law then reproduces the local one up to the
    // rounding of the weight J·(1/J))
    let only_self_pairs = model.non_local_manager().is_some_and(|m| {
        m.neighborhood("concrete")
            .is_ok_and(|n| n.pair_list(GhostType::NotGhost).iter().all(|(i, j)| i == j))
    });
    let dt = model.stable_time_step().unwrap();
    model.set_time_step(dt / 2.5, None).unwrap();
    // upstream BC::Dirichlet::FixedValue(0.0): axis _x by default
    model
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::X,
            },
            "Fixed",
        )
        .unwrap();
    let mut s = MatrixX::identity(2);
    for v in s.as_mut_slice() {
        *v *= 5e8;
    }
    model
        .apply_neumann_bc(&FromHigherDim::new(s), "Traction")
        .unwrap();
    for _ in 0..1100 {
        model.solve_step(None).unwrap();
    }
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let (lo, hi) = (mesh.lower_bounds().to_vec(), mesh.upper_bounds().to_vec());
    let (l, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    let law = model.material(0).unwrap();
    let ty = ElementType::Triangle3;
    let damage = {
        let mut d = None;
        law.for_each_internal(&mut |name, f| {
            if name == "damage"
                && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
            {
                d = Some(f.get(ty, GhostType::NotGhost).unwrap().clone());
            }
        });
        d.unwrap()
    };
    let nb_gp = 1;
    // diagnostic: the damage localises (not every element is broken)
    let all = damage.as_slice();
    #[allow(clippy::cast_precision_loss)] // diagnostic ratio
    let broken = all.iter().filter(|d| **d >= 0.9).count() as Real / all.len() as Real;
    eprintln!(
        "broken fraction (d >= 0.9): {broken:.3} of {} elements",
        all.len()
    );
    assert!(broken < 0.5, "damage is not localised: {broken}");
    let mut out = Vec::new();
    for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
        let b = mesh
            .barycenter(Element::new(
                ty,
                i32::try_from(e).unwrap(),
                GhostType::NotGhost,
            ))
            .unwrap();
        if (b[0] - (l / 2.) + 0.025).abs() < 0.025 && (b[1] - (h / 2.) + 0.025).abs() < 0.025 {
            out.push(damage.as_slice()[e * nb_gp]);
        }
    }
    (out, is_non_local, only_self_pairs)
}

/// TEST-147 as the reference runs it (UD-027 compat: local Marigo, the
/// `non_local` section ignored): the damage localises in the centre
/// region (upstream criterion: every first point there ≥ 0.9).
#[test]
fn test_material_damage_non_local_upstream_local_marigo() {
    let _g = RNG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (damage, is_non_local, _) = damage_non_local_run(true);
    assert!(
        !is_non_local,
        "upstream: no non-local manager for the local Marigo law"
    );
    assert!(!damage.is_empty());
    for d in &damage {
        assert!(
            *d >= 0.9,
            "damage {d} < 0.9 in the centre region: {damage:?}"
        );
    }
}

/// TEST-147 with the corrected default (UD-027): the real non-local
/// Marigo law averaging `Y` with `remove_wf`; the same localisation
/// criterion.
#[test]
fn test_material_damage_non_local() {
    let _g = RNG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (damage, is_non_local, only_self_pairs) = damage_non_local_run(false);
    assert!(is_non_local);
    assert!(
        only_self_pairs,
        "radius 0.01 below the element size: singleton neighbourhoods"
    );
    assert!(!damage.is_empty());
    for d in &damage {
        assert!(
            *d >= 0.9,
            "damage {d} < 0.9 in the centre region: {damage:?}"
        );
    }
}

/// TEST-148 `test_material_non_local`: `custom_non_local_test_material`
/// (`local_damage` averaged into `damage` with `base_wf`, radius 0.4, then
/// `σ ← σ·(1 − damage)`). Upstream only prints and dumps (one random
/// point's `local_damage` set to 1 with a time-seeded generator); the port
/// sets the middle point and checks the averaging and the stress update
/// against the manager's own weights: `damage(q)·V(q) = J(g)·W(|x_q −
/// x_g|)` (≤ 4ε relative: `damage = fl(fl(J·W)·fl(1/V))`, three roundings)
/// with `damage(q) > 0` exactly for the points strictly inside the radius,
/// and `σ = σ_el·(1 − damage)` bit for bit under a uniform eigen strain.
#[test]
fn test_material_non_local() {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut mesh, &frozen_mesh(), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(data("material.dat")).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    register(
        &mut model.physics.factory,
        "custom_non_local_test_material",
        TestNonLocalMaterial::custom,
    );
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    assert_eq!(model.material(0).unwrap().name(), "test");
    let ty = ElementType::Triangle3;
    let ng = GhostType::NotGhost;
    // uniform eigen strain so that the elastic stress is not zero
    {
        let law = model.material_mut(0).unwrap();
        for g in law
            .base_mut()
            .eigen_grad_u
            .get_mut(ty, ng)
            .unwrap()
            .as_mut_slice()
            .as_chunks_mut::<4>()
            .0
        {
            *g = [-1e-3, 0., 0., -2e-3];
        }
    }
    model.assemble_internal_forces().unwrap();
    let sigma_el = model
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(ty, ng)
        .unwrap()
        .clone();
    assert!(
        test_law(&model, 0)
            .internal("damage")
            .unwrap()
            .get(ty, ng)
            .unwrap()
            .as_slice()
            .iter()
            .all(|d| *d == 0.)
    );
    let n = sigma_el.len();
    let g = n / 2;
    test_law_mut(&mut model, 0)
        .internal_mut("local_damage")
        .unwrap()
        .get_mut(ty, ng)
        .unwrap()
        .as_mut_slice()[g] = 1.;
    model.assemble_internal_forces().unwrap();
    let law = test_law(&model, 0);
    let damage = law
        .internal("damage")
        .unwrap()
        .get(ty, ng)
        .unwrap()
        .as_slice()
        .to_vec();
    let sigma = law.base().stress.get(ty, ng).unwrap().as_slice().to_vec();
    let manager = model.non_local_manager().unwrap();
    let nh = manager.neighborhood("test").unwrap();
    let radius = nh.radius();
    assert_eq!(radius, 0.4);
    // the law's elements are every triangle (filter = identity), one point each
    let slot = |e: usize| {
        manager
            .layout()
            .slot(ty, ng, i32::try_from(e).unwrap())
            .unwrap()
    };
    let mut x = akantu_core::Array::new(2);
    {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        model
            .physics
            .base
            .fems
            .get("")
            .unwrap()
            .integration_points_coordinates(&mesh, &mut x, ty, ng, None)
            .unwrap();
    }
    let (jac, vol) = (manager.jacobians(), manager.volumes());
    let mut touched = 0;
    for q in 0..n {
        let r = akantu_model::non_local::distance(x.tuple(q), x.tuple(g));
        let w = akantu_model::non_local::base_weight(r, radius, radius * radius);
        let expected = jac[slot(g)] * w;
        let got = damage[q] * vol[slot(q)];
        assert!(
            (got - expected).abs() <= 4. * Real::EPSILON * expected.abs(),
            "point {q}: damage·V = {got}, J(g)·W = {expected}"
        );
        assert_eq!(damage[q] > 0., r < radius, "point {q} at {r}");
        touched += usize::from(damage[q] > 0.);
        for c in 0..4 {
            assert_eq!(
                sigma[q * 4 + c],
                sigma_el.as_slice()[q * 4 + c] * (1. - damage[q])
            );
        }
    }
    assert!(touched > 1);
}
