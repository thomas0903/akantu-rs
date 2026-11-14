// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs scaling probe (no upstream counterpart): times the
// MUMPS phases of src/solver/sparse_solver_mumps.cc on distributed
// 3-D Laplacian / elasticity systems for the AWS sizing note.

//! `mpirun -np P mumps_scaling <laplacian|elasticity> <m> [repeats]`:
//! assembles the system element-wise in contiguous element blocks per rank
//! (the shapes of `tests/mumps_common`), solves it with the MUMPS backend
//! and prints one TSV line on rank 0: kind, m, n, nnz (sum over ranks),
//! ranks, assembly / first solve (analysis + factorisation + solve) /
//! re-solve wall times, MUMPS memory estimates and actuals
//! (INFOG(16/17) MB predicted max/sum, INFOG(21/22) MB used max/sum), the
//! ordering (INFOG(7)), the factor entries (INFOG(29)), the flops
//! (RINFOG(3)), the peak resident set of rank 0 (VmHWM, MB) and the
//! relative residual.

#[path = "../tests/mumps_common/mod.rs"]
mod mumps_common;

use std::time::{Duration, Instant};

use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, MpiOptions, ReduceOp};
use akantu_linalg::centralize::gather_owned_to_root;
use akantu_linalg::direct::SparseDirectSolver;
use akantu_linalg::mumps::MumpsSolver;

fn vm_hwm_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kind = args.get(1).map_or("laplacian", String::as_str).to_owned();
    let m: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(20);
    let repeats: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(1);
    let comm = Communicator::mpi_world(MpiOptions {
        timeout: Some(Duration::from_secs(3600)),
        ..MpiOptions::default()
    })
    .expect("MPI world");
    let (r, p) = (comm.rank() as usize, comm.size());
    let t0 = Instant::now();
    let sys = match kind.as_str() {
        "elasticity" => mumps_common::elasticity(m, p),
        _ => mumps_common::laplacian(m, true, p),
    };
    let a = sys.matrix(r);
    let layout = mumps_common::layout(sys.n, p, r);
    let rhs = mumps_common::local_rhs(&layout, &sys.b);
    comm.barrier().expect("barrier");
    let t_asm = t0.elapsed().as_secs_f64();
    let mut nnz = [a.nnz() as u64];
    comm.all_reduce(&mut nnz, ReduceOp::Sum).expect("nnz");
    let mut s = MumpsSolver::default();
    let mut x = vec![0.0; layout.nb_local()];
    let mut times = Vec::new();
    let mut g = None;
    for k in 0..=repeats {
        if k == 1 {
            s.invalidate(); // timed repeats: full analysis + factorisation + solve
        }
        comm.barrier().expect("barrier");
        let t = Instant::now();
        {
            let mut ph = Phases::new(&comm, "scaling");
            s.solve_distributed(&mut ph, "s", &layout, Ok(&a), &rhs, &mut x)
                .expect("solve");
        }
        comm.barrier().expect("barrier");
        times.push(t.elapsed().as_secs_f64());
        if k == 0 {
            let mut ph = Phases::new(&comm, "gather");
            g = gather_owned_to_root(&mut ph, "g", &layout, &x).expect("gather");
        }
        if k > 0 {
            s.invalidate();
        }
    }
    // a solve-only timing with the current factors
    let mut ph = Phases::new(&comm, "scaling");
    s.solve_distributed(&mut ph, "s", &layout, Ok(&a), &rhs, &mut x)
        .expect("factorise");
    drop(ph);
    comm.barrier().expect("barrier");
    let t = Instant::now();
    let mut ph = Phases::new(&comm, "scaling");
    s.solve_distributed(&mut ph, "s", &layout, Ok(&a), &rhs, &mut x)
        .expect("solve");
    drop(ph);
    comm.barrier().expect("barrier");
    let t_resolve = t.elapsed().as_secs_f64();
    let mut hwm = [vm_hwm_mb()];
    comm.all_reduce(&mut hwm, ReduceOp::Max).expect("hwm");
    let rep = s.report().clone();
    if r == 0 {
        let res = g.map_or(f64::NAN, |x| sys.residual(&x));
        let best = times[1..].iter().copied().fold(times[0], f64::min);
        println!(
            "kind\tm\tn\tnnz\tranks\tassembly_s\tfirst_solve_s\tbest_full_solve_s\tresolve_s\t\
             infog16_mb\tinfog17_mb\tinfog21_mb\tinfog22_mb\tordering\tfactor_entries\t\
             flops\tmax_vmhwm_mb\tresidual"
        );
        println!(
            "{kind}\t{m}\t{}\t{}\t{p}\t{t_asm:.3}\t{:.3}\t{best:.3}\t{t_resolve:.4}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3e}\t{:.0}\t{res:.2e}",
            sys.n,
            nnz[0],
            times[0],
            rep.infog(16),
            rep.infog(17),
            rep.infog(21),
            rep.infog(22),
            rep.infog(7),
            rep.infog(29),
            rep.rinfog(3),
            hwm[0],
        );
    }
    drop(s);
    drop(comm);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
}
