// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_multi_material_elastic.cc,
// test_multi_material_elastic.dat, test_multi_material_elastic.geo}.
//! Port of upstream `test_multi_material_elastic` (TEST-139): a 2-D column
//! of three layers (`weak` / `strong` / `weak`, assigned by
//! `MeshDataMaterialSelector<std::string>("physical_names")`), `ground`
//! blocked in y, `corner` blocked in x, a unit traction in y on `air`,
//! static analysis with Newton-Raphson limited to one iteration (threshold
//! 1e-8 on the residual). Every quadrature point of both materials must
//! carry `σ = e_y ⊗ e_y` and `∇u = ε_yy e_y ⊗ e_y` with `ε_yy = 1 / E`
//! (`strong`: E = 2 → 0.5, `weak`: E = 1 → 1), Frobenius distance ≤ 1e-10
//! (upstream `AKANTU_ERROR` otherwise). Mesh: the frozen Gmsh 4.13.1 mesh
//! of the upstream `add_mesh` target (D-009).

mod model_common;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{GhostType, Real};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FlagOnly, FromTraction};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::{read_mesh, set_newton};

/// `test_multi_material_elastic.dat` (verbatim).
const MATERIALS: &str = "material elastic [\n\
\t name = weak\n\
\t rho = 1.\n\
\t nu = 0.\n\
\t E = 1.\n\
]\n\
\n\
material elastic [\n\
\t name = strong\n\
\t rho = 1.\n\
\t nu = 0.\n\
\t E = 2.\n\
]\n";

#[test]
fn test_multi_material_elastic() {
    const DIM: usize = 2;
    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
         test_materials/test_multi_material_elastic.msh",
    );
    let mesh = read_mesh(&path, DIM);
    let parser = Parser::parse_str(MATERIALS, "test_multi_material_elastic.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();

    model
        .apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::Y,
            },
            "ground",
        )
        .unwrap();
    model
        .apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "corner",
        )
        .unwrap();
    model
        .apply_neumann_bc(&FromTraction::new(vec![0., 1.]), "air")
        .unwrap();

    set_newton(&mut model, 1, 1e-8, "residual");
    model.solve_step(None).unwrap();

    // column-major 2 × 2: only the (y, y) entry (index 3) is non-zero
    let reference = |yy: Real| [0., 0., 0., yy];
    let ref_stress = reference(1.);
    let mut checked = 0;
    for (mat, eps_yy) in [("strong", 0.5), ("weak", 1.0)] {
        let ref_strain = reference(eps_yy);
        let law = model.material_by_name(mat).unwrap();
        let types = law.base().filter_types(GhostType::NotGhost);
        assert!(!types.is_empty(), "{mat} owns no element");
        for ty in types {
            if ty.spatial_dimension() != DIM {
                continue;
            }
            let stress = law.base().stress.get(ty, GhostType::NotGhost).unwrap();
            let grad_u = law.base().grad_u.get(ty, GhostType::NotGhost).unwrap();
            assert!(!stress.is_empty());
            let dist = |a: &[Real; 4], b: &[Real; 4]| {
                a.iter()
                    .zip(b)
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<Real>()
                    .sqrt()
            };
            for (q, (s, g)) in stress
                .as_slice()
                .as_chunks::<{ DIM * DIM }>()
                .0
                .iter()
                .zip(grad_u.as_slice().as_chunks::<{ DIM * DIM }>().0.iter())
                .enumerate()
            {
                assert!(
                    dist(s, &ref_stress) <= 1e-10,
                    "The stresses are not correct: {mat} {ty} q {q}: {s:?}"
                );
                assert!(
                    dist(g, &ref_strain) <= 1e-10,
                    "The grad_u are not correct: {mat} {ty} q {q}: {g:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}
