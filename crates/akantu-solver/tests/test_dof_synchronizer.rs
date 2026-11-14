// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/test_dof_synchronizer.cc
// (PARALLEL): a nodal DOF family on a distributed mesh, its DOF
// synchronizer (MPI-018) synchronising, gathering and scattering a
// per-DOF array, on the local world at 1/2/4 ranks: on the upstream mesh
// `bar.msh` (as upstream) and on a larger structured triangle grid. The
// real-MPI run of the same scenario is `akantu-solid`'s `mpi_smm`
// (`test_dof_synchronizer_bar_msh`, scripts/mpi/run-par-tests.sh).

mod dof_sync_scenario;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};

fn on_ranks(f: fn(Communicator) -> Result<(), String>) {
    for n in [1, 2, 4] {
        for (r, o) in run_local(n, Duration::from_secs(60), f)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn test_dof_synchronizer() {
    on_ranks(|comm| {
        let mesh = dof_sync_scenario::grid(&comm, 4)?;
        dof_sync_scenario::check_dof_synchronizer(&comm, &mesh)
    });
}

/// The upstream mesh (`test/test_synchronizer/bar.msh`, 25 nodes,
/// 16 `_quadrangle_4`), read on rank 0 and distributed.
#[test]
fn test_dof_synchronizer_bar_msh() {
    on_ranks(|comm| {
        let mesh = dof_sync_scenario::bar_msh(&comm)?;
        dof_sync_scenario::check_dof_synchronizer(&comm, &mesh)
    });
}
