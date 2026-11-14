// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): helpers of extra_packages/igfem/test/patch_tests/
// test_igfem_triangle_{4,5}.cc (computeL2Error, boundary conditions, volume)
// and test_solid_mechanics_model_igfem.cc.

//! Model helpers of the IGFEM tests.

#![allow(dead_code)]

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Array, Element, ElementKind, ElementType, GhostType, Real, TypeFilter};
use akantu_igfem::SolidMechanicsModelIgfem;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ElementDataSelector, LawIndexSelector, SelectorChain};
use akantu_parser::{ParamValue, Parser};

use akantu_igfem::model::IgfemKindSelector;

/// IGFEM model on `mesh` with the input file `dat` (text).
pub fn model(mesh: Mesh, dat: &str) -> SolidMechanicsModelIgfem {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    SolidMechanicsModelIgfem::new(
        SharedMesh::new(mesh),
        Some(2),
        akantu_igfem::model::DEFAULT_ID,
        parser,
    )
    .unwrap()
}

/// Upstream `StraightInterfaceMatSelector`: IGFEM elements → law `igfem`,
/// otherwise the law named by the element's `physical_names`.
pub fn straight_interface_selector(model: &mut SolidMechanicsModelIgfem, igfem: usize) {
    model.set_material_selector_chain(SelectorChain {
        selectors: vec![
            Box::new(IgfemKindSelector(igfem)),
            Box::new(ElementDataSelector::new("physical_names")),
            Box::new(LawIndexSelector),
        ],
        fallback_value: 0,
    });
}

/// Static init.
pub fn init_static(model: &mut SolidMechanicsModelIgfem) {
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
}

/// Newton–Raphson settings of upstream `solveStep<_scm_newton_raphson_tangent,
/// _increment>(threshold, error, max_iterations)` (`_increment` is v5's
/// `solution` criterion).
pub fn set_newton(model: &mut SolidMechanicsModelIgfem, max_iterations: i64, threshold: Real) {
    let nls = model.smm.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("max_iterations", ParamValue::Int(max_iterations))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(threshold))
        .unwrap();
    nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
        .unwrap();
}

/// Upstream patch-test boundary conditions: bottom on rollers (`u_y = 0`),
/// left on rollers (`u_x = 0`), right `u_x = 1` (`eps = 1e-6 · width`).
pub fn patch_bcs(model: &mut SolidMechanicsModelIgfem) {
    let pos = model.mesh().read().unwrap().nodes().clone();
    let mut lower = [Real::INFINITY; 2];
    let mut upper = [Real::NEG_INFINITY; 2];
    for x in pos.tuples() {
        for d in 0..2 {
            lower[d] = lower[d].min(x[d]);
            upper[d] = upper[d].max(x[d]);
        }
    }
    let (bottom, left, right) = (lower[1], lower[0], upper[0]);
    let eps = ((right - left) * 1e-6).abs();
    let n = pos.len();
    let mut blocked = model.smm.blocked_dofs().unwrap().clone();
    let mut disp = model.smm.displacement().unwrap().clone();
    for i in 0..n {
        let x = pos.tuple(i);
        if (x[1] - bottom).abs() < eps {
            blocked.tuple_mut(i)[1] = true;
            disp.tuple_mut(i)[1] = 0.0;
        }
        if (x[0] - left).abs() < eps {
            blocked.tuple_mut(i)[0] = true;
            disp.tuple_mut(i)[0] = 0.0;
        }
        if (x[0] - right).abs() < eps {
            blocked.tuple_mut(i)[0] = true;
            disp.tuple_mut(i)[0] = 1.0;
        }
    }
    model
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .copy_from(&blocked)
        .unwrap();
    model
        .smm
        .displacement_mut()
        .unwrap()
        .copy_from(&disp)
        .unwrap();
}

/// Upstream `computeL2Error`: `sqrt(Σ ∫|u − u_exact|²) / sqrt(Σ ∫|u_exact|²)`
/// over the regular (model FE engine) and IGFEM (IGFEM engine) elements,
/// `u_exact` along x only (y exact solution 0), evaluated at the
/// integration points.
pub fn l2_error(model: &SolidMechanicsModelIgfem, exact: &dyn Fn(Real) -> Real) -> Real {
    let mesh_h = model.mesh().clone();
    let mesh = mesh_h.read().unwrap();
    let ghost = GhostType::NotGhost;
    let u = model.smm.displacement().unwrap();
    let mut error = 0.0;
    let mut norm = 0.0;
    let mut accumulate = |disp: &Array<Real>,
                          coords: &Array<Real>,
                          nq: usize,
                          integrate: &dyn Fn(&[Real], usize) -> Real| {
        let ne = disp.len() / nq;
        for e in 0..ne {
            let mut err = vec![0.0; nq];
            let mut nrm = vec![0.0; nq];
            for q in 0..nq {
                let x = coords.tuple(e * nq + q)[0];
                let ex = exact(x);
                let d = disp.tuple(e * nq + q);
                let (ex0, ex1) = (d[0] - ex, d[1]);
                err[q] = ex0 * ex0 + ex1 * ex1;
                nrm[q] = ex.abs() * ex.abs();
            }
            error += integrate(&err, e);
            norm += integrate(&nrm, e);
        }
    };
    let fe = model.smm.physics.base.fems.get("").unwrap();
    for ty in mesh
        .element_types(TypeFilter {
            spatial_dimension: Some(2),
            ghost_type: ghost,
            kind: Some(ElementKind::Regular),
        })
        .collect::<Vec<_>>()
    {
        let nq = fe.nb_integration_points(ty, ghost).unwrap();
        let mut disp = Array::new(2);
        fe.interpolate_on_integration_points(&mesh, u.view(), &mut disp, 2, ty, ghost, None)
            .unwrap();
        let mut coords = Array::new(2);
        fe.integration_points_coordinates(&mesh, &mut coords, ty, ghost, None)
            .unwrap();
        let integrate = |f: &[Real], e: usize| {
            let filter = [akantu_core::Idx::try_from(e).unwrap()];
            let mut out = Array::new(1);
            fe.integrate(
                Array::from_vec(f.to_vec(), 1).unwrap().view(),
                &mut out,
                1,
                ty,
                ghost,
                Some(&filter),
            )
            .unwrap();
            out.as_slice()[0]
        };
        accumulate(&disp, &coords, nq, &integrate);
    }
    let engine = model.ctx.engine().unwrap();
    for ty in engine.element_types(&mesh, ghost).collect::<Vec<_>>() {
        let nq = engine.nb_integration_points(ty).unwrap();
        let mut disp = Array::new(2);
        engine
            .interpolate_on_integration_points(&mesh, u.view(), &mut disp, 2, ty, ghost, None)
            .unwrap();
        let mut coords = Array::new(2);
        engine
            .integration_points_coordinates(&mesh, &mut coords, ty, ghost, None)
            .unwrap();
        let integrate =
            |f: &[Real], e: usize| engine.integrate_element(&mesh, f, ty, e, ghost).unwrap();
        accumulate(&disp, &coords, nq, &integrate);
    }
    error.sqrt() / norm.sqrt()
}

/// Number of elements of each of `_triangle_3`, `_igfem_triangle_4`,
/// `_igfem_triangle_5`.
pub fn counts(mesh: &Mesh) -> [usize; 3] {
    [
        mesh.nb_element(ElementType::Triangle3, GhostType::NotGhost),
        mesh.nb_element(ElementType::IgfemTriangle4, GhostType::NotGhost),
        mesh.nb_element(ElementType::IgfemTriangle5, GhostType::NotGhost),
    ]
}

/// Law of an element.
pub fn law_of(model: &SolidMechanicsModelIgfem, el: Element) -> usize {
    let m = model.smm.material_by_element().element_ref(el).unwrap()[0];
    usize::try_from(m).unwrap()
}

/// Read `msh`, apply the growing-gel passes `passes` (sphere lists) on the
/// mesh alone, then attach an IGFEM model (input `dat`) with the last
/// spheres registered as `domain` and initialise it (static). Returns the
/// model, the number of standard nodes and the counts after each pass.
/// (Feature `cgal`: the growing-gel mesh generation.)
#[cfg(feature = "cgal")]
#[allow(clippy::type_complexity)]
pub fn model_on_generated(
    msh: &str,
    dat: &str,
    passes: &[Vec<akantu_geometry_cgal::SkSphere>],
    domain: &str,
    selector: Option<usize>,
) -> (SolidMechanicsModelIgfem, usize, Vec<([usize; 3], usize)>) {
    use akantu_igfem::growing_gel::MeshIgfemSphericalGrowingGel;
    let shared = SharedMesh::new(super::read_msh(2, msh, "mesh"));
    let nb_standard = shared.read().unwrap().nb_nodes();
    let mut gel = MeshIgfemSphericalGrowingGel::new(&shared.read().unwrap()).unwrap();
    gel.init(&shared.read().unwrap()).unwrap();
    let mut stages = Vec::new();
    for spheres in passes {
        shared
            .mutate(
                &mut [],
                |tx| gel.build_igfem_mesh_from_spheres(tx, spheres).map(|_| ()),
                None,
            )
            .unwrap();
        let m = shared.read().unwrap();
        stages.push((counts(&m), m.nb_nodes() - nb_standard));
    }
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModelIgfem::new(shared, Some(2), akantu_igfem::model::DEFAULT_ID, parser)
            .unwrap();
    if let Some(i) = selector {
        straight_interface_selector(&mut model, i);
    }
    if let Some(last) = passes.last() {
        model
            .register_geometry_object(last.clone(), domain)
            .unwrap();
    }
    init_static(&mut model);
    (model, nb_standard, stages)
}

/// IGFEM model on `mesh` with the input file `dat` and the extra materials
/// registered (`damage_iterative`, `iterative_stiffness_reduction` as
/// sub-materials of the IGFEM damage laws).
pub fn model_with_extra_materials(mesh: Mesh, dat: &str) -> SolidMechanicsModelIgfem {
    let mut m = model(mesh, dat);
    akantu_extra_materials::register_extra_materials(&mut m.smm.physics.factory).unwrap();
    m
}

/// Upstream `ASRMaterialSelector` / `TestMaterialSelector` of the IGFEM
/// damage tests: IGFEM elements → law `igfem`; regular elements → `1` when
/// their barycenter is inside the model's **current** default geometry
/// (upstream `model.isInside(barycenter)`, read through the shared IGFEM
/// context), else `0`.
#[derive(Debug)]
pub struct GeometrySelector {
    pub igfem: usize,
    pub ctx: std::sync::Arc<akantu_igfem::context::IgfemContext>,
}

impl akantu_model::constitutive_law::ConstitutiveLawSelector for GeometrySelector {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        el: Element,
    ) -> akantu_core::Result<Option<usize>> {
        if el.ty().kind() == ElementKind::Igfem {
            return Ok(Some(self.igfem));
        }
        let b = ctx.mesh.barycenter(el)?;
        Ok(Some(usize::from(self.ctx.is_inside(&b, "")?)))
    }
}

/// Install a [`GeometrySelector`].
pub fn geometry_selector(model: &mut SolidMechanicsModelIgfem, igfem: usize) {
    let ctx = model.ctx.clone();
    model.set_material_selector(Box::new(GeometrySelector { igfem, ctx }));
}

/// The `Real` internal `name` of law `law` on the not-ghost block `ty`.
pub fn real_internal(
    model: &SolidMechanicsModelIgfem,
    law: usize,
    name: &str,
    ty: ElementType,
) -> Vec<Real> {
    let mut out = None;
    model
        .smm
        .material(law)
        .unwrap()
        .for_each_internal(&mut |n, f| {
            if n == name
                && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
            {
                out = Some(f.get(ty, GhostType::NotGhost).unwrap().as_slice().to_vec());
            }
        });
    out.unwrap_or_else(|| panic!("no Real internal {name} on law {law}"))
}

/// The `UInt` internal `name` of law `law` on the not-ghost block `ty`.
pub fn uint_internal(
    model: &SolidMechanicsModelIgfem,
    law: usize,
    name: &str,
    ty: ElementType,
) -> Vec<akantu_core::UInt> {
    let mut out = None;
    model
        .smm
        .material(law)
        .unwrap()
        .for_each_internal(&mut |n, f| {
            if n == name
                && let akantu_model::constitutive_law::InternalFieldRef::UInt(f) = f
            {
                out = Some(f.get(ty, GhostType::NotGhost).unwrap().as_slice().to_vec());
            }
        });
    out.unwrap_or_else(|| panic!("no UInt internal {name} on law {law}"))
}

/// Set point `q` of the `Real` internal `name` of law `law` (block `ty`,
/// not ghost).
pub fn set_real_internal(
    model: &mut SolidMechanicsModelIgfem,
    law: usize,
    name: &str,
    ty: ElementType,
    q: usize,
    value: Real,
) {
    let mut done = false;
    model
        .smm
        .material_mut(law)
        .unwrap()
        .for_each_internal_mut(&mut |n, f| {
            if n == name
                && let akantu_model::constitutive_law::InternalFieldMut::Real(f) = f
            {
                f.get_mut(ty, GhostType::NotGhost)?.as_mut_slice()[q] = value;
                done = true;
            }
            Ok(())
        })
        .unwrap();
    assert!(done, "no Real internal {name} on law {law}");
}

/// Law and law-local index of an element.
pub fn law_and_local(model: &SolidMechanicsModelIgfem, el: Element) -> (usize, usize) {
    let law = law_of(model, el);
    let local = model
        .smm
        .physics
        .materials
        .local_numbering_map()
        .element(el)
        .unwrap()[0];
    (law, usize::try_from(local).unwrap())
}

/// Barycenter of the parent nodes of an IGFEM element (upstream
/// `Math::barycenter` of the first `getNbParentNodes` nodes).
pub fn parent_barycenter(model: &SolidMechanicsModelIgfem, el: Element) -> Vec<Real> {
    let mesh = model.mesh().read().unwrap();
    let np = akantu_igfem::element::nb_parent_nodes(el.ty()).unwrap();
    let conn = mesh.element_connectivity(el).unwrap().to_vec();
    let mut b = vec![0.0; 2];
    for &n in &conn[..np] {
        let x = mesh.nodes().tuple(usize::try_from(n).unwrap()).to_vec();
        for d in 0..2 {
            b[d] += x[d];
        }
    }
    for v in &mut b {
        *v /= np as Real;
    }
    b
}

/// Mesh diagonal (upstream `upper_bounds.distance(lower_bounds)`).
pub fn mesh_diagonal(model: &SolidMechanicsModelIgfem) -> Real {
    let mesh = model.mesh().read().unwrap();
    let mut lo = [Real::INFINITY; 2];
    let mut hi = [Real::NEG_INFINITY; 2];
    for x in mesh.nodes().tuples() {
        for d in 0..2 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    ((hi[0] - lo[0]) * (hi[0] - lo[0]) + (hi[1] - lo[1]) * (hi[1] - lo[1])).sqrt()
}
