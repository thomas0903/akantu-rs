// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). SMM-024 adversarial
// cases (adopted): the material
// stress-interpolation wrappers (upstream
// src/model/solid_mechanics/material.cc initElementalFieldInterpolation,
// interpolateStress, interpolateStressOnFacets) replayed against
// fixtures/model/v87/* (a probe on the pinned x86_64 oracle).

//! Cases (`fixtures/model/v87/<case>`):
//! - `interp_mixed_linear_2d`: triangle_3 + quadrangle_4 sharing segment_2
//!   facets; material 1 holds only the odd quadrangles (its triangle filter
//!   is empty), material 2 holds no element at all;
//! - `interp_mixed_quadratic_2d`: triangle_6 + quadrangle_8 (segment_3
//!   facets), same selection;
//! - `interp_block_3d`: tetrahedron_4, material 1 = the first third of the
//!   element indices;
//! - `interp_reinit_2d`: triangle_6; after the facet interpolation, a second
//!   `initElementalFieldInterpolation` with one point per element (the
//!   first quadrature point). Upstream reuses its matrices with the old
//!   point count (`shape_functions.cc:64-81` only resizes them) and returns
//!   96 stale values where 16 are expected (UD-685, category C); the port
//!   rebuilds the interpolation, which is checked against the exact answer.
//!
//! Every other recorded value is compared bitwise: facet numbering and
//! neighbours, element filters, facet / element quadrature points, the
//! stresses set from the quadratic field, each material's
//! `interpolateStress` result (including the zero blocks of the material
//! without elements) and the facet result after every material.

mod model_common;

use std::path::PathBuf;

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{Array, Element, ElementKind, ElementType, ElementTypeMapArray, GhostType, Real};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, init_mesh_facets};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;
use akantu_validation::{Fixture, fixtures_root};
use model_common::*;

const G: GhostType = GhostType::NotGhost;

/// UD-685 regression: largest relative deviation (to `max(|exact|, 1)`) of
/// the port's re-initialised interpolation from the exact answer (the
/// stress at the element's first quadrature point). With 3 quadrature
/// points and the basis `[1, x, y]`, the least-squares fit interpolates
/// the quadrature values exactly in exact arithmetic; the rounding of the
/// 3 × 3 inverse and of the two products stays a small multiple of the
/// coordinate matrices' condition number times ε (observed 8.35e-16);
/// the bound is recorded with its derivation (κ∞ of the
/// four fit matrices ≤ 36.2, a priori ≈ 4e-14; docs/NUMERICAL_VALIDATION.md).
const REINIT_TOL: Real = 1e-12;

fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// v87_probe `MixedSelector`: material 1 for odd quadrangles, 0 otherwise
/// (material 2 gets no element).
#[derive(Debug)]
struct Mixed;

impl ConstitutiveLawSelector for Mixed {
    fn select(&self, _: &SelectorContext<'_>, e: Element) -> akantu_core::Result<Option<usize>> {
        let quad = matches!(e.ty(), ElementType::Quadrangle4 | ElementType::Quadrangle8);
        Ok(Some(usize::from(quad && i64::from(e.index()) % 2 == 1)))
    }
}

/// v87_probe `BlockSelector`: material 1 for the first third of the indices.
#[derive(Debug)]
struct Block(i64);

impl ConstitutiveLawSelector for Block {
    fn select(&self, _: &SelectorContext<'_>, e: Element) -> akantu_core::Result<Option<usize>> {
        Ok(Some(usize::from(3 * i64::from(e.index()) < self.0)))
    }
}

/// smm2_probe / v87_probe quadratic field (same operation order).
fn field(s: usize, x: Real, y: Real, z: Real) -> Real {
    let s = s as Real;
    (s + 1.) * (1. + 2. * x * x - 3. * x * y + 0.5 * y * y + z * (4. - x)) + 0.125 * s
}

struct FacetPoints {
    points: Array<Real>,
    nq_facet: usize,
    nb_facet: usize,
    facet_type: ElementType,
    s2e: Vec<i64>,
}

/// Facet quadrature points of every element of `ty`, facet-major in
/// `subelement_to_element` order (test_interpolate_stress.cc layout).
fn facet_points(mesh: &Mesh, ty: ElementType, dim: usize) -> FacetPoints {
    let mesh_facets = mesh.mesh_facets().unwrap();
    let facet_type = ty.facet_type(0).unwrap();
    let facets_fe = FeEngine::with_precomputation(
        mesh_facets,
        Some(dim - 1),
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "FacetsFEEngine",
    )
    .unwrap();
    let nq_facet = facets_fe.nb_integration_points(facet_type, G).unwrap();
    let nb_facet = mesh_facets.nb_element(facet_type, G);
    let mut quad_facets = Array::zeros(nq_facet * nb_facet, dim);
    facets_fe
        .interpolate_on_integration_points(
            mesh_facets,
            mesh_facets.nodes().view(),
            &mut quad_facets,
            dim,
            facet_type,
            G,
            None,
        )
        .unwrap();
    let f2e = mesh_facets
        .data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .get(ty, G)
        .unwrap();
    let nfe = f2e.nb_component();
    let ne = mesh.nb_element(ty, G);
    let mut points = Array::zeros(ne * nfe * nq_facet, dim);
    let mut s2e = Vec::with_capacity(ne * nfe);
    for el in 0..ne {
        for f in 0..nfe {
            let facet = f2e.tuple(el)[f];
            let gf = usize::try_from(facet.index()).unwrap();
            s2e.push(gf as i64);
            for q in 0..nq_facet {
                points
                    .tuple_mut(el * nfe * nq_facet + f * nq_facet + q)
                    .copy_from_slice(quad_facets.tuple(gf * nq_facet + q));
            }
        }
    }
    FacetPoints {
        points,
        nq_facet,
        nb_facet,
        facet_type,
        s2e,
    }
}

fn quad_points(
    model: &SolidMechanicsModel,
    mesh: &Mesh,
    ty: ElementType,
    dim: usize,
) -> Array<Real> {
    let fe = model.physics.base.fems.get("").unwrap();
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let mut q = Array::zeros(nq * mesh.nb_element(ty, G), dim);
    fe.interpolate_on_integration_points(mesh, mesh.nodes().view(), &mut q, dim, ty, G, None)
        .unwrap();
    q
}

/// Filter of material `m` for `ty` (empty if the port has no block).
fn filter_of(model: &SolidMechanicsModel, m: usize, ty: ElementType) -> Vec<i64> {
    model
        .material(m)
        .unwrap()
        .base()
        .element_filter
        .get(ty, G)
        .map(|a| a.as_slice().iter().map(|&e| i64::from(e)).collect())
        .unwrap_or_default()
}

/// Stresses of material `m` set to `field(s + 10 m, x_q)` (v87_probe).
fn set_stress(
    model: &mut SolidMechanicsModel,
    m: usize,
    ty: ElementType,
    quad: &Array<Real>,
    dim: usize,
) -> Vec<Real> {
    let filter = filter_of(model, m, ty);
    let law = model.physics.materials.law_mut(m).unwrap();
    let Ok(mut stress) = law.base_mut().stress.get_mut(ty, G) else {
        return Vec::new();
    };
    let nq = stress.len() / filter.len().max(1);
    for (l, &e) in filter.iter().enumerate() {
        let e = usize::try_from(e).unwrap();
        for q in 0..nq {
            let x = quad.tuple(e * nq + q);
            let z = if dim == 3 { x[2] } else { 0.0 };
            for (s, v) in stress.tuple_mut(l * nq + q).iter_mut().enumerate() {
                *v = field(s + 10 * m, x[0], x[1], z);
            }
        }
    }
    stress.as_slice().to_vec()
}

struct Check<'a> {
    f: &'a Fixture,
    values: usize,
    problems: Vec<String>,
}

impl Check<'_> {
    fn bits(&mut self, name: &str, got: &[f64]) {
        if !has(self.f, name) {
            self.problems
                .push(format!("{name}: not recorded by the oracle"));
            return;
        }
        let want = f64s(self.f, name);
        let (n, worst) = diff_stats(got, &want);
        if got.len() != want.len() {
            self.problems.push(format!(
                "{name}: {} values, oracle {}",
                got.len(),
                want.len()
            ));
        } else if n != 0 {
            self.problems.push(format!(
                "{name}: {n} of {} values differ (max rel {worst:e})",
                want.len()
            ));
        } else {
            self.values += want.len();
        }
    }

    fn ints(&mut self, name: &str, got: &[i64]) {
        let want = i64s(self.f, name);
        if got != want.as_slice() {
            let first = got.iter().zip(&want).position(|(a, b)| a != b);
            self.problems.push(format!(
                "{name}: differs (port {} entries, oracle {}, first difference at {first:?})",
                got.len(),
                want.len()
            ));
        } else {
            self.values += want.len();
        }
    }
}

/// UD-685 outcome of `interp_reinit_2d`: (port result tuples, filtered
/// elements, oracle values of the stale upstream result, max deviation).
struct Reinit {
    port_tuples: usize,
    filtered: usize,
    oracle_values: usize,
    worst: Real,
}

fn replay(f: &Fixture) -> (usize, Vec<String>, Option<Reinit>) {
    let dim = param(f, "spatial_dimension").as_u64().unwrap() as usize;
    let mut mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
    let _ = init_mesh_facets(&mut mesh).unwrap();
    let types: Vec<ElementType> = param(f, "types")
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().parse().unwrap())
        .collect();
    let total: i64 = types.iter().map(|&t| mesh.nb_element(t, G) as i64).sum();
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    match param(f, "selection").as_str().unwrap() {
        "mixed" => model.set_material_selector(Box::new(Mixed)),
        _ => model.set_material_selector(Box::new(Block(total))),
    }
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    let nb_mat = model.nb_materials();
    let mut c = Check {
        f,
        values: 0,
        problems: Vec::new(),
    };
    if nb_mat as u64 != param(f, "nb_materials").as_u64().unwrap() {
        c.problems.push(format!("port has {nb_mat} materials"));
    }
    let mesh_h = model.physics.base.mesh.clone();
    let mut points = ElementTypeMapArray::new("element_quad_facet");
    let mut first_points = ElementTypeMapArray::new("first_points");
    let mut facet_stress = ElementTypeMapArray::new("facet_stress");
    for &ty in &types {
        let tn = ty.name();
        let (fp, quad) = {
            let mesh = mesh_h.read().unwrap();
            let fp = facet_points(&mesh, ty, dim);
            let quad = quad_points(&model, &mesh, ty, dim);
            if !facet_stress.exists(fp.facet_type, G) {
                let e2s = mesh
                    .mesh_facets()
                    .unwrap()
                    .data()
                    .element_lists(ELEMENT_TO_SUBELEMENT)
                    .unwrap()
                    .get(fp.facet_type, G)
                    .unwrap();
                let lists = e2s.as_slice();
                let first: Vec<i64> = lists
                    .iter()
                    .map(|l| l.first().map_or(-1, |e| i64::from(e.index())))
                    .collect();
                let first_ty: Vec<i64> = lists
                    .iter()
                    .map(|l| {
                        l.first().map_or(-1, |e| {
                            types
                                .iter()
                                .position(|&t| t == e.ty())
                                .map_or(-1, |p| p as i64)
                        })
                    })
                    .collect();
                let second: Vec<i64> = lists
                    .iter()
                    .map(|l| match l.get(1) {
                        Some(e) if !e.is_null() => i64::from(e.index()),
                        _ => -1,
                    })
                    .collect();
                let ft = fp.facet_type.name();
                c.ints(&format!("first_neighbour/{ft}"), &first);
                c.ints(&format!("first_neighbour_type/{ft}"), &first_ty);
                c.ints(&format!("second_neighbour/{ft}"), &second);
                facet_stress
                    .alloc(
                        fp.nb_facet,
                        2 * dim * dim * fp.nq_facet,
                        fp.facet_type,
                        G,
                        0.0,
                    )
                    .unwrap();
            }
            (fp, quad)
        };
        c.ints(&format!("subelement_to_element/{tn}"), &fp.s2e);
        c.bits(&format!("facet_points/{tn}"), fp.points.as_slice());
        c.bits(&format!("quad_points/{tn}"), quad.as_slice());
        points
            .as_map_mut()
            .insert(ty, G, fp.points.clone())
            .unwrap();
        let nq = quad.len() / mesh_h.read().unwrap().nb_element(ty, G).max(1);
        let ne = quad.len() / nq.max(1);
        let fpts = first_points.alloc(ne, dim, ty, G, 0.0).unwrap();
        for el in 0..ne {
            fpts.tuple_mut(el).copy_from_slice(quad.tuple(el * nq));
        }
        for m in 0..nb_mat {
            let filter = filter_of(&model, m, ty);
            c.ints(&format!("filter/{m}/{tn}"), &filter);
            let s = set_stress(&mut model, m, ty, &quad, dim);
            c.bits(&format!("stress/{m}/{tn}"), &s);
        }
    }
    for m in 0..nb_mat {
        if let Err(e) = model.init_material_elemental_field_interpolation(m, &points) {
            c.problems.push(format!("material {m}: init: {e}"));
            continue;
        }
        let mut by_elem = ElementTypeMapArray::new("by_elem");
        if let Err(e) = model.interpolate_material_stress(m, &mut by_elem, G) {
            c.problems
                .push(format!("material {m}: interpolateStress: {e}"));
            continue;
        }
        for &ty in &types {
            match by_elem.get(ty, G) {
                Ok(a) => c.bits(&format!("by_elem/{m}/{}", ty.name()), a.as_slice()),
                Err(_) => c.problems.push(format!(
                    "by_elem/{m}/{}: no block in the port's result (oracle: {} values)",
                    ty.name(),
                    f64s(f, &format!("by_elem/{m}/{}", ty.name())).len()
                )),
            }
        }
        let mut by2 = ElementTypeMapArray::new("by_elem2");
        if let Err(e) =
            model.interpolate_material_stress_on_facets(m, &mut facet_stress, &mut by2, G)
        {
            c.problems
                .push(format!("material {m}: interpolateStressOnFacets: {e}"));
        }
    }
    let fs: Vec<(ElementType, Vec<Real>)> = facet_stress
        .as_map()
        .iter(G)
        .map(|(t, a)| (t, a.as_slice().to_vec()))
        .collect();
    for (t, v) in fs {
        c.bits(&format!("facet_stress/{}", t.name()), &v);
    }

    let mut reinit = None;
    if let Some(outcome) = f
        .manifest()
        .parameters
        .get("reinit_outcome")
        .and_then(|v| v.as_str())
    {
        // upstream did not throw: it interpolated with the stale matrices
        assert_eq!(outcome, "ok", "{}: recorded reinit outcome", f.case_id());
        model
            .init_material_elemental_field_interpolation(0, &first_points)
            .unwrap();
        let mut by = ElementTypeMapArray::new("by_elem_reinit");
        model.interpolate_material_stress(0, &mut by, G).unwrap();
        // exact answer: one point per element = the element's first
        // quadrature point; with nq = number of basis functions (3 for
        // triangle_6, basis [1, x, y]) the fit interpolates the quadrature
        // values, so the result is the stress at q = 0
        let law = model.material(0).unwrap();
        for &ty in &types {
            let filter = filter_of(&model, 0, ty);
            let stress = law.base().stress.get(ty, G).unwrap();
            let nq = stress.len() / filter.len().max(1);
            let got = by.get(ty, G).unwrap();
            let mut worst = 0.0_f64;
            for (l, &e) in filter.iter().enumerate() {
                let want = stress.tuple(l * nq);
                let g = got.tuple(usize::try_from(e).unwrap());
                for (a, b) in g.iter().zip(want) {
                    worst = worst.max((a - b).abs() / b.abs().max(1.0));
                }
            }
            reinit = Some(Reinit {
                port_tuples: got.len(),
                filtered: filter.len(),
                oracle_values: f64s(f, &format!("reinit/by_elem/{}", ty.name())).len(),
                worst,
            });
        }
    }
    (c.values, c.problems, reinit)
}

fn cases() -> Vec<PathBuf> {
    let dir = fixtures_root().join("model/v87");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("manifest.json").exists())
        .collect();
    cases.sort();
    cases
}

/// The four adversarial cases bitwise (every value except UD-685's stale
/// upstream re-initialisation, checked by the test below).
#[test]
fn v87_adversarial_interpolation_cases_match_the_oracle_bitwise() {
    let cases = cases();
    assert_eq!(cases.len(), 4, "fixtures/model/v87 cases: {cases:?}");
    let mut all = Vec::new();
    let mut total = 0;
    for p in &cases {
        let f = Fixture::open(p).unwrap();
        f.require_golden().unwrap();
        f.verify_checksums().unwrap();
        let (n, problems, _) = replay(&f);
        eprintln!(
            "{}: {n} values identical, {} problem(s)",
            f.case_id(),
            problems.len()
        );
        assert!(n > 0, "{}: nothing compared", f.case_id());
        total += n;
        all.extend(
            problems
                .into_iter()
                .map(|p| format!("{}: {p}", f.case_id())),
        );
    }
    eprintln!("v87 adversarial cases: {total} values bitwise");
    assert!(
        all.is_empty(),
        "{} problem(s):\n{}",
        all.len(),
        all.join("\n")
    );
}

/// UD-685 (category C): a second `initElementalFieldInterpolation` with
/// another number of points per element. Upstream (oracle, no throw)
/// interpolates with the old point count over partly overwritten matrices:
/// `reinit/by_elem/_triangle_6` holds 6 stale points × 4 elements × 4
/// components = 96 values where one point per element (16 values) is
/// expected. The port rebuilds the interpolation: one tuple per element,
/// equal to the stress at the first quadrature point.
#[test]
fn ud685_reinit_with_another_point_count_rebuilds_the_interpolation() {
    let f = Fixture::open(&fixtures_root().join("model/v87/interp_reinit_2d")).unwrap();
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    let (_, problems, reinit) = replay(&f);
    assert!(problems.is_empty(), "{problems:?}");
    let r = reinit.expect("interp_reinit_2d records the re-initialisation");
    let nb_el = 4; // _triangle_6 patch mesh
    assert_eq!(
        r.port_tuples, nb_el,
        "port: one interpolated tuple per element"
    );
    assert!(r.filtered > 0, "material 0 holds elements of the patch");
    assert_eq!(
        r.oracle_values,
        6 * nb_el * 4,
        "upstream (UD-685): stale point count (6 facet points) kept"
    );
    assert!(
        r.worst <= REINIT_TOL,
        "port's re-initialised interpolation deviates by {:e} from the stress at q = 0",
        r.worst
    );
    eprintln!(
        "UD-685: port {} tuples (max rel. deviation {:.2e}), upstream {} stale values",
        r.port_tuples, r.worst, r.oracle_values
    );
}
