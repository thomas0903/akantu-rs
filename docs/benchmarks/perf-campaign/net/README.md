# The port across several hosts: diagnosis and fix

On one 128-core machine the port ran the full-size cohesive benchmark 2.3×
faster than the C++ reference (128 ranks); on four 32-core hosts over TCP the
reference was 1.5× faster than the port: the reference's loop fell from
1948 s to 1040 s, the port's rose from 835 s to 1538 s (64 ranks: port 846 s
on one machine, 1046 s on two 32-core hosts). This directory holds the
reproduction on a small four-host cluster, the measurements that locate the
cause, the fix (D-126: collective checks at commit points) with its results,
and the data.

## Machines and software

Four cloud virtual machines on one private network (MTU 1460), 8 cores each
(one thread per core), 30–32 GiB, Ubuntu 24.04, kernel 7.0:

| host | CPU |
|---|---|
| 1 (head, rank 0) | AMD EPYC 9B14 (Zen 4) |
| 2 | AMD EPYC 9B45 (Zen 5) |
| 3 | Intel Xeon Platinum 8581C |
| 4 | AMD EPYC 7B13 (Zen 3) |

The CPUs differ, so absolute times mean little; the tables compare two
programs, or two layouts, in the same session. The network is slow for a
cluster: MPI ping-pong half round trip 25 µs between hosts 1 and 2, ICMP round
trip 0.25 ms from host 1 to host 2 and 0.70 ms to host 4 (`microbench.txt`).

Open MPI 5.0.7 (`pml ob1`, `btl tcp,self,sm`), the multi-node launcher
`crates/akantu-bench/scripts/mn_cluster.py` (ssh backend, `bind = self`: one
core per rank), two sessions on the same four hosts: `code = rust` (image
`akantu-rs-bench:mn`, the port at 5455adfc2, driver SHA-256 `bb7119e5…`; the
same binary rebuilt from that commit has the same digest) and `code = cpp`
(image `akantu-rs-bench:mncpp`, the strict C++ harness, SHA-256 `5b85f310…`).
One run at a time across both sessions.

Case: `benchmarks/cohesive-perf/inputs/material-cohesive.dat` as the launcher
writes it (blocked_node = global, energies every step), MUMPS static solve
(`auto`) unless noted:

- 8 ranks: mesh h = 0.06 (`cube-h0.06.msh`), 2000 steps, the C++ Scotch
  partition (`partition-np8.csv`; the C++ runs re-create it, checked);
- 32 ranks: mesh h = 0.04, 1500 steps, the partition the C++ harness
  exported in its first 32-rank run (`partition_matches` true in the later
  ones), replayed by the port.

Layout `NxH` = N ranks on H hosts, N / H per host, rank 0 on host 1.

## 1. Reproduction (18 November 2026, 07:25–09:09 UTC)

Rank 0's phase times in seconds, median [min, max] of 3 repetitions,
interleaved (order rotated per repetition), no interposer:

| case | loop | check_cohesive_stress | solve_step | energies |
|---|---|---|---|---|
| C++ 8x1 | 176.4 [176.2, 176.7] | 118.8 [118.4, 119.1] | 49.6 [49.6, 49.7] | 8.0 [7.9, 8.0] |
| C++ 8x4 | 130.3 [128.3, 133.6] | 82.3 [81.8, 84.8] | 33.9 [33.7, 35.3] | 12.8 [12.7, 14.9] |
| port 8x1 | 68.8 [68.7, 69.1] | 40.3 [40.2, 40.6] | 25.9 [25.7, 25.9] | 2.7 [2.6, 2.7] |
| port 8x4 | 86.7 [85.9, 95.0] | 42.5 [42.1, 46.1] | 29.5 [29.4, 32.3] | 14.6 [14.4, 16.6] |
| C++ 32x4 | 138.5 [135.7, 141.0] | 89.9 [87.8, 90.6] | 39.0 [38.6, 39.4] | 10.0 [8.9, 11.0] |
| port 32x4 | 106.7 [102.9, 107.2] | 49.3 [47.7, 49.5] | 38.8 [37.3, 39.2] | 18.4 [17.9, 18.7] |

Paired ratios (same repetition):

| ratio | loop | check_cohesive_stress | solve_step | energies |
|---|---|---|---|---|
| port 8x4 / port 8x1 | 1.260 [1.243, 1.384] | 1.058 [1.037, 1.144] | 1.143 [1.136, 1.258] | 5.45 [5.36, 6.23] |
| C++ 8x4 / C++ 8x1 | 0.737 [0.728, 0.758] | 0.691 [0.689, 0.716] | 0.682 [0.678, 0.713] | 1.63 [1.57, 1.86] |
| port / C++, 8x1 | 0.390 [0.389, 0.392] | 0.340 [0.338, 0.341] | 0.521 [0.516, 0.522] | 0.336 [0.331, 0.339] |
| port / C++, 8x4 | 0.669 [0.665, 0.711] | 0.517 [0.514, 0.544] | 0.873 [0.836, 0.953] | 1.13 [1.11, 1.15] |
| port / C++, 32x4 | 0.759 [0.757, 0.774] | 0.547 [0.544, 0.548] | 0.984 [0.954, 1.016] | 1.87 [1.68, 2.03] |

The full-size pattern appears at 8 ranks: the reference gets faster when its
ranks move to four hosts (0.74×), the port slower (1.26×); the port's lead
shrinks from 2.56× to 1.49× at 8 ranks and is 1.32× at 32 ranks. Inside the
port's insertion check the compute parts get faster across hosts
(interpolation 25.1 → 14.2 s) while the insertion part grows from 2.8 s to
23.0 s; energies grow 5.5×.

The 32-rank series has two further port repetitions (`rep-r32x4-k1`, `-k2`,
loops 176.6 s and 230.5 s) that are excluded: a symbol-resolution job ran on
host 1 during them (rank 0's interpolation took 25.6 s and 38.4 s against
17.8–18.9 s in the clean runs). Two extra pairs (`k4`, `k5`) replace them;
`k1`/`k2` of both codes are left out of the medians and ratios
(`python3 -I summarize.py runs.csv rep rep-r32x4-k1,rep-r32x4-k2,rep-c32x4-k1,rep-c32x4-k2`).

## 2. Where the port's time goes across hosts

### Collectives per step

A PMPI interposer (`pmpistat.c`, preloaded into every rank with the
launcher's `MN_PRELOAD`) counts every MPI call, its time and the lifetime of
non-blocking requests (`pmpi-counters.txt`, `pmpi_summary.py`). Per step and
rank, 8 ranks on 4 hosts:

| | C++ | port |
|---|---|---|
| collectives | 8.4 (`MPI_Allreduce` 6.45, `MPI_Exscan` 1.29, `MPI_Bcast` 0.64) | 51.6 (`MPI_Iallgather` 51.56, `MPI_Iallgatherv` 0.49) |
| point-to-point messages sent | 43 (2.38 MiB) | 19.5 (1.22 MiB) |
| time in collectives | 8.8 ms inside the blocking calls | 28.9 ms request lifetime (2.06 ms on one host) |
| NIC packets sent, all hosts | 907 | 3075 |

At 32 ranks on 4 hosts the port sends 43,900 packets per step against 2,800
for the reference (`runs.csv`, `nic_tx_pkt_per_step`), with fewer bytes
(39 MB against 47 MB per step).

The port's backend implements every collective as a checked collective whose
header is one `MPI_Iallgather` (Open MPI's non-blocking collectives, `coll
libnbc`, linear algorithm by default: every rank sends to every other rank);
rank-ordered reductions of up to 64 bytes travel inside that header. The
interposer's call-stack sampling (`PMPI_BT=Iallgather`, `btcat.py`,
`collectives-by-phase.txt`) attributes the 51.6 collectives per step:

| phase | agreements | data collectives |
|---|---|---|
| solve_step | 17.3 | 0.0 |
| check_cohesive_stress | 15.5 | 4.7 (+1.0 the driver's count) |
| energies | 9.0 | 4.0 |
| total | 41.8 | 9.7 |

The agreements are the collective error protocol (`akantu_comm::phase`,
D-018): every `Phases` step (`agree`, `agreed`, `run`, `run_collective`, the
solver's model callbacks through `DofManager::step`, the energy functions'
scopes) is one all-reduce of `(status, phase id, digest)`. Upstream issues
about 8 collectives per step for the same work (the energies' four
`MPI_Allreduce`, the insertion's global counts and scan).

### Cost of one collective

`microbench.txt` (`mpi_bench agree|allreduce`, `bench_baseline
allreduce|iallgather`): one agreement costs 2.9 µs on one host, 472 µs with
8 ranks on 4 hosts and 760 µs with 32 ranks; a raw `MPI_Allreduce` costs
868 µs and 892 µs there. On this network a collective is bound by latency
(about one round trip to the farthest host), so its count decides: 51.6 ×
0.5 ms ≈ 26 ms per step for the port (measured request lifetime 28.9 ms),
8.4 × 0.9 ms ≈ 7.6 ms for the reference (measured 8.8 ms).

### Bound: the same runs without the agreements

A diagnostic build (branch `perf/net-diag`, not for merge) skips the
agreements' communication when `AKANTU_DIAG_SKIP_AGREEMENTS` is set (valid
only when every rank succeeds, as in these runs). Single runs, loop in
seconds:

| case | port | port, agreements skipped |
|---|---|---|
| 8x1 | 69.0 | 68.1 |
| 8x4 | 88.8 | 43.4 |
| 8x4, Eigen static solve | 90.3 | 45.0 |
| 32x4 | 105.6 | 57.0 |

Without them the port is faster across four hosts than on one (43.4 against
68.8 s at 8 ranks; its compute phases run faster with 2 ranks per host,
section 3) and 3.0× (8 ranks) and 2.4× (32 ranks) faster than the reference
across hosts. With the Eigen static solve (deterministic; MUMPS's parallel
factorisation varies between runs) `energies.csv`, `insertions.csv` and the
summary values are bitwise identical between the port on one host, on four
hosts, and on four hosts without agreements.

### What is not the cause

- The pacing of polled waits (`akantu-comm/src/pacing.rs`): with the sleep
  phase disabled (`AKANTU_DIAG_NO_SLEEP`) the 8x4 loop is 86.4 s against
  88.8 s (one run each, inside the spread of the base series).
- Point-to-point traffic: the port sends fewer messages and bytes than the
  reference (above).

### The full-size numbers

The linear all-gather sends P − P/H messages per rank over the network for
each collective, so the network messages per host and step are about
C × (P/H) × (P − P/H) with C = 51.6 collectives per step: 52,800 per host
for 64 ranks on two hosts, 158,500 for 128 ranks on four. The port's loss
against one machine per step was 40 ms (64 ranks, 2 hosts) and 141 ms
(128 ranks, 4 hosts): 0.76 µs and 0.89 µs per message and host. The ratio of
the losses (3.5) follows the ratio of the message counts (3.0), which is why
the port collapses between two and four hosts. The reference's tuned
blocking collectives use logarithmic algorithms (a few hundred network
messages per host and step at 128 ranks).

## 3. Why ranks on separate hosts compute faster

Same 4-rank job (h = 0.06, 2000 steps) on 4 cores of host 1, alone and with a
4-rank job of the other code on the other 4 cores (`i5-*`), 3 repetitions:

| | alone | with the other job | ratio of the medians |
|---|---|---|---|
| port | 69.0 [68.9, 69.1] | 108.6 [108.3, 109.7] | 1.57 |
| C++ | 246.8 [245.1, 248.6] | 280.7 [280.5, 281.0] | 1.14 |

The port's 4-rank job alone (69.0 s) is as fast as its 8-rank job on all 8
cores (68.8 s): this host's memory bandwidth (or shared cache) is saturated
at 4 ranks, and the port's per-rank compute speed depends on how many ranks
share a host. With 2 ranks per host the port's interpolation takes 14.2 s
instead of 25.1 s, the reference's insertion check 82.3 s instead of 118.8 s.

## 4. The fix: collective checks at commit points (D-126)

`docs/DECISIONS.md`, D-126, has the rules; in short, under the default
mode `commit-points` (`AKANTU_COLLECTIVE_CHECKS`; `every-phase` restores
D-018's agreement after every phase):

- the outcomes of purely local phases are deferred to the scope's next
  agreement (in scopes whose owner settles them: the DOF manager's
  operations, the mesh transaction, the standalone synchronizer exchanges,
  the cohesive insertion order);
- every checked collective's header carries the protocol status, so a
  reduction carries the deferred outcomes itself (no separate agreement)
  and a failure reported by any rank fails the collective on every rank;
- commit points stay immediate (the transaction's capability check and
  dispatch end, a DOF manager's outermost operation, the native
  trampolines, everything inside a native library region);
- every point-to-point exchange keeps its agreement before it (a status
  in the message was tried and hung: a rank that failed earlier waits in an
  agreement, never in the exchange);
- from 64 ranks the header is exchanged by gather and broadcast
  (2 (P − 1) messages instead of P (P − 1)).

Collectives per step and rank, 8 ranks, h = 0.06, 2,000 steps (977 with an
insertion), from the interposer's stacks (`collectives-by-phase.txt`):

| phase | port before | port after | C++ |
|---|---|---|---|
| solve_step | 17.3 | 3.2 | |
| check_cohesive_stress (+ driver count) | 21.2 | 12.2 | |
| energies | 13.0 | 4.0 | |
| total | 51.6 | 19.4 | 8.4 |

What is left after the fix, per step: the solve's operation close and the
agreements before its two exchanges (ghost stresses, cohesive tractions);
the four energy reductions (the reference's four `MPI_Allreduce`); the
facet-stress exchange's agreement and settle, the insertion decision's
reduction and the driver's global count; on insertion steps the agreements
before the insertion-order exchanges and of its uniqueness check, the
transaction's commit point and dispatch end, the consumers' exchange
agreements and the global-id, count and bounding-box reductions.

Header exchange at 128 ranks (one host, Open MPI's pml monitoring,
`microbench.txt`): one agreement sends 16,147 messages with the all-gather
header and 254 with gather and broadcast (libnbc's own `MPI_Iallreduce`
also sends 254; it cannot carry the per-rank contributions that the
rank-ordered reductions need, so the header stays an all-gather in
function). On this cluster's slow network the two-round gather and
broadcast costs more than the one-round all-gather up to 32 ranks
(1,014 against 726 µs per agreement at 32 ranks on four hosts), hence the
switch at 64 ranks.

## 5. Results after the fix (18 November 2026, 12:48–13:46 UTC)

Port before (the image's driver, 5455adfc2, SHA-256 `bb7119e5…`), port
after (7cafbd708, `9a37a052…`, default settings: commit points, header by
all-gather below 64 ranks) and the C++ reference, interleaved, order
rotated per repetition, 3 repetitions; loop and rank 0's phases in seconds,
median [min, max] (`python3 -I summarize.py runs.csv fin`):

| case | loop | check_cohesive_stress | solve_step | energies |
|---|---|---|---|---|
| before 8x1 | 69.2 [69.1, 69.5] | 40.6 [40.5, 40.9] | 26.0 [25.9, 26.0] | 2.6 [2.6, 2.6] |
| after 8x1 | 68.8 [68.7, 69.0] | 40.3 [40.2, 40.5] | 26.0 [26.0, 26.0] | 2.5 [2.5, 2.5] |
| C++ 8x1 | 176.5 [176.4, 176.5] | 118.6 [118.4, 118.7] | 49.8 [49.6, 49.8] | 8.0 [8.0, 8.5] |
| before 8x4 | 90.0 [87.6, 91.5] | 42.5 [42.5, 44.3] | 30.7 [30.0, 31.1] | 16.0 [15.0, 16.7] |
| after 8x4 | 51.2 [50.6, 51.9] | 31.0 [30.8, 31.6] | 14.7 [14.7, 15.4] | 5.1 [5.0, 5.5] |
| C++ 8x4 | 132.6 [129.2, 132.6] | 84.0 [82.4, 84.9] | 34.0 [33.7, 34.4] | 13.9 [12.5, 14.6] |
| before 32x4 | 104.1 [103.6, 104.6] | 48.2 [47.9, 48.7] | 38.0 [37.4, 38.2] | 18.0 [17.9, 18.0] |
| after 32x4 | 65.0 [64.4, 70.9] | 38.1 [37.9, 41.3] | 20.8 [20.3, 21.9] | 6.1 [6.0, 7.7] |
| C++ 32x4 | 135.9 [135.0, 136.2] | 87.9 [87.9, 87.9] | 39.0 [38.7, 39.2] | 8.8 [8.4, 9.3] |

Paired ratios (same repetition), loop:

| ratio | 8x1 | 8x4 | 32x4 |
|---|---|---|---|
| after / before | 0.994 [0.988, 0.998] | 0.563 [0.560, 0.593] | 0.622 [0.621, 0.681] |
| before / C++ | 0.392 [0.392, 0.394] | 0.679 [0.661, 0.708] | 0.765 [0.762, 0.775] |
| after / C++ | 0.390 [0.389, 0.391] | 0.392 [0.382, 0.396] | 0.481 [0.474, 0.521] |

| ratio | before | after | C++ |
|---|---|---|---|
| 8 ranks: four hosts / one host | 1.302 [1.260, 1.322] | 0.745 [0.734, 0.755] | 0.751 [0.732, 0.751] |

Across four hosts the port is now 2.55× faster than the reference at 8
ranks (as on one host: 2.56×) and 2.08× at 32 ranks; moving its 8 ranks
from one host to four makes it 25 % faster, like the reference (the
compute phases run faster with 2 ranks per host, section 3), where before
the change it lost 30 %. On one host nothing changed (0.994).

Messages and collectives per step (NIC packets sent by all hosts, from the
timing runs; collectives and the time in their requests per rank, from
the interposer runs `cal-r8x4`, `pm-r32x4`, `pm-a8x4`, `pm-a32x4`):

| | before 8x4 | after 8x4 | C++ 8x4 | before 32x4 | after 32x4 | C++ 32x4 |
|---|---|---|---|---|---|---|
| NIC packets per step | 3,083 | 1,460 | 903 | 44,284 | 16,730 | 2,741 |
| collectives per step and rank | 51.6 | 19.4 | 8.4 | 48.3 | 17.7 | 7.8 |
| ms per step in collective requests | 28.9 | 11.6 | – | 48.7 | 24.9 | – |

Results: with the Eigen static solve (deterministic) `energies.csv`,
`insertions.csv` and the 29 result values of `summary.json` are bitwise
identical between the port before on one host and the port after on one
host (`eig-final-8x1`) and on four hosts (`eig-v3-8x4`, an earlier build of
the same protocol). The suite's MPI cases (below) give identical digests.

Shared memory, one host (AMD EPYC 9B14), the MPI cases of `suite_solid`
(size m) before (5455adfc2) and after (7cafbd708), 5 paired repetitions,
`scripts/perf/suite/run_mpi.sh`, ranks bound to cores 0–7 (`suite-ab.txt`,
raw data `suite/`):

| case | ranks | phase | after / before | digests |
|---|---|---|---|---|
| par_cohesive_extrinsic | 2 | step | 0.970 [0.965, 0.976] | identical |
| par_cohesive_extrinsic | 4 | step | 0.973 [0.968, 0.986] | identical |
| par_cohesive_extrinsic | 8 | step | 0.977 [0.971, 0.994] | identical |
| par_explicit_tet4_ss | 2 | solve_step | 0.975 [0.953, 0.982] | identical |
| par_explicit_tet4_ss | 4 | solve_step | 0.992 [0.986, 1.005] | identical |
| par_explicit_tet4_ss | 8 | solve_step | 1.008 [1.003, 1.011] | identical |
| par_explicit_tet4_ss | 2 | energies | 1.015 [1.008, 1.023] | identical |

No timed phase has a median ratio above 1.015.

## 6. What is left

- The port makes 19.4 collectives per step and rank against the
  reference's 8.4 (section 4 lists them): mostly the agreements before
  point-to-point exchanges (an exchange cannot carry the agreement, see
  D-126) and the insertion transaction's checks. On this cluster each
  costs about 0.5 ms across hosts; the port's per-step time in collective
  requests across hosts is still 11.6 ms (8 ranks) and 24.9 ms (32 ranks).
- At 64 and 128 ranks the header switches to gather and broadcast, which
  this cluster (32 cores) cannot measure at that size; at 128 ranks it
  sends 254 messages per collective instead of 16,147.
- Item 5's conclusion (section 3) holds: the speed of the compute phases
  depends on the ranks per host (shared memory bandwidth); the reference
  gains across hosts for that reason, and so does the port now.

## Files

- `runs.csv`: every run of the sessions (`collect.py`); `env` names the
  interposer (`LD_PRELOAD`, `PMPI_BT`), the diagnostic switches and the
  Eigen solve; timing tables use the runs without the interposer. Prefixes:
  `cal-`, `d1-`, `d2-`, `rep-` diagnosis; `i5-` the contention experiment;
  `eig-` bitwise checks; `fin-` the series of section 5; `bt*-`, `pm-`
  interposer runs.
- `summarize.py`: the tables of sections 1, 3 and 5.
- `pmpistat.c`: the PMPI interposer (`mpicc -O2 -fPIC -shared`); its counters
  `pmpi-counters.txt` (`pmpi_summary.py`); call sites before and after the
  fix `collectives-by-phase.txt` (`btcat.py`).
- `microbench.txt`: the cost and message count of one collective, before
  and after.
- `suite-ab.txt`, `suite/`: the shared-memory A/B of section 5.

Commands (head host; sessions as in the launcher's header, `hosts` with the
four private addresses and `slots=8`): one run is

    python3 -I crates/akantu-bench/scripts/mn_cluster.py run SESSION OUT --name NAME \
        --np 8 --hosts 4 --mesh /mn/work/meshes/cube-h0.06.msh --steps 2000 \
        --partition file:/mn/work/partitions/h0.06-np8.csv

with `MN_DRIVER=/path/in/container` (another port build), `MN_PRELOAD`
(the interposer) and `MN_XENV="VAR=value ..."` (diagnostic switches) in the
environment when a run needs them; the four hosts' NIC counters
(`/sys/class/net/*/statistics`) are read before and after each run.
