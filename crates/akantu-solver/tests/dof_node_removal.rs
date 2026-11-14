// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the DOF
// manager's node removal (SMM-015, UD-209): upstream's
// DOFManager::onNodesRemoved is empty (dof_manager.cc:901-902); the port
// renumbers as a fresh registration on the current mesh (registerDOFs /
// updateDOFsData, dof_manager.cc:590-626, :768-870) and compacts the arrays
// as Mesh::removeNodesFromArray (mesh_inline_impl.hh:126-145).

//! The criterion of every test: after a removal the manager **equals** a
//! manager registered afresh on the mesh after the removal (layout, local
//! and global equations, associated nodes, flags, periodic aliases), with
//! the surviving values (every array follows its node) and the
//! per-equation state (residual, lumped matrices, blocked snapshot)
//! translated to the new equations.

use std::time::Duration;

use akantu_comm::{Communicator, Synchronizer, run_local};
use akantu_core::ElementType;
use akantu_core::enums::{EventPriority, MatrixType};
use akantu_core::{Error, GhostType, Idx, NodeFlag, Real};
use akantu_mesh::distributed_removal::DistributedRemovalHook;
use akantu_mesh::{Mesh, MeshAccessor, MeshEventHandler, MeshPart, SharedMesh};
use akantu_solver::DofManager;

const SEG: ElementType = ElementType::Segment2;

/// 1D mesh: nodes at `x = 0, 1, …, nb_nodes − 1`, segments `segs`.
fn line(nb_nodes: usize, segs: &[[Idx; 2]]) -> Mesh {
    let mut mesh = Mesh::new(1, "line").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for i in 0..nb_nodes {
            a.push_node(&[i as Real]).unwrap();
        }
        for s in segs {
            a.connectivity_mut(SEG, GhostType::NotGhost).push_tuple(s);
        }
    }
    mesh
}

/// Families of every test: `u` (2 components, whole mesh, derivatives 1
/// and 2, blocked, previous, increment) and `lambda` (generic, 3 × 1).
fn register(dm: &mut DofManager, mesh: &Mesh) {
    dm.register_nodal_dofs(mesh, "u", 2).unwrap();
    dm.register_generic_dofs("lambda", 3, 1).unwrap();
    dm.register_dofs_derivative("u", 1).unwrap();
    dm.register_dofs_derivative("u", 2).unwrap();
    dm.register_blocked_dofs("u").unwrap();
    dm.register_dofs_previous("u").unwrap();
    dm.register_dofs_increment("u").unwrap();
}

/// Distinct values in every array of `u` (value of DOF `k` of array `a`:
/// `100 a + k`), blocked flags on every third DOF, `lambda = [7, 8, 9]`,
/// residual and the lumped matrix `M` = 1000 + local equation.
fn fill(dm: &mut DofManager) {
    let fill_with = |s: &mut [Real], a: Real| {
        for (k, v) in s.iter_mut().enumerate() {
            *v = 100.0 * a + k as Real;
        }
    };
    fill_with(dm.dofs_mut("u").unwrap().as_mut_slice(), 1.0);
    fill_with(dm.dofs_derivative_mut("u", 1).unwrap().as_mut_slice(), 2.0);
    fill_with(dm.dofs_derivative_mut("u", 2).unwrap().as_mut_slice(), 3.0);
    {
        let f = dm.family_mut("u").unwrap();
        fill_with(
            f.previous.as_mut().unwrap().write().unwrap().as_mut_slice(),
            4.0,
        );
        fill_with(
            f.increment
                .as_mut()
                .unwrap()
                .write()
                .unwrap()
                .as_mut_slice(),
            5.0,
        );
        fill_with(f.solution.as_mut_slice(), 6.0);
    }
    for (k, b) in dm
        .blocked_dofs_mut("u")
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *b = k % 3 == 0;
    }
    dm.dofs_mut("lambda")
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&[7.0, 8.0, 9.0]);
    let n = dm.local_system_size();
    for (e, r) in dm.residual_mut().as_mut_slice().iter_mut().enumerate() {
        *r = 1000.0 + e as Real;
    }
    let m = dm.new_lumped_matrix("M").unwrap();
    m.values = (0..n).map(|e| 1000.0 + e as Real).collect();
}

/// The numbering of `a` equals the one of `b` (a fresh registration).
fn assert_same_numbering(a: &DofManager, b: &DofManager, ids: &[&str]) {
    assert_eq!(a.layout(), b.layout(), "layout");
    assert_eq!(a.system_size(), b.system_size());
    assert_eq!(a.local_system_size(), b.local_system_size());
    assert_eq!(a.pure_local_system_size(), b.pure_local_system_size());
    for id in ids {
        let (fa, fb) = (a.family(id).unwrap(), b.family(id).unwrap());
        assert_eq!(
            fa.local_equation_numbers, fb.local_equation_numbers,
            "{id}: equations"
        );
        assert_eq!(fa.associated_nodes, fb.associated_nodes, "{id}: nodes");
        // the node global ids of the trace dump follow the renumbering
        assert_eq!(
            fa.associated_node_gids, fb.associated_node_gids,
            "{id}: node global ids"
        );
        assert_eq!(
            fa.periodic_slave, fb.periodic_slave,
            "{id}: periodic slaves"
        );
        assert_eq!(fa.node_to_tuple, fb.node_to_tuple, "{id}: node → tuple");
        assert_eq!(fa.dofs.len(), fb.dofs.len(), "{id}: tuples");
        assert_eq!(
            a.global_equation_numbers(id).unwrap(),
            b.global_equation_numbers(id).unwrap(),
            "{id}: global equations"
        );
    }
    for l in 0..a.local_system_size() {
        assert_eq!(
            a.dof_flag(l).unwrap(),
            b.dof_flag(l).unwrap(),
            "flag of {l}"
        );
    }
}

/// Rows of `old` (`nbc` components) at their new index.
fn moved<T: Copy + Default>(old: &[T], nbc: usize, numbering: &[Idx]) -> Vec<T> {
    let kept = numbering.iter().filter(|&&n| n >= 0).count();
    let mut out = vec![T::default(); kept * nbc];
    for (i, &n) in numbering.iter().enumerate() {
        if let Ok(n) = usize::try_from(n) {
            out[n * nbc..(n + 1) * nbc].copy_from_slice(&old[i * nbc..(i + 1) * nbc]);
        }
    }
    out
}

/// Every array of `u` and `lambda` of `dm` after the removal is the old
/// one moved with `numbering`; the residual and `M` follow the equations.
fn assert_arrays_followed(dm: &DofManager, before: &Snapshot, numbering: &[Idx]) {
    let u = dm.family("u").unwrap();
    assert_eq!(
        dm.dofs("u").unwrap().as_slice(),
        &moved(&before.u, 2, numbering)[..]
    );
    assert_eq!(
        dm.dofs_derivative("u", 1).unwrap().as_slice(),
        &moved(&before.v, 2, numbering)[..]
    );
    assert_eq!(
        dm.dofs_derivative("u", 2).unwrap().as_slice(),
        &moved(&before.a, 2, numbering)[..]
    );
    assert_eq!(
        u.previous.as_ref().unwrap().read().unwrap().as_slice(),
        &moved(&before.previous, 2, numbering)[..]
    );
    assert_eq!(
        u.increment.as_ref().unwrap().read().unwrap().as_slice(),
        &moved(&before.increment, 2, numbering)[..]
    );
    assert_eq!(
        u.solution.as_slice(),
        &moved(&before.solution, 2, numbering)[..]
    );
    assert_eq!(
        dm.blocked_dofs("u").unwrap().as_slice(),
        &moved(&before.blocked, 2, numbering)[..]
    );
    assert_eq!(dm.dofs("lambda").unwrap().as_slice(), &[7.0, 8.0, 9.0]);
    // per-equation state: the value of the old equation of each surviving
    // DOF sits at its new equation
    for id in ["u", "lambda"] {
        let new_eqs = dm.local_equation_numbers(id).unwrap();
        let old_eqs = &before.eqs[id];
        let map: Vec<Idx> = if id == "u" {
            moved(old_eqs, 2, numbering)
        } else {
            old_eqs.clone()
        };
        for (k, (&ne, &oe)) in new_eqs.iter().zip(&map).enumerate() {
            let (ne, oe) = (ne as usize, oe as usize);
            assert_eq!(
                dm.residual().values()[ne],
                1000.0 + oe as Real,
                "{id} dof {k}: residual"
            );
            assert_eq!(
                dm.lumped_matrix("M").unwrap().values[ne],
                1000.0 + oe as Real,
                "{id} dof {k}: lumped"
            );
        }
    }
    assert_eq!(dm.residual().local_size(), dm.local_system_size());
}

struct Snapshot {
    u: Vec<Real>,
    v: Vec<Real>,
    a: Vec<Real>,
    previous: Vec<Real>,
    increment: Vec<Real>,
    solution: Vec<Real>,
    blocked: Vec<bool>,
    eqs: std::collections::HashMap<&'static str, Vec<Idx>>,
}

fn snapshot(dm: &DofManager) -> Snapshot {
    let u = dm.family("u").unwrap();
    Snapshot {
        u: dm.dofs("u").unwrap().as_slice().to_vec(),
        v: dm.dofs_derivative("u", 1).unwrap().as_slice().to_vec(),
        a: dm.dofs_derivative("u", 2).unwrap().as_slice().to_vec(),
        previous: u
            .previous
            .as_ref()
            .unwrap()
            .read()
            .unwrap()
            .as_slice()
            .to_vec(),
        increment: u
            .increment
            .as_ref()
            .unwrap()
            .read()
            .unwrap()
            .as_slice()
            .to_vec(),
        solution: u.solution.as_slice().to_vec(),
        blocked: dm.blocked_dofs("u").unwrap().as_slice().to_vec(),
        eqs: [
            ("u", dm.local_equation_numbers("u").unwrap().to_vec()),
            (
                "lambda",
                dm.local_equation_numbers("lambda").unwrap().to_vec(),
            ),
        ]
        .into_iter()
        .collect(),
    }
}

/// Remove the unused nodes of `mesh` (purify numbering: first appearance)
/// through the set-up API and the manager; returns the numbering.
fn purify(mesh: &mut Mesh, dm: &mut DofManager) -> Vec<Idx> {
    let numbering = akantu_mesh::utils::purify_numbering(mesh).unwrap();
    let events = akantu_mesh::removal::remove_nodes(mesh, &numbering, "test").unwrap();
    for e in events.events() {
        dm.on_mesh_event(mesh, e).unwrap();
    }
    numbering
}

/// Unused nodes 2 and 5 removed, and the purify numbering permutes the
/// others (the first segment uses nodes 6 and 7): the manager equals a
/// fresh registration, every array followed its node, the per-equation
/// state its equation; the matrices were reset.
#[test]
fn removal_equals_a_fresh_registration() {
    let segs = [[6, 7], [0, 1], [1, 3], [3, 4], [4, 6]];
    let mut mesh = line(8, &segs);
    let mut dm = DofManager::new("dm", Communicator::serial());
    register(&mut dm, &mesh);
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    fill(&mut dm);
    dm.update_global_blocked_dofs();
    let before = snapshot(&dm);
    let numbering = purify(&mut mesh, &mut dm);
    assert_eq!(numbering, vec![2, 3, -1, 4, 5, -1, 0, 1]);
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    register(&mut fresh, &mesh);
    assert_same_numbering(&dm, &fresh, &["u", "lambda"]);
    assert_arrays_followed(&dm, &before, &numbering);
    // the blocked snapshot names the new equations: the fresh manager's
    // snapshot of the same (compacted) flags is the same set
    fresh
        .blocked_dofs_mut("u")
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(dm.blocked_dofs("u").unwrap().as_slice());
    fresh.update_global_blocked_dofs();
    assert_eq!(
        dm.global_blocked_dofs_indexes(),
        fresh.global_blocked_dofs_indexes()
    );
    assert_eq!(dm.global_blocked_dofs(), fresh.global_blocked_dofs());
    // matrices: profile and values reset (order = new system size)
    let k = dm.matrix("K").unwrap();
    assert_eq!(k.nnz(), 0);
    assert_eq!(k.size(), dm.system_size());
}

/// The node global ids recorded for the Newton trace
/// dump (`DofData::associated_node_gids`, DOF-017) follow the removal: the
/// mesh-free `save_associated_nodes_numbered` writes the same file as a
/// fresh registration's `save_associated_nodes(mesh)`, which reads the
/// mesh (before the fix the family kept the old, longer id vector).
#[test]
fn removal_renumbers_the_traced_node_ids() {
    use akantu_linalg::matrix_market::MatrixMarketOptions;
    let segs = [[6, 7], [0, 1], [1, 3], [3, 4], [4, 6]];
    let mut mesh = line(8, &segs);
    let mut dm = DofManager::new("dm", Communicator::serial());
    register(&mut dm, &mesh);
    let old: Vec<Real> = dm.family("u").unwrap().associated_node_gids.clone();
    assert_eq!(old.len(), 16);
    let numbering = purify(&mut mesh, &mut dm);
    let gids = dm.family("u").unwrap().associated_node_gids.clone();
    let expected: Vec<Real> = (0..6)
        .flat_map(|n| [Real::from(n), Real::from(n)])
        .collect();
    assert_eq!(gids, expected, "after {numbering:?}");
    let dir = std::env::temp_dir().join(format!("akantu-ls07-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let numbered = dm
        .save_associated_nodes_numbered(&dir.join("numbered"), &MatrixMarketOptions::default())
        .unwrap();
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    register(&mut fresh, &mesh);
    fresh
        .save_associated_nodes(&mesh, &dir.join("from_mesh"))
        .unwrap();
    let from_mesh = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().contains("from_mesh"))
        .unwrap();
    let a = std::fs::read(&numbered).unwrap();
    let b = std::fs::read(&from_mesh).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        a == b,
        "numbered:\n{}\nfrom the mesh:\n{}",
        String::from_utf8_lossy(&a),
        String::from_utf8_lossy(&b)
    );
}

/// A removal after a node addition (two numbering blocks: registration,
/// then insertion) collapses the blocks: the result is the fresh
/// registration on the current mesh.
#[test]
fn removal_after_insertion_collapses_the_blocks() {
    let mut mesh = line(6, &[[0, 1], [1, 3], [3, 4]]);
    let mut dm = DofManager::new("dm", Communicator::serial());
    register(&mut dm, &mesh);
    // insertion: node 6 (copy of 4) used by a new segment
    let n = {
        let mut a = MeshAccessor::new(&mut mesh);
        let n = a.push_node(&[4.0]).unwrap();
        a.connectivity_mut(SEG, GhostType::NotGhost)
            .push_tuple(&[n, 5]);
        n
    };
    dm.on_nodes_added(&mesh, &[n]).unwrap();
    // node 5 is used now, node 2 is not
    fill(&mut dm);
    let before = snapshot(&dm);
    let numbering = purify(&mut mesh, &mut dm);
    assert_eq!(numbering, vec![0, 1, -1, 2, 3, 5, 4]);
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    register(&mut fresh, &mesh);
    assert_same_numbering(&dm, &fresh, &["u", "lambda"]);
    assert_arrays_followed(&dm, &before, &numbering);
}

/// A family supported by an element group takes the group's node order in
/// the renumbered mesh, as a registration on the group does.
#[test]
fn group_family_follows_its_group() {
    let mut mesh = line(8, &[[6, 7], [0, 1], [1, 3], [3, 4], [4, 6]]);
    mesh.groups_mut().create_element_group("g", 1).unwrap();
    for n in [6, 3, 4, 7] {
        mesh.groups_mut()
            .element_group_mut("g")
            .unwrap()
            .add_node(n, true);
    }
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    dm.register_nodal_dofs_on_group(&mesh, "w", 1, "g").unwrap();
    dm.register_blocked_dofs("w").unwrap();
    for (k, v) in dm
        .dofs_mut("w")
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *v = 10.0 + k as Real; // tuples of 6, 3, 4, 7
    }
    let numbering = purify(&mut mesh, &mut dm);
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh.register_nodal_dofs(&mesh, "u", 2).unwrap();
    fresh
        .register_nodal_dofs_on_group(&mesh, "w", 1, "g")
        .unwrap();
    assert_same_numbering(&dm, &fresh, &["u", "w"]);
    // each value stayed with its node
    let w = dm.family("w").unwrap();
    for (old, value) in [(6, 10.0), (3, 11.0), (4, 12.0), (7, 13.0)] {
        let new = numbering[old];
        let t = w.node_to_tuple.as_ref().unwrap()[&new];
        assert_eq!(
            dm.dofs("w").unwrap().as_slice()[t],
            value,
            "node {old} → {new}"
        );
    }
}

/// Periodic pairs: a removed periodic slave drops its pair; a used slave
/// whose master is removed becomes a regular node with its own equation
/// (the mesh clears its periodic flag), exactly as in a fresh registration.
#[test]
fn periodic_aliases_are_rebuilt() {
    // used: 0..3; 4 unused slave of 1; 3 used slave of 5 (unused master);
    // 2 used slave of 0
    let mut mesh = line(6, &[[0, 1], [1, 2], [2, 3]]);
    {
        let mut a = MeshAccessor::new(&mut mesh);
        a.add_periodic_pair(4, 1).unwrap();
        a.add_periodic_pair(3, 5).unwrap();
        a.add_periodic_pair(2, 0).unwrap();
    }
    let mut dm = DofManager::new("dm", Communicator::serial());
    register(&mut dm, &mesh);
    fill(&mut dm);
    let before = snapshot(&dm);
    let numbering = purify(&mut mesh, &mut dm);
    assert_eq!(numbering, vec![0, 1, 2, 3, -1, -1]);
    assert!(mesh.periodic_master(2).is_some() && mesh.periodic_master(3).is_none());
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    register(&mut fresh, &mesh);
    assert_same_numbering(&dm, &fresh, &["u", "lambda"]);
    assert_arrays_followed(&dm, &before, &numbering);
}

/// Through `SharedMesh::mutate` with the manager as the consumer: a pure
/// renumbering (`purify` of a mesh whose nodes are all used, recorded and
/// dispatched although nothing is removed) renumbers too.
#[test]
fn purify_through_a_mutation_renumbers() {
    let mesh = line(4, &[[3, 2], [2, 1], [1, 0]]);
    let shared = SharedMesh::new(mesh);
    let mut dm = DofManager::new("dm", Communicator::serial());
    register(&mut dm, &shared.read().unwrap());
    fill(&mut dm);
    let before = snapshot(&dm);
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| tx.purify(MeshPart::Main),
            None,
        )
        .unwrap();
    let mesh = shared.read().unwrap();
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    register(&mut fresh, &mesh);
    assert_same_numbering(&dm, &fresh, &["u", "lambda"]);
    assert_arrays_followed(&dm, &before, &[3, 2, 1, 0]);
}

/// Composition inside one mutation (the manager receives every event with
/// the final mesh, ARCHITECTURE): removal then addition is accepted;
/// an addition whose nodes a later removal renumbers is refused before the
/// commit (recoverable, nothing changed).
#[test]
fn composed_node_events() {
    use akantu_mesh::MeshEvent;
    let shared = SharedMesh::new(line(5, &[[0, 1], [1, 3], [3, 4]]));
    let mut dm = DofManager::new("dm", Communicator::serial());
    register(&mut dm, &shared.read().unwrap());
    fill(&mut dm);
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    // remove node 2, then append a node: [0, 1, 3, 4, new]
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| {
                tx.remove_nodes(MeshPart::Main, &[0, 1, -1, 2, 3])?;
                let n = tx.push_node(MeshPart::Main, &[5.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    {
        let mesh = shared.read().unwrap();
        assert_eq!(mesh.nb_nodes(), 5);
        assert_eq!(dm.dofs("u").unwrap().len(), 5);
        // fresh registration of the 4 kept nodes, then the appended block
        assert_eq!(
            dm.local_equation_numbers("u").unwrap(),
            &[0, 1, 2, 3, 4, 5, 6, 7, 11, 12]
        );
        assert_eq!(dm.local_equation_numbers("lambda").unwrap(), &[8, 9, 10]);
        assert_eq!(
            &dm.dofs("u").unwrap().as_slice()[4..8],
            &[106.0, 107.0, 108.0, 109.0]
        );
        assert_eq!(&dm.dofs("u").unwrap().as_slice()[8..], &[0.0, 0.0]);
    }
    // addition then a removal renumbering the added node: refused
    let layout = dm.layout().clone();
    let revision = shared.revision().unwrap();
    let r = shared.mutate(
        &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[6.0])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "test".into(),
                    extension: None,
                },
            );
            // remove node 4 (unused): the added node 5 moves to 4
            tx.remove_nodes(MeshPart::Main, &[0, 1, 2, 3, -1, 4])
        },
        None,
    );
    let Err(e) = &r else {
        panic!("expected the composition refusal, got {r:?}")
    };
    assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(dm.layout(), &layout);
}

// ------------------------------------------------------------ distributed

/// The 1D bar of `dof_numbering_distributed.rs` on 3 ranks (elements 0-1 on
/// rank 0, 2-3 on rank 1, rank 2 empty) plus one **shared node used by no
/// element**: global node g5 at `x = 2.5`, mastered by rank 0 (local 4) and
/// slave on rank 1 (local 4), in the node synchronizer's schemes. Removing
/// it on both ranks is a removal on the rank boundary.
///
/// | rank | local nodes |
/// |---|---|
/// | 0 | g0, g1, g2 (master), g3 (pure ghost), g5 (master) |
/// | 1 | g2 (slave), g3, g4, g1 (pure ghost), g5 (slave) |
fn boundary_mesh(comm: &Communicator) -> Mesh {
    let mut mesh = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
    let r = comm.rank();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let (xs, flags): (Vec<Real>, Vec<NodeFlag>) = match r {
            0 => (
                vec![0., 1., 2., 3., 2.5],
                vec![
                    NodeFlag::NORMAL,
                    NodeFlag::NORMAL,
                    NodeFlag::MASTER,
                    NodeFlag::PURE_GHOST,
                    NodeFlag::MASTER,
                ],
            ),
            1 => (
                vec![2., 3., 4., 1., 2.5],
                vec![
                    NodeFlag::SLAVE,
                    NodeFlag::NORMAL,
                    NodeFlag::NORMAL,
                    NodeFlag::PURE_GHOST,
                    NodeFlag::SLAVE,
                ],
            ),
            _ => (vec![], vec![]),
        };
        for (x, f) in xs.iter().zip(&flags) {
            let n = acc.push_node(&[*x]).unwrap();
            acc.nodes_flags_mut().as_mut_slice()[n as usize] = *f;
        }
        let (local, ghost): (&[[Idx; 2]], &[[Idx; 2]]) = match r {
            0 => (&[[0, 1], [1, 2]], &[[2, 3]]),
            1 => (&[[0, 1], [1, 2]], &[[3, 0]]),
            _ => (&[], &[]),
        };
        for c in local {
            acc.connectivity_mut(SEG, GhostType::NotGhost).push_tuple(c);
        }
        for c in ghost {
            acc.connectivity_mut(SEG, GhostType::Ghost).push_tuple(c);
        }
        let mut sync = Synchronizer::new("nodes", comm.clone());
        match r {
            0 => {
                sync.communications.send.insert(1, vec![2, 4]);
            }
            1 => {
                sync.communications.recv.insert(0, vec![0, 4]);
                acc.set_node_prank(0, 0);
                acc.set_node_prank(4, 0);
            }
            _ => {}
        }
        acc.synchronizers_mut().node = Some(sync);
        acc.set_distributed();
    }
    mesh
}

/// Numbering, node schemes and fresh-registration equality of one rank
/// after a distributed removal.
type RankResult = Result<(Vec<Option<i64>>, Vec<Option<i64>>, Vec<(i32, Vec<Idx>)>), String>;

fn distributed_removal(comm: Communicator, remove_on: &[usize]) -> RankResult {
    let shared = SharedMesh::new(boundary_mesh(&comm));
    let mut dm = DofManager::new("dm", comm.clone());
    {
        let mesh = shared.read().map_err(|e| e.to_string())?;
        dm.register_nodal_dofs(&mesh, "u", 1)
            .map_err(|e| e.to_string())?;
        dm.register_nodal_dofs(&mesh, "d", 2)
            .map_err(|e| e.to_string())?;
    }
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .map_err(|e| e.to_string())?;
    let r = comm.rank();
    let numbering: Option<Vec<Idx>> = usize::try_from(r)
        .is_ok_and(|r| remove_on.contains(&r))
        .then(|| vec![0, 1, 2, 3, -1]);
    let mut hook = DistributedRemovalHook::new();
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| match &numbering {
                Some(n) => tx.remove_nodes(MeshPart::Main, n),
                // the identity numbering keeps the event schedule identical
                None => {
                    let nb = tx.mesh().nb_nodes();
                    let id: Vec<Idx> = (0..nb as Idx).collect();
                    tx.remove_nodes(MeshPart::Main, &id)
                }
            },
            Some(&mut hook),
        )
        .map_err(|e| e.to_string())?;
    let mesh = shared.read().map_err(|e| e.to_string())?;
    let mut fresh = DofManager::new("fresh", comm.clone());
    fresh
        .register_nodal_dofs(&mesh, "u", 1)
        .map_err(|e| e.to_string())?;
    fresh
        .register_nodal_dofs(&mesh, "d", 2)
        .map_err(|e| e.to_string())?;
    assert_same_numbering(&dm, &fresh, &["u", "d"]);
    let sync = mesh.synchronizers().node.as_ref().map(|s| {
        s.communications
            .send
            .iter()
            .chain(&s.communications.recv)
            .map(|(&p, v)| (p, v.clone()))
            .collect()
    });
    Ok((
        dm.global_equation_numbers("u").map_err(|e| e.to_string())?,
        dm.global_equation_numbers("d").map_err(|e| e.to_string())?,
        sync.unwrap_or_default(),
    ))
}

/// The shared unused node g5 removed on both ranks: the node schemes drop
/// it on both sides, the global equations are re-derived (rank 1's copy of
/// g2 receives its owner's new numbers through the renumbered scheme), and
/// every rank equals a fresh registration on its renumbered mesh.
#[test]
fn removal_on_the_rank_boundary_renumbers_global_equations() {
    let out = run_local(3, Duration::from_secs(20), |comm| {
        distributed_removal(comm, &[0, 1])
    });
    let out: Vec<_> = out
        .into_iter()
        .enumerate()
        .map(|(r, o)| o.unwrap_or_else(|e| panic!("rank {r}: {e}")))
        .collect();
    let s = Some;
    // u: rank 0 owns g0, g1, g2 (0, 1, 2), rank 1 owns g3, g4 (3, 4);
    // d (2 components) from the offset 5: rank 0 owns 6, rank 1 owns 4
    assert_eq!(out[0].0, vec![s(0), s(1), s(2), None]);
    assert_eq!(out[1].0, vec![s(2), s(3), s(4), None]);
    assert_eq!(
        out[0].1,
        vec![s(5), s(6), s(7), s(8), s(9), s(10), None, None]
    );
    assert_eq!(
        out[1].1,
        vec![s(9), s(10), s(11), s(12), s(13), s(14), None, None]
    );
    assert_eq!(out[0].2, vec![(1, vec![2])]);
    assert_eq!(out[1].2, vec![(0, vec![0])]);
    assert!(out[2].2.is_empty());
}

/// Only the slave copy removed (rank 1): allowed — the owner drops it from
/// its send scheme and keeps its master node (now without copy).
#[test]
fn removing_only_a_slave_copy_drops_it_from_the_owner_scheme() {
    let out = run_local(3, Duration::from_secs(20), |comm| {
        distributed_removal(comm, &[1])
    });
    let out: Vec<_> = out
        .into_iter()
        .enumerate()
        .map(|(r, o)| o.unwrap_or_else(|e| panic!("rank {r}: {e}")))
        .collect();
    assert_eq!(out[0].2, vec![(1, vec![2])]);
    assert_eq!(out[1].2, vec![(0, vec![0])]);
    // rank 0 still owns g5 (5 owned u DOFs)
    assert_eq!(out[0].0.len(), 5);
    assert_eq!(out[1].0.len(), 4);
}

/// The master removed while its copy is kept (rank 0 only): refused by the
/// hook on rank 0, rolled back on **every** rank (recoverable; the mesh,
/// its schemes and the managers unchanged; the domain usable).
#[test]
fn removing_a_master_whose_copy_is_kept_rolls_back_everywhere() {
    let out = run_local(3, Duration::from_secs(20), |comm| {
        let shared = SharedMesh::new(boundary_mesh(&comm));
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_nodal_dofs(&shared.read().unwrap(), "u", 1)
            .unwrap();
        let layout = dm.layout().clone();
        let token = shared
            .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
            .unwrap();
        let revision = shared.revision().unwrap();
        let r = comm.rank();
        let mut hook = DistributedRemovalHook::new();
        let res = shared.mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| {
                let nb = tx.mesh().nb_nodes();
                let mut n: Vec<Idx> = (0..nb as Idx).collect();
                if r == 0 {
                    n[4] = -1;
                }
                tx.remove_nodes(MeshPart::Main, &n)
            },
            Some(&mut hook),
        );
        let err = res
            .err()
            .map(|e| (e.fatal_kind().is_none(), format!("{e}")));
        let unchanged = shared.revision().unwrap() == revision
            && dm.layout() == &layout
            && shared.read().unwrap().nb_nodes() == if r == 2 { 0 } else { 5 };
        let schemes = shared
            .read()
            .unwrap()
            .synchronizers()
            .node
            .as_ref()
            .map(|s| (s.communications.send.clone(), s.communications.recv.clone()));
        // the domain stays usable: an empty mutation commits
        let again = shared
            .mutate(
                &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
                |_| Ok(()),
                Some(&mut DistributedRemovalHook::new()),
            )
            .is_ok();
        (err, unchanged, schemes, again)
    });
    for (r, (err, unchanged, schemes, again)) in out.iter().enumerate() {
        let (recoverable, msg) = err
            .as_ref()
            .unwrap_or_else(|| panic!("rank {r}: committed"));
        assert!(recoverable, "rank {r}: {msg}");
        if r == 0 {
            assert!(msg.contains("node 4 is removed here"), "rank 0: {msg}");
        }
        assert!(unchanged, "rank {r}");
        assert!(again, "rank {r}");
        if r == 0 {
            assert_eq!(schemes.as_ref().unwrap().0.get(&1), Some(&vec![2, 4]));
        }
        if r == 1 {
            assert_eq!(schemes.as_ref().unwrap().1.get(&0), Some(&vec![0, 4]));
        }
    }
}
