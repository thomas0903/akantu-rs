// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). D-105 / UD-1280:
// the solid mechanics model's record of its Neumann loads
// per boundary element (akantu_model::neumann_store) follows the mesh
// events and the writes by other paths. Mesh: upstream's
// test_cohesive/_cohesive_2d_4_triangle_3.msh (groups "sides", "loading");
// Neumann application of src/model/common/boundary_condition/
// boundary_condition_tmpl.hh.

//! Criteria (every comparison by element geometry or node, independent of
//! the numbering):
//! * the record holds exactly the loads it assembled: at every node the
//!   external force equals the sum of the recorded contributions of its
//!   elements (rounding: the record sums per element over the calls, the
//!   application per node per call);
//! * an element erasure renumbers the record (each surviving element keeps
//!   its row, bitwise) — the mutant "store not renumbered at an element
//!   erasure" fails here;
//! * a node compaction (`purify`) compacts the store's view of the external
//!   force with it (a later application sees no foreign write);
//! * an identity change (`ElementsChanged`, upstream `substituteElements`)
//!   moves the rows with their elements; a change whose numbering moves a
//!   recorded element without the lists stating it (a renumbering with
//!   empty lists) is refused before the commit,
//!   recoverable, the mesh and the record unchanged;
//! * a node reset to zero by another path forgets its entries in the
//!   records (the "zero, apply again" loop records the last application);
//!   another write is a load of other origin and keeps them (D-108);
//! * a model that never applies a Neumann load has no store.

mod model_common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real};
use akantu_mesh::{MeshEvent, MeshPart};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromTraction};
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::{model_with, read_mesh};

const MATERIAL: &str = "material elastic [\n\
\t name = body\n\
\t rho = 1e3\n\
\t E   = 1e9\n\
\t nu  = 0.2\n\
]\n";
const SEG: ElementType = ElementType::Segment2;
/// A node sums at most four contributions (two segments of each of the two
/// loaded groups) in another order than the record: a few roundings of the
/// largest nodal load.
const ROUNDING: Real = 1e-14;
const G: GhostType = GhostType::NotGhost;

fn mesh_path() -> PathBuf {
    fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_cohesive/\
         _cohesive_2d_4_triangle_3.msh",
    )
}

fn model() -> SolidMechanicsModel {
    let mut m = model_with(read_mesh(&mesh_path(), 2), MATERIAL);
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    m
}

fn loads(m: &mut SolidMechanicsModel) {
    m.apply_neumann_bc(&FromTraction::new(vec![3.0e5, -2.0e5]), "sides")
        .unwrap();
    m.apply_neumann_bc(&FromTraction::new(vec![-1.0e5, 4.0e5]), "loading")
        .unwrap();
}

/// The record's rows keyed by the coordinates (bits) of the element's nodes
/// in connectivity order.
fn rows_by_geometry(m: &SolidMechanicsModel) -> BTreeMap<Vec<[u64; 2]>, Vec<Real>> {
    let st = m.neumann_store().expect("a store");
    let mesh = m.physics.base.mesh.read().unwrap();
    let x = mesh.nodes();
    let x = x.as_slice();
    let conn = mesh.connectivity(SEG, G).unwrap();
    let block = st.forces().get(SEG, G).unwrap();
    let mut out = BTreeMap::new();
    for e in 0..block.len() {
        let key: Vec<[u64; 2]> = conn
            .tuple(e)
            .iter()
            .map(|&n| {
                let n = n as usize;
                [x[2 * n].to_bits(), x[2 * n + 1].to_bits()]
            })
            .collect();
        assert!(out.insert(key, block.tuple(e).to_vec()).is_none());
    }
    out
}

/// max over nodes |f_ext − Σ recorded contributions| / max |f_ext|, and the
/// per-node residuals (other-origin values).
fn residual(m: &SolidMechanicsModel) -> (Real, Vec<Real>) {
    let st = m.neumann_store().expect("a store");
    let mesh = m.physics.base.mesh.read().unwrap();
    let f = m.external_force().unwrap();
    let f = f.as_slice();
    let mut sum = vec![0.0; f.len()];
    for ghost in GhostType::ALL {
        for ty in st.forces().element_types(akantu_core::TypeFilter {
            spatial_dimension: None,
            ghost_type: ghost,
            kind: None,
        }) {
            let conn = mesh.connectivity(ty, ghost).unwrap();
            let block = st.forces().get(ty, ghost).unwrap();
            for e in 0..block.len() {
                for (k, &n) in conn.tuple(e).iter().enumerate() {
                    for d in 0..2 {
                        sum[2 * n as usize + d] += block.tuple(e)[2 * k + d];
                    }
                }
            }
        }
    }
    let fmax = f.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    let res: Vec<Real> = f.iter().zip(&sum).map(|(a, b)| a - b).collect();
    let worst = res.iter().fold(0.0_f64, |a, v| a.max(v.abs())) / fmax;
    (worst, res)
}

#[test]
fn no_neumann_load_no_store() {
    let mut m = model();
    m.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: SpatialDirection::Y,
        },
        "fixed",
    )
    .unwrap();
    assert!(m.neumann_store().is_none());
}

#[test]
fn the_record_holds_the_loads_it_assembled() {
    let mut m = model();
    loads(&mut m);
    let st = m.neumann_store().unwrap();
    assert_eq!(st.stats().applications, 2);
    assert_eq!(st.stats().foreign_writes, 0);
    let (worst, _) = residual(&m);
    eprintln!("record vs external force: max |Δ| / max |f| = {worst:.3e}");
    assert!(worst <= ROUNDING, "{worst:e}");
}

/// Mutant M3 ("store not renumbered at an element erasure") is killed here.
#[test]
fn an_element_erasure_renumbers_the_record() {
    let mut m = model();
    loads(&mut m);
    let before = rows_by_geometry(&m);
    // two loaded segments, the first of the block and one in its middle:
    // upstream's eraseElements fills the holes with the last elements
    let erased: Vec<Element> = {
        let st = m.neumann_store().unwrap();
        let b = st.forces().get(SEG, G).unwrap();
        let loaded: Vec<usize> = (0..b.len())
            .filter(|&e| b.tuple(e).iter().any(|v| *v != 0.0))
            .collect();
        assert!(loaded.len() > 4, "loaded segments");
        [loaded[0], loaded[loaded.len() / 2]]
            .iter()
            .map(|&e| Element::new(SEG, e as Idx, G))
            .collect()
    };
    let erased_keys: Vec<Vec<[u64; 2]>> = {
        let mesh = m.physics.base.mesh.read().unwrap();
        let x = mesh.nodes();
        let conn = mesh.connectivity(SEG, G).unwrap();
        erased
            .iter()
            .map(|e| {
                conn.tuple(e.idx())
                    .iter()
                    .map(|&n| {
                        let n = n as usize;
                        [
                            x.as_slice()[2 * n].to_bits(),
                            x.as_slice()[2 * n + 1].to_bits(),
                        ]
                    })
                    .collect()
            })
            .collect()
    };
    m.mutate_mesh(|tx| tx.erase_elements(MeshPart::Main, &erased), None)
        .unwrap();
    let n = m.physics.base.mesh.read().unwrap().nb_element(SEG, G);
    let block_len = m
        .neumann_store()
        .unwrap()
        .forces()
        .get(SEG, G)
        .unwrap()
        .len();
    assert_eq!(block_len, n, "one record per element after the erasure");
    let after = rows_by_geometry(&m);
    assert_eq!(after.len(), before.len() - 2);
    for (key, row) in &after {
        assert_eq!(
            before.get(key),
            Some(row),
            "a surviving element keeps its record"
        );
    }
    for k in &erased_keys {
        assert!(!after.contains_key(k), "an erased element has no record");
    }
    // a later application adds to the surviving rows (no foreign write: the
    // erasure does not touch the external force)
    loads(&mut m);
    assert_eq!(m.neumann_store().unwrap().stats().foreign_writes, 0);
}

#[test]
fn a_node_compaction_compacts_the_store_view() {
    let mut m = model();
    loads(&mut m);
    let committed = m.mutate_mesh(|tx| tx.purify(MeshPart::Main), None).unwrap();
    let permuted = committed.events.iter().any(|(p, e)| {
        *p == MeshPart::Main
            && matches!(e, MeshEvent::NodesRemoved { new_numbering, .. }
                if new_numbering.iter().enumerate().any(|(i, &n)| n != i as Idx))
    });
    assert!(
        permuted,
        "purify must renumber the nodes for the test to bite"
    );
    loads(&mut m);
    let st = m.neumann_store().unwrap();
    assert_eq!(st.stats().applications, 4);
    assert_eq!(
        st.stats().foreign_writes,
        0,
        "the store's view followed the compaction"
    );
    let (worst, _) = residual(&m);
    assert!(worst <= ROUNDING, "{worst:e}");
}

#[test]
fn an_identity_change_moves_the_records() {
    let mut m = model();
    loads(&mut m);
    let before = rows_by_geometry(&m);
    let (a, b) = {
        let st = m.neumann_store().unwrap();
        let blk = st.forces().get(SEG, G).unwrap();
        let loaded: Vec<usize> = (0..blk.len())
            .filter(|&e| blk.tuple(e).iter().any(|v| *v != 0.0))
            .collect();
        // a loaded and an unloaded segment exchange their identities
        let unloaded = (0..blk.len())
            .find(|e| !loaded.contains(e))
            .expect("an unloaded segment");
        (
            Element::new(SEG, loaded[0] as Idx, G),
            Element::new(SEG, unloaded as Idx, G),
        )
    };
    m.mutate_mesh(
        |tx| {
            let ca = tx.mesh().connectivity(SEG, G)?.tuple(a.idx()).to_vec();
            let cb = tx.mesh().connectivity(SEG, G)?.tuple(b.idx()).to_vec();
            tx.set_element_nodes(MeshPart::Main, a, &cb)?;
            tx.set_element_nodes(MeshPart::Main, b, &ca)?;
            let n = tx.mesh().nb_element(SEG, G);
            let mut perm: Vec<Idx> = (0..n).map(|i| i as Idx).collect();
            perm.swap(a.idx(), b.idx());
            let mut new_numbering = ElementTypeMapArray::new("new_numbering");
            new_numbering
                .as_map_mut()
                .insert(SEG, G, akantu_core::Array::from_vec(perm, 1)?)?;
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsChanged {
                    old: vec![a, b],
                    new: vec![b, a],
                    new_numbering,
                    origin: "neumann store test".into(),
                },
            );
            Ok(())
        },
        None,
    )
    .unwrap();
    assert_eq!(rows_by_geometry(&m), before, "each row follows its element");
}

/// An `ElementsChanged` with
/// empty lists and a permuting numbering reaches every consumer. The mesh
/// does not apply a change's numbering, so the record cannot follow it: the
/// solid model refuses it in the check stage, before the commit
/// (recoverable; the mesh, its revision and the record are unchanged), and
/// the same mesh accepts the next mutation.
#[test]
fn a_renumbering_change_without_lists_is_refused_before_the_commit() {
    let mut m = model();
    loads(&mut m);
    let before = rows_by_geometry(&m);
    let revision = m.physics.base.mesh.revision().unwrap();
    let n = m.physics.base.mesh.read().unwrap().nb_element(SEG, G);
    let e = m
        .mutate_mesh(
            |tx| {
                let mut perm: Vec<Idx> = (0..n).map(|i| i as Idx).collect();
                perm.swap(0, n - 1);
                let mut new_numbering = ElementTypeMapArray::new("new_numbering");
                new_numbering.as_map_mut().insert(
                    SEG,
                    G,
                    akantu_core::Array::from_vec(perm, 1)?,
                )?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsChanged {
                        old: vec![],
                        new: vec![],
                        new_numbering,
                        origin: "neumann store test".into(),
                    },
                );
                Ok(())
            },
            None,
        )
        .expect_err("refused before the commit");
    assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
    assert!(!e.is_fatal(), "recoverable: {e:?}");
    assert!(format!("{e}").contains("Neumann store"), "{e}");
    assert_eq!(m.physics.base.mesh.revision().unwrap(), revision);
    assert_eq!(rows_by_geometry(&m), before, "the record is unchanged");
    // the domain is not poisoned: a later mutation commits
    m.mutate_mesh(|tx| tx.purify(MeshPart::Main), None).unwrap();
    loads(&mut m);
    let (worst, _) = residual(&m);
    assert!(worst <= ROUNDING, "{worst:e}");
}

#[test]
fn a_reset_forgets_and_a_user_force_does_not() {
    // "zero the external force, apply the loads again": the record is the
    // last application's
    let mut once = model();
    loads(&mut once);
    let single = rows_by_geometry(&once);
    let mut m = model();
    loads(&mut m);
    m.external_force_mut().unwrap().as_mut_slice().fill(0.0);
    loads(&mut m);
    let st = m.neumann_store().unwrap().stats();
    assert!(st.reset_nodes > 0 && st.forgotten_entries > 0, "{st:?}");
    assert_eq!(rows_by_geometry(&m), single, "the last application only");
    let (worst, _) = residual(&m);
    assert!(worst <= ROUNDING, "{worst:e}");

    // a nodal force written by the user is of other origin: the records stay
    let mut u = model();
    loads(&mut u);
    let node = {
        let blk = u.neumann_store().unwrap().forces().get(SEG, G).unwrap();
        let e = (0..blk.len())
            .find(|&e| blk.tuple(e).iter().any(|v| *v != 0.0))
            .unwrap();
        let mesh = u.physics.base.mesh.read().unwrap();
        mesh.connectivity(SEG, G).unwrap().tuple(e)[0] as usize
    };
    let p = [1.0e3, 7.0e2];
    {
        let mut f = u.external_force_mut().unwrap();
        f.as_mut_slice()[2 * node] += p[0];
        f.as_mut_slice()[2 * node + 1] += p[1];
    }
    loads(&mut u);
    let st = u.neumann_store().unwrap().stats();
    assert_eq!(
        (st.foreign_writes, st.reset_nodes, st.forgotten_entries),
        (1, 0, 0)
    );
    // D-108: a node written while it holds a record is marked (a later move
    // from it is counted and warned)
    assert!(u.neumann_store().unwrap().is_written(node));
    let doubled: BTreeMap<_, Vec<Real>> = single
        .iter()
        .map(|(k, r)| (k.clone(), r.iter().map(|v| v + v).collect()))
        .collect();
    let got = rows_by_geometry(&u);
    for (k, r) in &got {
        for (a, b) in r.iter().zip(&doubled[k]) {
            assert!((a - b).abs() <= 1e-15 * b.abs().max(1.0), "{a} vs {b}");
        }
    }
    let (_, res) = residual(&u);
    for d in 0..2 {
        assert!(
            (res[2 * node + d] - p[d]).abs() <= 1e-9,
            "the user's force is the only other-origin value: {} vs {}",
            res[2 * node + d],
            p[d]
        );
    }
}
