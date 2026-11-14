// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); regression test for
// UD-890 on the set-up of performance-testing-cohesive @ 483bace8
// (cohesive_extrinsic.cc: the frozen h = 0.12 cube, material-cohesive.dat,
// the `FromHigherDim` shear increment on "top", "bottom", "side left",
// "side right") and Akantu's boundary_condition_tmpl.hh (Neumann `applyBC`
// integrates the group's local and ghost elements).

//! UD-890: upstream's distributed cohesive insertion
//! overwrites ghost boundary triangles of the loaded groups with an interior
//! facet's nodes, and its Neumann `applyBC` integrates ghost boundary
//! elements, so its distributed benchmark loads an interior node (3,607 N on
//! node 2261 at step 900 of the h = 0.12 run at 2 ranks). The port must keep
//! the serial problem:
//!
//! 1. after a distributed insertion that doubles nodes of the loaded faces
//!    on and next to the partition interface, every element of the four
//!    loaded groups — local **and ghost** — still lies on its face (all
//!    three nodes on the face's plane, exactly), the local elements of all
//!    ranks are together exactly the serial run's group (same node
//!    positions per triangle, same count), and every ghost element is one of
//!    them;
//! 2. the external force of the shear increment applied after that
//!    insertion equals the serial one at every owned node, the nodes being
//!    matched by position and side (the mean barycentre of the node's
//!    attached tetrahedra, which tells a doubled node from its original),
//!    within a summation-order rounding bound
//!    (`docs/NUMERICAL_VALIDATION.md`), and the totals per direction agree.
//!
//! Insertion is forced (facets flagged directly, not by the stress check):
//! every insertable facet whose barycentre lies within 0.1 of "side left"
//! (x = −1) or "side right" (x = 1) and has |z| ≤ 0.5. Partitions: the C++
//! harness's Scotch partitions of this mesh at 2 and 4 ranks (the 2-rank one
//! is the partition of the UD-890 evidence). In-process local MPI world (no
//! `mpirun`): fast tier.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use akantu_bench::Config;
use akantu_cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::tensor::MatrixX;
use akantu_core::{
    Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter,
};
use akantu_mesh::distribution::{PrescribedPartition, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FromHigherDim;

/// The loaded groups: name, axis of the face normal, plane coordinate.
const GROUPS: [(&str, usize, Real); 4] = [
    ("top", 2, 1.0),
    ("bottom", 2, -1.0),
    ("side left", 0, -1.0),
    ("side right", 0, 1.0),
];
/// Summation-order bound on a nodal force, relative to the largest nodal
/// force of the serial run (see the module documentation): rule C1F-1 at
/// 1e-14 (was 1e-12, a ×4,000 margin over
/// the observed 2.4e-16, now ×42; the D-082 second-check value of the other
/// distributed-vs-serial guards);
/// still 1.7e13 times below UD-890's signature (0.24).
const FORCE_RTOL: Real = 1e-14;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Positions of a triangle's nodes, sorted (bit patterns).
type Tri = [[u64; 3]; 3];
/// An owned node: position (bits) and side (mean barycentre of its attached
/// tetrahedra, rounded to 1e-9).
type NodeKey = ([u64; 3], [i64; 3]);

#[derive(Clone, Debug, Default)]
struct RankResult {
    /// cohesive elements local to this rank after the insertion
    inserted: usize,
    /// group -> (local triangles, ghost triangles)
    groups: BTreeMap<String, (Vec<Tri>, Vec<Tri>)>,
    /// ghost triangles of the loaded groups, and how many of them the
    /// forced insertion rewrote (nodes doubled; the
    /// test's precondition, that the ghost half is exercised)
    ghost_triangles: usize,
    ghost_rewritten: usize,
    /// triangles of a group with a node off the group's plane
    off_plane: Vec<String>,
    /// owned nodes with a non-zero external force
    forces: Vec<(NodeKey, [Real; 3])>,
}

fn read_parts(path: &Path) -> Vec<Idx> {
    let text = std::fs::read_to_string(path).expect("partition.csv");
    let mut tets: Vec<(usize, Idx)> = text
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            (f[0] == "_tetrahedron_4").then(|| (f[1].parse().unwrap(), f[2].parse().unwrap()))
        })
        .collect();
    tets.sort_unstable();
    assert!(tets.iter().enumerate().all(|(k, (e, _))| *e == k));
    tets.into_iter().map(|(_, p)| p).collect()
}

fn bits(x: &[Real]) -> [u64; 3] {
    [x[0].to_bits(), x[1].to_bits(), x[2].to_bits()]
}

fn case(comm: &Communicator, partition: Option<&Path>) -> Result<RankResult> {
    let dir = fixtures().join("perf_cohesive/coh_np2_global");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ghost_boundary_insertion");
    let cfg = Config::from_file(&dir.join("input.dat"), &dir, &out)?;
    let root = comm.rank() == 0;
    let mut mesh = Mesh::with_communicator(3, "mesh", comm.clone())?;
    if root {
        akantu_io::read_mesh(&mut mesh, &cfg.mesh_path(), MeshIoType::Gmsh)?;
    }
    if comm.size() > 1 {
        let mut p = PrescribedPartition::new(if root {
            read_parts(partition.expect("partition"))
        } else {
            Vec::new()
        });
        // no consumer of the distribution events in this test
        let _events = distribute_with(
            &mut mesh,
            if root {
                Some(&mut p as &mut dyn akantu_mesh::distribution::MeshPartitioner)
            } else {
                None
            },
            0,
        )?;
    }
    let shared = SharedMesh::new(mesh);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        shared.clone(),
        Some(3),
        akantu_cohesive::model::DEFAULT_ID,
        cfg.parser.clone(),
    )?;
    model.ext.compat = CohesiveUpstreamCompat::upstream();
    model.init_full(&ModelOptions::new(AnalysisMethod::Static), true)?;

    // the node ids of the loaded groups' ghost triangles before the
    // insertion
    let ghost_before = |shared: &SharedMesh| -> Result<Vec<(Idx, [Idx; 3])>> {
        let m = shared.read()?;
        let Ok(conn) = m.connectivity(ElementType::Triangle3, GhostType::Ghost) else {
            return Ok(Vec::new());
        };
        let conn = conn.as_slice();
        let mut out = Vec::new();
        for (name, _, _) in GROUPS {
            let g = m.groups().element_group(name)?;
            for &e in g
                .elements_of(ElementType::Triangle3, GhostType::Ghost)
                .iter()
            {
                let k = usize::try_from(e).unwrap();
                out.push((e, [conn[3 * k], conn[3 * k + 1], conn[3 * k + 2]]));
            }
        }
        Ok(out)
    };
    let before = ghost_before(&shared)?;

    // ---- forced insertion next to the side faces --------------------------
    {
        let m = shared.read()?;
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::invalid("no facet mesh"))?;
        let (check, insertion) = model.ext.inserter.flags_mut();
        let mut b = [0.0; 3];
        for ghost in [GhostType::NotGhost, GhostType::Ghost] {
            let Ok(chk) = check.get(ElementType::Triangle3, ghost) else {
                continue;
            };
            let chk = chk.as_slice().to_vec();
            let ins = insertion.get_mut(ElementType::Triangle3, ghost)?;
            let ins = ins.as_mut_slice();
            for (f, ok) in chk.iter().enumerate() {
                let e = Element::new(ElementType::Triangle3, Idx::try_from(f).unwrap(), ghost);
                fm.barycenter_into(e, &mut b)?;
                if *ok && b[0].abs() >= 0.9 && b[2].abs() <= 0.5 {
                    ins[f] = true;
                }
            }
        }
    }
    model.insert_elements()?;
    // cohesive elements of this rank (not ghost): each new element is local on exactly one rank
    let inserted = {
        let m = shared.read()?;
        m.element_types(TypeFilter {
            spatial_dimension: Some(3),
            ghost_type: GhostType::NotGhost,
            kind: Some(ElementKind::Cohesive),
        })
        .map(|t| m.nb_element(t, GhostType::NotGhost))
        .sum()
    };

    let after: BTreeMap<Idx, [Idx; 3]> = ghost_before(&shared)?.into_iter().collect();
    let ghost_rewritten = before
        .iter()
        .filter(|(e, nodes)| after.get(e) != Some(nodes))
        .count();

    // ---- 1. the loaded groups after the insertion -----------------------
    let mut res = RankResult {
        inserted,
        ghost_triangles: before.len(),
        ghost_rewritten,
        ..RankResult::default()
    };
    {
        let m = shared.read()?;
        let x = m.nodes();
        let x = x.as_slice();
        for (name, axis, plane) in GROUPS {
            let g = m.groups().element_group(name)?;
            let mut tris = (Vec::new(), Vec::new());
            for ghost in [GhostType::NotGhost, GhostType::Ghost] {
                let Ok(conn) = m.connectivity(ElementType::Triangle3, ghost) else {
                    continue;
                };
                let conn = conn.as_slice();
                for &e in g.elements_of(ElementType::Triangle3, ghost).iter() {
                    let e = usize::try_from(e).unwrap();
                    let nodes = &conn[3 * e..3 * e + 3];
                    let mut t: Tri = [[0; 3]; 3];
                    for (k, &n) in nodes.iter().enumerate() {
                        let n = usize::try_from(n).unwrap();
                        if x[3 * n + axis] != plane {
                            res.off_plane.push(format!(
                                "rank {} {name} {ghost:?} triangle {e}: node {n} at {:?}",
                                comm.rank(),
                                &x[3 * n..3 * n + 3]
                            ));
                        }
                        t[k] = bits(&x[3 * n..3 * n + 3]);
                    }
                    t.sort_unstable();
                    if ghost == GhostType::NotGhost {
                        tris.0.push(t);
                    } else {
                        tris.1.push(t);
                    }
                }
            }
            res.groups.insert(name.to_string(), tris);
        }
    }

    // ---- 2. the shear increment after the insertion -----------------------
    {
        let inc = cfg.inc_shear;
        // symmetric: the same in row- and column-major order
        let new_shear = FromHigherDim::new(MatrixX::from_col_major(
            3,
            3,
            vec![0., 0., inc, 0., 0., 0., inc, 0., 0.],
        )?);
        let smm = &mut model.smm;
        smm.external_force_mut()?.as_mut_slice().fill(0.0);
        for (name, _, _) in GROUPS {
            smm.apply_neumann_bc(&new_shear, name)?;
        }
    }
    let m = shared.read()?;
    let x = m.nodes();
    let x = x.as_slice();
    let nb = m.nb_nodes();
    let mut side = vec![[0.0_f64; 3]; nb];
    let mut count = vec![0_usize; nb];
    let mut b = [0.0; 3];
    for ghost in [GhostType::NotGhost, GhostType::Ghost] {
        let Ok(conn) = m.connectivity(ElementType::Tetrahedron4, ghost) else {
            continue;
        };
        let conn = conn.as_slice();
        for e in 0..conn.len() / 4 {
            m.barycenter_into(
                Element::new(ElementType::Tetrahedron4, Idx::try_from(e).unwrap(), ghost),
                &mut b,
            )?;
            for &n in &conn[4 * e..4 * e + 4] {
                let n = usize::try_from(n).unwrap();
                for d in 0..3 {
                    side[n][d] += b[d];
                }
                count[n] += 1;
            }
        }
    }
    let f = model.smm.external_force()?;
    let f = f.as_slice();
    for n in 0..nb {
        let fn_ = [f[3 * n], f[3 * n + 1], f[3 * n + 2]];
        if fn_ == [0.0; 3] || !m.is_local_or_master_node(Idx::try_from(n).unwrap())? {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)] // a rounded coordinate in [-1, 1] × 1e9
        let key_side = side[n].map(|s| (s / count[n] as Real * 1e9).round() as i64);
        res.forces
            .push(((bits(&x[3 * n..3 * n + 3]), key_side), fn_));
    }
    Ok(res)
}

fn on(np: usize, partition: Option<&Path>) -> Vec<RankResult> {
    run_local(np, Duration::from_secs(1800), |c| case(&c, partition))
        .into_iter()
        .enumerate()
        .map(|(r, o)| o.unwrap_or_else(|e| panic!("np {np} rank {r}: {e}")))
        .collect()
}

fn compare(np: usize, serial: &RankResult, ranks: &[RankResult]) {
    let inserted: usize = ranks.iter().map(|r| r.inserted).sum();
    eprintln!(
        "np {np}: {inserted} cohesive elements inserted (serial {})",
        serial.inserted
    );
    assert!(
        serial.inserted > 100,
        "the forced insertion must double nodes of the side faces"
    );
    assert_eq!(inserted, serial.inserted, "np {np}: inserted elements");
    // 1. groups
    assert!(serial.off_plane.is_empty(), "{:?}", serial.off_plane);
    let off: Vec<&String> = ranks.iter().flat_map(|r| &r.off_plane).collect();
    assert!(
        off.is_empty(),
        "np {np}: boundary elements off their face after the insertion: {off:?}"
    );
    let mut ghosts = 0;
    for (name, _, _) in GROUPS {
        let want: BTreeSet<Tri> = serial.groups[name].0.iter().copied().collect();
        assert_eq!(
            want.len(),
            serial.groups[name].0.len(),
            "{name}: serial duplicates"
        );
        let mut local: Vec<Tri> = ranks
            .iter()
            .flat_map(|r| r.groups[name].0.iter().copied())
            .collect();
        local.sort_unstable();
        let got: BTreeSet<Tri> = local.iter().copied().collect();
        assert_eq!(
            local.len(),
            want.len(),
            "np {np} {name}: local elements over the ranks"
        );
        assert_eq!(got, want, "np {np} {name}: local elements over the ranks");
        for r in ranks {
            for t in &r.groups[name].1 {
                ghosts += 1;
                assert!(
                    want.contains(t),
                    "np {np} {name}: a ghost element is not a triangle of the group: {t:?}"
                );
            }
        }
    }
    assert!(
        ghosts > 0,
        "np {np}: the case must hold ghost boundary elements"
    );
    // the precondition of the ghost half: the forced
    // insertion rewrote ghost triangles of the loaded groups
    let rewritten: Vec<(usize, usize)> = ranks
        .iter()
        .map(|r| (r.ghost_rewritten, r.ghost_triangles))
        .collect();
    eprintln!("np {np}: ghost group triangles rewritten / held per rank: {rewritten:?}");
    assert!(
        rewritten.iter().any(|(k, _)| *k > 0),
        "np {np}: the forced insertion rewrote no ghost triangle of the loaded groups: the \
         ghost half of the test exercises nothing ({rewritten:?})"
    );
    // 2. external force per owned node
    let fmax = serial
        .forces
        .iter()
        .flat_map(|(_, f)| f.iter())
        .fold(0.0_f64, |a, v| a.max(v.abs()));
    let want: BTreeMap<NodeKey, [Real; 3]> = serial.forces.iter().copied().collect();
    assert_eq!(
        want.len(),
        serial.forces.len(),
        "serial node keys must be unique"
    );
    let mut got: BTreeMap<NodeKey, [Real; 3]> = BTreeMap::new();
    for r in ranks {
        for (k, f) in &r.forces {
            assert!(
                got.insert(*k, *f).is_none(),
                "np {np}: node {k:?} owned twice"
            );
        }
    }
    let keys: BTreeSet<&NodeKey> = want.keys().chain(got.keys()).collect();
    let mut worst = 0.0_f64;
    let mut sum = [[0.0_f64; 3]; 2];
    for k in keys {
        let a = want.get(k).copied().unwrap_or([0.0; 3]);
        let b = got.get(k).copied().unwrap_or([0.0; 3]);
        for d in 0..3 {
            worst = worst.max((a[d] - b[d]).abs() / fmax);
            sum[0][d] += a[d];
            sum[1][d] += b[d];
        }
    }
    eprintln!(
        "np {np}: {} loaded owned nodes (serial {}), max |Δf| / max|f| = {worst:.3e}, total \
         force serial {:?} vs {:?} (max |f| {fmax:.6e} N)",
        got.len(),
        want.len(),
        sum[0],
        sum[1]
    );
    assert!(
        worst <= FORCE_RTOL,
        "np {np}: external force differs from the serial one (max |Δf| / max|f| = {worst:.3e})"
    );
    #[allow(clippy::cast_precision_loss)] // a node count
    let bound = FORCE_RTOL * fmax * want.len() as Real;
    for (d, (a, b)) in sum[0].iter().zip(&sum[1]).enumerate() {
        assert!(
            (a - b).abs() <= bound,
            "np {np}: total force {d}: serial {a} vs {b}"
        );
    }
}

/// UD-890 regression: ghost boundary facets keep their nodes after a
/// distributed insertion, and the Neumann load equals the serial one.
#[test]
fn ghost_boundary_facets_keep_their_nodes_and_the_load_is_serial() {
    let serial = on(1, None).pop().unwrap();
    let p2 = fixtures().join("perf_cohesive_x86_64/coh_np2_global/partition.csv");
    let p4 = fixtures().join("perf_cohesive_mumps_x86_64/partition-np4.csv");
    let np2 = on(2, Some(&p2));
    compare(2, &serial, &np2);
    compare(4, &serial, &on(4, Some(&p4)));

    // The checks catch UD-890's signature (upstream's 2-rank state at step
    // 900): (a) a ghost "side left" triangle carrying an interior facet's
    // nodes, (b) 3,607 N in z on an interior node.
    let interior: Tri = {
        let mut t = [
            [0.2447, -0.0496, 0.0743],
            [0.2602, -0.0313, 0.1310],
            [0.1916, 0.0121, 0.0868],
        ]
        .map(|x: [Real; 3]| bits(&x));
        t.sort_unstable();
        t
    };
    let mut a = np2.clone();
    let r = a
        .iter_mut()
        .find(|r| !r.groups["side left"].1.is_empty())
        .expect("a ghost side triangle");
    r.groups.get_mut("side left").unwrap().1[0] = interior;
    let mut b = np2.clone();
    b[0].forces
        .push(((interior[0], [0, 0, 0]), [0.0, 0.0, 3607.05]));
    for (what, case) in [("ghost triangle", a), ("interior load", b)] {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compare(2, &serial, &case);
        }));
        assert!(
            caught.is_err(),
            "the comparison must detect a corrupted {what}"
        );
    }
}
