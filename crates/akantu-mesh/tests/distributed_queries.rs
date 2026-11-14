// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh.cc (computeBoundingBox,
// :415-462) and src/mesh/mesh_inline_impl.hh (getGlobalConnectivity,
// :497-560) — the distributed mesh queries of PART-016,
// on the in-process local world at 1, 2 and 4 ranks.

//! `cube.msh` (frozen) read on rank 0, kept serially on rank 0 and
//! distributed by bisection: on every rank the global bounding box equals
//! the serial one bit for bit (min/max reductions of copied coordinates),
//! the local box lies inside it, and the global connectivities of the
//! not-ghost elements of all ranks form exactly the serial connectivity
//! (as a multiset of rows; node global ids are the root's node numbers).
//! `Mesh::compute_bounding_box` called on the distributed mesh reduces the
//! local boxes to the same global box.

mod par_common;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::{GhostType, TypeFilter};
use akantu_mesh::utils::global_connectivity;
use par_common::{Outcome, distribute_rcb, ok, read_on_root};

const CUBE: &str = "test_synchronizer/cube.msh";

/// Rows (one per not-ghost element of the mesh dimension) of the global
/// connectivity, prefixed by the element type's node count.
fn rows(m: &akantu_mesh::Mesh) -> std::result::Result<Vec<Vec<i64>>, String> {
    let gc = ok(global_connectivity(m), "global connectivity")?;
    let mut out = Vec::new();
    for ty in m.element_types(TypeFilter::new().dim(3).ghost(GhostType::NotGhost)) {
        let a = ok(gc.get(ty, GhostType::NotGhost), "connectivity")?;
        let nn = ty.nb_nodes_per_element();
        for row in a.as_slice().chunks_exact(nn) {
            let mut r = vec![nn as i64];
            r.extend(row.iter().copied());
            out.push(r);
        }
    }
    Ok(out)
}

fn distributed_queries(comm: Communicator) -> Outcome {
    // the serial reference on rank 0
    let serial = if comm.rank() == 0 {
        let mut s = read_on_root(&Communicator::serial(), CUBE, 3)?;
        ok(s.compute_bounding_box(), "serial bbox")?;
        Some(s)
    } else {
        None
    };
    let mut m = read_on_root(&comm, CUBE, 3)?;
    distribute_rcb(&mut m)?;
    // global bbox: identical on every rank and equal to the serial one
    let mut bounds: Vec<f64> = m
        .bbox()
        .lower
        .iter()
        .chain(&m.bbox().upper)
        .copied()
        .collect();
    let mine = bounds.clone();
    ok(comm.broadcast(&mut bounds, 0), "broadcast")?;
    ensure!(
        mine.iter()
            .zip(&bounds)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "rank {}: global bbox {mine:?} differs from rank 0's {bounds:?}",
        comm.rank()
    );
    if let Some(s) = &serial {
        let want: Vec<f64> = s
            .bbox()
            .lower
            .iter()
            .chain(&s.bbox().upper)
            .copied()
            .collect();
        ensure!(
            want.iter()
                .zip(&mine)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "global bbox {mine:?} != serial {want:?}"
        );
    }
    let local = m.local_bbox();
    for d in 0..3 {
        ensure!(
            local.lower[d] >= m.bbox().lower[d] && local.upper[d] <= m.bbox().upper[d],
            "rank {}: local bbox outside the global one",
            comm.rank()
        );
    }
    // `Mesh::compute_bounding_box` on the distributed mesh (collective; the
    // port of upstream `computeBoundingBox` + `BBox::allSum`, mesh.cc:415-462):
    // `distribute` sets the box through its own
    // reduction, so the distributed branch is called here explicitly. The
    // result is again the serial box bit for bit on every rank, and the
    // local box differs from the global one on some rank (so a call that
    // skipped the reduction would be seen).
    ok(m.compute_bounding_box(), "distributed compute_bounding_box")?;
    let recomputed: Vec<f64> = m
        .bbox()
        .lower
        .iter()
        .chain(&m.bbox().upper)
        .copied()
        .collect();
    ensure!(
        recomputed
            .iter()
            .zip(&bounds)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "rank {}: compute_bounding_box gives {recomputed:?}, distribute gave {bounds:?}",
        comm.rank()
    );
    let local = m.local_bbox();
    let differs = local
        .lower
        .iter()
        .chain(&local.upper)
        .zip(&recomputed)
        .any(|(a, b)| a.to_bits() != b.to_bits());
    let mut any = [u64::from(differs)];
    ok(
        comm.all_reduce(&mut any, akantu_comm::ReduceOp::Max),
        "all_reduce",
    )?;
    ensure!(
        (comm.size() == 1) == (any[0] == 0),
        "{} ranks: a local box differs from the global one on some rank iff distributed",
        comm.size()
    );
    // global connectivity: the not-ghost rows of every rank = the serial rows
    let flat: Vec<i64> = rows(&m)?.into_iter().flatten().collect();
    let gathered = ok(comm.gather_v(&flat, 0), "gather_v")?;
    if let (Some(per_rank), Some(s)) = (gathered, &serial) {
        let mut got = Vec::new();
        for r in per_rank {
            let mut i = 0;
            while i < r.len() {
                let nn = usize::try_from(r[i]).map_err(|e| e.to_string())?;
                got.push(r[i..=i + nn].to_vec());
                i += nn + 1;
            }
        }
        let mut want = rows(s)?;
        got.sort();
        want.sort();
        ensure!(
            got == want,
            "global connectivity of the distributed mesh ({} rows) differs from the serial one ({} rows)",
            got.len(),
            want.len()
        );
    }
    Ok(())
}

#[test]
fn distributed_bounding_box_and_global_connectivity() {
    for n in [1, 2, 4] {
        for (r, o) in run_local(n, Duration::from_secs(60), distributed_queries)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}
