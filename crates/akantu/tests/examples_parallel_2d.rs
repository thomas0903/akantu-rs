// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/parallel_2d (port of
// examples/c++/solid_mechanics_model/parallel/parallel_2d.cc, upstream
// ctest `parallel_2d-test`, registered PARALLEL: 2 ranks).

mod examples_common;
#[path = "../examples/parallel_2d/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod parallel_2d;

use std::collections::BTreeMap;
use std::time::Duration;

use akantu::comm::{Communicator, run_local};
use examples_common::{assert_close_rel, files_below, pvtu_pieces, scratch};
use parallel_2d::Partition;

const STEPS: usize = 40;

/// `(u, v)` of every owned node by global id, and the time step.
type Owned = (BTreeMap<i64, [f64; 4]>, f64);

fn run(comm: Communicator, out: &std::path::Path, partition: &Partition) -> Result<Owned, String> {
    let mut ex = parallel_2d::example().for_test(out);
    ex.steps = Some(STEPS);
    let r = parallel_2d::run_on(comm, &ex, partition).map_err(|e| e.to_string())?;
    let mesh = r
        .model
        .physics
        .base
        .mesh
        .read()
        .map_err(|e| e.to_string())?;
    let u_g = r.model.displacement().map_err(|e| e.to_string())?;
    let u = u_g.as_slice();
    let v_g = r.model.velocity().map_err(|e| e.to_string())?;
    let v = v_g.as_slice();
    let mut owned = BTreeMap::new();
    for n in 0..mesh.nb_nodes() {
        let id = i32::try_from(n).unwrap();
        if mesh
            .is_local_or_master_node(id)
            .map_err(|e| e.to_string())?
        {
            owned.insert(
                mesh.node_global_id(id),
                [u[2 * n], u[2 * n + 1], v[2 * n], v[2 * n + 1]],
            );
        }
    }
    Ok((owned, r.time_step))
}

/// The example on 2 in-process ranks (recursive bisection partition) gives
/// the serial result: same time step (an exact min reduction), per global
/// node the displacement and velocity after 40 steps within 1e-12 of the
/// largest value (summation order of the interface contributions; the
/// Rule of `explicit_distributed_vs_serial`, NUMERICAL_VALIDATION
/// "Mesh distribution and parallel SMM"); rank 0 writes a `.pvtu` with one
/// piece per rank.
#[test]
fn parallel_2d_two_ranks_match_serial() {
    let out1 = scratch("parallel_2d_np1");
    let (serial, dt1) = run(Communicator::serial(), &out1, &Partition::Default).unwrap();
    let out2 = scratch("parallel_2d_np2");
    let results = run_local(2, Duration::from_secs(300), |c| {
        run(c, &out2, &Partition::Default)
    });
    let mut merged = BTreeMap::new();
    for (rank, r) in results.into_iter().enumerate() {
        let (owned, dt) = r.unwrap_or_else(|e| panic!("rank {rank}: {e}"));
        assert_eq!(dt.to_bits(), dt1.to_bits(), "time step on rank {rank}");
        assert!(!owned.is_empty(), "rank {rank} owns nodes");
        merged.extend(owned);
    }
    assert_eq!(merged.len(), serial.len(), "every node owned once");
    for c in 0..4 {
        let s: Vec<f64> = serial.values().map(|v| v[c]).collect();
        let p: Vec<f64> = serial.keys().map(|id| merged[id][c]).collect();
        assert!(s.iter().any(|&v| v != 0.0), "component {c} moves");
        assert_close_rel(
            &format!("component {c} at 2 ranks vs serial"),
            &p,
            &s,
            1e-12,
            "interface summation order, 40 explicit steps (distributed-model rule, \
             NUMERICAL_VALIDATION)",
        );
    }

    // dumps at step 0 only (then every 200 steps)
    assert_eq!(
        pvtu_pieces(&out2.join("paraview/multi_0000.pvtu")),
        [
            "multi-VTU/multi_0000.proc0.vtu",
            "multi-VTU/multi_0000.proc1.vtu"
        ]
    );
    assert_eq!(
        pvtu_pieces(&out1.join("paraview/multi_0000.pvtu")),
        ["multi-VTU/multi_0000.vtu"]
    );
    assert_eq!(
        files_below(&out2.join("paraview")),
        [
            "multi-VTU/multi_0000.proc0.vtu",
            "multi-VTU/multi_0000.proc1.vtu",
            "multi.pvd",
            "multi_0000.pvtu"
        ]
    );
}

/// A recorded partition is replayed: the one written as text is read back
/// and distributes the mesh with exactly those parts (here: x < 0.5 on
/// rank 0).
#[test]
fn parallel_2d_prescribed_partition() {
    let ex = parallel_2d::example();
    let mesh = ex.read_mesh(2, "square_2d.msh").unwrap();
    let ty = akantu::core::ElementType::Triangle3;
    let bary = mesh
        .barycenters(ty, akantu::core::GhostType::NotGhost)
        .unwrap();
    let parts: Vec<String> = bary
        .as_slice()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| if b[0] < 0.5 { "0" } else { "1" }.to_owned())
        .collect();
    let dir = scratch("parallel_2d_prescribed");
    let file = dir.join("partition.txt");
    std::fs::write(&file, parts.join("\n")).unwrap();
    let parts = parallel_2d::read_partition(&file).unwrap();
    assert_eq!(parts.len(), bary.len());
    let partition = Partition::Prescribed(parts);
    let results = run_local(2, Duration::from_secs(300), |c| {
        let mut ex = parallel_2d::example().for_test(&dir);
        ex.steps = Some(1);
        ex.dump = false;
        let rank = c.rank();
        let r = parallel_2d::run_on(c, &ex, &partition).map_err(|e| e.to_string())?;
        let mesh = r
            .model
            .physics
            .base
            .mesh
            .read()
            .map_err(|e| e.to_string())?;
        let b = mesh
            .barycenters(ty, akantu::core::GhostType::NotGhost)
            .map_err(|e| e.to_string())?;
        for c in b.as_slice().as_chunks::<2>().0.iter() {
            if (c[0] < 0.5) != (rank == 0) {
                return Err(format!("rank {rank} holds the element at {c:?}"));
            }
        }
        Ok::<_, String>(b.len())
    });
    let counts: Vec<usize> = results.into_iter().map(Result::unwrap).collect();
    assert_eq!(counts.iter().sum::<usize>(), bary.len());
}

/// Run `steps` steps on 2 local ranks with the oracle's recorded Scotch
/// partition (`parallel_2d_partition`) and `--upstream-compat` (UD-110
/// stress layout) and compare: the `.pvtu` files byte for byte, every array
/// of the per-rank `.vtu` files but the padded stress bitwise (payload
/// digests), and the padded stress of rank 0's step-200 piece bitwise except
/// the out-of-plane shear entries, heap garbage upstream (UD-111: entries
/// 2, 11, 18, 19 of each 9 × 9 block).
///
/// This is the comparison with the **unmodified upstream binary**:
/// with `AKA_SEED=1` the unmodified binary (`parallel_2d`)
/// writes exactly the files of the variant that records its partition
/// (every one of its 154 output digests equals the variant's; checked
/// here), and the port's `.pvtu`/`.pvd` files are also compared with the
/// unmodified binary's digests directly.
fn compare_with_partition_oracle(steps: usize, complete: bool) {
    use examples_common::oracle::oracle;
    use examples_common::read_vtu;
    let o = oracle("parallel_2d_partition");
    let up = oracle("parallel_2d");
    let mut variant = o.digests.clone();
    assert!(variant.remove("partition.txt").is_some());
    assert_eq!(
        variant, up.digests,
        "the unmodified binary wrote the partition variant's files"
    );
    let parts = parallel_2d::read_partition(&o.kept("partition.txt")).unwrap();
    let partition = Partition::Prescribed(parts);
    let out = scratch(&format!("parallel_2d_partition_{steps}"));
    let results = run_local(2, Duration::from_secs(3600), |c| {
        let mut ex = parallel_2d::example().for_test(&out);
        ex.steps = Some(steps);
        ex.upstream_compat = true;
        parallel_2d::run_on(c, &ex, &partition)
            .map(|_| ex.transcript())
            .map_err(|e| e.to_string())
    });
    let transcripts: Vec<String> = results.into_iter().map(Result::unwrap).collect();
    // standard output: rank 0 prints the material
    // block (`std::cout << model.getMaterial(0)`), every rank the time
    // step, rank 0 the progress; the oracle's stdout.txt merges both ranks
    // (rank 1's time-step line after rank 0's)
    let mut want: Vec<String> = up.stdout().split('\n').map(str::to_owned).collect();
    let ts = want
        .iter()
        .position(|l| l.starts_with("Time Step"))
        .unwrap();
    assert_eq!(want[ts], want[ts + 1], "both ranks print the time step");
    let rank1 = want.remove(ts + 1);
    let lines = (!complete).then_some(ts + 1);
    examples_common::oracle::assert_stdout(
        "parallel_2d rank 0",
        &transcripts[0],
        &want.join("\n"),
        lines,
    );
    assert_eq!(transcripts[1], format!("{rank1}\n"), "parallel_2d rank 1");
    let mut arrays = 0;
    for f in files_below(&out) {
        if f.ends_with(".vtu") {
            arrays += o.assert_vtu_arrays_match(&out, &f, &|name| name == "stress");
        }
    }
    // partition.txt is the oracle's record, an input of the port
    let skip =
        |f: &str| f.ends_with(".vtu") || f == "partition.txt" || (!complete && f.ends_with(".pvd"));
    let pvtu = o.assert_files_match(&out, complete, &skip);
    let dumps = 1 + steps / 200;
    assert_eq!(pvtu, dumps + usize::from(complete));
    assert_eq!(up.assert_files_match(&out, complete, &skip), pvtu);
    assert_eq!(arrays, dumps * 2 * 8, "arrays compared");
    // rank 0's piece is kept (fixture size budget)
    {
        let rel = "paraview/multi-VTU/multi_0001.proc0.vtu".to_owned();
        let mine = read_vtu(&out.join(&rel));
        let theirs = read_vtu(&o.kept(&rel));
        let (m, t) = (mine.array("stress"), theirs.array("stress"));
        assert_eq!((m.nb_component, t.nb_component), (9, 9));
        let (m, t) = (m.values.reals(), t.values.reals());
        assert_eq!(m.len(), t.len(), "{rel}: 81 values per cell (UD-110)");
        let mut compared = 0;
        for (k, (a, b)) in m.iter().zip(t).enumerate() {
            if ![2, 11, 18, 19].contains(&(k % 81)) {
                assert_eq!(a.to_bits(), b.to_bits(), "{rel}: stress value {k}");
                compared += 1;
            }
        }
        assert_eq!(compared, m.len() / 81 * 77);
    }
}

/// Oracle variant `parallel_2d_partition`, 200 steps on 2 local ranks with
/// the recorded partition.
#[test]
fn parallel_2d_matches_the_partition_oracle() {
    compare_with_partition_oracle(200, false);
}

/// Medium tier: the whole run (10 000 steps, 51 dumps) with the recorded
/// partition.
#[cfg(not(debug_assertions))]
#[test]
fn parallel_2d_full_run_matches_the_partition_oracle() {
    compare_with_partition_oracle(10_000, true);
}

/// Medium tier, **partition-independence diagnostic**
/// (not the oracle match, which is the exact replayed-partition
/// comparison above, bitwise over all 10,000 steps): the unmodified
/// upstream binary at 2 ranks with its Scotch partition against the port on
/// 2 local ranks with a **different** partition (recursive bisection),
/// final dump, nodes of the written cells matched by coordinates.
/// **Velocity and acceleration** within 1e-11 of the largest value
/// (PROPOSED, NUMERICAL_VALIDATION): measured
/// 3.9e-14 and 3.6e-14 on x86_64, margins ≈ 250× and 280×, calibrated by
/// the partition-noise level of the port's own serial run against its
/// 2-rank bisection run (4.8e-14, 4.1e-14). The **displacement** difference
/// (6.3e-12) is printed, not asserted (only checked to be finite): it is
/// the partition-dependent rounding of the interface sums (`f_int`, lumped
/// mass) accumulated over 10,000 steps in the low-frequency modes — the
/// free rigid y-translation explains only 3 % of it (mean shift −2.05e-13;
/// 6.27e-12 remain after removing it), and the port's serial run against
/// its own 2-rank run already differs by 4.0e-12 — so no a-priori bound of
/// that size exists and a measured one would fail on any legitimate change
/// of the summation order.
#[cfg(not(debug_assertions))]
#[test]
fn parallel_2d_full_run_partition_independence() {
    use examples_common::{oracle::oracle, read_vtu};
    let o = oracle("parallel_2d");
    let out = scratch("parallel_2d_upstream_binary");
    let results = run_local(2, Duration::from_secs(3600), |c| {
        let ex = parallel_2d::example().for_test(&out);
        parallel_2d::run_on(c, &ex, &Partition::Default)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    for r in results {
        r.unwrap();
    }
    // node coordinates (bits) → values over both ranks' pieces, for the
    // nodes referenced by the pieces' cells (pure-ghost points carry
    // unsynchronised values, UD-475)
    let gather = |dir: &dyn Fn(usize) -> std::path::PathBuf, name: &str| {
        let mut m = BTreeMap::new();
        for rank in 0..2 {
            let v = read_vtu(&dir(rank));
            let x = v.array("positions").values.reals().to_vec();
            let referenced: std::collections::BTreeSet<i64> = v
                .array("connectivity")
                .values
                .ints()
                .iter()
                .copied()
                .collect();
            let a = v.array(name);
            let vals = a.values.reals();
            for (k, p) in x.as_chunks::<3>().0.iter().enumerate() {
                if !referenced.contains(&(k as i64)) {
                    continue;
                }
                let key = (p[0].to_bits(), p[1].to_bits());
                m.insert(
                    key,
                    vals[k * a.nb_component..(k + 1) * a.nb_component].to_vec(),
                );
            }
        }
        m
    };
    for name in ["displacement", "velocity", "acceleration"] {
        let mine = gather(
            &|r| out.join(format!("paraview/multi-VTU/multi_0050.proc{r}.vtu")),
            name,
        );
        let theirs = gather(
            &|r| o.kept(&format!("paraview/multi-VTU/multi_0050.proc{r}.vtu")),
            name,
        );
        assert_eq!(mine.len(), theirs.len(), "{name}: node sets");
        let t: Vec<f64> = theirs.values().flatten().copied().collect();
        let m: Vec<f64> = theirs.keys().flat_map(|k| mine[k].clone()).collect();
        let d = if name == "displacement" {
            // printed only; a NaN or an infinity still fails
            let d = examples_common::rel_diff(&m, &t);
            assert!(d.is_finite(), "{name}: non-finite values");
            d
        } else {
            assert_close_rel(
                &format!("{name}: RCB partition vs the upstream binary's Scotch partition"),
                &m,
                &t,
                1e-11,
                "PROPOSED partition-independence diagnostic: measured v 3.9e-14, a 3.6e-14 \
                 (serial vs bisection 4.8e-14 / 4.1e-14); displacement not asserted",
            )
        };
        println!("parallel_2d partition independence, {name}: {d:e}");
    }
}
