// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_matrix_aij.cc:166-198
// (SparseMatrixAIJ::matVecMul, in-place accumulation; UD-003 zero `tmp`),
// src/model/common/dof_manager/dof_manager_default.cc
// (assembleMatMulVectToResidual); mesh test/test_solver/bar.msh.

//! The distributed
//! matrix–vector product of [`DofManager::mat_vec`] on a mesh distributed
//! by `Mesh::distribute` (recursive coordinate bisection), 1, 2 and 4 ranks
//! on the in-process local world, nodal family of 2 components on
//! `bar.msh` (100 `Quadrangle4`), symmetric `K` assembled from elemental
//! matrices, `y = 1·y0 − 1·K·x` (the shape of the inertial residual term
//! `r −= M·a`).
//!
//! * Upstream's per-rank result is the AIJ product without reduction
//!   (`matVecMul` reduces a zero temporary, UD-003): the matrix-level
//!   product with [`NoReduction`]. Every row the reduction does not touch
//!   (not a shared node: all rows on one rank, interior and pure-ghost rows
//!   on several) must equal it **bit for bit** (pseudo-random data, so the
//!   association of the in-place sum matters). On one rank the product
//!   must also equal the product of a manager on the undistributed mesh
//!   (no synchronizer). Both fail on the former zeroed-temporary branch.
//! * Shared rows carry the UD-003 correction: with small-integer data every
//!   sum is exact in any order, so the gathered product at 2 and 4 ranks
//!   equals the serial product exactly, and upstream's unreduced product
//!   differs from it on the shared rows (the test exercises them).

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_linalg::NoReduction;
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::synchronizers::NodeEntities;
use akantu_solver::DofManager;
use akantu_validation::fixtures_root;

const QUAD: ElementType = ElementType::Quadrangle4;
const NBC: usize = 2;

/// Deterministic hash of a key (splitmix64 finaliser).
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[derive(Clone, Copy, Debug)]
enum Data {
    /// Full 52-bit mantissas in [0.5, 1.5): rounding depends on the order.
    Random,
    /// Integers 1..=7: every sum is exact whatever the order.
    SmallIntegers,
}

impl Data {
    fn value(self, key: u64) -> Real {
        let h = mix(key);
        match self {
            Data::Random => 0.5 + (h >> 11) as Real / (1u64 << 53) as Real,
            Data::SmallIntegers => (h % 7 + 1) as Real,
        }
    }
    /// Symmetric entry between DOF (global node `ga`, component `ca`) and
    /// (`gb`, `cb`): the same on every rank whatever the partition.
    fn entry(self, ga: u64, ca: usize, gb: u64, cb: usize) -> Real {
        let a = ga * NBC as u64 + ca as u64;
        let b = gb * NBC as u64 + cb as u64;
        self.value((a.min(b) << 32) | a.max(b))
    }
    fn nodal(self, salt: u64, g: u64, c: usize) -> Real {
        self.value((salt << 48) ^ (g * NBC as u64 + c as u64))
    }
}

struct RankResult {
    /// Rows the reduction does not touch whose bits differ from upstream's.
    unshared_mismatches: usize,
    unshared_rows: usize,
    shared_rows: usize,
    /// On the root: the product and upstream's unreduced product, gathered
    /// in global node order (`node · NBC + component`).
    gathered: Option<(Vec<Real>, Vec<Real>)>,
    /// One rank: the product of a manager on the undistributed mesh.
    serial: Option<Vec<Real>>,
}

fn read_bar(comm: &Communicator, root_reads: bool) -> Result<Mesh, String> {
    let e = |e: akantu_core::Error| e.to_string();
    let mut mesh = Mesh::with_communicator(2, "bar", comm.clone()).map_err(e)?;
    if root_reads {
        let path = fixtures_root().join("mesh_io/test_solver/bar/inputs/bar.msh");
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).map_err(e)?;
    }
    mesh.compute_bounding_box().map_err(e)?;
    Ok(mesh)
}

/// Manager with `K` assembled on `mesh`; returns it with `x`, `y0` over
/// local equations.
fn manager(
    comm: &Communicator,
    mesh: &Mesh,
    data: Data,
) -> Result<(DofManager, Vec<Real>, Vec<Real>), String> {
    let e = |e: akantu_core::Error| e.to_string();
    let mut dm = DofManager::new("dm", comm.clone());
    dm.register_nodal_dofs(mesh, "u", NBC).map_err(e)?;
    dm.new_matrix("K", MatrixType::Symmetric).map_err(e)?;
    let gid = |n: Idx| -> Result<u64, String> {
        u64::try_from(mesh.node_global_id(n)).map_err(|x| x.to_string())
    };
    let ne = mesh.nb_element(QUAD, GhostType::NotGhost);
    let conn = mesh
        .connectivity(QUAD, GhostType::NotGhost)
        .map_err(e)?
        .as_slice()
        .to_vec();
    let nd = 4 * NBC;
    let mut k = Array::new(nd * nd);
    for el in 0..ne {
        let nodes = &conn[4 * el..4 * el + 4];
        let mut block = vec![0.0; nd * nd];
        for (a, &na) in nodes.iter().enumerate() {
            for (b, &nb) in nodes.iter().enumerate() {
                for ca in 0..NBC {
                    for cb in 0..NBC {
                        let (r, c) = (a * NBC + ca, b * NBC + cb);
                        block[c * nd + r] = data.entry(gid(na)?, ca, gid(nb)?, cb);
                    }
                }
            }
        }
        k.push_tuple(&block);
    }
    dm.assemble_elemental_matrices_to_matrix(
        mesh,
        "K",
        "u",
        k.view(),
        QUAD,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .map_err(e)?;
    dm.finalize_matrix("K").map_err(e)?;
    let n_local = dm.local_system_size();
    let mut x = vec![0.0; n_local];
    let mut y0 = vec![0.0; n_local];
    let eqs = dm.local_equation_numbers("u").map_err(e)?.to_vec();
    for (k, &eq) in eqs.iter().enumerate() {
        let eq = usize::try_from(eq).map_err(|x| x.to_string())?;
        let n = Idx::try_from(k / NBC).map_err(|x| x.to_string())?;
        x[eq] = data.nodal(1, gid(n)?, k % NBC);
        y0[eq] = data.nodal(2, gid(n)?, k % NBC);
    }
    Ok((dm, x, y0))
}

/// Values of a local-equation vector per (local node, component).
fn nodal(dm: &DofManager, v: &[Real]) -> Result<Vec<Real>, String> {
    let eqs = dm.local_equation_numbers("u").map_err(|e| e.to_string())?;
    eqs.iter()
        .map(|&eq| {
            usize::try_from(eq)
                .map(|eq| v[eq])
                .map_err(|x| x.to_string())
        })
        .collect()
}

fn scenario(comm: Communicator, data: Data) -> Result<RankResult, String> {
    let e = |e: akantu_core::Error| e.to_string();
    let size = comm.size();
    let mut mesh = read_bar(&comm, comm.rank() == 0)?;
    let mut partitioner = RcbPartitioner;
    // no mesh-dependent component exists yet: the events need no dispatch
    let _ = distribute_with(
        &mut mesh,
        Some(&mut partitioner as &mut dyn MeshPartitioner),
        0,
    )
    .map_err(e)?;
    if mesh.synchronizers().node.is_none() {
        return Err("distribute created no node synchronizer".into());
    }
    let (mut dm, x, y0) = manager(&comm, &mesh, data)?;
    let mut y = y0.clone();
    dm.mat_vec(&mesh, "K", &x, &mut y, -1.0, 1.0).map_err(e)?;
    // Upstream: in place, the reduction of a zero temporary is a no-op.
    let mut up = y0.clone();
    let layout = dm.layout().clone();
    dm.matrix("K")
        .map_err(e)?
        .mat_vec_mul(&layout, &NoReduction, &x, &mut up, -1.0, 1.0)
        .map_err(e)?;
    let eqs = dm.local_equation_numbers("u").map_err(e)?.to_vec();
    let (mut unshared_mismatches, mut unshared_rows, mut shared_rows) = (0, 0, 0);
    for (k, &eq) in eqs.iter().enumerate() {
        let eq = usize::try_from(eq).map_err(|x| x.to_string())?;
        let n = Idx::try_from(k / NBC).map_err(|x| x.to_string())?;
        let f = mesh.node_flag(n);
        if f.is_master() || f.is_slave() {
            shared_rows += 1;
        } else {
            unshared_rows += 1;
            if y[eq].to_bits() != up[eq].to_bits() {
                unshared_mismatches += 1;
            }
        }
    }
    // Gather both products in global node order.
    let (yn, upn) = (nodal(&dm, &y)?, nodal(&dm, &up)?);
    let gathered = if size > 1 {
        let ns = mesh
            .synchronizers()
            .node
            .as_ref()
            .ok_or("no node synchronizer")?;
        let ent = NodeEntities { mesh: &mesh };
        let gy = ns.gather(&ent, &yn, NBC).map_err(e)?;
        let gup = ns.gather(&ent, &upn, NBC).map_err(e)?;
        gy.zip(gup)
    } else {
        let mut gy = vec![0.0; yn.len()];
        let mut gup = vec![0.0; upn.len()];
        for n in 0..mesh.nb_nodes() {
            let g = usize::try_from(mesh.node_global_id(n as Idx)).map_err(|x| x.to_string())?;
            gy[NBC * g..NBC * g + NBC].copy_from_slice(&yn[NBC * n..NBC * n + NBC]);
            gup[NBC * g..NBC * g + NBC].copy_from_slice(&upn[NBC * n..NBC * n + NBC]);
        }
        Some((gy, gup))
    };
    // One rank: also the undistributed mesh (no synchronizer at all).
    let serial = if size == 1 {
        let plain = read_bar(&Communicator::serial(), true)?;
        if plain.synchronizers().node.is_some() {
            return Err("an undistributed mesh has a node synchronizer".into());
        }
        let (mut sdm, sx, sy0) = manager(&Communicator::serial(), &plain, data)?;
        let mut sy = sy0;
        sdm.mat_vec(&plain, "K", &sx, &mut sy, -1.0, 1.0)
            .map_err(e)?;
        Some(nodal(&sdm, &sy)?)
    } else {
        None
    };
    Ok(RankResult {
        unshared_mismatches,
        unshared_rows,
        shared_rows,
        gathered,
        serial,
    })
}

fn on(n: usize, data: Data) -> Vec<RankResult> {
    run_local(n, Duration::from_secs(120), move |c| scenario(c, data))
        .into_iter()
        .enumerate()
        .map(|(r, o)| o.unwrap_or_else(|e| panic!("{data:?}, {n} ranks, rank {r}: {e}")))
        .collect()
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Rows the reduction does not touch are upstream's in-place
/// product bit for bit, on one rank after `Mesh::distribute` (every row)
/// and on 2 and 4 ranks (interior and pure-ghost rows).
#[test]
fn ex1_01_distributed_product_keeps_upstream_bits_on_unshared_rows() {
    for n in [1, 2, 4] {
        let out = on(n, Data::Random);
        let unshared: usize = out.iter().map(|r| r.unshared_rows).sum();
        let shared: usize = out.iter().map(|r| r.shared_rows).sum();
        let bad: usize = out.iter().map(|r| r.unshared_mismatches).sum();
        println!(
            "{n} ranks: {unshared} unshared rows ({bad} differ from upstream), {shared} shared"
        );
        assert_eq!(bad, 0, "{n} ranks: unshared rows differ from upstream");
        assert!(unshared > 0);
        if n == 1 {
            assert_eq!(shared, 0, "one rank has no shared node");
            assert_eq!(unshared, 121 * NBC);
            let (gy, gup) = out[0].gathered.as_ref().unwrap();
            assert_eq!(bits(gy), bits(gup), "one rank: upstream bits");
            // the one-rank distributed mesh keeps the serial numbering
            let s = out[0].serial.as_ref().unwrap();
            assert_eq!(
                bits(gy),
                bits(s),
                "one rank: synchronizer present or not, same bits"
            );
        } else {
            assert!(shared > 0, "{n} ranks: the partition has shared nodes");
        }
    }
}

/// UD-003 stays corrected: with exact data the reduced product at 2 and 4
/// ranks equals the serial one in every entry; upstream's unreduced
/// product misses the other ranks' partial sums on shared rows.
#[test]
fn ex1_01_shared_rows_keep_the_ud003_correction() {
    let serial = on(1, Data::SmallIntegers);
    let (reference, _) = serial[0].gathered.clone().unwrap();
    assert_eq!(reference.len(), 121 * NBC);
    for n in [2, 4] {
        let out = on(n, Data::SmallIntegers);
        let (gy, gup) = out[0].gathered.clone().unwrap();
        assert!(out[1..].iter().all(|r| r.gathered.is_none()));
        assert_eq!(bits(&gy), bits(&reference), "{n} ranks: corrected product");
        let partial = gup.iter().zip(&reference).filter(|(a, b)| a != b).count();
        println!("{n} ranks: upstream's unreduced product differs in {partial} entries");
        assert!(partial > 0, "{n} ranks: shared rows are exercised");
    }
}
