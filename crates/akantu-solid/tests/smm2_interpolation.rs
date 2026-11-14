// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). SMM-024 tests:
// the material stress-interpolation wrappers (upstream material.cc
// initElementalFieldInterpolation, interpolateStress,
// interpolateStressOnFacets) against the oracle cases
// fixtures/model/smm2/interp_* (oracle/probes/smm2/smm2_probe.cc), plus
// independent checks (a linear field reproduced on both sides of every
// interior facet) and the error paths.
//
// Every recorded value is compared bitwise: the facet / element quadrature
// points (FE engine), the facet numbering, the stresses set from the
// quadratic field, the `interpolateStress` result of each material and the
// facet result after both materials.

mod model_common;

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
use akantu_validation::Fixture;
use model_common::*;

const G: GhostType = GhostType::NotGhost;

/// The probe's alternating selector (element index parity).
#[derive(Debug)]
struct Alternating;

impl ConstitutiveLawSelector for Alternating {
    fn select(
        &self,
        _: &SelectorContext<'_>,
        element: Element,
    ) -> akantu_core::Result<Option<usize>> {
        Ok(Some(usize::try_from(element.index()).unwrap_or(0) % 2))
    }
}

/// The probe's quadratic stress field (same operation order).
fn field(s: usize, x: Real, y: Real, z: Real) -> Real {
    let s = s as Real;
    (s + 1.) * (1. + 2. * x * x - 3. * x * y + 0.5 * y * y + z * (4. - x)) + 0.125 * s
}

fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

fn model_of_mesh(mut mesh: Mesh, dat: &std::path::Path) -> SolidMechanicsModel {
    let _ = init_mesh_facets(&mut mesh).unwrap();
    let parser = akantu_parser::Parser::parse_file(dat).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.set_material_selector(Box::new(Alternating));
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    model
}

/// Facet quadrature points of every element (element, facet, point), as
/// upstream's `test_interpolate_stress`, and the facet FE engine data.
struct FacetPoints {
    points: ElementTypeMapArray<Real>,
    nq_facet: usize,
    nb_facet: usize,
    facet_type: ElementType,
    s2e: Vec<i64>,
}

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
    let mut points = ElementTypeMapArray::new("element_quad_facet");
    let mut s2e = Vec::with_capacity(ne * nfe);
    {
        let el_q = points.alloc(ne * nfe * nq_facet, dim, ty, G, 0.0).unwrap();
        for el in 0..ne {
            for f in 0..nfe {
                let facet = f2e.tuple(el)[f];
                let gf = usize::try_from(facet.index()).unwrap();
                s2e.push(gf as i64);
                for q in 0..nq_facet {
                    el_q.tuple_mut(el * nfe * nq_facet + f * nq_facet + q)
                        .copy_from_slice(quad_facets.tuple(gf * nq_facet + q));
                }
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

/// Element quadrature points of every element (the model's FE engine).
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

/// Set the stress of material `m` to `f(s + 10 m, x_q)` (or the given
/// field), returning the stress block.
fn set_stress(
    model: &mut SolidMechanicsModel,
    m: usize,
    ty: ElementType,
    quad: &Array<Real>,
    dim: usize,
    f: &dyn Fn(usize, Real, Real, Real) -> Real,
) -> Vec<Real> {
    let law = model.physics.materials.law_mut(m).unwrap();
    let filter: Vec<_> = law
        .base()
        .element_filter
        .get(ty, G)
        .unwrap()
        .as_slice()
        .to_vec();
    let nq = law.base().stress.get(ty, G).unwrap().len() / filter.len().max(1);
    let mut stress = law.base_mut().stress.get_mut(ty, G).unwrap();
    for (l, &e) in filter.iter().enumerate() {
        let e = usize::try_from(e).unwrap();
        for q in 0..nq {
            let x = quad.tuple(e * nq + q);
            let z = if dim == 3 { x[2] } else { 0.0 };
            for (s, v) in stress.tuple_mut(l * nq + q).iter_mut().enumerate() {
                *v = f(s + 10 * m, x[0], x[1], z);
            }
        }
    }
    stress.as_slice().to_vec()
}

fn replay(f: &Fixture) -> usize {
    let dim = param(f, "spatial_dimension").as_u64().unwrap() as usize;
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
    let mut model = model_of_mesh(mesh, &f.input_path("material").unwrap());
    let mesh_h = model.physics.base.mesh.clone();
    let types: Vec<ElementType> = param(f, "types")
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().parse().unwrap())
        .collect();
    let mut n = 0;
    let mut points = ElementTypeMapArray::new("element_quad_facet");
    let mut facet_stress = ElementTypeMapArray::new("facet_stress");
    let nb_mat = model.nb_materials();
    assert_eq!(nb_mat, 2);
    for &ty in &types {
        let tn = ty.name();
        let (fp, quad) = {
            let mesh = mesh_h.read().unwrap();
            let fp = facet_points(&mesh, ty, dim);
            let quad = quad_points(&model, &mesh, ty, dim);
            // first neighbour of every facet
            let e2s = mesh
                .mesh_facets()
                .unwrap()
                .data()
                .element_lists(ELEMENT_TO_SUBELEMENT)
                .unwrap()
                .get(fp.facet_type, G)
                .unwrap();
            let first: Vec<i64> = e2s
                .as_slice()
                .iter()
                .map(|l| l.first().map_or(-1, |e| i64::from(e.index())))
                .collect();
            let want = i64s(f, &format!("first_neighbour/{}", fp.facet_type.name()));
            let diff: Vec<_> = first
                .iter()
                .zip(&want)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| (i, *a, *b))
                .collect();
            assert!(
                first.len() == want.len() && diff.is_empty(),
                "{}: first neighbours (facet, port, oracle) {diff:?} ({} vs {} facets)",
                f.case_id(),
                first.len(),
                want.len()
            );
            (fp, quad)
        };
        assert_eq!(
            fp.facet_type.name(),
            param(f, &format!("facet_type/{tn}")).as_str().unwrap()
        );
        assert_eq!(
            fp.s2e,
            i64s(f, &format!("subelement_to_element/{tn}")),
            "{tn}: facets"
        );
        n += assert_bits(
            f,
            &format!("facet_points/{tn}"),
            fp.points.get(ty, G).unwrap().as_slice(),
        );
        n += assert_bits(f, &format!("quad_points/{tn}"), quad.as_slice());
        points
            .as_map_mut()
            .insert(ty, G, fp.points.get(ty, G).unwrap().clone())
            .unwrap();
        facet_stress
            .alloc(
                fp.nb_facet,
                2 * dim * dim * fp.nq_facet,
                fp.facet_type,
                G,
                0.0,
            )
            .unwrap();
        for m in 0..nb_mat {
            let filter: Vec<i64> = model
                .material(m)
                .unwrap()
                .base()
                .element_filter
                .get(ty, G)
                .unwrap()
                .as_slice()
                .iter()
                .map(|&e| i64::from(e))
                .collect();
            assert_eq!(filter, i64s(f, &format!("filter/{m}/{tn}")));
            let s = set_stress(&mut model, m, ty, &quad, dim, &field);
            n += assert_bits(f, &format!("stress/{m}/{tn}"), &s);
        }
    }
    for m in 0..nb_mat {
        model
            .init_material_elemental_field_interpolation(m, &points)
            .unwrap();
        let mut by_elem = ElementTypeMapArray::new("by_elem");
        model
            .interpolate_material_stress(m, &mut by_elem, G)
            .unwrap();
        for &ty in &types {
            n += assert_bits(
                f,
                &format!("by_elem/{m}/{}", ty.name()),
                by_elem.get(ty, G).unwrap().as_slice(),
            );
        }
        let mut by_elem2 = ElementTypeMapArray::new("by_elem2");
        model
            .interpolate_material_stress_on_facets(m, &mut facet_stress, &mut by_elem2, G)
            .unwrap();
        for &ty in &types {
            assert_eq!(
                by_elem2.get(ty, G).unwrap().as_slice(),
                by_elem.get(ty, G).unwrap().as_slice()
            );
        }
    }
    for (ty, a) in facet_stress.as_map().iter(G) {
        n += assert_bits(f, &format!("facet_stress/{}", ty.name()), a.as_slice());
    }
    n
}

#[test]
fn interpolate_stress_matches_the_oracle_bitwise() {
    let cases: Vec<String> = cases_of("smm2")
        .into_iter()
        .filter(|c| c.trim_start_matches("smm2/").starts_with("interp_"))
        .collect();
    assert!(
        cases.len() >= 7,
        "expected the 7 interp_* oracle cases, found {}",
        cases.len()
    );
    let mut total = 0;
    for c in &cases {
        let f = case(c);
        let n = replay(&f);
        eprintln!("{c}: {n} values bitwise");
        total += n;
    }
    eprintln!(
        "SMM-024 interpolation: {} cases, {total} values bitwise",
        cases.len()
    );
}

/// Independent check: a linear stress field is reproduced exactly by the
/// fit (basis `[1, x, y(, z)]` or `[1, x, y, xy]`, ≥ 3 quadrature points),
/// so on every interior facet both sides hold `f(x_q)` (within rounding),
/// and on a boundary facet side 1 is never written (zero).
#[test]
fn linear_field_on_both_sides_of_every_interior_facet() {
    let linear =
        |s: usize, x: Real, y: Real, z: Real| (s as Real + 1.) * (3. + 2. * x - y + 0.5 * z);
    let up = akantu_validation::fixture::workspace_root();
    let dat = up.join("crates/akantu-solid/tests/data/smm2_two_elastic.dat");
    for ty in [ElementType::Tetrahedron10] {
        let dim = 3;
        let mesh = read_mesh(
            &up.join(
                "fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
                 test_materials/interpolation.msh",
            ),
            dim,
        );
        let mut model = model_of_mesh(mesh, &dat);
        let mesh_h = model.physics.base.mesh.clone();
        let (fp, quad) = {
            let mesh = mesh_h.read().unwrap();
            (
                facet_points(&mesh, ty, dim),
                quad_points(&model, &mesh, ty, dim),
            )
        };
        let mut facet_stress = ElementTypeMapArray::new("facet_stress");
        facet_stress
            .alloc(
                fp.nb_facet,
                2 * dim * dim * fp.nq_facet,
                fp.facet_type,
                G,
                0.0,
            )
            .unwrap();
        for m in 0..model.nb_materials() {
            let _ = set_stress(&mut model, m, ty, &quad, dim, &|s, x, y, z| {
                linear(s % 10, x, y, z)
            });
            model
                .init_material_elemental_field_interpolation(m, &fp.points)
                .unwrap();
            let mut by = ElementTypeMapArray::new("by");
            model
                .interpolate_material_stress_on_facets(m, &mut facet_stress, &mut by, G)
                .unwrap();
        }
        // facet quadrature points
        let mesh = mesh_h.read().unwrap();
        let mesh_facets = mesh.mesh_facets().unwrap();
        let e2s = mesh_facets
            .data()
            .element_lists(ELEMENT_TO_SUBELEMENT)
            .unwrap()
            .get(fp.facet_type, G)
            .unwrap();
        let facets_fe = FeEngine::with_precomputation(
            mesh_facets,
            Some(dim - 1),
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "FacetsFEEngine",
        )
        .unwrap();
        let mut xq = Array::zeros(fp.nq_facet * fp.nb_facet, dim);
        facets_fe
            .interpolate_on_integration_points(
                mesh_facets,
                mesh_facets.nodes().view(),
                &mut xq,
                dim,
                fp.facet_type,
                G,
                None,
            )
            .unwrap();
        let res = facet_stress.get(fp.facet_type, G).unwrap();
        let ss = dim * dim;
        let (mut interior, mut boundary, mut worst) = (0, 0, 0.0_f64);
        for facet in 0..fp.nb_facet {
            let neighbours = e2s.tuple(facet)[0].iter().filter(|e| !e.is_null()).count();
            let t = res.tuple(facet);
            for q in 0..fp.nq_facet {
                let x = xq.tuple(facet * fp.nq_facet + q);
                for side in 0..2 {
                    for s in 0..ss {
                        let got = t[(q * 2 + side) * ss + s];
                        if side == 1 && neighbours < 2 {
                            assert_eq!(got.to_bits(), 0, "boundary facet {facet}: side 1 written");
                            continue;
                        }
                        let want = linear(s, x[0], x[1], x[2]);
                        let err = (got - want).abs();
                        worst = worst.max(err / want.abs().max(1.0));
                        assert!(
                            err <= 1e-10 * want.abs().max(1.0),
                            "facet {facet} q {q} side {side} comp {s}: {got} vs {want}"
                        );
                    }
                }
            }
            if neighbours < 2 {
                boundary += 1;
            } else {
                interior += 1;
            }
        }
        assert!(interior > 0 && boundary > 0);
        eprintln!(
            "linear field: {interior} interior + {boundary} boundary facets, max rel. error {worst:.2e}"
        );
    }
}

/// Error paths (each an upstream throw or undefined behaviour): interpolate
/// before the initialisation, a missing facet result block, a facet result
/// with the wrong number of components, an unknown material.
#[test]
fn interpolation_error_paths() {
    let up = akantu_validation::fixture::workspace_root();
    let dat = up.join("crates/akantu-solid/tests/data/smm2_two_elastic.dat");
    let mesh = read_mesh(
        &up.join(
            "fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
             test_materials/interpolation.msh",
        ),
        3,
    );
    let ty = ElementType::Tetrahedron10;
    let mut model = model_of_mesh(mesh, &dat);
    let mut out = ElementTypeMapArray::new("out");
    let e = model
        .interpolate_material_stress(0, &mut out, G)
        .unwrap_err();
    assert!(e.to_string().contains("not initialised"), "{e}");
    assert!(model.interpolate_material_stress(2, &mut out, G).is_err());
    let fp = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        facet_points(&mesh, ty, 3)
    };
    model
        .init_material_elemental_field_interpolation(0, &fp.points)
        .unwrap();
    // missing facet block
    let mut facets = ElementTypeMapArray::new("facets");
    let mut by = ElementTypeMapArray::new("by");
    assert!(
        model
            .interpolate_material_stress_on_facets(0, &mut facets, &mut by, G)
            .is_err()
    );
    // wrong component count
    facets
        .alloc(fp.nb_facet, 2 * 9 * fp.nq_facet + 1, fp.facet_type, G, 0.0)
        .unwrap();
    let mut by = ElementTypeMapArray::new("by");
    let e = model
        .interpolate_material_stress_on_facets(0, &mut facets, &mut by, G)
        .unwrap_err();
    assert!(e.to_string().contains("components"), "{e}");
}

/// UD-682 (category C): a filter type without any
/// element in the mesh. Upstream divides by the element count 0 before its
/// loop over the (necessarily empty) filter; the port skips the type, i.e.
/// writes nothing, as upstream's loop would. Reached here with an empty
/// ghost connectivity block of the mesh's type (a rank without ghost
/// elements of that type), so the material filter has an empty
/// `(tetrahedron_10, ghost)` block, and a caller-allocated
/// `by_elem_result` block (the only way past upstream's lookup).
#[test]
fn ud682_filter_type_without_elements_writes_nothing() {
    let up = akantu_validation::fixture::workspace_root();
    let dat = up.join("crates/akantu-solid/tests/data/smm2_two_elastic.dat");
    let mut mesh = read_mesh(
        &up.join(
            "fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
             test_materials/interpolation.msh",
        ),
        3,
    );
    let ty = ElementType::Tetrahedron10;
    let ghost = GhostType::Ghost;
    let _ = akantu_mesh::MeshAccessor::new(&mut mesh).connectivity_mut(ty, ghost);
    assert_eq!(mesh.nb_element(ty, ghost), 0);
    let mut model = model_of_mesh(mesh, &dat);
    assert!(
        model
            .material(0)
            .unwrap()
            .base()
            .element_filter
            .get(ty, ghost)
            .is_ok_and(|f| f.is_empty()),
        "the material filter has the empty ghost block"
    );
    let fp = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        facet_points(&mesh, ty, 3)
    };
    model
        .init_material_elemental_field_interpolation(0, &fp.points)
        .unwrap();
    let tuple = 2 * 9 * fp.nq_facet;
    let mut facets = ElementTypeMapArray::new("facets");
    facets
        .alloc(fp.nb_facet, tuple, fp.facet_type, ghost, 0.0)
        .unwrap();
    // without a by_elem_result block the lookup fails, as upstream's
    let mut by = ElementTypeMapArray::new("by");
    assert!(
        model
            .interpolate_material_stress_on_facets(0, &mut facets, &mut by, ghost)
            .is_err()
    );
    let mut by = ElementTypeMapArray::new("by");
    by.alloc(0, 9, ty, ghost, 0.0).unwrap();
    model
        .interpolate_material_stress_on_facets(0, &mut facets, &mut by, ghost)
        .unwrap();
    assert!(
        facets
            .get(fp.facet_type, ghost)
            .unwrap()
            .as_slice()
            .iter()
            .all(|&v| v.to_bits() == 0),
        "nothing written"
    );
}

/// UD-682 (category A): a filtered element whose facet is missing
/// (`ElementNull` in `subelement_to_element`) is refused; upstream
/// dereferences it.
#[test]
fn ud682_missing_facet_of_a_filtered_element_is_refused() {
    let up = akantu_validation::fixture::workspace_root();
    let dat = up.join("crates/akantu-solid/tests/data/smm2_two_elastic.dat");
    let mut mesh = read_mesh(
        &up.join(
            "fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
             test_materials/interpolation.msh",
        ),
        3,
    );
    let ty = ElementType::Tetrahedron10;
    let _ = init_mesh_facets(&mut mesh).unwrap();
    let fp = facet_points(&mesh, ty, 3);
    // element 0 (material 0 under the alternating selector) loses facet 2
    mesh.mesh_facets_mut()
        .unwrap()
        .data_mut()
        .element_map_mut(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .get_mut(ty, G)
        .unwrap()
        .tuple_mut(0)[2] = Element::NULL;
    let mut model = model_of_mesh(mesh, &dat);
    model
        .init_material_elemental_field_interpolation(0, &fp.points)
        .unwrap();
    let mut facets = ElementTypeMapArray::new("facets");
    facets
        .alloc(fp.nb_facet, 2 * 9 * fp.nq_facet, fp.facet_type, G, 0.0)
        .unwrap();
    let mut by = ElementTypeMapArray::new("by");
    let e = model
        .interpolate_material_stress_on_facets(0, &mut facets, &mut by, G)
        .unwrap_err();
    assert!(e.to_string().contains("ElementNull"), "{e}");
}

/// The facet loop visits the regular element types of
/// the filter only (upstream `elementTypes(dim, ghost)` defaults to
/// `_ek_regular`). A non-regular (cohesive) block in a material's filter
/// is ignored, as upstream; before the fix the port looked up its
/// `by_elem_result` block and failed.
#[test]
fn facet_loop_ignores_non_regular_filter_types() {
    let up = akantu_validation::fixture::workspace_root();
    let dat = up.join("crates/akantu-solid/tests/data/smm2_two_elastic.dat");
    let mesh = read_mesh(
        &up.join(
            "fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
             test_materials/interpolation.msh",
        ),
        3,
    );
    let ty = ElementType::Tetrahedron10;
    let mut model = model_of_mesh(mesh, &dat);
    let fp = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        facet_points(&mesh, ty, 3)
    };
    model
        .init_material_elemental_field_interpolation(0, &fp.points)
        .unwrap();
    let mut facets = ElementTypeMapArray::new("facets");
    facets
        .alloc(fp.nb_facet, 2 * 9 * fp.nq_facet, fp.facet_type, G, 0.0)
        .unwrap();
    let mut by = ElementTypeMapArray::new("by");
    model
        .interpolate_material_stress_on_facets(0, &mut facets, &mut by, G)
        .unwrap();
    let reference = facets.get(fp.facet_type, G).unwrap().as_slice().to_vec();
    model
        .physics
        .materials
        .law_mut(0)
        .unwrap()
        .base_mut()
        .element_filter
        .alloc(0, 1, ElementType::Cohesive3d6, G, 0)
        .unwrap();
    let mut by = ElementTypeMapArray::new("by");
    model
        .interpolate_material_stress_on_facets(0, &mut facets, &mut by, G)
        .unwrap();
    assert_eq!(
        facets.get(fp.facet_type, G).unwrap().as_slice(),
        reference.as_slice()
    );
}
