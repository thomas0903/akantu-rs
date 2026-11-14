// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Node
// removal (SMM-015, UD-209): the DOF manager after a node removal
// against an independent registration (upstream registerDOFs /
// updateDOFsData, dof_manager.cc:590-626, :768-870; removeNodesFromArray,
// mesh_inline_impl.hh:126-145), with the two composition probes
// expecting the corrected behaviour
// and a test of the removal-last guard.

//! Three meshes, each mutated through
//! `SharedMesh::mutate` with the DOF manager as the only consumer: an
//! element erasure **in the middle** of the element block plus `purify`
//! (orphaned node removed, every other node permuted by first appearance).
//! After the mutation the manager must equal a manager registered afresh on
//! the final mesh (layout, local and global equations, associated nodes,
//! periodic aliases, node → tuple map, flags), every per-DOF value must
//! have followed its node, and the per-equation state (residual, a lumped
//! matrix) its equation. The expected values are computed here from a
//! snapshot keyed by `(family, old node, component)`, independently of the
//! port's compaction.

use std::collections::HashMap;

use akantu_comm::Communicator;
use akantu_core::enums::{EventPriority, MatrixType};
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_solver::DofManager;

const TRI: ElementType = ElementType::Triangle3;

/// `nx × ny` cells of two triangles, nodes created **top row first**;
/// returns the mesh and the node index of grid point `(i, j)`.
fn grid(nx: usize, ny: usize) -> (Mesh, Vec<Idx>) {
    let mut mesh = Mesh::new(2, "grid").unwrap();
    let mut index = vec![0 as Idx; (nx + 1) * (ny + 1)];
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in (0..=ny).rev() {
            for i in 0..=nx {
                index[j * (nx + 1) + i] = a.push_node(&[i as Real, j as Real]).unwrap();
            }
        }
        let id = |i: usize, j: usize| index[j * (nx + 1) + i];
        let mut c = a.connectivity_mut(TRI, GhostType::NotGhost);
        for j in 0..ny {
            for i in 0..nx {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    (mesh, index)
}

/// The two triangles of cell `(i, j)` (element index `2 (j nx + i)`).
fn cell(nx: usize, i: usize, j: usize) -> Vec<Element> {
    let e = 2 * (j * nx + i);
    (e..e + 2)
        .map(|k| Element::new(TRI, k as Idx, GhostType::NotGhost))
        .collect()
}

/// One family's per-DOF snapshot keyed by `(node, component)` and the
/// per-equation values keyed the same way.
#[derive(Default)]
struct FamilySnap {
    values: HashMap<(Idx, usize), Vec<u64>>,
    blocked: HashMap<(Idx, usize), bool>,
    residual: HashMap<(Idx, usize), u64>,
    lumped: HashMap<(Idx, usize), u64>,
}

fn snap(dm: &DofManager, id: &str) -> FamilySnap {
    let f = dm.family(id).unwrap();
    let nbc = f.dofs.read().unwrap().nb_component();
    let mut arrays: Vec<Vec<Real>> = vec![f.dofs.read().unwrap().as_slice().to_vec()];
    for d in f.derivatives.values() {
        arrays.push(d.read().unwrap().as_slice().to_vec());
    }
    if let Some(p) = &f.previous {
        arrays.push(p.read().unwrap().as_slice().to_vec());
    }
    if let Some(p) = &f.increment {
        arrays.push(p.read().unwrap().as_slice().to_vec());
    }
    arrays.push(f.solution.as_slice().to_vec());
    let blocked = f
        .blocked
        .as_ref()
        .map(|b| b.read().unwrap().as_slice().to_vec());
    let lumped = dm.lumped_matrix("M").unwrap().values.clone();
    let mut s = FamilySnap::default();
    for (k, (&node, &eq)) in f
        .associated_nodes
        .iter()
        .zip(&f.local_equation_numbers)
        .enumerate()
    {
        let c = k % nbc;
        let t = k / nbc;
        let key = (node, c);
        s.values.insert(
            key,
            arrays.iter().map(|a| a[t * nbc + c].to_bits()).collect(),
        );
        if let Some(b) = &blocked {
            s.blocked.insert(key, b[t * nbc + c]);
        }
        s.residual
            .insert(key, dm.residual().values()[eq as usize].to_bits());
        s.lumped.insert(key, lumped[eq as usize].to_bits());
    }
    s
}

/// Distinct values everywhere: array `a` of family `fid`, DOF `k`:
/// `1000 fid + 100 a + k`; residual and `M`: `1e5 + 7 e` / `2e5 + 3 e`.
fn fill(dm: &mut DofManager, ids: &[&str]) {
    for (fid, id) in ids.iter().enumerate() {
        let f = dm.family_mut(id).unwrap();
        let mut a = 0.0;
        let mut put = |s: &mut [Real]| {
            for (k, v) in s.iter_mut().enumerate() {
                *v = 1000.0 * fid as Real + 100.0 * a + k as Real;
            }
            a += 1.0;
        };
        put(f.dofs.write().unwrap().as_mut_slice());
        for d in f.derivatives.values_mut() {
            put(d.write().unwrap().as_mut_slice());
        }
        if let Some(p) = &mut f.previous {
            put(p.write().unwrap().as_mut_slice());
        }
        if let Some(p) = &mut f.increment {
            put(p.write().unwrap().as_mut_slice());
        }
        put(f.solution.as_mut_slice());
        if let Some(b) = &mut f.blocked {
            for (k, x) in b.write().unwrap().as_mut_slice().iter_mut().enumerate() {
                *x = (k + fid) % 3 == 0;
            }
        }
    }
    for (e, r) in dm.residual_mut().as_mut_slice().iter_mut().enumerate() {
        *r = 1e5 + 7.0 * e as Real;
    }
    let n = dm.local_system_size();
    let m = dm.new_lumped_matrix("M").unwrap();
    m.values = (0..n).map(|e| 2e5 + 3.0 * e as Real).collect();
}

/// The manager equals a fresh registration (`fresh`) on the final mesh.
fn assert_fresh(dm: &DofManager, fresh: &DofManager, ids: &[&str]) {
    assert_eq!(dm.layout(), fresh.layout(), "layout");
    assert_eq!(dm.local_system_size(), fresh.local_system_size());
    for id in ids {
        let (a, b) = (dm.family(id).unwrap(), fresh.family(id).unwrap());
        assert_eq!(
            a.local_equation_numbers, b.local_equation_numbers,
            "{id}: eqs"
        );
        assert_eq!(a.associated_nodes, b.associated_nodes, "{id}: nodes");
        assert_eq!(a.periodic_slave, b.periodic_slave, "{id}: slaves");
        assert_eq!(a.node_to_tuple, b.node_to_tuple, "{id}: node → tuple");
        assert_eq!(
            dm.global_equation_numbers(id).unwrap(),
            fresh.global_equation_numbers(id).unwrap(),
            "{id}: global equations"
        );
        assert_eq!(
            a.dofs.read().unwrap().len(),
            b.dofs.read().unwrap().len(),
            "{id}: tuples"
        );
    }
    for l in 0..dm.local_system_size() {
        assert_eq!(
            dm.dof_flag(l).unwrap(),
            fresh.dof_flag(l).unwrap(),
            "flag {l}"
        );
    }
}

/// Every surviving DOF `(old node, c)` of `id` sits at `(new node, c)` with
/// its values and blocked flag; the per-equation state follows unless the
/// old equation was shared by several DOFs whose new equations differ
/// (`dissolved`: a periodic pair whose master was removed).
fn assert_followed(
    dm: &DofManager,
    before: &FamilySnap,
    id: &str,
    numbering: &[Idx],
    dissolved: &[Idx],
) {
    let after = snap(dm, id);
    let mut seen = 0;
    for (&(old, c), v) in &before.values {
        let new = numbering[old as usize];
        if new < 0 {
            continue;
        }
        seen += 1;
        let key = (new, c);
        assert_eq!(
            after.values.get(&key),
            Some(v),
            "{id}: values of {old} → {new}"
        );
        if let Some(b) = before.blocked.get(&(old, c)) {
            assert_eq!(
                after.blocked.get(&key),
                Some(b),
                "{id}: blocked {old} → {new}"
            );
        }
        if !dissolved.contains(&new) {
            assert_eq!(
                after.residual.get(&key),
                before.residual.get(&(old, c)),
                "{id}: residual {old} → {new}"
            );
            assert_eq!(
                after.lumped.get(&key),
                before.lumped.get(&(old, c)),
                "{id}: lumped {old} → {new}"
            );
        }
    }
    assert_eq!(seen, after.values.len(), "{id}: every DOF accounted for");
}

/// Mutate with the manager as the consumer; the removal numbering.
fn mutate(
    shared: &SharedMesh,
    dm: &mut DofManager,
    change: impl FnOnce(&mut akantu_mesh::MeshTransaction<'_>) -> akantu_core::Result<()>,
) -> Vec<Idx> {
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    let c = shared
        .mutate(
            &mut [(&token, dm as &mut dyn MeshEventHandler)],
            change,
            None,
        )
        .unwrap();
    c.events
        .iter()
        .find_map(|(p, e)| match e {
            MeshEvent::NodesRemoved { new_numbering, .. } if *p == MeshPart::Main => {
                Some(new_numbering.clone())
            }
            _ => None,
        })
        .expect("a removal")
}

/// Mesh 1 — a group-supported family (3 components) next to a whole-mesh
/// family (2 components, every optional array): the group lists nodes out
/// of order and loses its node at the erased corner.
#[test]
fn v134_group_family_on_my_grid() {
    let (nx, ny) = (4, 3);
    let (mut mesh, idx) = grid(nx, ny);
    let id = |i: usize, j: usize| idx[j * (nx + 1) + i];
    mesh.groups_mut().create_element_group("g", 2).unwrap();
    for (i, j) in [(4, 2), (4, 0), (2, 0), (4, 1), (3, 0), (0, 0), (4, 3)] {
        mesh.groups_mut()
            .element_group_mut("g")
            .unwrap()
            .add_node(id(i, j), true);
    }
    let shared = SharedMesh::new(mesh);
    let mut dm = DofManager::new("dm", Communicator::serial());
    {
        let m = shared.read().unwrap();
        dm.register_nodal_dofs(&m, "u", 2).unwrap();
        dm.register_nodal_dofs_on_group(&m, "w", 3, "g").unwrap();
    }
    for id in ["u", "w"] {
        dm.register_dofs_derivative(id, 1).unwrap();
        dm.register_blocked_dofs(id).unwrap();
    }
    dm.register_dofs_derivative("u", 2).unwrap();
    dm.register_dofs_previous("u").unwrap();
    dm.register_dofs_increment("u").unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    fill(&mut dm, &["u", "w"]);
    dm.update_global_blocked_dofs();
    let (bu, bw) = (snap(&dm, "u"), snap(&dm, "w"));
    // cell (3, 0): elements 6 and 7 of 24 (middle of the block); its corner
    // (4, 0) is used by no other element
    let numbering = mutate(&shared, &mut dm, |tx| {
        tx.erase_elements(MeshPart::Main, &cell(nx, 3, 0))?;
        tx.purify(MeshPart::Main)
    });
    assert_eq!(numbering[id(4, 0) as usize], -1);
    assert!(
        numbering
            .iter()
            .enumerate()
            .any(|(i, &n)| n >= 0 && n as usize != i)
    );
    let m = shared.read().unwrap();
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh.register_nodal_dofs(&m, "u", 2).unwrap();
    fresh.register_nodal_dofs_on_group(&m, "w", 3, "g").unwrap();
    assert_fresh(&dm, &fresh, &["u", "w"]);
    assert_followed(&dm, &bu, "u", &numbering, &[]);
    assert_followed(&dm, &bw, "w", &numbering, &[]);
    // the blocked snapshot names the new equations
    let snapshot = dm.global_blocked_dofs_indexes().to_vec();
    dm.update_global_blocked_dofs();
    assert_eq!(dm.global_blocked_dofs_indexes(), &snapshot[..]);
    assert!(!dm.has_blocked_dofs_changed());
    assert_eq!(dm.matrix("K").unwrap().nnz(), 0);
    assert_eq!(dm.matrix("K").unwrap().size(), dm.system_size());
}

/// Mesh 2 — a periodic strip: the right column slaves of the left column;
/// an unused slave of an interior node; an unused master of a used top
/// node. Cell (0, 0) erased (the first elements: the hole is filled by the
/// last ones): the left-bottom master (0, 0) loses its last element and is
/// removed while its slave (4, 0) is kept — the pair is dissolved, as for
/// the unused master; the unused slave drops its pair.
#[test]
fn v134_periodic_family_on_my_strip() {
    let (nx, ny) = (4, 2);
    let (mut mesh, idx) = grid(nx, ny);
    let id = |i: usize, j: usize| idx[j * (nx + 1) + i];
    let (unused_slave, unused_master) = {
        let mut a = MeshAccessor::new(&mut mesh);
        let s = a.push_node(&[2.0, 1.5]).unwrap();
        let m = a.push_node(&[2.0, 2.5]).unwrap();
        for j in 0..=ny {
            a.add_periodic_pair(id(4, j), id(0, j)).unwrap();
        }
        a.add_periodic_pair(s, id(2, 1)).unwrap();
        a.add_periodic_pair(id(2, 2), m).unwrap();
        (s, m)
    };
    assert!(mesh.is_periodic());
    let shared = SharedMesh::new(mesh);
    let mut dm = DofManager::new("dm", Communicator::serial());
    {
        let m = shared.read().unwrap();
        dm.register_nodal_dofs(&m, "u", 2).unwrap();
        dm.register_generic_dofs("lam", 2, 2).unwrap();
        dm.register_nodal_dofs(&m, "t", 1).unwrap();
    }
    for id in ["u", "t"] {
        dm.register_dofs_derivative(id, 1).unwrap();
        dm.register_blocked_dofs(id).unwrap();
    }
    fill(&mut dm, &["u", "t"]);
    let (bu, bt) = (snap(&dm, "u"), snap(&dm, "t"));
    let lam = dm.dofs("lam").unwrap().as_slice().to_vec();
    let lam_eqs = dm.local_equation_numbers("lam").unwrap().to_vec();
    let lam_res: Vec<Real> = lam_eqs
        .iter()
        .map(|&e| dm.residual().values()[e as usize])
        .collect();
    let numbering = mutate(&shared, &mut dm, |tx| {
        tx.erase_elements(MeshPart::Main, &cell(nx, 0, 0))?;
        tx.purify(MeshPart::Main)
    });
    let m = shared.read().unwrap();
    assert_eq!(numbering[id(0, 0) as usize], -1);
    assert_eq!(numbering[unused_slave as usize], -1);
    assert_eq!(numbering[unused_master as usize], -1);
    let (s40, s22) = (numbering[id(4, 0) as usize], numbering[id(2, 2) as usize]);
    assert!(m.periodic_master(s40).is_none(), "pair of (4, 0) dissolved");
    assert!(m.periodic_master(s22).is_none(), "pair of (2, 2) dissolved");
    assert!(
        m.periodic_master(numbering[id(4, 1) as usize]).is_some()
            && m.periodic_master(numbering[id(4, 2) as usize]).is_some(),
        "the other pairs are kept"
    );
    assert!(m.is_periodic());
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh.register_nodal_dofs(&m, "u", 2).unwrap();
    fresh.register_generic_dofs("lam", 2, 2).unwrap();
    fresh.register_nodal_dofs(&m, "t", 1).unwrap();
    assert_fresh(&dm, &fresh, &["u", "lam", "t"]);
    assert_followed(&dm, &bu, "u", &numbering, &[]);
    assert_followed(&dm, &bt, "t", &numbering, &[]);
    assert_eq!(dm.dofs("lam").unwrap().as_slice(), &lam[..]);
    let new_lam_eqs = dm.local_equation_numbers("lam").unwrap();
    for (k, &e) in new_lam_eqs.iter().enumerate() {
        assert_eq!(
            dm.residual().values()[e as usize].to_bits(),
            lam_res[k].to_bits()
        );
    }
    // slaves alias their master's equation in every nodal family
    for fam in ["u", "t"] {
        let f = dm.family(fam).unwrap();
        let nbc = if fam == "u" { 2 } else { 1 };
        for (k, &n) in f.associated_nodes.iter().enumerate() {
            if let Some(master) = m.periodic_master(n) {
                let mk = f
                    .associated_nodes
                    .iter()
                    .enumerate()
                    .position(|(q, &x)| x == master && q % nbc == k % nbc)
                    .unwrap();
                assert_eq!(f.local_equation_numbers[k], f.local_equation_numbers[mk]);
            }
        }
    }
}

/// Mesh 3 — two nodal families of 1 and 3 components registered as `z`,
/// generic `g`, then `a` (registration order ≠ lexicographic order), then a
/// node insertion (the addition blocks come in lexicographic order: `a`
/// then `z`), then the removal: the blocks collapse into the registration
/// order of a fresh registration.
#[test]
fn v134_two_families_different_components_after_insertion() {
    let (nx, ny) = (3, 2);
    let (mesh, idx) = grid(nx, ny);
    let id = |i: usize, j: usize| idx[j * (nx + 1) + i];
    let shared = SharedMesh::new(mesh);
    let mut dm = DofManager::new("dm", Communicator::serial());
    {
        let m = shared.read().unwrap();
        dm.register_nodal_dofs(&m, "z", 1).unwrap();
        dm.register_generic_dofs("g", 3, 1).unwrap();
        dm.register_nodal_dofs(&m, "a", 3).unwrap();
    }
    for id in ["z", "a"] {
        dm.register_blocked_dofs(id).unwrap();
        dm.register_dofs_derivative(id, 1).unwrap();
    }
    // insertion: a node above the top-right corner and a triangle using it
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[3.0, 3.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "v134".into(),
                        extension: None,
                    },
                );
                let e = tx.push_element(
                    MeshPart::Main,
                    TRI,
                    GhostType::NotGhost,
                    &[id(2, 2), id(3, 2), n],
                )?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: None,
                        origin: "v134".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    drop(token);
    // the insertion appended a block per family: not a fresh numbering yet
    {
        let m = shared.read().unwrap();
        let mut fresh = DofManager::new("fresh", Communicator::serial());
        fresh.register_nodal_dofs(&m, "z", 1).unwrap();
        fresh.register_generic_dofs("g", 3, 1).unwrap();
        fresh.register_nodal_dofs(&m, "a", 3).unwrap();
        assert_ne!(
            dm.local_equation_numbers("z").unwrap(),
            fresh.local_equation_numbers("z").unwrap()
        );
    }
    fill(&mut dm, &["z", "a"]);
    let (bz, ba) = (snap(&dm, "z"), snap(&dm, "a"));
    let numbering = mutate(&shared, &mut dm, |tx| {
        // cell (1, 0): elements 2 and 3 of 13 — its nodes are all used by
        // other cells — and cell (2, 0), whose corner (3, 0) becomes unused,
        // in **one** call (two erasures of a block in one mutation
        // are refused; two separate calls give the same final mesh:
        // holes 2..5 filled by 12, 11, 10, 9 either way)
        let mut both = cell(nx, 1, 0);
        both.extend(cell(nx, 2, 0));
        tx.erase_elements(MeshPart::Main, &both)?;
        tx.purify(MeshPart::Main)
    });
    assert_eq!(numbering[id(3, 0) as usize], -1);
    let m = shared.read().unwrap();
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh.register_nodal_dofs(&m, "z", 1).unwrap();
    fresh.register_generic_dofs("g", 3, 1).unwrap();
    fresh.register_nodal_dofs(&m, "a", 3).unwrap();
    assert_fresh(&dm, &fresh, &["z", "g", "a"]);
    assert_followed(&dm, &bz, "z", &numbering, &[]);
    assert_followed(&dm, &ba, "a", &numbering, &[]);
}

// ------------------------------------------------------------ composition

fn line(nb: usize, segs: &[[Idx; 2]]) -> Mesh {
    let mut mesh = Mesh::new(1, "line").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for i in 0..nb {
            a.push_node(&[i as Real]).unwrap();
        }
        for s in segs {
            a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
                .push_tuple(s);
        }
    }
    mesh
}

fn added(n: Idx) -> MeshEvent {
    MeshEvent::NodesAdded {
        nodes: vec![n],
        old_nodes: None,
        origin: "v134".into(),
        extension: None,
    }
}

/// The refused mutation through the manager as the only consumer: the
/// error is recoverable (the DOF manager's capability check),
/// the revision, the node count and the manager's layout and values
/// unchanged. Returns the error (its variant is checked by the caller).
fn refused_before_commit(
    shared: &SharedMesh,
    dm: &mut DofManager,
    change: impl FnOnce(&mut akantu_mesh::MeshTransaction<'_>) -> akantu_core::Result<()>,
) -> akantu_core::Error {
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    let revision = shared.revision().unwrap();
    let nodes = shared.read().unwrap().nb_nodes();
    let layout = format!("{:?}", dm.layout());
    let u = dm.dofs("u").unwrap().as_slice().to_vec();
    let r = shared.mutate(
        &mut [(&token, dm as &mut dyn MeshEventHandler)],
        change,
        None,
    );
    drop(token);
    let e = r.expect_err("refused before the commit");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(shared.read().unwrap().nb_nodes(), nodes);
    assert_eq!(format!("{:?}", dm.layout()), layout);
    assert_eq!(dm.dofs("u").unwrap().as_slice(), &u[..]);
    e
}

/// Composition (flipped after the fix; was: fatal at dispatch): removal
/// then addition (the order the composition rule asks for) with a
/// group-supported family whose group receives the added node in the same
/// mutation. The group part of the renumbering is planned in the
/// capability check on the final mesh's group: refused before the commit,
/// recoverable; the domain stays usable (the removal alone commits, then
/// the addition in a mutation of its own).
#[test]
fn v134_group_family_removal_then_addition_is_refused_before_commit() {
    let mut mesh = line(5, &[[0, 1], [1, 3], [3, 4]]);
    mesh.groups_mut().create_element_group("g", 1).unwrap();
    for n in [1, 3, 4] {
        mesh.groups_mut()
            .element_group_mut("g")
            .unwrap()
            .add_node(n, true);
    }
    let shared = SharedMesh::new(mesh);
    let mut dm = DofManager::new("dm", Communicator::serial());
    {
        let m = shared.read().unwrap();
        dm.register_nodal_dofs(&m, "u", 1).unwrap();
        dm.register_nodal_dofs_on_group(&m, "w", 1, "g").unwrap();
    }
    let e = refused_before_commit(&shared, &mut dm, |tx| {
        tx.remove_nodes(MeshPart::Main, &[0, 1, -1, 2, 3])?;
        let n = tx.push_node(MeshPart::Main, &[5.0])?;
        tx.record(MeshPart::Main, added(n));
        tx.groups_mut(MeshPart::Main)?
            .element_group_mut("g")?
            .add_node(n, true);
        Ok(())
    });
    // the group check of the renumbering, planned in the capability check
    assert!(matches!(e, akantu_core::Error::Mesh(_)), "{e:?}");
    assert!(format!("{e}").contains("group 'g'"), "{e}");
    // a second removal in the same mutation (here a permutation: nodes 0
    // and 1 swapped) is refused too: the first removal's group plan, run in
    // the check on the final group, already fails (events are checked in
    // order) ...
    let e = refused_before_commit(&shared, &mut dm, |tx| {
        tx.remove_nodes(MeshPart::Main, &[0, 1, -1, 2, 3])?;
        tx.remove_nodes(MeshPart::Main, &[1, 0, 2, 3])
    });
    assert!(matches!(e, akantu_core::Error::Mesh(_)), "{e:?}");
    assert!(format!("{e}").contains("group 'g'"), "{e}");
    // ... and the second one's numbering, shorter than the nodes the
    // manager holds, is refused by the length rule
    let second = MeshEvent::NodesRemoved {
        nodes: Vec::new(),
        new_numbering: vec![1, 0, 2, 3],
        origin: "v134".into(),
    };
    let e = dm
        .check_mesh_event(&shared.read().unwrap(), &second)
        .unwrap_err();
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(format!("{e}").contains("second node removal"), "{e}");
    // usable: the removal, then the addition joining the group
    mutate(&shared, &mut dm, |tx| {
        tx.remove_nodes(MeshPart::Main, &[0, 1, -1, 2, 3])
    });
    let token = shared
        .register_consumer("dm2", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[5.0])?;
                tx.groups_mut(MeshPart::Main)?
                    .element_group_mut("g")?
                    .add_node(n, true);
                tx.record(MeshPart::Main, added(n));
                Ok(())
            },
            None,
        )
        .unwrap();
    assert_eq!(dm.dofs("u").unwrap().len(), 5);
    assert_eq!(dm.dofs("w").unwrap().len(), 4);
}

/// Composition, second part (flipped after the fix; was: not refused):
/// an addition followed by a **pure renumbering** (`remove_nodes` with a
/// permutation: nothing removed) is refused before the commit (the
/// removal's numbering maps more nodes than the manager holds: an addition
/// precedes); as two mutations it commits and equals a fresh registration.
#[test]
fn v134_addition_then_pure_permutation_is_refused_before_commit() {
    let shared = SharedMesh::new(line(5, &[[0, 1], [1, 2], [2, 3], [3, 4]]));
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&shared.read().unwrap(), "u", 1)
        .unwrap();
    for (k, v) in dm
        .dofs_mut("u")
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *v = 10.0 + k as Real;
    }
    let e = refused_before_commit(&shared, &mut dm, |tx| {
        let n = tx.push_node(MeshPart::Main, &[5.0])?;
        tx.record(MeshPart::Main, added(n));
        // swap the new node 5 with node 0
        tx.remove_nodes(MeshPart::Main, &[5, 1, 2, 3, 4, 0])
    });
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(
        format!("{e}").contains("preceded by a node addition"),
        "{e}"
    );
    // the same as two mutations
    let token = shared
        .register_consumer("dm", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[5.0])?;
                tx.record(MeshPart::Main, added(n));
                Ok(())
            },
            None,
        )
        .unwrap();
    shared
        .mutate(
            &mut [(&token, &mut dm as &mut dyn MeshEventHandler)],
            |tx| tx.remove_nodes(MeshPart::Main, &[5, 1, 2, 3, 4, 0]),
            None,
        )
        .unwrap();
    assert_eq!(
        dm.dofs("u").unwrap().as_slice(),
        &[0.0, 11.0, 12.0, 13.0, 14.0, 10.0]
    );
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh
        .register_nodal_dofs(&shared.read().unwrap(), "u", 1)
        .unwrap();
    assert_fresh(&dm, &fresh, &["u"]);
}

/// The "removal must be the last node event" rule on a
/// distributed or periodic mesh is unreachable through today's hooks
/// (`DistributedRemovalHook` refuses node additions and a second removal;
/// no transaction creates periodic pairs); it is kept as a guard and pinned
/// here through `DofManager::check_mesh_event` directly: on a periodic mesh
/// a `NodesRemoved` keeping fewer nodes than the (final) mesh has is
/// refused, the same removal keeping the mesh's count is accepted, and on a
/// serial non-periodic mesh the guard does not apply.
#[test]
fn v134_removal_not_last_on_a_periodic_mesh_is_refused_by_the_check() {
    let mut mesh = line(4, &[[0, 1], [1, 2], [2, 3]]);
    let plain = line(4, &[[0, 1], [1, 2], [2, 3]]);
    MeshAccessor::new(&mut mesh)
        .add_periodic_pair(3, 0)
        .unwrap();
    assert!(mesh.is_periodic());
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    let removed = |numbering: Vec<Idx>| MeshEvent::NodesRemoved {
        nodes: Vec::new(),
        new_numbering: numbering,
        origin: "v134".into(),
    };
    // 5 nodes mapped (as if one had been added), 3 kept, the final mesh 4
    // nodes: the addition rule fires first; a numbering of the held 4 nodes
    // keeping 3 while the final mesh has 4 (a later addition): the guard
    let not_last = removed(vec![0, 1, -1, 2]);
    let e = dm.check_mesh_event(&mesh, &not_last).unwrap_err();
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(format!("{e}").contains("last node event"), "{e}");
    // the removal as the last node event: kept count = final count
    let last = removed(vec![1, 0, 2, 3]);
    dm.check_mesh_event(&mesh, &last).unwrap();
    // serial, non-periodic: nothing is read, the composition is accepted
    let mut dp = DofManager::new("dp", Communicator::serial());
    dp.register_nodal_dofs(&plain, "u", 1).unwrap();
    dp.check_mesh_event(&plain, &not_last).unwrap();
}

/// Before the fix this mutation committed and the residual of the moved
/// node was lost: on a serial
/// periodic mesh, a removal followed by a **pure permutation** in one
/// mutation keeps the node count, so the DOF manager's count guard does not
/// see that the removal is not the last node event. The mesh domain's node
/// rule refuses it before the commit (recoverable, the manager unchanged);
/// the composed numbering in one call commits, equals a fresh registration
/// and every value and per-equation entry follows its node.
#[test]
fn lr10_removal_then_permutation_on_a_periodic_mesh_is_refused_before_commit() {
    let mut mesh = line(6, &[[0, 1], [1, 2], [2, 3], [3, 4]]);
    MeshAccessor::new(&mut mesh)
        .add_periodic_pair(4, 0)
        .unwrap();
    assert!(mesh.is_periodic());
    let shared = SharedMesh::new(mesh);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&shared.read().unwrap(), "u", 1)
        .unwrap();
    dm.register_dofs_derivative("u", 1).unwrap();
    dm.register_blocked_dofs("u").unwrap();
    fill(&mut dm, &["u"]);
    let before = snap(&dm, "u");
    let first: Vec<Idx> = vec![0, 1, 2, 3, 4, -1];
    let second: Vec<Idx> = vec![0, 1, 4, 3, 2];
    let e = refused_before_commit(&shared, &mut dm, |tx| {
        tx.remove_nodes(MeshPart::Main, &first)?;
        tx.remove_nodes(MeshPart::Main, &second)
    });
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(
        format!("{e}").contains("make the removal the last node event"),
        "{e}"
    );
    assert_eq!(snap(&dm, "u").residual, before.residual);
    // the composed numbering, recorded as one removal
    let composed: Vec<Idx> = first
        .iter()
        .map(|&n| if n < 0 { -1 } else { second[n as usize] })
        .collect();
    let numbering = mutate(&shared, &mut dm, |tx| {
        tx.remove_nodes(MeshPart::Main, &composed)
    });
    assert_eq!(numbering, composed);
    let m = shared.read().unwrap();
    assert_eq!(m.nb_nodes(), 5);
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh.register_nodal_dofs(&m, "u", 1).unwrap();
    assert_fresh(&dm, &fresh, &["u"]);
    assert_followed(&dm, &before, "u", &composed, &[]);
}
