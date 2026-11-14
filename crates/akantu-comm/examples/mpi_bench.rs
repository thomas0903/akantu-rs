// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs benchmark (no upstream counterpart; Akantu by EPFL/LSMS): point
// to point latency of the MPI backend of akantu-comm against raw MPI
// (scripts/mpi/bench_baseline.c).

//! Point-to-point latency of `akantu-comm`'s MPI backend,
//! compared by `scripts/mpi/run-mpi-tier.sh` with the same
//! measurements in raw MPI (`scripts/mpi/bench_baseline.c`); results in
//! `docs/BENCHMARKS.md`. Build in release:
//!
//! ```text
//! cargo build --release -p akantu-comm --features mpi --example mpi_bench
//! mpirun -np 2 mpi_bench pingpong <bytes> <iterations>      # blocking send / matched receive
//! mpirun -np 2 mpi_bench pingpong_pre <bytes> <iterations>  # posted receive, then send
//! mpirun -np N mpi_bench halo <bytes> <iterations>          # all-to-all irecv/isend/wait
//! mpirun -np 2 mpi_bench calibrated pingpong <bytes> <iterations per block> <blocks>
//! mpirun -np N mpi_bench calibrated halo <bytes> <iterations per block> <blocks>
//! ```
//!
//! Prints one `PERF …` line on rank 0. Ping-pong: half round-trip time;
//! halo: time per exchange, the maximum over the ranks (`all_reduce` Max).
//! `calibrated` (the MPI tier's performance gate)
//! measures raw MPI and akantu-comm in the same job and prints `CAL` lines
//! (see `calibrated`). No progress deadline. Uses only long-standing
//! API, so the same file also
//! builds against older versions of the crate for comparisons.

fn main() {
    bench::main();
}

mod bench {
    use std::time::{Duration, Instant};

    use akantu_comm::lifecycle::ThreadLevel;
    use akantu_comm::{Communicator, MpiOptions, ReduceOp, SendMode, Tag};

    type R = Result<(), String>;

    fn e2s(e: akantu_core::Error) -> String {
        e.to_string()
    }

    fn pingpong(c: &Communicator, len: usize, iters: usize, preposted: bool) -> R {
        const T: Tag = Tag(80);
        let buf = vec![7u8; len];
        if c.rank() > 1 {
            return c.barrier().map_err(e2s);
        }
        let peer = 1 - c.rank();
        let warm = iters / 10 + 1;
        let mut t0 = Instant::now();
        for i in 0..iters + warm {
            if i == warm {
                t0 = Instant::now();
            }
            match (c.rank(), preposted) {
                (0, false) => {
                    c.send(&buf, peer, T).map_err(e2s)?;
                    c.receive_vec::<u8>(peer, T).map_err(e2s)?;
                }
                (0, true) => {
                    let rq = c.irecv::<u8>(len, peer, T).map_err(e2s)?;
                    c.send(&buf, peer, T).map_err(e2s)?;
                    rq.wait().map_err(e2s)?;
                }
                (_, false) => {
                    c.receive_vec::<u8>(peer, T).map_err(e2s)?;
                    c.send(&buf, peer, T).map_err(e2s)?;
                }
                (_, true) => {
                    let rq = c.irecv::<u8>(len, peer, T).map_err(e2s)?;
                    rq.wait().map_err(e2s)?;
                    c.send(&buf, peer, T).map_err(e2s)?;
                }
            }
        }
        let dt = t0.elapsed();
        if c.rank() == 0 {
            let half = dt.as_secs_f64() / iters as f64 / 2.0;
            println!(
                "PERF pingpong len={len} preposted={preposted}: half-RTT {:.2} us, {:.3} GB/s",
                half * 1e6,
                len as f64 / half / 1e9
            );
        }
        c.barrier().map_err(e2s)
    }

    fn halo(c: &Communicator, len: usize, iters: usize) -> R {
        const T: Tag = Tag(81);
        let me = c.rank();
        let p = i32::try_from(c.size()).map_err(|e| e.to_string())?;
        let buf = vec![3u8; len];
        let warm = iters / 10 + 1;
        c.barrier().map_err(e2s)?;
        let mut t0 = Instant::now();
        for i in 0..iters + warm {
            if i == warm {
                c.barrier().map_err(e2s)?;
                t0 = Instant::now();
            }
            let mut rqs = Vec::new();
            for q in (0..p).filter(|&q| q != me) {
                rqs.push(c.irecv::<u8>(len, q, T).map_err(e2s)?);
            }
            let mut sqs = Vec::new();
            for q in (0..p).filter(|&q| q != me) {
                sqs.push(c.isend(buf.clone(), q, T).map_err(e2s)?);
            }
            for r in rqs {
                r.wait().map_err(e2s)?;
            }
            for s in sqs {
                s.wait().map_err(e2s)?;
            }
        }
        let mut v = [t0.elapsed().as_secs_f64()];
        c.all_reduce(&mut v, ReduceOp::Max).map_err(e2s)?;
        if me == 0 {
            println!(
                "PERF halo np={p} len={len}: {:.2} us per exchange ({} peers)",
                v[0] / iters as f64 * 1e6,
                p - 1
            );
        }
        Ok(())
    }

    /// What one block of [`calibrated`] measures.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Prog {
        /// Plain MPI through the `mpi` crate's safe API on `MPI_COMM_WORLD`,
        /// pre-allocated buffers (the loops of `scripts/mpi/bench_baseline.c`).
        Raw,
        /// The same exchange through `akantu-comm` (the loops above).
        Akantu,
        /// Rank 0 alone: the same message sent to itself and received
        /// (`send` + `receive_vec` through the self channel): the buffer
        /// work of the public API (copy and packing on the sending side,
        /// unpacking into a fresh vector on the receiving side) without MPI.
        Calib,
    }

    fn median(v: &mut [f64]) -> f64 {
        v.sort_by(f64::total_cmp);
        let n = v.len();
        if n == 0 {
            f64::NAN
        } else if n % 2 == 1 {
            v[n / 2]
        } else {
            0.5 * (v[n / 2 - 1] + v[n / 2])
        }
    }

    /// Raw MPI and `akantu-comm` measured in **one job**
    /// — the same processes, placement and memory state — in interleaved
    /// blocks of `iters` exchanges (the order rotates per block), plus, for
    /// the ping-pong, the calibration block [`Prog::Calib`]. Per block on
    /// rank 0: `CAL block=… raw_us=… akantu_us=… [calib_us=…]` (ping-pong:
    /// half round trip; halo: time per exchange, maximum over the ranks;
    /// calibration: one `send` + `receive_vec` to self); then one
    /// `CAL summary` line with the medians over the blocks of the times, of
    /// the per-block ratio `akantu / raw` and of the per-block **backend
    /// ratio** `(akantu − calib) / raw` (halo: no calibration, the backend
    /// ratio is the ratio).
    fn calibrated(c: &Communicator, kind: &str, len: usize, iters: usize, blocks: usize) -> R {
        use mpi::traits::{Communicator as _, Destination, Source};
        const T: Tag = Tag(82);
        let world = mpi::topology::SimpleCommunicator::world();
        let me = c.rank();
        let p = i32::try_from(c.size()).map_err(|e| e.to_string())?;
        let ping = kind == "pingpong";
        if ping && p < 2 {
            return Err("the calibrated ping-pong needs two ranks".into());
        }
        let sbuf = vec![7u8; len];
        let peers: Vec<i32> = if ping {
            if me < 2 { vec![1 - me] } else { Vec::new() }
        } else {
            (0..p).filter(|&q| q != me).collect()
        };
        let mut rbufs: Vec<Vec<u8>> = peers.iter().map(|_| vec![0u8; len]).collect();
        // One exchange of `prog`; returns nothing, errors as strings.
        let mut step = |prog: Prog| -> R {
            match (prog, ping) {
                (Prog::Raw, true) => {
                    let Some(&q) = peers.first() else {
                        return Ok(());
                    };
                    let peer = world.process_at_rank(q);
                    let rb = &mut rbufs[0][..];
                    if me == 0 {
                        peer.send_with_tag(&sbuf[..], 80);
                        peer.receive_into_with_tag(rb, 80);
                    } else {
                        peer.receive_into_with_tag(rb, 80);
                        peer.send_with_tag(&sbuf[..], 80);
                    }
                    Ok(())
                }
                (Prog::Raw, false) => {
                    let world = &world;
                    let (sbuf, peers) = (&sbuf, &peers);
                    mpi::request::scope(|sc| {
                        let recvs: Vec<_> = rbufs
                            .iter_mut()
                            .zip(peers)
                            .map(|(b, &q)| {
                                world.process_at_rank(q).immediate_receive_into_with_tag(
                                    sc,
                                    &mut b[..],
                                    81,
                                )
                            })
                            .collect();
                        let sends: Vec<_> = peers
                            .iter()
                            .map(|&q| {
                                world
                                    .process_at_rank(q)
                                    .immediate_send_with_tag(sc, &sbuf[..], 81)
                            })
                            .collect();
                        for r in recvs {
                            r.wait();
                        }
                        for s in sends {
                            s.wait();
                        }
                    });
                    Ok(())
                }
                (Prog::Akantu, true) => {
                    let Some(&q) = peers.first() else {
                        return Ok(());
                    };
                    if me == 0 {
                        c.send(&sbuf, q, T).map_err(e2s)?;
                        c.receive_vec::<u8>(q, T).map_err(e2s)?;
                    } else {
                        c.receive_vec::<u8>(q, T).map_err(e2s)?;
                        c.send(&sbuf, q, T).map_err(e2s)?;
                    }
                    Ok(())
                }
                (Prog::Akantu, false) => {
                    let mut rqs = Vec::new();
                    for &q in &peers {
                        rqs.push(c.irecv::<u8>(len, q, T).map_err(e2s)?);
                    }
                    let mut sqs = Vec::new();
                    for &q in &peers {
                        sqs.push(c.isend(sbuf.clone(), q, T).map_err(e2s)?);
                    }
                    for r in rqs {
                        r.wait().map_err(e2s)?;
                    }
                    for s in sqs {
                        s.wait().map_err(e2s)?;
                    }
                    Ok(())
                }
                (Prog::Calib, _) => {
                    if me == 0 {
                        c.send(&sbuf, 0, T).map_err(e2s)?;
                        let v = c.receive_vec::<u8>(0, T).map_err(e2s)?;
                        if v.len() != len {
                            return Err(format!("calibration: {} bytes", v.len()));
                        }
                    }
                    Ok(())
                }
            }
        };
        let progs: &[Prog] = if ping {
            &[Prog::Raw, Prog::Akantu, Prog::Calib]
        } else {
            &[Prog::Raw, Prog::Akantu]
        };
        let warm = iters / 10 + 1;
        for &prog in progs {
            c.barrier().map_err(e2s)?;
            for _ in 0..warm {
                step(prog)?;
            }
        }
        // per program, per block: microseconds per exchange
        let mut times: Vec<Vec<f64>> = (0..3).map(|_| Vec::with_capacity(blocks)).collect();
        for b in 0..blocks {
            for k in 0..progs.len() {
                let prog = progs[(b + k) % progs.len()];
                c.barrier().map_err(e2s)?;
                let t0 = Instant::now();
                for _ in 0..iters {
                    step(prog)?;
                }
                let mut v = [t0.elapsed().as_secs_f64()];
                if !ping {
                    c.all_reduce(&mut v, ReduceOp::Max).map_err(e2s)?;
                }
                let per = match (prog, ping) {
                    (Prog::Calib, _) | (_, false) => v[0] / iters as f64,
                    (_, true) => v[0] / iters as f64 / 2.0,
                };
                times[prog as usize].push(per * 1e6);
            }
            if me == 0 {
                let calib = times[Prog::Calib as usize]
                    .get(b)
                    .map_or(String::new(), |x| format!(" calib_us={x:.2}"));
                println!(
                    "CAL block={b} raw_us={:.2} akantu_us={:.2}{calib}",
                    times[Prog::Raw as usize][b],
                    times[Prog::Akantu as usize][b]
                );
            }
        }
        if me == 0 {
            let (raw, ak) = (&times[Prog::Raw as usize], &times[Prog::Akantu as usize]);
            let cal = &times[Prog::Calib as usize];
            let mut ratio: Vec<f64> = ak.iter().zip(raw).map(|(a, r)| a / r).collect();
            let mut backend: Vec<f64> = if ping {
                ak.iter()
                    .zip(cal)
                    .zip(raw)
                    .map(|((a, k), r)| (a - k) / r)
                    .collect()
            } else {
                ratio.clone()
            };
            let (mut raw, mut ak, mut cal) = (raw.clone(), ak.clone(), cal.clone());
            println!(
                "CAL summary kind={kind} len={len} iters={iters} blocks={blocks} \
                 raw_med_us={:.2} akantu_med_us={:.2} calib_med_us={:.2} ratio_med={:.3} \
                 backend_ratio_med={:.3}",
                median(&mut raw),
                median(&mut ak),
                median(&mut cal),
                median(&mut ratio),
                median(&mut backend)
            );
        }
        c.barrier().map_err(e2s)
    }

    pub(super) fn main() {
        let args: Vec<String> = std::env::args().collect();
        let name = args.get(1).cloned().unwrap_or_default();
        let arg = |i: usize, d: usize| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
        let watchdog = Duration::from_secs(600);
        std::thread::spawn(move || {
            std::thread::sleep(watchdog);
            eprintln!("WATCHDOG: mpi_bench did not finish within {watchdog:?}");
            std::process::abort();
        });
        if !["pingpong", "pingpong_pre", "halo", "calibrated"].contains(&name.as_str()) {
            println!("usage: mpi_bench pingpong|pingpong_pre|halo <bytes> <iterations>");
            println!(
                "       mpi_bench calibrated pingpong|halo <bytes> <iterations per block> <blocks>"
            );
            return;
        }
        let c = Communicator::mpi_world(MpiOptions {
            thread_level: ThreadLevel::Funneled,
            timeout: None,
            send_mode: SendMode::Eager,
            ..MpiOptions::default()
        })
        .expect("MPI world communicator");
        c.barrier().expect("start barrier");
        let r = match name.as_str() {
            "pingpong" => pingpong(&c, arg(2, 8), arg(3, 20_000), false),
            "pingpong_pre" => pingpong(&c, arg(2, 8), arg(3, 20_000), true),
            "calibrated" => match args.get(2).map(String::as_str) {
                Some(kind @ ("pingpong" | "halo")) => {
                    calibrated(&c, kind, arg(3, 1 << 20), arg(4, 100), arg(5, 9))
                }
                _ => Err("calibrated: kind must be pingpong or halo".into()),
            },
            _ => halo(&c, arg(2, 8192), arg(3, 2000)),
        };
        if let Err(e) = &r {
            println!("BENCH FAIL {name} rank={}: {e}", c.rank());
        }
        let code = i32::from(r.is_err());
        drop(c);
        std::process::exit(code);
    }
}
