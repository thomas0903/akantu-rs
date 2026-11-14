// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_fe_engine_precomputation_structural.cc
// and its fixture test/test_fe_engine/test_fe_engine_structural_fixture.hh
// (TestBernoulliB2/B3.PrecomputeRotations, TestDKT18.DISABLED_PrecomputeRotations).

//! TEST-015 with upstream's inputs: the meshes `_bernoulli_beam_2.msh`,
//! `_bernoulli_beam_3.msh` and `_discrete_kirchhoff_triangle_18.msh`
//! (byte-identical copies in `fixtures/mesh_io/test_fe_engine/<type>/inputs`)
//! are read with the structural Gmsh reader (`_miot_gmsh_struct`, as
//! `TestFEMStructuralFixture::readMesh`) into a mesh of the element's
//! spatial dimension, the structural FE engine (`FEEngineTemplate
//! <IntegratorGauss, ShapeStructural, _ek_structural>`, id `my_fem`) is
//! built and `initShapeFunctions()` is called, then the precomputed
//! rotation matrices are compared with upstream's closed forms, Frobenius
//! norm of the error `≤ 1e-15` (`Math::setTolerance(1e-15)`,
//! `test_fe_engine_precomputation_structural.cc:60-69, 82-96`).
//!
//! `TestBernoulliB3` adds the elemental `extra_normal`
//! `(-36/65, -48/65, 5/13)` after reading (its `readMesh` override). The
//! DKT test is `DISABLED_` upstream (gtest builds but does not run it; it
//! only prints the rotations): here it runs on the upstream mesh with
//! independent checks instead of the print (block-diagonal, orthonormal,
//! proper rotation, first row = first edge, last row = the normal ±e_z of
//! the z = 0 plate). `crates/akantu-fe/tests/test_fe_engine_precomputation_structural.rs`
//! runs the same checks on meshes built in code.

use std::path::PathBuf;

use akantu_core::enums::MeshIoType;
use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_fe::structural::{EXTRA_NORMAL, StructuralFeEngine};
use akantu_io::read_mesh;
use akantu_mesh::Mesh;

const G: GhostType = GhostType::NotGhost;

/// `Math::setTolerance(1e-15)` of the upstream tests.
const TOLERANCE: Real = 1e-15;

/// `TestFEMBaseFixture::SetUp` with `TestFEMStructuralFixture::readMesh`:
/// `Mesh(dim)` with `dim = ElementClass<type>::getSpatialDimension()`, then
/// `mesh->read("<type>.msh", _miot_gmsh_struct)`.
fn read_structural_mesh(ty: ElementType) -> Mesh {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/mesh_io/test_fe_engine")
        .join(ty.name())
        .join("inputs")
        .join(format!("{}.msh", ty.name()));
    let mut mesh = Mesh::new(ty.spatial_dimension(), "mesh").unwrap();
    read_mesh(&mut mesh, &path, MeshIoType::GmshStruct).unwrap();
    assert!(mesh.nb_element(ty, G) > 0, "{}: no element read", ty.name());
    mesh
}

/// `fem = std::make_unique<FEM>(*mesh, dim, "my_fem")` (precomputes both
/// ghost types) followed by the test's `this->fem->initShapeFunctions()`.
fn engine(mesh: &Mesh) -> StructuralFeEngine {
    let mut fem = StructuralFeEngine::with_precomputation(mesh, None, "my_fem").unwrap();
    fem.init_shape_functions(mesh, G).unwrap();
    fem
}

/// `ElementClass<type>::getNbDegreeOfFreedom()` (DOFs per node).
fn nb_degree_of_freedom(ty: ElementType) -> usize {
    ty.interpolation_type()
        .info()
        .structural
        .map(|s| s.nb_degree_of_freedom)
        .unwrap()
}

fn frobenius(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<Real>()
        .sqrt()
}

/// `globalToLocalRotation(theta)` = `{{c, s, 0}, {-s, c, 0}, {0, 0, 1}}`
/// (row-wise initialiser), stored column-major as `make_view(rot, ndof,
/// ndof)` reads it.
fn global_to_local_rotation(theta: Real) -> [Real; 9] {
    let (c, s) = (theta.cos(), theta.sin());
    [c, -s, 0., s, c, 0., 0., 0., 1.]
}

/// `TEST_F(TestBernoulliB2, PrecomputeRotations)`.
#[test]
fn test_bernoulli_b2_precompute_rotations() {
    let ty = ElementType::BernoulliBeam2;
    let mesh = read_structural_mesh(ty);
    let fem = engine(&mesh);
    let ndof = nb_degree_of_freedom(ty);
    assert_eq!(ndof, 3);
    let rot = fem.rotations(&mesh, ty, G).unwrap();
    assert_eq!(rot.nb_component(), ndof * ndof);
    let a = (4.0_f64 / 3.).atan();
    let angles = [a, -a, 0.];
    // upstream zips the rotations with the three angles: the mesh has two
    // elements, so two rotations are checked (the zip stops at the shorter)
    assert_eq!(rot.len(), 2);
    for (e, (rotation, angle)) in rot.as_slice().chunks(9).zip(angles).enumerate() {
        let rotation_error = frobenius(rotation, &global_to_local_rotation(angle));
        assert!(
            rotation_error <= TOLERANCE,
            "element {e}: |R - R(θ)| = {rotation_error:e}, R = {rotation:?}"
        );
    }
}

/// `TEST_F(TestBernoulliB3, PrecomputeRotations)`.
#[test]
fn test_bernoulli_b3_precompute_rotations() {
    let ty = ElementType::BernoulliBeam3;
    let mut mesh = read_structural_mesh(ty);
    let dim = mesh.spatial_dimension();
    // TestBernoulliB3::readMesh: getElementalData<Real>("extra_normal")
    // .alloc(0, dim, type, _not_ghost) and push_back of the normal
    let normal = [-36. / 65., -48. / 65., 5. / 13.];
    mesh.data_mut()
        .reals_mut_or_insert(EXTRA_NORMAL)
        .unwrap()
        .as_map_mut()
        .replace(ty, G, Array::from_vec(normal.to_vec(), dim).unwrap().into());
    let fem = engine(&mesh);
    let ndof = nb_degree_of_freedom(ty);
    assert_eq!(ndof, 6);
    let reference = [
        [3. / 13., 4. / 13., 12. / 13.],
        [-4. / 5., 3. / 5., 0.],
        [-36. / 65., -48. / 65., 5. / 13.],
    ];
    // solution.block<3, 3>(0, 0) = ref; solution.block<3, 3>(3, 3) = ref
    // (column-major ndof × ndof)
    let mut solution = vec![0.0; ndof * ndof];
    for i in 0..3 {
        for j in 0..3 {
            solution[i + ndof * j] = reference[i][j];
            solution[(i + 3) + ndof * (j + 3)] = reference[i][j];
        }
    }
    let rot = fem.rotations(&mesh, ty, G).unwrap();
    assert_eq!(rot.nb_component(), ndof * ndof);
    assert_eq!(rot.len(), 1);
    for rotation in rot.as_slice().chunks(ndof * ndof) {
        let rotation_error = frobenius(rotation, &solution);
        assert!(
            rotation_error <= TOLERANCE,
            "|R - R_ref| = {rotation_error:e}, R = {rotation:?}"
        );
    }
}

/// `TEST_F(TestDKT18, DISABLED_PrecomputeRotations)`: upstream prints the
/// rotations of every element of `_discrete_kirchhoff_triangle_18.msh`
/// (and is not run by default). Checked here instead: each rotation is
/// `diag(P, P)` with `P` orthonormal, `det P = +1`, first row the unit
/// direction of the element's first edge and last row the unit normal of
/// the z = 0 plate.
#[test]
fn test_dkt18_disabled_precompute_rotations() {
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    let mesh = read_structural_mesh(ty);
    let fem = engine(&mesh);
    let ndof = nb_degree_of_freedom(ty);
    assert_eq!(ndof, 6);
    let nodes = mesh.nodes().as_slice().to_vec();
    let dim = mesh.spatial_dimension();
    let conn = mesh.connectivity(ty, G).unwrap();
    assert!(nodes.chunks(dim).all(|x| x[2] == 0.), "plate not in z = 0");
    let rot = fem.rotations(&mesh, ty, G).unwrap();
    assert_eq!(rot.len(), conn.len());
    for (e, (r, c)) in rot
        .as_slice()
        .chunks(ndof * ndof)
        .zip(conn.tuples())
        .enumerate()
    {
        let p = |i: usize, j: usize| r[i + ndof * j];
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(p(i, j), p(i + 3, j + 3), "element {e}");
                assert_eq!(p(i, j + 3), 0., "element {e}");
                assert_eq!(p(i + 3, j), 0., "element {e}");
                let d: Real = (0..3).map(|k| p(i, k) * p(j, k)).sum();
                let want = if i == j { 1. } else { 0. };
                assert!((d - want).abs() < TOLERANCE, "element {e}: P Pᵀ");
            }
        }
        let det = p(0, 0) * (p(1, 1) * p(2, 2) - p(1, 2) * p(2, 1))
            - p(0, 1) * (p(1, 0) * p(2, 2) - p(1, 2) * p(2, 0))
            + p(0, 2) * (p(1, 0) * p(2, 1) - p(1, 1) * p(2, 0));
        assert!((det - 1.).abs() < TOLERANCE, "element {e}: det P = {det}");
        let (a, b) = (c[0] as usize, c[1] as usize);
        let (dx, dy) = (
            nodes[b * dim] - nodes[a * dim],
            nodes[b * dim + 1] - nodes[a * dim + 1],
        );
        let len = (dx * dx + dy * dy).sqrt();
        assert!((p(0, 0) - dx / len).abs() < TOLERANCE, "element {e}");
        assert!((p(0, 1) - dy / len).abs() < TOLERANCE, "element {e}");
        assert_eq!((p(2, 0), p(2, 1)), (0., 0.), "element {e}");
        assert_eq!(p(2, 2).abs(), 1., "element {e}");
    }
}
