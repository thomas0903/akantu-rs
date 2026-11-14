// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ported behaviour:
// src/mesh/group_manager.cc:113-148 (`createElementGroup` creates the node
// group `<name>_nodes` and the element group refers to it),
// src/mesh/element_group.cc:176-202 (element-group nodes rebuilt after
// cohesive node events).

//! Integration of the mesh-reader group semantics (every
//! element group owns the node group `<name>_nodes`, found, listed and
//! protected like a standalone node group) with the journaled v4 mesh
//! mutation: the owned node group is part of the
//! group snapshot, so a rolled-back mutation restores it — through the
//! node-group API as well as the element-group API — and a committed one
//! exposes the rebuilt node list under `<name>_nodes`.

use akantu_comm::phase::Phases;
use akantu_core::{Element, ElementType, Error, GhostType, Result};
use akantu_mesh::{
    DistributedHook, Mesh, MeshAccessor, MeshEvent, MeshPart, MeshTransaction, SharedMesh,
};

const S2: ElementType = ElementType::Segment2;
const G: GhostType = GhostType::NotGhost;

/// Bar n0 – n1 – n2 (segments e0, e1), element group "right" = {e1} (its
/// node group `right_nodes` = [1, 2]) and the standalone node group "pts".
fn bar(distributed: bool) -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [0.0, 1.0, 2.0] {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(S2, G);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
        drop(c);
        if distributed {
            a.set_distributed();
        }
    }
    let groups = m.groups_mut();
    let mut g = groups.create_element_group("right", 1).unwrap();
    g.elements
        .as_map_mut()
        .get_or_insert_with(S2, G, || akantu_core::Array::new(1).into())
        .borrow_mut()
        .push_value(1);
    g.node_group.set_nodes(vec![1, 2]);
    groups.create_node_group("pts").unwrap().set_nodes(vec![0]);
    m.compute_bounding_box().unwrap();
    m
}

/// Toy cohesive split at node 1: duplicate it, reconnect e1 to the copy,
/// record the cohesive node event (which triggers the group rebuild).
fn split(tx: &mut MeshTransaction<'_>) -> Result<()> {
    let n = tx.duplicate_node(MeshPart::Main, 1)?;
    tx.set_element_nodes(MeshPart::Main, Element::new(S2, 1, G), &[n, 2])?;
    tx.record(
        MeshPart::Main,
        MeshEvent::NodesAdded {
            nodes: vec![n],
            old_nodes: Some(vec![1]),
            origin: "test".into(),
            extension: None,
        },
    );
    Ok(())
}

fn node_groups(m: &Mesh) -> Vec<(String, Vec<i32>)> {
    m.groups()
        .node_groups()
        .map(|g| (g.name().to_owned(), g.node_list()))
        .collect()
}

/// Committed split: the element group's rebuilt nodes are those of its node
/// group `right_nodes`, through both APIs and in the node-group listing.
#[test]
fn committed_mutation_rebuilds_the_owned_node_group() {
    let shared = SharedMesh::new(bar(false));
    shared.mutate(&mut [], split, None).unwrap();
    let m = shared.read().unwrap();
    assert_eq!(m.groups().element_group("right").unwrap().nodes(), &[2, 3]);
    assert_eq!(
        node_groups(&m),
        vec![
            ("pts".to_owned(), vec![0]),
            ("right_nodes".to_owned(), vec![2, 3])
        ]
    );
}

/// A distributed hook whose preparation fails after the topology stage
/// (on a distributed mesh the topology stage rebuilds no group since
/// UD-1250: see `distributed_rebuild_is_the_hooks_and_is_rolled_back`).
struct FailingHook;

impl DistributedHook for FailingHook {
    fn prepare(&mut self, _: &Mesh, _: &[(MeshPart, MeshEvent)]) -> Result<()> {
        Err(Error::Mesh("injected hook failure".into()))
    }
    fn apply(
        &mut self,
        _: &mut MeshTransaction<'_>,
        _: &[(MeshPart, MeshEvent)],
        _: &mut Phases,
    ) -> Result<()> {
        unreachable!("prepare failed")
    }
}

/// Rolled back after the topology stage (failed hook) and after direct edits
/// of the owned node group inside the topology closure: the complete state
/// (`state_fingerprint`), `right_nodes` included, is restored, and names
/// stay protected.
#[test]
fn rolled_back_mutation_restores_the_owned_node_group() {
    // failure in stage 2 (the hook's preparation)
    let shared = SharedMesh::new(bar(true));
    let before = shared.read().unwrap().state_fingerprint();
    let mut hook = FailingHook;
    assert!(shared.mutate(&mut [], split, Some(&mut hook)).is_err());
    {
        let m = shared.read().unwrap();
        assert_eq!(m.state_fingerprint(), before);
        assert_eq!(
            m.groups().node_group("right_nodes").unwrap().node_list(),
            vec![1, 2]
        );
    }
    // direct edits through the node-group API, then a topology failure
    let shared = SharedMesh::new(bar(false));
    let before = shared.read().unwrap().state_fingerprint();
    let r = shared.mutate(
        &mut [],
        |tx| {
            let groups = tx.groups_mut(MeshPart::Main)?;
            groups.node_group_mut("right_nodes")?.extend_nodes(&[0]);
            assert!(groups.create_node_group("right_nodes").is_err());
            groups.create_element_group("left", 1)?;
            Err(Error::Mesh("injected topology failure".into()))
        },
        None,
    );
    assert!(r.is_err());
    let m = shared.read().unwrap();
    assert_eq!(m.state_fingerprint(), before);
    assert!(m.groups().node_group("left_nodes").is_err());
    assert_eq!(m.groups().element_group("right").unwrap().nodes(), &[1, 2]);
}

/// A distributed hook that rebuilds the element groups' node lists in
/// `apply` (as the cohesive hook does when upstream would send its node
/// event, UD-1250) and then, if `fail`, fails.
struct RebuildingHook {
    rebuild: bool,
    fail: bool,
}

impl DistributedHook for RebuildingHook {
    fn prepare(&mut self, _: &Mesh, _: &[(MeshPart, MeshEvent)]) -> Result<()> {
        Ok(())
    }
    fn apply(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        _: &[(MeshPart, MeshEvent)],
        _: &mut Phases,
    ) -> Result<()> {
        if self.rebuild {
            tx.rebuild_element_group_nodes(MeshPart::Main)?;
            assert_eq!(
                tx.mesh().groups().element_group("right").unwrap().nodes(),
                &[2, 3]
            );
        }
        if self.fail {
            return Err(Error::Mesh("injected failure after the rebuild".into()));
        }
        Ok(())
    }
}

/// On a distributed mesh the topology stage no longer
/// rebuilds the groups (UD-1250); the hook decides. Without the hook's
/// rebuild the node lists keep the distribution's nodes; with it they are
/// rebuilt; a hook failure after the rebuild restores them (the method is
/// journaled through the group snapshot).
#[test]
fn distributed_rebuild_is_the_hooks_and_is_rolled_back() {
    // the hook does not rebuild: the lists are unchanged (no stage-1 rebuild)
    let shared = SharedMesh::new(bar(true));
    let mut hook = RebuildingHook {
        rebuild: false,
        fail: false,
    };
    shared.mutate(&mut [], split, Some(&mut hook)).unwrap();
    assert_eq!(
        shared
            .read()
            .unwrap()
            .groups()
            .element_group("right")
            .unwrap()
            .nodes(),
        &[1, 2]
    );
    // the hook rebuilds: committed
    let shared = SharedMesh::new(bar(true));
    let mut hook = RebuildingHook {
        rebuild: true,
        fail: false,
    };
    shared.mutate(&mut [], split, Some(&mut hook)).unwrap();
    {
        let m = shared.read().unwrap();
        assert_eq!(m.groups().element_group("right").unwrap().nodes(), &[2, 3]);
        assert_eq!(
            m.groups().node_group("right_nodes").unwrap().node_list(),
            vec![2, 3]
        );
    }
    // the hook rebuilds, then fails: everything restored
    let shared = SharedMesh::new(bar(true));
    let before = shared.read().unwrap().state_fingerprint();
    let mut hook = RebuildingHook {
        rebuild: true,
        fail: true,
    };
    assert!(shared.mutate(&mut [], split, Some(&mut hook)).is_err());
    let m = shared.read().unwrap();
    assert_eq!(m.state_fingerprint(), before);
    assert_eq!(m.groups().element_group("right").unwrap().nodes(), &[1, 2]);
    assert_eq!(
        m.groups().node_group("right_nodes").unwrap().node_list(),
        vec![1, 2]
    );
}
