// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/{dof_synchronizer.hh,
// dof_synchronizer.cc, dof_synchronizer_inline_impl.hh}.

//! DOF synchronizer (MPI-018): the node synchronizer's schemes translated
//! into **local equation** schemes (upstream `DOFSynchronizer`, built by
//! `DOFManagerDefault` on distributed meshes).
//!
//! For every nodal family in registration order (upstream iterates
//! `getDOFIDs()`), for both directions and every neighbour, every node of
//! the node scheme contributes its DOFs' global equations **sorted**, mapped
//! back to local equations (periodic aliases collapse onto one local
//! equation, as upstream's `globalToLocalEquationNumber`). Generic (non
//! nodal) families contribute nothing. The root gather/scatter mapping
//! ([`DofEntities`]) is the owned (local-or-master) local equations with
//! their global numbers (upstream `fillEntityToSend`, `canScatterSize` =
//! local system size, `gatheredSize` = system size).
//!
//! The DOF manager itself exchanges shared equations through the node
//! synchronizer (`_ask_nodes`, distributed products, solution
//! synchronisation, ARCHITECTURE.md, "DOF manager"); this synchronizer is the upstream-compatible
//! view used by gathers/scatters of DOF vectors and by user code
//! (`test_dof_synchronizer`).

use akantu_comm::Synchronizer;
use akantu_comm::synchronizer::{ScatterGatherEntities, SendRecv};
use akantu_core::enums::DofSupportType;
use akantu_core::{Error, GlobalIdx, Idx, Result, try_idx, try_to_usize};
use akantu_mesh::Mesh;

use crate::dof_manager::DofManager;

/// Build the DOF synchronizer of `dm` for the distributed `mesh` (upstream
/// `DOFSynchronizer(dof_manager)`): empty on a mesh without node
/// synchronizer or on one rank (upstream returns early for `nb_proc == 1`).
pub fn build_dof_synchronizer(dm: &DofManager, mesh: &Mesh) -> Result<Synchronizer<Idx>> {
    let mut sync = Synchronizer::new(
        format!("{}:dof_synchronizer", dm.id()),
        dm.communicator().clone(),
    );
    let Some(ns) = mesh.synchronizers().node.as_ref() else {
        return Ok(sync);
    };
    if dm.communicator().size() == 1 {
        return Ok(sync);
    }
    let layout = dm.layout();
    for id in dm.dof_ids() {
        if dm.support_type(id)? != DofSupportType::Nodal {
            continue;
        }
        let family = dm.family(id)?;
        // node → positions of its DOFs (upstream: std::find over
        // associated_nodes, in DOF order)
        let mut by_node: std::collections::HashMap<Idx, Vec<usize>> =
            std::collections::HashMap::new();
        for (pos, &n) in family.associated_nodes.iter().enumerate() {
            by_node.entry(n).or_default().push(pos);
        }
        for sr in [SendRecv::Send, SendRecv::Recv] {
            for (&proc, nodes) in ns.communications.schemes(sr) {
                let scheme = sync.communications.scheme_mut(proc, sr);
                for n in nodes {
                    let mut globals: Vec<GlobalIdx> = Vec::new();
                    for &pos in by_node.get(n).map_or(&[][..], Vec::as_slice) {
                        let l = try_to_usize(family.local_equation_numbers[pos])?;
                        let g = layout.global(l).ok_or_else(|| {
                            Error::Communication(format!(
                                "dof synchronizer: node {n} of a node scheme has no global equation"
                            ))
                        })?;
                        globals.push(g);
                    }
                    globals.sort_unstable();
                    for g in globals {
                        scheme.push(try_idx(layout.local(g)?)?);
                    }
                }
            }
        }
    }
    sync.set_entities_changed();
    Ok(sync)
}

/// Root gather/scatter mapping of the DOF synchronizer (upstream
/// `fillEntityToSend`, `canScatterSize`, `gatheredSize`).
#[derive(Clone, Copy, Debug)]
pub struct DofEntities<'a> {
    /// The DOF manager.
    pub dm: &'a DofManager,
}

impl ScatterGatherEntities for DofEntities<'_> {
    fn owned_entities(&self) -> (Vec<Idx>, Vec<Idx>) {
        let layout = self.dm.layout();
        let mut local = Vec::new();
        let mut global = Vec::new();
        for l in 0..layout.nb_local() {
            if layout.is_owned(l)
                && let (Ok(li), Some(g)) = (try_idx(l), layout.global(l))
            {
                local.push(li);
                global.push(Idx::try_from(g).unwrap_or(-1));
            }
        }
        (local, global)
    }
    fn local_size(&self) -> usize {
        self.dm.local_system_size()
    }
    fn global_size(&self) -> usize {
        usize::try_from(self.dm.system_size()).unwrap_or(usize::MAX)
    }
}
