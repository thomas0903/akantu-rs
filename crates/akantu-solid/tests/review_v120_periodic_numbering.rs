// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/dof_manager.cc
// (updateDOFsData: _giu_global_conn then _ask_nodes, lines 858-869); mesh
// and material of test/test_mesh_utils/test_mesh_periodic/.
//! The distributed periodic equation numbering
//! itself (DOF-003 / DOF-011), checked without the oracle (the
//! global numbers depend on the partition): on `cube.msh` periodic in 1 / 2
//! / 3 directions at 2 / 4 / 8 ranks (local world, rank 0 reads, RCB, the
//! distributed `makePeriodic`), every copy of a node, pure-ghost periodic
//! master copies included, carries its owner's global equation, a periodic
//! slave its master's, the owned equations over the ranks are exactly
//! `0..N`, and `N` is the serial system size. Without the periodic
//! numbering exchange the test fails ("node … flag 0x39: no global
//! equation").

use std::path::PathBuf;
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::Idx;
use akantu_core::enums::{AnalysisMethod, MeshIoType, SpatialDirection};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::periodic::make_periodic;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;

const MATERIAL: &str =
    "material elastic [\n name = steel\n rho = 7800\n E = 2.1e11\n nu = 0.3\n]\n";

/// `(system size, sorted (global node, component, global eq, owned) of every
/// local DOF)` of the model on `comm` (cube periodic in x, y: `dim_periodic`).
fn numbering(
    comm: Option<&Communicator>,
    dim_periodic: usize,
) -> (i64, Vec<(i64, usize, i64, bool)>) {
    let dim = 3;
    let f: PathBuf =
        fixtures_root().join("meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_periodic/cube.msh");
    let mut mesh = match comm {
        None => {
            let mut m = Mesh::new(dim, "mesh").unwrap();
            akantu_io::read_mesh(&mut m, &f, MeshIoType::Gmsh).unwrap();
            m
        }
        Some(c) => {
            let mut m = Mesh::with_communicator(dim, "mesh", c.clone()).unwrap();
            if c.rank() == 0 {
                akantu_io::read_mesh(&mut m, &f, MeshIoType::Gmsh).unwrap();
            }
            let mut p = RcbPartitioner;
            let _ = distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
            m
        }
    };
    for d in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ]
    .into_iter()
    .take(dim_periodic)
    {
        let _ = make_periodic(&mut mesh, d, None).unwrap();
    }
    let gid: Vec<i64> = (0..mesh.nb_nodes())
        .map(|n| mesh.node_global_id(Idx::try_from(n).unwrap()))
        .collect();
    let master: Vec<Option<i64>> = (0..mesh.nb_nodes())
        .map(|n| {
            mesh.periodic_master(Idx::try_from(n).unwrap())
                .map(|m| gid[usize::try_from(m).unwrap()])
        })
        .collect();
    let flags: Vec<u8> = (0..mesh.nb_nodes())
        .map(|n| mesh.node_flag(Idx::try_from(n).unwrap()).bits())
        .collect();
    let parser = Parser::parse_str(MATERIAL, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let dm = m.dof_manager();
    let layout = dm.layout();
    let fam = dm.family("displacement").unwrap();
    let mut out = Vec::new();
    for (k, (&node, &eq)) in fam
        .associated_nodes
        .iter()
        .zip(&fam.local_equation_numbers)
        .enumerate()
    {
        let n = usize::try_from(node).unwrap();
        let eq = usize::try_from(eq).unwrap();
        let g = layout.global(eq).unwrap_or(-1);
        // only an ordinary (non-periodic) pure ghost has no global equation
        let plain_pure_ghost = flags[n] & 0x0F == 0x09 && flags[n] & 0xF0 == 0;
        assert!(
            g >= 0 || plain_pure_ghost,
            "node {} flag {:#04x}: no global equation",
            gid[n],
            flags[n]
        );
        if g < 0 {
            continue;
        }
        // a periodic slave is recorded under its master's global node id
        let key = master[n].unwrap_or(gid[n]);
        out.push((key, k % dim, g, layout.is_owned(eq) && master[n].is_none()));
    }
    out.sort_unstable();
    (dm.system_size(), out)
}

#[test]
fn distributed_periodic_numbering_invariants_on_2_4_8_ranks() {
    for dim_periodic in [1, 2, 3] {
        let (serial_n, _) = numbering(None, dim_periodic);
        for n in [2, 4, 8] {
            let out = run_local(n, Duration::from_secs(300), move |c| {
                numbering(Some(&c), dim_periodic)
            });
            let mut all: Vec<(i64, usize, i64, bool)> = Vec::new();
            for (r, (size, v)) in out.iter().enumerate() {
                assert_eq!(*size, serial_n, "{n} ranks, rank {r}: system size");
                all.extend(v.iter().copied());
            }
            // every (master node, component) has one global equation on all ranks
            let mut by_key = std::collections::BTreeMap::new();
            for &(key, c, g, _) in &all {
                let e = by_key.entry((key, c)).or_insert(g);
                assert_eq!(
                    *e, g,
                    "{n} ranks, periodic {dim_periodic}: node {key} comp {c}: {e} vs {g}"
                );
            }
            let mut owned: Vec<i64> = all.iter().filter(|t| t.3).map(|t| t.2).collect();
            owned.sort_unstable();
            let want: Vec<i64> = (0..serial_n).collect();
            assert_eq!(
                owned, want,
                "{n} ranks, periodic {dim_periodic}: owned equations"
            );
            eprintln!(
                "numbering, periodic in {dim_periodic} directions, {n} ranks: system size {serial_n}, {} DOF copies checked",
                all.len()
            );
        }
    }
}
