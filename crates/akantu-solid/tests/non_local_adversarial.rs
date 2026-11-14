// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential replays of the adversarial
// cases recorded by oracle/probes/non_local/adversarial/adversarial_probe.cc
// (adversarial cases, the UD-557 damage bands and the UD-558 two-law
// references; upstream MyModel of test/test_model/test_common/
// test_non_local_toolbox/my_model.hh, the averaging test law of
// test_material_damage.{hh,cc}, the non-local Mazars and Marigo laws of
// src/model/solid_mechanics/materials/material_damage/).

//! Bitwise differential tests against `fixtures/model/non_local/<case>`
//! for the adversarial cases:
//!
//! * `mgr_plate_*`, `avg_line_*`, `avg_mixed3d_*`, `avg_plate_two_nh`,
//!   `avg_plate_rate3`: the default options (UD-037 compat identity on the
//!   mixed meshes), every quantity bitwise;
//! * `avg_plate_dnear1_*`, `avg_plate_dbands_*` (UD-557): the x86_64 compat
//!   arithmetic of `damage_wf` replays the oracle bitwise, its infinite
//!   weights and volumes and NaN averages included (default NaNs compared
//!   under D-046); the corrected default is finite and NaN-free there and
//!   equal to the oracle wherever upstream's conversion is defined;
//! * `law_ud558_*` (UD-558): the compat registration reproduces the
//!   oracle's two-law defect bitwise; the corrected default gives each law
//!   the averages of its own variable, i.e. exactly the oracle's single-
//!   variable references `law_ud558_ref_ehat` (law 1) and
//!   `law_ud558_ref_damage` (law 2);
//! * `law_ud561_mixed` (UD-561): the compat error is the oracle's message.
//!
//! Every comparison is exact: identical bits, or two default quiet NaNs
//! differing only in sign (decision D-046, `akantu_validation::exact`).
//! `avg_plate_rate0` has no fixture: the oracle segfaults (UD-560).

mod non_local_common;

use std::collections::BTreeMap;

use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ElementDataSelector, InternalFieldMut, InternalFieldRef};
use akantu_model::non_local::{
    NoGhosts, NonLocalManager, NonLocalManagerCallback, NonLocalNeighborhood, NonLocalOptions,
    PointIdentity, QuadPoint, VariableRegistration, WeightArithmetic,
};
use akantu_parser::Parser;
use akantu_solid::material::LawOptions;
use akantu_solid::{Material, SolidMechanicsModel};
use akantu_solver::SolverCallback;
use akantu_validation::Fixture;
use akantu_validation::exact::exact_eq;
use non_local_common::TestNonLocalMaterial;

const NG: GhostType = GhostType::NotGhost;

fn open(case: &str) -> Fixture {
    let f = Fixture::open_by_id(&format!("model/non_local/{case}")).unwrap();
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

/// Comparison tally: every difference is recorded with its first index.
#[derive(Default)]
struct Tally {
    equal: usize,
    diffs: Vec<String>,
}

impl Tally {
    fn assert_clean(&self, tag: &str) {
        assert!(self.equal > 0, "{tag}: nothing compared");
        assert!(
            self.diffs.is_empty(),
            "{tag}: {} of {} quantities differ:\n{}",
            self.diffs.len(),
            self.diffs.len() + self.equal,
            self.diffs.join("\n")
        );
    }
}

fn fmt_bits(x: Real) -> String {
    format!("{x:e} (0x{:016x})", x.to_bits())
}

fn cmp_f(t: &mut Tally, case: &str, what: &str, got: &[Real], exp: &[Real]) {
    if got.len() != exp.len() {
        t.diffs.push(format!(
            "{case} {what}: length port {} oracle {}",
            got.len(),
            exp.len()
        ));
        return;
    }
    let bad: Vec<usize> = (0..got.len())
        .filter(|&k| !exact_eq(exp[k], got[k]))
        .collect();
    if bad.is_empty() {
        t.equal += 1;
    } else {
        let k = bad[0];
        t.diffs.push(format!(
            "{case} {what}: {}/{} differ, first {k}: port {} oracle {}",
            bad.len(),
            got.len(),
            fmt_bits(got[k]),
            fmt_bits(exp[k])
        ));
    }
}

fn cmp_i(t: &mut Tally, case: &str, what: &str, got: &[i64], exp: &[i64]) {
    if got == exp {
        t.equal += 1;
    } else {
        let k = got
            .iter()
            .zip(exp)
            .position(|(a, b)| a != b)
            .unwrap_or(got.len().min(exp.len()));
        t.diffs.push(format!(
            "{case} {what}: pairs port {} oracle {} (first differing entry {})",
            got.len() / 8,
            exp.len() / 8,
            k / 8
        ));
    }
}

fn pairs_rows(f: &Fixture, nh: &NonLocalNeighborhood) -> Vec<i64> {
    let names = f.manifest().parameters["type_names"].as_array().unwrap();
    nh.pairs(NG)
        .flat_map(|(q1, q2)| {
            let row = |q: &QuadPoint| {
                let code = names
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
        .collect()
}

fn check_pairs(t: &mut Tally, case: &str, f: &Fixture, prefix: &str, nh: &NonLocalNeighborhood) {
    let p = f.load_i64(&format!("{prefix}pairs_not_ghost")).unwrap();
    cmp_i(
        t,
        case,
        &format!("{prefix}pairs"),
        &pairs_rows(f, nh),
        &p.data,
    );
    let w = f.load_f64(&format!("{prefix}weights_not_ghost")).unwrap();
    let wg: Vec<Real> = nh.pair_weights(NG).iter().flat_map(|w| *w).collect();
    cmp_f(t, case, &format!("{prefix}weights"), &wg, &w.data);
}

fn check_blocks(
    t: &mut Tally,
    case: &str,
    f: &Fixture,
    name: &str,
    manager: &NonLocalManager,
    values: &[Real],
) {
    for (ty, g, off, len) in manager.layout().blocks() {
        if g != NG || len == 0 {
            continue;
        }
        let q = format!("{name}/{}", ty.name());
        match f.load_f64(&q) {
            Ok(e) => cmp_f(t, case, &q, &values[off..off + len], &e.data),
            Err(_) => {
                t.diffs.push(format!("{case} {q}: missing in the fixture"));
            }
        }
    }
}

/* ------------------------------------------------------------------------ */
/* mgr_plate_*: upstream MyModel                                             */
/* ------------------------------------------------------------------------ */
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

fn eigen_of(x: &[Real], dim: usize) -> Vec<Real> {
    let mut g = vec![0.; dim * dim];
    for i in 0..dim {
        for j in 0..dim {
            #[allow(clippy::cast_precision_loss)]
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

/// The probe's `damage_of(pattern, k, c)`.
fn damage_of(pattern: &str, k: usize, c: usize) -> Real {
    #[allow(clippy::cast_precision_loss)]
    match pattern {
        "probe" => {
            if k % 7 == 3 {
                0.95
            } else {
                0.1 * (k % 5) as Real
            }
        }
        "shift" => {
            let m = k + c;
            if m % 7 == 3 {
                0.95
            } else {
                0.1 * (m % 5) as Real
            }
        }
        "bands" => {
            // the probe's UD-557 band values (adversarial_probe.cc)
            let d = [
                0.,
                0.3,
                0.99,
                1. - 1e-4,
                Real::from_bits(0x3fef_ffb4_f9d6_acc3),
                Real::from_bits(0x3fef_ffb4_f9d6_acc4),
                1. - 3e-5,
                1. - 1e-6,
                1. - 1e-8,
                1. - 1e-10,
                1. - 1e-11,
                1. - 1e-12,
                1.,
                -0.5,
            ];
            d[k % d.len()]
        }
        "near1" => {
            let d = [
                0.,
                0.5,
                0.99,
                1. - 1e-4,
                1. - 3e-5,
                1. - 1e-6,
                1. - 1e-9,
                1. - 1e-12,
                1.,
            ];
            d[k % d.len()]
        }
        _ => panic!("pattern {pattern}"),
    }
}

fn avg_model(f: &Fixture, options: NonLocalOptions) -> akantu_core::Result<SolidMechanicsModel> {
    let p = &f.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_u64().unwrap()).unwrap();
    let by_names = p["by_physical_names"].as_bool().unwrap();
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    read_mesh(&mut mesh, &f.input_path("mesh").unwrap(), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )?;
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
    registered?;
    if by_names {
        model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    }
    model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
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
    Ok(model)
}

fn set_damage(model: &mut SolidMechanicsModel, pattern: &str, call: usize) {
    for m in 0..model.nb_materials() {
        let law = model.material_mut(m).unwrap();
        let types = law.base().filter_types(NG);
        law.for_each_internal_mut(&mut |n, fr| {
            if n != "damage" {
                return Ok(());
            }
            if let InternalFieldMut::Real(fr) = fr {
                for &ty in &types {
                    for (k, v) in fr.get_mut(ty, NG)?.as_mut_slice().iter_mut().enumerate() {
                        *v = damage_of(pattern, k, call);
                    }
                }
            }
            Ok(())
        })
        .unwrap();
    }
}

fn manager_case(case: &str) -> Tally {
    let mut t = Tally::default();
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
    check_pairs(
        &mut t,
        case,
        &f,
        "",
        manager.neighborhood("test_region").unwrap(),
    );
    check_blocks(&mut t, case, &f, "volumes", &manager, manager.volumes());
    check_blocks(&mut t, case, &f, "jacobians", &manager, manager.jacobians());
    t
}

/// Per call: pair weights of every law, volumes and every law's averages.
#[derive(Default)]
struct AvgRun {
    calls: Vec<BTreeMap<String, Vec<Real>>>,
}

fn avg_case(case: &str, options: NonLocalOptions) -> (Tally, AvgRun) {
    let mut t = Tally::default();
    let mut run = AvgRun::default();
    let f = open(case);
    let tag = format!("{case}[{options:?}]");
    let p = f.manifest().parameters.clone();
    let calls = usize::try_from(p["calls"].as_u64().unwrap()).unwrap();
    let pattern = p["damage_pattern"].as_str().unwrap().to_owned();
    let mut model = avg_model(&f, options).unwrap();
    let laws: Vec<String> = (0..model.nb_materials())
        .map(|m| model.material(m).unwrap().name().to_owned())
        .collect();
    for call in 0..calls {
        if call >= 1 {
            set_damage(&mut model, &pattern, call);
        }
        model.assemble_internal_forces().unwrap();
        let mut record = BTreeMap::new();
        let pre = format!("call{call}/");
        let manager = model.non_local_manager().unwrap();
        for l in &laws {
            let nh = manager.neighborhood(l).unwrap();
            check_pairs(&mut t, &tag, &f, &format!("{pre}{l}/"), nh);
            record.insert(
                format!("{l}/weights"),
                nh.pair_weights(NG)
                    .iter()
                    .flat_map(|w| *w)
                    .collect::<Vec<_>>(),
            );
        }
        check_blocks(
            &mut t,
            &tag,
            &f,
            &format!("{pre}volumes"),
            manager,
            manager.volumes(),
        );
        record.insert("volumes".into(), manager.volumes().to_vec());
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
                let got_g = nl.get(ty, NG).unwrap();
                let got = got_g.as_slice();
                cmp_f(&mut t, &tag, &q, got, &f.load_f64(&q).unwrap().data);
                record.insert(
                    format!("{name}/grad_u_non_local/{}", ty.name()),
                    got.to_vec(),
                );
            }
        }
        run.calls.push(record);
    }
    let manager = model.non_local_manager().unwrap();
    check_blocks(&mut t, &tag, &f, "jacobians", manager, manager.jacobians());
    (t, run)
}

/// Every law field per step (`step<s>/<law>/<field>`) and the internal force.
type LawRecord = BTreeMap<String, Vec<Real>>;

const LAW_FIELDS: [&str; 6] = [
    "damage",
    "stress",
    "Ehat",
    "non_local_variable",
    "Y",
    "Y_non_local",
];

fn law_pair_case(case: &str, options: NonLocalOptions) -> (Tally, akantu_core::Result<LawRecord>) {
    let mut t = Tally::default();
    let f = open(case);
    let tag = format!("{case}[{options:?}]");
    let p = f.manifest().parameters.clone();
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
    model.physics.non_local_options = options;
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
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    if let Err(e) = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)) {
        return (t, Err(e));
    }
    let nodes = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let ty = ElementType::Quadrangle4;
    let mut record = LawRecord::new();
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
        if let Err(e) = model.assemble_internal_forces() {
            return (t, Err(e));
        }
        model
            .physics
            .after_solve_step(&mut model.solver.dof_manager, true)
            .unwrap();
        let pre = format!("step{s}/");
        for m in 0..model.nb_materials() {
            let law = model.material(m).unwrap();
            let name = law.name().to_owned();
            let mut fields = BTreeMap::new();
            law.for_each_internal(&mut |n, fr| {
                if let InternalFieldRef::Real(fr) = fr
                    && let Ok(a) = fr.get(ty, NG)
                {
                    fields.insert(n.replace(' ', "_"), a.as_slice().to_vec());
                }
            });
            for q in LAW_FIELDS {
                let key = format!("{pre}{name}/{q}");
                if let Some(g) = fields.remove(q) {
                    if p.get("exception").is_none()
                        && let Ok(e) = f.load_f64(&key)
                    {
                        cmp_f(&mut t, &tag, &key, &g, &e.data);
                    }
                    record.insert(key, g);
                }
            }
        }
        let fi = model.internal_force().unwrap().as_slice().to_vec();
        if p.get("exception").is_none() {
            let e = f.load_f64(&format!("{pre}internal_force")).unwrap();
            cmp_f(&mut t, &tag, &format!("{pre}internal_force"), &fi, &e.data);
        }
        record.insert(format!("{pre}internal_force"), fi);
    }
    (t, Ok(record))
}

fn compat() -> NonLocalOptions {
    NonLocalOptions::upstream()
}

/* ------------------------------------------------------------------------ */
/* tests                                                                     */
/* ------------------------------------------------------------------------ */

#[test]
fn adversarial_manager_cases_bitwise() {
    for c in ["mgr_plate_r10", "mgr_plate_r0p01", "mgr_plate_r0p25"] {
        manager_case(c).assert_clean(c);
    }
}

#[test]
fn adversarial_averaging_cases_bitwise() {
    // single element type: the default options are upstream's arithmetic
    for c in [
        "avg_line_r0p1",
        "avg_line_exact_r0p5",
        "avg_plate_two_nh",
        "avg_plate_rate3",
    ] {
        let (t, _) = avg_case(c, NonLocalOptions::default());
        t.assert_clean(c);
    }
    // mixed element types: UD-037 compat identity (the corrected default
    // differs at rounding level by the duplicate pairs)
    for c in ["avg_line_mixed_remove_wf", "avg_mixed3d_damage_wf"] {
        let options = NonLocalOptions {
            identity: PointIdentity::UpstreamGlobalNum,
            ..NonLocalOptions::default()
        };
        let (t, _) = avg_case(c, options);
        t.assert_clean(c);
        let (t, _) = avg_case(c, NonLocalOptions::default());
        assert!(
            !t.diffs.is_empty(),
            "{c}: UD-037 default identity must differ (duplicates)"
        );
    }
}

/// UD-557 compat: `damage_wf` with the x86_64 conversion replays the
/// oracle bitwise in every damage band, including the infinite weights
/// and volumes and the NaN averages the oracle produces (D-046 NaN rule).
#[test]
fn damage_wf_x86_64_compat_replays_the_oracle_in_every_band() {
    for c in [
        "avg_plate_dnear1_r0p5",
        "avg_plate_dnear1_r5",
        "avg_plate_dbands_r0p5",
        "avg_plate_dbands_r100",
    ] {
        let options = NonLocalOptions {
            weight_arithmetic: WeightArithmetic::UpstreamX86_64,
            ..NonLocalOptions::default()
        };
        let (t, run) = avg_case(c, options);
        t.assert_clean(c);
        // the oracle's INT_MIN outcomes are really there (call 1): infinite
        // volumes and NaN averages, or (R = 100: alpha = 1 − r²/R⁴ is within
        // ~1e-8 of 1, so alpha^(−2³¹) stays finite) volumes of order e^20
        // instead of O(1); NaN-propagating maximum
        let vmax = run.calls[1]["volumes"]
            .iter()
            .fold(0., |m, v| akantu_validation::reduce::nan_max(m, v.abs()));
        assert!(
            vmax.is_nan() || vmax > 1e3,
            "{c}: expected the oracle's INT_MIN volumes, max {vmax:e}"
        );
        // the full compat set (NonLocalOptions::upstream) gives the same
        // result on these single-type meshes
        let (t, _) = avg_case(c, compat());
        t.assert_clean(c);
    }
}

/// UD-557 corrected default: finite weights, volumes and averages in every
/// band (no NaN, no infinity), and the oracle's values wherever upstream's
/// conversion is defined (call 0: undamaged; the bands below the Int limit
/// are checked weight by weight in `non_local_materials.rs`).
#[test]
fn damage_wf_corrected_default_is_finite_in_every_band() {
    for c in [
        "avg_plate_dnear1_r0p5",
        "avg_plate_dnear1_r5",
        "avg_plate_dbands_r0p5",
        "avg_plate_dbands_r100",
    ] {
        let (t, run) = avg_case(c, NonLocalOptions::default());
        for (call, record) in run.calls.iter().enumerate() {
            for (k, v) in record {
                if let Some(i) = v.iter().position(|x| !x.is_finite()) {
                    panic!(
                        "{c} call {call} {k}[{i}] = {} in the corrected default",
                        v[i]
                    );
                }
                if k.ends_with("/weights") {
                    assert!(
                        v.iter().all(|w| *w >= 0.),
                        "{c} call {call} {k}: negative weight"
                    );
                }
            }
        }
        // call 0 (damage 0 everywhere) is bitwise the oracle; call 1 differs
        // exactly where the oracle is non-finite or uses INT_MIN
        assert!(
            t.diffs.iter().all(|d| d.contains("call1/")),
            "{c}: call 0 must replay bitwise in the default mode:\n{}",
            t.diffs.join("\n")
        );
        assert!(
            !t.diffs.is_empty(),
            "{c}: the corrected default must differ from the oracle"
        );
    }
}

/// UD-558 compat: two `mazars_non_local` laws averaging `Ehat` and
/// `damage` under one non-local name reproduce the oracle's defect bitwise
/// (law 1 receives law 2's averaged damage and never damages).
#[test]
fn ud558_compat_reproduces_the_oracle_two_law_defect() {
    let options = NonLocalOptions {
        registration: VariableRegistration::UpstreamShared,
        ..NonLocalOptions::default()
    };
    for (o, what) in [(options, "UpstreamShared"), (compat(), "upstream()")] {
        let (t, rec) = law_pair_case("law_ud558_two_mazars", o);
        t.assert_clean(&format!("law_ud558_two_mazars[{what}]"));
        let rec = rec.unwrap();
        let d1 = &rec["step13/mat_1/damage"];
        assert!(
            d1.iter().all(|d| *d == 0.),
            "compat: law 1 never damages (oracle)"
        );
    }
}

/// UD-558 corrected default: each law averages its declared variable. The
/// two-law model gives law 1 exactly the oracle's results of the split
/// with both laws averaging `Ehat` and law 2 those of the split with both
/// averaging `damage` (every field, every step, bitwise).
#[test]
fn ud558_default_each_law_averages_its_declared_variable() {
    let (_, rec) = law_pair_case("law_ud558_two_mazars", NonLocalOptions::default());
    let rec = rec.unwrap();
    let mut t = Tally::default();
    for (reference, law) in [
        ("law_ud558_ref_ehat", "mat_1"),
        ("law_ud558_ref_damage", "mat_2"),
    ] {
        let f = open(reference);
        let steps = f.manifest().parameters["lambda"].as_array().unwrap().len();
        for s in 0..steps {
            for q in ["damage", "stress", "Ehat", "non_local_variable"] {
                let key = format!("step{s}/{law}/{q}");
                cmp_f(
                    &mut t,
                    reference,
                    &key,
                    &rec[&key],
                    &f.load_f64(&key).unwrap().data,
                );
            }
        }
    }
    t.assert_clean("law_ud558_two_mazars[default] vs references");
    let d1 = &rec["step13/mat_1/damage"];
    assert!(d1.iter().any(|d| *d > 0.), "default: law 1 damages");
    // and the default differs from the oracle's defective run
    let (t, _) = law_pair_case("law_ud558_two_mazars", NonLocalOptions::default());
    assert!(!t.diffs.is_empty());
}

/// The references themselves replay bitwise in both modes (no conflicting
/// registration: the corrected and the upstream registration agree).
#[test]
fn ud558_references_replay_bitwise_in_both_modes() {
    for c in ["law_ud558_ref_ehat", "law_ud558_ref_damage"] {
        for o in [NonLocalOptions::default(), compat()] {
            let (t, rec) = law_pair_case(c, o);
            rec.unwrap();
            t.assert_clean(c);
        }
    }
}

/// UD-561: the oracle throws at the first assembly; the compat mode returns
/// the same message, the default runs.
#[test]
fn ud561_mixed_laws_compat_error_is_the_oracle_message() {
    let f = open("law_ud561_mixed");
    let expected = f.manifest().parameters["exception"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, rec) = law_pair_case("law_ud561_mixed", compat());
    let e = rec.expect_err("compat must refuse").to_string();
    let first = expected.lines().next().unwrap().trim();
    let tail = first.rsplit("] ").next().unwrap_or(first).trim();
    assert!(
        e.contains(tail) || first.contains(&e),
        "compat error {e:?} vs oracle {first:?}"
    );
    let (_, rec) = law_pair_case("law_ud561_mixed", NonLocalOptions::default());
    rec.unwrap();
}
