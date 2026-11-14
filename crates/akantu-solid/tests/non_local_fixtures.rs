// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential replays of the cases recorded
// by oracle/probes/non_local/non_local_probe.cc (upstream MyModel of
// test/test_model/test_common/test_non_local_toolbox/my_model.hh, the
// averaging test law of test_material_damage.{hh,cc}, and the non-local
// damage laws of src/model/solid_mechanics/materials/material_damage/).

//! Bitwise differential tests of the non-local toolbox and laws against the
//! oracle fixtures `fixtures/model/non_local/*` (D-035, x86_64):
//!
//! * `manager_plate_r*`: pair lists (identity and order), pair weights,
//!   volumes, jacobians of upstream's `MyModel`;
//! * `avg_<mesh>_<wf>`: pairs, weights, volumes and averages of the
//!   averaging test law for `base_wf`, `remove_wf`, `damage_wf` on a quad
//!   mesh (two laws), a mixed triangle/quadrangle mesh (UD-037: compat
//!   identity for the bitwise replay, the corrected default measured) and a
//!   hexahedron mesh;
//! * `law_*`: `mazars_non_local` (both averaged variables),
//!   `plastic_mazars_non_local` and upstream's `MaterialMarigoNonLocal`
//!   under a prescribed displacement history (every law internal and the
//!   internal force per step).

mod non_local_common;

use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ElementDataSelector, InternalFieldRef};
use akantu_model::non_local::{
    NoGhosts, NonLocalManager, NonLocalManagerCallback, NonLocalNeighborhood, NonLocalOptions,
    PointIdentity, QuadPoint,
};
use akantu_parser::Parser;
use akantu_solid::material::LawOptions;
use akantu_solid::{Material, SolidMechanicsModel};
use akantu_solver::SolverCallback;
use akantu_validation::Fixture;
use akantu_validation::reduce::nan_max;
use non_local_common::TestNonLocalMaterial;

const NG: GhostType = GhostType::NotGhost;

fn open(case: &str) -> Fixture {
    let f = Fixture::open_by_id(&format!("model/non_local/{case}")).unwrap();
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

fn bits(a: &[Real]) -> Vec<u64> {
    a.iter().map(|x| x.to_bits()).collect()
}

fn assert_bitwise(what: &str, got: &[Real], expected: &[Real]) {
    assert_eq!(got.len(), expected.len(), "{what}: length");
    if bits(got) != bits(expected) {
        let (k, (g, e)) = got
            .iter()
            .zip(expected)
            .enumerate()
            .find(|(_, (g, e))| g.to_bits() != e.to_bits())
            .unwrap();
        panic!("{what}: first difference at {k}: {g:e} vs oracle {e:e}");
    }
}

/// Pairs and weights of `nh` vs the fixture quantities `<prefix>pairs_*`,
/// `<prefix>weights_*` (owned list; the ghost list is empty in serial).
fn check_pairs(f: &Fixture, prefix: &str, nh: &NonLocalNeighborhood) -> usize {
    let p = f.load_i64(&format!("{prefix}pairs_not_ghost")).unwrap();
    let w = f.load_f64(&format!("{prefix}weights_not_ghost")).unwrap();
    let got: Vec<i64> = nh
        .pairs(NG)
        .flat_map(|(q1, q2)| {
            let row = |q: &QuadPoint| {
                let code = f.manifest().parameters["type_names"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|n| n.as_str() == Some(q.ty.name()))
                    .unwrap();
                [
                    i64::try_from(code).unwrap(),
                    i64::from(q.element),
                    i64::from(q.num_point),
                    i64::from(q.global_num),
                ]
            };
            let mut r = row(q1).to_vec();
            r.extend(row(q2));
            r
        })
        .collect();
    assert_eq!(got, p.data, "{prefix}pairs_not_ghost (identity and order)");
    let wg: Vec<Real> = nh.pair_weights(NG).iter().flat_map(|w| *w).collect();
    assert_bitwise(&format!("{prefix}weights_not_ghost"), &wg, &w.data);
    assert_eq!(
        f.load_i64(&format!("{prefix}pairs_ghost"))
            .unwrap()
            .data
            .len(),
        0
    );
    nh.pair_list(NG).len()
}

/// Per-slot manager array `values` vs the fixture's per-type blocks
/// `<name>/<type>`.
fn check_blocks(f: &Fixture, name: &str, manager: &NonLocalManager, values: &[Real]) {
    let mut n = 0;
    for (ty, g, off, len) in manager.layout().blocks() {
        if g != NG || len == 0 {
            continue;
        }
        let q = format!("{name}/{}", ty.name());
        let e = f.load_f64(&q).unwrap();
        assert_bitwise(&q, &values[off..off + len], &e.data);
        n += 1;
    }
    assert!(n > 0, "{name}: no block compared");
}

/* -------------------------------------------------------------------------- */
/* manager_plate_r*                                                           */
/* -------------------------------------------------------------------------- */

/// Upstream `MyModel`.
struct MyModel {
    dim: usize,
    blocks: Vec<(ElementType, GhostType, akantu_core::Array<Real>)>,
}

impl NonLocalManagerCallback for MyModel {
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> akantu_core::Result<()> {
        let mut global_num: Idx = 0;
        for (ty, g, x) in &self.blocks {
            if *g != ghost {
                continue;
            }
            for (num_point, c) in x.as_slice().chunks(self.dim).enumerate() {
                let q = QuadPoint {
                    ty: *ty,
                    ghost,
                    element: -1,
                    num_point: Idx::try_from(num_point).unwrap(),
                    global_num,
                    global_element: -1,
                };
                manager.insert_integration_point("test_region", q, c)?;
                global_num += 1;
            }
        }
        Ok(())
    }
    fn update_local_internal(
        &self,
        _: &str,
        _: GhostType,
        _: &mut akantu_core::ElementTypeMapArray<Real>,
    ) -> akantu_core::Result<()> {
        Ok(())
    }
    fn update_non_local_internal(
        &mut self,
        _: &str,
        _: GhostType,
        _: &akantu_core::ElementTypeMapArray<Real>,
        _: bool,
    ) -> akantu_core::Result<()> {
        Ok(())
    }
    fn compute_non_local_contribution(&mut self, _: GhostType) -> akantu_core::Result<()> {
        Ok(())
    }
}

fn manager_case(case: &str) {
    let f = open(case);
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut mesh, &f.input_path("mesh").unwrap(), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let fe = akantu_fe::FeEngine::with_precomputation(
        &mesh,
        None,
        akantu_core::ElementKind::Regular,
        akantu_fe::shape::ShapeFamily::Lagrange,
        "FEEngine",
    )
    .unwrap();
    let mut x = akantu_core::Array::new(2);
    fe.integration_points_coordinates(&mesh, &mut x, ElementType::Quadrangle4, NG, None)
        .unwrap();
    let mut model = MyModel {
        dim: 2,
        blocks: vec![(ElementType::Quadrangle4, NG, x)],
    };
    let mut manager = NonLocalManager::new("non_local_manager", 2);
    manager.read_sections(&parser.root, false);
    manager.set_grid_center_from(&mesh).unwrap();
    manager
        .register_neighborhood("test_region", "test_region")
        .unwrap();
    manager
        .initialize(&mesh, &fe, &mut model, &mut NoGhosts)
        .unwrap();
    let n = check_pairs(&f, "", manager.neighborhood("test_region").unwrap());
    check_blocks(&f, "volumes", &manager, manager.volumes());
    check_blocks(&f, "jacobians", &manager, manager.jacobians());
    eprintln!("{case}: {n} pairs, weights/volumes/jacobians bitwise");
}

#[test]
fn manager_plate_r0p51() {
    manager_case("manager_plate_r0p51");
}

#[test]
fn manager_plate_r0p5() {
    manager_case("manager_plate_r0p5");
}

/* -------------------------------------------------------------------------- */
/* avg_<mesh>_<wf>                                                            */
/* -------------------------------------------------------------------------- */

/// The probe's `eigen_of(x, dim)` (column-major `dim × dim`), same operand
/// order.
fn eigen_of(x: &[Real], dim: usize) -> Vec<Real> {
    let mut g = vec![0.; dim * dim];
    for i in 0..dim {
        for j in 0..dim {
            #[allow(clippy::cast_precision_loss)] // small loop indices
            let (fi, fj) = (i as Real, j as Real);
            let mut v = 0.1 * (fi + 1.) - 0.05 * fj;
            for k in 0..dim {
                #[allow(clippy::cast_precision_loss)]
                let fk = k as Real;
                v += (0.3 + 0.1 * fi - 0.07 * fj + 0.02 * fk) * x[k] * x[(k + i + j) % dim];
            }
            g[i + j * dim] = v;
        }
    }
    g
}

/// Run `avg_<mesh>_<wf>` with the given options; returns the model.
fn avg_model(f: &Fixture, options: NonLocalOptions) -> SolidMechanicsModel {
    let p = &f.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_u64().unwrap()).unwrap();
    let two_laws = p["two_laws"].as_bool().unwrap();
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    read_mesh(&mut mesh, &f.input_path("mesh").unwrap(), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.physics.non_local_options = options;
    let registered: akantu_core::Result<()> = akantu_core::with_dim!(dim, D => {
        let ctor: fn(&akantu_model::constitutive_law::LawSpec, Option<&Mesh>)
            -> akantu_core::Result<TestNonLocalMaterial<D>> = TestNonLocalMaterial::<D>::test_material_damage;
        model.physics.factory.replace(
            "probe_avg_material",
            std::sync::Arc::new(move |spec, services| {
                Ok(Box::new(ctor(spec, Some(services.mesh()))?) as Box<dyn Material>)
            }),
        );
    });
    registered.unwrap();
    if two_laws {
        model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    }
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    // eigen_grad_u = eigen_of(x) at every point of every law
    let mut coords = Vec::new();
    {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let fe = model.physics.base.fems.get("").unwrap();
        for m in 0..model.nb_materials() {
            let law = model.material(m).unwrap();
            for ty in law.base().filter_types(NG) {
                let filter = law
                    .element_filter()
                    .get(ty, NG)
                    .unwrap()
                    .as_slice()
                    .to_vec();
                if filter.is_empty() {
                    continue;
                }
                let mut xq = akantu_core::Array::new(dim);
                fe.integration_points_coordinates(&mesh, &mut xq, ty, NG, Some(&filter))
                    .unwrap();
                coords.push((m, ty, xq));
            }
        }
    }
    for (m, ty, xq) in coords {
        let law = model.material_mut(m).unwrap();
        let mut eg = law.base_mut().eigen_grad_u.get_mut(ty, NG).unwrap();
        for (k, g) in eg.as_mut_slice().chunks_exact_mut(dim * dim).enumerate() {
            g.copy_from_slice(&eigen_of(xq.tuple(k), dim));
        }
    }
    model
}

fn set_probe_damage(model: &mut SolidMechanicsModel) {
    for m in 0..model.nb_materials() {
        let law = model.material_mut(m).unwrap();
        let types = law.base().filter_types(NG);
        law.for_each_internal_mut(&mut |n, fr| {
            if n != "damage" {
                return Ok(());
            }
            if let akantu_model::constitutive_law::InternalFieldMut::Real(fr) = fr {
                for &ty in &types {
                    for (k, v) in fr.get_mut(ty, NG)?.as_mut_slice().iter_mut().enumerate() {
                        #[allow(clippy::cast_precision_loss)] // small index
                        let r = (k % 5) as Real;
                        *v = if k % 7 == 3 { 0.95 } else { 0.1 * r };
                    }
                }
            }
            Ok(())
        })
        .unwrap();
    }
}

/// Every recorded quantity of an `avg_*` case for the run `model` (calls 0
/// and 1); returns the number of compared values.
fn check_avg(f: &Fixture, model: &mut SolidMechanicsModel) {
    let laws: Vec<String> = (0..model.nb_materials())
        .map(|m| model.material(m).unwrap().name().to_owned())
        .collect();
    for call in 0..2 {
        if call == 1 {
            set_probe_damage(model);
        }
        model.assemble_internal_forces().unwrap();
        let pre = format!("call{call}/");
        let manager = model.non_local_manager().unwrap();
        for l in &laws {
            check_pairs(f, &format!("{pre}{l}/"), manager.neighborhood(l).unwrap());
        }
        check_blocks(f, &format!("{pre}volumes"), manager, manager.volumes());
        for m in 0..model.nb_materials() {
            let law = model.material(m).unwrap();
            let name = law.name().to_owned();
            let mut nl = None;
            law.for_each_internal(&mut |n, fr| {
                if n == "grad_u non local"
                    && let InternalFieldRef::Real(fr) = fr
                {
                    nl = Some(fr.clone());
                }
            });
            let nl = nl.unwrap();
            for ty in law.base().filter_types(NG) {
                if law.element_filter().get(ty, NG).unwrap().is_empty() {
                    continue;
                }
                let q = format!("{pre}{name}/grad_u_non_local/{}", ty.name());
                assert_bitwise(
                    &q,
                    nl.get(ty, NG).unwrap().as_slice(),
                    &f.load_f64(&q).unwrap().data,
                );
            }
        }
    }
    let manager = model.non_local_manager().unwrap();
    check_blocks(f, "jacobians", manager, manager.jacobians());
}

fn avg_case(case: &str, identity: PointIdentity) {
    let f = open(case);
    let options = NonLocalOptions {
        identity,
        ..NonLocalOptions::default()
    };
    let mut model = avg_model(&f, options);
    check_avg(&f, &mut model);
    eprintln!("{case}: bitwise ({identity:?})");
}

#[test]
fn avg_plate_base_wf() {
    avg_case("avg_plate_base_wf", PointIdentity::Complete);
}

#[test]
fn avg_plate_remove_wf() {
    avg_case("avg_plate_remove_wf", PointIdentity::Complete);
}

#[test]
fn avg_plate_damage_wf() {
    avg_case("avg_plate_damage_wf", PointIdentity::Complete);
}

#[test]
fn avg_cube_hex8_base_wf() {
    avg_case("avg_cube_hex8_base_wf", PointIdentity::Complete);
}

#[test]
fn avg_cube_hex8_remove_wf() {
    avg_case("avg_cube_hex8_remove_wf", PointIdentity::Complete);
}

#[test]
fn avg_cube_hex8_damage_wf() {
    avg_case("avg_cube_hex8_damage_wf", PointIdentity::Complete);
}

/// UD-037 compat: the mixed triangle/quadrangle mesh with upstream's
/// global-number identity reproduces the oracle bit for bit.
#[test]
fn avg_mixed_ud037_upstream_identity() {
    for wf in ["base_wf", "remove_wf", "damage_wf"] {
        avg_case(&format!("avg_mixed_{wf}"), PointIdentity::UpstreamGlobalNum);
    }
}

/// UD-037 corrected default on the mixed mesh: every cross-type pair is
/// stored exactly once (upstream: the half-list test on the per-type
/// `global_num` stores the cross-type pairs whose points share a
/// `global_num` twice, each copy with a zero second weight; it never drops
/// a pair), so the pair list differs from the oracle's
/// by those duplicates; the averages of a constant field stay exact and the
/// measured difference to the oracle (summation order only) is printed.
#[test]
fn avg_mixed_ud037_corrected_default() {
    let f = open("avg_mixed_base_wf");
    let mut model = avg_model(&f, NonLocalOptions::default());
    model.assemble_internal_forces().unwrap();
    let manager = model.non_local_manager().unwrap();
    let nh = manager.neighborhood("body").unwrap();
    // each unordered pair once, every point paired with itself
    let mut seen = std::collections::BTreeSet::new();
    for (q1, q2) in nh.pairs(NG) {
        let (a, b) = (q1.key().min(q2.key()), q1.key().max(q2.key()));
        assert!(seen.insert((a, b)), "pair {q1} {q2} stored twice");
    }
    let upstream = f.load_i64("call0/body/pairs_not_ghost").unwrap();
    eprintln!(
        "UD-037 mixed mesh: {} pairs (corrected) vs {} (upstream)",
        nh.pair_list(NG).len(),
        upstream.shape[0]
    );
    // the averaged gradient vs the oracle's (measured, not asserted)
    let law = model.material(0).unwrap();
    let mut max_rel: Real = 0.;
    law.for_each_internal(&mut |n, fr| {
        if n == "grad_u non local"
            && let InternalFieldRef::Real(fr) = fr
        {
            for ty in [ElementType::Triangle3, ElementType::Quadrangle4] {
                let e = f
                    .load_f64(&format!("call0/body/grad_u_non_local/{}", ty.name()))
                    .unwrap();
                for (g, x) in fr.get(ty, NG).unwrap().as_slice().iter().zip(&e.data) {
                    max_rel = nan_max(
                        max_rel,
                        (g - x).abs() / nan_max(x.abs(), Real::MIN_POSITIVE),
                    );
                }
            }
        }
    });
    eprintln!(
        "UD-037 mixed mesh: max relative difference of grad_u non local to the oracle {max_rel:e}"
    );
}

/* -------------------------------------------------------------------------- */
/* law_*                                                                      */
/* -------------------------------------------------------------------------- */

fn law_case(case: &str) {
    let f = open(case);
    let p = &f.manifest().parameters;
    let local = p["local_variable"].as_str().unwrap().to_owned();
    let non_local = p["non_local_variable"].as_str().unwrap().to_owned();
    let lambdas: Vec<Real> = p["lambda"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut mesh, &f.input_path("mesh").unwrap(), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.physics.law_options = Some(LawOptions::upstream());
    // upstream's MaterialMarigoNonLocal, registered by the probe under this keyword
    model.physics.factory.replace(
        "marigo_non_local_class",
        std::sync::Arc::new(|spec, services| {
            Ok(Box::new(
                akantu_solid::materials::non_local::MaterialMarigoNonLocal::<2>::new(
                    spec,
                    Some(services.mesh()),
                )?,
            ) as Box<dyn Material>)
        }),
    );
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    assert!(model.is_non_local());
    let nodes = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let ty = ElementType::Quadrangle4;
    let mut compared = 0usize;
    for (s, lam) in lambdas.iter().enumerate() {
        let l = 1e-4 * lam;
        {
            let mut u = model.displacement_mut().unwrap();
            for (n, uu) in u
                .as_mut_slice()
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (x, y) = (nodes[2 * n], nodes[2 * n + 1]);
                uu[0] = l * (0.6 * x + 0.3 * y * y);
                uu[1] = l * (0.2 * x * y - 0.1 * y);
            }
        }
        model
            .physics
            .before_solve_step(&mut model.solver.dof_manager)
            .unwrap();
        model.assemble_internal_forces().unwrap();
        model
            .physics
            .after_solve_step(&mut model.solver.dof_manager, true)
            .unwrap();
        let pre = format!("step{s}/");
        let law = model.material(0).unwrap();
        let mut fields = std::collections::BTreeMap::new();
        law.for_each_internal(&mut |n, fr| {
            if let InternalFieldRef::Real(fr) = fr {
                fields.insert(n.to_owned(), fr.get(ty, NG).unwrap().as_slice().to_vec());
            }
        });
        let mut cmp = |q: &str, field: &str| {
            let e = f.load_f64(&format!("{pre}{q}")).unwrap();
            let got = fields
                .get(field)
                .unwrap_or_else(|| panic!("internal {field}"));
            assert_bitwise(&format!("{pre}{q}"), got, &e.data);
            compared += e.data.len();
        };
        cmp("damage", "damage");
        cmp("stress", "stress");
        cmp("local_variable", &local);
        cmp("non_local_variable", &non_local);
        cmp("grad_u", "grad_u");
        if f.quantity(&format!("{pre}inelastic_strain")).is_ok() {
            cmp("inelastic_strain", "inelastic_strain");
        }
        let fi = model.internal_force().unwrap().as_slice().to_vec();
        let e = f.load_f64(&format!("{pre}internal_force")).unwrap();
        assert_bitwise(&format!("{pre}internal_force"), &fi, &e.data);
        compared += e.data.len();
    }
    eprintln!(
        "{case}: {compared} values bitwise over {} steps",
        lambdas.len()
    );
}

#[test]
fn law_mazars_non_local() {
    law_case("law_mazars_non_local");
}

#[test]
fn law_mazars_non_local_average_on_damage() {
    law_case("law_mazars_non_local_average_on_damage");
}

#[test]
fn law_plastic_mazars_non_local() {
    law_case("law_plastic_mazars_non_local");
}

#[test]
fn law_marigo_non_local_class() {
    law_case("law_marigo_non_local_class");
}

/// NL-007: upstream's header-only
/// `RemoveDamagedWithDamageRateWeightFunction::operator()` compiled with
/// the oracle's flags (`non_local_probe.cc` version 2, case
/// `weight_rate_sweep`) over ten radii, 605 distances each (0 … 1.5 R in
/// steps of R/200, perturbed values, R and its neighbours) and four damage
/// rates (below, at and just below the default `damage_limit` 1), plus the
/// same-point weight: the port's weight bit for bit, including the clamp
/// `max(0, 1 − pow(r²/R², 1))` beyond the radius.
#[test]
fn weight_rate_sweep_matches_the_oracle() {
    use akantu_model::non_local::{PointRef, WeightFunction, WeightInternals};
    use akantu_solid::materials::non_local::RemoveDamagedWithDamageRateWeightFunction;
    let f = open("weight_rate_sweep");
    let r = f.load_f64("r").unwrap().data;
    let radius = f.load_f64("radius").unwrap().data;
    let want = f.load_f64("weight").unwrap().data;
    let same = f.load_f64("weight_same_point").unwrap().data;
    let rates = f.load_f64("damage_rate").unwrap().data;
    assert_eq!(rates, [0., 0.5, 1., 1. - 1e-12]);
    assert_eq!(want.len(), r.len() * rates.len());
    let mut wf = RemoveDamagedWithDamageRateWeightFunction::default();
    assert_eq!(wf.damage_limit, 1.);
    let mut internals = WeightInternals::new();
    internals.insert("damage-rate".into(), rates.clone());
    wf.update_internals(&internals).unwrap();
    let q1 = QuadPoint::new(ElementType::Triangle3, NG, 1000, 0, 1);
    let q2s: Vec<QuadPoint> = (0..rates.len())
        .map(|g| QuadPoint::new(ElementType::Triangle3, NG, Idx::try_from(g).unwrap(), 0, 1))
        .collect();
    let mut got = Vec::with_capacity(want.len());
    let (mut beyond, mut clamped) = (0, 0);
    for (k, (&rk, &rad)) in r.iter().zip(&radius).enumerate() {
        if wf.radius().to_bits() != rad.to_bits() {
            wf.set_radius(rad);
        }
        for (g, q2) in q2s.iter().enumerate() {
            let w = wf.weight(
                rk,
                PointRef {
                    point: &q1,
                    slot: 0,
                },
                PointRef { point: q2, slot: g },
            );
            if rates[g] < 1. && rk > rad {
                beyond += 1;
                clamped += usize::from(want[k * rates.len() + g] == 0.);
            }
            got.push(w);
        }
    }
    assert_bitwise("weight", &got, &want);
    // decisive for pow(alpha, 2.) vs alpha * alpha (D-030: black box): the
    // probe's non-foldable libm pow(alpha, 2.) differs from alpha * alpha at
    // these alphas, and upstream's compiled weights there are alpha * alpha
    let n_diff = f.manifest().parameters["diagnostic/libm_pow2_ne_mul"]
        .as_i64()
        .unwrap();
    let alphas = f.load_f64("diagnostic/pow2_ne_mul_alpha").unwrap().data;
    assert!(n_diff > 0 && alphas.len() == usize::try_from(n_diff).unwrap());
    let mut decisive = 0;
    for (k, (&rk, &rad)) in r.iter().zip(&radius).enumerate() {
        let alpha = (1. - rk * rk / (rad * rad)).max(0.);
        if alphas.iter().any(|a| a.to_bits() == alpha.to_bits()) {
            assert_eq!(want[k * rates.len()].to_bits(), (alpha * alpha).to_bits());
            decisive += 1;
        }
    }
    assert!(
        decisive > 0,
        "the sweep holds points where pow(a, 2.) != a * a"
    );
    let p = PointRef {
        point: &q1,
        slot: 0,
    };
    assert_bitwise("weight_same_point", &[wf.weight(0.25, p, p)], &same);
    // the sweep reaches the clamp: every point beyond the radius weighs 0
    assert!(beyond > 1000, "{beyond} points beyond the radius");
    assert_eq!(clamped, beyond, "the oracle clamps beyond the radius");
    eprintln!(
        "weight_rate_sweep: {} weights bitwise ({beyond} beyond the radius, clamped to 0; \
         {decisive} points where libm pow(a, 2.) != a * a)",
        want.len() + 1
    );
}
