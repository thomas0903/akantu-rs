// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/test_communicator.cc
// (`TEST(Communicator, Bcast)`, `TEST(Communicator, ReceiveAny)`, gtest
// suite `test_synchronizers`, PARALLEL).
//! Ports of the communicator cases of the synchronizer suite (TEST-166) on
//! the in-process local world at 1, 2 and 4 ranks (fast tier); the same
//! scenarios run under real MPI in `mpi_acceptance.rs`
//! (`upstream_communicator_{bcast,receive_any}`, `scripts/mpi/run-tests.sh`).
//!
//! * `Bcast`: `0xdeadbeef` broadcast from rank 0 is received on every rank.
//! * `ReceiveAny`: 100 rounds; rank 0 draws, for every other rank `p`, a
//!   number `s_p ∈ [1, 10]` and broadcasts them; rank 0 sends `s_p` to every
//!   `p` with `s_p > 5`, every rank `p ≤ s_p` sends `s_p + 100 p` to rank 0,
//!   all with synchronous (rendezvous) sends; `receiveAnyNumber` must deliver
//!   exactly these messages, with the right sources and contents. As in
//!   `mpi_acceptance.rs`, the draws come from a fixed-seed generator (D-035;
//!   upstream seeds `mt19937` from `std::random_device`, only the range
//!   matters) and every round uses its own tag (the Rust contract of
//!   `receive_any_number`, UD-076; upstream reuses one tag).

use std::time::Duration;

use akantu_comm::{Communicator, SendMode, SendRequest, Tag, run_local};

const TIMEOUT: Duration = Duration::from_secs(60);

/// Fixed-seed linear congruential generator (draws in `[lo, hi]`).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, lo: i32, hi: i32) -> i32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let span = u64::try_from(hi - lo + 1).unwrap_or(1);
        lo + i32::try_from((self.0 >> 33) % span).unwrap_or(0)
    }
}

fn bcast(c: Communicator) -> Result<(), String> {
    let mut r = [if c.rank() == 0 { 0xdead_beef_u32 } else { 0 }];
    c.broadcast(&mut r, 0).map_err(|e| e.to_string())?;
    if r[0] != 0xdead_beef {
        return Err(format!("rank {}: got {:#x}", c.rank(), r[0]));
    }
    Ok(())
}

/// `TEST(Communicator, ReceiveAny)` (per-round tags); returns the number
/// of messages received by this rank.
fn receive_any(c: Communicator) -> Result<usize, String> {
    let e = |e: akantu_core::Error| e.to_string();
    let (rank, size) = (c.rank(), c.size());
    let mut rng = Lcg(0x5eed);
    let mut received = 0usize;
    for n in 0..100 {
        let tag = Tag(2000 + n);
        let mut sends = vec![0i32; size.saturating_sub(1)];
        if rank == 0 {
            for s in &mut sends {
                *s = rng.next(1, 10);
            }
        }
        c.broadcast(&mut sends, 0).map_err(e)?;
        if rank == 0 {
            let mut reqs: Vec<SendRequest<i32>> = Vec::new();
            let mut nb_recvs = 0i32;
            for (i, &send) in sends.iter().enumerate() {
                let p = i32::try_from(i).unwrap_or(0) + 1;
                if send > 5 {
                    reqs.push(
                        c.isend_mode(vec![send], p, tag, SendMode::Rendezvous)
                            .map_err(e)?,
                    );
                }
                if p <= send {
                    nb_recvs += 1;
                }
            }
            let mut errors = Vec::new();
            c.receive_any_number(reqs, tag, |proc, msg| {
                let k = usize::try_from(proc - 1).unwrap_or(0);
                // EXPECT_EQ(msg[0], sends[proc - 1] + 100 * proc)
                if msg.first() != Some(&(sends[k] + 100 * proc)) {
                    errors.push(format!("round {n}: from {proc}: {msg:?}"));
                }
                // EXPECT_LE(proc, sends[proc - 1])
                if proc > sends[k] {
                    errors.push(format!("round {n}: unexpected sender {proc}"));
                }
                nb_recvs -= 1;
                received += 1;
                Ok(())
            })
            .map_err(e)?;
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
            if nb_recvs != 0 {
                return Err(format!("round {n}: {nb_recvs} messages missing"));
            }
        } else {
            let k = usize::try_from(rank - 1).unwrap_or(0);
            let mut reqs = Vec::new();
            if rank <= sends[k] {
                reqs.push(
                    c.isend_mode(vec![sends[k] + 100 * rank], 0, tag, SendMode::Rendezvous)
                        .map_err(e)?,
                );
            }
            let mut has_recv = false;
            let mut errors = Vec::new();
            c.receive_any_number(reqs, tag, |proc, msg| {
                if msg.first() != Some(&sends[k]) || proc != 0 {
                    errors.push(format!("round {n}: from {proc}: {msg:?}"));
                }
                has_recv = true;
                received += 1;
                Ok(())
            })
            .map_err(e)?;
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
            // EXPECT_EQ(has_recv, recv[rank - 1] > 5)
            if has_recv != (sends[k] > 5) {
                return Err(format!("rank {rank} round {n}: has_recv = {has_recv}"));
            }
        }
    }
    Ok(received)
}

/// `TEST(Communicator, Bcast)` at 1, 2 and 4 ranks.
#[test]
fn test_communicator_bcast() {
    for n in [1, 2, 4] {
        for (r, o) in run_local(n, TIMEOUT, bcast).into_iter().enumerate() {
            o.unwrap_or_else(|e| panic!("{n} ranks, rank {r}: {e}"));
        }
    }
}

/// `TEST(Communicator, ReceiveAny)` at 1, 2 and 4 ranks.
#[test]
fn test_communicator_receive_any() {
    for n in [1, 2, 4] {
        let out = run_local(n, TIMEOUT, receive_any);
        let mut total = 0;
        for (r, o) in out.into_iter().enumerate() {
            total += o.unwrap_or_else(|e| panic!("{n} ranks, rank {r}: {e}"));
        }
        if n > 1 {
            assert!(total > 0, "{n} ranks: no message exchanged");
        }
    }
}
