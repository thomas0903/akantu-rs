// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_interpolate_stress.cc, material_interpolate.dat,
// interpolation.geo}.
//! Port of upstream `test_interpolate_stress` (TEST-136): on the frozen
//! second-order tetrahedral mesh `interpolation.msh` (D-009, `_tetrahedron_10`)
//! with its facet mesh (`initMeshFacets` builds every facet level; the
//! test's extra `buildAllFacets(mesh, mesh_facets)` call appends a second,
//! geometrically identical copy of every facet upstream — 484 instead of 242
//! `_triangle_6`, oracle case `model/smm2` probe — and re-points
//! `subelement_to_element` to the copies, which does not change the facet
//! quadrature-point positions checked here, so it is not reproduced), the
//! stress of the
//! material is set at every quadrature point to `σ_s(x) = s · f(x)`,
//! `f(x, y, z) = 100 + 2x + 3y + 4z`, `s = 0…8`, and interpolated at the
//! quadrature points of the element's facets (positions from a facet FE
//! engine, ordered element → facet (`subelement_to_element`) → facet
//! quadrature point). The interpolation fits, per element, the polynomial
//! basis `[1, x, y, z]` through the 4 quadrature points (upstream
//! `interpolateElementalFieldFromIntegrationPoints`), so a linear field is
//! reproduced: every value must equal `s · f(x)` within 1e-10 (upstream
//! `EXIT_FAILURE` otherwise).
//!
//! The stresses are interpolated through the material wrappers, as
//! upstream: `model.getMaterial(0).initElementalFieldInterpolation(...)` /
//! `interpolateStress(...)` are
//! `SolidMechanicsModel::init_material_elemental_field_interpolation(0, ..)`
//! / `interpolate_material_stress(0, ..)` (SMM-024, on the model's default FE
//! engine, upstream `Material::getFEEngine()`), and `interpolated_stress` is
//! pre-sized like upstream's (`initialize(mesh, dim², dim)` then `resize`).

mod model_common;

use akantu_core::{Array, ElementKind, ElementType, ElementTypeMapArray, GhostType, Real};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::SharedMesh;
use akantu_mesh::utils::{SUBELEMENT_TO_ELEMENT, init_mesh_facets};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use akantu_validation::reduce::NanMinMax;
use model_common::read_mesh;

/// `material_interpolate.dat` (verbatim).
const MATERIAL: &str = "material elastic [\n\
\t name = steel\n\
\t rho = 7800   # density\n\
\t E   = 2.1e11 # young's modulus\n\
\t nu  = 0.0    # poisson's ratio\n\
]\n";

fn function(x: Real, y: Real, z: Real) -> Real {
    100. + 2. * x + 3. * y + 4. * z
}

#[test]
fn test_interpolate_stress() {
    const DIM: usize = 3;
    const G: GhostType = GhostType::NotGhost;
    let ty = ElementType::Tetrahedron10;

    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
         test_materials/interpolation.msh",
    );
    let mut mesh = read_mesh(&path, DIM);
    let type_facet = ty.facet_type(0).unwrap();
    assert_eq!(type_facet, ElementType::Triangle6);
    let _ = init_mesh_facets(&mut mesh).unwrap();

    let parser = Parser::parse_str(MATERIAL, "material_interpolate.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.init_full(&ModelOptions::default()).unwrap();

    let mesh_h = model.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let mesh_facets = mesh.mesh_facets().unwrap();
    let position = mesh.nodes();
    let nb_facet = mesh_facets.nb_element(type_facet, G);
    let nb_element = mesh.nb_element(ty, G);
    assert!(nb_element > 0 && nb_facet > 0);

    // quadrature points positions on facets
    let facets_fe = FeEngine::with_precomputation(
        mesh_facets,
        Some(DIM - 1),
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "FacetsFEEngine",
    )
    .unwrap();
    let nb_quad_per_facet = facets_fe.nb_integration_points(type_facet, G).unwrap();
    let mut quad_facets = Array::zeros(nb_quad_per_facet * nb_facet, DIM);
    facets_fe
        .interpolate_on_integration_points(
            mesh_facets,
            mesh_facets.nodes().view(),
            &mut quad_facets,
            DIM,
            type_facet,
            G,
            None,
        )
        .unwrap();

    let facet_to_element = mesh_facets
        .data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .get(ty, G)
        .unwrap();
    let nb_facet_per_elem = facet_to_element.nb_component();
    assert_eq!(nb_facet_per_elem, 4);

    let n_points = nb_element * nb_facet_per_elem * nb_quad_per_facet;
    let mut element_quad_facet = ElementTypeMapArray::new("element_quad_facet");
    {
        let el_q_facet = element_quad_facet.alloc(n_points, DIM, ty, G, 0.0).unwrap();
        for el in 0..nb_element {
            for f in 0..nb_facet_per_elem {
                let facet = facet_to_element.tuple(el)[f];
                assert_eq!(facet.ty(), type_facet);
                let global_facet = usize::try_from(facet.index()).unwrap();
                for q in 0..nb_quad_per_facet {
                    let row =
                        el * nb_facet_per_elem * nb_quad_per_facet + f * nb_quad_per_facet + q;
                    el_q_facet
                        .tuple_mut(row)
                        .copy_from_slice(quad_facets.tuple(global_facet * nb_quad_per_facet + q));
                }
            }
        }
    }

    // quadrature points positions of the elements (the model's FE engine)
    let fe = model.physics.base.fems.get("").unwrap();
    let nb_quad_per_element = fe.nb_integration_points(ty, G).unwrap();
    let nb_tot_quad_el = nb_quad_per_element * nb_element;
    let mut quad_elements = Array::zeros(nb_tot_quad_el, DIM);
    fe.interpolate_on_integration_points(
        &mesh,
        position.view(),
        &mut quad_elements,
        DIM,
        ty,
        G,
        None,
    )
    .unwrap();

    // assign some values to the stresses (every element is in material 0,
    // in element order)
    {
        let law = model.physics.materials.law_mut(0).unwrap();
        assert_eq!(
            law.base().element_filter.get(ty, G).unwrap().len(),
            nb_element
        );
        let mut stress = law.base_mut().stress.get_mut(ty, G).unwrap();
        assert_eq!(stress.len(), nb_tot_quad_el);
        for q in 0..nb_tot_quad_el {
            let p = quad_elements.tuple(q);
            let v = function(p[0], p[1], p[2]);
            for (s, value) in stress.tuple_mut(q).iter_mut().enumerate() {
                *value = s as Real * v;
            }
        }
    }

    // the guards end before the mesh is dropped and the model is used mutably (P12)
    drop(position);
    drop(mesh);

    // interpolate the stresses on the facets' quadrature points (material
    // wrappers, SMM-024); `interpolated_stress` sized as upstream
    let mut interpolated_stress = ElementTypeMapArray::new("interpolated_stress");
    interpolated_stress
        .alloc(n_points, DIM * DIM, ty, G, 0.0)
        .unwrap();
    model
        .init_material_elemental_field_interpolation(0, &element_quad_facet)
        .unwrap();
    model
        .interpolate_material_stress(0, &mut interpolated_stress, G)
        .unwrap();
    let interp_stress = interpolated_stress.get(ty, G).unwrap();
    assert_eq!(interp_stress.len(), n_points);
    assert_eq!(interp_stress.nb_component(), DIM * DIM);

    let tolerance = 1.0e-10;
    let el_q_facet = element_quad_facet.get(ty, G).unwrap();
    let mut worst = 0.0_f64;
    for row in 0..n_points {
        let p = el_q_facet.tuple(row);
        let numerical = interp_stress.tuple(row);
        for (s, &value) in numerical.iter().enumerate() {
            let theoretical = s as Real * function(p[0], p[1], p[2]);
            let err = (theoretical - value).abs();
            worst = worst.nan_max(err);
            assert!(
                err <= tolerance,
                "Theoretical and numerical values aren't coincident! point {row}, component \
                 {s}: {theoretical} vs {value}"
            );
        }
    }
    eprintln!(
        "interpolate stress: {n_points} facet points × {} components, max |error| {worst:.3e} (tol 1e-10)",
        DIM * DIM
    );
}
