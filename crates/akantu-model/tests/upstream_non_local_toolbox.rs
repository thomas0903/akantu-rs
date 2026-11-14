// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// test_non_local_toolbox/{my_model.hh, test_non_local_neighborhood_base.cc,
// test_weight_computation.cc} with their `.verified` reference outputs.

//! TEST-075 `test_non_local_neighborhood_base` and TEST-076
//! `test_weight_computation`: upstream's minimal model (`MyModel`: one
//! neighbourhood `test_region`, every integration point inserted with the
//! null element index and a running point number) on `plate.msh`; the pair
//! list, the neighbour coordinates and the weights printed by the manager
//! are compared with the upstream `.verified` outputs as the upstream test
//! driver does (`diff -biw`).

use std::path::PathBuf;

use akantu_core::element_type_map::TypeFilter;
use akantu_core::enums::MeshIoType;
use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_io::read_mesh;
use akantu_mesh::Mesh;
use akantu_model::non_local::{NoGhosts, NonLocalManager, NonLocalManagerCallback, QuadPoint};
use akantu_parser::Parser;

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/upstream/non_local")
        .join(name)
}

/// Upstream `diff -biw` equality of two texts (line by line, white space
/// and case ignored).
fn diff_biw(out: &str, reference: &str) -> std::result::Result<(), String> {
    let norm = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase()
            })
            .collect()
    };
    let (a, b) = (norm(out), norm(reference));
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        if x != y {
            return Err(format!("line {}: {x:?} vs reference {y:?}", k + 1));
        }
    }
    if a.len() != b.len() {
        return Err(format!("{} lines vs {} reference lines", a.len(), b.len()));
    }
    Ok(())
}

/// Upstream `MyModel` (`my_model.hh`): the integration-point coordinates
/// of every `(type, ghost)` block of the model's dimension.
struct MyModel {
    dim: usize,
    blocks: Vec<(ElementType, GhostType, Array<Real>)>,
}

impl MyModel {
    fn new(mesh: &Mesh, fe: &FeEngine) -> Self {
        let dim = mesh.spatial_dimension();
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            let filter = TypeFilter::new()
                .dim(dim)
                .ghost(g)
                .kind(ElementKind::Regular);
            for ty in mesh.element_types(filter).collect::<Vec<_>>() {
                let mut x = Array::new(dim);
                fe.integration_points_coordinates(mesh, &mut x, ty, g, None)
                    .unwrap();
                blocks.push((ty, g, x));
            }
        }
        Self { dim, blocks }
    }
}

impl NonLocalManagerCallback for MyModel {
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> Result<()> {
        // upstream: `q.global_num = 0` once, then a running number over
        // every type; `q.element` keeps the null element's index -1;
        // `q.num_point` restarts at 0 for every type
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
        _: &mut ElementTypeMapArray<Real>,
    ) -> Result<()> {
        Ok(())
    }
    fn update_non_local_internal(
        &mut self,
        _: &str,
        _: GhostType,
        _: &ElementTypeMapArray<Real>,
        _: bool,
    ) -> Result<()> {
        Ok(())
    }
    fn compute_non_local_contribution(&mut self, _: GhostType) -> Result<()> {
        Ok(())
    }
}

/// `MyModel(mesh, 2)` with the input file `dat`: register `test_region`,
/// initialise the shape functions and the manager.
fn my_model(dat: &str) -> NonLocalManager {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut mesh, &data("plate.msh"), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_file(data(dat)).unwrap();
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "FEEngine",
    )
    .unwrap();
    let mut manager = NonLocalManager::new("non_local_manager", 2);
    manager.read_sections(&parser.root, parser.is_permissive());
    manager.set_grid_center_from(&mesh).unwrap();
    manager
        .register_neighborhood("test_region", "test_region")
        .unwrap();
    let mut model = MyModel::new(&mesh, &fe);
    manager
        .initialize(&mesh, &fe, &mut model, &mut NoGhosts)
        .unwrap();
    manager
}

#[test]
fn test_non_local_neighborhood_base() {
    let manager = my_model("material.dat");
    let nh = manager.neighborhood("test_region").unwrap();
    assert_eq!(nh.radius(), 0.51);
    assert_eq!(nh.weight_function().update_rate(), 1);
    let out = nh.save_pairs_text() + &nh.save_neighbor_coords_text();
    let reference =
        std::fs::read_to_string(data("test_non_local_neighborhood_base.verified")).unwrap();
    diff_biw(&out, &reference).unwrap();
    // the pair mesh of upstream savePairs: two segments per distinct pair
    let (nodes, conn, tags) = nh.pairs_mesh_arrays();
    let pairs = nh.pair_list(GhostType::NotGhost);
    let self_pairs = pairs.iter().filter(|(i, j)| i == j).count();
    assert_eq!(conn.len(), 2 * pairs.len() - self_pairs);
    assert_eq!(nodes.len(), 2 * 64);
    assert_eq!(tags.len(), conn.len());
}

#[test]
fn test_weight_computation() {
    let manager = my_model("material_weight_computation.dat");
    let nh = manager.neighborhood("test_region").unwrap();
    let out = nh.save_weights_text().unwrap();
    let reference = std::fs::read_to_string(data("test_weight_computation.verified")).unwrap();
    diff_biw(&out, &reference).unwrap();
    // independent check: the normalised weights of every point sum to one
    // (partition of unity). Each of the n weights of a point is
    // `w_k·(1/V)` with `V = Σ w_k` summed in the same pair order: the
    // reciprocal and each product round once (≤ ε/2 relative each) and the
    // n-term sum adds ≤ (n − 1)·ε/2, so |Σ − 1| ≤ (n + 1)·ε.
    let n = nh.points().len();
    let (mut sum, mut count) = (vec![0.; n], vec![0usize; n]);
    for (w, &(i, j)) in nh
        .pair_weights(GhostType::NotGhost)
        .iter()
        .zip(nh.pair_list(GhostType::NotGhost))
    {
        sum[i] += w[0];
        count[i] += 1;
        if i != j {
            sum[j] += w[1];
            count[j] += 1;
        }
    }
    for (s, c) in sum.iter().zip(&count) {
        #[allow(clippy::cast_precision_loss)] // small neighbour counts
        let tol = (*c as Real + 1.) * Real::EPSILON;
        assert!(
            (s - 1.).abs() <= tol,
            "partition of unity: {s} ({c} neighbours)"
        );
    }
}
