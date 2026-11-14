// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// the Scotch node reordering, the load callbacks of the Scotch partitioner
// and their dblDump debug files (MUTIL-014) against
// fixtures/mesh2/scotch/* (oracle/probes/mesh2: MeshPartitionScotch::reorder
// and partitionate(nb_part, edge_load_func, vertex_load_func), seed 1234,
// SCOTCH_PTHREAD_NUMBER=1). Feature `scotch` (Docker image docker/scotch:
// Debian's libscotch 7.0.7, as the oracle image).
#![cfg(feature = "scotch")]

mod mesh2_common;

use akantu_core::enums::MeshIoType;
use akantu_core::{Element, Error, GhostType, Idx, Int};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::partition::MeshPartitioner;
use akantu_mesh::scotch::{ScotchPartitioner, scotch_version};
use akantu_mesh::scotch_reorder::{ReorderMode, reorder};
use akantu_mesh::{Mesh, MeshEvent};
use mesh2_common::{Case, cases, load_mesh, scratch};

const SEED: i64 = 1234;

fn check_scotch_params(c: &mut Case) {
    c.eq_param("scotch/version", &scotch_version());
    c.eq_param_u("scotch/seed", usize::try_from(SEED).unwrap());
    let threads = std::env::var("SCOTCH_PTHREAD_NUMBER").unwrap_or_else(|_| "unset".into());
    c.eq_param("scotch/pthread_number", &threads);
}

/// Upstream `reorder()` (connectivities and coordinates only, node groups
/// stale: UD-670) bitwise, its dblDump files byte for byte, and the
/// corrected default giving the same permutation with every node-indexed
/// state renumbered.
#[test]
fn scotch_reordering_matches_the_oracle() {
    let mut failures = Vec::new();
    for id in cases("scotch") {
        let mut c = Case::open("mesh2_scotch_reorder", &id);
        if !c.ops().contains("reorder") {
            continue;
        }
        let dump = c.ops().contains("dbldump");
        check_scotch_params(&mut c);
        let mut mesh = load_mesh(&c, "reorder");
        c.nodes("reorder/before", &mesh);
        c.connectivities("reorder/before", &mesh, false);
        c.groups("reorder/before", &mesh);
        let dir = scratch(&format!("reorder_{}", id.replace('/', "_")));
        let up = reorder(
            &mut mesh,
            SEED,
            ReorderMode::Upstream,
            dump.then_some(dir.as_path()),
        )
        .unwrap();
        assert!(up.events.is_empty(), "upstream sends no event");
        c.nodes("reorder/after", &mesh);
        c.connectivities("reorder/after", &mesh, false);
        c.groups("reorder/after", &mesh);
        c.eq_param_u("reorder/nb_global_nodes", mesh.nb_global_nodes());
        if dump {
            c.tree("reorder_dump/", &dir);
        }
        // corrected default: same permutation, everything renumbered
        let mut fixed = load_mesh(&c, "reorder");
        let before = load_mesh(&c, "reorder");
        let out = reorder(&mut fixed, SEED, ReorderMode::Corrected, None).unwrap();
        assert_eq!(out.permutation, up.permutation, "{id}");
        assert_eq!(fixed.nodes().as_slice(), mesh.nodes().as_slice(), "{id}");
        assert_eq!(
            fixed.connectivities().as_map().iter_all().count(),
            mesh.connectivities().as_map().iter_all().count()
        );
        for (ty, g, a) in mesh.connectivities().as_map().iter_all() {
            assert_eq!(
                fixed.connectivity(ty, g).unwrap().as_slice(),
                a.read().unwrap().as_slice()
            );
        }
        let perm = |n: Idx| out.permutation[usize::try_from(n).unwrap()];
        for (old, new) in before
            .groups()
            .node_groups()
            .zip(fixed.groups().node_groups())
        {
            let want: Vec<Idx> = old.nodes().iter().map(|&n| perm(n)).collect();
            assert_eq!(new.node_list(), want, "{id}: node group {}", old.name());
        }
        match out.events.events() {
            [
                MeshEvent::NodesRemoved {
                    nodes,
                    new_numbering,
                    ..
                },
            ] => {
                assert!(nodes.is_empty());
                assert_eq!(new_numbering, &out.permutation);
            }
            other => panic!("{id}: unexpected events {other:?}"),
        }
        fixed.check_consistency().unwrap();
        if let Err(e) = c.finish() {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Upstream `partitionate(nb_parts)` and `partitionate(nb_parts,
/// edge_load_func, vertex_load_func)` (the probe's loads), the latter with
/// its dblDump files (GraphIniFile.grf, GeomIniFile.xyz, GraphFile.grf,
/// MapFile.map).
#[test]
fn weighted_scotch_partitions_match_the_oracle() {
    let mut failures = Vec::new();
    for id in cases("scotch") {
        let mut c = Case::open("mesh2_scotch_weighted", &id);
        if !c.ops().contains("weighted") {
            continue;
        }
        let dump = c.ops().contains("dbldump");
        check_scotch_params(&mut c);
        let parts: usize = c.spec("parts").unwrap().parse().unwrap();
        c.eq_param_u("partition/nb_parts", parts);
        let mesh = load_mesh(&c, "weighted");
        let lin = |p: &akantu_mesh::partition::MeshPartition| -> Vec<i64> {
            p.linearized().iter().map(|&x| i64::from(x)).collect()
        };
        let unit = ScotchPartitioner::new(SEED)
            .partition(&mesh, parts)
            .unwrap();
        c.i64("partition/unit", &lin(&unit));
        let edge = |a: Element, b: Element| -> Int { 1 + (a.index() + b.index()) % 4 };
        let vertex = |a: Element| -> Int { 1 + a.index() % 3 };
        let dir = scratch(&format!("weighted_{}", id.replace('/', "_")));
        let mut w = ScotchPartitioner::new(SEED).with_loads(&edge, &vertex);
        if dump {
            w = w.with_debug_dump(&dir);
        }
        let weighted = w.partition(&mesh, parts).unwrap();
        c.i64("partition/weighted", &lin(&weighted));
        if dump {
            c.tree("weighted_dump/", &dir);
        }
        let _ = GhostType::NotGhost;
        if let Err(e) = c.finish() {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// UD-673 (category A): upstream ignores Scotch's return
/// codes, so `partitionate(0)` returns an all-zero partition on the oracle
/// (probe case `weights parts_zero`; its output is not committed) and a
/// failed ordering would be applied. The port refuses
/// 0 parts before calling Scotch and turns every Scotch failure into an
/// error: loads that `SCOTCH_graphCheck` rejects (a negative vertex load,
/// asymmetric edge loads; the oracle stops at a debug assertion) are
/// errors, not a partition. Valid corner cases still partition (1 part:
/// all zeros; more parts than elements).
#[test]
fn scotch_failures_are_errors_ud673() {
    let cube = || {
        let mut m = Mesh::new(3, "ud673").unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/meshes/gmsh-4.13.1/test/test_fe_engine/cube.msh");
        let opts = ReadOptions {
            fill_unnamed_physical_groups: false,
            write_diana_material_file: false,
        };
        read_mesh_with(&mut m, &path, MeshIoType::Auto, &opts).unwrap();
        m
    };
    let mesh = cube();
    let p = ScotchPartitioner::new(SEED);
    let unit_e = |_: Element, _: Element| -> Int { 1 };
    let unit_v = |_: Element| -> Int { 1 };
    let weighted =
        |edge: &dyn Fn(Element, Element) -> Int, vertex: &dyn Fn(Element) -> Int, parts: usize| {
            p.with_loads(edge, vertex).partition(&mesh, parts)
        };
    // 0 parts: refused through every entry point (upstream: all-zero
    // partition)
    let e = weighted(&unit_e, &unit_v, 0).unwrap_err();
    assert!(
        matches!(&e, Error::InvalidArgument(m) if m.contains("UD-673")),
        "{e}"
    );
    let mut trait_p = ScotchPartitioner::new(SEED);
    let e = MeshPartitioner::partition(&mut trait_p, &mesh, 0).unwrap_err();
    assert!(
        matches!(&e, Error::InvalidArgument(m) if m.contains("UD-673")),
        "{e}"
    );
    let dir = scratch("ud673_debug_dump");
    let e = p
        .with_loads(&unit_e, &unit_v)
        .with_debug_dump(&dir)
        .partition(&mesh, 0)
        .unwrap_err();
    assert!(
        matches!(&e, Error::InvalidArgument(m) if m.contains("UD-673")),
        "{e}"
    );
    assert!(
        !dir.join("GeomIniFile.xyz").exists(),
        "refused before any debug file"
    );
    // loads Scotch rejects: an error, never a partition
    let negative = |a: Element| -> Int { if a.index() == 0 { -1 } else { 1 } };
    assert!(weighted(&unit_e, &negative, 4).is_err());
    let asymmetric = |a: Element, _: Element| -> Int { 1 + Int::try_from(a.index() % 3).unwrap() };
    assert!(weighted(&asymmetric, &unit_v, 4).is_err());
    // valid corner cases (oracle: `parts_one` all zeros, `parts_many`)
    let one = weighted(&unit_e, &unit_v, 1).unwrap();
    assert!(one.linearized().iter().all(|&x| x == 0));
    let many = weighted(&unit_e, &unit_v, 2000).unwrap();
    assert_eq!(
        many.linearized(),
        vec![249, 499, 1249, 1499, 999, 749, 1749, 1999],
        "oracle `weights parts_many`"
    );
}
