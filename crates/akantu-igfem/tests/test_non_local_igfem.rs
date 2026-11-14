// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/non_local_manager_igfem.{hh,cc}
// (NonLocalManagerIGFEM: init, computeAllNonLocalStresses, onElementsAdded /
// onElementsRemoved); no upstream test is registered for it. Built only with
// the feature `cgal` (GPL-3.0-or-later sphere intersector, decision D-042).

//! IGFEM-009: the non-local manager over both element kinds.
//!
//! No upstream test exists; the evidence is independent:
//!
//! * `both_kinds_in_one_neighborhood_equal_a_brute_force_average`: every
//!   regular and IGFEM integration point of an IGFEM mesh (four gel pockets
//!   on `fine_mesh.msh`) in one `base_wf` neighbourhood of radius 0.3, the
//!   local variable `f(x) = x + 2y²` at the points; the manager's averages
//!   (regular points weighted with the model FE engine's integration
//!   weights, IGFEM points with the IGFEM engine's) equal the brute-force
//!   `Σ_j W(d_ij) J_j f_j / Σ_j W(d_ij) J_j` over every point within the
//!   radius, computed in the test;
//! * `igfem_element_events_invalidate_the_neighborhoods`: the element-added
//!   hook — an IGFEM element event invalidates a manager whose layout has
//!   the IGFEM kind (averaging refused until rebuilt), not a regular-only one;
//! * `igfem_model_with_a_non_local_law`: an IGFEM model with
//!   `damage_iterative_non_local` (which averages `grad_u`, an internal the
//!   IGFEM law has too): `initFull`, a solve, an interface update (the
//!   manager is rebuilt) and another solve; the averaged `grad_u` of the
//!   non-local law equals a brute-force average of the regular points of
//!   its neighbourhood.

mod common;

use std::collections::BTreeMap;

use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result,
};
use akantu_fe::FeEngine;
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::engine::IgfemFeEngine;
use akantu_igfem::non_local::IgfemPointSource;
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};
use akantu_model::non_local::{
    FePointSource, NoGhosts, NonLocalManager, NonLocalManagerCallback, QuadPoint, base_weight,
};
use akantu_parser::Parser;
use common::model::*;

const NG: GhostType = GhostType::NotGhost;
const RADIUS: Real = 0.3;

fn f(x: &[Real]) -> Real {
    x[0] + 2.0 * x[1] * x[1]
}

/// The integration points of every block of one kind: `(type, coords
/// (n_el·nq tuples), jacobians, nq)`.
type KindPoints = Vec<(ElementType, Array<Real>, Vec<Real>, usize)>;

fn regular_points(mesh: &Mesh, fe: &FeEngine) -> KindPoints {
    let mut out = Vec::new();
    for ty in mesh
        .element_types(akantu_core::TypeFilter {
            spatial_dimension: Some(2),
            ghost_type: NG,
            kind: Some(ElementKind::Regular),
        })
        .collect::<Vec<_>>()
    {
        let nq = fe.nb_integration_points(ty, NG).unwrap();
        let mut c = Array::new(2);
        fe.integration_points_coordinates(mesh, &mut c, ty, NG, None)
            .unwrap();
        let j = fe.jacobians(ty, NG).unwrap().as_slice().to_vec();
        out.push((ty, c, j, nq));
    }
    out
}

fn igfem_points(mesh: &Mesh, engine: &IgfemFeEngine) -> KindPoints {
    let mut out = Vec::new();
    for ty in engine.element_types(mesh, NG).collect::<Vec<_>>() {
        if mesh.nb_element(ty, NG) == 0 {
            continue;
        }
        let nq = engine.nb_integration_points(ty).unwrap();
        let mut c = Array::new(2);
        engine
            .integration_points_coordinates(mesh, &mut c, ty, NG, None)
            .unwrap();
        let j = engine.jacobians(mesh, ty, NG).unwrap().as_slice().to_vec();
        out.push((ty, c, j, nq));
    }
    out
}

/// A callback that inserts the given points in one neighbourhood and
/// serves `f` at every point.
struct PointsCallback<'a> {
    kinds: &'a [&'a KindPoints],
    result: Option<ElementTypeMapArray<Real>>,
}

fn idx(v: usize) -> Idx {
    Idx::try_from(v).unwrap()
}

impl NonLocalManagerCallback for PointsCallback<'_> {
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> Result<()> {
        manager.register_neighborhood("nh", "test")?;
        manager.register_neighborhood_variable("nh", "f", "f non local", 1)?;
        if ghost != NG {
            return Ok(());
        }
        for pts in self.kinds {
            for (ty, coords, _, nq) in pts.iter() {
                for k in 0..coords.len() {
                    let (e, q) = (k / nq, k % nq);
                    let p = QuadPoint::new(*ty, NG, idx(e), idx(q), idx(*nq));
                    manager.insert_integration_point("nh", p, coords.tuple(k))?;
                }
            }
        }
        Ok(())
    }

    fn update_local_internal(
        &self,
        field: &str,
        ghost: GhostType,
        flat: &mut ElementTypeMapArray<Real>,
    ) -> Result<()> {
        assert_eq!(field, "f");
        if ghost != NG {
            return Ok(());
        }
        for pts in self.kinds {
            for (ty, coords, _, _) in pts.iter() {
                let a = flat.alloc(coords.len(), 1, *ty, NG, 0.0)?;
                for k in 0..coords.len() {
                    a.as_mut_slice()[k] = f(coords.tuple(k));
                }
            }
        }
        Ok(())
    }

    fn update_non_local_internal(
        &mut self,
        field: &str,
        ghost: GhostType,
        flat: &ElementTypeMapArray<Real>,
        _strict: bool,
    ) -> Result<()> {
        if field == "f non local" && ghost == NG {
            self.result = Some(flat.clone());
        }
        Ok(())
    }

    fn compute_non_local_contribution(&mut self, _ghost: GhostType) -> Result<()> {
        Ok(())
    }
}

/// The IGFEM model of TEST-201 after its first update (elastic laws).
fn pocket_model() -> akantu_igfem::SolidMechanicsModelIgfem {
    let dat = "material elastic [\n name = mat_2\n E = 1.\n nu = 0.25\n]\n\
               material elastic [\n name = mat_1\n E = 10.\n nu = 0.3\n]\n\
               material igfem_elastic [\n name = igfem\n name_sub_mat_1 = mat_1\n name_sub_mat_2 = mat_2\n]\n";
    let mesh = common::read_msh(2, "smm_igfem/fine_mesh.msh", "mesh");
    let mut model = model(mesh, dat);
    model.register_geometry_object(Vec::new(), "mat_1").unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    let s = |c: [Real; 2], r: Real| SkSphere::try_from_f64([c[0], c[1], 0.], r * r).unwrap();
    model
        .set_geometry_object(
            vec![
                s([0., 0.], 0.13),
                s([0.5, 0.5], 0.4),
                s([-0.75, -0.75], 0.12),
                s([0.625, -0.625], 0.25),
            ],
            "mat_1",
        )
        .unwrap();
    model.update("mat_1").unwrap();
    model
}

fn manager() -> NonLocalManager {
    let p = Parser::parse_str(
        &format!("non_local test base_wf [\n radius = {RADIUS}\n]\n"),
        "nl.dat",
    )
    .unwrap();
    let mut m = NonLocalManager::new("nl", 2);
    m.read_sections(&p.root, false);
    m
}

/// Brute force: `Σ_j W(d_ij) J_j f_j / Σ_j W(d_ij) J_j` over every point,
/// and the scale `Σ_j W J abs(f_j) / Σ_j W J` that bounds its rounding error
/// (an average near a sign change of `f` is small, its error is not).
fn brute_force_and_scale(all: &[(Vec<Real>, Real, Real)], x: &[Real]) -> (Real, Real) {
    let (mut num, mut abs_num, mut den) = (0.0, 0.0, 0.0);
    for (y, j, fy) in all {
        let d = ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2)).sqrt();
        if d <= RADIUS + akantu_core::math::DEFAULT_TOLERANCE {
            let w = base_weight(d, RADIUS, RADIUS * RADIUS) * j;
            num += w * fy;
            abs_num += w * fy.abs();
            den += w;
        }
    }
    (num / den, abs_num / den)
}

fn brute_force(all: &[(Vec<Real>, Real, Real)], x: &[Real]) -> Real {
    brute_force_and_scale(all, x).0
}

#[test]
fn both_kinds_in_one_neighborhood_equal_a_brute_force_average() {
    let model = pocket_model();
    let mesh_h = model.mesh().clone();
    let mesh = mesh_h.read().unwrap();
    let fe = model.smm.physics.base.fems.get("").unwrap();
    let reg = regular_points(&mesh, fe);
    let ig = igfem_points(&mesh, &model.ctx.engine().unwrap());
    let n_ig: usize = ig.iter().map(|(_, c, _, _)| c.len()).sum();
    let n_reg: usize = reg.iter().map(|(_, c, _, _)| c.len()).sum();
    assert!(n_ig > 0 && n_reg > 0);
    let mut m = manager();
    let mut cb = PointsCallback {
        kinds: &[&reg, &ig],
        result: None,
    };
    let src = IgfemPointSource {
        ctx: model.ctx.clone(),
    };
    m.initialize_with_sources(&mesh, &[&FePointSource(fe), &src], &mut cb, &mut NoGhosts)
        .unwrap();
    assert_eq!(m.neighborhood("nh").unwrap().points().len(), n_reg + n_ig);
    m.compute_all_non_local_contribution(&mut cb, &mut NoGhosts)
        .unwrap();
    let out = cb
        .result
        .take()
        .expect("the averaged variable was distributed");
    // every point: (coords, integration weight, f)
    let mut all = Vec::new();
    for pts in [&reg, &ig] {
        for (_, c, j, _) in pts.iter() {
            for (k, &jk) in j.iter().enumerate().take(c.len()) {
                all.push((c.tuple(k).to_vec(), jk, f(c.tuple(k))));
            }
        }
    }
    let mut max_rel: Real = 0.0;
    let mut checked = BTreeMap::new();
    for pts in [&reg, &ig] {
        for (ty, c, _, _) in pts.iter() {
            let got = out.get(*ty, NG).unwrap();
            for k in 0..c.len() {
                let (want, scale) = brute_force_and_scale(&all, c.tuple(k));
                let rel = (got.as_slice()[k] - want).abs() / scale.max(1e-300);
                max_rel = max_rel.max(rel);
                assert!(
                    rel <= 1e-12,
                    "{ty:?} point {k}: manager {} vs brute force {want}",
                    got.as_slice()[k]
                );
                *checked.entry(ty.kind()).or_insert(0) += 1;
            }
        }
    }
    eprintln!(
        "points checked {checked:?}; max difference relative to Σ W J abs(f) / Σ W J {max_rel:e}"
    );
    assert_eq!(checked[&ElementKind::Igfem], n_ig);
    // the IGFEM points matter: without them the average differs near the
    // interface
    let reg_only: Vec<_> = all[..n_reg].to_vec();
    let (_, c0, _, _) = &ig[0];
    let with = brute_force(&all, c0.tuple(0));
    let without = brute_force(&reg_only, c0.tuple(0));
    assert!(
        (with - without).abs() > 1e-6 * with.abs(),
        "{with} {without}"
    );
}

/// The element-added hook (upstream `onElementsAdded` of the IGFEM
/// manager): an IGFEM element event invalidates a layout with the IGFEM
/// kind; a regular-only layout ignores it.
#[test]
fn igfem_element_events_invalidate_the_neighborhoods() {
    let model = pocket_model();
    let mesh_h = model.mesh().clone();
    let mesh = mesh_h.read().unwrap();
    let fe = model.smm.physics.base.fems.get("").unwrap();
    let reg = regular_points(&mesh, fe);
    let ig = igfem_points(&mesh, &model.ctx.engine().unwrap());
    let event = MeshEvent::ElementsAdded {
        elements: vec![akantu_core::Element::try_new(ElementType::IgfemTriangle5, 0, NG).unwrap()],
        constitutive_law: None,
        origin: "test".into(),
        extension: None,
    };
    let src = IgfemPointSource {
        ctx: model.ctx.clone(),
    };
    // both kinds: invalidated, averaging refused
    let mut m = manager();
    let mut cb = PointsCallback {
        kinds: &[&reg, &ig],
        result: None,
    };
    m.initialize_with_sources(&mesh, &[&FePointSource(fe), &src], &mut cb, &mut NoGhosts)
        .unwrap();
    assert!(m.is_valid());
    m.on_mesh_event(&mesh, &event).unwrap();
    assert!(!m.is_valid());
    assert!(
        m.compute_all_non_local_contribution(&mut cb, &mut NoGhosts)
            .is_err()
    );
    m.initialize_with_sources(&mesh, &[&FePointSource(fe), &src], &mut cb, &mut NoGhosts)
        .unwrap();
    assert!(
        m.compute_all_non_local_contribution(&mut cb, &mut NoGhosts)
            .is_ok()
    );
    // regular only: an IGFEM event leaves it valid
    let mut m = manager();
    let mut cb = PointsCallback {
        kinds: &[&reg],
        result: None,
    };
    m.initialize(&mesh, fe, &mut cb, &mut NoGhosts).unwrap();
    m.on_mesh_event(&mesh, &event).unwrap();
    assert!(m.is_valid());
    // two sources of one kind are refused
    let mut m = manager();
    assert!(
        m.initialize_with_sources(&mesh, &[&src, &src], &mut cb, &mut NoGhosts)
            .is_err()
    );
}

/// An IGFEM model with a regular non-local law averaging `grad_u` (which
/// the IGFEM law also has): the IGFEM points are counted with the IGFEM
/// engine when the internals are flattened, the manager is rebuilt after
/// the interface update, and the non-local law's averaged gradient equals
/// a brute-force average over the regular points of its neighbourhood.
#[test]
fn igfem_model_with_a_non_local_law() {
    let dat = "material damage_iterative_non_local [\n name = mat_2\n rho = 1\n E = 1.\n nu = 0.25\n Sc = 100\n]\n\
               material elastic [\n name = mat_1\n E = 10.\n nu = 0.3\n]\n\
               material elastic [\n name = mat_3\n E = 1.\n nu = 0.25\n]\n\
               material igfem_elastic [\n name = igfem\n name_sub_mat_1 = mat_1\n name_sub_mat_2 = mat_3\n]\n\
               non_local mat_2 base_wf [\n radius = 0.3\n]\n";
    let mesh = common::read_msh(2, "smm_igfem/fine_mesh.msh", "mesh");
    let mut model = model(mesh, dat);
    akantu_extra_materials::register_extra_non_local_materials(&mut model.smm.physics.factory)
        .unwrap();
    model.register_geometry_object(Vec::new(), "mat_1").unwrap();
    geometry_selector(&mut model, 3);
    init_static(&mut model);
    assert!(model.smm.is_non_local());
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-10);
    model.smm.solve_step(None).unwrap();
    let s = |c: [Real; 2], r: Real| SkSphere::try_from_f64([c[0], c[1], 0.], r * r).unwrap();
    model
        .set_geometry_object(vec![s([0.5, 0.5], 0.4), s([0.625, -0.625], 0.25)], "mat_1")
        .unwrap();
    let pass = model.update("mat_1").unwrap();
    assert!(pass.new_elements > 0);
    assert!(
        model.smm.non_local_manager().unwrap().is_valid(),
        "rebuilt after the update"
    );
    model.smm.solve_step(None).unwrap();
    // the averaged gradient of the non-local law against brute force
    let mesh_h = model.mesh().clone();
    let mesh = mesh_h.read().unwrap();
    let fe = model.smm.physics.base.fems.get("").unwrap();
    let nh = model
        .smm
        .non_local_manager()
        .unwrap()
        .neighborhood("mat_2")
        .unwrap();
    let ty = ElementType::Triangle3;
    let filter: Vec<Idx> = model
        .smm
        .material(0)
        .unwrap()
        .element_filter()
        .get(ty, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(
        nh.points().len(),
        filter.len(),
        "one point per element of mat_2"
    );
    let mut coords = Array::new(2);
    fe.integration_points_coordinates(&mesh, &mut coords, ty, NG, None)
        .unwrap();
    let jac = fe.jacobians(ty, NG).unwrap().as_slice().to_vec();
    let grad_u = real_internal(&model, 0, "grad_u", ty);
    let grad_nl = real_internal(&model, 0, "grad_u non local", ty);
    let mut max_rel: Real = 0.0;
    for (k, &ek) in filter.iter().enumerate() {
        let x = coords.tuple(usize::try_from(ek).unwrap()).to_vec();
        for c in 0..4 {
            let (mut num, mut den) = (0.0, 0.0);
            for (j, &ej) in filter.iter().enumerate() {
                let e = usize::try_from(ej).unwrap();
                let y = coords.tuple(e);
                let d = ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2)).sqrt();
                if d <= 0.3 + akantu_core::math::DEFAULT_TOLERANCE {
                    let w = base_weight(d, 0.3, 0.09) * jac[e];
                    num += w * grad_u[4 * j + c];
                    den += w;
                }
            }
            let want = num / den;
            let got = grad_nl[4 * k + c];
            let scale = grad_u.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            let rel = (got - want).abs() / scale;
            max_rel = max_rel.max(rel);
            assert!(rel <= 1e-12, "element {ek} component {c}: {got} vs {want}");
        }
    }
    eprintln!(
        "non-local grad_u vs brute force: max difference relative to max abs(grad_u) {max_rel:e}"
    );
}
