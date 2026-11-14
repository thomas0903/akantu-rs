// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). PY-045 with node
// removal (SMM-015):
// python/py_solver.cc:85-92 (`SolverVector.getValues()`, a view of the
// residual) against src/model/common/dof_manager/dof_manager.cc:901-902
// (upstream's empty `onNodesRemoved`) as the port renumbers.

//! An exported residual (the Python `getValues()` view) across a node
//! removal: the live residual follows its equations, the export lease is
//! retired with the pre-removal values (stale but safe, D-047), and the
//! residual is exportable again afterwards; a fresh lease follows the
//! next assembly.

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::array_shared::ForeignRuntime;
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, MeshEventHandler};
use akantu_solver::DofManager;

const SEG: ElementType = ElementType::Segment2;

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

#[test]
fn exported_residual_across_a_node_removal() {
    let segs = [[6, 7], [0, 1], [1, 3], [3, 4], [4, 6]];
    let mut mesh = line(8, &segs);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    let n = dm.local_system_size();
    assert_eq!(n, 16);
    let rt = ForeignRuntime::new();
    let mut interp = rt.enter().unwrap();
    let (lease, eqs_before) = {
        let _s = interp.native().unwrap();
        for (e, r) in dm.residual_mut().as_mut_slice().iter_mut().enumerate() {
            *r = 1000.0 + e as Real;
        }
        let lease = dm.residual_shared_mut().export(&rt).unwrap();
        (lease, dm.local_equation_numbers("u").unwrap().to_vec())
    };
    let before: Vec<Real> = (0..n).map(|e| 1000.0 + e as Real).collect();
    let numbering = {
        let _s = interp.native().unwrap();
        let numbering = akantu_mesh::utils::purify_numbering(&mesh).unwrap();
        let events = akantu_mesh::removal::remove_nodes(&mut mesh, &numbering, "t").unwrap();
        for e in events.events() {
            dm.on_mesh_event(&mesh, e).unwrap();
        }
        numbering
    };
    assert_eq!(numbering, vec![2, 3, -1, 4, 5, -1, 0, 1]);
    // the lease is retired and keeps the pre-removal values
    assert!(lease.is_retired());
    assert_eq!(lease.len(), 16);
    {
        let _s = interp.native().unwrap();
        assert!(!dm.residual_shared_mut().is_attached(&lease));
        assert_eq!(dm.residual().local_size(), 12);
        // the live residual: each surviving DOF's old value at its new eq
        let eqs_after = dm.local_equation_numbers("u").unwrap().to_vec();
        let r = dm.residual().values().to_vec();
        for (old, &nn) in numbering.iter().enumerate() {
            let Ok(nn) = usize::try_from(nn) else {
                continue;
            };
            for c in 0..2 {
                let oe = eqs_before[old * 2 + c] as usize;
                let ne = eqs_after[nn * 2 + c] as usize;
                assert_eq!(r[ne], before[oe], "node {old}→{nn} c {c}");
            }
        }
    }
    // the retired lease still owns its (old) block: foreign access to it
    // stays admitted (the Python view keeps the pre-removal values)
    lease.check_foreign_access(&interp).unwrap();
    // a fresh lease follows the next assembly
    let lease2 = {
        let _s = interp.native().unwrap();
        dm.residual_shared_mut().export(&rt).unwrap()
    };
    {
        let _s = interp.native().unwrap();
        dm.zero_residual();
        dm.assemble_to_residual("u", ArrayView::new(&[1.0; 12], 2).unwrap(), 2.0)
            .unwrap();
        assert!(dm.residual_shared_mut().is_attached(&lease2));
        assert_eq!(dm.residual().values(), [2.0; 12]);
    }
    assert!(!lease2.is_retired());
}
